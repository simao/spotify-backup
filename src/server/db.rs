use uuid::Uuid;
use rspotify::spotify::oauth2::TokenInfo;
use time::Timespec;
use std::path::PathBuf;

pub type Pool = r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>;
pub type Connection = r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>;

#[derive(Debug)]
pub struct BackupRequest {
    pub id: Uuid,
    pub token: TokenInfo,
    pub time_created: Timespec,
    pub file: Option<PathBuf>,
    pub last_error: Option<String>,
}

//    Pending +--> Executed --> timeout ----> CompletedOk
//            |
//            +--> Error    --> timeout ----> CompletedError
#[derive(Debug, serde::Serialize, PartialEq, Clone)]
pub enum RequestStatus {
    Pending,
    Executed,
    Error,
    CompletedOk,
    CompletedError,
}

impl RequestStatus {
    pub fn executed(&self) -> bool {
        match self {
            Self::Executed | Self::Error => true,
            _ => false
        }
    }

    pub fn completed(&self) -> bool {
        match self {
            Self::CompletedOk | Self::CompletedError => true,
            _ => false
        }
    }
}


pub mod backup_request {
    use super::Connection;
    use super::BackupRequest;
    use rspotify::spotify::oauth2::TokenInfo;
    use failure::Error;
    use uuid::Uuid;
    use rusqlite::types::{FromSql, FromSqlError, ValueRef, ToSqlOutput};
    use rusqlite::{OptionalExtension, ToSql};
    use std::path::PathBuf;
    use rusqlite::params;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use crate::server::db::RequestStatus;

    fn from_row(row: &rusqlite::Row) -> Result<BackupRequest, rusqlite::Error> {
        let id: SqlUuid = row.get(0)?;
        let token: SqlTokenInfo = row.get(1)?;
        let path: Option<SqlPathBuf> = row.get(3)?;

        Ok(BackupRequest {
            id: id.0,
            token: token.0,
            time_created: row.get(2)?,
            file: path.map(|p| p.0),
            last_error: row.get(4)?
        })
    }

    pub fn create(c: Connection, req: &BackupRequest) -> Result<Uuid, Error> {
        let oauth_json = SqlTokenInfo(req.token.clone());

        c.execute("INSERT INTO backup_requests (id, token, status, created_at) VALUES (?1, ?2, ?3, ?4)",
                 params![req.id.to_string(), &oauth_json, &RequestStatus::Pending, &time::get_time()])
            .map(|_| req.id)
            .map_err(Error::from)
    }

    pub fn set_error(c: Connection, id: Uuid, error: &str) -> Result<(), Error> {
        let count = c.execute("UPDATE backup_requests set last_error = ?, status = ? WHERE id = ?",
                              params![error, RequestStatus::Error, id.to_string()])
            .map_err(Error::from)?;

        if count > 0 {
            Ok(())
        } else {
            failure::bail!("could not set error on BackupRequest, updated 0 rows")
        }
    }

    pub fn set_completed(c: Connection, id: Uuid, with_error: bool) -> Result<(), Error> {
        let final_status = if with_error {
            RequestStatus::CompletedError
        } else {
            RequestStatus::CompletedOk
        };

        let count = c.execute("UPDATE backup_requests set status = ?, file = NULL, token = ? WHERE id = ?",
                              params![final_status, SqlTokenInfo(TokenInfo::default()), id.to_string()])
            .map_err(Error::from)?;

        if count > 0 {
            Ok(())
        } else {
            failure::bail!("could set BackupRequest to executed, updated 0 rows")
        }
    }

    pub fn set_executed(c: Connection, id: Uuid, file: &PathBuf) -> Result<(), Error> {
        let count = c.execute("UPDATE backup_requests set file = ?, status = ? WHERE id = ?",
                              params![file.to_str(), RequestStatus::Executed, id.to_string()])
            .map_err(Error::from)?;

        if count > 0 {
            Ok(())
        } else {
            failure::bail!("could complete BackupRequest, updated 0 rows")
        }
    }

    fn add_thread_id_function(c: &Connection, total_thread_count: u32) -> Result<(), Error> {
        c.create_scalar_function("sb_thread_id", 1, true, move |ctx| {
            assert_eq!(ctx.len(), 1, "called with unexpected number of arguments");

            let data = ctx.get::<String>(0)?;

            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            let hash = hasher.finish();

            Ok((hash % total_thread_count as u64) as i64)
        }).map_err(Error::from)
    }

