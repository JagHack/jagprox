const { ipcRenderer } = require("electron");
const { WEB_LINK_BASE_URL } = require("../utils/api_constants.js");
const { API_BASE_URL } = require("../utils/api_constants.js");

function switchPage(pageId) {
    document.querySelectorAll('.page').forEach(page => {
        page.classList.remove('active');
    });
    document.getElementById(pageId).classList.add('active');

    document.querySelectorAll('.nav-link').forEach(link => {
        link.classList.remove('active');
    });
    const activeLink = document.querySelector(`.nav-link[data-page="${pageId}"]`);
    if (activeLink) {
        activeLink.classList.add('active');
    }

    const authButtonsContainer = document.getElementById('auth-buttons-container');
    if (authButtonsContainer) {
        if (pageId === 'home') {
            authButtonsContainer.style.display = 'block';
        } else {
            authButtonsContainer.style.display = 'none';
        }
    }
}

function updateLoginStatus(username) {
    const loginStatusDisplay = document.getElementById('login-status-display');
    const loginViaBrowserButton = document.getElementById('login-via-browser-button');
    const logoutButton = document.getElementById('logout-button');

    if (username) {
        loginStatusDisplay.innerHTML = `<p>Logged in as: <strong>${username}</strong></p>`;
        loginStatusDisplay.classList.add('logged-in');
        loginViaBrowserButton.style.display = 'none';
        logoutButton.style.display = 'block';
    } else {
        loginStatusDisplay.innerHTML = `<p>Not logged in</p>`;
        loginStatusDisplay.classList.remove('logged-in');
        loginViaBrowserButton.style.display = 'block';
        logoutButton.style.display = 'none';
    }
}

ipcRenderer.on('auth-token-received', async (event, token) => {
    localStorage.setItem('jwt_token', token);
    ipcRenderer.send('set-jwt', token);

    try {
        const response = await fetch(`${API_BASE_URL}/user/profile`, {
            method: 'GET',
            headers: {
                'Authorization': `Bearer ${token}`,
                'Content-Type': 'application/json'
            }
        });

        const profileData = await response.json();

        if (!response.ok) {
            console.error('Failed to fetch user profile:', profileData.message);
            try {
                const decodedToken = JSON.parse(atob(token.split('.')[1]));
                localStorage.setItem('user_display_name', decodedToken.email);
                updateLoginStatus(decodedToken.email);
            } catch (decodeError) {
                console.error('Failed to decode token for fallback:', decodeError);
                localStorage.setItem('user_display_name', 'User');
                updateLoginStatus('User');
            }
        } else {
            console.log('User Profile:', profileData);
            const displayName = profileData.username || profileData.email;
            localStorage.setItem('user_display_name', displayName);
            updateLoginStatus(displayName);
        }

        switchPage('home');
        document.body.classList.add('sidebar-open');

    } catch (error) {
        console.error('Critical Error during profile fetch:', error);
        localStorage.removeItem('jwt_token');
        localStorage.removeItem('user_display_name');
        updateLoginStatus(null);
        ipcRenderer.send('clear-jwt');
    }
});
function formatMinecraftString(html) {
    html = html.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    const colorMap = {
      '§0': 'mc-color-0', '§1': 'mc-color-1', '§2': 'mc-color-2', '§3': 'mc-color-3',
      '§4': 'mc-color-4', '§5': 'mc-color-5', '§6': 'mc-color-6', '§7': 'mc-color-7',
      '§8': 'mc-color-8', '§9': 'mc-color-9', '§a': 'mc-color-a', '§b': 'mc-color-b',
      '§c': 'mc-color-c', '§d': 'mc-color-d', '§e': 'mc-color-e', '§f': 'mc-color-f'
    };
    const formatMap = {
      '§l': 'mc-format-l', '§o': 'mc-format-o', '§n': 'mc-format-n', '§m': 'mc-format-m'
    };
  
    let openSpans = [];
    const parts = html.split(/(§[0-9a-fl-or])/g);
    let result = '';
  
    for (const part of parts) {
      if (part.startsWith('§')) {
        if (part === '§r') {
          result += '</span>'.repeat(openSpans.length);
          openSpans = [];
        } else if (colorMap[part]) {
          result += '</span>'.repeat(openSpans.length);
          openSpans = [];
          result += `<span class="${colorMap[part]}">`;
          openSpans.push('</span>');
        } else if (formatMap[part]) {
          result += `<span class="${formatMap[part]}">`;
          openSpans.push('</span>');
        }
      } else {
        result += part;
      }
    }
  
    result += '</span>'.repeat(openSpans.length);
    return result;
}

