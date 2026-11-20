//! Tests moved up from playing's files (the app split's phase 2): each runs on the whole app (every_copy's `App`), so they go
//! up to the top. By origin, their bodies as they were but for paths.

/// From `playon`'s tests.
#[cfg(test)]
mod playon {
    use crate::app::ToggleError;
    use crate::app::playon_base::{Command, PlayFrom};

    /// X6: Home Assistant names a playlist as it is spoken: Play On finds it by its name, whatever its capitals, and asks for it by
    /// its id. A name no playlist has is said to be none.
    #[tokio::test]
    async fn play_on_plays_a_playlist_by_its_name_regardless_of_capitals() {
        let app = super::super::every_copy::app("playon-named").await;
        app.playlists.create("Restaurant de Lift").await.unwrap();
        let id = app.playlists.view(None).await.unwrap().into_iter().find(|p| p.name == "Restaurant de Lift").unwrap().id;
        assert!(id.starts_with("pl-"), "{id}");
        let p = &app.play_on;
        let _page = p.connect_browser("Laptop", Some("tab-1")).unwrap();
        let play = |name: &str| Command::Play { from: PlayFrom { playlist: Some(name.into()), ..PlayFrom::default() }, tracks: None, index: 0, position_ms: 0, shuffle: false, paused: false, queue_id: None, queue_name: None };
        // Found and read, by its id: it holds nothing that plays (no library here), which is all it can say.
        for asked in ["Restaurant de Lift", "restaurant DE lift"] {
            let r = p.command("Laptop", play(asked)).await;
            assert!(matches!(&r, Err(ToggleError::Refused(_))), "{asked}: {r:?}");
        }
        assert!(matches!(p.command("Laptop", play("Restaurant de Gare")).await, Err(ToggleError::UnknownPlaylist)));
        assert_eq!(p.playlist_named("restaurant DE lift").await.unwrap(), id);
    }
}
