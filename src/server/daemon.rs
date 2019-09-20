use r2d2_sqlite::SqliteConnectionManager;
use std::thread;
use failure::Error;

use super::db;
use crate::serialize::Backup;
use crate::backup_fn::*;
use crate::backup_fn::BackupFn;
use crate::config::Config;
use std::path::PathBuf;
use std::fs::File;
use std::io::prelude::*;
use crate::server::db::BackupRequest;
use uuid::Uuid;
use log;

fn write_backup(req: &BackupRequest, backup: &Backup, backups_dir: &PathBuf) -> Result<PathBuf, Error> {
    let path = PathBuf::from(format!("{}/{}.json", backups_dir.display(), req.id));
    let mut file = File::create(&path)?;
    let json = serde_json::to_string(backup)?;
    file.write_all(json.as_bytes())?;
    Ok(path)
}

fn process_backup_request(pool: db::Pool, backup_fn: impl BackupFn, req: &BackupRequest, backups_dir: &PathBuf) -> Result<Uuid, Error> {
    log::info!("Starting backup {}", req.id);

    if req.time_created < (time::now() - time::Duration::hours(1)).to_timespec() {
        log::warn!("Pending backup is too old, setting error");
        failure::bail!("pending backup is too old")
    } else {
        let backup = backup_fn.apply(req.token.clone())?;
        let file = write_backup(&req, &backup, backups_dir)?;
        db::backup_request::set_executed(pool.get()?, req.id, &file)?;
        log::info!("Completed backup {} saved to {:?}", req.id, file);
        Ok(req.id)
    }
}

fn process_oldest_backup_request(pool: db::Pool, backup_fn: impl BackupFn, backups_dir: &PathBuf, thread_id: u32, total_thread_count: u32) -> Result<Option<Uuid>,Error> {
    match db::backup_request::oldest_pending(pool.get()?, thread_id, total_thread_count) {
        Ok(Some(req)) => {
            log::info!("Found new backup request {}", req.id);

            if let Err(err) = process_backup_request(pool.clone(), backup_fn, &req, backups_dir) {
                if let Err(save_err) = db::backup_request::set_error(pool.get()?, req.id, &format!("{}", err)) {
                    log::error!("Could not set error on backup request: {}", save_err)
                }
                Err(err)
            } else {
                Ok(Some(req.id))
            }
        },
        Ok(None) =>
            Ok(None),
        Err(err) =>
            Err(err)
    }
}

fn delete_executed(pool: db::Pool) -> Result<Vec<Uuid>, Error> {
    let executed = db::backup_request::find_executed(pool.get()?)?;

    for req in executed.iter() {
        log::info!("Found executed backup {}", req.id);

        if let Some(file) = &req.file {
            std::fs::remove_file(file)?;
            log::info!("deleted expired backup file: {:?}", file);
        }

        db::backup_request::set_completed(pool.get()?, req.id, req.last_error.is_some())?;
    }

    Ok(executed.into_iter().map(|e| e.id).collect())
}

pub fn daemon(config: Config) -> () {
    let manager = SqliteConnectionManager::file(&config.db_path());
    let pool = db::Pool::new(manager).unwrap();

    let num_threads = config.worker_count;
    let mut workers = vec![];

    for tid in 0..num_threads {
        log::info!("Starting thread {}/{}", tid, num_threads);

        let pool = pool.clone();
        let backups_dir = config.downloads_path().clone();

        workers.push(thread::spawn(move || {
            loop {
                let pool = pool.clone();

                match process_oldest_backup_request(pool, DefaultBackup, &backups_dir, tid, num_threads) {
                    Ok(Some(id)) => {
                        log::info!("Finished backup processing for {}", id);
                    },
                    Ok(None) => {
                        log::debug!("No pending backups, checking later");
                        thread::sleep(std::time::Duration::from_secs(1))
                    },
                    Err(err) => {
                        log::error!("Could not process backup: {}, {:?}", err, err);
                        thread::sleep(std::time::Duration::from_secs(5));
                    }
                }
            }
        }));
    }

    let cleanup_thread = thread::spawn(move || {
        loop {
            match delete_executed(pool.clone()) {
                Ok(_) =>
                    log::debug!("Processed executed/error backups"),
                Err(err) => {
                    log::error!("Could not process executed/error backups : {}, {:?}", err, err)
                }
            }

            thread::sleep(std::time::Duration::from_secs(5));
        }
    });

    for w in workers {
        w.join().unwrap();
    }

    cleanup_thread.join().unwrap();
}



#[cfg(test)]
mod tests {
    use super::*;
    use rspotify::spotify::oauth2::TokenInfo;
    use db::backup_request::tests::new_db;
    use tempfile::tempdir;
    use lazy_static::lazy_static;
    use crate::server::db::backup_request::{find_executed, find_with_status};

    lazy_static! {
      static ref TEST_BACKUP_DIR: PathBuf = tempdir().unwrap().into_path();
    }

    struct EmptyBackup;

    impl BackupFn for EmptyBackup {
        fn apply(&self, _token_info: TokenInfo) -> Result<Backup, Error> {
            Ok(
                Backup {
                    albums: vec![],
                    playlists: vec![]
                }
            )
        }
    }

    struct ErrorBackup;

