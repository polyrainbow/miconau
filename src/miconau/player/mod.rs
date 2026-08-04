mod mpv_process;

use mpv_process::*;
use mpvipc::{Event, Mpv, MpvCommand, NumberChangeOptions, PlaylistAddOptions};
use tokio::sync::{broadcast};

use crate::library::{Library};
use std::env;
use std::ops::Deref;
use std::path::Path;
use std::process::Child;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct QueueItem {
    pub playlist_name: String,
    pub track_title: String,
    pub track_artist: Option<String>,
    pub file_path: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum AppEvent {
    #[serde(rename = "playerState")]
    PlayerState(PlayerState),
    #[serde(rename = "libraryUpdated")]
    LibraryUpdated,
    #[serde(rename = "queueUpdated")]
    QueueUpdated { queue: Vec<QueueItem> },
}

#[derive(Serialize, Clone, Debug)]
enum PlayerMode {
    Paused,
    Playing,
    Stopped,
}

#[derive(Serialize, Clone, Debug)]
enum SourceInfo {
    Stream { stream_name: String },
    Track { track_title: String, artist: Option<String>, playlist_name: String },
}
#[derive(Serialize, Clone, Debug)]
pub struct PlayerState {
    source_info: Option<SourceInfo>,
    mode: PlayerMode,
}

pub struct Player {
    pub library: Library,
    mpv_process: Child,
    mpv_controller: Mpv,
    pub state: PlayerState,
    pub event_transmitter: broadcast::Sender<AppEvent>,
    _event_receiver: broadcast::Receiver<AppEvent>,
    pub queue: Vec<QueueItem>,
}

impl Player {
    pub async fn new(
        library: Library,
        output_device_name: Option<String>,
        socket_path: String,
    ) -> Player {
        let mpv_process = launch_mpv(output_device_name, socket_path.clone()).await;
        println!("MPV process initialized");

        let mpv_controller = Mpv::connect(&socket_path).unwrap();
        mpv_controller.set_volume(
            100.0,
            NumberChangeOptions::Absolute,
        ).unwrap();

        let (event_transmitter, _event_receiver) = broadcast::channel(16);

        let initial_state = PlayerState {
            source_info: None,
            mode: PlayerMode::Stopped,
        };

        return Player {
            library,
            mpv_process,
            mpv_controller,
            state: initial_state,
            event_transmitter,
            _event_receiver, // we need to keep the receiver to avoid dropping the channel
            queue: Vec::new(),
        };
    }

    fn set_state(&mut self, state: PlayerState) {
        self.state = state;

        match self.event_transmitter.send(AppEvent::PlayerState(self.state.clone())) {
            Ok(_) => println!("State updated: {:?}", self.state),
            Err(e) => println!("Error sending state update: {}", e),
        }
    }

    pub fn notify_library_updated(&self) {
        match self.event_transmitter.send(AppEvent::LibraryUpdated) {
            Ok(_) => println!("Library updated notification sent"),
            Err(e) => println!("Error sending library update: {}", e),
        }
    }

    pub fn destroy(&mut self) -> std::io::Result<()> {
        terminate(&mut self.mpv_process).unwrap();
        println!("MPV process terminated");
        Ok(())
    }

    pub fn play_playlist(&mut self, playlist_index: usize) {
        // Bounds are checked up front so the reference to the playlist below
        // does not keep the library borrowed while the error path needs
        // `&mut self`. A missing playlist has no tracks, so the one check
        // covers a bad playlist index as well.
        let track_count = self.library.playlists
            .get(playlist_index)
            .map_or(0, |playlist| playlist.tracks.len());

        if track_count == 0 {
            println!("Playlist with index {} not found. Playing error sound.", playlist_index);
            self.play_error();
            self.set_state(PlayerState {
                source_info: None,
                mode: PlayerMode::Stopped,
            });
            return;
        }

        let playlist = self.library.playlists.get(playlist_index).unwrap();
        let playlist_name = playlist.title.clone();
        println!("Playing playlist {}", playlist_name);

        let first_track = playlist.tracks.first().unwrap();
        let track_title = first_track.display_title();
        let artist = first_track.artist.clone();
        let first_path = first_track.filename.to_string_lossy().to_string();

        // The tracks that follow, as the queue will mirror them.
        let rest: Vec<QueueItem> = playlist.tracks
            .iter()
            .skip(1)
            .map(|track| QueueItem {
                playlist_name: playlist_name.clone(),
                track_title: track.display_title(),
                track_artist: track.artist.clone(),
                file_path: track.filename.to_string_lossy().to_string(),
            })
            .collect();

        // mpv is handed the tracks one by one rather than the playlist folder.
        // Given a folder, mpv enumerates it itself and plays everything it
        // considers playable, which includes the file types the scan filtered
        // out and leaves mpv's playlist out of step with `queue`.
        self.mpv_controller.run_command(
            MpvCommand::LoadFile {
                file: first_path,
                option: PlaylistAddOptions::Replace,
            }
        ).unwrap();

        for item in &rest {
            self.mpv_controller.run_command(
                MpvCommand::LoadFile {
                    file: item.file_path.clone(),
                    option: PlaylistAddOptions::Append,
                }
            ).unwrap();
        }

        self.mpv_controller.set_property(
            "loop-playlist",
            String::from("no"),
        ).unwrap();

        self.mpv_controller.set_property("pause", false)
            .expect("Error setting pause property to false");

        self.queue = rest;
        self.notify_queue_updated();

        self.set_state(PlayerState {
            source_info: Some(SourceInfo::Track {
                track_title,
                artist,
                playlist_name,
            }),
            mode: PlayerMode::Playing,
        });
    }

    pub fn play_playlist_track(
        &mut self,
        playlist_index: usize,
        track_index: usize,
    ) {
        // Bounds are checked up front so the reference to the playlist below
        // does not keep the library borrowed while the error path needs
        // `&mut self`. A missing playlist has no tracks, so the one check
        // covers a bad playlist index as well.
        let track_count = self.library.playlists
            .get(playlist_index)
            .map_or(0, |playlist| playlist.tracks.len());

        if track_index < track_count {
            let playlist = self.library.playlists
                .get(playlist_index).unwrap();
            let playlist_name = playlist.title.clone();
            let track = playlist.tracks
                .get(track_index).unwrap();
            let track_path = &track.filename;
            let track_title = track.title.clone().unwrap_or_else(|| {
                track_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Unknown".to_string())
            });
            let artist = track.artist.clone();
            println!("Playing track {}", track_path.clone().to_string_lossy());
            self.mpv_controller.run_command(
                MpvCommand::LoadFile {
                    file: track_path.to_string_lossy().to_string(),
                    option: PlaylistAddOptions::Replace,
                }
            ).unwrap();

            self.mpv_controller.set_property(
                "loop-playlist",
                String::from("no"),
            ).unwrap();

            self.mpv_controller.set_property("pause", false)
                .expect("Error setting pause property to false");

            // Clear queue since we replaced the playlist with a single track
            self.queue.clear();
            self.notify_queue_updated();

            self.set_state(PlayerState {
                source_info: Some(SourceInfo::Track {
                    track_title,
                    artist,
                    playlist_name,
                }),
                mode: PlayerMode::Playing,
            });
        } else {
            println!(
                "Track {} of playlist {} not found. Playing error sound.",
                track_index, playlist_index,
            );
            self.play_error();
            self.set_state(PlayerState {
                source_info: None,
                mode: PlayerMode::Stopped,
            });
        }
    }

    pub fn play_stream(&mut self, stream_index: usize) {
        if stream_index < self.library.streams.len() {
            let stream = self.library.streams.get(stream_index).unwrap();
            println!("Playing stream {}", &stream.url);
            self.mpv_controller.run_command(
                MpvCommand::LoadFile {
                    file: stream.url.clone(),
                    option: PlaylistAddOptions::Replace,
                }
            ).unwrap();

            self.mpv_controller.set_property(
                "loop-playlist",
                String::from("no"),
            ).unwrap();

            self.mpv_controller.set_property("pause", false)
                .expect("Error setting pause property to false");

            // Clear queue since we replaced the playlist with a stream
            self.queue.clear();
            self.notify_queue_updated();

            self.set_state(PlayerState {
                source_info: Some(SourceInfo::Stream {
                    stream_name: stream.name.clone(),
                }),
                mode: PlayerMode::Playing,
            });
        } else {
            println!("Stream with index {} not found. Playing error sound.", stream_index);
            self.play_error();
            self.set_state(PlayerState {
                source_info: None,
                mode: PlayerMode::Stopped,
            });
        }
    }

    pub fn play_error(&mut self) {
        let mut dir = env::current_exe().unwrap();
        dir.pop();
        dir.pop();
        dir.pop();
        dir.push("assets");
        dir.push("error.wav");
        let dir_str = dir.to_string_lossy().deref().to_string();

        self.mpv_controller.run_command(
            MpvCommand::LoadFile {
                file: dir_str,
                option: PlaylistAddOptions::Replace,
            }
        ).unwrap();

        // Clear queue since we replaced the playlist
        self.queue.clear();
        self.notify_queue_updated();
    }

    pub fn play_pause(&mut self) {
        let is_paused: bool = self.mpv_controller.get_property("pause").unwrap();
        println!("setting is paused: {:?}", !is_paused);
        self.mpv_controller.set_property("pause", !is_paused)
            .expect("Error pausing");

        self.set_state(PlayerState {
            source_info: self.state.source_info.clone(),
            mode: if is_paused { PlayerMode::Playing } else { PlayerMode::Paused },
        });
    }

    pub fn play_previous_track(&mut self) {
        // At the first track there is nothing to go back to: mpv refuses the
        // command and keeps playing, so the queue must stay untouched too.
        let playlist_pos: usize = self.mpv_controller
            .get_property("playlist-pos")
            .unwrap_or(0);
        if playlist_pos == 0 {
            return;
        }

        // Going back puts the track we are leaving at the front of the queue:
        // it is the next thing that will play again.
        let current_track = match &self.state.source_info {
            Some(SourceInfo::Track { track_title, artist, playlist_name }) => Some((
                track_title.clone(),
                artist.clone(),
                playlist_name.clone(),
            )),
            _ => None,
        };
        if let Some((track_title, track_artist, playlist_name)) = current_track {
            if let Ok(file_path) = self.mpv_controller.get_property::<String>("path") {
                self.queue.insert(0, QueueItem {
                    playlist_name,
                    track_title,
                    track_artist,
                    file_path,
                });
                self.notify_queue_updated();
            }
        }

        let _ = self.mpv_controller.run_command(
            MpvCommand::PlaylistPrev,
        );
        // State is updated by on_track_started when mpv fires StartFile event
    }

    pub fn play_next_track(&mut self) {
        let _ = self.mpv_controller.run_command(
            MpvCommand::PlaylistNext,
        );
        // Queue sync is handled by on_track_started when mpv fires StartFile event
    }

    /// What to display for the file mpv is playing, looked up in the library.
    /// Returns None for anything the library doesn't know, such as a stream or
    /// the error sound.
    fn source_info_for_file(&self, file_path: &str) -> Option<SourceInfo> {
        let (playlist, track) = self.library.find_track(Path::new(file_path))?;
        Some(SourceInfo::Track {
            track_title: track.display_title(),
            artist: track.artist.clone(),
            playlist_name: playlist.title.clone(),
        })
    }

    pub fn stop(&mut self) {
        self.mpv_controller.run_command_raw(
            "stop",
            &[&"keep-playlist"],
        ).unwrap();

        self.set_state(PlayerState {
            source_info: None,
            mode: PlayerMode::Stopped,
        });
    }

    pub fn add_to_queue(&mut self, playlist_index: usize, track_index: usize) -> Result<(), String> {
        if playlist_index >= self.library.playlists.len() {
            return Err("Playlist not found".to_string());
        }
        let playlist = &self.library.playlists[playlist_index];
        if track_index >= playlist.tracks.len() {
            return Err("Track not found".to_string());
        }
        let track = &playlist.tracks[track_index];
        let track_title = track.title.clone().unwrap_or_else(|| {
            track.filename
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        });
        let track_artist = track.artist.clone();
        let file_path = track.filename.to_string_lossy().to_string();

        // Append to mpv's internal playlist
        self.mpv_controller.run_command(
            MpvCommand::LoadFile {
                file: file_path.clone(),
                option: PlaylistAddOptions::Append,
            }
        ).map_err(|e| format!("Failed to append to mpv playlist: {}", e))?;

        // Add to our queue for UI display
        self.queue.push(QueueItem {
            playlist_name: playlist.title.clone(),
            track_title,
            track_artist,
            file_path,
        });
        self.notify_queue_updated();
        Ok(())
    }

    pub fn remove_from_queue(&mut self, index: usize) -> Result<(), String> {
        if index >= self.queue.len() {
            return Err("Queue item not found".to_string());
        }
        
        // Get current playlist position to calculate the correct mpv playlist index
        // Queue items are appended after the current playlist, so we need to offset
        let current_pos: usize = self.mpv_controller
            .get_property("playlist-pos")
            .unwrap_or(0);
        let mpv_index = current_pos + 1 + index;
        
        // Remove from mpv's playlist
        let _ = self.mpv_controller.run_command_raw(
            "playlist-remove",
            &[&mpv_index.to_string()],
        );
        
        self.queue.remove(index);
        self.notify_queue_updated();
        Ok(())
    }

    pub fn clear_queue(&mut self) {
        // Get current playlist position
        let current_pos: usize = self.mpv_controller
            .get_property("playlist-pos")
            .unwrap_or(0);
        
        let playlist_count: usize = self.mpv_controller
            .get_property("playlist-count")
            .unwrap_or(0);
        
        // Remove all items after the current position from mpv's playlist
        if playlist_count > current_pos + 1 {
            // Remove from the end to avoid index shifting issues
            for i in ((current_pos + 1)..playlist_count).rev() {
                let _ = self.mpv_controller.run_command_raw(
                    "playlist-remove",
                    &[&i.to_string()],
                );
            }
        }
        
        self.queue.clear();
        self.notify_queue_updated();
    }

    fn notify_queue_updated(&self) {
        match self.event_transmitter.send(AppEvent::QueueUpdated { queue: self.queue.clone() }) {
            Ok(_) => println!("Queue updated notification sent"),
            Err(e) => println!("Error sending queue update: {}", e),
        }
    }

    /// Called when mpv starts a file. Keeps the queue and the displayed track
    /// in step with what mpv is actually playing, which is the file mpv
    /// reports rather than a position: going back leaves the queue where it
    /// is, so a position alone cannot tell the two directions apart.
    pub fn on_track_started(&mut self) {
        let current_file: String = match self.mpv_controller.get_property("path") {
            Ok(path) => path,
            Err(_) => return, // Can't tell what is playing, don't update state
        };

        // Playing on: the file that started is the one at the head of the
        // queue, so it moves out of the queue and into the display.
        let plays_head_of_queue = self.queue
            .first()
            .map_or(false, |item| item.file_path == current_file);
        if plays_head_of_queue {
            let item = self.queue.remove(0);
            println!("Playing queued track: {} - {}", item.playlist_name, item.track_title);

            self.set_state(PlayerState {
                source_info: Some(SourceInfo::Track {
                    track_title: item.track_title,
                    artist: item.track_artist,
                    playlist_name: item.playlist_name,
                }),
                mode: PlayerMode::Playing,
            });

            self.notify_queue_updated();
            return;
        }

        // Anything else - going back, or the first track of a playlist - keeps
        // the queue and takes what to display from the library.
        if let Some(source_info) = self.source_info_for_file(&current_file) {
            self.set_state(PlayerState {
                source_info: Some(source_info),
                mode: PlayerMode::Playing,
            });
        }
    }
}

/// Spawns a background task that listens for mpv events and syncs the queue.
/// This should be called after creating the Player.
pub fn spawn_mpv_event_listener(
    socket_path: String,
    player: std::sync::Arc<tokio::sync::Mutex<Player>>,
) {
    std::thread::spawn(move || {
        // Create a separate mpv connection for event listening
        let mut event_mpv = match Mpv::connect(&socket_path) {
            Ok(mpv) => mpv,
            Err(e) => {
                eprintln!("Failed to connect event listener to mpv: {}", e);
                return;
            }
        };
        
        println!("MPV event listener started");
        
        loop {
            match event_mpv.event_listen() {
                Ok(Event::StartFile) => {
                    println!("MPV: StartFile event received");
                    // A new file started - sync queue and update status
                    // Use a blocking approach with retry
                    loop {
                        match player.try_lock() {
                            Ok(mut player_guard) => {
                                player_guard.on_track_started();
                                break;
                            }
                            Err(_) => {
                                std::thread::sleep(std::time::Duration::from_millis(10));
                            }
                        }
                    }
                }
                Ok(Event::Idle) => {
                    println!("MPV: Idle event received");
                }
                Ok(Event::Shutdown) => {
                    println!("MPV: Shutdown event received");
                    break;
                }
                Ok(_) => {
                    // Ignore other events
                }
                Err(e) => {
                    eprintln!("MPV event listener error: {}", e);
                    break;
                }
            }
        }
        
        println!("MPV event listener stopped");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_state_serializes_correctly_when_stopped() {
        let state = PlayerState {
            source_info: None,
            mode: PlayerMode::Stopped,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"mode\":\"Stopped\""));
        assert!(json.contains("\"source_info\":null"));
    }

    #[test]
    fn player_state_serializes_correctly_for_stream() {
        let state = PlayerState {
            source_info: Some(SourceInfo::Stream {
                stream_name: "Test Radio".to_string(),
            }),
            mode: PlayerMode::Playing,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"mode\":\"Playing\""));
        assert!(json.contains("\"Stream\""));
        assert!(json.contains("\"stream_name\":\"Test Radio\""));
    }

    #[test]
    fn player_state_serializes_correctly_for_playlist() {
        let state = PlayerState {
            source_info: Some(SourceInfo::Track {
                track_title: "My Song".to_string(),
                artist: Some("The Artist".to_string()),
                playlist_name: "My Playlist".to_string(),
            }),
            mode: PlayerMode::Playing,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"Track\""));
        assert!(json.contains("\"track_title\":\"My Song\""));
        assert!(json.contains("\"artist\":\"The Artist\""));
        assert!(json.contains("\"playlist_name\":\"My Playlist\""));
    }

    #[test]
    fn player_state_serializes_correctly_for_playlist_without_artist() {
        let state = PlayerState {
            source_info: Some(SourceInfo::Track {
                track_title: "Unknown Track".to_string(),
                artist: None,
                playlist_name: "Untitled".to_string(),
            }),
            mode: PlayerMode::Paused,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"mode\":\"Paused\""));
        assert!(json.contains("\"artist\":null"));
    }

    #[test]
    fn player_state_serializes_correctly_for_queue() {
        let state = PlayerState {
            source_info: Some(SourceInfo::Track {
                track_title: "Queued Song".to_string(),
                artist: Some("Queue Artist".to_string()),
                playlist_name: "Source Playlist".to_string(),
            }),
            mode: PlayerMode::Playing,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"Track\""));
        assert!(json.contains("\"track_title\":\"Queued Song\""));
    }

    #[test]
    fn app_event_serializes_with_type_tag() {
        let event = AppEvent::PlayerState(PlayerState {
            source_info: None,
            mode: PlayerMode::Stopped,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"playerState\""));
    }

    #[test]
    fn app_event_library_updated_serializes_correctly() {
        let event = AppEvent::LibraryUpdated;
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"libraryUpdated\""));
    }

    #[test]
    fn app_event_queue_updated_serializes_correctly() {
        let event = AppEvent::QueueUpdated {
            queue: vec![
                QueueItem {
                    playlist_name: "Test Playlist".to_string(),
                    track_title: "Test Track".to_string(),
                    track_artist: Some("Test Artist".to_string()),
                    file_path: "/path/to/file.flac".to_string(),
                }
            ],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"queueUpdated\""));
        assert!(json.contains("\"queue\""));
        assert!(json.contains("\"track_title\":\"Test Track\""));
    }

    #[test]
    fn queue_item_serializes_correctly() {
        let item = QueueItem {
            playlist_name: "Album".to_string(),
            track_title: "Song".to_string(),
            track_artist: None,
            file_path: "/music/song.flac".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"playlist_name\":\"Album\""));
        assert!(json.contains("\"track_title\":\"Song\""));
        assert!(json.contains("\"track_artist\":null"));
        assert!(json.contains("\"file_path\":\"/music/song.flac\""));
    }
}
