mod mpv_process;

use mpv_process::*;
use mpvipc::{Mpv, MpvCommand, NumberChangeOptions, PlaylistAddOptions};
use tokio::sync::{broadcast};

use crate::library::{Library};
use std::env;
use std::ops::Deref;
use std::process::Child;
use serde::Serialize;

static MPV_SOCKET_PATH: &str = "/tmp/mpvsocket";

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
    pub state_transmitter: broadcast::Sender<PlayerState>,
    _state_receiver: broadcast::Receiver<PlayerState>,
}

impl Player {
    pub async fn new(
        library: Library,
        output_device_name: Option<String>,
    ) -> Player {
        let mpv_process = launch_mpv(output_device_name).await;
        println!("MPV process initialized");

        let mpv_controller = Mpv::connect(MPV_SOCKET_PATH).unwrap();
        mpv_controller.set_volume(
            100.0,
            NumberChangeOptions::Absolute,
        ).unwrap();

        let (state_transmitter, _state_receiver) = broadcast::channel(1);

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
            state_transmitter,
            _state_receiver, // we need to keep the receiver to avoid dropping the channel
        };
    }

    fn set_state(&mut self, state: PlayerState) {
        self.state = state;

        match self.state_transmitter.send(self.state.clone()) {
            Ok(_) => println!("State updated: {:?}", self.state),
            Err(e) => println!("Error sending state update: {}", e),
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
}
