extern crate clap;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    pub library_folder: String,

    #[arg(short, long)]
    pub output_device: Option<String>,

    #[arg(short, long)]
    pub midi_device_index: Option<u8>,
}

pub fn get_args() -> Args {
  Args::parse()
}