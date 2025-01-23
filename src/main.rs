use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::str::FromStr;

use iced::widget::{button, center, column, container, keyed_column, row, text};
use iced::Length::Fill;
use iced::{window, Element, Task};
use lofty::file::AudioFile;
use lofty::probe::Probe;
use rodio::{OutputStream, Sink, Source};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, Sender};
use uuid::Uuid;

mod track;
use crate::track::*;

pub const HOME_PATH: &str = "/home/lf/Music";

fn main() -> iced::Result {
    // Make config with its config file
    let path = PathBuf::from_str(HOME_PATH).unwrap();

    if !path.exists() {
        std::fs::create_dir(path).unwrap();
    }

    iced::application(Player::title, Player::update, Player::view)
        .window(window::Settings {
            ..Default::default()
        })
        .run_with(Player::new)
}

struct Player {
    tracks: Vec<Track>,
    current_queue: Vec<Track>,
    current_track_idx: Option<usize>,
    sender: Sender<Command>,
}

#[derive(Debug, Clone)]
enum Command {
    Play(PathBuf),
    ToggleTrack,
}

#[derive(Debug, Clone)]
enum Message {
    Loaded(Result<Vec<Track>, LoadError>),
    TrackMessage(usize, TrackMessage),
    ToggleTrack,
    JumpToNext,
    JumpToPrev,
    Err(Result<(), String>),
}

impl Player {
    fn new() -> (Self, Task<Message>) {
        let (tx, mut rx) = mpsc::channel::<Command>(100);

        tokio::task::spawn_blocking(move || {
            let (_stream, stream_handle) = OutputStream::try_default().unwrap();
            let sink = Sink::try_new(&stream_handle).unwrap();

            while let Some(command) = rx.blocking_recv() {
                match command.clone() {
                    Command::Play(path) => {
                        let file = File::open(path).unwrap();
                        let source = rodio::Decoder::new(BufReader::new(file)).unwrap();
                        let dur = source.total_duration();

                        println!("Total duration = {dur:#?}");
                        sink.stop();
                        sink.append(source);
                    }
                    Command::ToggleTrack => {
                        if sink.is_paused() {
                            sink.play();
                            println!("Track resumed");
                        } else {
                            sink.pause();
                            println!("Track paused");
                        }
                    }
                };

                println!("Currently tracks in queue = {}", sink.len());
                println!("Track is on position = {:?} s", sink.get_pos());
            }
            dbg!("Engine died")
        });

        let player = Player {
            tracks: vec![],
            current_queue: vec![],
            current_track_idx: None,
            sender: tx,
        };

        (player, Task::perform(SavedState::load(), Message::Loaded))
    }

    fn title(&self) -> String {
        "Iced music player".to_string()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Loaded(Ok(tracks)) => {
                self.tracks = tracks;

                Task::none()
            }
            Message::Loaded(Err(_err)) => Task::none(),
            Message::TrackMessage(i, track_message) => {
                if let Some(track) = self.tracks.get_mut(i) {
                    println!("{i}");

                    match track_message {
                        TrackMessage::PlayTrack => {
                            let _ = track.update(track_message);

                            let path = track.path.clone();
                            let sender = self.sender.clone();

                            println!("Track played");
                            self.current_track_idx = Some(i);
                            Task::perform(
                                async move {
                                    let _ = sender.send(Command::Play(path)).await;
                                },
                                |_| (),
                            )
                            .discard()
                        }
                        _ => Task::none(),
                    }
                } else {
                    Task::none()
                }
            }
            Message::ToggleTrack => {
                let sender = self.sender.clone();
                Task::perform(
                    async move {
                        let _ = sender.send(Command::ToggleTrack).await;
                    },
                    |_| (),
                )
                .discard()
            }
            Message::JumpToNext => {
                let sender = self.sender.clone();
                let idx = if let Some(idx) = self.current_track_idx.unwrap().checked_add(1) {
                    idx
                } else {
                    0
                };

                if let Some(prev_track) = self.tracks.get(idx) {
                    let path = prev_track.path.clone();
                    self.current_track_idx = Some(idx);
                    Task::perform(
                        async move {
                            let _ = sender.send(Command::Play(path)).await;
                        },
                        |_| (),
                    )
                    .discard()
                } else {
                    let path = self.tracks[0].path.clone();
                    self.current_track_idx = Some(0);
                    Task::perform(
                        async move {
                            let _ = sender.send(Command::Play(path)).await;
                        },
                        |_| (),
                    )
                    .discard()
                }
            }
            Message::JumpToPrev => {
                let sender = self.sender.clone();
                let idx = if let Some(idx) = self.current_track_idx.unwrap().checked_add(1) {
                    idx
                } else {
                    self.tracks.len() - 1
                };

                if let Some(prev_track) = self.tracks.get(idx) {
                    let path = prev_track.path.clone();
                    self.current_track_idx = Some(idx);
                    Task::perform(
                        async move {
                            let _ = sender.send(Command::Play(path)).await;
                        },
                        |_| (),
                    )
                    .discard()
                } else {
                    let path = self.tracks[self.tracks.len() - 1].path.clone();
                    self.current_track_idx = Some(self.tracks.len() - 1);
                    Task::perform(
                        async move {
                            let _ = sender.send(Command::Play(path)).await;
                        },
                        |_| (),
                    )
                    .discard()
                }
            }
            Message::Err(res) => {
                println!("{res:#?}");
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let tracks: Element<_> = if self.tracks.len() > 0 {
            keyed_column(self.tracks.iter().enumerate().map(|(i, track)| {
                (
                    track.uuid,
                    track
                        .view()
                        .map(move |message| Message::TrackMessage(i, message)),
                )
            }))
            .spacing(10)
            .height(Fill)
            .into()
        } else {
            center(text("Hello").width(Fill).size(25).color([0.7, 0.7, 0.7]))
                .height(200)
                .into()
        };

        let control = container(
            row![
                button("<").on_press(Message::JumpToPrev),
                button("||").on_press(Message::ToggleTrack),
                button(">").on_press(Message::JumpToNext),
            ]
            .spacing(50),
        )
        .center_x(Fill);
        let content = column![tracks, control].padding([10, 20]);
        container(content).width(Fill).height(Fill).into()
    }
}

#[derive(Debug, Clone)]
pub enum LoadError {
    File,
    Format,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedState {
    tracks: Vec<Track>,
}

impl SavedState {
    pub async fn load() -> Result<Vec<Track>, LoadError> {
        let mut tracks = vec![];
        let mut paths = vec![];
        Self::visit_dir(&mut paths, HOME_PATH.into());

        for path in paths {
            let track_metadata = Probe::open(&path)
                .map_err(|_| LoadError::File)?
                .read()
                .map_err(|_| LoadError::File)?;

            let duration = track_metadata.properties().duration();
            let duration_str = format!("{}:{}", duration.as_secs() / 60, duration.as_secs() % 60);

            tracks.push(Track {
                uuid: Uuid::new_v4(),
                name: path.file_name().unwrap().to_str().unwrap().to_string(),
                duration_str,
                duration,
                path,
            });
        }

        return Ok(tracks);
    }

    fn visit_dir(paths: &mut Vec<PathBuf>, dir: PathBuf) {
        if dir.is_dir() {
            for entry in dir.read_dir().unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    Self::visit_dir(paths, path);
                } else if path.is_file() && path.extension().unwrap() == "mp3" {
                    paths.push(path);
                }
            }
        }
    }
}
