extern crate midir;
mod args;
mod library;
mod midi_listener;
mod player;
mod utils;
mod callback_source;
use args::get_args;
use library::Library;
use midi_listener::listen;
use player::Player;
use std::error::Error;
use std::sync::mpsc::{self};
use utils::*;

pub enum MainThreadEvent {
    MIDIEvent(u8),
    PlayerEvent,
}

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
    let (main_thread_sender, rx) = mpsc::channel::<MainThreadEvent>();
    let tx_for_player = main_thread_sender.clone();
    let tx_for_midi_listener = main_thread_sender;

    // we'll pass a sender to player so that it can use the main event loop
    // to update its state
    let mut player = Player::new(
        library,
        args.output_device,
        error_sound,
        tx_for_player,
    );

    if args.midi_device_index.is_some() {
        println!(
            "MIDI device index provided via CLI argument: {}",
            args.midi_device_index.unwrap(),
        );
    }

    let midi_connection = listen(
        tx_for_midi_listener,
        args.midi_device_index,
    );

    if midi_connection.is_err() {
        println!("No MIDI device detected. Playing first album.");
        player.play_album(0);
    }

    loop {
        match rx.recv() {
            Ok(MainThreadEvent::MIDIEvent(received)) => {
                println!("MIDI key pressed: {}", received);
                handle_midi_key_press(received, args.start_octave, &mut player);
            }
            Ok(MainThreadEvent::PlayerEvent) => {
                player.handle_player_event();
            }
            Err(error) => {
                println!("{:?}", error)
            }
        }
    }
}
