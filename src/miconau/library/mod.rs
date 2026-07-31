use std::{
    fs,
    path::{Path, PathBuf},
};
use lofty::prelude::*;
use lofty::probe::Probe;

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

/// Walks `dir` and all of its subfolders, adding every folder that directly
/// contains audio files as a playlist.
fn scan_folder(
    dir: &Path,
    root: &Path,
    allowed_extensions: &[&str],
    playlists: &mut Vec<Playlist>,
) {
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

        playlists.push(album);
    }

    subfolders.sort();
    for subfolder in subfolders {
        scan_folder(&subfolder, root, allowed_extensions, playlists);
    }
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
    pub fn new(library_folder: String) -> Library {
        let allowed_extensions = vec!["mp3", "flac"];
        let mut streams_file_found = false;
        let mut library = Library {
            folder: library_folder.clone(),
            playlists: Vec::new(),
            streams: Vec::new(),
        };
        let root = PathBuf::from(&library_folder);
        scan_folder(
            &root,
            &root,
            &allowed_extensions,
            &mut library.playlists,
        );

        let paths = fs::read_dir(&library_folder).unwrap();
        for path_result in paths {
            let root_dir_entry = path_result.unwrap();
            let metadata = fs::metadata(root_dir_entry.path()).unwrap();

            if metadata.is_file() && root_dir_entry.file_name() == "streams.txt" {
                streams_file_found = true;
                println!("Streams file found");
                let file_content = fs::read_to_string(root_dir_entry.path()).unwrap();
                
                // Split the content by double newlines to get blocks
                let stream_blocks = file_content.split("\n\n");
                
                for block in stream_blocks {
                    let lines: Vec<&str> = block.trim().lines().collect();
                    
                    // Skip empty blocks
                    if lines.is_empty() {
                        continue;
                    }
                    
                    // Each block must have at least name and URL
                    if lines.len() >= 2 {
                        let name = lines[0].trim();
                        let url = lines[1].trim();
                        
                        // Optional logo filename
                        let logo_svg = if lines.len() >= 3 {
                            let filename = lines[2].trim().to_string();
                            let filepath = PathBuf::from(
                                format!("{}/{}/{}", library.folder, "logos", filename),
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


                        
                        library.streams.push(Stream {
                            name: name.to_string(),
                            url: url.to_string(),
                            logo_svg: logo_svg.clone(),
                        });
                        
                        println!(
                            "Stream {} found: {}, Logo: {}",
                            library.streams.len(),
                            name,
                            logo_svg.is_some(),
                        );
                    }
                }
                    
            }
        }

        if !streams_file_found {
            println!("No streams file found.");
        }

        library.playlists.sort_by_key(|a| a.title.clone().to_lowercase());
        println!("Found {} playlists.", library.playlists.len());
        for (i, album) in library.playlists.iter().enumerate() {
            println!("{}: {} ({} tracks)", i + 1, album.title, album.tracks.len());
        }
        return library;
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
