extern crate midir;
mod player;
mod library;
mod midi_listener;
use std::env;
use std::io::{stdin};
use std::error::Error;
use std::sync::mpsc::{self, TryRecvError};
use std::thread::sleep;
use std::time::Duration;
use rodio::DeviceTrait;
use rodio::cpal::default_host;
use rodio::cpal::traits::HostTrait;
use std::num::NonZeroU8;

use player::Player;
use library::Library;
use midi_listener::listen;

static WHITE_KEYS:[u8; 7] = [0, 2, 4, 5, 7, 9, 11];
static START_OCTAVE: u8 = 2;

fn main() {
    match run() {
        Ok(_) => (),
        Err(err) => println!("Error: {}", err)
    }
}

// https://www.inspiredacoustics.com/en/MIDI_note_numbers_and_center_frequencies
fn is_white_key(key: u8) -> bool {
    return WHITE_KEYS.contains(&(key % 12));
}


fn get_album_index(key: u8, start_octave: NonZeroU8) -> Option<u8> {
    let octave = key / 12;

    let index_within_octave = WHITE_KEYS.iter()
        .position(|&x| x == (key) % 12);

    match index_within_octave {
        Some(index_within_octave) => {
            let album_index
                = (
                    octave * WHITE_KEYS.len() as u8
                    + index_within_octave as u8
                ).saturating_sub(u8::from(start_octave) * WHITE_KEYS.len() as u8);

            Some(album_index)
        }
        None => {
            None
        }
    }

}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();

    if args.contains(&String::from("list-devices")) {
        let host = default_host();
        let output_devices = host.output_devices().unwrap();
        println!("Available output devices:");
        for (i, device) in output_devices.enumerate() {
            println!("Device {}: {:?}", i, device.name().unwrap());
        }
        return Ok(());
    };

    let output_device_arg = args.iter()
        .find(|&x| x.starts_with("output_device="));

    let output_device_name = match output_device_arg {
        Some(output_device_arg)  => {
            let device_name:String = output_device_arg.clone().chars().skip(14).collect();
            println!("Device name provided by argument: {:?}", device_name);
            Some(device_name)
        }
        None => {
            None
        }
    };

    let library_arg = args.iter()
        .find(|&x| x.starts_with("library="));

    let library_folder = match library_arg {
        Some(library_arg) => {
            let library_folder = library_arg.clone().chars().skip(8).collect();
            println!("Library folder provided by argument: {:?}", library_folder);
            library_folder
        }
        None => {
            println!("Please provide a library folder.");
            return Ok(())
        }
    };


    let library = Library::new(library_folder);
    let mut player = Player::new(library, output_device_name);

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
                            let album_index = get_album_index(
                                received,
                                NonZeroU8::new(START_OCTAVE).unwrap(),
                            ).unwrap();
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



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_white_key_works() {
        assert!(is_white_key(48)); // C
        assert!(!is_white_key(49)); // C#
        assert!(is_white_key(50)); // D
        assert!(!is_white_key(51)); // D#
        assert!(is_white_key(52)); // E
        assert!(is_white_key(53)); // F
        assert!(!is_white_key(54)); // F#
        assert!(is_white_key(55)); // G
        assert!(!is_white_key(56)); // G#
        assert!(is_white_key(57)); // A
        assert!(!is_white_key(58)); // Bb
        assert!(is_white_key(59)); // B
        assert!(is_white_key(60)); // C
    }

    #[test]
    fn get_album_index_works() {
        // low key with high offset octave, album index is always 0
        assert_eq!(get_album_index(21, NonZeroU8::new(10).unwrap()).unwrap(), 0); // A
        assert!(get_album_index(22, NonZeroU8::new(10).unwrap()).is_none()); // Bb
        assert_eq!(get_album_index(23, NonZeroU8::new(10).unwrap()).unwrap(), 0); // B
        assert_eq!(get_album_index(24, NonZeroU8::new(10).unwrap()).unwrap(), 0); // C
        assert!(get_album_index(25, NonZeroU8::new(10).unwrap()).is_none()); // C#
        assert_eq!(get_album_index(26, NonZeroU8::new(10).unwrap()).unwrap(), 0); // D
        assert!(get_album_index(27, NonZeroU8::new(10).unwrap()).is_none()); // D#
        assert_eq!(get_album_index(28, NonZeroU8::new(10).unwrap()).unwrap(), 0); // E

        // octave offset = 1
        assert_eq!(get_album_index(12, NonZeroU8::new(1).unwrap()).unwrap(), 0); // C
        assert!(get_album_index(13, NonZeroU8::new(1).unwrap()).is_none()); // C#
        assert_eq!(get_album_index(14, NonZeroU8::new(1).unwrap()).unwrap(), 1); // D
        assert!(get_album_index(15, NonZeroU8::new(1).unwrap()).is_none()); // D#
        assert_eq!(get_album_index(16, NonZeroU8::new(1).unwrap()).unwrap(), 2); // E

        // octave offset = 2
        assert_eq!(get_album_index(24, NonZeroU8::new(2).unwrap()).unwrap(), 0); // C
        assert!(get_album_index(25, NonZeroU8::new(2).unwrap()).is_none()); // C#
        assert_eq!(get_album_index(26, NonZeroU8::new(2).unwrap()).unwrap(), 1); // D
        assert!(get_album_index(27, NonZeroU8::new(2).unwrap()).is_none()); // D#
        assert_eq!(get_album_index(28, NonZeroU8::new(2).unwrap()).unwrap(), 2); // E

        assert_eq!(get_album_index(36, NonZeroU8::new(2).unwrap()).unwrap(), 7); // Higher C

    }
}