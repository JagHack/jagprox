const { ipcRenderer } = require('electron');

const toggleBtn = document.getElementById('toggle-proxy-btn');
const logOutput = document.getElementById('log-output');
const chatOutput = document.getElementById('chat-output');
const topBar = document.querySelector('.top-bar');
const body = document.querySelector('body');
const navLinks = document.querySelectorAll('.nav-link');
const pages = document.querySelectorAll('.page');
const aliasesContainer = document.getElementById('aliases-container');
const addAliasBtn = document.getElementById('add-alias-btn');
const saveAliasesBtn = document.getElementById('save-aliases-btn');
const modal = document.getElementById('confirm-modal');
const modalConfirmBtn = document.getElementById('modal-confirm-btn');
const modalCancelBtn = document.getElementById('modal-cancel-btn');
let elementToRemove = null;
const apiKeyInput = document.getElementById('api-key-input');
const saveApiKeyBtn = document.getElementById('save-api-key-btn');
const toggleApiKeyBtn = document.getElementById('toggle-api-key-btn');
const statSearchNameInput = document.getElementById('stat-search-name');
const statSearchGamemodeSelect = document.getElementById('stat-search-gamemode');
const statSearchBtn = document.getElementById('stat-search-btn');
const statSearchResults = document.getElementById('stat-search-results');
const statusCheckNameInput = document.getElementById('status-check-name');
const statusCheckBtn = document.getElementById('status-check-btn');
const statusCheckResults = document.getElementById('status-check-results');
const minimizeBtn = document.getElementById('minimize-btn');
const maximizeBtn = document.getElementById('maximize-btn');
const closeBtn = document.getElementById('close-btn');
const saveSettingsBtn = document.getElementById('save-settings-btn');
const autoggEnabledCheckbox = document.getElementById('autogg-enabled');
const autoggMessageInput = document.getElementById('autogg-message');
const autoggDelayInput = document.getElementById('autogg-delay');
const discordRpcEnabledCheckbox = document.getElementById('discord-rpc-enabled');
const checkForUpdatesBtn = document.getElementById('check-for-updates-btn');
const updateInfo = document.getElementById('update-info');
let duelsDivisions = null;
let romanNumerals = null;

minimizeBtn.addEventListener('click', () => ipcRenderer.send('minimize-window'));
maximizeBtn.addEventListener('click', () => ipcRenderer.send('maximize-window'));
closeBtn.addEventListener('click', () => ipcRenderer.send('close-window'));
topBar.addEventListener('click', () => { body.classList.toggle('sidebar-collapsed'); });

function switchPage(pageId) {
    const currentActive = document.querySelector('.page.active');
    const newActive = document.getElementById(pageId);
    if (currentActive && currentActive !== newActive) {
        currentActive.classList.add('exiting');
        currentActive.classList.remove('active');
        setTimeout(() => currentActive.classList.remove('exiting'), 400);
    }
    newActive.classList.add('active');
    navLinks.forEach(link => {
        link.classList.remove('active');
        if (link.dataset.page === pageId) link.classList.add('active');
    });
    if (pageId === 'aliases') ipcRenderer.send('get-aliases');
    if (pageId === 'settings') {
        ipcRenderer.send('get-config');
        ipcRenderer.send('get-app-version');
    }
}

navLinks.forEach(link => { link.addEventListener('click', (e) => { e.preventDefault(); switchPage(link.dataset.page); }); });
toggleBtn.addEventListener('click', () => { ipcRenderer.send('toggle-proxy', toggleBtn.dataset.status !== 'running'); });
saveApiKeyBtn.addEventListener('click', () => { if (apiKeyInput.value.trim()) ipcRenderer.send('save-api-key', apiKeyInput.value.trim()); });
toggleApiKeyBtn.addEventListener('click', () => {
    apiKeyInput.type = apiKeyInput.type === 'password' ? 'text' : 'password';
    toggleApiKeyBtn.classList.toggle('fa-eye');
    toggleApiKeyBtn.classList.toggle('fa-eye-slash');
});
statSearchBtn.addEventListener('click', () => {
    const name = statSearchNameInput.value.trim();
    const gamemode = statSearchGamemodeSelect.value;
    if (!name) return;
    statSearchBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i>';
    ipcRenderer.send('get-player-stats', { name, gamemode });
});
statusCheckBtn.addEventListener('click', () => {
    const name = statusCheckNameInput.value.trim();
    if (!name) return;
    statusCheckBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i>';
    ipcRenderer.send('get-player-status', name);
});

