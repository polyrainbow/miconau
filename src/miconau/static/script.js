async function loadStreams() {
  try {
    const response = await fetch('/api/streams');
    const streams = await response.json();
    const streamsContainer = document.getElementById('streams');
    streamsContainer.innerHTML = streams.map(stream => `
            <div class="stream-item" 
                 onclick="playStream(${stream.index})"
                 data-name="${stream.name}">
                 ${stream.logo_svg
        ? `<img src="/api/stream-logo/${stream.name}" alt="${stream.name} icon" class="stream-icon">`
        : ''
      }
                ${stream.name}
            </div>
        `).join('');
  } catch (error) {
    console.error('Error loading streams:', error);
  }
}

async function loadPlaylists() {
  try {
    const response = await fetch('/api/playlists');
    const playlists = await response.json();
    const playlistsContainer = document.getElementById('playlists');
    playlistsContainer.innerHTML = playlists.map(playlist => `
            <div class="playlist-item" 
                 onclick="playPlaylist(${playlist.index})"
                 data-name="${playlist.name}">
                ${playlist.name}
            </div>
        `).join('');
  } catch (error) {
    console.error('Error loading playlists:', error);
  }
}

async function playStream(index) {
  await fetch(`/api/play/stream/${index}`, { method: 'POST' });
}

async function playPlaylist(index) {
  await fetch(`/api/play/playlist/${index}`, { method: 'POST' });
}

async function playPause() {
  await fetch('/api/play/pause', { method: 'POST' });
}

async function stop() {
  await fetch('/api/stop', { method: 'POST' });
}

async function nextTrack() {
  await fetch('/api/next', { method: 'POST' });
}

async function previousTrack() {
  await fetch('/api/previous', { method: 'POST' });
}

function renderState(state) {
  const symbol = state.mode === "Stopped"
    ? "⏹"
    : (state.mode === "Playing"
      ? "⏵"
      : "⏸");

  let statusText = `${symbol}`;

  if (state.mode === "Playing" || state.mode === "Paused") {
    statusText += ` ${state.source_name}`;
  } else {
    statusText += ` Stopped`;
  }

  document.getElementById('status').innerHTML = statusText;
}

function connectToEvents() {
  const eventSource = new EventSource('/api/notifications');

  eventSource.onmessage = function (event) {
    const state = JSON.parse(event.data);
    renderState(state);
  }
};

// Initial load
document.addEventListener('DOMContentLoaded', () => {
  loadStreams();
  loadPlaylists();
  connectToEvents();
  fetch('/api/state')
    .then(response => response.json())
    .then(renderState)
    .catch(error => console.error('Error fetching initial status:', error));
}); 