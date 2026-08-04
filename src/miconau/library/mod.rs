use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use lofty::prelude::*;
use lofty::probe::Probe;
use crate::utils::format_duration;

/// How often the scan reports that it is still alive while working through a
/// single folder.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Keeps track of how far the scan has got, so a slow library (a big
/// collection on an external drive takes minutes) doesn't look like a hang.
struct ScanProgress {
    started: Instant,
    last_heartbeat: Instant,
    folders: usize,
    tracks: usize,
    playlists: usize,
}

impl ScanProgress {
    fn new() -> ScanProgress {
        let now = Instant::now();
        ScanProgress {
            started: now,
            last_heartbeat: now,
            folders: 0,
            tracks: 0,
            playlists: 0,
        }
    }

    /// Prints at most one line per HEARTBEAT_INTERVAL. Called per track, so
    /// even a folder with thousands of files keeps showing movement.
    fn heartbeat(&mut self, current: &Path) {
        if self.last_heartbeat.elapsed() < HEARTBEAT_INTERVAL {
            return;
        }
        self.last_heartbeat = Instant::now();
        println!(
            "Still scanning after {}: {} folders, {} tracks. Currently in {:?}",
            format_duration(self.started.elapsed()),
            self.folders,
            self.tracks,
            current,
        );
    }
}

pub struct Track {
    pub filename: PathBuf,
    pub artist: Option<String>,
    pub title: Option<String>,
    /// Whether the file has embedded artwork. The image itself is not kept:
    /// a few thousand albums with embedded covers add up to gigabytes, so
    /// covers are read from the file again when they are served.
    pub has_cover_art: bool,
}

impl Track {
    /// The name to show for this track: its title tag, or the file name for
    /// the untagged files a library always has some of.
    pub fn display_title(&self) -> String {
        self.title.clone().unwrap_or_else(|| {
            self.filename
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        })
    }
}

/// Reads the tags of one audio file. The scan already opens every file here,
/// so noting whether it has artwork costs nothing extra and saves reopening
/// the first track of every playlist.
fn read_track(path: PathBuf) -> Track {
    let (artist, title, has_cover_art) = match Probe::open(&path).and_then(|p| p.read()) {
        Ok(tagged_file) => {
            match tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
                Some(tag) => (
                    tag.artist().map(|s| s.to_string()),
                    tag.title().map(|s| s.to_string()),
                    !tag.pictures().is_empty(),
                ),
                None => (None, None, false),
            }
        }
        Err(_) => (None, None, false),
    };

    Track {
        filename: path,
        artist,
        title,
        has_cover_art,
    }
}

/// Reads the embedded cover of an audio file. Called when a cover is actually
/// requested rather than during the scan, so covers never accumulate in memory.
pub fn read_cover_art(path: &Path) -> Option<(Vec<u8>, String)> {
    match Probe::open(path).and_then(|p| p.read()) {
        Ok(tagged_file) => {
            if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
                if let Some(picture) = tag.pictures().first() {
                    let mime = picture
                        .mime_type()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "image/jpeg".to_string());
                    return Some((picture.data().to_vec(), mime));
                }
            }
            None
        }
        Err(_) => None,
    }
}

/// Name of the playlist for a folder: its path relative to the library root,
/// so nested folders stay unique (e.g. "Artist/Album"). The library root
/// itself falls back to its own folder name.
fn playlist_title(dir: &Path, root: &Path) -> String {
    let relative = dir.strip_prefix(root).unwrap_or(dir);
    if relative.as_os_str().is_empty() {
        root.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| root.to_string_lossy().to_string())
    } else {
        relative.to_string_lossy().to_string()
    }
}

