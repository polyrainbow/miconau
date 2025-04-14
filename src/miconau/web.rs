use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use actix_files as fs;
use serde::{Deserialize, Serialize};
use std::{env::current_exe, sync::{Arc, Mutex}};
use crate::{library::Stream, player::Player};

#[derive(Serialize, Deserialize)]
struct PlayerState {
    current_stream: Option<String>,
    current_playlist: Option<String>,
    is_playing: bool,
}

#[derive(Serialize)]
struct StreamInfo {
    name: String,
    logo_svg: Option<String>,
    index: usize,
}

#[derive(Serialize)]
struct PlaylistInfo {
    name: String,
    index: usize,
}

pub struct WebServer {
    player: Arc<Mutex<Player>>,
    address: String,
}

impl WebServer {
    pub fn new(player: Arc<Mutex<Player>>, address: String) -> Self {
        WebServer { player, address }
    }

    pub async fn start(&self) -> std::io::Result<()> {
        let player = self.player.clone();

        let mut static_path = current_exe().unwrap();
        static_path.pop();
        static_path.pop();
        static_path.pop();
        static_path.push("src");
        static_path.push("miconau");
        static_path.push("static");
        
        HttpServer::new(move || {
            let player = player.clone();
            App::new()
                .app_data(web::Data::new(player))
                .route("/api/streams", web::get().to(get_streams))
                .route("/api/stream-logo/{name}", web::get().to(get_stream_logo))
                .route("/api/playlists", web::get().to(get_playlists))
                .route("/api/play/stream/{index}", web::post().to(play_stream))
                .route("/api/play/playlist/{index}", web::post().to(play_playlist))
                .route("/api/play/pause", web::post().to(play_pause))
                .route("/api/stop", web::post().to(stop))
                .route("/api/next", web::post().to(next_track))
                .route("/api/previous", web::post().to(previous_track))
                .service(fs::Files::new(
                    "/",
                    &static_path,
                ).index_file("index.html"))
        })
        .bind(self.address.clone())?
        .run()
        .await
    }
}

async fn get_streams(player: web::Data<Arc<Mutex<Player>>>) -> impl Responder {
    let player = player.lock().unwrap();
    let streams: Vec<StreamInfo> = player.library.streams
        .iter()
        .enumerate()
        .map(|(index, stream)| StreamInfo {
            name: stream.name.clone(),
            logo_svg: stream.logo_svg.clone(),
            index,
        })
        .collect();
    HttpResponse::Ok().json(streams)
}

async fn get_stream_logo(
    player: web::Data<Arc<Mutex<Player>>>,
    name: web::Path<String>,
) -> impl Responder {
    let player = player.lock().unwrap();
    let stream: Option<&Stream> = player.library.streams
        .iter()
        .find(|&x| x.name == *name);
    if let Some(stream) = stream {
        if let Some(logo_svg) = &stream.logo_svg {
            return HttpResponse::Ok().content_type("image/svg+xml")
                .body(logo_svg.clone());
        }
    }
    HttpResponse::NotFound().finish()
}

async fn get_playlists(player: web::Data<Arc<Mutex<Player>>>) -> impl Responder {
    let player = player.lock().unwrap();
    let playlists: Vec<PlaylistInfo> = player.library.playlists
        .iter()
        .enumerate()
        .map(|(index, playlist)| PlaylistInfo {
            name: playlist.title.clone(),
            index,
        })
        .collect();
    HttpResponse::Ok().json(playlists)
}

async fn play_stream(
    player: web::Data<Arc<Mutex<Player>>>,
    index: web::Path<usize>,
) -> impl Responder {
    let mut player = player.lock().unwrap();
    player.play_stream(*index as u8);
    HttpResponse::Ok().finish()
}

async fn play_playlist(
    player: web::Data<Arc<Mutex<Player>>>,
    index: web::Path<usize>,
) -> impl Responder {
    let mut player = player.lock().unwrap();
    player.play_playlist(*index as u8);
    HttpResponse::Ok().finish()
}

async fn play_pause(player: web::Data<Arc<Mutex<Player>>>) -> impl Responder {
    let mut player = player.lock().unwrap();
    player.play_pause();
    HttpResponse::Ok().finish()
}

async fn stop(player: web::Data<Arc<Mutex<Player>>>) -> impl Responder {
    let mut player = player.lock().unwrap();
    player.stop();
    HttpResponse::Ok().finish()
}

async fn next_track(player: web::Data<Arc<Mutex<Player>>>) -> impl Responder {
    let mut player = player.lock().unwrap();
    player.play_next_track();
    HttpResponse::Ok().finish()
}

async fn previous_track(player: web::Data<Arc<Mutex<Player>>>) -> impl Responder {
    let mut player = player.lock().unwrap();
    player.play_previous_track();
    HttpResponse::Ok().finish()
} 