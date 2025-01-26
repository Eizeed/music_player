use std::str::FromStr;

use iced::{
    widget::{button, container},
    Element, Task,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::playlist_model::PlaylistModel;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Playlist {
    pub uuid: Uuid,
    pub title: String,
    pub tracks: Vec<Uuid>,
}

impl From<PlaylistModel> for Playlist {
    fn from(value: PlaylistModel) -> Self {
        let uuid = Uuid::from_str(&value.uuid).unwrap();
        let tracks: Vec<Uuid> = serde_json::from_str(value.tracks.as_str().unwrap()).map_err(|e| eprintln!("{e:?}")).unwrap();
        Self {
            uuid,
            title: value.title,
            tracks,
        }
    }
}

#[derive(Debug, Clone)]
pub enum PlaylistMessage {
    SelectPlaylist,
    DiscardPlaylist,
    AddPlaylist,
    RemovePlaylist,
}

impl Playlist {
    pub fn update(&mut self, message: PlaylistMessage) -> Task<PlaylistMessage> {
        match message {
            PlaylistMessage::SelectPlaylist => Task::none(),
            PlaylistMessage::DiscardPlaylist => Task::none(),
            PlaylistMessage::AddPlaylist => Task::none(),
            PlaylistMessage::RemovePlaylist => Task::none(),
        }
    }

    pub fn view(&self) -> Element<PlaylistMessage> {
        let title = container(button(self.title.as_ref()).on_press(PlaylistMessage::SelectPlaylist));

        return title.into();
    }
}













