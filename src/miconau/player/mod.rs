use crate::MainThreadEvent;
use crate::library::Library;
use crate::callback_source::Callback;

use rodio::cpal::default_host;
use rodio::cpal::traits::HostTrait;
use rodio::DeviceTrait;
use rodio::{Decoder, Device, OutputStream, OutputStreamHandle, Sink, StreamError};
use std::fs::File;
use std::io::BufReader;
use std::io::Cursor;
use std::mem::replace;
use std::sync::mpsc::{Sender};

#[derive(Debug, Copy, Clone)]
pub struct Indexes {
    pub album: u8,
    pub track: u8,
}

#[derive(Debug, Copy, Clone)]
enum PlaybackSourceDescriptor {
    LibraryTrack(Indexes),
    ErrorSound,
}

enum AudioSource<'a> {
    LibrarySource(Decoder<BufReader<File>>),
    ErrorSource(Decoder<Cursor<&'a [u8]>>),
}

// we need to keep a reference to all those rodio interfaces because otherwise
// playback would be dropped
pub struct Player {
    output_device_name: Option<String>,
    output_stream: Option<OutputStream>,
    main_thread_sender: Sender<MainThreadEvent>,
    library: Library,
    current_indexes: Option<Indexes>,
    error_sound: &'static [u8],
    active_sink: Option<Sink>,
}

impl Player {
    pub fn new(
        library: Library,
        output_device_name: Option<String>,
        error_sound: &'static [u8],
        main_thread_sender: Sender<MainThreadEvent>,
    ) -> Player {
        return Player {
            output_device_name,
            output_stream: None,
            library,
            current_indexes: None,
            main_thread_sender,
            error_sound,
            active_sink: None,
        };
    }

    fn get_device(&mut self) -> Option<Device> {
        let host = default_host();

        match &self.output_device_name {
            Some(output_device_name) => {
                let device = host
                    .output_devices()
                    .unwrap()
                    .filter(|dev| dev.name().unwrap() == output_device_name.to_string())
                    .next();

                match device {
                    Some(device) => return Some(device),
                    None => {
                        println!(
                            "Device {} provided by argument is unknown or not available!",
                            output_device_name
                        );
                        return None;
                    }
                }
            }
            None => None,
        }
    }

    fn get_source(&mut self, indexes: Indexes) -> Decoder<BufReader<File>> {
        let album = &self.library.albums[indexes.album as usize];
        let track = &album.tracks[indexes.track as usize];
        let filename = &track.filename;
        println!(
            "Playing {}-{}: {:?}",
            indexes.album + 1,
            indexes.track + 1,
            filename
        );
        let file = BufReader::new(File::open(&filename).unwrap());
        Decoder::new(file).unwrap()
    }

    fn get_error_sound_decoder(&self) -> Decoder<Cursor<&'static [u8]>> {
        Decoder::new(Cursor::new(self.error_sound)).unwrap()
    }

    fn get_stream_handle(
        &mut self,
        device: Option<Device>,
    ) -> Result<(OutputStream, OutputStreamHandle), StreamError> {
        // with ALSA we can get only one output stream at a time, so let's
        // destroy the old one first
        // (on Mac, it seems we can obtain several output streams at once)
        self.output_stream = None;
        match device {
            Some(device) => OutputStream::try_from_device(&device),
            None => OutputStream::try_default(),
        }
    }

    fn play_track(&mut self, source_descriptor: PlaybackSourceDescriptor) {
        self.stop();
        let device = self.get_device();
        match self.get_stream_handle(device) {
            Ok((stream, stream_handle)) => {
                let audio_source = match source_descriptor {
                    PlaybackSourceDescriptor::LibraryTrack(indexes) => {
                        AudioSource::LibrarySource(self.get_source(indexes))
                    }
                    PlaybackSourceDescriptor::ErrorSound => {
                        AudioSource::ErrorSource(self.get_error_sound_decoder())
                    }
                };

                let main_thread_sender = self.main_thread_sender.clone();

                let on_sink_empty = Box::new(move || {
                    main_thread_sender
                        .send(MainThreadEvent::PlayerEvent)
                        .unwrap();
                });

                let sink = Sink::try_new(&stream_handle).unwrap();
                match audio_source {
                    AudioSource::LibrarySource(source) => {
                        sink.append(source);
                    }
                    AudioSource::ErrorSource(source) => {
                        sink.append(source);
                    }
                }

                // append callback source that triggers an event in the 
                // main thread to update the player
                sink.append::<Callback<f32>>(Callback::new(on_sink_empty));
                self.output_stream = Some(stream);
                self.active_sink = Some(sink);

                if let PlaybackSourceDescriptor::LibraryTrack(indexes) = source_descriptor {
                    self.current_indexes = Some(indexes);
                } else {
                    self.current_indexes = None;
                }
            }
            Err(e) => {
                println!("Could not obtain stream: {}", e);
            }
        };
    }

    /* PUBLIC FUNCTIONS */

    pub fn play_album(&mut self, album_index: u8) {
        if album_index < self.library.albums.len() as u8 {
            self.play_track(PlaybackSourceDescriptor::LibraryTrack(Indexes {
                album: album_index,
                track: 0,
            }));
        } else {
            self.play_error();
        }
    }

    pub fn play_error(&mut self) {
        self.play_track(PlaybackSourceDescriptor::ErrorSound);
    }

    pub fn play_pause(&mut self) {
        match &self.active_sink {
            Some(sink) => {
                if sink.is_paused() {
                    sink.play();
                } else {
                    sink.pause();
                }
                return ();
            }
            None => match &self.current_indexes {
                Some(current_indexes) => {
                    self.play_album(current_indexes.album);
                }
                None => {
                    println!("[play_pause] No album selected");
                }
            },
        }
    }

    pub fn previous_track(&mut self) {
        match &self.current_indexes {
            Some(current_indexes) => {
                let there_is_a_track_before = current_indexes.track > 0;

                self.play_track(PlaybackSourceDescriptor::LibraryTrack(Indexes {
                    album: current_indexes.album,
                    track: if there_is_a_track_before {
                        current_indexes.track - 1
                    } else {
                        0
                    },
                }));
            }
            None => {
                println!("[previous_track] No album selected");
            }
        }
    }

    pub fn next_track(&mut self) {
        match &self.current_indexes {
            Some(current_indexes) => {
                let there_is_another_track = current_indexes.track
                    < (self.library.albums[current_indexes.album as usize]
                        .tracks
                        .len()
                        - 1) as u8;

                self.play_track(PlaybackSourceDescriptor::LibraryTrack(Indexes {
                    album: current_indexes.album,
                    track: if there_is_another_track {
                        current_indexes.track + 1
                    } else {
                        0
                    },
                }));
            }
            None => {
                println!("[next_track] No album selected");
            }
        }
    }

    fn handle_sink_thread_finish(&mut self) {
        if self.current_indexes.is_some() {
            self.next_track();
        }
    }

    pub fn stop(&mut self) {
        let old_sink = replace(&mut self.active_sink, None);
        match old_sink {
            Some(old_sink) => {
                old_sink.stop();
                old_sink.detach();
            }
            None => ()
        }

        self.output_stream = None;
    }

    pub fn handle_player_event(&mut self) {
        self.handle_sink_thread_finish()
    }

}
