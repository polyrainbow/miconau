extern crate midir;
mod args;
mod library;
mod midi_listener;
mod player;
mod utils;
mod web;
use args::get_args;
use library::Library;
use midi_listener::listen;
use player::Player;
use std::error::Error;
use std::process::exit;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, park};
use utils::*;
use signal_hook::{consts::SIGINT, consts::SIGTERM, iterator::Signals};
use actix_rt;

pub enum MainThreadEvent {
    MIDIEvent(u8),
}

fn main() {
    match run() {
        Ok(_) => (),
        Err(err) => println!("Error: {}", err),
    }
}


fn run() -> Result<(), Box<dyn Error>> {
    let args = get_args();

    let library = Library::new(args.library_folder);
    let (main_thread_sender, rx) = mpsc::channel::<MainThreadEvent>();

    let player = Arc::new(Mutex::new(Player::new(library, args.output_device)));
    println!("Player module initialized");

    if args.address.is_some() {
        let address = args.address.unwrap();
        println!("Starting webserver on {}", address);
        // Start web server in a separate thread
        let player_for_web = player.clone();
        std::thread::spawn(move || {
            let web_server = web::WebServer::new(player_for_web, address);
            actix_rt::System::new().block_on(async move {
                if let Err(e) = web_server.start().await {
                    eprintln!("Web server error: {}", e);
                }
            });
        });
    } else {
        println!("Web server disabled");
    }

    if args.midi_device_index.is_some() {
        println!(
            "MIDI device index provided via CLI argument: {}",
            args.midi_device_index.unwrap(),
        );
    }

    let midi_connection = listen(main_thread_sender, args.midi_device_index);

    let mut signals = Signals::new([SIGINT, SIGTERM])?;
    let player_for_interrupt_thread = player.clone();

    let current_thread = thread::current();

    thread::spawn(move || {
        for sig in signals.forever() {
            println!("Received signal {:?}", sig);
            let mut player
                = player_for_interrupt_thread.lock().unwrap();
            player.destroy().unwrap();
            current_thread.unpark();
            println!("Exiting...");
            exit(0);
        }
    });

    if midi_connection.is_err() {
        println!("No MIDI device detected.");
        park();
        Ok(())
    } else {
        println!("MIDI device detected. Listening for MIDI events.");

        loop {
            match rx.recv() {
                Ok(MainThreadEvent::MIDIEvent(received)) => {
                    println!("MIDI key pressed: {}", received);
                    let mut player = player.lock().unwrap();
                    handle_midi_key_press(received, args.start_octave, &mut player);
                }
                Err(error) => {
                    println!("{:?}", error);
                    let mut player = player.lock().unwrap();
                    player.destroy().unwrap();
                    exit(1);
                }
            }
        }
    }
}