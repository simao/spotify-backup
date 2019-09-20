use rspotify::spotify::util::get_token;
use crate::spotify::*;
use crate::backup_fn::DefaultBackup;
use std::path::PathBuf;

pub fn cli() {
    // Needs env file
    let mut oauth = build_spotify_oauth("http://localhost:8000", PathBuf::from("./spotify_token_cache.json"));

    match get_token(&mut oauth) {
        Some(token_info) => {
            let backup = DefaultBackup::run_backup(token_info).unwrap();

            let serialized = serde_json::to_string_pretty(&backup).unwrap();

            println!("{}", serialized);

            log::info!(
                "Saved {} albums, {} playlists",
                backup.albums.len(),
                backup.playlists.len()
            );
        }
        None => log::error!("auth failed"),
    };
}
