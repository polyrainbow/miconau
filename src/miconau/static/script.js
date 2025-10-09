async function loadStreams() {
  try {
    const response = await fetch('/api/streams');
    const streams = await response.json();
    const streamsContainer = document.getElementById('streams');
    streamsContainer.innerHTML = streams.map(stream => `
            <button class="stream-item" 
                 onclick="playStream(${stream.index})"
                 data-name="${stream.name}">
                 ${stream.logo_svg
        ? `<img src="/api/stream-logo/${stream.name}" alt="${stream.name} icon" class="stream-icon">`
        : ''
      }
                ${stream.name}
            </button>
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
    playlistsContainer.innerHTML = '';

    for (const playlist of playlists) {
      const details = document.createElement('details');
      details.className = 'playlist';

      const summary = document.createElement('summary');
      summary.className = 'playlist-summary';
      
      const titleSpan = document.createElement('span');
      titleSpan.textContent = playlist.name;
      titleSpan.className = 'playlist-title';
      
      const playBtn = document.createElement('button');
      playBtn.textContent = 'Play';
      playBtn.className = 'playlist-play-button';
      playBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        e.preventDefault();
        playPlaylist(playlist.index);
      });
      
      summary.appendChild(titleSpan);
      summary.appendChild(playBtn);
      details.appendChild(summary);

      const trackList = document.createElement('ul');
      trackList.className = 'track-list';
      trackList.innerHTML = '<li>Loading...</li>';
      details.appendChild(trackList);

      // Lazy load tracks when opening
      details.addEventListener('toggle', async () => {
        if (details.open && !details.dataset.loaded) {
          try {
            const trackResponse = await fetch(`/api/playlist/${playlist.index}/tracks`);
            if (!trackResponse.ok) throw new Error('Failed to load tracks');
            const tracks = await trackResponse.json();
            if (tracks.length === 0) {
              trackList.innerHTML = '<li><em>No tracks</em></li>';
            } else {
              trackList.innerHTML = tracks.map(track => 
                `<li>${escapeHtml(track.title)}</li>`
              ).join('');
            }
            details.dataset.loaded = 'true';
          } catch (err) {
            console.error('Error loading tracks:', err);
            trackList.innerHTML = '<li><em>Error loading tracks</em></li>';
          }
        }
      });

      playlistsContainer.appendChild(details);
    }
  } catch (error) {
    console.error('Error loading playlists:', error);
  }
}

function escapeHtml(str) {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
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
      ? "▶️"
      : "⏸️");

  let statusText = `${symbol}`;

  if (state.mode === "Playing" || state.mode === "Paused") {
    if (typeof state.source_name === "string") {
      statusText += ` ${state.source_name}`;
    }
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