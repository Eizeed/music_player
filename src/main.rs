use std::collections::VecDeque;
use std::env;
use std::ffi::OsString;
use std::fmt::Debug;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, Instant};

use player::models::playlist_model::PlaylistModel;
use sqlx::pool::PoolOptions;
use sqlx::SqlitePool;

use iced::widget::{button, center, column, container, keyed_column, progress_bar, row, text};
use iced::Length::Fill;
use iced::{time, window, Element, Subscription, Task};
use lofty::file::AudioFile;
use lofty::probe::Probe;
use rodio::{OutputStream, Sink, Source};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, Sender};
use uuid::Uuid;

use player::{db, track::*};

pub const HOME_PATH: &str = "/home/lf/Music";

fn main() -> iced::Result {
    dotenvy::dotenv().ok();
    // Make config with its config file
    let path = PathBuf::from_str(HOME_PATH).unwrap();

    if !path.exists() {
        std::fs::create_dir(path).unwrap();
    }

    iced::application(Player::title, Player::update, Player::view)
        .window(window::Settings {
            ..Default::default()
        })
        .subscription(Player::subscription)
        .run_with(Player::new)
}

struct Player {
    tracks: Vec<Track>,          // All tracks found in system
    queue: Vec<Track>,           // All tracks OR tracks from playlist
    prio_queue: VecDeque<Track>, // Tracks added by user
    playlists: Vec<PlaylistModel>,
    current_pos: Duration, // Current time pos of track
    current_track_idx: Option<usize>,
    sender: Sender<Command>,
    timer: DurationBar,
    playlist: Option<PlaylistModel>,
    db_pool: SqlitePool,
}

#[derive(Debug, Clone)]
enum Command {
    Play(PathBuf),
    ToggleTrack,
}

#[derive(Debug, Clone)]
enum Message {
    Loaded(Result<SavedState, LoadError>),
    TrackMessage(usize, TrackMessage),
    PlayTrack((PathBuf, usize)),
    ChooseTrack((PathBuf, usize)),
    ToggleTrack,
    JumpToNext,
    JumpToPrev,
    AddToQueue(usize),
    SetQueue(Result<Vec<Track>, String>),
    Tick(Instant),
    Err(Result<(), String>),
}

#[derive(Debug, Clone, Default)]
enum DurationBar {
    #[default]
    Idle,
    Paused,
    Ticking {
        last_tick: Instant,
    },
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

