use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::thread::{self};
use std::io::BufRead;


pub fn launch_mpv(output_device: Option<String>) -> Child {
  let mut args = vec![
    "-v".to_string(),
    "--idle".to_string(),
    "--no-video".to_string(),
    "--no-input-default-bindings".to_string(),
    "--no-config".to_string(),
    "--input-ipc-server=/tmp/mpvsocket".to_string()
  ];

  if output_device.is_some() {
    let output_device_str = output_device.unwrap();
    println!("Using output device {}", output_device_str);
    let mut arg: String = "--audio-device=".to_owned();
    arg.push_str(&output_device_str);
    args.push(arg.clone());
  } else {
    println!("No output device provided. MPV will use default one.");
  }

  let mut command = Command::new("mpv");
  command.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
     
  let mut process = command.spawn()
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