/// Walks `dir` and all of its subfolders, handing every folder that directly
/// contains audio files to `on_playlist` as a playlist.
fn scan_folder(
    dir: &Path,
    root: &Path,
    allowed_extensions: &[&str],
    on_playlist: &mut dyn FnMut(Playlist),
    progress: &mut ScanProgress,
) {
    progress.folders += 1;
    progress.heartbeat(dir);

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            println!("Could not read folder {:?}: {}", dir, error);
            return;
        }
    };

    let mut tracks: Vec<Track> = Vec::new();
    let mut subfolders: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.starts_with(".") {
            continue;
        }

        if path.is_dir() {
            subfolders.push(path);
            continue;
        }

        let is_audio_file = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| allowed_extensions.contains(&extension.to_lowercase().as_str()))
            .unwrap_or(false);

        if is_audio_file {
            tracks.push(read_track(path));
            progress.tracks += 1;
            progress.heartbeat(dir);
        }
    }

    if !tracks.is_empty() {
        tracks.sort_by_key(|a| a.filename.clone());

        // The first track's artwork represents the playlist. Only the path is
        // kept; the image is read when it is served.
        let cover_source = tracks
            .first()
            .filter(|track| track.has_cover_art)
            .map(|track| track.filename.clone());

        let album = Playlist {
            title: playlist_title(dir, root),
            tracks,
            cover_source,
        };

        println!("Playlist found: {} ({} tracks)", album.title, album.tracks.len());
        progress.playlists += 1;
        on_playlist(album);
    }

    subfolders.sort();
    for subfolder in subfolders {
        scan_folder(&subfolder, root, allowed_extensions, on_playlist, progress);
    }
}

/// Walks the library folder, handing every playlist to `on_playlist` the
/// moment it is found. Lets callers fill a library progressively instead of
/// waiting for the whole (potentially very slow) scan to finish.
pub fn scan_playlists(library_folder: &str, on_playlist: &mut dyn FnMut(Playlist)) {
    let allowed_extensions = vec!["mp3", "flac", "wav", "ogg", "oga", "opus"];
    let root = PathBuf::from(library_folder);

    println!("Scanning library at {}...", library_folder);
    let mut progress = ScanProgress::new();
    scan_folder(&root, &root, &allowed_extensions, on_playlist, &mut progress);

    println!(
        "Scan finished in {}: {} playlists, {} tracks in {} folders.",
        format_duration(progress.started.elapsed()),
        progress.playlists,
        progress.tracks,
        progress.folders,
    );
}

/// Reads the streams from `streams.txt` in `streams_folder`, with the logos
/// they name resolved against `logos/` in that same folder. The folder is
/// deliberately not the library: streams have nothing to do with the music on
/// disk, so they are configured wherever the user keeps their config.
///
/// Cheap compared to the folder scan, so it can be loaded up front. Streams
/// occupy the lowest white keys, so loading them first keeps the playlist keys
/// from shifting once the scan starts.
pub fn read_streams(streams_folder: &str) -> Vec<Stream> {
    let mut streams: Vec<Stream> = Vec::new();

    let streams_file = PathBuf::from(streams_folder).join("streams.txt");
    if !streams_file.is_file() {
        println!("No streams file found.");
        return streams;
    }
    println!("Streams file found");

    let file_content = match fs::read_to_string(&streams_file) {
        Ok(content) => content,
        Err(error) => {
            println!("Could not read {:?}: {}", streams_file, error);
            return streams;
        }
    };

    // Split the content by double newlines to get blocks
    let stream_blocks = file_content.split("\n\n");

    for block in stream_blocks {
        let lines: Vec<&str> = block.trim().lines().collect();

        // Each block must have at least name and URL
        if lines.len() >= 2 {
            let name = lines[0].trim();
            let url = lines[1].trim();

            // Optional logo filename
            let logo_svg = if lines.len() >= 3 {
                let filename = lines[2].trim().to_string();
                let filepath = PathBuf::from(streams_folder)
                    .join("logos")
                    .join(&filename);
                println!("Logo file path: {:?}", filepath);
                let svg = fs::read_to_string(filepath);
                match svg {
                    Ok(svg_content) => Some(svg_content),
                    Err(_) => {
                        println!("Error reading logo file: {}", filename);
                        None
                    }
                }
            } else {
                None
            };

            streams.push(Stream {
                name: name.to_string(),
                url: url.to_string(),
                logo_svg: logo_svg.clone(),
            });

            println!(
                "Stream {} found: {}, Logo: {}",
                streams.len(),
                name,
                logo_svg.is_some(),
            );
        }
    }

    streams
}

