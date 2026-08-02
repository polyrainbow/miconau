# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A MIDI-controlled audio player: white keys play albums/streams, black keys are transport controls. mpv does the actual playback, driven over its JSON IPC socket. An optional web UI exposes the same controls plus a queue and playlist upload. Linux/macOS only (Windows unsupported).

## Commands

```bash
cargo build
cargo test                                   # 37 unit tests, all inline #[cfg(test)] modules
cargo test library::tests::skips_hidden_files_and_folders   # single test
cargo test library::                         # one module's tests
cargo run --bin miconau -- --library-folder <lib> --start-octave 4 \
    --address 127.0.0.1:8899 --mpv-socket /tmp/mic-test.sock
```

`mpv` must be on PATH. No MIDI device is needed to run: the main thread parks and the web UI still works. `--mpv-socket` must be a **short** path — a long one (e.g. inside a scratchpad dir) panics with `ConnectError("path must be shorter than SUN_LEN")`.

Generate a test library with tagged files:
```bash
ffmpeg -f lavfi -i "anullsrc=r=44100:cl=mono" -t 1 -metadata artist=X -metadata title=Y out.mp3
```

## Architecture

Everything hangs off one `Arc<Mutex<Player>>` shared by four concurrent parts:

- **main thread** (`main.rs`) — blocking `mpsc` loop receiving `MainThreadEvent::MIDIEvent`, dispatched through `utils::handle_midi_key_press`.
- **MIDI callback** (`midi_listener/`) — midir thread, forwards note-on messages into the channel.
- **library scan** (`spawn_library_scan` in `main.rs`) — a plain OS thread, not a tokio task, because the scan is long and fully blocking. Calls `library::scan_playlists` with a callback that takes the lock per playlist and inserts it via `Library::insert_playlist`, so the library is progressively playable while scanning. Notifications to the web UI are coalesced to one every `SCAN_NOTIFY_INTERVAL`.
- **mpv event listener** (`spawn_mpv_event_listener` in `player/`) — second mpv IPC connection on its own thread; on `StartFile` it calls `Player::on_track_started`, which spins on `try_lock`.
- **axum server** (`web.rs`) — tokio tasks, `/api/*` routes plus static files.

The player never blocks the lock on disk I/O. Anything that reads files while serving (cover art, upload rescan) clones the path out of the library, drops the lock, and does the read in `spawn_blocking`.

### Key invariants

**Source indices.** White keys map to a single flat index space: streams first, then playlists (`utils::resolve_source`). Streams are loaded *before* the scan starts so playlist keys don't shift as playlists arrive. These counts are `usize` deliberately — a `u8` truncated libraries over 255 sources; there's a regression test for it.

**Playlist index stability.** `GET /api/playlists?filter=` numbers playlists *before* filtering, so an index always addresses the same playlist regardless of the filter. Filtering runs server-side because the track titles it searches are only fetched into the browser one playlist at a time.

**Queue vs. mpv playlist.** `Player::queue` is a UI mirror of what mpv has appended after the current position; `remove_from_queue`/`clear_queue` translate queue indices to mpv playlist indices via `playlist-pos + 1 + i`. `on_track_started` decides direction by comparing mpv's `path` property against the head of the queue rather than by position — going back and playing on are indistinguishable from position alone.

**Playlist identity.** A playlist is a *folder that directly contains audio files*; its title is the path relative to the library root (`Artist/Album`), which is what makes `Library::find_track` a folder lookup rather than a full scan. Only `.mp3` and `.flac`, case-insensitive; dotfiles skipped.

**Cover art.** The scan records only `has_cover_art` and a `cover_source` path — never the image bytes, which would be gigabytes for a large library. `read_cover_art` re-reads the file when a cover is actually served.

**mpv stdout must keep being drained** (`player/mpv_process.rs`). Dropping the reader after startup closes the pipe; mpv (started with `-v`) writes more, gets SIGPIPE, and dies mid-IPC-handshake surfacing as `ConnectionReset`. A thread reads it for the process lifetime.

### Path resolution quirk

`assets/error.wav` and `src/miconau/static/` are located by popping three components off `current_exe()`, i.e. they assume the binary sits at `target/<profile>/miconau` inside the repo. Running the binary from anywhere else breaks the error sound and the web UI's static files.

## Frontend

`src/miconau/static/` — vanilla JS, no build step, served directly from the source tree with caching disabled. State arrives over SSE at `/api/notifications` (`playerState`, `libraryUpdated`, `queueUpdated`); everything else is fetch against `/api`. `libraryUpdated` triggers a full playlist reload, so `updatePlaylistIndex` re-points already-expanded rows instead of collapsing them.

## Library folder conventions

- `streams.txt` in the library root: blocks separated by blank lines, each `name` / `url` / optional `logo.svg` filename on its own line.
- `logos/` in the library root holds the SVGs referenced by `streams.txt`.
