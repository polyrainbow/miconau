use axum::{extract::{Path, Request, State, Multipart}, http::{header, HeaderMap, StatusCode}, middleware::{self, Next}, response::{sse::{Event, KeepAlive}, Response, Sse}, routing::{get, post}, Json, Router};
use serde::{Serialize};
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use std::{env::current_exe, path::PathBuf, sync::{Arc}};
use crate::{library::Stream as AudioStream, player::{Player, PlayerState}};
use std::error::Error;
use axum::response::IntoResponse;
use futures_util::stream::{Stream};
use std::convert::Infallible;
use futures_util::stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use serde_json::json;
use axum::extract::DefaultBodyLimit;

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

#[derive(Serialize)]
struct TrackInfo {
    title: String,
    index: usize,
}

#[derive(Clone)]
struct ServerState {
    player: Arc<Mutex<Player>>,
}


async fn sse_handler(
    State(server_state): State<ServerState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let player = server_state.player.lock().await;
    let receiver = player.event_transmitter.subscribe();
    let event_stream = BroadcastStream::new(receiver)
        .filter_map(|result| async move {
            match result {
                Ok(event) => {
                    Some(Ok(Event::default().json_data(&event).unwrap()))
                }
                Err(_) => None,
            }
        });
    Sse::new(event_stream).keep_alive(KeepAlive::default())
}


fn get_static_path() -> PathBuf {
    let mut static_path = current_exe().unwrap();
    static_path.pop();
    static_path.pop();
    static_path.pop();
    static_path.push("src");
    static_path.push("miconau");
    static_path.push("static");
    static_path
}


async fn get_streams(
    State(server_state): State<ServerState>
) -> Json<Vec<StreamInfo>> {
    let player = server_state.player.lock().await;
    let streams: Vec<StreamInfo> = player.library.streams
        .iter()
        .enumerate()
        .map(|(index, stream)| StreamInfo {
            name: stream.name.clone(),
            logo_svg: stream.logo_svg.clone(),
            index,
        })
        .collect();
    Json(streams)
}

async fn get_stream_logo(
    State(server_state): State<ServerState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let player = server_state.player.lock().await;
    let stream: Option<&AudioStream> = player.library.streams
        .iter()
        .find(|&x| x.name == *name);
    println!("Getting logo for stream: {}", name);
    if let Some(stream) = stream {
        if let Some(logo_svg) = &stream.logo_svg {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
            return Ok((
                headers,
                logo_svg.clone(),
            ))
        }
    }
    Err(StatusCode::NOT_FOUND)
}

async fn get_playlists(
    State(server_state): State<ServerState>,
) -> Json<Vec<PlaylistInfo>> {
    let player = server_state.player.lock().await;
    let playlists: Vec<PlaylistInfo> = player.library.playlists
        .iter()
        .enumerate()
        .map(|(index, playlist)| PlaylistInfo {
            name: playlist.title.clone(),
            index,
        })
        .collect();
    Json(playlists)
}

async fn get_playlist_tracks(
    State(server_state): State<ServerState>,
    Path(index): Path<usize>,
) -> Result<Json<Vec<TrackInfo>>, StatusCode> {
    let player = server_state.player.lock().await;
    if index >= player.library.playlists.len() {
        return Err(StatusCode::NOT_FOUND);
    }
    let playlist = &player.library.playlists[index];
    let tracks: Vec<TrackInfo> = playlist.tracks
        .iter()
        .enumerate()
        .map(|(track_index, track)| {
            let title = track.filename
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            TrackInfo {
                title,
                index: track_index,
            }
        })
        .collect();
    Ok(Json(tracks))
}

async fn get_state(
    State(server_state): State<ServerState>,
) -> Json<PlayerState> {
    let player = server_state.player.lock().await;
    Json(player.state.clone())
}

async fn play_stream(
    Path(index): Path<u64>,
    State(server_state): State<ServerState>,
) -> Result<StatusCode, StatusCode> {
    let mut player = server_state.player.lock().await;
    player.play_stream(index as u8);
    Ok(StatusCode::OK)
}

async fn play_playlist(
    State(server_state): State<ServerState>,
    Path(index): Path<u64>,
) -> Result<StatusCode, StatusCode> {
    let mut player = server_state.player.lock().await;
    player.play_playlist(index as u8);
    Ok(StatusCode::OK)
}