                        println!("Track Thread: Playing track");
                        println!("Total duration = {dur:#?}");
                        sink.stop();
                        sink.play();
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
            }
            dbg!("Engine died")
        });

        if let None = env::var("DATABASE_URL").ok() {
            env::set_var("DATABASE_URL", "sqlite://db.sql");
        }

        let connection_string = env::var("DATABASE_URL").unwrap();

        let db_pool = PoolOptions::new()
            .max_connections(5)
            .connect_lazy(&connection_string)
            .expect("SQLite doesn't work");

        let player = Player {
            tracks: vec![],
            queue: vec![],
            playlists: vec![],
            prio_queue: VecDeque::default(),
            current_track_idx: None,
            current_pos: Duration::default(),
            timer: DurationBar::default(),
            playlist: None,
            sender: tx,
            db_pool,
        };

        let pool = player.db_pool.clone();

        (
            player,
            Task::perform(SavedState::load(pool), Message::Loaded),
        )
    }

    fn title(&self) -> String {
        "Iced music player".to_string()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Loaded(Ok(state)) => {
                self.tracks = state.tracks;
                self.playlists = state.playlists;
                self.queue = self.tracks.clone();

                Task::none()
            }
            Message::Loaded(Err(_err)) => Task::none(),
            Message::TrackMessage(i, track_message) => {
                if let Some(track) = self.queue.get_mut(i) {
                    match track_message {
                        TrackMessage::ChooseTrack => {
                            let _ = track.update(track_message);
                            let path = track.path.clone();
                            Task::perform(async move { return (path, i) }, Message::ChooseTrack)
                        }
                        TrackMessage::AddToQueue => {
                            let _ = track.update(track_message);
                            Task::perform(async move { return i }, Message::AddToQueue)
                        }
                        TrackMessage::TrackEnd(_) => Task::none(),
                    }
                } else {
                    Task::none()
                }
            }
            Message::PlayTrack((path, idx)) => {
                let sender = self.sender.clone();

                self.current_pos = Duration::default();
                self.timer = DurationBar::Ticking {
                    last_tick: Instant::now(),
                };

                let track_path;
                if self.prio_queue.len() > 0 {
                    track_path = self.prio_queue[0].path.clone();
                } else {
                    self.current_track_idx = Some(idx);
                    track_path = path;
                }

                println!("Track played");
                Task::perform(
                    async move {
                        let _ = sender.send(Command::Play(track_path)).await;
                    },
                    |_| (),
                )
                .discard()
            }
            Message::ChooseTrack((path, idx)) => {
                if !self.playlist.is_some() {
                    self.queue = self.tracks.clone();
                    Task::perform(async move { return (path, idx) }, Message::PlayTrack)
                } else {
                    let pool = self.db_pool.clone();
                    let uuid = &self.playlist.as_ref().unwrap().uuid;
                    let playlist_uuid = Uuid::from_str(uuid).unwrap();

                    let play_task = Task::perform(
                        async move {
                            return (path, idx);
                        },
                        Message::PlayTrack,
                    );

                    let set_queue_task = Task::perform(
                        get_tracks_from_playlist(playlist_uuid, pool),
                        Message::SetQueue,
                    );

                    Task::batch(vec![play_task, set_queue_task])
                }
            }
            Message::ToggleTrack => {
                if let None = self.current_track_idx {
                    return Task::none();
                }

                if let DurationBar::Paused = self.timer {
                    self.timer = DurationBar::Ticking {
                        last_tick: Instant::now(),
                    };
                } else if let DurationBar::Ticking { .. } = self.timer {
                    self.timer = DurationBar::Paused;
                }

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
                if let None = self.current_track_idx {
                    return Task::none();
                }

                if self.prio_queue.len() > 0 {
                    self.prio_queue.pop_front();
                }

                let idx;
                if self.current_track_idx.unwrap() + 1 > self.queue.len() - 1 {
                    idx = 0;
                } else {
                    idx = self.current_track_idx.unwrap() + 1;
                };

                let prev_track = self.queue.get(idx).unwrap();
                let path = prev_track.path.clone();
                self.current_track_idx = Some(idx);
                Task::perform(
                    async move {
                        return (path, idx);
                    },
                    Message::PlayTrack,
                )
            }
            Message::JumpToPrev => {
                if let None = self.current_track_idx {
                    return Task::none();
                }

                let idx = if let Some(idx) = self.current_track_idx.unwrap().checked_sub(1) {
                    idx
                } else {
                    self.queue.len() - 1
                };

                let prev_track = self.queue.get(idx).unwrap();
                let path = prev_track.path.clone();
                self.current_track_idx = Some(idx);
                Task::perform(
                    async move {
                        return (path, idx);
                    },
                    Message::PlayTrack,
                )
            }
            Message::AddToQueue(idx) => {
                let track = self.queue[idx].clone();
                self.prio_queue.push_back(track);
                if let None = self.current_track_idx {
                    self.current_track_idx = Some(idx);
                }
                Task::none()
            }
            Message::SetQueue(tracks) => {
                self.queue = tracks.unwrap();
                Task::none()
            }
            Message::Tick(now) => {
                if let DurationBar::Ticking { last_tick } = &mut self.timer {
                    let dur = if self.prio_queue.len() > 0 {
                        self.prio_queue[0].duration
                    } else {
                        self.queue
                            .get(self.current_track_idx.unwrap())
                            .unwrap()
                            .duration
                    };

                    if self.current_pos >= dur {
                        return Task::perform(async move { () }, |_| Message::JumpToNext);
                    } else {
                        self.current_pos += now - *last_tick;
                        *last_tick = now;
                        return Task::none();
                    }
                    // println!("now {:#?}", self.current_pos);
                }
                Task::none()
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

        let mut dur = 0.0;
        if let Some(idx) = self.current_track_idx {
            let track = &self.queue[idx];
            dur = track.duration.as_secs_f32();
        } else {
            ()
        };

        let control = container(column![
            progress_bar(0.0..=dur, self.current_pos.as_secs_f32()),
            row![
                button("<").on_press(Message::JumpToPrev),
                button("||").on_press(Message::ToggleTrack),
                button(">").on_press(Message::JumpToNext),
            ]
            .spacing(50),
        ])
        .center_x(Fill);
        let content = column![tracks, control].padding([10, 20]);
        container(content).width(Fill).height(Fill).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let tick = match self.timer {
            DurationBar::Idle | DurationBar::Paused => Subscription::none(),
            DurationBar::Ticking { .. } => {
                time::every(Duration::from_millis(10)).map(Message::Tick)
            }
        };

        Subscription::batch(vec![tick])
    }
}