saveSettingsBtn.addEventListener('click', () => {
    const settings = {
        auto_gg: {
            enabled: autoggEnabledCheckbox.checked,
            message: autoggMessageInput.value || "gg",
            delay: parseInt(autoggDelayInput.value) || 1500
        }
    };
    ipcRenderer.send('save-settings', settings);
});

discordRpcEnabledCheckbox.addEventListener('change', () => {
    ipcRenderer.send('toggle-discord-rpc', discordRpcEnabledCheckbox.checked);
});



checkForUpdatesBtn.addEventListener('click', () => {
    ipcRenderer.send('check-for-updates');
});

ipcRenderer.on('app-version', (event, version) => {
    updateInfo.innerText = `Current Version: v${version}`;
});

ipcRenderer.on('update-status', (event, message) => {
    updateInfo.innerText = message;
});

ipcRenderer.on('config-loaded', (event, config) => {
    if (config) {
        if (config.auto_gg) {
            autoggEnabledCheckbox.checked = config.auto_gg.enabled || false;
            autoggMessageInput.value = config.auto_gg.message || "gg";
            autoggDelayInput.value = config.auto_gg.delay || 1500;
        }
        if (config.discord_rpc) {
            discordRpcEnabledCheckbox.checked = config.discord_rpc.enabled || false;
        }
    }
});

ipcRenderer.on('proxy-status', (event, status) => {
    toggleBtn.dataset.status = status;
    toggleBtn.textContent = status === 'running' ? 'Stop Proxy' : 'Launch Proxy';
});
ipcRenderer.on('proxy-log', (event, message) => { appendToScrollbox(logOutput, message); });
ipcRenderer.on('proxy-chat', (event, message) => { appendToScrollbox(chatOutput, message, true); });

