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
use player::spawn_mpv_event_listener;
use tokio::spawn;
use tokio::sync::Mutex;
use std::error::Error;
use std::process::exit;
use std::sync::{mpsc, Arc};
use std::thread::{self, park};
use utils::*;
use signal_hook::{consts::SIGINT, consts::SIGTERM, iterator::Signals};

pub enum MainThreadEvent {
    MIDIEvent(u8),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = get_args();
    let main_thread = thread::current();

    let library = Library::new(args.library_folder);
    let (
        main_thread_sender,
        rx
    ) = mpsc::channel::<MainThreadEvent>();

    let socket_path = args.mpv_socket.clone();
    let player = Arc::new(
        Mutex::new(
            Player::new(library, args.output_device, args.mpv_socket).await
        )
    );
    println!("Player module initialized");

    // Spawn mpv event listener to sync queue when tracks advance
    spawn_mpv_event_listener(socket_path, player.clone());

    if args.address.is_some() {
        let address = args.address.unwrap();
        println!("Starting webserver on {}", address);
        // Start web server in a separate thread
        let player_for_web = player.clone();

        spawn(async move {
            let _ = web::start_server(
                player_for_web,
                address,
            ).await;
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

    spawn(async move {
        for sig in signals.forever() {
            println!("Received signal {:?}", sig);
            let mut player
                = player_for_interrupt_thread.lock().await;
            player.destroy().unwrap();
            main_thread.unpark();
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
                    let mut player = player.lock().await;
                    handle_midi_key_press(received, args.start_octave, &mut player);
                }
                Err(error) => {
                    println!("{:?}", error);
                    let mut player = player.lock().await;
                    player.destroy().unwrap();
                    exit(1);
                }
            }
        }
    }
}