use std::{
    fs, path::PathBuf
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

pub struct Playlist {
    pub title: String,
    pub tracks: Vec<Track>,
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
                                let track_path = dir_entry.path();
                                let (artist, title) = read_track_metadata(&track_path);
                                let track = Track {
                                    filename: track_path,
                                    artist,
                                    title,
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
