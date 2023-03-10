use midir::{Ignore, MidiInput};
use rodio::cpal::default_host;
use rodio::cpal::traits::HostTrait;
use rodio::DeviceTrait;

fn list_midi_devices() {
    let mut midi_in = MidiInput::new("midir reading input").unwrap();
    midi_in.ignore(Ignore::None);

    // Get an input port (read from console if multiple are available)
    let in_ports = midi_in.ports();

    println!("Available MIDI input ports:");
    for (i, p) in in_ports.iter().enumerate() {
        println!("Port {}: {}", i + 1, midi_in.port_name(p).unwrap());
    }
}

fn list_audio_devices() {
    let host = default_host();
    let output_devices = host.output_devices().unwrap();
    println!("Available audio output devices:");
    for (i, device) in output_devices.enumerate() {
        println!("Device {}: {:?}", i + 1, device.name().unwrap());
    }
}

fn main() -> Result<(), ()> {
    list_audio_devices();
    println!();
    list_midi_devices();
    return Ok(());
}
