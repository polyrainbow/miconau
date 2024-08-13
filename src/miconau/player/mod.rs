mod mpv_process;

use mpv_process::*;
use mpvipc::{Mpv, MpvCommand, NumberChangeOptions, PlaylistAddOptions};

use crate::library::Library;
use std::env;
use std::ops::Deref;
use std::process::Child;

static MPV_SOCKET_PATH: &str = "/tmp/mpvsocket";

pub struct Player {
    // output_device_name: Option<String>,
    pub library: Library,
    // current_indexes: Option<Indexes>,
    mpv_process: Child,
    mpv_controller: Mpv,
}

impl Player {
    pub fn new(
        library: Library,
        // output_device_name: Option<String>,
    ) -> Player {
        let mpv_process = launch_mpv();
        println!("MPV process initialized");

        let mpv_controller = Mpv::connect(MPV_SOCKET_PATH).unwrap();
        mpv_controller.set_volume(
            100.0,
            NumberChangeOptions::Absolute,
        ).unwrap();

        return Player {
            // output_device_name,
            library,
            // current_indexes: None,
            mpv_process,
            mpv_controller,
        };
    }

    pub fn destroy(&mut self) -> std::io::Result<()> {
        terminate(&mut self.mpv_process)
    }

    pub fn play_playlist(&mut self, playlist_index: u8) {
        if playlist_index < self.library.playlists.len() as u8 {
            let playlist = self.library.playlists.get(playlist_index as usize).unwrap();
            let title = &playlist.title;
            println!("Playing playlist {}", title);
            let mut path = "../miconau-music-lib/".to_owned();
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
        } else {
            println!("Playlist with index {} not found. Playing error sound.", playlist_index);
            self.play_error();
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
        } else {
            println!("Stream with index {} not found. Playing error sound.", stream_index);
            self.play_error();
        }
    }

    pub fn play_error(&mut self) {
        let mut dir = env::current_exe().unwrap();
        dir.pop();
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
        println!("is paused: {:?}", is_paused);
        self.mpv_controller.set_property("pause", !is_paused)
            .expect("Error pausing")
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
    }

    pub fn handle_player_event(&mut self) {
        todo!();
    }
}
