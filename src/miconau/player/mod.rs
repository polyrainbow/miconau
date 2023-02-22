use std::sync::mpsc::{self, TryRecvError, Sender};
use std::time::Duration;
use std::{thread};
use std::thread::{sleep, JoinHandle};
use std::{fs::File};
use std::io::BufReader;
use rodio::{Decoder, OutputStream, Sink, Device, OutputStreamHandle, StreamError};
use rodio::cpal::default_host;
use crate::library::{Library};
use rodio::cpal::traits::HostTrait;
use rodio::DeviceTrait;
use std::io::Cursor;
use std::mem::replace;

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
    sink_transmitter: Option<Sender<u8>>,
    pub library: Library,
    current_indexes: Option<Indexes>,
    sink_thread_handle: Option<JoinHandle<()>>,
    error_sound: &'static [u8],
}

impl Player {
    pub fn new(
        library: Library,
        output_device_name: Option<String>,
        error_sound: &'static [u8],
    ) -> Player {
        return Player {
            output_device_name,
            sink_transmitter: None,
            output_stream: None,
            library,
            current_indexes: None,
            sink_thread_handle: None,
            error_sound,
        }
    }


    fn get_device(&mut self) -> Option<Device> {
        let host = default_host();

        match &self.output_device_name {
            Some(output_device_name) => {
                let device = host.output_devices().unwrap()
                .filter(|dev| dev.name().unwrap() == output_device_name.to_string() )
                .next();

                match device {
                    Some(device) => {
                        return Some(device)
                    }
                    None => {
                        println!("Device {} provided by argument is unknown or not available!", output_device_name);
                        return None
                    }
                }
            }
            None => {
                None
            }
        }
    }


    fn get_source(&mut self, indexes: Indexes) -> Decoder<BufReader<File>>{
        let album = &self.library.albums[indexes.album as usize];
        let track = &album.tracks[indexes.track as usize];
        let filename = &track.filename;
        println!("Playing {}-{}: {:?}", indexes.album + 1, indexes.track + 1, filename);
        let file = BufReader::new(File::open(&filename).unwrap());
        Decoder::new(file).unwrap()
    }


    fn get_error_sound_decoder(&self) -> Decoder<Cursor<&'static [u8]>> {
        Decoder::new(Cursor::new(self.error_sound)).unwrap()
    }


    fn get_stream_handle(&mut self, device: Option<Device>) -> Result<(OutputStream, OutputStreamHandle), StreamError> {
        // with ALSA we can get only one output stream at a time, so let's
        // destroy the old one first
        // (on Mac, it seems we can obtain several output streams at once)
        self.output_stream = None;
        match device {
            Some(device) => {
                OutputStream::try_from_device(&device)
            }
            None => {
                OutputStream::try_default()
            }
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
                let (tx, rx) = mpsc::channel::<u8>();
                let join_handle = thread::spawn(move || {
                    let sink = Sink::try_new(&stream_handle).unwrap();
                    match audio_source {
                        AudioSource::LibrarySource(source) => {
                            sink.append(source);
                        }
                        AudioSource::ErrorSource(source) => {
                            sink.append(source);
                        }
                    }
                    
        
                    loop {
                        sleep(Duration::from_millis(100));
                        if sink.empty() {
                            return;
                        }
                        match rx.try_recv() {
                            Ok(1) => {
                                if sink.is_paused() {
                                    sink.play();
                                } else {
                                    sink.pause();
                                }
                            }
                            Ok(_) => {
                                break;   
                            }
                            Err(TryRecvError::Disconnected) => {
                                println!("Terminating sink thread because of TryRecvError::Disconnected");
                                break;
                            }
                            Err(TryRecvError::Empty) => {}
                        }
                    }
                });
        
                self.sink_transmitter = Some(tx);
                self.sink_thread_handle = Some(join_handle);
                self.output_stream = Some(stream);

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
        match &self.sink_transmitter {
            Some(sink_transmitter) => {
                match sink_transmitter.send(1) {
                    Ok(_) => (),
                    Err(_) => (),
                };
                return ();
            }
            None => match &self.current_indexes {
                Some(current_indexes) => {
                    self.play_album(current_indexes.album);
                }
                None => {
                    println!("[play_pause] No album selected");
                }
            }
        }
    }


    pub fn previous_track(&mut self) {
        match &self.current_indexes {
            Some(current_indexes) => {
                let there_is_a_track_before
                    = current_indexes.track > 0;

                self.play_track(
                    PlaybackSourceDescriptor::LibraryTrack(
                        Indexes {
                            album: current_indexes.album,
                            track: if there_is_a_track_before {
                                current_indexes.track - 1
                            } else {
                                0
                            }
                        }
                    ),
                );
            }
            None => {
                println!("[previous_track] No album selected");
            }
        }
    }


    pub fn next_track(&mut self) {
        match &self.current_indexes {
            Some(current_indexes) => {
                let there_is_another_track
                    = current_indexes.track < (self.library.albums[current_indexes.album as usize].tracks.len() - 1) as u8;

                self.play_track(
                    PlaybackSourceDescriptor::LibraryTrack(
                        Indexes {
                            album: current_indexes.album,
                            track: if there_is_another_track {
                                current_indexes.track + 1
                            } else {
                                0
                            }
                        }
                    ),
                );
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


    fn is_finished(&self) -> bool {
        match &self.sink_thread_handle {
            Some(thread_handle) => {
                thread_handle.is_finished()
            }
            None => {
                false
            }
        }
    }


    pub fn stop(&mut self) {
        match &self.sink_transmitter {
            Some(sink_transmitter) => {
                match sink_transmitter.send(0) {
                    Ok(_) => (),
                    Err(_) => (),
                }
            }
            None => (),
        }

        if self.sink_thread_handle.is_some() {
            let old_sink_thread_handle = replace(
                &mut self.sink_thread_handle,
                None,
            );
            old_sink_thread_handle.unwrap().join().unwrap();
        }

        self.sink_transmitter = None;
        self.output_stream = None;
    }


    pub fn loop_routine(&mut self) {
        if self.is_finished() {
            self.handle_sink_thread_finish();
        }
    }

}


