use crate::player::Player;
use std::time::Duration;

static WHITE_KEYS: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Formats how long something took, for the log. A big collection on an
/// external drive takes minutes to scan, and "312.4s" is harder to read at a
/// glance than "5m 12s".
pub fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    if total_seconds < 60 {
        return format!("{:.1}s", duration.as_secs_f32());
    }

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{}h {:02}m {:02}s", hours, minutes, seconds)
    } else {
        format!("{}m {:02}s", minutes, seconds)
    }
}

// https://www.inspiredacoustics.com/en/MIDI_note_numbers_and_center_frequencies
pub fn is_white_key(key: u8) -> bool {
    return WHITE_KEYS.contains(&(key % 12));
}

pub fn get_source_index(key: u8, start_octave: u8) -> Option<u8> {
    let octave = key / 12;

    let index_within_octave = WHITE_KEYS.iter().position(|&x| x == (key) % 12);

    match index_within_octave {
        Some(index_within_octave) => {
            let (playlist_index, overflow) = (octave * WHITE_KEYS.len() as u8
                + index_within_octave as u8)
                .overflowing_sub(start_octave * WHITE_KEYS.len() as u8);

            if overflow {
                None
            } else {
                Some(playlist_index)
            }
        }
        None => None,
    }
}


/// What a source index refers to. Streams occupy the white keys below the
/// playlists, so a single index addresses both.
#[derive(Debug, PartialEq)]
pub enum Source {
    Stream(usize),
    Playlist(usize),
}

/// Resolves a source index against the sizes of the library.
///
/// Everything here is `usize` on purpose. The index itself comes from a MIDI
/// note and would fit in a `u8`, but the library can hold far more than the
/// 255 sources a `u8` count can express, and truncating the counts made high
/// indices resolve to the wrong source or to none at all.
pub fn resolve_source(
    source_index: usize,
    n_streams: usize,
    n_playlists: usize,
) -> Option<Source> {
    if source_index < n_streams {
        Some(Source::Stream(source_index))
    } else if source_index < n_streams + n_playlists {
        Some(Source::Playlist(source_index - n_streams))
    } else {
        None
    }
}

