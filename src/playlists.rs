use rspotify::spotify::model::page::Page;
use failure::Error;

use crate::serialize::*;
use crate::spotify::client::*;

fn extract_playlists(
    user_id: &str,
    spotify: &dyn SpotifyClient,
    dest: &mut Vec<Playlist>,
    playlists: Vec<(Playlist, Page<Track>)>,
) -> Result<(), Error> {
    for (p, page) in &playlists {
        log::debug!("Parsing playlist {:?}", p.name);

        let mut tracks = vec![];
        let mut next_page: Page<Track> = page.clone();

        loop {
            tracks.append(&mut next_page.items);

            let offset = next_page.offset + next_page.limit;

            if next_page.next.is_some() {
                next_page = spotify
                    .playlist_tracks(user_id, &p.id, Some(next_page.limit), Some(offset))?;
            } else {
                break;
            }
        }

        let track_count = tracks.len();

        let parsed_playlist = Playlist {
            id: p.id.clone(),
            name: p.name.clone(),
            tracks,
            track_count,
        };

        dest.push(parsed_playlist);
    }

    Ok(())
}

fn get_full_playlists(
    spotify: &dyn SpotifyClient,
    playlist_ids: Vec<PlaylistId>,
) -> Result<Vec<(Playlist, Page<Track>)>, Error> {
    playlist_ids
        .iter()
        .map(|p| spotify.playlist(&p))
        .collect()
}

pub fn backup_playlists(user_id: &str, spotify: &dyn SpotifyClient) -> Result<Vec<Playlist>, Error> {
    let mut parsed_playlists: Vec<Playlist> = vec![];
    let mut offset = 0;

    loop {
        let playlists = spotify.playlists(user_id, Some(50), Some(offset))?;
        let full_playlists = get_full_playlists(spotify, playlists.items)?;
        extract_playlists(user_id, spotify, &mut parsed_playlists, full_playlists)?;

        if playlists.next.is_none() {
            break;
        } else {
            offset = offset + 50;
        }
    }

    Ok(parsed_playlists)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;
    use mockall::*;
    use rspotify::spotify::model::page::Page;

    mock! {
        pub SpotifyClientM { }
        trait SpotifyClient {
            fn saved_albums(
                &self,
                limit: Option<u32>,
                offset: Option<u32>
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
    }

    fn new_page<T>(items: Vec<T>, offset: u32, total: u32, next: Option<String>) -> Page<T> {
        Page {
            href: "https://...".into(),
            items,
            limit: 50,
            offset,
            previous: None,
            total,
            next,
        }
    }

    fn new_track(name: &str) -> Track {
        let artist = Artist {
            name: format!("Artist: {}", name),
        };

        let album = Album {
            title: format!("Album: {}", name),
            artists: vec![artist],
            album_type: None,
            release_date: None,
        };

        Track {
            name: format!("Track: {}", name),
            album,
        }
    }

    #[test]
    fn test_backup_playlists_empty() {
        let mut mock = MockSpotifyClientM::new();

        mock.expect_playlists()
            .with(eq("myuser"), eq(Some(50)), eq(Some(0)))
            .times(1)
            .returning(|_, _, _| Ok(new_page(vec![], 0, 0, None)));

        let backup = backup_playlists("myuser", &mock).unwrap();
        assert_eq!(backup.len(), 0)
    }

    #[test]
    fn test_backup_playlists_single_playlist() {
        let mut mock = MockSpotifyClientM::new();

        mock.expect_playlists()
            .with(eq("myuser"), eq(Some(50)), eq(Some(0)))
            .times(1)
            .returning(|_, _, _| Ok(new_page(vec!["playlist-id-01".into()], 0, 1, None)));

        mock.expect_playlist_tracks()
            .with(
                eq("myuser"),
                eq("playlist-id-01".to_owned()),
                eq(Some(50)),
                eq(Some(50)),
            )
            .times(1)
            .returning(|_, _, _, _| {
                let track = new_track("track from `playlist_tracks`");
                Ok(new_page(vec![track], 1, 1, None))
            });

        mock.expect_playlist()
            .with(eq("playlist-id-01".to_owned()))
            .times(1)
            .returning(|_| {
                let playlist = Playlist {
                    id: "playlist-id-01".into(),
                    name: "Playlist 01".into(),
                    tracks: vec![],
                    track_count: 0,
                };

                let track = new_track("My Track");
                let tracks_page =
                    new_page(vec![track], 0, 1, Some("http://some-other-page".to_owned()));

                Ok((playlist, tracks_page))
            });

        let backup = backup_playlists("myuser", &mock).unwrap();
        assert_eq!(backup.len(), 1);

        let playlist = backup.get(0).unwrap();
        assert_eq!(playlist.id, "playlist-id-01".to_owned());
        assert_eq!(playlist.name, "Playlist 01".to_owned());
        assert_eq!(playlist.tracks.len(), 2);
        assert_eq!(playlist.track_count, 2);

        let track = backup.get(0).unwrap().tracks.get(0).unwrap();
        assert_eq!(track.name, "Track: My Track");
        assert_eq!(track.album.title, "Album: My Track");

        let track02 = backup.get(0).unwrap().tracks.get(1).unwrap();
        assert_eq!(track02.name, "Track: track from `playlist_tracks`");
        assert_eq!(track02.album.title, "Album: track from `playlist_tracks`");
    }

    #[test]
    fn test_backup_playlists_multiple_pages() {
        let mut mock = MockSpotifyClientM::new();

        mock.expect_playlists()
            .with(eq("myuser"), eq(Some(50)), eq(Some(0)))
            .times(1)
            .returning(|_, _, _| {
                Ok(new_page(
                    vec!["playlist-id-01".into()],
                    0,
                    1,
                    Some("https://second-playlist-page".into()),
                ))
            });

        mock.expect_playlists()
            .with(eq("myuser"), eq(Some(50)), eq(Some(50)))
            .times(1)
            .returning(|_, _, _| Ok(new_page(vec!["playlist-id-02".into()], 0, 1, None)));

        mock.expect_playlist().times(2).returning(|id| {
            let playlist = Playlist {
                id: id.into(),
                name: format!("Playlist: {}", id),
                tracks: vec![],
                track_count: 0,
            };

            let track = new_track(&format!("My Track: {}", id));
            let tracks_page = new_page(vec![track], 0, 1, None);

            Ok((playlist, tracks_page))
        });

        let backup = backup_playlists("myuser", &mock).unwrap();
        assert_eq!(backup.len(), 2);

        let mut playlist = backup.get(0).unwrap();
        assert_eq!(playlist.id, "playlist-id-01");
        assert_eq!(playlist.name, "Playlist: playlist-id-01");
        assert_eq!(playlist.tracks.len(), 1);
        assert_eq!(playlist.track_count, 1);

        playlist = backup.get(1).unwrap();
        assert_eq!(playlist.id, "playlist-id-02");
        assert_eq!(playlist.name, "Playlist: playlist-id-02");
        assert_eq!(playlist.tracks.len(), 1);
        assert_eq!(playlist.track_count, 1);
    }
}
