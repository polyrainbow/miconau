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
use std::time::{Duration, Instant};
use utils::*;
use signal_hook::{consts::SIGINT, consts::SIGTERM, iterator::Signals};

pub enum MainThreadEvent {
    MIDIEvent(u8),
}

/// How often the web UI is told about newly found playlists while a scan is
/// running. Each notification makes it reload the whole library, so they are
/// coalesced rather than sent per playlist.
const SCAN_NOTIFY_INTERVAL: Duration = Duration::from_secs(2);

/// Scans the library in the background, adding each playlist to the player as
/// soon as it is found. Everything scanned so far is immediately playable, both
/// by MIDI key and from the web UI, while the rest is still being read.
///
/// This is a plain OS thread rather than a tokio task: the scan is long and
/// fully blocking, and main parks its own thread when no MIDI device is found.
fn spawn_library_scan(library_folder: String, player: Arc<Mutex<Player>>) {
    thread::spawn(move || {
        // Streams first. They are cheap to read and occupy the white keys
        // below the playlists, so loading them up front keeps every playlist
        // key from shifting once the first playlist arrives.
        let streams = library::read_streams(&library_folder);
        {
            let mut player = player.blocking_lock();
            player.library.streams = streams;
            player.notify_library_updated();
        }

        let mut last_notification = Instant::now();
        library::scan_playlists(&library_folder, &mut |playlist| {
            // The lock is only held for the insert, never for the file reads,
            // so playback and the web server stay responsive throughout.
            let mut player = player.blocking_lock();
            player.library.insert_playlist(playlist);
            if last_notification.elapsed() >= SCAN_NOTIFY_INTERVAL {
                last_notification = Instant::now();
                player.notify_library_updated();
            }
        });

        let player = player.blocking_lock();
        player.library.log_playlists();
        player.notify_library_updated();
        println!("Library is ready.");
    });
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = get_args();
    let main_thread = thread::current();

    // Start out with an empty library so mpv, the web server and MIDI come up
    // immediately. Scanning a large library takes minutes and would otherwise
    // block all of it.
    let library = Library::empty(args.library_folder.clone());
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

    spawn_library_scan(args.library_folder, player.clone());

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