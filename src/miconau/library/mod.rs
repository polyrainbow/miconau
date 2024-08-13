use std::{
    fs::{self, File}, io::BufReader, path::PathBuf
};
use std::io::BufRead;

pub struct Track {
    pub filename: PathBuf,
}

pub struct Playlist {
    pub title: String,
    pub tracks: Vec<Track>,
}

pub struct Stream {
    pub url: String,
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
        let paths = fs::read_dir(library_folder).unwrap();
        for path_result in paths {
            let root_dir_entry = path_result.unwrap();
            let metadata = fs::metadata(root_dir_entry.path()).unwrap();
            if metadata.is_dir() {
                let mut album = Playlist {
                    title: root_dir_entry
                        .path()
                        .file_name()
                        .unwrap()
                        .to_owned()
                        .into_string()
                        .unwrap(),
                    tracks: Vec::new(),
                };

                let paths_in_album = fs::read_dir(root_dir_entry.path()).unwrap();
                for path_result in paths_in_album {
                    let dir_entry = path_result.unwrap();
                    let path_buf = dir_entry.path();
                    let extension_as_path = path_buf.as_path().extension();

                    match extension_as_path {
                        Some(os_str) => {
                            let extension_str = os_str.to_str().unwrap();
                            let attr = fs::metadata(dir_entry.path()).unwrap();
                            let filename_without_path = path_buf.file_name().unwrap();
                            let filename_is_valid = !filename_without_path
                                .to_owned()
                                .into_string()
                                .unwrap()
                                .starts_with(".");
                            if attr.is_file()
                                && allowed_extensions.contains(&extension_str)
                                && filename_is_valid
                            {
                                let track = Track {
                                    filename: dir_entry.path(),
                                };
                                album.tracks.push(track);
                            }
                        }
                        None => {
                            continue;
                        }
                    }
                }

                album.tracks.sort_by_key(|a| a.filename.clone());

                // don't push empty playlists to the library
                if album.tracks.len() > 0 {
                    library.playlists.push(album);
                }
            }

            if metadata.is_file() && root_dir_entry.file_name() == "streams.txt" {
                streams_file_found = true;
                println!("Streams file found");
                let file = File::open(root_dir_entry.path()).unwrap();
                let reader = BufReader::new(file);

                for line in reader.lines() {
                    match line {
                        Ok(url) => {
                            let trimmed = String::from(url.trim());
                            if trimmed.len() > 0 {
                                library.streams.push(Stream{url: trimmed.clone()});
                                println!("Stream {} found: {}", library.streams.len(), trimmed);
                            }
                        }
                        _ => {}
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
