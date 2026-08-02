use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::io::BufRead;
use std::thread;


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

  let mut lines = BufReader::new(stdout).lines();
  /* it waits for new output */
  for line in &mut lines {
      let output = match line {
        Ok(output) => output,
        Err(error) => {
          println!("MPV: could not read output: {}", error);
          break;
        }
      };
      println!("MPV: {}", output);
      if output.contains("Done loading scripts.") {
        println!("MPV process created");
        break;
      }
  }

  // Keep reading for as long as mpv runs, rather than dropping the reader here.
  //
  // Dropping it closes the read end of the pipe, and mpv is not finished
  // talking: it was started with -v and logs its way through audio device
  // setup for a while yet. The next line it writes then goes to a pipe nobody
  // holds open, and mpv is killed by SIGPIPE - Rust ignores that signal in this
  // process but resets it to its default in children, so mpv gets the fatal
  // one. Losing mpv that early usually means losing it mid-handshake, while the
  // first IPC command is in flight: mpv dies with the command still unread, the
  // kernel resets the socket, and the reply we are waiting for comes back as
  // ConnectionReset. Under load that was most starts, which is why it showed up
  // as a crash at boot and almost never by hand.
  //
  // Draining in a thread also puts mpv's log back in the journal, where it has
  // been missing after startup for as long as the reader was dropped.
  thread::spawn(move || {
    for line in lines {
      match line {
        Ok(output) => println!("MPV: {}", output),
        // mpv has exited and closed its end. Nothing left to read.
        Err(_) => break,
      }
    }
  });

  process
}

pub fn terminate(process: &mut Child) -> std::io::Result<()> {
  process.kill()
}