/// Sort key for playlists. Also used to keep the list ordered while a
/// background scan is still filling it.
fn playlist_sort_key(title: &str) -> String {
    title.to_lowercase()
}

pub struct Playlist {
    pub title: String,
    pub tracks: Vec<Track>,
    /// The audio file whose embedded cover represents this playlist, if any.
    /// Holding the path instead of the image keeps a large library's covers
    /// out of memory.
    pub cover_source: Option<PathBuf>,
}

impl Playlist {
    /// Whether this playlist should be shown for `filter`. Every word of the
    /// filter has to turn up somewhere in the playlist - its own title, or the
    /// title or artist of one of its tracks - but they may turn up in
    /// different places, so "beatles revolver" finds the album even though
    /// neither word alone identifies it. An empty filter matches everything.
    pub fn matches_filter(&self, filter: &str) -> bool {
        let words: Vec<String> = filter
            .split_whitespace()
            .map(|word| word.to_lowercase())
            .collect();
        if words.is_empty() {
            return true;
        }

        let mut fields: Vec<String> = vec![self.title.to_lowercase()];
        for track in &self.tracks {
            fields.push(track.display_title().to_lowercase());
            if let Some(artist) = &track.artist {
                fields.push(artist.to_lowercase());
            }
        }

        words
            .iter()
            .all(|word| fields.iter().any(|field| field.contains(word)))
    }
}

pub struct Stream {
    pub name: String,
    pub url: String,
    pub logo_svg: Option<String>,
}

pub struct Library {
    pub folder: String,
    pub playlists: Vec<Playlist>,
    pub streams: Vec<Stream>,
}

impl Library {
    /// An unscanned library, used while the real scan runs in the background.
    pub fn empty(library_folder: String) -> Library {
        Library {
            folder: library_folder,
            playlists: Vec::new(),
            streams: Vec::new(),
        }
    }

    /// Inserts a playlist at its sorted position, so the library stays ordered
    /// even while a background scan is still adding to it.
    pub fn insert_playlist(&mut self, playlist: Playlist) {
        let key = playlist_sort_key(&playlist.title);
        let position = self
            .playlists
            .partition_point(|existing| playlist_sort_key(&existing.title) <= key);
        self.playlists.insert(position, playlist);
    }

    /// Finds the playlist and track a file belongs to. Tracks are grouped by
    /// the folder they live in, so the file's own path says which playlist to
    /// look in and no scan of the whole library is needed.
    pub fn find_track(&self, file_path: &Path) -> Option<(&Playlist, &Track)> {
        let folder = file_path.parent()?;
        let title = playlist_title(folder, Path::new(&self.folder));
        let playlist = self.playlists.iter().find(|playlist| playlist.title == title)?;
        let track = playlist.tracks.iter().find(|track| track.filename == file_path)?;
        Some((playlist, track))
    }

    /// Logs the playlists with the index each one is reachable at, both on the
    /// keyboard and in the web API.
    pub fn log_playlists(&self) {
        for (i, album) in self.playlists.iter().enumerate() {
            println!("{}: {} ({} tracks)", i + 1, album.title, album.tracks.len());
        }
    }

