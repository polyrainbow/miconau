use std::{
    fs::{self},
    path::PathBuf,
};

pub struct Track {
    pub filename: PathBuf,
}

pub struct Album {
    pub title: String,
    pub tracks: Vec<Track>,
}

pub struct Library {
    pub albums: Vec<Album>,
}

impl Library {
    pub fn new(library_folder: String) -> Library {
        let allowed_extensions = vec!["mp3", "flac"];
        let mut library = Library { albums: Vec::new() };
        let paths = fs::read_dir(library_folder).unwrap();
        for path_result in paths {
            let path = path_result.unwrap();
            let attr = fs::metadata(path.path()).unwrap();
            if attr.is_dir() {
                let mut album = Album {
                    title: path
                        .path()
                        .file_name()
                        .unwrap()
                        .to_owned()
                        .into_string()
                        .unwrap(),
                    tracks: Vec::new(),
                };

                let paths_in_album = fs::read_dir(path.path()).unwrap();
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

                // don't push empty albums to the library
                if album.tracks.len() > 0 {
                    library.albums.push(album);
                }
            }
        }

        library.albums.sort_by_key(|a| a.title.clone());
        println!("Found {} albums.", library.albums.len());
        for (i, album) in library.albums.iter().enumerate() {
            println!("{}: {} ({} tracks)", i + 1, album.title, album.tracks.len());
        }
        return library;
    }
}