#[derive(Debug, Clone)]
pub enum LoadError {
    File,
    Format,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavedState {
    tracks: Vec<Track>,
    playlists: Vec<PlaylistModel>,
}

impl SavedState {
    pub async fn load(pool: SqlitePool) -> Result<SavedState, LoadError> {
        let mut tracks: Vec<Track> = vec![];
        let mut paths = vec![];
        Self::visit_dir(&mut paths, HOME_PATH.into());

        db::init(&pool).await;
        db::update_track_state(&pool, &paths).await;
        let track_md_vec = db::get_tracks(&pool).await;
        let playlists = db::get_playlists(&pool).await;

        for track in track_md_vec {
            let track_metadata = Probe::open(&track.path)
                .map_err(|_| LoadError::File)?
                .read()
                .map_err(|_| LoadError::File)?;

            let duration = track_metadata.properties().duration();
            let duration_str = format!("{}:{}", duration.as_secs() / 60, duration.as_secs() % 60);

            let uuid = Uuid::from_str(&track.uuid).unwrap();
            let path = PathBuf::from_str(&track.path).unwrap();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();

            tracks.push(Track {
                uuid,
                name,
                duration_str,
                duration,
                path,
            });
        }

        Ok(SavedState { tracks, playlists })
    }

    fn visit_dir(paths: &mut Vec<PathBuf>, dir: PathBuf) {
        println!("{:?}", dir);
        if dir.is_dir() {
            for entry in dir.read_dir().unwrap() {
                let path = entry.unwrap().path();
                if path.file_name().unwrap().to_str().unwrap().starts_with(".") {
                    continue;
                };

                println!("{:?}", path);
                if path.is_dir() {
                    Self::visit_dir(paths, path);
                } else if path.is_file() && path.extension() == Some(&OsString::from("mp3")) {
                    paths.push(path);
                }
            }
        }
    }
}

async fn get_tracks_from_playlist(
    playlist_uuid: Uuid,
    pool: SqlitePool,
) -> Result<Vec<Track>, String> {
    let mut res = vec![];
    let tracks = db::get_tracks_from_playlist(&pool, playlist_uuid).await;
    for track in tracks {
        let track_metadata = Probe::open(&track.path)
            .map_err(|_| LoadError::File)
            .unwrap()
            .read()
            .map_err(|_| LoadError::File)
            .unwrap();

        let duration = track_metadata.properties().duration();
        let duration_str = format!("{}:{}", duration.as_secs() / 60, duration.as_secs() % 60);

        let uuid = Uuid::from_str(&track.uuid).unwrap();
        let path = PathBuf::from_str(&track.path).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();

        res.push(Track {
            uuid,
            name,
            duration_str,
            duration,
            path,
        });
    }

    Ok(res)
}
