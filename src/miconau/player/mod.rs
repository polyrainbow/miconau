mod mpv_process;

use mpv_process::*;
use mpvipc::{Event, Mpv, MpvCommand, NumberChangeOptions, PlaylistAddOptions};
use tokio::sync::{broadcast};

use crate::library::{Library};
use std::env;
use std::ops::Deref;
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

    pub fn play_playlist(&mut self, playlist_index: u8) {
        if playlist_index < self.library.playlists.len() as u8 {
            let playlist = self.library.playlists.get(playlist_index as usize).unwrap();
            let playlist_name = playlist.title.clone();
            println!("Playing playlist {}", playlist_name);
            let mut path = self.library.folder.clone();
            path.push_str("/");
            path.push_str(&playlist_name);
            
            // Get first track info for display
            let (track_title, artist) = if let Some(first_track) = playlist.tracks.first() {
                let title = first_track.title.clone().unwrap_or_else(|| {
                    first_track.filename
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".to_string())
                });
                (title, first_track.artist.clone())
            } else {
                (String::new(), None)
            };
            
            self.mpv_controller.run_command(
                MpvCommand::LoadFile {
                    file: path,
                    option: PlaylistAddOptions::Replace,
                }
            ).unwrap();

            self.mpv_controller.set_property(
                "loop-playlist",
                String::from("no"),
            ).unwrap();

            self.mpv_controller.set_property("pause", false)
                .expect("Error setting pause property to false");

            // Clear any existing queue and populate with remaining tracks from playlist
            self.queue.clear();
            for track in playlist.tracks.iter().skip(1) {
                let queue_track_title = track.title.clone().unwrap_or_else(|| {
                    track.filename
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".to_string())
                });
                self.queue.push(QueueItem {
                    playlist_name: playlist_name.clone(),
                    track_title: queue_track_title,
                    track_artist: track.artist.clone(),
                    file_path: track.filename.to_string_lossy().to_string(),
                });
            }
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
            println!("Playlist with index {} not found. Playing error sound.", playlist_index);
            self.play_error();
            self.set_state(PlayerState {
                source_info: None,
                mode: PlayerMode::Stopped,
            });
        }
    }

    pub fn play_playlist_track(
        &mut self,
        playlist_index: u8,
        track_index: u8,
    ) {
        if playlist_index < self.library.playlists.len() as u8 {
            let playlist = self.library.playlists
                .get(playlist_index as usize).unwrap();
            let playlist_name = playlist.title.clone();
            let track = playlist.tracks
                .get(track_index as usize).unwrap();
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

            self.set_state(PlayerState {
                source_info: Some(SourceInfo::Track {
                    track_title,
                    artist,
                    playlist_name,
                }),
                mode: PlayerMode::Playing,
            });
        } else {
            println!("Playlist with index {} not found. Playing error sound.", playlist_index);
            self.play_error();
            self.set_state(PlayerState {
                source_info: None,
                mode: PlayerMode::Stopped,
            });
        }
    }

    pub fn play_stream(&mut self, stream_index: u8) {
        if stream_index < self.library.streams.len() as u8 {
            let stream = self.library.streams.get(stream_index as usize).unwrap();
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
        let _ = self.mpv_controller.run_command(
            MpvCommand::PlaylistPrev,
        );
        self.update_state_from_mpv_playlist();
    }

    pub fn play_next_track(&mut self) {
        // If there are items in the queue, play from queue instead
        if !self.queue.is_empty() {
            self.play_next_from_queue();
            return;
        }
        let _ = self.mpv_controller.run_command(
            MpvCommand::PlaylistNext,
        );
        self.update_state_from_mpv_playlist();
    }

    /// Updates the player state based on the current mpv playlist position.
    /// Used after playlist-next/playlist-prev commands.
    fn update_state_from_mpv_playlist(&mut self) {
        // Get current playlist position from mpv
        let playlist_pos: usize = match self.mpv_controller.get_property("playlist-pos") {
            Ok(pos) => pos,
            Err(_) => return, // Can't get position, don't update state
        };

        // Try to find the current playlist from state
        let playlist_name = match &self.state.source_info {
            Some(SourceInfo::Track { playlist_name, .. }) => playlist_name.clone(),
            _ => return, // Not playing a playlist, don't update
        };

        // Find the playlist in the library
        let playlist = match self.library.playlists.iter().find(|p| p.title == playlist_name) {
            Some(p) => p,
            None => return,
        };

        // Get the track at the current position
        if let Some(track) = playlist.tracks.get(playlist_pos) {
            let track_title = track.title.clone().unwrap_or_else(|| {
                track.filename
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Unknown".to_string())
            });
            let artist = track.artist.clone();

            self.set_state(PlayerState {
                source_info: Some(SourceInfo::Track {
                    track_title,
                    artist,
                    playlist_name,
                }),
                mode: PlayerMode::Playing,
            });
        }
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

    pub fn play_next_from_queue(&mut self) -> bool {
        if self.queue.is_empty() {
            return false;
        }
        let item = self.queue.remove(0);
        println!("Playing from queue: {}", item.track_title);
        
        self.mpv_controller.run_command(
            MpvCommand::LoadFile {
                file: item.file_path.clone(),
                option: PlaylistAddOptions::Replace,
            }
        ).unwrap();

        self.mpv_controller.set_property("pause", false)
            .expect("Error setting pause property to false");

        self.set_state(PlayerState {
            source_info: Some(SourceInfo::Track {
                track_title: item.track_title,
                artist: item.track_artist,
                playlist_name: item.playlist_name,
            }),
            mode: PlayerMode::Playing,
        });

        self.notify_queue_updated();
        true
    }

    /// Called when mpv advances to the next track in its playlist.
    /// Syncs the Rust queue by removing the first item if the queue is non-empty
    /// and we're past the first track in mpv's playlist.
    pub fn on_track_started(&mut self) {
        // Check if we have queue items and we're playing a queued track
        // mpv's playlist-pos > 0 means we've advanced beyond the first track
        let playlist_pos: usize = self.mpv_controller
            .get_property("playlist-pos")
            .unwrap_or(0);
        
        if !self.queue.is_empty() && playlist_pos > 0 {
            // We're playing a track from the queue
            let item = self.queue.remove(0);
            println!("Playing queued track: {} - {}", item.playlist_name, item.track_title);
            
            // Update state to show the new track
            self.set_state(PlayerState {
                source_info: Some(SourceInfo::Track {
                    track_title: item.track_title,
                    artist: item.track_artist,
                    playlist_name: item.playlist_name,
                }),
                mode: PlayerMode::Playing,
            });
            
            self.notify_queue_updated();
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
        assert!(json.contains("\"Playlist\""));
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
        assert!(json.contains("\"Queue\""));
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
