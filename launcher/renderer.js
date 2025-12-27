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

    // Helper function to format game-specific stats lines
    function formatGameStatsLines(p, d, a, apiName, prefix) {
        const lines = [];
        switch (apiName) {
            case "Bedwars":
                lines.push(`    §fWins: §a${(d.wins_bedwars || 0).toLocaleString()} §8| §fLosses: §c${(d.losses_bedwars || 1).toLocaleString()}`);
                lines.push(`    §fFKDR: §6${((d.final_kills_bedwars || 0) / (d.final_deaths_bedwars || 1)).toFixed(2)} §8| §fWLR: §6${((d.wins_bedwars || 0) / (d.losses_bedwars || 1)).toFixed(2)}`);
                break;
            case "SkyWars":
                lines.push(`    §fWins: §a${(d.wins || 0).toLocaleString()} §8| §fLosses: §c${(d.losses || 1).toLocaleString()}`);
                lines.push(`    §fKDR: §6${((d.kills || 0) / (d.deaths || 1)).toFixed(2)} §8| §fWLR: §6${((d.wins || 0) / (d.losses || 1)).toFixed(2)}`);
                break;
            case "Duels":
                const winsKey = prefix ? `${prefix}_wins` : 'wins';
                const lossesKey = prefix ? `${prefix}_losses` : 'losses';
                const killsKey = prefix ? `${prefix}_kills` : 'kills';
                const deathsKey = prefix ? `${prefix}_deaths` : 'deaths';
                const wins = d[winsKey] || 0;
                const losses = d[lossesKey] || 1;
                const kills = d[killsKey] || 0;
                const deaths = d[deathsKey] || 1;
                lines.push(`    §fWins: §a${wins.toLocaleString()} §8| §fLosses: §c${losses.toLocaleString()}`);
                lines.push(`    §fWLR: §6${(wins / (losses || 1)).toFixed(2)} §8| §fKDR: §6${(kills / (deaths || 1)).toFixed(2)}`);
                break;
            case "Walls3":
                lines.push(`    §fWins: §a${(d.wins || 0).toLocaleString()} §8| §fLosses: §c${(d.losses || 1).toLocaleString()}`);
                lines.push(`    §fFinal Kills: §a${(d.final_kills || 0).toLocaleString()} §8| §fFinal Deaths: §c${(d.final_deaths || 1).toLocaleString()}`);
                lines.push(`    §fFKDR: §6${((d.final_kills || 0) / (d.final_deaths || 1)).toFixed(2)} §8| §fWLR: §6${((d.wins || 0) / (d.losses || 1)).toFixed(2)}`);
                break;
            case "UHC":
                lines.push(`    §fWins: §a${(d.wins || 0).toLocaleString()} §8| §fKills: §a${(d.kills || 0).toLocaleString()}`);
                lines.push(`    §fDeaths: §c${(d.deaths || 1).toLocaleString()} §8| §fKDR: §6${((d.kills || 0) / (d.deaths || 1)).toFixed(2)}`);
                break;
            case "MurderMystery":
                lines.push(`    §fGames: §a${(d.games || 0).toLocaleString()} §8| §fWins: §a${(d.wins || 0).toLocaleString()}`);
                lines.push(`    §fKills: §a${(d.kills || 0).toLocaleString()} §8| §fWin Rate: §6${(((d.wins || 0) / (d.games || 1)) * 100).toFixed(2)}%`);
                break;
            case "BuildBattle":
                lines.push(`    §fWins: §a${(d.wins || 0).toLocaleString()} §8| §fGames Played: §e${(d.games_played || 0).toLocaleString()}`);
                lines.push(`    §fScore: §e${(d.score || 0).toLocaleString()} §8| §fWin Rate: §6${(((d.wins || 0) / (d.games_played || 1)) * 100).toFixed(2)}%`);
                break;
            case "Pit":
                const pitStats = p.stats.Pit ? p.stats.Pit.pit_stats_ptl : {};
                lines.push(`    §fKills: §a${(pitStats.kills || 0).toLocaleString()} §8| §fDeaths: §c${(pitStats.deaths || 1).toLocaleString()}`);
                lines.push(`    §fKDR: §6${((pitStats.kills || 0) / (pitStats.deaths || 1)).toFixed(2)}`);
                break;
            case "WoolGames":
                const ww = d.wool_wars || {};
                const stats = ww.stats || {};
                lines.push(`    §fWins: §a${(stats.wins || 0).toLocaleString()} §8| §fGames: §e${(stats.games_played || 0).toLocaleString()}`);
                lines.push(`    §fKills: §a${(stats.kills || 0).toLocaleString()} §8| §fAssists: §b${(stats.assists || 0).toLocaleString()}`);
                lines.push(`    §fWLR: §6${((stats.wins || 0) / ((stats.games_played - (stats.wins || 0)) || 1)).toFixed(2)}`);
                break;
            default:
                lines.push(`    §fWins: §a${(d.wins || 'N/A').toLocaleString()} §8| §fKills: §a${(d.kills || 'N/A').toLocaleString()}`);
                lines.push(`    §fDeaths: §c${(d.deaths || 'N/A').toLocaleString()}`);
                break;
        }
        return lines;
    }
    
    ipcRenderer.on('player-stats-result', (event, result) => {
        const resultsDiv = document.getElementById('stat-search-results');
        resultsDiv.innerHTML = ''; // Clear previous results
        if (result.error) {
            resultsDiv.innerHTML = `<p class="error">${formatMinecraftString(result.error)}</p>`;
            return;
        }
    
        const sendLine = () => `<p class="mc-chat-line">${formatMinecraftString("§d§m----------------------------------------------------")}</p>`;
        
        let outputHtml = sendLine();
        outputHtml += `<p class="mc-chat-line">${formatMinecraftString(`  §d§lPlayer Stats for ${result.game.displayName}`)}</p>`;
        outputHtml += `<p class="mc-chat-line">${formatMinecraftString(`  ${result.stats.rank} ${result.username} §7${result.stats.guild ? `[§e${result.stats.guild}§7]` : ''}`)}</p>`;
        
        const p = result.stats.player; // Full player object from Hypixel
        const d = p.stats?.[result.game.apiName] || {}; // Game mode specific stats
        const a = p.achievements || {}; // Player achievements
    
        const statLines = formatGameStatsLines(p, d, a, result.game.apiName, result.game.prefix);
        statLines.forEach(line => {
            outputHtml += `<p class="mc-chat-line">${formatMinecraftString(line)}</p>`;
        });
    
        outputHtml += sendLine();
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
            resultsDiv.innerHTML = `<p class="error">${formatMinecraftString(result.error)}</p>`;
            return;
        }

        const sendLine = () => `<p class="mc-chat-line">${formatMinecraftString("§d§m----------------------------------------------------")}</p>`;

        let statusHtml = sendLine();
        statusHtml += `<p class="mc-chat-line">${formatMinecraftString(`  §d§lPlayer Status for ${result.username}`)}</p>`;
        statusHtml += `<p class="mc-chat-line">${formatMinecraftString(`  ${result.rank} ${result.username}`)}</p>`;
        
        if (result.online) {
            statusHtml += `<p class="mc-chat-line">${formatMinecraftString(`  §aOnline.`)}</p>`;
            if (!result.hidden) {
                statusHtml += `<p class="mc-chat-line">${formatMinecraftString(`  §fGame: §b${result.gameType}`)}</p>`;
                if (result.mode) statusHtml += `<p class="mc-chat-line">${formatMinecraftString(`  §fMode: §e${result.mode}`)}</p>`;
                if (result.map) statusHtml += `<p class="mc-chat-line">${formatMinecraftString(`  §fMap: §e${result.map}`)}</p>`;
            } else {
                statusHtml += `<p class="mc-chat-line">${formatMinecraftString(`  §7(Status is hidden, game info unavailable)`)}</p>`;
            }
        } else {
            statusHtml += `<p class="mc-chat-line">${formatMinecraftString(`  §cOffline.`)}</p>`;
        }
        statusHtml += sendLine();
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

