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
                <span class="stream-title">${stream.name}</span>
            </button>
        `).join('');
  } catch (error) {
    console.error('Error loading streams:', error);
  }
}

let filterTimeout;

/// Reloads the list a moment after the last keystroke, so typing a word does
/// not fire a request per letter.
function filterPlaylists() {
  clearTimeout(filterTimeout);
  filterTimeout = setTimeout(loadPlaylists, 200);
}

/// The rows currently on screen, by playlist name. Reusing them instead of
/// rebuilding the list keeps expanded playlists open, their tracks loaded and
/// their covers displayed while a scan keeps filling the library.
const playlistRows = new Map();

/// Builds the row for a playlist. The row keeps its playlist index in
/// `row.playlistIndex` and every handler reads it from there, because a scan
/// inserts playlists in sorted order and so shifts the indices of the rows
/// already on screen.
function createPlaylistRow(playlist) {
  // Wrapper div for the playlist row
  const playlistWrapper = document.createElement('div');
  playlistWrapper.className = 'playlist';
  playlistWrapper.playlistIndex = playlist.index;

  // Details element for expandable tracks
  const details = document.createElement('details');
  details.className = 'playlist-details';

  const summary = document.createElement('summary');
  summary.className = 'playlist-summary';

  if (playlist.has_cover) {
    const coverImg = document.createElement('img');
    coverImg.src = `/api/playlist/${playlist.index}/cover`;
    coverImg.alt = '';
    coverImg.className = 'playlist-cover';
    coverImg.loading = 'lazy';
    summary.appendChild(coverImg);
  }

  const titleSpan = document.createElement('span');
  titleSpan.textContent = playlist.name;
  titleSpan.className = 'playlist-title';

  summary.appendChild(titleSpan);
  details.appendChild(summary);

  // Inner play button (visible when expanded, outside summary for accessibility)
  const innerPlayBtn = document.createElement('button');
  innerPlayBtn.textContent = '▶ Play';
  innerPlayBtn.className = 'playlist-play-button-inner';
  innerPlayBtn.addEventListener('click', () => {
    playPlaylist(playlistWrapper.playlistIndex);
  });
  details.appendChild(innerPlayBtn);

  const trackList = document.createElement('ul');
  trackList.className = 'track-list';
  trackList.innerHTML = '<li>Loading...</li>';
  details.appendChild(trackList);

  // The track buttons only carry their track index, so the row survives a
  // shifting playlist index without having to be re-rendered.
  trackList.addEventListener('click', (event) => {
    const button = event.target.closest('button');
    if (!button || !button.dataset.trackIndex) return;
    const trackIndex = Number(button.dataset.trackIndex);
    if (button.classList.contains('track-play-button')) {
      playPlaylistTrack(playlistWrapper.playlistIndex, trackIndex);
    } else {
      addToQueue(playlistWrapper.playlistIndex, trackIndex);
    }
  });

  // Play button outside details/summary for accessibility
  const playBtn = document.createElement('button');
  playBtn.textContent = '▶';
  playBtn.className = 'playlist-play-button';
  playBtn.addEventListener('click', () => {
    playPlaylist(playlistWrapper.playlistIndex);
  });

  // Lazy load tracks when opening
  details.addEventListener('toggle', async () => {
    if (details.open && !details.dataset.loaded) {
      try {
        const trackResponse = await fetch(
          `/api/playlist/${playlistWrapper.playlistIndex}/tracks`,
        );
        if (!trackResponse.ok) throw new Error('Failed to load tracks');
        const tracks = await trackResponse.json();
        if (tracks.length === 0) {
          trackList.innerHTML = '<li><em>No tracks</em></li>';
        } else {
          trackList.innerHTML = tracks.map(track =>
            `<li>
              <button class="track-play-button" data-track-index="${track.index}">
                <span class="track-title">${escapeHtml(track.title)}</span>
                ${track.artist ? `<span class="track-artist">${escapeHtml(track.artist)}</span>` : ''}
              </button>
              <button class="track-queue-button" data-track-index="${track.index}">
                <img src="/icons/queue_music.svg" alt="Add to queue" class="queue-icon">
              </button>
            </li>`
          ).join('');
        }
        details.dataset.loaded = 'true';
      } catch (err) {
        console.error('Error loading tracks:', err);
        trackList.innerHTML = '<li><em>Error loading tracks</em></li>';
      }
    }
  });

  playlistWrapper.appendChild(details);
  playlistWrapper.appendChild(playBtn);
  return playlistWrapper;
}

/// Points an existing row at the index the playlist now has. Only a cover that
/// has not been fetched yet needs a new URL; one that is already on screen
/// shows the right image no matter which index it was loaded from.
function updatePlaylistIndex(row, index) {
  if (row.playlistIndex === index) return;
  row.playlistIndex = index;

  const coverImg = row.querySelector('.playlist-cover');
  if (coverImg && !coverImg.complete) {
    coverImg.src = `/api/playlist/${index}/cover`;
  }
}

async function loadPlaylists() {
  try {
    const filter = document.getElementById('playlistFilter').value.trim();
    const response = await fetch(`/api/playlists?filter=${encodeURIComponent(filter)}`);
    const playlists = await response.json();
    const playlistsContainer = document.getElementById('playlists');
    document.getElementById('playlistsEmpty').style.display =
      playlists.length === 0 && filter ? 'block' : 'none';

    // Move the rows that are already there into their new position instead of
    // recreating them, and only build the ones that are new.
    let previousRow = null;
    for (const playlist of playlists) {
      let row = playlistRows.get(playlist.name);
      if (row) {
        updatePlaylistIndex(row, playlist.index);
      } else {
        row = createPlaylistRow(playlist);
        playlistRows.set(playlist.name, row);
      }

      const expectedNext = previousRow ? previousRow.nextSibling : playlistsContainer.firstChild;
      if (row !== expectedNext) {
        playlistsContainer.insertBefore(row, expectedNext);
      }
      previousRow = row;
    }

    // Drop what the filter or a rescan removed.
    const names = new Set(playlists.map(playlist => playlist.name));
    for (const [name, row] of playlistRows) {
      if (!names.has(name)) {
        row.remove();
        playlistRows.delete(name);
      }
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

async function playPlaylistTrack(playlistIndex, trackIndex) {
  await fetch(
    `/api/play/playlist/${playlistIndex}/${trackIndex}`,
    { method: 'POST' },
  );
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

async function loadQueue() {
  try {
    const response = await fetch('/api/queue');
    const queue = await response.json();
    renderQueue(queue);
  } catch (error) {
    console.error('Error loading queue:', error);
  }
}

function renderQueue(queue) {
  const queueList = document.getElementById('queue');
  const queueEmpty = document.getElementById('queueEmpty');
  const clearBtn = document.getElementById('clearQueueBtn');

  if (queue.length === 0) {
    queueList.innerHTML = '';
    queueEmpty.style.display = 'block';
    clearBtn.style.display = 'none';
  } else {
    queueEmpty.style.display = 'none';
    clearBtn.style.display = 'inline-block';
    queueList.innerHTML = queue.map((item, index) => `
      <li class="queue-item">
        <span class="queue-item-position">${index + 1}.</span>
        <div class="queue-item-info">
          <span class="queue-item-title">${escapeHtml(item.track_title)}</span>
          ${item.track_artist ? `<span class="queue-item-artist">${escapeHtml(item.track_artist)}</span>` : ''}
          <span class="queue-item-playlist">${escapeHtml(item.playlist_name)}</span>
        </div>
        <button class="queue-remove-button" onclick="removeFromQueue(${index})">Remove</button>
      </li>
    `).join('');
  }
}

async function addToQueue(playlistIndex, trackIndex) {
  try {
    const response = await fetch('/api/queue/add', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ playlist_index: playlistIndex, track_index: trackIndex }),
    });
    if (!response.ok) {
      const error = await response.text();
      console.error('Error adding to queue:', error);
    }
  } catch (error) {
    console.error('Error adding to queue:', error);
  }
}

async function removeFromQueue(index) {
  try {
    const response = await fetch(`/api/queue/remove/${index}`, { method: 'POST' });
    if (!response.ok) {
      const error = await response.text();
      console.error('Error removing from queue:', error);
    }
  } catch (error) {
    console.error('Error removing from queue:', error);
  }
}

async function clearQueue() {
  try {
    await fetch('/api/queue/clear', { method: 'POST' });
  } catch (error) {
    console.error('Error clearing queue:', error);
  }
}

async function uploadPlaylist() {
  const playlistName = document.getElementById('playlistName').value.trim();
  const fileInput = document.getElementById('flacFiles');
  const files = fileInput.files;
  const statusDiv = document.getElementById('uploadStatus');
  const uploadButton = document.querySelector('.upload-button');

  // Validation
  if (!playlistName) {
    statusDiv.textContent = 'Please enter a playlist name';
    statusDiv.className = 'error';
    return;
  }

  if (files.length === 0) {
    statusDiv.textContent = 'Please select at least one FLAC file';
    statusDiv.className = 'error';
    return;
  }

  // Check that all files are FLAC
  for (let i = 0; i < files.length; i++) {
    if (!files[i].name.toLowerCase().endsWith('.flac')) {
      statusDiv.textContent = 'All files must be FLAC format';
      statusDiv.className = 'error';
      return;
    }
  }

  try {
    uploadButton.disabled = true;
    statusDiv.textContent = 'Uploading...';
    statusDiv.className = 'info';

    // Create FormData with files and playlist name
    const formData = new FormData();
    formData.append('playlistName', playlistName);
    for (let i = 0; i < files.length; i++) {
      formData.append(`file-${i}`, files[i], files[i].name);
    }

    const response = await fetch('/api/upload-playlist', {
      method: 'POST',
      body: formData,
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(error || `Upload failed with status ${response.status}`);
    }

    statusDiv.textContent = 'Playlist uploaded successfully!';
    statusDiv.className = 'success';

    // Clear the form
    document.getElementById('playlistName').value = '';
    fileInput.value = '';

    // Clear status after a delay (playlists will be reloaded via SSE)
    setTimeout(() => {
      statusDiv.textContent = '';
    }, 1500);
  } catch (error) {
    console.error('Upload error:', error);
    statusDiv.textContent = `Error: ${error.message}`;
    statusDiv.className = 'error';
  } finally {
    uploadButton.disabled = false;
  }
}

function renderState(state) {
  const symbol = state.mode === "Stopped"
    ? "⏹"
    : (state.mode === "Playing"
      ? "▶️"
      : "⏸️");

  let statusText = `${symbol}`;

  if (state.mode === "Playing" || state.mode === "Paused") {
    if (state.source_info) {
      if (state.source_info.Stream) {
        statusText += ` ${escapeHtml(state.source_info.Stream.stream_name)}`;
      } else if (state.source_info.Track) {
        const info = state.source_info.Track;
        if (info.track_title) {
          statusText += ` ${escapeHtml(info.track_title)}`;
          if (info.artist) {
            statusText += ` - ${escapeHtml(info.artist)}`;
          }
          statusText += ` (${escapeHtml(info.playlist_name)})`;
        } else {
          statusText += ` ${escapeHtml(info.playlist_name)}`;
        }
      }
    }
  } else {
    statusText += ` Stopped`;
  }

  document.getElementById('status').innerHTML = statusText;
}

function connectToEvents() {
  const eventSource = new EventSource('/api/notifications');

  eventSource.onmessage = function (event) {
    const data = JSON.parse(event.data);
    
    if (data.type === 'playerState') {
      renderState(data);
    } else if (data.type === 'libraryUpdated') {
      // Sent repeatedly while a scan fills the library, so both lists grow
      // as folders are read.
      loadStreams();
      loadPlaylists();
    } else if (data.type === 'queueUpdated') {
      renderQueue(data.queue);
    }
  }
};

// Initial load
document.addEventListener('DOMContentLoaded', () => {
  loadStreams();
  loadPlaylists();
  loadQueue();
  connectToEvents();
  fetch('/api/state')
    .then(response => response.json())
    .then(renderState)
    .catch(error => console.error('Error fetching initial status:', error));
}); 