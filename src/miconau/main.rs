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
use std::process::exit;
use std::sync::mpsc::{self};
use utils::*;
use ctrlc;

pub enum MainThreadEvent {
    MIDIEvent(u8),
    PlayerEvent,
    InterruptEvent,
}

fn main() {
    match run() {
        Ok(_) => (),
        Err(err) => println!("Error: {}", err),
    }
}

fn handle_midi_key_press(received: u8, start_octave: u8, player: &mut Player) {
    if is_white_key(received) {
        let source_index = get_source_index(received, start_octave);

        match source_index {
            Some(source_index) => {
                println!("Source index: {}", source_index);
                let n_streams = player.library.streams.len() as u8;
                let n_playlists = player.library.playlists.len() as u8;
                if source_index < n_streams {
                    player.play_stream(source_index);
                } else if source_index < (n_streams + n_playlists) {
                    let playlist_index = source_index - n_streams;
                    player.play_playlist(playlist_index);
                } else {
                    player.play_error();
                }
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
        player.play_previous_track();
    }

    if received_within_octave == 8 {
        player.play_pause();
    }

    if received_within_octave == 10 {
        player.play_next_track();
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = get_args();

    let library = Library::new(args.library_folder);
    let (main_thread_sender, rx) = mpsc::channel::<MainThreadEvent>();
    let tx_for_interrupt_listener = main_thread_sender.clone();
    let tx_for_midi_listener = main_thread_sender;

    let mut player = Player::new(library, args.output_device);
    println!("Player module initialized");

    ctrlc::set_handler(move || { 
        println!("CTRL+C");
        tx_for_interrupt_listener.send(MainThreadEvent::InterruptEvent).unwrap();
        exit(0);
    })
        .expect("Error setting Ctrl-C handler");


    if args.midi_device_index.is_some() {
        println!(
            "MIDI device index provided via CLI argument: {}",
            args.midi_device_index.unwrap(),
        );
    }

    let midi_connection = listen(tx_for_midi_listener, args.midi_device_index);

    if midi_connection.is_err() {
        println!("No MIDI device detected. Playing first playlist from library.");
        player.play_playlist(0);
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
            Ok(MainThreadEvent::InterruptEvent) => {
                player.destroy().unwrap();
            }
            Err(error) => {
                println!("{:?}", error)
            }
        }
    }
}