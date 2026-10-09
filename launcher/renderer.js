// Runs without Node access. Talks to the main process only through window.jagprox (preload.js).
const bridge = window.jagprox;

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
        authButtonsContainer.style.display = pageId === 'home' ? 'block' : 'none';
    }

    if (pageId === 'aliases') bridge.send('get-aliases');
    if (pageId === 'settings') {
        bridge.send('get-config');
        bridge.send('get-api-key-status');
    }
}

function updateLoginStatus(username) {
    const loginStatusDisplay = document.getElementById('login-status-display');
    const loginViaBrowserButton = document.getElementById('login-via-browser-button');
    const logoutButton = document.getElementById('logout-button');

    const p = document.createElement('p');
    if (username) {
        // textContent: the username comes from the server and must never be parsed as HTML.
        p.append('Logged in as: ');
        const strong = document.createElement('strong');
        strong.textContent = username;
        p.append(strong);
        loginStatusDisplay.classList.add('logged-in');
        loginViaBrowserButton.style.display = 'none';
        logoutButton.style.display = 'block';
    } else {
        p.textContent = 'Not logged in';
        loginStatusDisplay.classList.remove('logged-in');
        loginViaBrowserButton.style.display = 'block';
        logoutButton.style.display = 'none';
    }
    loginStatusDisplay.replaceChildren(p);
}

function clearStoredSession() {
    localStorage.removeItem('jwt_token');
    localStorage.removeItem('user_display_name');
}

// Escapes HTML first, then turns § codes into spans, so the result is safe for innerHTML.
function formatMinecraftString(text) {
    const html = String(text)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
    const colorMap = {
      '§0': 'mc-color-0', '§1': 'mc-color-1', '§2': 'mc-color-2', '§3': 'mc-color-3',
      '§4': 'mc-color-4', '§5': 'mc-color-5', '§6': 'mc-color-6', '§7': 'mc-color-7',
      '§8': 'mc-color-8', '§9': 'mc-color-9', '§a': 'mc-color-a', '§b': 'mc-color-b',
      '§c': 'mc-color-c', '§d': 'mc-color-d', '§e': 'mc-color-e', '§f': 'mc-color-f'
    };
    const formatMap = {
      '§l': 'mc-format-l', '§o': 'mc-format-o', '§n': 'mc-format-n', '§m': 'mc-format-m'
    };

    let openSpans = 0;
    const parts = html.split(/(§[0-9a-fk-or])/g);
    let result = '';

    for (const part of parts) {
      if (part.startsWith('§')) {
        if (part === '§r') {
          result += '</span>'.repeat(openSpans);
          openSpans = 0;
        } else if (colorMap[part]) {
          result += '</span>'.repeat(openSpans);
          result += `<span class="${colorMap[part]}">`;
          openSpans = 1;
        } else if (formatMap[part]) {
          result += `<span class="${formatMap[part]}">`;
          openSpans++;
        }
      } else {
        result += part;
      }
    }

    result += '</span>'.repeat(openSpans);
    return result;
}

function renderLines(container, result) {
    container.replaceChildren();
    const lines = result.error
        ? [`§c${result.error}`]
        : ['§d§m----------------------------------------------------', ...result.lines.map(l => `  ${l}`), '§d§m----------------------------------------------------'];
    for (const line of lines) {
        const p = document.createElement('p');
        p.className = result.error ? 'error' : 'mc-chat-line';
        p.innerHTML = formatMinecraftString(line);
        container.appendChild(p);
    }
}

function appendFormattedLine(containerId, text) {
    const container = document.getElementById(containerId);
    const entry = document.createElement('div');
    entry.innerHTML = formatMinecraftString(text);
    container.appendChild(entry);
    container.scrollTop = container.scrollHeight;
}

function flashButton(button, success) {
    const original = button.innerHTML;
    button.textContent = success ? 'Saved!' : 'Failed';
    setTimeout(() => {
        button.innerHTML = original;
    }, 2000);
}

bridge.on('session-state', (session) => {
    if (session && session.ok) {
        if (session.token) localStorage.setItem('jwt_token', session.token);
        localStorage.setItem('user_display_name', session.displayName);
        updateLoginStatus(session.displayName);
        switchPage('home');
        document.body.classList.add('sidebar-open');
    } else {
        clearStoredSession();
        updateLoginStatus(null);
        switchPage('home');
        document.body.classList.remove('sidebar-open');
        if (session && session.error) appendFormattedLine('log-output', `§c${session.error}`);
    }
});

