# Miconau (MIDI controlled audio player)

A audio player that is controlled via MIDI note-on events.
On startup, the application will scan your audio library and assign every album to a white key on the keyboard.
You can control the audio with the black keys. Uses mpv under the hood which can play a lot of audio file and stream formats.

## Usage
Make sure mpv is installed and in PATH. Windows is not supported.
Make sure, `error.wav` is in the same folder as the executable `miconau`.

```
cargo run --bin miconau -- --library-folder [PATH_TO_LIBRARY] --midi-device-index [MIDI_INPUT_DEVICE_INDEX] --start-octave [START_OCTAVE] --output-device [AUDIO_OUTPUT_DEVICE]
```
Example: 
```
cargo run --bin miconau -- --library-folder /mnt/usb1/Music --midi-device-index 1 --start-octave 4 --output-device alsa/plughw:CARD=Audio,DEV=0
```

Add a `streams.txt` file in the library folder with a line-separated list of
stream urls. These streams are then assigned to the lowest white keys.

## List available audio devices

Use mpv to list available audio devices:

```
mpv --audio-device=help
```

## Key bindings

![Key bindings](./assets/keys.jpg)

- All white keys starting from middle C: play album 1-n
- D#: Stop
- F#: Previous track in album
- G#: Play/pause
- A#: Next track in album