    /// Scans the whole library at once. Blocks until the scan is done, so
    /// callers that need to stay responsive should use `scan_playlists`
    /// together with `insert_playlist` instead.
    ///
    /// Leaves `streams` empty: they come from a folder of their own that the
    /// library knows nothing about, so a caller replacing its library with a
    /// rescan has to carry the streams over itself.
    pub fn new(library_folder: String) -> Library {
        let mut library = Library::empty(library_folder.clone());

        let mut playlists: Vec<Playlist> = Vec::new();
        scan_playlists(&library_folder, &mut |playlist| playlists.push(playlist));
        for playlist in playlists {
            library.insert_playlist(playlist);
        }

        library.log_playlists();
        library
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::MAIN_SEPARATOR;

    /// A library folder in the system temp dir that deletes itself again when
    /// the test ends.
    struct TempLibrary {
        path: PathBuf,
    }

    impl TempLibrary {
        fn new(name: &str) -> TempLibrary {
            let path = std::env::temp_dir()
                .join(format!("miconau-test-{}-{}", std::process::id(), name));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempLibrary { path }
        }

        /// Creates an empty folder, relative to the library root.
        fn folder(&self, relative: &str) -> &TempLibrary {
            fs::create_dir_all(self.path.join(relative)).unwrap();
            self
        }

        /// Creates a file (and any missing parent folders), relative to the
        /// library root. The content is irrelevant for scanning, so tracks are
        /// simply empty files without tags.
        fn file(&self, relative: &str, content: &str) -> &TempLibrary {
            let path = self.path.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
            self
        }

        /// Creates a file with raw bytes, for content that has to be a valid
        /// audio file.
        fn bytes(&self, relative: &str, content: &[u8]) -> PathBuf {
            let path = self.path.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            path
        }

        fn scan(&self) -> Library {
            Library::new(self.path.to_str().unwrap().to_string())
        }

        fn playlist_titles(&self) -> Vec<String> {
            self.scan()
                .playlists
                .into_iter()
                .map(|playlist| playlist.title)
                .collect()
        }

        fn name(&self) -> String {
            self.path.file_name().unwrap().to_string_lossy().to_string()
        }
    }

    impl Drop for TempLibrary {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// Builds a playlist title from path segments, so expectations don't depend
    /// on the platform's path separator.
    fn title(segments: &[&str]) -> String {
        segments.join(&MAIN_SEPARATOR.to_string())
    }

    #[test]
    fn playlist_title_is_relative_to_the_library_root() {
        let root = PathBuf::from("/music");

        assert_eq!(playlist_title(&root.join("Album"), &root), "Album");
        assert_eq!(
            playlist_title(&root.join("Artist").join("Album"), &root),
            title(&["Artist", "Album"])
        );
        // the library root itself falls back to its own folder name
        assert_eq!(playlist_title(&root, &root), "music");
    }

    #[test]
    fn finds_playlists_in_subdirectories() {
        let library = TempLibrary::new("subdirectories");
        library
            .file("Top Album/01.mp3", "")
            .file("Artist/Album A/01.mp3", "")
            .file("Artist/Album B/01.mp3", "")
            .file("Deep/One/Two/Three/01.flac", "");

        assert_eq!(
            library.playlist_titles(),
            vec![
                title(&["Artist", "Album A"]),
                title(&["Artist", "Album B"]),
                title(&["Deep", "One", "Two", "Three"]),
                "Top Album".to_string(),
            ]
        );
    }

    #[test]
    fn adds_audio_files_in_the_library_root_as_a_playlist() {
        let library = TempLibrary::new("root-tracks");
        library.file("01.mp3", "").file("Album/01.mp3", "");

        assert_eq!(
            library.playlist_titles(),
            vec!["Album".to_string(), library.name()]
        );
    }

    #[test]
    fn skips_folders_without_audio_files() {
        let library = TempLibrary::new("no-audio");
        library
            .folder("Empty Folder")
            .file("logos/station.svg", "<svg/>")
            .file("Artist/notes.txt", "hello")
            // "Artist" itself holds no audio, only the album below it does
            .file("Artist/Album/01.mp3", "");

        assert_eq!(
            library.playlist_titles(),
            vec![title(&["Artist", "Album"])]
        );
    }

    #[test]
    fn skips_hidden_files_and_folders() {
        let library = TempLibrary::new("hidden");
        library
            .file("Album/01.mp3", "")
            .file("Album/._02.mp3", "")
            .file(".hidden/01.mp3", "");

        let playlists = library.scan().playlists;
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].title, "Album");
        assert_eq!(playlists[0].tracks.len(), 1);
    }

