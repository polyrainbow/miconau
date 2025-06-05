use axum::{extract::{Path, Request, State}, http::{header, HeaderMap, StatusCode}, middleware::{self, Next}, response::{sse::{Event, KeepAlive}, Response, Sse}, routing::{get, post}, Json, Router};
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

#[derive(Clone)]
struct ServerState {
    player: Arc<Mutex<Player>>,
}


async fn sse_handler(
    State(server_state): State<ServerState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let player = server_state.player.lock().await;
    let receiver = player.state_transmitter.subscribe();
    let event_stream = BroadcastStream::new(receiver)
        .filter_map(|result| async move {
            match result {
                Ok(state) => {
                    Some(Ok(Event::default().json_data(&state).unwrap()))
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
        .route("/play/stream/{index}", post(play_stream))
        .route("/play/playlist/{index}", post(play_playlist))
        .route("/play/pause", post(play_pause))
        .route("/stop", post(stop))
        .route("/next", post(next_track))
        .route("/previous", post(previous_track))
        .route("/notifications", get(sse_handler))
        .route("/state", get(get_state))
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