async fn play_playlist_track(
    State(server_state): State<ServerState>,
    Path((index, track_index)): Path<(u64, u64)>,
) -> Result<StatusCode, StatusCode> {
    let mut player = server_state.player.lock().await;
    player.play_playlist_track(index as u8, track_index as u8);
    Ok(StatusCode::OK)
}

async fn play_pause(
    State(server_state): State<ServerState>,
) -> Result<StatusCode, StatusCode> {
    let mut player = server_state.player.lock().await;
    player.play_pause();
    Ok(StatusCode::OK)
}

async fn stop(
    State(server_state): State<ServerState>,
) -> Result<StatusCode, StatusCode> {
    let mut player = server_state.player.lock().await;
    player.stop();
    Ok(StatusCode::OK)
}

async fn next_track(
    State(server_state): State<ServerState>,
) -> Result<StatusCode, StatusCode> {
    let mut player = server_state.player.lock().await;
    player.play_next_track();
    Ok(StatusCode::OK)
}

async fn previous_track(
    State(server_state): State<ServerState>,
) -> Result<StatusCode, StatusCode> {
    let mut player = server_state.player.lock().await;
    player.play_previous_track();
    Ok(StatusCode::OK)
}

async fn upload_playlist(
    State(server_state): State<ServerState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut playlist_name = String::new();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(field) = multipart.next_field().await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Error parsing multipart: {}", e)))? {
        
        let field_name = field.name().unwrap_or("").to_string();
        
        if field_name == "playlistName" {
            playlist_name = field.text().await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Error reading playlist name: {}", e)))?;
        } else if field_name.starts_with("file-") {
            let file_name = field.file_name().unwrap_or("unknown").to_string();
            let bytes = field.bytes().await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("Error reading file: {}", e)))?;
            files.push((file_name, bytes.to_vec()));
        }
    }

    if playlist_name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Playlist name is required".to_string()));
    }

    if files.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "At least one file is required".to_string()));
    }

    // Get the library folder from the player
    let player = server_state.player.lock().await;
    let library_folder = player.library.folder.clone();
    drop(player);

    // Create playlist directory
    let playlist_path = PathBuf::from(&library_folder).join(&playlist_name);
    tokio::fs::create_dir_all(&playlist_path)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error creating directory: {}", e)))?;

    // Write files to the playlist directory
    for (filename, data) in files {
        let file_path = playlist_path.join(&filename);
        tokio::fs::write(&file_path, data)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Error writing file: {}", e)))?;
    }

    // Reload library
    let mut player = server_state.player.lock().await;
    player.library = crate::library::Library::new(library_folder);
    player.notify_library_updated();

    Ok(Json(json!({"success": true})))
}


async fn disable_browser_cache(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        "Cache-Control",
        "no-cache, no-store, must-revalidate".parse().unwrap(),
    );
    response.headers_mut().insert("Pragma", "no-cache".parse().unwrap());
    response.headers_mut().insert("Expires", "0".parse().unwrap());
    response
}

pub async fn start_server(
    player_arc: Arc<Mutex<Player>>,
    address: String,
) -> Result<(), Box<dyn Error>> {
    let static_path = get_static_path();

    let api_routes = Router::new()
        .route("/streams", get(get_streams))
        .route("/stream-logo/{name}", get(get_stream_logo))
        .route("/playlists", get(get_playlists))
        .route("/playlist/{index}/tracks", get(get_playlist_tracks))
        .route("/play/stream/{index}", post(play_stream))
        .route("/play/playlist/{index}", post(play_playlist))
        .route("/play/playlist/{index}/{track_index}", post(play_playlist_track))
        .route("/play/pause", post(play_pause))
        .route("/stop", post(stop))
        .route("/next", post(next_track))
        .route("/previous", post(previous_track))
        .route("/upload-playlist", post(upload_playlist))
        .route("/notifications", get(sse_handler))
        .route("/state", get(get_state))
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024))
        .with_state(ServerState {
            player: player_arc,
        });

    let static_service = ServiceBuilder::new()
        .layer(middleware::from_fn(disable_browser_cache))
        .service(ServeDir::new(static_path));

    let app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(static_service);

    println!("Starting HTTP server on http://{}", address);
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
