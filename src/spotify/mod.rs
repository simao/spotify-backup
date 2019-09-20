use rspotify::spotify::oauth2::SpotifyOAuth;
use failure::Error;
use std::path::PathBuf;

pub mod auth;
pub mod client;

pub fn build_spotify_oauth(base_url: &str, cache_path: PathBuf) -> SpotifyOAuth {
    // Needs env file
    SpotifyOAuth::default()
        .redirect_uri(&format!("{}/callback", base_url.trim_end_matches("/")))
        .cache_path(cache_path)
        .scope("user-library-read playlist-read-private") // TODO: Maybe needs more scopes?
        .build()
}

pub fn build_user_redirect_uri(oauth: &SpotifyOAuth) -> Result<String, Error> {
    let state = rspotify::spotify::util::generate_random_string(16);
    let auth_url = oauth.get_authorize_url(Some(&state), None);

    Ok(auth_url)
}
