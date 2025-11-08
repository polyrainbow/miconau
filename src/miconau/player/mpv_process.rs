use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::io::BufRead;


pub async fn launch_mpv(output_device: Option<String>, socket_path: String) -> Child {
  let mut args = vec![
    "-v".to_string(),
    "--idle".to_string(),
    "--no-video".to_string(),
    "--no-input-default-bindings".to_string(),
    "--no-config".to_string(),
  ];
  
  let mut socket_arg = "--input-ipc-server=".to_owned();
  socket_arg.push_str(&socket_path);
  args.push(socket_arg);

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

  process
}

pub fn terminate(process: &mut Child) -> std::io::Result<()> {
  process.kill()
}