    pub fn oldest_pending(c: Connection, thread_id: u32, total_thread_count: u32) -> Result<Option<BackupRequest>, Error> {
        add_thread_id_function(&c, total_thread_count)?;

        let mut stmt = c.prepare(
            "SELECT id, token, created_at, file, last_error \
            FROM backup_requests \
            where status = ? and sb_thread_id(id) = ? \
            order by created_at asc limit 1"
        )?;

        stmt.query_row(
            params![&RequestStatus::Pending, thread_id],
            from_row).optional().map_err(Error::from)
    }

    pub fn find_executed(c: Connection) -> Result<Vec<BackupRequest>, Error> {
        let since = time::now_utc() - time::Duration::hours(1);

        log::debug!("using since = {}", since.rfc3339());

        let mut stmt = c.prepare(
            "SELECT id, token, created_at, file, last_error FROM backup_requests \
            where (status = ? OR status = ?) \
            and created_at < ? \
            order by created_at desc LIMIT 5")?;

        let rows = stmt.query_map(params![RequestStatus::Error, RequestStatus::Executed, &since.to_timespec()],from_row)?;

        let mut result = vec![];

        for row in rows {
            result.push(row?);
        }

        Ok(result)
    }

    pub fn find_with_status(c: Connection, id: Uuid) -> Result<Option<(BackupRequest, RequestStatus)>, Error> {
        let mut stmt = c.prepare("SELECT id, token, created_at, file, last_error, status FROM backup_requests where id = ?")?;
        stmt.query_row(params![&id.to_string()], move |row| {
            let req = from_row(row)?;
            let status: RequestStatus = row.get(5)?;
            Ok((req, status))
        }).optional().map_err(Error::from)
    }

    struct SqlUuid(Uuid);

    impl FromSql for SqlUuid {
        fn column_result(value: ValueRef<'_>) -> Result<Self, FromSqlError> {
            value
                .as_str()
                .and_then(|s| s.parse::<Uuid>().map_err(|_| FromSqlError::InvalidType))
                .map(SqlUuid)
        }
    }

    struct SqlTokenInfo(TokenInfo);

    impl FromSql for SqlTokenInfo {
        fn column_result(value: ValueRef<'_>) -> Result<Self, FromSqlError> {
            value
                .as_str()
                .and_then(|s| serde_json::from_str::<TokenInfo>(s).map_err(|_| FromSqlError::InvalidType))
                .map(SqlTokenInfo)
        }
    }