    #[test]
    fn matches_extensions_case_insensitively() {
        let library = TempLibrary::new("extensions");
        library
            .file("Album/01.MP3", "")
            .file("Album/02.Flac", "")
            .file("Album/03.WaV", "")
            .file("Album/04.OGG", "")
            .file("Album/05.Oga", "")
            .file("Album/06.OPUS", "")
            .file("Album/07.m4a", "")
            .file("Album/08", "");

        let playlists = library.scan().playlists;
        assert_eq!(playlists.len(), 1);
        assert_eq!(
            playlists[0]
                .tracks
                .iter()
                .map(|track| track.filename.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<String>>(),
            vec![
                "01.MP3".to_string(),
                "02.Flac".to_string(),
                "03.WaV".to_string(),
                "04.OGG".to_string(),
                "05.Oga".to_string(),
                "06.OPUS".to_string(),
            ]
        );
    }

    #[test]
    fn sorts_tracks_by_filename() {
        let library = TempLibrary::new("track-order");
        library
            .file("Album/03.mp3", "")
            .file("Album/01.mp3", "")
            .file("Album/02.mp3", "");

        let playlists = library.scan().playlists;
        assert_eq!(
            playlists[0]
                .tracks
                .iter()
                .map(|track| track.filename.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<String>>(),
            vec!["01.mp3".to_string(), "02.mp3".to_string(), "03.mp3".to_string()]
        );
    }

    #[test]
    fn finds_a_track_by_its_file_path() {
        let library = TempLibrary::new("find-track");
        library
            .file("Artist/Album/01.mp3", "")
            .file("Artist/Album/02.mp3", "")
            .file("Other/01.mp3", "");
        let scanned = library.scan();

        let (playlist, track) = scanned
            .find_track(&library.path.join("Artist/Album/02.mp3"))
            .expect("track should be found");
        assert_eq!(playlist.title, title(&["Artist", "Album"]));
        assert_eq!(track.display_title(), "02");
    }

    #[test]
    fn finds_a_track_in_the_library_root() {
        let library = TempLibrary::new("find-root-track");
        library.file("01.mp3", "").file("Album/01.mp3", "");
        let scanned = library.scan();

        let (playlist, _) = scanned
            .find_track(&library.path.join("01.mp3"))
            .expect("track should be found");
        assert_eq!(playlist.title, library.name());
    }

    #[test]
    fn finds_no_track_for_files_outside_the_library() {
        let library = TempLibrary::new("find-nothing");
        library.file("Album/01.mp3", "");
        let scanned = library.scan();

        // a stream, and a file in a folder the library does know
        assert!(scanned.find_track(Path::new("http://example.com/stream")).is_none());
        assert!(scanned.find_track(&library.path.join("Album/99.mp3")).is_none());
    }

    #[test]
    fn playlist_titles_are_unique_for_identically_named_subfolders() {
        let library = TempLibrary::new("duplicate-names");
        library
            .file("Artist A/Live/01.mp3", "")
            .file("Artist B/Live/01.mp3", "");

        let titles = library.playlist_titles();
        assert_eq!(
            titles,
            vec![title(&["Artist A", "Live"]), title(&["Artist B", "Live"])]
        );
    }

    /// Writes an mp3 with an embedded cover, and returns the picture bytes it
    /// was given.
    fn write_mp3_with_cover(library: &TempLibrary, relative: &str) -> Vec<u8> {
        use lofty::config::WriteOptions;
        use lofty::picture::{MimeType, Picture, PictureType};
        use lofty::tag::{Tag, TagType};

        // A run of silent MPEG-1 Layer III frames (128 kbps, 44.1 kHz, so
        // 417 bytes each). lofty reads audio properties and rejects the file
        // unless it finds consecutive valid frames.
        let mut frame = vec![0xFF, 0xFB, 0x90, 0x00];
        frame.resize(417, 0);
        let mp3 = frame.repeat(8);
        let path = library.bytes(relative, &mp3);

        let picture_data = b"not really a jpeg, but never decoded".to_vec();
        let mut tag = Tag::new(TagType::Id3v2);
        tag.push_picture(Picture::new_unchecked(
            PictureType::CoverFront,
            Some(MimeType::Jpeg),
            None,
            picture_data.clone(),
        ));
        tag.save_to_path(&path, WriteOptions::default()).unwrap();

        picture_data
    }

    #[test]
    fn cover_art_is_referenced_by_path_and_read_on_demand() {
        let library = TempLibrary::new("cover-art");
        let picture_data = write_mp3_with_cover(&library, "With Cover/01.mp3");
        library.file("Without Cover/01.mp3", "");

        let scanned = library.scan();
        let with_cover = scanned
            .playlists
            .iter()
            .find(|playlist| playlist.title == "With Cover")
            .unwrap();
        let without_cover = scanned
            .playlists
            .iter()
            .find(|playlist| playlist.title == "Without Cover")
            .unwrap();

        // the scan records where the cover is, not the cover itself
        assert_eq!(
            with_cover.cover_source,
            Some(library.path.join("With Cover/01.mp3"))
        );
        assert!(with_cover.tracks[0].has_cover_art);
        assert_eq!(without_cover.cover_source, None);
        assert!(!without_cover.tracks[0].has_cover_art);

        // and serving it reads the image back out of the file
        let (data, mime) = read_cover_art(with_cover.cover_source.as_ref().unwrap()).unwrap();
        assert_eq!(data, picture_data);
        assert_eq!(mime, "image/jpeg");
    }

    fn empty_playlist(title: &str) -> Playlist {
        Playlist {
            title: title.to_string(),
            tracks: Vec::new(),
            cover_source: None,
        }
    }

    #[test]
    fn inserted_playlists_stay_sorted() {
        let mut library = Library::empty("/music".to_string());
        for title in ["Zebra", "apple", "Middle", "Apricot"] {
            library.insert_playlist(empty_playlist(title));
        }

        assert_eq!(
            library
                .playlists
                .iter()
                .map(|playlist| playlist.title.clone())
                .collect::<Vec<String>>(),
            vec!["apple", "Apricot", "Middle", "Zebra"]
        );
    }

    /// The progressive scan main uses must end up with exactly the library a
    /// blocking `Library::new` would have produced.
    #[test]
    fn scanning_progressively_yields_the_same_playlists_as_a_full_scan() {
        let temp = TempLibrary::new("progressive");
        temp.file("Zebra/01.mp3", "")
            .file("apple/01.mp3", "")
            .file("Artist/01.mp3", "")
            .file("Artist/Album/01.mp3", "");

        let folder = temp.path.to_str().unwrap().to_string();
        let mut progressive = Library::empty(folder.clone());
        scan_playlists(&folder, &mut |playlist| {
            progressive.insert_playlist(playlist)
        });

        assert_eq!(
            progressive
                .playlists
                .iter()
                .map(|playlist| playlist.title.clone())
                .collect::<Vec<String>>(),
            temp.playlist_titles()
        );
        assert_eq!(progressive.playlists.len(), 4);
    }

    fn track(filename: &str, title: Option<&str>, artist: Option<&str>) -> Track {
        Track {
            filename: PathBuf::from(filename),
            artist: artist.map(|artist| artist.to_string()),
            title: title.map(|title| title.to_string()),
            has_cover_art: false,
        }
    }

    fn filter_playlist() -> Playlist {
        Playlist {
            title: "The Beatles/Revolver".to_string(),
            tracks: vec![
                track("01.mp3", Some("Taxman"), Some("The Beatles")),
                track("02.mp3", Some("Eleanor Rigby"), Some("The Beatles")),
                track("03 Untagged Song.mp3", None, None),
            ],
            cover_source: None,
        }
    }

    #[test]
    fn filter_matches_playlist_title_song_title_and_artist() {
        let playlist = filter_playlist();

        assert!(playlist.matches_filter("revolver"));
        assert!(playlist.matches_filter("rigby"));
        assert!(playlist.matches_filter("beatles"));
        // untagged tracks are matched by the name they are shown under
        assert!(playlist.matches_filter("untagged song"));

        assert!(!playlist.matches_filter("zappa"));
    }

    #[test]
    fn filter_matches_case_insensitively_and_on_partial_words() {
        let playlist = filter_playlist();

        assert!(playlist.matches_filter("BEATLES"));
        assert!(playlist.matches_filter("ReVoLvEr"));
        assert!(playlist.matches_filter("axma"));
    }

    #[test]
    fn every_filter_word_has_to_match_but_they_may_match_different_fields() {
        let playlist = filter_playlist();

        // "beatles" is an artist, "revolver" the playlist title
        assert!(playlist.matches_filter("beatles revolver"));
        // and one word missing is enough to drop the playlist
        assert!(!playlist.matches_filter("beatles yesterday"));
    }

    #[test]
    fn an_empty_filter_matches_every_playlist() {
        let playlist = filter_playlist();

        assert!(playlist.matches_filter(""));
        assert!(playlist.matches_filter("   "));
        assert!(empty_playlist("Anything").matches_filter(""));
    }

    #[test]
    fn reads_streams_from_the_streams_folder() {
        let folder = TempLibrary::new("streams");
        folder.file(
            "streams.txt",
            "A Stream\nhttp://example.com/a\n\nB Stream\nhttp://example.com/b",
        );

        let streams = read_streams(folder.path.to_str().unwrap());
        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].name, "A Stream");
        assert_eq!(streams[0].url, "http://example.com/a");
        assert_eq!(streams[1].name, "B Stream");
    }

