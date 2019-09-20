use rspotify::spotify::client::Spotify;
use rspotify::spotify::model::page::Page;
use rspotify::spotify::model::track::FullTrack;
use rspotify::spotify::senum::AlbumType;
use std::str::FromStr;
use rspotify::spotify::model::album::FullAlbum;

use failure::Error;

use crate::serialize::*;

// TODO: Nothing in this file is tested at all, write it tests

pub trait SpotifyClient {
    fn saved_albums(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Page<Album>, Error>;

    fn playlists(
        &self,
        user_id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Page<PlaylistId>, Error>;

    fn playlist(
        &self,
        playlist_id: &PlaylistId,
    ) -> Result<(Playlist, Page<Track>), Error>;

    fn playlist_tracks(
        &self,
        user_id: &str,
        playlist_id: &PlaylistId,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Page<Track>, Error>;
}


trait SpotifyApiRetry {
    fn with_api_retry<F, T>(&self, max_tries: u32, f: F) -> Result<T, Error>
        where
            F: Fn(&Self) -> Result<T, Error>;
}

impl SpotifyApiRetry for Spotify {
    fn with_api_retry<F, T>(&self, max_tries: u32, f: F) -> Result<T, Error>
        where
            F: Fn(&Spotify) -> Result<T, Error> {
        let result = f(&self);

        if let Err(ref err) = result {
            if let Some(rspotify::spotify::client::ApiError::RateLimited(Some(i))) = err.downcast_ref::<rspotify::spotify::client::ApiError>() {
                std::thread::sleep(std::time::Duration::from_secs(*i as u64 * 2));

                if max_tries > 0 {
                    log::warn!("rate limited, retrying api call after {} seconds", i);
                    self.with_api_retry(max_tries - 1, f)
                } else {
                    log::error!("rate limited, giving up");
                    result
                }
            } else {
                result
            }
        } else {
            result
        }
    }
}

impl SpotifyClient for Spotify {
    fn saved_albums(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Page<Album>, Error> {
        self.current_user_saved_albums(limit, offset)
            .map(|page| {
                let albums: Vec<Album> = page.items.iter().map(|i| i.album.clone().into()).collect();

                Page {
                    href: page.href,
                    items: albums,
                    limit: page.limit,
                    offset: page.offset,
                    previous: page.previous,
                    total: page.total,
                    next: page.next,
                }
            })
            .map_err(|e| e.into())
    }

    fn playlists(
        &self,
        user_id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Page<PlaylistId>, Error> {
        self.user_playlists(user_id, limit, offset)
            .map(|page| {
                let ids = page.items.iter().map(|p| p.id.clone()).collect();

                Page {
                    href: page.href,
                    items: ids,
                    limit: page.limit,
                    offset: page.offset,
                    previous: page.previous,
                    total: page.total,
                    next: page.next,
                }
            })
            .map_err(|e| e.into())
    }

    fn playlist(
        &self,
        playlist_id: &PlaylistId,
    ) -> Result<(Playlist, Page<Track>), Error> {
        let f = |spotify: &Self| {
            spotify.playlist(playlist_id, None, None)
                .map(|p| {
                    let playlist = Playlist {
                        id: p.id,
                        name: p.name,
                        tracks: vec![],
                        track_count: 0,
                    };

                    let full_tracks: Vec<Track> =
                        p
                            .tracks
                            .items
                            .iter()
                            .map(|i| i.track.iter())
                            .flatten()
                            .map(|t| t.clone().into())
                            .collect();

                    let tracks = Page {
                        href: p.tracks.href,
                        items: full_tracks,
                        limit: p.tracks.limit,
                        offset: p.tracks.offset,
                        previous: p.tracks.previous,
                        total: p.tracks.total,
                        next: p.tracks.next,
                    };

                    (playlist, tracks)
                })
                .map_err(|e| e.into())
        };

        self.with_api_retry(3, f)
    }

    fn playlist_tracks(
        &self,
        user_id: &str,
        playlist_id: &PlaylistId,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Page<Track>, Error> {
        let f = |spotify: &Self| {
            spotify.user_playlist_tracks(user_id, playlist_id, None, limit, offset, None)
                .map(|page| {
                    let full_tracks: Vec<Track> =
                        page
                            .items
                            .iter()
                            .map(|i| i.track.iter())
                            .flatten()
                            .map(|t| t.clone().into())
                            .collect();

                    Page {
                        href: page.href,
                        items: full_tracks,
                        limit: page.limit,
                        offset: page.offset,
                        previous: page.previous,
                        total: page.total,
                        next: page.next,
                    }})
                .map_err(|e| e.into())
        };

        self.with_api_retry(3, f)
    }
}

impl From<FullAlbum> for Album {
    fn from(full_album: FullAlbum) -> Self {
        let artists: Vec<Artist> = full_album
            .artists
            .iter()
            .map(|artist| Artist {
                name: artist.name.clone(),
            })
            .collect();

        Album {
            title: full_album.name.clone(),
            artists,
            album_type: Some(full_album.album_type.clone()),
            release_date: Some(full_album.release_date.clone()),
        }
    }
}

impl From<FullTrack> for Track {
    fn from(full_track: FullTrack) -> Self {
        Track {
            name: full_track.name.clone(),
            album: Album {
                title: full_track.album.name.clone(),
                artists: full_track
                    .album
                    .artists
                    .iter()
                    .map(|a| Artist {
                        name: a.name.clone(),
                    })
                    .collect(),
                album_type: full_track
                    .album
                    .album_type
                    .clone()
                    .and_then(|at| AlbumType::from_str(&at).ok()),
                release_date: full_track.album.release_date.clone(),
            },
        }
    }
}
