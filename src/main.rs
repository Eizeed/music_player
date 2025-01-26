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

use iced::widget::{
    button, center, column, container, horizontal_space, keyed_column, progress_bar, row, text,
};
use iced::Length::{self, Fill};
use iced::{time, window, Element, Subscription, Task};
use lofty::file::AudioFile;
use lofty::probe::Probe;
use rodio::{OutputStream, Sink, Source};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, Sender};
use uuid::Uuid;

use player::{db, playlist::*, track::*};

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
    tracks: Vec<Track>, // All tracks found in system they are not meant to play anything
    init_queue: Vec<Track>, // This will be initial queue. It will be master-copy for queue
    queue: VecDeque<Track>, // All tracks OR tracks from playlist. This will pop tracks
    prio_queue: VecDeque<Track>, // Tracks added by user

    // To jump to prev tracks
    // Its FILA so vec is perfect
    backward_queue: Vec<Track>,

    // Currently playing track. As we pop tracks from queue or
    // prio_queue it will be here
    current_track: Option<Track>,

    playlists: Vec<Playlist>,
    current_playlist: Option<Playlist>,
    current_pos: Duration, // Current time pos of track

    sender: Sender<Command>,
    timer: DurationBar,
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
    LoadPlaylist(Vec<Playlist>),
    TrackMessage(usize, Uuid, TrackMessage),
    PlaylistMessage(usize, Uuid, PlaylistMessage),
    PlayTrack,
    ToggleTrack,
    JumpToNext,
    JumpToPrev,
    SetQueue((Result<Vec<Track>, String>, usize)),
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
            init_queue: vec![],
            queue: VecDeque::new(),
            prio_queue: VecDeque::default(),
            backward_queue: vec![],
            current_track: None,

            playlists: vec![],
            current_playlist: None,
            current_pos: Duration::default(),

            timer: DurationBar::default(),
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
                self.init_queue = self.tracks.clone();
                self.backward_queue = vec![];
                self.queue = VecDeque::new();

                Task::none()
            }
            Message::Loaded(Err(_err)) => Task::none(),
            Message::LoadPlaylist(playlists) => {
                self.playlists = playlists;
                Task::none()
            }
            Message::TrackMessage(i, _uuid, track_message) => {
                if let Some(track) = self.init_queue.get_mut(i) {
                    match track_message {
                        TrackMessage::ChooseTrack => {
                            let _ = track.update(track_message);
                            if !self.current_playlist.is_some() {
                                let tracks = self.tracks.clone();

                                let set_queue_task = Task::done(Message::SetQueue((Ok(tracks), i)));
                                let play_task = Task::done(Message::PlayTrack);

                                Task::batch(vec![set_queue_task, play_task])
                            } else {
                                let pool = self.db_pool.clone();
                                let playlist_uuid = self.current_playlist.as_ref().unwrap().uuid.clone();

                                let set_queue_task = Task::perform(
                                    async move {
                                        let tracks =
                                            get_tracks_from_playlist(playlist_uuid, pool).await;
                                        return (tracks, i);
                                    },
                                    Message::SetQueue,
                                );
                                let play_task = Task::done(Message::PlayTrack);

                                Task::batch(vec![play_task, set_queue_task])
                            }
                        }
                        TrackMessage::AddToQueue => {
                            let _ = track.update(track_message);
                            self.prio_queue.push_back(track.clone());

                            Task::none()
                        }
                        TrackMessage::OpenPlaylistMenu(_playlist) => {
                            let _ = track
                                .update(TrackMessage::OpenPlaylistMenu(self.playlists.clone()));
                            Task::none()
                        }
                        TrackMessage::ClosePlaylistMenu => {
                            let _ = track.update(track_message);
                            Task::none()
                        }
                        TrackMessage::ToggleInPlaylist(playlist) => {
                            let tracks = playlist.tracks.clone();
                            let _ = track.update(TrackMessage::ToggleInPlaylist(playlist.clone()));
                            let pool = self.db_pool.clone();
                            let track_uuid = track.uuid.clone();

                            let db_task;

                            let exists = tracks.iter().find(|uuid| **uuid == track_uuid);
                            if exists.is_some() {
                                println!("DELETE FROM PLAYLIST");
                                db_task = Task::perform(
                                    async move {
                                        let playlist_models =
                                            db::delete_from_playlist(&pool, playlist, track_uuid)
                                                .await;
                                        let playlists: Vec<Playlist> = playlist_models
                                            .into_iter()
                                            .map(Playlist::from)
                                            .collect();
                                        return playlists;
                                    },
                                    Message::LoadPlaylist,
                                )
                            } else {
                                println!("INSERT INTO PLAYLIST");
                                db_task = Task::perform(
                                    async move {
                                        let playlist_models =
                                            db::insert_into_playlist(&pool, playlist, track_uuid)
                                                .await;
                                        let playlists: Vec<Playlist> = playlist_models
                                            .into_iter()
                                            .map(Playlist::from)
                                            .collect();
                                        return playlists;
                                    },
                                    Message::LoadPlaylist,
                                )
                            }

                            db_task.chain(Task::done(Message::TrackMessage(
                                i,
                                _uuid,
                                TrackMessage::OpenPlaylistMenu(self.playlists.clone()),
                            )))
                        }
                        TrackMessage::TrackEnd(_) => Task::none(),
                    }
                } else {
                    Task::none()
                }
            }
            Message::PlaylistMessage(i, uuid, playlist_message) => match playlist_message {
                PlaylistMessage::SelectPlaylist => {
                    println!("selected");
                    let pool = self.db_pool.clone();

                    match &self.current_playlist {
                        Some(playlist) => {
                            println!("Current playlist.uuid: {}", playlist.uuid);
                            if playlist.uuid == uuid {
                                self.current_playlist = None;
                            } else {
                                self.current_playlist = Some(self.playlists[i].clone());
                            }
                        },
                        None => {
                            println!("No playlist");
                            self.current_playlist = Some(self.playlists[i].clone());
                        }
                    }

                    if self.current_playlist.is_some() {
                        Task::perform(
                            async move {
                                let track_models = db::get_tracks_from_playlist(&pool, uuid).await;
                                let tracks: Vec<Track> = track_models
                                    .into_iter()
                                    .map(|track| {
                                        let track_metadata = Probe::open(&track.path)
                                            .map_err(|_| LoadError::File)
                                            .unwrap()
                                            .read()
                                            .map_err(|_| LoadError::File)
                                            .unwrap();

                                        let duration = track_metadata.properties().duration();
                                        let duration_str = format!(
                                            "{}:{}",
                                            duration.as_secs() / 60,
                                            duration.as_secs() % 60
                                        );

                                        let uuid = Uuid::from_str(&track.uuid).unwrap();
                                        let path = PathBuf::from_str(&track.path).unwrap();
                                        let name =
                                        path.file_name().unwrap().to_str().unwrap().to_string();

                                        Track {
                                            uuid,
                                            name,
                                            duration_str,
                                            duration,
                                            path,
                                            playlists: None,
                                        }
                                    })
                                    .collect();

                                return (Ok(tracks), 0);
                            },
                            Message::SetQueue,
                        )
                    } else {
                        Task::done(Message::SetQueue((Ok(self.tracks.clone()), 0)))
                    }

                }
                _ => Task::none(),
            },
            Message::PlayTrack => {
                let sender = self.sender.clone();

                self.current_pos = Duration::default();
                self.timer = DurationBar::Ticking {
                    last_tick: Instant::now(),
                };

                let path = self.current_track.as_ref().unwrap().path.clone();

                println!("Track played");
                Task::perform(
                    async move {
                        let _ = sender.send(Command::Play(path)).await;
                    },
                    |_| (),
                )
                .discard()
            }
            Message::ToggleTrack => {
                if self.current_track.is_none() {
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
                if self.current_track.is_none() {
                    return Task::none();
                }

                self.backward_queue.push(self.current_track.take().unwrap());

                if self.prio_queue.len() > 0 {
                    self.current_track = self.prio_queue.pop_front();
                } else {
                    if self.queue.len() == 0 {
                        self.queue = self.init_queue.clone().into();
                        self.backward_queue = vec![];
                    };
                    self.current_track = self.queue.pop_front();
                }

                Task::done(Message::PlayTrack)
            }
            Message::JumpToPrev => {
                if self.current_track.is_none() {
                    return Task::none();
                }

                self.queue.push_front(self.current_track.take().unwrap());

                if self.backward_queue.len() == 0 {
                    self.backward_queue = self.init_queue.clone();
                    self.queue = VecDeque::new();
                };
                self.current_track = self.backward_queue.pop();

                Task::done(Message::PlayTrack)
            }
            Message::SetQueue((tracks, idx)) => {
                println!("Tracks for init queue: {tracks:#?}");
                self.init_queue = tracks.unwrap();
                self.backward_queue = self.init_queue.clone().into();
                self.queue = self.backward_queue.split_off(idx).into();

                self.current_track = Some(self.queue.pop_front().unwrap());
                Task::none()
            }
            Message::Tick(now) => {
                if self.current_track.is_none() {
                    return Task::none();
                }

                if let DurationBar::Ticking { last_tick } = &mut self.timer {
                    let dur = self.current_track.as_ref().unwrap().duration;
                    if self.current_pos >= dur {
                        return Task::done(Message::JumpToNext);
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
        let tracks: Element<_> = if self.init_queue.len() > 0 {
            keyed_column(self.init_queue.iter().enumerate().map(|(i, track)| {
                let uuid = track.uuid;
                (
                    track.uuid,
                    track
                        .view()
                        .map(move |message| Message::TrackMessage(i, uuid, message)),
                )
            }))
            .spacing(10)
            .width(Length::FillPortion(5))
            .height(Fill)
            .into()
        } else {
            center(text("Hello").width(Fill).size(25).color([0.7, 0.7, 0.7]))
                .height(200)
                .into()
        };

        let playlists = container(column![
            container(text("playlists")).padding([10, 0]),
            keyed_column(self.playlists.iter().enumerate().map(|(i, playlist)| {
                let uuid = playlist.uuid;
                (
                    playlist.uuid,
                    playlist
                        .view()
                        .map(move |message| Message::PlaylistMessage(i, uuid, message)),
                )
            }))
        ])
        .width(Length::FillPortion(1))
        .height(Length::Fill);

        let content = row![playlists, tracks].width(Fill).height(Fill);

        let mut dur = 0.0;
        if let Some(track) = &self.current_track {
            dur = track.duration.as_secs_f32();
        };

        let control = container(column![
            row![
                horizontal_space().width(Length::FillPortion(1)),
                progress_bar(0.0..=dur, self.current_pos.as_secs_f32())
                    .height(15)
                    .width(Length::FillPortion(2)),
                horizontal_space().width(Length::FillPortion(1)),
            ],
            row![
                horizontal_space(),
                button("<").on_press(Message::JumpToPrev),
                button("||").on_press(Message::ToggleTrack),
                button(">").on_press(Message::JumpToNext),
                horizontal_space(),
            ]
            .padding([10, 0])
            .spacing(50),
        ])
        .center_x(Fill);

        let content = column![content, control].padding([10, 20]);
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
    playlists: Vec<Playlist>,
}

impl SavedState {
    pub async fn load(pool: SqlitePool) -> Result<SavedState, LoadError> {
        let mut tracks: Vec<Track> = vec![];
        let mut paths = vec![];
        Self::visit_dir(&mut paths, HOME_PATH.into());

        db::init(&pool).await;
        db::update_track_state(&pool, &paths).await;
        let track_md_vec = db::get_tracks(&pool).await;
        let playlists = db::get_playlists(&pool)
            .await
            .into_iter()
            .map(Playlist::from)
            .collect();

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
                playlists: None,
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
            playlists: None,
        });
    }

    Ok(res)
}