document.addEventListener('DOMContentLoaded', async () => {
    document.getElementById('minimize-btn').addEventListener('click', () => {
        ipcRenderer.send('minimize-window');
    });

    document.getElementById('maximize-btn').addEventListener('click', () => {
        ipcRenderer.send('maximize-window');
    });

    document.getElementById('close-btn').addEventListener('click', () => {
        ipcRenderer.send('close-window');
    });
    // Burger menu button
    document.getElementById('burger-menu-btn').addEventListener('click', () => {
        document.body.classList.toggle('sidebar-collapsed');
    });

    document.querySelectorAll('.nav-link').forEach(link => {
        link.addEventListener('click', (event) => {
            event.preventDefault();
            const pageId = link.dataset.page;
            switchPage(pageId);
        });
    });

    document.getElementById('login-via-browser-button').addEventListener('click', async () => {
        const localAuthCallbackUrl = ipcRenderer.sendSync('get-local-auth-callback-url');
        if (localAuthCallbackUrl) {
            const authUrl = `${WEB_LINK_BASE_URL}/login?redirect_uri=${encodeURIComponent(localAuthCallbackUrl)}`;
            ipcRenderer.send('open-external-url', authUrl);
        } else {
            console.error('Local auth callback URL not available.');
        }
    });

    document.getElementById('logout-button').addEventListener('click', async () => {
        localStorage.removeItem('jwt_token');
        localStorage.removeItem('user_display_name');
        ipcRenderer.send('clear-jwt'); // Tell main process to clear its session state
        updateLoginStatus(null);
        switchPage('home');
        document.body.classList.remove('sidebar-open');
    });

    const existingToken = localStorage.getItem('jwt_token');

    if (existingToken) {
        try {
            const response = await fetch(`${API_BASE_URL}/user/profile`, {
                method: 'GET',
                headers: { 'Authorization': `Bearer ${existingToken}` }
            });

            if (!response.ok) {
                throw new Error(`Token validation failed with status: ${response.status}`);
            }
            
            const profileData = await response.json();
            const displayName = profileData.username || profileData.email;
            
            localStorage.setItem('user_display_name', displayName);
            updateLoginStatus(displayName);
            ipcRenderer.send('set-jwt', existingToken);
            
            switchPage('home');
            document.body.classList.add('sidebar-open');

        } catch (error) {
            console.warn('Startup token validation failed:', error.message);
            localStorage.removeItem('jwt_token');
            localStorage.removeItem('user_display_name');
            updateLoginStatus(null);
            ipcRenderer.send('clear-jwt');
            switchPage('home');
            document.body.classList.remove('sidebar-open');
        }
    } else {
        updateLoginStatus(null);
        switchPage('home');
        document.body.classList.remove('sidebar-open');
    }
    document.getElementById('toggle-proxy-btn').addEventListener('click', async () => {
        const toggleProxyBtn = document.getElementById('toggle-proxy-btn');
        const currentStatus = toggleProxyBtn.dataset.status;

        if (currentStatus === 'stopped') {
            const token = localStorage.getItem('jwt_token');
            if (!token) {
                alert("You must be logged in to launch the proxy.");
                return;
            }
            
            ipcRenderer.send('toggle-proxy', { start: true, token: token });
        } else {
            ipcRenderer.send('toggle-proxy', { start: false });
        }
    });

    // Stat Search functionality
    document.getElementById('stat-search-btn').addEventListener('click', () => {
        const name = document.getElementById('stat-search-name').value;
        const gamemode = document.getElementById('stat-search-gamemode').value;
        if (name && gamemode) {
            ipcRenderer.send('get-player-stats', { name, gamemode });
        } else {
            document.getElementById('stat-search-results').innerHTML = '<p class="error">Please enter player name and select a gamemode.</p>';
        }
    });

    ipcRenderer.on('player-stats-result', (event, result) => {
        const resultsDiv = document.getElementById('stat-search-results');
        resultsDiv.innerHTML = ''; // Clear previous results
        if (result.error) {
            resultsDiv.innerHTML = `<p class="error">${result.error}</p>`;
            return;
        }
        
        // Display formatted stats
        let outputHtml = '<h3>Player Stats:</h3>';
        outputHtml += `<p><strong>${result.username}</strong> (${result.uuid})</p>`;
        outputHtml += `<p>Game: ${result.game.displayName}</p>`;
        outputHtml += `<p>Wins: ${result.stats.player?.stats?.[result.game.apiName]?.wins || 'N/A'}</p>`;
        outputHtml += `<p>Kills: ${result.stats.player?.stats?.[result.game.apiName]?.kills || 'N/A'}</p>`;
        // Add more stats as needed from result.stats
        resultsDiv.innerHTML = outputHtml;
    });


    // Status Check functionality
    document.getElementById('status-check-btn').addEventListener('click', () => {
        const name = document.getElementById('status-check-name').value;
        if (name) {
            ipcRenderer.send('get-player-status', name);
        } else {
            document.getElementById('status-check-results').innerHTML = '<p class="error">Please enter player name.</p>';
        }
    });

    ipcRenderer.on('player-status-result', (event, result) => {
        const resultsDiv = document.getElementById('status-check-results');
        resultsDiv.innerHTML = ''; // Clear previous results
        if (result.error) {
            resultsDiv.innerHTML = `<p class="error">${result.error}</p>`;
            return;
        }

        let statusHtml = '<h3>Player Status:</h3>';
        statusHtml += `<p><strong>${result.username}</strong></p>`;
        statusHtml += `<p>Status: ${result.online ? '§aOnline' : '§cOffline'}</p>`;
        if (result.online && !result.hidden) {
            statusHtml += `<p>Game: ${result.gameType || 'N/A'}</p>`;
            statusHtml += `<p>Mode: ${result.mode || 'N/A'}</p>`;
            statusHtml += `<p>Map: ${result.map || 'N/A'}</p>`;
        } else if (result.online && result.hidden) {
            statusHtml += `<p>(Status is hidden)</p>`;
        }
        resultsDiv.innerHTML = statusHtml;
    });


    ipcRenderer.on('proxy-status', (event, status) => {
        const toggleProxyBtn = document.getElementById('toggle-proxy-btn');
        if (status === 'running') {
            toggleProxyBtn.dataset.status = 'running';
            toggleProxyBtn.textContent = 'Stop Proxy';
        } else {
            toggleProxyBtn.dataset.status = 'stopped';
            toggleProxyBtn.textContent = 'Launch Proxy';
        }
    });

    ipcRenderer.on('proxy-log', (event, log) => {
        const logOutput = document.getElementById('log-output');
        const logEntry = document.createElement('div');
        logEntry.innerHTML = formatMinecraftString(log);
        logOutput.appendChild(logEntry);
        logOutput.scrollTop = logOutput.scrollHeight;
    });

    ipcRenderer.on('proxy-chat', (event, message) => {
        const chatOutput = document.getElementById('chat-output');
        const messageElement = document.createElement('div');
        messageElement.innerHTML = formatMinecraftString(message); 
        chatOutput.appendChild(messageElement);
        chatOutput.scrollTop = chatOutput.scrollHeight;
    });

    ipcRenderer.on('gamemode-list-response', (event, gamemodes) => {
        const selector = document.getElementById('stat-search-gamemode');
        if (selector) {
            selector.innerHTML = ''; // Clear existing options
            gamemodes.forEach(mode => {
                const option = document.createElement('option');
                option.value = mode.value;
                option.textContent = mode.text;
                selector.appendChild(option);
            });
        }
    });

    ipcRenderer.send('get-gamemode-list'); // Request gamemodes on startup

    initializeSettingsPage();
});

function initializeSettingsPage() {
    const checkForUpdatesBtn = document.getElementById('check-for-updates-btn');
    const updateInfo = document.getElementById('update-info');

    checkForUpdatesBtn.addEventListener('click', () => {
        ipcRenderer.send('check-for-updates');
    });

    ipcRenderer.on('app-version', (event, version) => {
        updateInfo.innerText = `Current Version: v${version}`;
    });

    ipcRenderer.on('update-status', (event, message) => {
        updateInfo.innerText = message;
    });

    ipcRenderer.send('get-app-version');
}

