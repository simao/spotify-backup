use rspotify::spotify::client::Spotify;
use failure::Error;

mod playlists;
mod albums;
mod serialize;
mod spotify;

pub mod server;
pub mod cli;
pub mod config;

pub mod backup_fn {
    use super::*;
    use rspotify::spotify::oauth2::{SpotifyClientCredentials, TokenInfo};
    use crate::serialize::*;

    pub trait BackupFn {
        fn apply(&self, token_info: TokenInfo) -> Result<Backup, Error>;
    }

    pub struct DefaultBackup;

    impl DefaultBackup {
        pub fn run_backup(token_info: TokenInfo) -> Result<Backup, Error> {
            let client_credential = SpotifyClientCredentials::default()
                .token_info(token_info)
                .build();

            let spotify = Spotify::default()
                .client_credentials_manager(client_credential)
                .build();

            let user_id = spotify.me()?.id;

            Self::full_backup(&user_id, &spotify)
        }

        pub fn full_backup(user_id: &str, spotify: &Spotify) -> Result<Backup, Error> {
            let albums = albums::backup_albums(spotify)?;

            let playlists = playlists::backup_playlists(&user_id, spotify)?;

            Ok(Backup {
                albums,
                playlists,
            })
        }
    }

    impl BackupFn for DefaultBackup {
        fn apply(&self, token_info: TokenInfo) -> Result<Backup, Error> {
            DefaultBackup::run_backup(token_info)
        }
    }
}

