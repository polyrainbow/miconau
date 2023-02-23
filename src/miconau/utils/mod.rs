static WHITE_KEYS: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];

// https://www.inspiredacoustics.com/en/MIDI_note_numbers_and_center_frequencies
pub fn is_white_key(key: u8) -> bool {
    return WHITE_KEYS.contains(&(key % 12));
}

pub fn get_album_index(key: u8, start_octave: u8) -> Option<u8> {
    let octave = key / 12;

    let index_within_octave = WHITE_KEYS.iter().position(|&x| x == (key) % 12);

    match index_within_octave {
        Some(index_within_octave) => {
            let (album_index, overflow) = (octave * WHITE_KEYS.len() as u8
                + index_within_octave as u8)
                .overflowing_sub(start_octave * WHITE_KEYS.len() as u8);

            if overflow {
                None
            } else {
                Some(album_index)
            }
        }
        None => None,
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
    fn get_album_index_works() {
        // low key with high offset octave, album index is always 0
        assert!(get_album_index(21, 10).is_none()); // A
        assert!(get_album_index(22, 10).is_none()); // Bb
        assert!(get_album_index(23, 10).is_none()); // B
        assert!(get_album_index(24, 10).is_none()); // C
        assert!(get_album_index(25, 10).is_none()); // C#
        assert!(get_album_index(26, 10).is_none()); // D
        assert!(get_album_index(27, 10).is_none()); // D#
        assert!(get_album_index(28, 10).is_none()); // E

        // octave offset = 1
        assert_eq!(get_album_index(12, 1).unwrap(), 0); // C
        assert!(get_album_index(13, 1).is_none()); // C#
        assert_eq!(get_album_index(14, 1).unwrap(), 1); // D
        assert!(get_album_index(15, 1).is_none()); // D#
        assert_eq!(get_album_index(16, 1).unwrap(), 2); // E

        // octave offset = 2
        assert_eq!(get_album_index(24, 2).unwrap(), 0); // C
        assert!(get_album_index(25, 2).is_none()); // C#
        assert_eq!(get_album_index(26, 2).unwrap(), 1); // D
        assert!(get_album_index(27, 2).is_none()); // D#
        assert_eq!(get_album_index(28, 2).unwrap(), 2); // E

        assert_eq!(get_album_index(36, 2).unwrap(), 7); // Higher C
    }
}
