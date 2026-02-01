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
      // Wrapper div for the playlist row
      const playlistWrapper = document.createElement('div');
      playlistWrapper.className = 'playlist';

      // Details element for expandable tracks
      const details = document.createElement('details');
      details.className = 'playlist-details';

      const summary = document.createElement('summary');
      summary.className = 'playlist-summary';
      
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
        playPlaylist(playlist.index);
      });
      details.appendChild(innerPlayBtn);

      const trackList = document.createElement('ul');
      trackList.className = 'track-list';
      trackList.innerHTML = '<li>Loading...</li>';
      details.appendChild(trackList);

      // Play button outside details/summary for accessibility
      const playBtn = document.createElement('button');
      playBtn.textContent = '▶';
      playBtn.className = 'playlist-play-button';
      playBtn.addEventListener('click', () => {
        playPlaylist(playlist.index);
      });

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
                `<li>
                  <button class="track-play-button" onclick="playPlaylistTrack(${playlist.index}, ${track.index})">
                    <span class="track-title">${escapeHtml(track.title)}</span>
                    ${track.artist ? `<span class="track-artist">${escapeHtml(track.artist)}</span>` : ''}
                  </button>
                  <button class="track-queue-button" onclick="addToQueue(${playlist.index}, ${track.index})">
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
      playlistsContainer.appendChild(playlistWrapper);
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
      } else if (state.source_info.Playlist) {
        const info = state.source_info.Playlist;
        if (info.track_title) {
          statusText += ` ${escapeHtml(info.track_title)}`;
          if (info.artist) {
            statusText += ` - ${escapeHtml(info.artist)}`;
          }
          statusText += ` (${escapeHtml(info.playlist_name)})`;
        } else {
          statusText += ` ${escapeHtml(info.playlist_name)}`;
        }
      } else if (state.source_info.Queue) {
        const info = state.source_info.Queue;
        statusText += ` ${escapeHtml(info.track_title)}`;
        if (info.artist) {
          statusText += ` - ${escapeHtml(info.artist)}`;
        }
        statusText += ` (Queue)`;
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