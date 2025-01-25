use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use iced::{
    widget::{button, container, horizontal_space, row, text},
    Element, Length, Task,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub uuid: Uuid,
    pub name: String,
    pub duration_str: String,
    pub duration: Duration,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum TrackMessage {
    ChooseTrack,
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

        let duration = text(&self.duration_str).width(Length::FillPortion(1)).center();

        let add_button = container(button("+")
            .on_press(TrackMessage::AddToQueue))
            .width(Length::FillPortion(1))
            .center_x(Length::Fill);

        let content = row![name, duration, add_button].into();

        return content;
    }
}
