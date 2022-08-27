extern crate midir;
mod player;
mod library;
mod midi_listener;
use std::env;
use std::io::{stdin, stdout, Write};
use std::error::Error;
use std::sync::mpsc::{self, TryRecvError};
use std::thread::sleep;
use std::time::Duration;
use rodio::DeviceTrait;
use rodio::cpal::default_host;
use rodio::cpal::traits::HostTrait;

use player::Player;
use library::Library;
use midi_listener::listen;

static WHITE_KEYS:[u8; 7] = [0, 2, 4, 5, 7, 9, 11];
static ALBUM_START_KEY_OFFSET: u8 = 48;

fn main() {
    match run() {
        Ok(_) => (),
        Err(err) => println!("Error: {}", err)
    }
}

fn is_white_key(key: u8) -> bool {
    return WHITE_KEYS.contains(&(key % 12));
}


fn get_album_index(key: u8) -> u8 {
    if key <= ALBUM_START_KEY_OFFSET {
        return 0;
    }

    let octave = (key - ALBUM_START_KEY_OFFSET) / 12;
    let index_within_octave = WHITE_KEYS.iter().position(|&x| x == key % 12).unwrap() as u8;
    let index = (octave * WHITE_KEYS.len() as u8) + index_within_octave;
    return index;
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    

    let library_arg = args.iter()
        .find(|&x| x.starts_with("library="));

    let library_folder = match library_arg {
        Some(library_arg) => {
            let library_folder = library_arg.clone().chars().skip(8).collect();
            println!("Library folder provided by argument: {:?}", library_folder);
            library_folder
        }
        None => {
            let library_folder = String::from("audio");
            println!("No library folder argument provided. Taking default: audio");
            library_folder
        }
    };

    let host = default_host();
    let output_devices = host.output_devices().unwrap();

    let output_device_arg = args.iter()
        .find(|&x| x.starts_with("output_device="));

    let active_device = match output_device_arg {
        Some(device_arg) => {
            let device_name:String = device_arg.clone().chars().skip(14).collect();
            println!("Device name provided by argument: {:?}", device_name);

            host.output_devices().unwrap()
                .filter(|dev| dev.name().unwrap() == device_name )
                .next().ok_or("Device provided by argument is unknown!").unwrap()
        }
        None => {
            println!("Available output devices:");
            for (i, device) in output_devices.enumerate() {
                println!("Device {}: {:?}", i, device.name().unwrap());
            }
        
            print!("Please select output device: ");
            stdout().flush()?;
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            let index = input.trim().parse::<usize>()?;

            host.output_devices().unwrap()
                .enumerate()
                .filter(|&(i, _)| i == index )
                .map(|(_, e)| e)
                .next().unwrap()
        }
    };


    let library = Library::new(library_folder);
    let mut player = Player::new(library, &active_device);

    let (tx, rx) = mpsc::channel::<u8>();

    let midi_input_device_arg = args.iter()
        .find(|&x| x.starts_with("midi_device_index="));

    let midi_connection = match midi_input_device_arg {
        Some(midi_input_device_arg) => {
            let index_as_string:String = midi_input_device_arg.clone()
                .chars().skip(18).take(1).collect();
            let index = index_as_string.parse::<u8>().unwrap();
            println!("MIDI device index provided as argument: {}", index);
            listen(tx, Some(index))
        }
        None => {
            listen(tx, None)
        }
    };

    match midi_connection {
        Ok(_v) => {
            println!("MIDI device present. Listening!");

            loop {
                sleep(Duration::from_millis(200));
                match &player.sink_thread_handle {
                    Some(thread_handle) => {
                        if thread_handle.is_finished() {
                            let _ = &player.next_track();
                        }
                    }
                    None => {}
                }
                match rx.try_recv() {
                    Ok(received) => {
                        println!("MIDI key pressed: {}", received);
                        if is_white_key(received) {
                            let album_index = get_album_index(received);
                            player.play_album(album_index);
                        }

                        // every octave, we want to function keys to
                        // repeat, so let's do % 12 everywhere
                        let received_within_octave = received % 12;
                

                        if received_within_octave == 1 {
                            player.stop();
                        }

                        if received_within_octave == 3 {
                            player.stop();
                        }
        
                        if received_within_octave == 6 {
                            player.previous_track();
                        }
        
                        if received_within_octave == 8 {
                            player.play_pause();
                        }
                
                        if received_within_octave == 10 {
                            player.next_track();
                        }
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(error) => {
                        println!("{:?}", error)
                    }
                }
        
            }
        },
        Err(_e) => {
            println!("No MIDI device detected. Just starting to play the first album!");
            player.play_album(0);
            let mut input = String::new();
            input.clear();
            stdin().read_line(&mut input)?; // wait for next enter key press
        }
    }

    Ok(())
}