pub fn handle_midi_key_press(received: u8, start_octave: u8, player: &mut Player) {
    if is_white_key(received) {
        let source_index = get_source_index(received, start_octave);

        match source_index {
            Some(source_index) => {
                println!("Source index: {}", source_index);
                let source = resolve_source(
                    source_index as usize,
                    player.library.streams.len(),
                    player.library.playlists.len(),
                );
                match source {
                    Some(Source::Stream(stream_index)) => player.play_stream(stream_index),
                    Some(Source::Playlist(playlist_index)) => {
                        player.play_playlist(playlist_index)
                    }
                    None => {
                        println!("Source index out of range. Playing error sound.");
                        player.play_error();
                    }
                }
            }
            None => {
                player.play_error();
            }
        }
    }

    // every octave, we want the function keys to
    // repeat, so let's do % 12 everywhere
    let received_within_octave = received % 12;

    if received_within_octave == 1 {
        player.stop();
    }

    if received_within_octave == 3 {
        player.stop();
    }

    if received_within_octave == 6 {
        player.play_previous_track();
    }

    if received_within_octave == 8 {
        player.play_pause();
    }

    if received_within_octave == 10 {
        player.play_next_track();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_white_key_works() {
        assert!(is_white_key(0)); // lowest possible C

        assert!(is_white_key(48)); // C
        assert!(!is_white_key(49)); // C#
        assert!(is_white_key(50)); // D
        assert!(!is_white_key(51)); // D#
        assert!(is_white_key(52)); // E
        assert!(is_white_key(53)); // F
        assert!(!is_white_key(54)); // F#
        assert!(is_white_key(55)); // G
        assert!(!is_white_key(56)); // G#
        assert!(is_white_key(57)); // A
        assert!(!is_white_key(58)); // Bb
        assert!(is_white_key(59)); // B
        assert!(is_white_key(60)); // C
    }

    #[test]
    fn get_source_index_works() {
        // low key with high offset octave, album index is always 0
        assert!(get_source_index(21, 10).is_none()); // A
        assert!(get_source_index(22, 10).is_none()); // Bb
        assert!(get_source_index(23, 10).is_none()); // B
        assert!(get_source_index(24, 10).is_none()); // C
        assert!(get_source_index(25, 10).is_none()); // C#
        assert!(get_source_index(26, 10).is_none()); // D
        assert!(get_source_index(27, 10).is_none()); // D#
        assert!(get_source_index(28, 10).is_none()); // E

        // octave offset = 1
        assert_eq!(get_source_index(12, 1).unwrap(), 0); // C
        assert!(get_source_index(13, 1).is_none()); // C#
        assert_eq!(get_source_index(14, 1).unwrap(), 1); // D
        assert!(get_source_index(15, 1).is_none()); // D#
        assert_eq!(get_source_index(16, 1).unwrap(), 2); // E

        // octave offset = 2
        assert_eq!(get_source_index(24, 2).unwrap(), 0); // C
        assert!(get_source_index(25, 2).is_none()); // C#
        assert_eq!(get_source_index(26, 2).unwrap(), 1); // D
        assert!(get_source_index(27, 2).is_none()); // D#
        assert_eq!(get_source_index(28, 2).unwrap(), 2); // E

        assert_eq!(get_source_index(36, 2).unwrap(), 7); // Higher C
    }

    #[test]
    fn get_source_index_boundary_cases() {
        // Start of keyboard (octave 0)
        assert_eq!(get_source_index(0, 0).unwrap(), 0); // C0
        assert_eq!(get_source_index(2, 0).unwrap(), 1); // D0
        
        // Crossing octave boundary
        assert_eq!(get_source_index(23, 1).unwrap(), 6); // B1 (7th white key in octave 1)
        assert_eq!(get_source_index(24, 1).unwrap(), 7); // C2 (1st white key in octave 2)
        
        // High octave values
        assert_eq!(get_source_index(60, 5).unwrap(), 0); // C5 with offset 5
        assert_eq!(get_source_index(72, 5).unwrap(), 7); // C6 with offset 5
    }

    #[test]
    fn resolve_source_maps_the_low_keys_to_streams() {
        assert_eq!(resolve_source(0, 3, 10), Some(Source::Stream(0)));
        assert_eq!(resolve_source(2, 3, 10), Some(Source::Stream(2)));
        // First key past the streams is the first playlist.
        assert_eq!(resolve_source(3, 3, 10), Some(Source::Playlist(0)));
        assert_eq!(resolve_source(12, 3, 10), Some(Source::Playlist(9)));
        // One past the last playlist.
        assert_eq!(resolve_source(13, 3, 10), None);
    }

    #[test]
    fn resolve_source_handles_a_library_without_streams() {
        assert_eq!(resolve_source(0, 0, 2), Some(Source::Playlist(0)));
        assert_eq!(resolve_source(1, 0, 2), Some(Source::Playlist(1)));
        assert_eq!(resolve_source(2, 0, 2), None);
        // An empty library resolves nothing at all.
        assert_eq!(resolve_source(0, 0, 0), None);
    }

    /// Regression test: playlist indices used to be `u8`, so a library with
    /// more than 255 sources had both the index and the counts truncated.
    /// Index 398 wrapped to 142, which either played the wrong playlist or
    /// was rejected as not found.
    #[test]
    fn resolve_source_handles_more_sources_than_fit_in_a_u8() {
        assert_eq!(resolve_source(398, 0, 500), Some(Source::Playlist(398)));
        assert_eq!(resolve_source(255, 0, 500), Some(Source::Playlist(255)));
        assert_eq!(resolve_source(256, 0, 500), Some(Source::Playlist(256)));

        // The same with streams below the playlists, so the subtraction is
        // exercised past the u8 boundary too.
        assert_eq!(resolve_source(398, 3, 500), Some(Source::Playlist(395)));
        assert_eq!(resolve_source(300, 300, 300), Some(Source::Playlist(0)));
        assert_eq!(resolve_source(299, 300, 300), Some(Source::Stream(299)));

        // Out of range stays out of range instead of wrapping into it.
        assert_eq!(resolve_source(600, 0, 500), None);
        assert_eq!(resolve_source(500, 0, 500), None);
    }

    #[test]
    fn format_duration_stays_in_seconds_below_a_minute() {
        assert_eq!(format_duration(Duration::from_millis(0)), "0.0s");
        assert_eq!(format_duration(Duration::from_millis(1450)), "1.5s");
        assert_eq!(format_duration(Duration::from_secs(59)), "59.0s");
    }

    #[test]
    fn format_duration_switches_to_minutes_and_hours() {
        assert_eq!(format_duration(Duration::from_secs(60)), "1m 00s");
        assert_eq!(format_duration(Duration::from_secs(312)), "5m 12s");
        assert_eq!(format_duration(Duration::from_secs(3599)), "59m 59s");
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h 00m 00s");
        // a very slow library on a very slow drive
        assert_eq!(format_duration(Duration::from_secs(7452)), "2h 04m 12s");
    }

    #[test]
    fn get_source_index_returns_none_for_black_keys() {
        // All black keys should return None
        assert!(get_source_index(1, 0).is_none());   // C#
        assert!(get_source_index(3, 0).is_none());   // D#
        assert!(get_source_index(6, 0).is_none());   // F#
        assert!(get_source_index(8, 0).is_none());   // G#
        assert!(get_source_index(10, 0).is_none());  // A#
    }
}