ipcRenderer.on('player-stats-result', (event, result) => {
    statSearchBtn.innerHTML = '<i class="fas fa-search"></i> Search';
    statSearchResults.innerHTML = '';
    if (result.error) {
        statSearchResults.innerHTML = `<p class="result-error">${result.error}</p>`;
        return;
    }
    const player = result.stats.player;
    const d = (player.stats && player.stats[result.game.apiName]) || {};
    const a = player.achievements || {};
    let statsHtml = `<div class="result-grid">`;
    switch (result.game.apiName) {
        case 'Bedwars':
            statsHtml += `<span>Level:</span><span>${(a.bedwars_level || 0).toLocaleString()}✫</span>`;
            statsHtml += `<span>Wins:</span><span>${(d.wins_bedwars || 0).toLocaleString()}</span>`;
            statsHtml += `<span>WLR:</span><span>${((d.wins_bedwars || 0) / (d.losses_bedwars || 1)).toFixed(2)}</span>`;
            statsHtml += `<span>Final Kills:</span><span>${(d.final_kills_bedwars || 0).toLocaleString()}</span>`;
            statsHtml += `<span>Final Deaths:</span><span>${(d.final_deaths_bedwars || 1).toLocaleString()}</span>`;
            statsHtml += `<span>FKDR:</span><span>${((d.final_kills_bedwars || 0) / (d.final_deaths_bedwars || 1)).toFixed(2)}</span>`;
            break;
        case 'SkyWars':
            statsHtml += `<span>Level:</span><span>${(player.stats.SkyWars && player.stats.SkyWars.levelFormatted) || 'N/A'}</span>`;
            statsHtml += `<span>Wins:</span><span>${(d.wins || 0).toLocaleString()}</span>`;
            statsHtml += `<span>Kills:</span><span>${(d.kills || 0).toLocaleString()}</span>`;
            statsHtml += `<span>Deaths:</span><span>${(d.deaths || 1).toLocaleString()}</span>`;
            statsHtml += `<span>KDR:</span><span>${((d.kills || 0) / (d.deaths || 1)).toFixed(2)}</span>`;
            break;
        case 'Duels':
            const prefix = result.game.prefix;
            const wins = d[prefix ? `${prefix}_wins` : 'wins'] || 0;
            const losses = d[prefix ? `${prefix}_losses` : 'losses'] || 1;

            let prestige = 0;
            const gameName = result.game.displayName.split(' ')[0].toLowerCase();
            if (gameName && gameName !== 'duels') {
                for (const key in d) {
                    if (key.startsWith(gameName) && key.endsWith('_title_prestige')) {
                        prestige++;
                    }
                }
            } else { // for "Duels" (overall)
                for (const key in d) {
                    if (key.startsWith('all_modes_') && key.endsWith('_title_prestige')) {
                        prestige++;
                    }
                }
            }

            let division = 'N/A';
            if (duelsDivisions && romanNumerals) {
                const divisions = Object.keys(duelsDivisions);
                let highestDivision = 'N/A';
                let divisionLevel = '';

                const playerWins = wins;

                for (let i = divisions.length - 1; i >= 0; i--) {
                    const divisionName = divisions[i];
                    const divisionData = duelsDivisions[divisionName];
                    if (playerWins >= divisionData.wins) {
                        highestDivision = divisionName.charAt(0).toUpperCase() + divisionName.slice(1);
                        let level = 1;
                        for (let j = 1; j < divisionData.levels; j++) {
                            if (playerWins >= divisionData.wins + (j * divisionData.step)) {
                                level = j + 1;
                            }
                        }
                        divisionLevel = romanNumerals[level - 1];
                        break;
                    }
                }
                division = highestDivision !== 'N/A' ? `${highestDivision} ${divisionLevel}` : 'N/A';
            }

            statsHtml += `<span>Prestige:</span><span>${prestige}✫</span>`;
            statsHtml += `<span>Division:</span><span>${division}</span>`;
            statsHtml += `<span>Wins:</span><span>${wins.toLocaleString()}</span>`;
            statsHtml += `<span>WLR:</span><span>${(wins / losses).toFixed(2)}</span>`;
            break;
        default: statsHtml += `<span>Wins:</span><span>${(d.wins || 'N/A').toLocaleString()}</span>`;
    }
    statsHtml += `</div>`;

    const skinSrc = result.skinUrl ? result.skinUrl.replace("http://", "https://") : `https://crafatar.com/avatars/${result.uuid}?size=512&overlay=true`;

    statSearchResults.innerHTML = `
        <div class="result-header"><h3>${result.username} - ${result.game.displayName}</h3></div>
        ${statsHtml}
        <div class="result-avatar-container"><img src="${skinSrc}" alt="Player avatar"></div>
    `;
});

ipcRenderer.on('player-status-result', (event, result) => {
    statusCheckBtn.innerHTML = '<i class="fas fa-search"></i> Check Status';
    statusCheckResults.innerHTML = '';
    if (result.error) {
        statusCheckResults.innerHTML = `<p class="result-error">${result.error}</p>`;
        return;
    }
    let statusHtml = result.online ? `<span class="status-online">Online</span>` : `<span class="status-offline">Offline</span>`;
    let gameInfo = '';
    if (result.online && !result.hidden) {
        gameInfo = `<span>Game:</span><span>${result.gameType || 'N/A'}</span>
                    <span>Mode:</span><span>${result.mode || 'N/A'}</span>`;
    } else if (result.online && result.hidden) {
        gameInfo = `<span>Game:</span><span>Hidden</span>`;
    }
    const skinSrc = result.skinUrl ? result.skinUrl.replace("http://", "https://") : `https://crafatar.com/avatars/${result.uuid}?size=512&overlay=true`;

    statusCheckResults.innerHTML = `
        <div class="result-header"><h3>${result.username}</h3></div>
        <div class="result-grid">
            <span>Status:</span>${statusHtml}
            ${gameInfo}
        </div>
        <div class="result-avatar-container"><img src="${skinSrc}" alt="Player avatar"></div>
    `;
});

ipcRenderer.on('gamemodes-loaded', (event, gamemodes) => {
    statSearchGamemodeSelect.innerHTML = gamemodes
        .map(gm => `<option value="${gm.value}">${gm.text}</option>`).join('');
});