    #[test]
    fn reads_stream_logos_from_the_streams_folder() {
        let folder = TempLibrary::new("stream-logos");
        folder
            .file(
                "streams.txt",
                "With Logo\nhttp://example.com/a\nstation.svg\n\nMissing Logo\nhttp://example.com/b\ngone.svg\n\nNo Logo\nhttp://example.com/c",
            )
            .file("logos/station.svg", "<svg id=\"station\"/>");

        let streams = read_streams(folder.path.to_str().unwrap());
        assert_eq!(streams.len(), 3);
        // the logo is resolved against the streams folder, not the library
        assert_eq!(streams[0].logo_svg.as_deref(), Some("<svg id=\"station\"/>"));
        // a logo that isn't there must not lose the stream itself
        assert_eq!(streams[1].logo_svg, None);
        assert_eq!(streams[1].url, "http://example.com/b");
        assert_eq!(streams[2].logo_svg, None);
    }

    #[test]
    fn a_missing_streams_folder_yields_no_streams() {
        let folder = TempLibrary::new("no-streams");

        assert!(read_streams(folder.path.to_str().unwrap()).is_empty());
        assert!(read_streams(
            folder.path.join("does-not-exist").to_str().unwrap()
        )
        .is_empty());
    }

    /// Streams live outside the library folder, so a scan must not pick them
    /// up even when a leftover streams.txt is sitting in the library root.
    #[test]
    fn scanning_the_library_finds_no_streams() {
        let library = TempLibrary::new("streams-not-in-library");
        library
            .file("streams.txt", "A Stream\nhttp://example.com/a")
            .file("Album/01.mp3", "");

        let scanned = library.scan();
        assert!(scanned.streams.is_empty());
        // the streams file must not turn the root into a playlist either
        assert_eq!(
            scanned
                .playlists
                .into_iter()
                .map(|playlist| playlist.title)
                .collect::<Vec<String>>(),
            vec!["Album".to_string()]
        );
    }
}

