use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use iced::{
    overlay,
    widget::{button, checkbox, container, horizontal_space, row, text, Column},
    Element, Length, Task,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::playlist::{self, Playlist};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub uuid: Uuid,
    pub name: String,
    pub duration_str: String,
    pub duration: Duration,
    pub path: PathBuf,
    pub playlists: Option<Vec<Playlist>>,
}

#[derive(Debug, Clone)]
pub enum TrackMessage {
    ChooseTrack,
    OpenPlaylistMenu(Vec<Playlist>),
    ClosePlaylistMenu,
    ToggleInPlaylist(Playlist),
    AddToQueue,
    TrackEnd(Result<(), String>),
}

impl Track {
    pub fn update(&mut self, message: TrackMessage) -> Task<TrackMessage> {
        match message {
            TrackMessage::ChooseTrack => {
                println!("Play clicked");
                let path = self.path.clone();
                println!("{path:#?}");
                Task::none()
            }
            TrackMessage::OpenPlaylistMenu(playlists) => {
                self.playlists = Some(playlists);
                Task::none()
            }
            TrackMessage::ClosePlaylistMenu => {
                self.playlists = None;
                Task::none()
            }
            TrackMessage::ToggleInPlaylist(playlist) => {
                Task::none()
            }
            TrackMessage::AddToQueue => {
                println!("Added to queue");
                Task::none()
            }
            TrackMessage::TrackEnd(_res) => Task::none(),
        }
    }

    pub fn view(&self) -> Element<TrackMessage> {
        let name = button(text(&self.name))
            .on_press(TrackMessage::ChooseTrack)
            .width(Length::FillPortion(6));

        let duration = text(&self.duration_str)
            .width(Length::FillPortion(1))
            .center();

        let add_button = container(button("+").on_press(TrackMessage::AddToQueue))
            .width(Length::FillPortion(1))
            .center_x(Length::Fill);

        let add_to_liked = container(button("<3").on_press(TrackMessage::OpenPlaylistMenu(vec![])))
            .width(Length::FillPortion(1))
            .center_x(Length::Fill);

        let mut playlist_container: Vec<Element<'_, TrackMessage>> = vec![];
        if let Some(playlists) = &self.playlists {
            for playlist in playlists {
                playlist_container.push(
                    button(playlist.title.as_ref())
                        .on_press(TrackMessage::ToggleInPlaylist(playlist.clone()))
                        .into(),
                );
            }
        }

        let playlist_container = container(Column::from_vec(playlist_container));

        let buttons = row![add_button, add_to_liked];

        let content = row![name, duration, buttons, playlist_container].into();

        return content;
    }
}
