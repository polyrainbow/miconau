extern crate midir;
use midir::{MidiInput, MidiInputConnection};
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::sync::mpsc::Sender;

use crate::MainThreadEvent;

pub fn listen(
    tx: Sender<MainThreadEvent>,
    input_port_index: Option<u8>,
) -> Result<MidiInputConnection<()>, Box<dyn Error>> {
    let midi_in = MidiInput::new("midir reading input").unwrap();

    // Get an input port (read from console if multiple are available)
    let in_ports = midi_in.ports();
    let in_port = match in_ports.len() {
        0 => return Err("no input port found".into()),
        1 => {
            println!(
                "Choosing the only available input port: {}",
                midi_in.port_name(&in_ports[0]).unwrap()
            );
            &in_ports[0]
        }
        _ => match input_port_index {
            Some(input_port_index) => in_ports
                .get(input_port_index as usize)
                .ok_or("invalid input port selected")?,
            None => {
                println!("Available input ports:");
                for (i, p) in in_ports.iter().enumerate() {
                    println!("{}: {}", i, midi_in.port_name(p).unwrap());
                }
                print!("Please select input port: ");
                stdout().flush()?;
                let mut input = String::new();
                stdin().read_line(&mut input)?;
                in_ports
                    .get(input.trim().parse::<usize>()?)
                    .ok_or("invalid input port selected")?
            }
        },
    };

    println!("\nOpening MIDI connection...");
    let in_port_name = midi_in.port_name(in_port)?;

    // conn_in needs to be a named parameter, because it needs to be kept alive until the end of the scope
    let conn_in = midi_in.connect(
        in_port,
        "midir-read-input",
        move |_stamp, message, _| {
            if message[0] == 144 && message[2] > 0 {
                // if it's a noteOn message with velocity higher than 0
                let note = message[1];
                tx.send(MainThreadEvent::MIDIEvent(note)).unwrap();
            }
        },
        (),
    )?;

    println!(
        "MIDI connection open, reading input from '{}'",
        in_port_name
    );
    return Ok(conn_in);
}
