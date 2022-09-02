use std::sync::mpsc::{self, TryRecvError, Sender};
use std::time::Duration;
use std::{cmp, thread};
use std::thread::{sleep, JoinHandle};
use std::{fs::File};
use std::io::BufReader;
use rodio::{Decoder, OutputStream, Sink, Device, OutputStreamHandle, StreamError};
use rodio::cpal::default_host;
use crate::library::{Library};
use rodio::cpal::traits::HostTrait;
use rodio::DeviceTrait;


pub struct CurrentIndexes {
    pub album: u8,
    pub track: u8,
}

// we need to keep a reference to all those rodio interfaces because otherwise
// playback would be dropped
pub struct Player {
    pub output_device_name: Option<String>,
    pub output_stream: Option<OutputStream>,
    pub sink_transmitter: Option<Sender<u8>>,
    pub library: Library,
    pub current_indexes: Option<CurrentIndexes>,
    pub sink_thread_handle: Option<JoinHandle<()>>
}

impl Player {
    pub fn new(library: Library, output_device_name: Option<String>) -> Player {
        return Player {
            output_device_name,
            sink_transmitter: None,
            output_stream: None,
            library,
            current_indexes: None,
            sink_thread_handle: None,
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


    fn get_source(&mut self, album_index: u8, track_index: u8) -> Decoder<BufReader<File>> {
        let album = &self.library.albums[album_index as usize];
        let track = &album.tracks[track_index as usize];
        let filename = &track.filename;
        println!("Playing {}-{}: {:?}", album_index, track_index, filename);
        // Load a sound from a file, using a path relative to Cargo.toml
        let file = BufReader::new(File::open(&filename).unwrap());
        // Decode that sound file into a source
        let source = Decoder::new(file).unwrap();
        return source;
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


    fn play_track(&mut self, album_index: u8, track_index: u8) {
        self.stop();
        let device = self.get_device();
        match self.get_stream_handle(device) {
            Ok((stream, stream_handle)) => {
                let source = self.get_source(album_index, track_index);
                let (tx, rx) = mpsc::channel::<u8>();
                let join_handle = thread::spawn(move || {
                    let sink = Sink::try_new(&stream_handle).unwrap();
                    sink.append(source);
        
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
                self.current_indexes = Some(CurrentIndexes {
                    album: album_index,
                    track: track_index,
                });
            }
            Err(e) => {
                println!("Could not obtain stream: {}", e);
            }
        };
    }


    pub fn play_album(&mut self, album_index: u8) {
        self.play_track(cmp::min(album_index, (self.library.albums.len() - 1) as u8), 0);
    }


    pub fn play_pause(&mut self) {
        match &self.sink_transmitter {
            Some(sink_transmitter) => {
                sink_transmitter.send(1).unwrap();
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

                if there_is_a_track_before {
                    self.play_track(current_indexes.album, current_indexes.track - 1);
                } else {
                    let new_track = self.library.albums[current_indexes.album as usize].tracks.len() - 1;
                    self.play_track(current_indexes.album, new_track as u8);
                }
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

                if there_is_another_track {
                    self.play_track(current_indexes.album, current_indexes.track + 1);
                } else {
                    self.play_track(current_indexes.album, 0);
                }
            }
            None => {
                println!("[next_track] No album selected");
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

        self.sink_thread_handle = None;
        self.sink_transmitter = None;
        self.output_stream = None;

        // wait until sink thread is terminated
        sleep(Duration::from_millis(100));
    }

}



