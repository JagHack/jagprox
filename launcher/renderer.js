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
}

function updateLoginStatus(username) {
    const loginStatusDisplay = document.getElementById('login-status-display');
    const authButtonsContainer = document.getElementById('auth-buttons-container');
    
    if (username) {
        loginStatusDisplay.innerHTML = `<p>Logged in as: <strong>${username}</strong></p>`;
        loginStatusDisplay.classList.add('logged-in');
        document.getElementById('login-via-browser-button').style.display = 'none';
    } else {
        loginStatusDisplay.innerHTML = `<p>Not logged in</p>`;
        loginStatusDisplay.classList.remove('logged-in');
        document.getElementById('login-via-browser-button').style.display = 'block';
    }
}

ipcRenderer.on('auth-token-received', async (event, token) => {
    localStorage.setItem('jwt_token', token);
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
        localStorage.setItem('user_display_name', 'Guest');
        updateLoginStatus('Guest');
    }
});


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
        
            ipcRenderer.send('get-api-key');
            ipcRenderer.on('api-key-loaded', (event, apiKey) => {
                const apiKeyInput = document.getElementById('api-key-input');
                if (apiKeyInput && apiKey) {
                    apiKeyInput.value = apiKey;
                }
            });
        
            document.getElementById('save-api-key-btn').addEventListener('click', () => {
                const apiKey = document.getElementById('api-key-input').value;
                ipcRenderer.send('save-api-key', apiKey);
            });
        
            document.getElementById('toggle-api-key-btn').addEventListener('click', () => {
                const apiKeyInput = document.getElementById('api-key-input');
                if (apiKeyInput.type === 'password') {
                    apiKeyInput.type = 'text';
                    document.getElementById('toggle-api-key-btn').classList.remove('fa-eye');
                    document.getElementById('toggle-api-key-btn').classList.add('fa-eye-slash');
                } else {
                    apiKeyInput.type = 'password';
                    document.getElementById('toggle-api-key-btn').classList.remove('fa-eye-slash');
                    document.getElementById('toggle-api-key-btn').classList.add('fa-eye');
                }
            });
        
            document.getElementById('login-via-browser-button').addEventListener('click', async () => {
                const localAuthCallbackUrl = ipcRenderer.sendSync('get-local-auth-callback-url');
                if (localAuthCallbackUrl) {
                    const authUrl = `${WEB_LINK_BASE_URL}/login.html?redirect_uri=${encodeURIComponent(localAuthCallbackUrl)}`;
                    ipcRenderer.send('open-external-url', authUrl);
                } else {
                    console.error('Local auth callback URL not available.');
                }
            });
        
            document.getElementById('toggle-proxy-btn').addEventListener('click', () => {
                const toggleProxyBtn = document.getElementById('toggle-proxy-btn');
                const currentStatus = toggleProxyBtn.dataset.status;
        
                if (currentStatus === 'stopped') {
                    ipcRenderer.send('toggle-proxy', true);
                    toggleProxyBtn.dataset.status = 'running';
                    toggleProxyBtn.textContent = 'Stop Proxy';
                } else {
                    ipcRenderer.send('toggle-proxy', false);
                    toggleProxyBtn.dataset.status = 'stopped';
                    toggleProxyBtn.textContent = 'Launch Proxy';
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
        
            const existingToken = localStorage.getItem('jwt_token');
            if (existingToken) {
                try {
                    const response = await fetch(`${API_BASE_URL}/user/profile`, {
                        method: 'GET',
                        headers: {
                            'Authorization': `Bearer ${existingToken}`,
                            'Content-Type': 'application/json'
                        }
                    });
        
                    const profileData = await response.json();
        
                    if (response.ok) {
                        const displayName = profileData.username || profileData.email;
                        localStorage.setItem('user_display_name', displayName);
                        updateLoginStatus(displayName);
                    } else {
                        console.warn('Failed to refresh user profile on startup, token might be invalid/expired.');
                        localStorage.removeItem('jwt_token');
                        localStorage.removeItem('user_display_name');
                        updateLoginStatus(null);
                    }
                } catch (error) {
                    console.error('Error fetching profile on startup:', error);
                    localStorage.removeItem('jwt_token');
                    localStorage.removeItem('user_display_name');
                    updateLoginStatus(null);
                }
        
                switchPage('home');
                document.body.classList.add('sidebar-open');
            } else {
                updateLoginStatus(null);
            }
        });
