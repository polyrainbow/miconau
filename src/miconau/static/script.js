async function loadStreams() {
    try {
        const response = await fetch('/api/streams');
        const streams = await response.json();
        const streamsContainer = document.getElementById('streams');
        streamsContainer.innerHTML = streams.map(stream => `
            <div class="stream-item" 
                 onclick="playStream(${stream.index})"
                 data-name="${stream.name}">
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

// Initial load
document.addEventListener('DOMContentLoaded', () => {
    loadStreams();
    loadPlaylists();
}); 