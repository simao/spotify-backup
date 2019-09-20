use rspotify::spotify::senum::AlbumType;
use serde::Serialize;

pub type PlaylistId = String;
#[derive(Serialize, Debug, Clone)]
pub struct Backup {
    pub albums: Vec<Album>,
    pub playlists: Vec<Playlist>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Artist {
    pub name: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct Album {
    pub title: String,
    pub artists: Vec<Artist>,
    pub album_type: Option<AlbumType>,
    pub release_date: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Track {
    pub name: String,
    pub album: Album,
}

#[derive(Serialize, Debug, Clone)]
pub struct Playlist {
    #[serde(skip)]
    pub id: PlaylistId,
    pub name: String,
    pub tracks: Vec<Track>,
    pub track_count: usize,
}


impl PartialEq for Album {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title &&
            self.artists == other.artists &&
            self.album_type.as_ref().map(|a| a.as_str()) == other.album_type.as_ref().map(|a| a.as_str()) &&
            self.release_date == other.release_date
    }
}
