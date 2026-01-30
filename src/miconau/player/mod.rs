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

#[derive(Serialize, Copy, Clone, Debug)]
enum PlayerMode {
    Paused,
    Playing,
    Stopped,
}

#[derive(Serialize, Copy, Clone, Debug)]
enum SourceType {
    Stream,
    Playlist,
}
#[derive(Serialize, Clone, Debug)]
pub struct PlayerState {
    source_type: Option<SourceType>,
    source_name: Option<String>,
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

        let (event_transmitter, _event_receiver) = broadcast::channel(1);

        let initial_state = PlayerState {
            source_type: None,
            source_name: None,
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
            let title = &playlist.title;
            println!("Playing playlist {}", title);
            let mut path = self.library.folder.clone();
            path.push_str("/");
            path.push_str(title);
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

            self.set_state(PlayerState {
                source_type: Some(SourceType::Playlist),
                source_name: Some(title.clone()),
                mode: PlayerMode::Playing,
            });
        } else {
            println!("Playlist with index {} not found. Playing error sound.", playlist_index);
            self.play_error();
            self.set_state(PlayerState {
                source_type: None,
                source_name: None,
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
            let title = &playlist.title;
            let track_path = &playlist.tracks
                .get(track_index as usize).unwrap().filename;
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
                source_type: Some(SourceType::Playlist),
                source_name: Some(title.clone()),
                mode: PlayerMode::Playing,
            });
        } else {
            println!("Playlist with index {} not found. Playing error sound.", playlist_index);
            self.play_error();
            self.set_state(PlayerState {
                source_type: None,
                source_name: None,
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
                source_type: Some(SourceType::Stream),
                source_name: Some(stream.name.clone()),
                mode: PlayerMode::Playing,
            });
        } else {
            println!("Stream with index {} not found. Playing error sound.", stream_index);
            self.play_error();
            self.set_state(PlayerState {
                source_type: None,
                source_name: None,
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
            source_type: self.state.source_type,
            source_name: self.state.source_name.clone(),
            mode: if is_paused { PlayerMode::Playing } else { PlayerMode::Paused },
        });
    }

    pub fn play_previous_track(&mut self) {
        let _ = self.mpv_controller.run_command(
            MpvCommand::PlaylistPrev,
        );
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
    }

    pub fn stop(&mut self) {
        self.mpv_controller.run_command_raw(
            "stop",
            &[&"keep-playlist"],
        ).unwrap();

        self.set_state(PlayerState {
            source_type: None,
            source_name: None,
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
        let track_title = track.filename
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());
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

    pub fn get_queue(&self) -> Vec<QueueItem> {
        self.queue.clone()
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
            source_type: Some(SourceType::Playlist),
            source_name: Some(format!("{} - {}", item.playlist_name, item.track_title)),
            mode: PlayerMode::Playing,
        });

        self.notify_queue_updated();
        true
    }

    /// Called when mpv advances to the next track in its playlist.
    /// Syncs the Rust queue by removing the first item if the queue is non-empty.
    pub fn on_track_advanced(&mut self) {
        if !self.queue.is_empty() {
            let item = self.queue.remove(0);
            println!("Track advanced, removing from queue: {}", item.track_title);
            
            // Update state to show the new track
            self.set_state(PlayerState {
                source_type: Some(SourceType::Playlist),
                source_name: Some(format!("{} - {}", item.playlist_name, item.track_title)),
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
                Ok(Event::EndFile) => {
                    println!("MPV: EndFile event received");
                    // Use blocking lock since we're in a std::thread
                    if let Ok(mut player) = player.try_lock() {
                        player.on_track_advanced();
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