    impl BackupFn for ErrorBackup {
        fn apply(&self, _token_info: TokenInfo) -> Result<Backup, Error> {
            failure::bail!("[test] error backup")
        }
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

    #[test]
    fn test_processes_existing_backup_request() -> Result<(), Error> {
        let pool = new_db()?;
        let req = new_req();
        db::backup_request::create(pool.get()?, &req)?;

        let existing = process_oldest_backup_request(pool.clone(), EmptyBackup, &TEST_BACKUP_DIR, 0, 1)?;

        assert_eq!(existing.unwrap(), req.id);

        db::backup_request::tests::set_old(pool.get()?, req.id, None)?;

        let all_executed = db::backup_request::find_executed(pool.get()?)?;
        let executed = all_executed.first().unwrap();

        assert_eq!(executed.id, req.id);
        assert!(executed.file.is_some());
        assert!(executed.last_error.is_none());

        Ok(())
    }

    #[test]
    fn test_writes_backup_to_file() -> Result<(), Error> {
        let pool = new_db()?;
        let req = new_req();
        db::backup_request::create(pool.get()?, &req)?;

        let existing = process_backup_request(pool.clone(), EmptyBackup, &req, &TEST_BACKUP_DIR)?;

        assert_eq!(existing, req.id);

        let db_saved = db::backup_request::find_with_status(pool.get()?, req.id)?.unwrap();

        let f = std::fs::File::open(db_saved.0.file.unwrap())?;
        let s: serde_json::Value = serde_json::from_reader(f)?;

        let backup = s.as_object().unwrap();

        assert_eq!(backup.get("albums").unwrap().as_array().unwrap().len() , 0);
        assert_eq!(backup.get("playlists").unwrap().as_array().unwrap().len() , 0);

        Ok(())
    }

    #[test]
    fn test_sets_error_if_backup_fails() -> Result<(), Error> {
        let pool = new_db()?;
        let req = new_req();
        db::backup_request::create(pool.get()?, &req)?;

        let res = process_oldest_backup_request(pool.clone(), ErrorBackup, &TEST_BACKUP_DIR, 0, 1);
        db::backup_request::tests::set_old(pool.get()?, req.id, None)?;
        let err_msg = format!("{}", res.err().unwrap());

        assert_eq!(err_msg, "[test] error backup");

        let all_executed = db::backup_request::find_executed(pool.get()?)?;
        let executed = all_executed.first().unwrap();

        assert_eq!(executed.id, req.id);
        assert!(executed.file.is_none());
        assert_eq!(executed.last_error.as_ref().unwrap(), "[test] error backup");

        Ok(())
    }

    #[test]
    fn test_ok_if_no_backups() -> Result<(), Error> {
        let pool = new_db()?;
        let existing = process_oldest_backup_request(pool, DefaultBackup, &TEST_BACKUP_DIR, 0, 1)?;
        assert_eq!(existing, None);
        Ok(())
    }

    #[test]
    fn test_deletes_success_expired_backups() -> Result<(), Error> {
        let pool = new_db()?;
        let req = new_req();
        db::backup_request::create(pool.get()?, &req)?;

        process_backup_request(pool.clone(), EmptyBackup, &req, &TEST_BACKUP_DIR)?;
        let executed = db::backup_request::find_with_status(pool.get()?, req.id)?.unwrap().0;
        db::backup_request::tests::set_old(pool.get()?, req.id, None)?;

        let before = find_executed(pool.get()?)?;
        assert_eq!(before.first().unwrap().id, req.id);

        let deleted = delete_executed(pool.clone())?;
        assert_eq!(*deleted.first().unwrap(), req.id);
        assert!(!executed.file.unwrap().exists());

        let after = find_executed(pool.get()?)?;
        assert!(after.is_empty());

        Ok(())
    }

    #[test]
    fn test_deletes_error_expired_backups() -> Result<(), Error> {
        let pool = new_db()?;
        let req = new_req();
        db::backup_request::create(pool.get()?, &req)?;

        let _ = process_oldest_backup_request(pool.clone(), ErrorBackup, &TEST_BACKUP_DIR, 0, 1);
        db::backup_request::tests::set_old(pool.get()?, req.id, None)?;

        let before = find_executed(pool.get()?)?;
        assert_eq!(before.first().unwrap().id, req.id);

        let deleted = delete_executed(pool.clone())?;
        assert_eq!(*deleted.first().unwrap(), req.id);

        let after = find_executed(pool.get()?)?;
        assert!(after.is_empty());

        Ok(())
    }

    #[test]
    fn test_does_not_run_very_old_backups() -> Result<(), Error> {
        let pool = new_db()?;
        let req = new_req();
        db::backup_request::create(pool.get()?, &req)?;

        db::backup_request::tests::set_old(pool.get()?, req.id, Some(time::Duration::hours(2)))?;

        let _ = process_oldest_backup_request(pool.clone(), EmptyBackup, &TEST_BACKUP_DIR, 0, 1);

        let (after_req, after_status) = find_with_status(pool.get()?, req.id)?.unwrap();
        assert_eq!(after_status, db::RequestStatus::Error);
        assert_eq!(after_req.last_error.unwrap(), "pending backup is too old");

        let after = find_executed(pool.get()?)?;
        assert_eq!(after.first().unwrap().id, req.id);

        Ok(())
    }
}