ipcRenderer.on('duels-divisions-loaded', (event, data) => {
    duelsDivisions = data.duelsDivisions;
    romanNumerals = data.romanNumerals;
});

function appendToScrollbox(element, message, parseColors = false) {
    const line = document.createElement('div');
    if (parseColors) {
        line.innerHTML = minecraftColorParser(message);
    } else {
        line.textContent = message;
    }
    element.appendChild(line);
    element.scrollTop = element.scrollHeight;
}

function minecraftColorParser(text) {
    const segments = text.split('§');
    let html = '';
    let activeClasses = [];

    segments.forEach((segment, index) => {
        if (index === 0) {
            if (segment) html += `<span>${segment}</span>`;
            return;
        }

        const code = segment.charAt(0).toLowerCase();
        const content = segment.substring(1);

        if ('0123456789abcdef'.includes(code)) {
            activeClasses = activeClasses.filter(c => !c.startsWith('mc-color-'));
            activeClasses.push(`mc-color-${code}`);
        } else if ('lmnor'.includes(code)) {
            if (code === 'r') {
                activeClasses = [];
            } else {
                activeClasses = activeClasses.filter(c => c !== `mc-format-${code}`);
                activeClasses.push(`mc-format-${code}`);
            }
        }

        if (content) {
            html += `<span class="${activeClasses.join(' ')}">${content}</span>`;
        }
    });

    return html;
}

function createAliasInput(alias = '', command = '') {
    const div = document.createElement('div');
    div.className = 'alias-group';
    div.innerHTML = `
        <input type="text" class="alias-key" value="${alias}" placeholder="/command">
        <span>></span>
        <input type="text" class="alias-value" value="${command}" placeholder="/executed_command">
        <button type="button" class="remove-alias-btn"><i class="fas fa-times"></i></button>
    `;
    aliasesContainer.appendChild(div);
    div.querySelector('.remove-alias-btn').addEventListener('click', () => openConfirmationModal(div));
}

function openConfirmationModal(element) { elementToRemove = element; modal.classList.add('show'); }
function closeConfirmationModal() { modal.classList.remove('show'); elementToRemove = null; }

addAliasBtn.addEventListener('click', () => createAliasInput());
modalConfirmBtn.addEventListener('click', () => { if (elementToRemove) elementToRemove.remove(); closeConfirmationModal(); });
modalCancelBtn.addEventListener('click', closeConfirmationModal);

ipcRenderer.on('aliases-loaded', (event, aliases) => {
    aliasesContainer.innerHTML = '';
    for (const key in aliases) { createAliasInput(key, aliases[key]); }
});

saveAliasesBtn.addEventListener('click', () => {
    const newAliases = {};
    document.querySelectorAll('.alias-group').forEach(group => {
        const key = group.querySelector('.alias-key').value.trim();
        const value = group.querySelector('.alias-value').value.trim();
        if (key && value) { newAliases[key] = value; }
    });
    ipcRenderer.send('save-aliases', newAliases);
});

function showSaveFeedback(button, success) {
    const originalText = button.innerHTML;
    if (success) {
        button.innerHTML = '<i class="fas fa-check"></i> Saved!';
        button.style.borderColor = 'var(--purple-primary)';
    } else {
        button.innerHTML = '<i class="fas fa-times"></i> Error!';
        button.style.borderColor = 'var(--red-stop)';
    }
    setTimeout(() => {
        button.innerHTML = originalText;
        button.style.borderColor = 'var(--purple-primary)';
    }, 2000);
}

ipcRenderer.on('settings-saved-reply', (event, success) => showSaveFeedback(saveSettingsBtn, success));
ipcRenderer.on('aliases-saved-reply', (event, success) => showSaveFeedback(saveAliasesBtn, success));
ipcRenderer.on('api-key-saved-reply', (event, success) => showSaveFeedback(saveApiKeyBtn, success));

ipcRenderer.on('api-key-loaded', (event, apiKey) => {
    if (apiKey) {
        apiKeyInput.value = apiKey;
        apiKeyInput.placeholder = "API Key is set. Enter a new one to change it.";
    }
});

ipcRenderer.send('get-api-key');
ipcRenderer.send('get-gamemodes');
ipcRenderer.send('get-duels-divisions');