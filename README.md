# Miconau (MIDI controlled audio player)

A basic audio player that is controlled via MIDI note-on events. Plays most common audio formats (mp3, flac, etc.).
On startup, the application will scan your audio library and assign every album to a white key on the keyboard.
You can control the audio with the black keys.

## Usage

```
cargo run --bin miconau -- --library-folder [PATH_TO_LIBRARY] --midi-device-index [MIDI_INPUT_DEVICE_INDEX] --output-device [AUDIO_OUTPUT_DEVICE_NAME] --start-octave [START_OCTAVE]
```
Example: 
```
cargo run --bin miconau -- --library-folder /mnt/usb1/Music --midi-device-index 1 --output-device plughw:CARD=Audio,DEV=0 --start-octave 4
```

## List available audio devices

```
cargo run --bin list-devices
```

## Key bindings

![Key bindings](./assets/keys.jpg)

- All white keys starting from middle C: play album 1-n
- D#: Stop
- F#: Previous track in album
- G#: Play/pause
- A#: Next track in album


