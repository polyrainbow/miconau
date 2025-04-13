use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use actix_files as fs;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use crate::player::Player;

#[derive(Serialize, Deserialize)]
struct PlayerState {
    current_stream: Option<String>,
    current_playlist: Option<String>,
    is_playing: bool,
}

#[derive(Serialize)]
struct StreamInfo {
    name: String,
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
        
        HttpServer::new(move || {
            let player = player.clone();
            App::new()
                .app_data(web::Data::new(player))
                .route("/api/streams", web::get().to(get_streams))
                .route("/api/playlists", web::get().to(get_playlists))
                .route("/api/play/stream/{index}", web::post().to(play_stream))
                .route("/api/play/playlist/{index}", web::post().to(play_playlist))
                .route("/api/play/pause", web::post().to(play_pause))
                .route("/api/stop", web::post().to(stop))
                .route("/api/next", web::post().to(next_track))
                .route("/api/previous", web::post().to(previous_track))
                .service(fs::Files::new("/", "./src/miconau/static").index_file("index.html"))
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
            index,
        })
        .collect();
    HttpResponse::Ok().json(streams)
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