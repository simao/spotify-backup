use actix_web::{App, http, HttpResponse, HttpServer, middleware, web};

use r2d2_sqlite::SqliteConnectionManager;
use rspotify::spotify::oauth2::{SpotifyOAuth, TokenInfo};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::db::{BackupRequest, Pool, Connection};
use crate::spotify::*;
use crate::config::Config;
use tera::Tera;

mod db;
pub mod daemon;

mod app {
    use super::*;
    use crate::server::db::RequestStatus;
    use tera::Context;

    #[derive(Clone)]
    pub struct DefaultRenderer {
        pub base_path: String,
        tera: Tera
    }

    impl DefaultRenderer {
        pub fn new(base_path: &str, tera: Tera) -> DefaultRenderer {
            DefaultRenderer { base_path: base_path.into(), tera }
        }

        pub fn render(&self, tmpl: &str, ctx: &mut Context) -> Result<String, failure::Error> {
            ctx.insert("base_path", &self.base_path);
            self.tera.render(tmpl, &ctx).map_err(failure::Error::from)
        }
    }

    #[derive(Deserialize)]
    pub struct SpotifyApiCallbackParams {
        code: String
    }

    #[derive(Serialize)]
    pub struct BackupResponse {
        pub backup_id: Uuid,
        pub missing: bool,
        pub executed: bool,
        pub completed: bool,
        pub error: Option<String>,
    }

    pub async fn index(renderer: web::Data<DefaultRenderer>) -> Result<HttpResponse, actix_web::error::Error> {
        let body = renderer.render("index.html", &mut Context::new())?;
        Ok(HttpResponse::Ok().body(body))
    }

    pub async fn find_backup(c: Connection, uuid: Uuid) -> Option<(BackupRequest, RequestStatus)> {
        web::block(move || { db::backup_request::find_with_status(c, uuid) }).await.unwrap()
    }


    pub async fn backup_get(path: web::Path<(Uuid, )>, pool: web::Data<Pool>, renderer: web::Data<DefaultRenderer>) -> Result<HttpResponse, failure::Error> {
        let uuid = path.0;

        if let Some((backup, status)) = find_backup(pool.get()?, uuid).await {
            let resp = BackupResponse {
                backup_id: uuid,
                missing: false,
                executed:  status.executed(),
                completed: status.completed(),
                error: backup.last_error,
            };

            let body = renderer.render("backup.html", &mut Context::from_serialize(resp)?)?;
            Ok(HttpResponse::Ok().body(body))
        } else {
            let resp = BackupResponse {
                backup_id: uuid,
                missing: true,
                executed:  false,
                completed: false,
                error: "backup does not exist".to_string().into(),
            };

            let body = renderer.render("backup.html", &mut Context::from_serialize(resp)?)?;
            Ok(HttpResponse::NotFound().body(body))
        }
    }

    async fn save_backup_request(pool: &Pool, oauth_code: &TokenInfo) -> Uuid {
        let id = Uuid::new_v4();
        let p = pool.clone();
        let token = oauth_code.clone();

        let req = BackupRequest {
            id,
            token,
            time_created: time::now_utc().to_timespec(),
            file: None,
            last_error: None,
        };

        web::block(move || { db::backup_request::create(p.get()?, &req) }).await.unwrap()
    }

    async fn get_access_token(spotify_oauth: &SpotifyOAuth, code: &str) -> TokenInfo {
        let code_owned = code.to_string();
        let spotify_owned = spotify_oauth.clone();

        web::block(move || {
            spotify_owned.get_access_token(&code_owned).ok_or("Received token was not valid")
        }).await.unwrap()
    }

    pub async fn callback(renderer: web::Data<DefaultRenderer>, info: web::Query<SpotifyApiCallbackParams>, db: web::Data<Pool>, spotify_oauth: web::Data<SpotifyOAuth>) -> HttpResponse {
        let token = get_access_token(&spotify_oauth, &info.code).await;
        let uuid = save_backup_request(&db, &token).await;
        redirect(format!("{}/backups/{}", renderer.base_path, uuid))
    }

    pub fn redirect(url: String) -> HttpResponse {
        HttpResponse::Found().set_header(http::header::LOCATION, url).finish()
    }
}

mod api {
    use super::*;

    pub async fn backup_get(path: web::Path<(Uuid, )>, pool: web::Data<Pool>) -> HttpResponse {
        let uuid = path.0;

        if let Some((req, status)) = app::find_backup(pool.get().unwrap(), uuid).await {
            let response = app::BackupResponse {
                backup_id: req.id,
                missing: false,
                executed: status.executed(),
                completed: status.completed(),
                error: req.last_error
            };

            HttpResponse::Ok().json(response)
        } else {
            HttpResponse::NotFound().set_header("Content-Type", "json").finish()
        }
    }

    pub async fn backup_start(spotify_oauth: web::Data<SpotifyOAuth>) -> Result<HttpResponse, actix_web::error::Error> {
        let uri = build_user_redirect_uri(&spotify_oauth)?;
        Ok(app::redirect(uri))
    }
}

#[actix_rt::main]
pub async fn server(config: Config) -> std::io::Result<()> {
    let tera = Tera::new("templates/**/*").unwrap();

    let base_uri = config.base_uri();
    let base_path = base_uri.path().trim_end_matches("/");

    let spotify_oauth = build_spotify_oauth(&config.base_uri().to_string(), config.token_cache_path());

    let downloads_path = config.downloads_path();

    // Start N db executor actors (N = number of cores avail)
    let manager = SqliteConnectionManager::file(config.db_path());
    let pool = Pool::new(manager).unwrap();

    let renderer = app::DefaultRenderer::new(base_path, tera);

    HttpServer::new(move || {
        App::new()
            .wrap(middleware::Logger::default())
            .data(pool.clone())
            .data(spotify_oauth.clone())
            .data(renderer.clone())
            .route("/", web::get().to(app::index))
            .route("/callback", web::get().to(app::callback))
            .route("/backups/{id}", web::get().to(app::backup_get))
            .service(
                web::scope("/api")
                    .route("/backups/{id}", web::get().to(api::backup_get))
                    .route("/backups", web::post().to(api::backup_start))
            )
            .service(actix_files::Files::new("/static", "static/"))
            .service(actix_files::Files::new("/downloads", downloads_path.clone()))
    })
        .bind("0.0.0.0:8000")
        .unwrap()
        .run()
        .await
}
