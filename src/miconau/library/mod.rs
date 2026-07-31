use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use lofty::prelude::*;
use lofty::probe::Probe;

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
            "Still scanning after {}s: {} folders, {} tracks. Currently in {:?}",
            self.started.elapsed().as_secs(),
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
}

fn read_track_metadata(path: &PathBuf) -> (Option<String>, Option<String>) {
    match Probe::open(path).and_then(|p| p.read()) {
        Ok(tagged_file) => {
            if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
                let artist = tag.artist().map(|s| s.to_string());
                let title = tag.title().map(|s| s.to_string());
                (artist, title)
            } else {
                (None, None)
            }
        }
        Err(_) => (None, None)
    }
}

fn read_cover_art(path: &PathBuf) -> Option<(Vec<u8>, String)> {
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
            let (artist, title) = read_track_metadata(&path);
            tracks.push(Track {
                filename: path,
                artist,
                title,
            });
            progress.tracks += 1;
            progress.heartbeat(dir);
        }
    }

    if !tracks.is_empty() {
        tracks.sort_by_key(|a| a.filename.clone());

        let mut album = Playlist {
            title: playlist_title(dir, root),
            tracks,
            cover_art: None,
            cover_art_mime: None,
        };

        if let Some(first_track) = album.tracks.first() {
            if let Some((data, mime)) = read_cover_art(&first_track.filename) {
                album.cover_art = Some(data);
                album.cover_art_mime = Some(mime);
            }
        }

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
    let allowed_extensions = vec!["mp3", "flac"];
    let root = PathBuf::from(library_folder);

    println!("Scanning library at {}...", library_folder);
    let mut progress = ScanProgress::new();
    scan_folder(&root, &root, &allowed_extensions, on_playlist, &mut progress);

    println!(
        "Scan finished in {:.1}s: {} playlists, {} tracks in {} folders.",
        progress.started.elapsed().as_secs_f32(),
        progress.playlists,
        progress.tracks,
        progress.folders,
    );
}

/// Reads the streams from `streams.txt` in the library root. Cheap compared to
/// the folder scan, so it can be loaded up front. Streams occupy the lowest
/// white keys, so loading them first keeps the playlist keys from shifting
/// once the scan starts.
pub fn read_streams(library_folder: &str) -> Vec<Stream> {
    let mut streams: Vec<Stream> = Vec::new();

    let streams_file = PathBuf::from(library_folder).join("streams.txt");
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
                let filepath = PathBuf::from(
                    format!("{}/{}/{}", library_folder, "logos", filename),
                );
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
    pub cover_art: Option<Vec<u8>>,
    pub cover_art_mime: Option<String>,
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
    pub fn new(library_folder: String) -> Library {
        let mut library = Library::empty(library_folder.clone());
        library.streams = read_streams(&library_folder);

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
            .file("Album/03.wav", "")
            .file("Album/04", "");

        let playlists = library.scan().playlists;
        assert_eq!(playlists.len(), 1);
        assert_eq!(
            playlists[0]
                .tracks
                .iter()
                .map(|track| track.filename.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<String>>(),
            vec!["01.MP3".to_string(), "02.Flac".to_string()]
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

    fn empty_playlist(title: &str) -> Playlist {
        Playlist {
            title: title.to_string(),
            tracks: Vec::new(),
            cover_art: None,
            cover_art_mime: None,
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

    #[test]
    fn reads_streams_from_the_library_root() {
        let library = TempLibrary::new("streams");
        library
            .file("streams.txt", "A Stream\nhttp://example.com/a\n\nB Stream\nhttp://example.com/b")
            .file("Album/01.mp3", "");

        let scanned = library.scan();
        assert_eq!(scanned.streams.len(), 2);
        assert_eq!(scanned.streams[0].name, "A Stream");
        assert_eq!(scanned.streams[0].url, "http://example.com/a");
        assert_eq!(scanned.streams[1].name, "B Stream");
        // the streams file must not turn the root into a playlist
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