function initializeAliasesPage() {
    const aliasesContainer = document.getElementById('aliases-container');
    const modal = document.getElementById('confirm-modal');
    let pendingRemoval = null;

    function createAliasInput(alias = '', command = '') {
        const group = document.createElement('div');
        group.className = 'alias-group';

        const keyInput = document.createElement('input');
        keyInput.type = 'text';
        keyInput.className = 'alias-key';
        keyInput.placeholder = '/command';
        keyInput.value = alias;

        const arrow = document.createElement('span');
        arrow.textContent = '→';

        const valueInput = document.createElement('input');
        valueInput.type = 'text';
        valueInput.className = 'alias-value';
        valueInput.placeholder = '/executed_command';
        valueInput.value = command;

        const removeBtn = document.createElement('button');
        removeBtn.type = 'button';
        removeBtn.className = 'remove-alias-btn';
        removeBtn.innerHTML = '<i class="fas fa-times"></i>';
        removeBtn.addEventListener('click', () => {
            pendingRemoval = group;
            modal.classList.add('show');
        });

        group.append(keyInput, arrow, valueInput, removeBtn);
        aliasesContainer.appendChild(group);
    }

    function closeModal() {
        pendingRemoval = null;
        modal.classList.remove('show');
    }

    document.getElementById('modal-confirm-btn').addEventListener('click', () => {
        if (pendingRemoval) pendingRemoval.remove();
        closeModal();
    });
    document.getElementById('modal-cancel-btn').addEventListener('click', closeModal);

    document.getElementById('add-alias-btn').addEventListener('click', () => createAliasInput());

    document.getElementById('save-aliases-btn').addEventListener('click', () => {
        const newAliases = {};
        aliasesContainer.querySelectorAll('.alias-group').forEach(group => {
            const key = group.querySelector('.alias-key').value.trim();
            const value = group.querySelector('.alias-value').value.trim();
            if (key && value) newAliases[key] = value;
        });
        bridge.send('save-aliases', newAliases);
    });

    bridge.on('aliases-loaded', (aliases) => {
        aliasesContainer.replaceChildren();
        for (const [key, value] of Object.entries(aliases || {})) {
            createAliasInput(key, value);
        }
    });

    bridge.on('aliases-saved-reply', (success) => {
        flashButton(document.getElementById('save-aliases-btn'), success);
        if (success) bridge.send('get-aliases');
    });
}

function initializeSettingsPage() {
    const autoggEnabled = document.getElementById('autogg-enabled');
    const autoggMessage = document.getElementById('autogg-message');
    const autoggDelay = document.getElementById('autogg-delay');
    const discordRpcEnabled = document.getElementById('discord-rpc-enabled');
    const apiKeyInput = document.getElementById('api-key-input');
    const updateInfo = document.getElementById('update-info');

    bridge.on('config-loaded', (config) => {
        if (!config) return;
        const autoGG = config.auto_gg || {};
        autoggEnabled.checked = !!autoGG.enabled;
        autoggMessage.value = autoGG.message || 'gg';
        autoggDelay.value = autoGG.delay ?? 1500;
        discordRpcEnabled.checked = !!(config.discord_rpc && config.discord_rpc.enabled);
    });

    document.getElementById('save-settings-btn').addEventListener('click', () => {
        bridge.send('save-settings', {
            auto_gg: {
                enabled: autoggEnabled.checked,
                message: autoggMessage.value.trim() || 'gg',
                delay: parseInt(autoggDelay.value, 10),
            }
        });
    });

    bridge.on('settings-saved-reply', (success) => {
        flashButton(document.getElementById('save-settings-btn'), success);
    });

    discordRpcEnabled.addEventListener('change', () => {
        bridge.send('toggle-discord-rpc', discordRpcEnabled.checked);
    });

    document.getElementById('toggle-api-key-btn').addEventListener('click', () => {
        apiKeyInput.type = apiKeyInput.type === 'password' ? 'text' : 'password';
    });

    document.getElementById('save-api-key-btn').addEventListener('click', () => {
        const key = apiKeyInput.value.trim();
        if (key) bridge.send('save-api-key', key);
    });

    bridge.on('api-key-saved-reply', (success) => {
        flashButton(document.getElementById('save-api-key-btn'), success);
        if (success) {
            apiKeyInput.value = '';
            bridge.send('get-api-key-status');
        }
    });

    bridge.on('api-key-status', (isSet) => {
        apiKeyInput.placeholder = isSet
            ? 'API key is set. Enter a new one to change it.'
            : 'Enter your Hypixel API key';
    });

    document.getElementById('check-for-updates-btn').addEventListener('click', () => {
        bridge.send('check-for-updates');
    });

    bridge.on('app-version', (version) => {
        updateInfo.textContent = `Current Version: v${version}`;
    });

    bridge.on('update-status', (message) => {
        updateInfo.textContent = message;
    });

    bridge.send('get-app-version');
}