    impl ToSql for SqlTokenInfo {
        fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
            let s = serde_json::to_string(&self.0).map_err(|err| rusqlite::Error::ToSqlConversionFailure(err.into()))?;
            Ok(ToSqlOutput::from(s))
        }
    }

    struct SqlPathBuf(PathBuf);

    impl FromSql for SqlPathBuf {
        fn column_result(value: ValueRef<'_>) -> Result<Self, FromSqlError> {
            value
                .as_str()
                .and_then(|s| s.parse::<PathBuf>().map_err(|_| FromSqlError::InvalidType))
                .map(SqlPathBuf)
        }
    }

    impl ToSql for RequestStatus {
        fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
            Ok(ToSqlOutput::from(format!("{:?}", self)))
        }
    }

    impl FromSql for RequestStatus {
        fn column_result(value: ValueRef<'_>) -> Result<Self, FromSqlError> {
            match value.as_str()? {
                "Pending" => Ok(RequestStatus::Pending),
                "Executed" => Ok(RequestStatus::Executed),
                "Error" => Ok(RequestStatus::Error),
                "CompletedOk" => Ok(RequestStatus::CompletedOk),
                "CompletedError" => Ok(RequestStatus::CompletedError),
                s => Err(FromSqlError::Other(failure::format_err!("Could not parse RequestStatus: {}", s).into()))
            }
        }
    }

    #[cfg(test)]
    pub mod tests {
        use super::*;
        use rusqlite::NO_PARAMS;
        use r2d2_sqlite::SqliteConnectionManager;
        use crate::server::db::Pool;

        pub fn new_db() -> Result<Pool, Error> {
            let manager = SqliteConnectionManager::memory();
            let pool = Pool::new(manager)?;
            let schema_str = std::fs::read_to_string("./schema.sql")?;
            pool.get().unwrap().execute(&schema_str, NO_PARAMS)?;
            Ok(pool)
        }

        fn new_req() -> BackupRequest {
            BackupRequest {
                id: Uuid::new_v4(),
                token: TokenInfo::default(),
                time_created: time::get_time(),
                file: None,
                last_error: None,
            }
        }

        fn create_past(pool: &Pool) -> Result<BackupRequest, Error> {
            let req = new_req();
            create(pool.get()?, &req)?;
            set_old(pool.get()?, req.id, None)?;
            Ok(req)
        }

        impl PartialEq for BackupRequest {
            fn eq(&self, other: &Self) -> bool {
                self.id == other.id &&
                    self.token.access_token == other.token.access_token && // good enough
                    self.time_created.sec == other.time_created.sec && // ignores nsec, rusqlite loses precision when saving to db
                    self.file == other.file &&
                    self.last_error == other.last_error
            }
        }

        pub fn set_old(c: Connection, id: Uuid, duration: Option<time::Duration>) -> Result<(), Error> {
            let at = time::get_time() - duration.unwrap_or(time::Duration::minutes(61));
            c.execute("UPDATE backup_requests set created_at = ? where id = ?", params![&at, id.to_string()])?;
            Ok(())
        }

        #[test]
        fn test_add_pending_backup() -> Result<(), Error> {
            let pool = new_db()?;
            let req = new_req();
            let res = create(pool.get()?, &req)?;

            assert_eq!(res, req.id);
            Ok(())
        }

        #[test]
        fn test_pending_returns_latest_backup() -> Result<(), Error> {
            let pool = new_db()?;
            let req = new_req();

            create(pool.get()?, &req)?;

            let pending = oldest_pending(pool.get()?, 0, 1)?;

            assert_eq!(pending.unwrap().id, req.id);
            Ok(())
        }

        #[test]
        fn test_gets_backups_for_specified_thread_only() -> Result<(), Error> {
            let pool = new_db()?;
            let mut req = new_req();

            let id_1 = Uuid::parse_str("35027614-7cc0-4d51-81f5-fbcc5eeae2e5")?; // has sb_thread_id = 1
            let id_2 = Uuid::parse_str("d2c70a33-c1a2-4ee1-9061-f8aa80262466")?; // has sb_thread_id = 2

            req.id = id_1;
            create(pool.get()?, &req)?;

            req.id = id_2;
            create(pool.get()?, &req)?;

            let pending = oldest_pending(pool.get()?, 0, 3)?;
            assert_eq!(pending, None);

            let pending = oldest_pending(pool.get()?, 1, 3)?;
            assert_eq!(pending.unwrap().id, id_1);

            let pending = oldest_pending(pool.get()?, 2, 3)?;
            assert_eq!(pending.unwrap().id, id_2);

            Ok(())
        }

        #[test]
        fn test_finds_error_requests() -> Result<(), Error> {
            let pool = new_db()?;
            let req = create_past(&pool)?;
            set_error(pool.get()?, req.id, "[test] something happened")?;

            let all_executed = find_executed(pool.get()?)?;
            let executed = all_executed.first().unwrap();

            assert_eq!(executed.id, req.id);
            Ok(())
        }

        #[test]
        fn test_finds_finished_requests() -> Result<(), Error> {
            let pool = new_db()?;
            let req = create_past(&pool)?;
            set_executed(pool.get()?, req.id, &PathBuf::from("/tmp/done.json"))?;

            let all_executed = find_executed(pool.get()?)?;
            let executed = all_executed.first().unwrap();

            assert_eq!(executed.id, req.id);
            Ok(())
        }

        #[test]
        fn test_does_not_finds_completed_requests() -> Result<(), Error> {
            let pool = new_db()?;
            let req = create_past(&pool)?;
            set_completed(pool.get()?, req.id, false)?;

            let all_executed = find_executed(pool.get()?)?;

            assert_eq!(all_executed.len(), 0);
            Ok(())
        }

        #[test]
        fn test_find() -> Result<(), Error> {
            let pool = new_db()?;
            let req = new_req();

            create(pool.get()?, &req)?;

            let found = find_with_status(pool.get()?, req.id)?.unwrap();

            assert_eq!(found.0, req);
            assert_eq!(found.1, RequestStatus::Pending);
            Ok(())
        }
    }
}

