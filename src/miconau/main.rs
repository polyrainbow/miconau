extern crate midir;
mod args;
mod library;
mod midi_listener;
mod player;
mod utils;
use args::get_args;
use library::Library;
use midi_listener::listen;
use player::Player;
use std::error::Error;
use std::sync::mpsc::{self, TryRecvError};
use std::thread::sleep;
use std::time::Duration;
use utils::*;

static MAIN_LOOP_INTERVAL: Duration = Duration::from_millis(50);

fn main() {
    match run() {
        Ok(_) => (),
        Err(err) => println!("Error: {}", err),
    }
}

fn handle_midi_key_press(received: u8, start_octave: u8, player: &mut Player) {
    if is_white_key(received) {
        let album_index = get_album_index(received, start_octave);

        match album_index {
            Some(album_index) => {
                player.play_album(album_index);
            }
            None => {
                player.play_error();
            }
        }
    }

    // every octave, we want the function keys to
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

fn run() -> Result<(), Box<dyn Error>> {
    let error_sound = include_bytes!("error.wav");
    let args = get_args();

    let library = Library::new(args.library_folder);
    let mut player = Player::new(library, args.output_device, error_sound);

    let (tx, rx) = mpsc::channel::<u8>();

    if args.midi_device_index.is_some() {
        println!(
            "MIDI device index provided as argument: {}",
            args.midi_device_index.unwrap(),
        );
    }

    let midi_connection = listen(tx, args.midi_device_index);

    match midi_connection {
        Ok(_v) => {
            println!("MIDI device present. Listening!");

            loop {
                sleep(MAIN_LOOP_INTERVAL);
                player.loop_routine();
                match rx.try_recv() {
                    Ok(received) => {
                        println!("MIDI key pressed: {}", received);
                        handle_midi_key_press(received, args.start_octave, &mut player);
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(error) => {
                        println!("{:?}", error)
                    }
                }
            }
        }
        Err(_e) => {
            println!("No MIDI device detected.");
            player.play_album(0);
            loop {
                sleep(MAIN_LOOP_INTERVAL);
                player.loop_routine();
            }
        }
    };
}
