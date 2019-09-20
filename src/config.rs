use std::path::PathBuf;
use actix_web::http::Uri;
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use std::env;
use std::fmt::Display;
use failure::_core::fmt::{Formatter, Error};
use std::io::Read;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    data_dir: PathBuf,
    pub worker_count: u32,
    base_uri: ConfigUri,
}

#[derive(Debug)]
struct ConfigUri(Uri);


impl Default for Config {
    fn default() -> Self {
        let data_dir = PathBuf::from(env::var("DATA_DIR").ok().unwrap_or("./data".to_owned()));
        let uri: Uri = env::var("BASE_URL").ok().unwrap_or("http://localhost:8000/".into()).parse::<Uri>().expect("Could not parse BASE_URL");
        let worker_count =  env::var("WORKER_COUNT").unwrap_or("1".into()).parse::<u32>().expect("Could not parse WORKER_COUNT");

        Config {
            data_dir,
            worker_count,
            base_uri: ConfigUri(uri)
        }
    }
}

impl Serialize for ConfigUri {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error> where
        S: Serializer {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for ConfigUri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where
        D: Deserializer<'de> {
        let s: String = serde::Deserialize::deserialize(deserializer)?;
        s.parse::<Uri>()
            .map(|u| ConfigUri(u))
            .map_err(|err| serde::de::Error::custom(format!("Invalid value for base_uri : {}", err)))
    }
}

impl Config {
    pub fn load() -> Result<Config, failure::Error> {
        let config_path = PathBuf::from("./spotify-backup.toml");

        match std::fs::File::open(config_path) {
            Ok(mut cfg) => {
                let mut c = String::new();
                cfg.read_to_string(&mut c)?;
                toml::from_str(&c).map_err(failure::Error::from)
            },
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Config::default())
            },
            Err(err) => Err(err.into())
        }
    }

    pub fn token_cache_path(&self) -> PathBuf { self.data_dir.join(".spotify_token_cache.json") }

    pub fn downloads_path(&self) -> PathBuf { self.data_dir.join("downloads") }

    pub fn db_path(&self) -> PathBuf { self.data_dir.join("backup-requests.db") }

    pub fn base_uri(&self) ->  Uri { self.base_uri.0.clone() }
}

impl Display for Config {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "data_dir={:?}, worker_count={}, token_cache_path={:?}, downloads_path={:?}, db_path={:?}, base_uri={}",
            self.data_dir,
            self.worker_count,
            self.token_cache_path(),
            self.downloads_path(),
            self.db_path(),
            self.base_uri()
        )
    }
}