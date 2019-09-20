use crate::serialize::*;
use crate::spotify::client::*;
use failure::Error;

pub fn backup_albums(spotify: &dyn SpotifyClient) -> Result<Vec<Album>, Error> {
    let mut parsed_albums = vec![];
    let mut offset = 0;

    loop {
        let albums = spotify.saved_albums(Some(50), Some(offset))?;

        parsed_albums.extend(albums.items);

        if albums.next.is_none() {
            break;
        } else {
            offset = albums.offset + 50;
        }
    }

    Ok(parsed_albums)
}

#[cfg(test)]
mod tests {
    use super::*;

    use mockall::predicate::*;
    use mockall::*;
    use rspotify::spotify::model::page::Page;
    use std::iter;

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

    fn new_album() -> Album {
        let fake_artist = Artist {
            name: "Fernando Pessoa".into(),
        };

        Album {
            title: "Livro do desassossego".into(),
            artists: vec![fake_artist],
            album_type: None,
            release_date: None,
        }
    }

    fn new_page(size: Option<usize>, next: Option<String>) -> Page<Album> {
        let albums: Vec<Album> = iter::repeat_with(|| new_album())
            .take(size.unwrap_or(1))
            .collect();

        Page {
            href: "https://...".into(),
            items: albums,
            limit: 1,
            offset: 0,
            previous: None,
            total: 1,
            next,
        }
    }

    #[test]
    fn test_backup_albums() {
        let mut mock = MockSpotifyClientM::new();

        mock.expect_saved_albums()
            .with(eq(Some(50)), eq(Some(0)))
            .times(1)
            .returning(|_, _| Ok(new_page(None, None)));

        let backed_up_albums = backup_albums(&mock).unwrap();

        let p = new_page(None, None);

        assert_eq!(backed_up_albums, p.items);
    }

    #[test]
    fn test_backup_multiple_pages() {
        let mut mock = MockSpotifyClientM::new();

        mock.expect_saved_albums()
            .with(eq(Some(50)), eq(Some(0)))
            .times(1)
            .returning(|_, _| Ok(new_page(Some(50), Some("next".into()))));

        mock.expect_saved_albums()
            .with(eq(Some(50)), eq(Some(50)))
            .times(1)
            .returning(|_, _| Ok(new_page(Some(3), None)));

        let backed_up_albums = backup_albums(&mock).unwrap();

        let p = new_page(Some(53), None);

        assert_eq!(backed_up_albums, p.items);
    }
}
