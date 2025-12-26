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
    document.querySelector(`.nav-link[data-page="${pageId}"]`).classList.add('active');
    // Removed specific logic for aliases or settings pages.
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
    
    // Immediately send the token to the main process
    ipcRenderer.send('set-jwt', token);

    try {
        const response = await fetch(`${API_BASE_URL}/user/profile`, { // Use API_BASE_URL
            method: 'GET',
            headers: {
                'Authorization': `Bearer ${token}`, // Use the received JWT token
                'Content-Type': 'application/json'
            }
        });

        const profileData = await response.json();

        if (!response.ok) {
            console.error('Failed to fetch user profile:', profileData.message);
            // Fallback to decode token if profile fetch fails
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
        ipcRenderer.send('clear-jwt'); // Tell main process to clear session
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
    // ... (existing event listeners for minimize, maximize, close, burger-menu) ...
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

    // New Login/Logout button handlers
    document.getElementById('login-via-browser-button').addEventListener('click', async () => {
        const localAuthCallbackUrl = ipcRenderer.sendSync('get-local-auth-callback-url');
        if (localAuthCallbackUrl) {
            // Ensure WEB_LINK_BASE_URL points to your actual login page on jagprox.jaghack.com
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
        // Optionally redirect to home or login page within launcher
        switchPage('home');
        document.body.classList.remove('sidebar-open'); // Collapse sidebar on logout
    });


    // Handle initial login status check on startup
    const existingToken = localStorage.getItem('jwt_token');

    if (existingToken) {
        try {
            const response = await fetch(`${API_BASE_URL}/user/profile`, {
                method: 'GET',
                headers: { 'Authorization': `Bearer ${existingToken}` }
            });

            if (!response.ok) {
                // Token is invalid or expired
                throw new Error(`Token validation failed with status: ${response.status}`);
            }
            
            const profileData = await response.json();
            const displayName = profileData.username || profileData.email;
            
            // If we reach here, token is valid
            localStorage.setItem('user_display_name', displayName);
            updateLoginStatus(displayName);
            ipcRenderer.send('set-jwt', existingToken); // Set valid JWT in main process
            
            switchPage('home');
            document.body.classList.add('sidebar-open');

        } catch (error) {
            // This catches network errors or the thrown error from a bad response status
            console.warn('Startup token validation failed:', error.message);
            localStorage.removeItem('jwt_token');
            localStorage.removeItem('user_display_name');
            updateLoginStatus(null);
            ipcRenderer.send('clear-jwt'); // Ensure main process is also cleared
            switchPage('home');
            document.body.classList.remove('sidebar-open');
        }
    } else {
        updateLoginStatus(null); // No token exists
        switchPage('home'); // Show home by default if not logged in
        document.body.classList.remove('sidebar-open'); // Start collapsed if not logged in
    }

    // ... (rest of DOMContentLoaded for toggle-proxy-btn and ipcRenderer.on('proxy-status') ...
    // Ensure proxy-related logic remains.
    document.getElementById('toggle-proxy-btn').addEventListener('click', async () => { // Make it async
        const toggleProxyBtn = document.getElementById('toggle-proxy-btn');
        const currentStatus = toggleProxyBtn.dataset.status;

        if (currentStatus === 'stopped') {
            const token = localStorage.getItem('jwt_token');
            if (!token) {
                alert("You must be logged in to launch the proxy.");
                return;
            }
            
            // The key is no longer pre-fetched. The proxy process will get it on demand.
            ipcRenderer.send('toggle-proxy', { start: true, token: token });
        } else {
            // Stop command remains the same
            ipcRenderer.send('toggle-proxy', { start: false });
        }
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
        logOutput.scrollTop = logOutput.scrollHeight; // Scroll to bottom
    });

    ipcRenderer.on('proxy-chat', (event, message) => {
        const chatOutput = document.getElementById('chat-output');
        const messageElement = document.createElement('div');
        messageElement.innerHTML = formatMinecraftString(message); 
        chatOutput.appendChild(messageElement);
        chatOutput.scrollTop = chatOutput.scrollHeight;
    });

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

