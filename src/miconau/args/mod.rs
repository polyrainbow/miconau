extern crate clap;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    pub library_folder: String,

    /// Folder holding `streams.txt` and the `logos/` it refers to. Streams are
    /// unrelated to the music library, so they live wherever the user keeps
    /// their config. Without this argument there are simply no streams and the
    /// white keys start at the first playlist.
    #[arg(long)]
    pub streams_folder: Option<String>,

    #[arg(short, long)]
    pub output_device: Option<String>,

    #[arg(short, long)]
    pub midi_device_index: Option<u8>,

    #[arg(short, long)]
    pub start_octave: u8,

    #[arg(short, long)]
    pub address: Option<String>,

    #[arg(long, default_value = "/tmp/mpvsocket")]
    pub mpv_socket: String,
}

pub fn get_args() -> Args {
    Args::parse()
}
