extern crate spotify_backup;

use rspotify::spotify::client::Spotify;
use rspotify::spotify::oauth2::{SpotifyClientCredentials, SpotifyOAuth};
use rspotify::spotify::util::get_token;

#[test]
fn test_full_backup() {
    let mut oauth = SpotifyOAuth::default()
        .scope("user-library-read playlist-read-private")
        .build();

    let token = get_token(&mut oauth).unwrap();

    let client_credential = SpotifyClientCredentials::default()
        .token_info(token)
        .build();

    let spotify = Spotify::default()
        .client_credentials_manager(client_credential)
        .build();

    let backup = spotify_backup::backup_fn::DefaultBackup::full_backup("simaomm", &spotify).unwrap();

    assert!(backup.albums.len() >= 389);
}
