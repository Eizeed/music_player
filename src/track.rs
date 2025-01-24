use std::{path::{Path, PathBuf}, time::Duration};

use iced::{widget::{button, horizontal_space, row, text}, Element, Length, Task};
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
    PlayTrack,
    AddToQueue,
    TrackEnd(Result<(), String>),
}

impl Track {
    pub fn update(&mut self, message: TrackMessage) -> Task<TrackMessage> {
        match message {
            TrackMessage::PlayTrack => {
                println!("Play clicked");
                let path = self.path.clone();
                println!("{path:#?}");
                Task::none()
            },
            TrackMessage::AddToQueue => {
                println!("Added to queue");
                Task::none()
            }
            TrackMessage::TrackEnd(_res) => {
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<TrackMessage> {
        let name = text(&self.name).width(Length::FillPortion(2));
        let duration = text(&self.duration_str).width(Length::FillPortion(1));

        let content = row![name, duration];
        let track_data = button(content).on_press(TrackMessage::PlayTrack);

        let add_button = button("+").on_press(TrackMessage::AddToQueue);

        let track = row![track_data, horizontal_space(), add_button].into();

        return track;
    }

}







