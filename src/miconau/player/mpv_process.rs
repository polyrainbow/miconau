use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::thread::{self};
use std::io::BufRead;


pub fn launch_mpv() -> Child {
  let mut process = Command::new("mpv")
        .arg("-v")
        .arg("--idle")
        .arg("--no-video")
        .arg("--no-input-default-bindings")
        .arg("--no-config")
        .arg("--input-ipc-server=/tmp/mpvsocket")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

        let stdout = process.stdout.take().unwrap();

    let thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        /* it waits for new output */
        for line in reader.lines() {
            let output = line.unwrap();
            println!("MPV: {}", output);
            if output.contains("Done loading scripts.") {
              println!("MPV process created");
              break;
            }
        }
    });

    thread.join().unwrap();
    process
}

pub fn terminate(process: &mut Child) -> std::io::Result<()> {
  process.kill()
}
