use rodio::cpal::default_host;
use rodio::cpal::traits::HostTrait;
use rodio::DeviceTrait;

fn main() -> Result<(), ()> {
    let host = default_host();
    let output_devices = host.output_devices().unwrap();
    println!("Available output devices:");
    for (i, device) in output_devices.enumerate() {
        println!("Device {}: {:?}", i, device.name().unwrap());
    }
    return Ok(());
}