document.addEventListener('DOMContentLoaded', () => {
    document.getElementById('minimize-btn').addEventListener('click', () => bridge.send('minimize-window'));
    document.getElementById('maximize-btn').addEventListener('click', () => bridge.send('maximize-window'));
    document.getElementById('close-btn').addEventListener('click', () => bridge.send('close-window'));

    document.getElementById('burger-menu-btn').addEventListener('click', () => {
        document.body.classList.toggle('sidebar-collapsed');
    });

    document.querySelectorAll('.nav-link').forEach(link => {
        link.addEventListener('click', (event) => {
            event.preventDefault();
            switchPage(link.dataset.page);
        });
    });

    document.getElementById('login-via-browser-button').addEventListener('click', () => {
        bridge.send('start-login');
    });

    document.getElementById('logout-button').addEventListener('click', () => {
        clearStoredSession();
        bridge.send('clear-jwt');
        updateLoginStatus(null);
        switchPage('home');
        document.body.classList.remove('sidebar-open');
    });

    // The main process re-validates a stored token with the backend before using it.
    const existingToken = localStorage.getItem('jwt_token');
    if (existingToken) {
        updateLoginStatus(localStorage.getItem('user_display_name'));
        bridge.send('restore-session', existingToken);
    } else {
        updateLoginStatus(null);
        switchPage('home');
        document.body.classList.remove('sidebar-open');
    }

    document.getElementById('toggle-proxy-btn').addEventListener('click', () => {
        const toggleProxyBtn = document.getElementById('toggle-proxy-btn');
        if (toggleProxyBtn.dataset.status === 'stopped') {
            if (!localStorage.getItem('jwt_token')) {
                alert("You must be logged in to launch the proxy.");
                return;
            }
            bridge.send('toggle-proxy', { start: true });
        } else {
            bridge.send('toggle-proxy', { start: false });
        }
    });

    document.getElementById('copy-log-btn').addEventListener('click', () => {
        const logs = document.getElementById('log-output').innerText;
        navigator.clipboard.writeText(logs).then(() => {
            const copyBtn = document.getElementById('copy-log-btn');
            const originalContent = copyBtn.innerHTML;
            copyBtn.innerHTML = '<i class="fas fa-check"></i> Copied!';
            setTimeout(() => {
                copyBtn.innerHTML = originalContent;
            }, 2000);
        }).catch(err => {
            console.error('Failed to copy logs: ', err);
        });
    });

    document.getElementById('stat-search-btn').addEventListener('click', () => {
        const name = document.getElementById('stat-search-name').value.trim();
        const gamemode = document.getElementById('stat-search-gamemode').value;
        if (name && gamemode) {
            bridge.send('get-player-stats', { name, gamemode });
        } else {
            renderLines(document.getElementById('stat-search-results'), { error: 'Please enter player name and select a gamemode.' });
        }
    });

    bridge.on('player-stats-result', (result) => {
        renderLines(document.getElementById('stat-search-results'), result);
    });

    document.getElementById('status-check-btn').addEventListener('click', () => {
        const name = document.getElementById('status-check-name').value.trim();
        if (name) {
            bridge.send('get-player-status', name);
        } else {
            renderLines(document.getElementById('status-check-results'), { error: 'Please enter player name.' });
        }
    });

    bridge.on('player-status-result', (result) => {
        renderLines(document.getElementById('status-check-results'), result);
    });

    bridge.on('proxy-status', (status) => {
        const toggleProxyBtn = document.getElementById('toggle-proxy-btn');
        if (status === 'running') {
            toggleProxyBtn.dataset.status = 'running';
            toggleProxyBtn.textContent = 'Stop Proxy';
        } else {
            toggleProxyBtn.dataset.status = 'stopped';
            toggleProxyBtn.textContent = 'Launch Proxy';
        }
    });

    bridge.on('proxy-log', (line) => appendFormattedLine('log-output', line));
    bridge.on('proxy-chat', (message) => appendFormattedLine('chat-output', message));

    bridge.on('gamemode-list-response', (gamemodes) => {
        const selector = document.getElementById('stat-search-gamemode');
        if (!selector) return;
        selector.replaceChildren();
        gamemodes.forEach(mode => {
            const option = document.createElement('option');
            option.value = mode.value;
            option.textContent = mode.text;
            selector.appendChild(option);
        });
    });

    bridge.send('get-gamemode-list');

    initializeAliasesPage();
    initializeSettingsPage();
});
