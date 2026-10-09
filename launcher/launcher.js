const { app, BrowserWindow, ipcMain, shell } = require('electron');
const path = require('path');
const fs = require('fs');
const yaml = require('yaml');
const crypto = require('crypto');
const readline = require('readline');
const { spawn } = require('child_process');
const { autoUpdater } = require('electron-updater');
const log = require('electron-log');
const http = require('http');
const HypixelHandler = require('../modules/hypixelHandler.js');
const { gameModeMap } = require('../utils/constants.js');
const { API_BASE_URL, WEB_LINK_BASE_URL } = require('../utils/api_constants.js');
const discordRpc = require('../modules/discordRpcHandler.js');
const formatter = require('../formatter.js');
const statFormat = require('../utils/statFormat.js');
const { writeFileAtomic, syncConfigFile, updateConfigFile } = require('../utils/fileUtil.js');
const ApiHandler = require('../utils/apiHandler.js');

// Renderers get no Node access; everything goes through the allowlisted bridge in preload.js.
const SECURE_WEB_PREFERENCES = {
    preload: path.join(__dirname, 'preload.js'),
    contextIsolation: true,
    nodeIntegration: false,
    sandbox: true,
};

// A login callback is only accepted for this long after the user clicked "Login via Browser".
const LOGIN_WINDOW_MS = 10 * 60 * 1000;
const JWT_SHAPE = /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/;

const PLAYING_ACTIVITY = { ...discordRpc.IDLE_ACTIVITY, details: 'Playing', state: 'In Game' };

let mainWindow;
let splashWindow;
let updatePromptWindow;
let proxyProcess;
let statsHandler;
let userDataPath;
let aliasesPath;
let configPath;
let startupWarning = null;

let apiHandler;
let jwtToken = null;

let localAuthCallbackUrl = null;
let pendingLogin = null; // { state, expires }

function sendTo(win, channel, ...args) {
    if (win && !win.isDestroyed()) {
        win.webContents.send(channel, ...args);
    }
}

function isSafeExternalUrl(target) {
    try {
        return new URL(target).protocol === 'https:';
    } catch (e) {
        return false;
    }
}

function lockDownNavigation(win) {
    win.webContents.setWindowOpenHandler(({ url }) => {
        if (isSafeExternalUrl(url)) shell.openExternal(url);
        return { action: 'deny' };
    });
    win.webContents.on('will-navigate', (event) => event.preventDefault());
}

function createSecureWindow(options) {
    const win = new BrowserWindow({ ...options, webPreferences: SECURE_WEB_PREFERENCES });
    lockDownNavigation(win);
    return win;
}

const AUTH_SUCCESS_PAGE = `<!DOCTYPE html>
<html>
<head><title>Authentication Success</title></head>
<body>
    <h1>Login Successful!</h1>
    <p>You can now close this browser window and return to the JagProx Launcher.</p>
    <script>window.close();</script>
</body>
</html>`;

function startAuthServer() {
    let port = 8080;
    const MAX_PORT_ATTEMPTS = 10;
    let attempts = 0;

    const server = http.createServer((req, res) => {
        const parsedUrl = new URL(req.url, 'http://127.0.0.1');
        if (parsedUrl.pathname !== '/auth-callback') {
            res.writeHead(404, { 'Content-Type': 'text/plain' });
            res.end('Not Found');
            return;
        }

        const token = parsedUrl.searchParams.get('token');
        const state = parsedUrl.searchParams.get('state');

        // Any web page can make the browser hit this URL, so only accept a callback the
        // user started from the launcher, at most once, carrying the state we generated.
        if (!pendingLogin || Date.now() > pendingLogin.expires) {
            res.writeHead(403, { 'Content-Type': 'text/plain' });
            res.end('No login is in progress. Start the login from the JagProx launcher.');
            return;
        }
        if (state !== pendingLogin.state) {
            res.writeHead(403, { 'Content-Type': 'text/plain' });
            res.end('Login state mismatch. Please try logging in again.');
            return;
        }
        if (!token || !JWT_SHAPE.test(token)) {
            res.writeHead(400, { 'Content-Type': 'text/plain' });
            res.end('Error: No valid token provided.');
            return;
        }

        pendingLogin = null;
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(AUTH_SUCCESS_PAGE);
        establishSession(token).then(session => sendTo(mainWindow, 'session-state', session));
    });

    server.on('error', (e) => {
        if (e.code === 'EADDRINUSE' && attempts < MAX_PORT_ATTEMPTS) {
            log.warn(`Port ${port} is in use, trying next port.`);
            port++;
            attempts++;
            server.listen(port, '127.0.0.1');
        } else {
            log.error('Auth server error:', e);
        }
    });

    server.on('listening', () => {
        log.info(`Local auth callback server listening on http://127.0.0.1:${port}`);
        localAuthCallbackUrl = `http://127.0.0.1:${port}/auth-callback`;
    });
    server.listen(port, '127.0.0.1');
}

// Verifies a token with the backend before trusting it. Never decodes it locally.
async function establishSession(token) {
    try {
        const response = await fetch(`${API_BASE_URL}/user/profile`, {
            headers: { 'Authorization': `Bearer ${token}` }
        });
        if (!response.ok) {
            throw new Error(`Token validation failed with status ${response.status}`);
        }
        const profile = await response.json();
        jwtToken = token;
        apiHandler.setJwt(token);
        createHypixelHandler();
        log.info('Session established.');
        return { ok: true, token, displayName: String(profile.username || profile.email || 'User') };
    } catch (e) {
        log.warn('Login failed:', e.message);
        clearSession();
        return { ok: false, error: 'Login failed. Please try again.' };
    }
}

function clearSession() {
    jwtToken = null;
    apiHandler.setJwt(null);
    statsHandler = null;
}

function createSplashWindow() {
    splashWindow = createSecureWindow({
        width: 400,
        height: 350,
        frame: false,
        resizable: false,
        show: false,
        backgroundColor: '#1e1e2e',
        icon: path.join(__dirname, 'icon.png')
    });

    splashWindow.loadFile(path.join(__dirname, 'splash.html'));
    splashWindow.once('ready-to-show', () => {
        splashWindow.show();
    });
}

function createUpdatePromptWindow(version) {
    if (updatePromptWindow) return;

    updatePromptWindow = createSecureWindow({
        width: 450,
        height: 220,
        frame: false,
        resizable: false,
        parent: splashWindow || mainWindow,
        modal: true,
        backgroundColor: '#1e1e2e'
    });

    updatePromptWindow.loadFile(path.join(__dirname, 'update-prompt.html'));
    updatePromptWindow.webContents.on('did-finish-load', () => {
        sendTo(updatePromptWindow, 'version-info', version);
    });

    ipcMain.once('update-response', (event, response) => {
        if (response === 'update') {
            autoUpdater.downloadUpdate();
            sendUpdateStatus('Downloading update...');
        } else {
            launchApp(true);
        }
        if (updatePromptWindow) {
            updatePromptWindow.close();
            updatePromptWindow = null;
        }
    });
}

function launchApp(immediate = false) {
    if (mainWindow) return;

    createWindow();
    startAuthServer();

    if (splashWindow) {
        if (immediate) {
            mainWindow.once('ready-to-show', () => {
                if (splashWindow) {
                    splashWindow.close();
                    splashWindow = null;
                }
                mainWindow.show();
            });
        } else {
            sendTo(splashWindow, 'update-status', 'Launching JagProx...');
            setTimeout(() => {
                if (mainWindow && mainWindow.isVisible()) return;
                if (splashWindow) {
                    splashWindow.close();
                    splashWindow = null;
                }
                if (mainWindow) mainWindow.show();
            }, 1500);
        }
    } else {
        mainWindow.once('ready-to-show', () => {
            mainWindow.show();
        });
    }
}

function createWindow() {
    mainWindow = createSecureWindow({
        width: 1000,
        height: 650,
        frame: false,
        show: false,
        autoHideMenuBar: true,
        icon: path.join(__dirname, 'icon.png')
    });

    mainWindow.loadFile(path.join(__dirname, 'index.html'));
    mainWindow.webContents.on('did-finish-load', () => {
        if (startupWarning) sendTo(mainWindow, 'proxy-log', startupWarning);
    });
    mainWindow.on('closed', () => {
        mainWindow = null;
    });
}

function sendUpdateStatus(message) {
    sendTo(splashWindow, 'update-status', message);
    sendTo(mainWindow, 'update-status', message);
}

function handleUpdates() {
    autoUpdater.on('checking-for-update', () => {
        sendUpdateStatus('Checking for updates...');
    });

    autoUpdater.on('update-available', (info) => {
        sendUpdateStatus(`Update v${info.version} available!`);
        createUpdatePromptWindow(info.version);
    });

    autoUpdater.on('update-not-available', () => {
        sendUpdateStatus(`You're on the latest version (v${app.getVersion()}).`);
        launchApp();
    });

    autoUpdater.on('error', (err) => {
        log.error('Update error:', err);
        sendUpdateStatus('Update check failed. See logs for details.');
        launchApp();
    });

    autoUpdater.on('download-progress', (progressObj) => {
        sendUpdateStatus(`Downloading update (${Math.round(progressObj.percent)}%)`);
        sendTo(splashWindow, 'update-progress', progressObj.percent);
    });

    autoUpdater.on('update-downloaded', () => {
        sendUpdateStatus('Restarting...');
        autoUpdater.quitAndInstall(false, true);
    });

    if (app.isPackaged) {
        autoUpdater.checkForUpdates();
    } else {
        setTimeout(launchApp, 2000);
    }
}

app.whenReady().then(() => {
    userDataPath = app.getPath('userData');
    aliasesPath = path.join(userDataPath, 'aliases.json');
    configPath = path.join(userDataPath, 'config.yml');
    apiHandler = new ApiHandler();

    log.transports.file.resolvePathFn = () => path.join(userDataPath, 'logs/main.log');
    autoUpdater.logger = log;
    autoUpdater.logger.transports.file.level = "info";
    autoUpdater.autoDownload = false;

    if (process.platform === 'win32') {
        app.setAppUserModelId("com.jaghack.jagprox");
    }

    log.info('App is ready.');
    initializeAliases();
    syncConfig();
    try {
        const config = readConfig();
        if (config.discord_rpc && config.discord_rpc.enabled) {
            discordRpc.login();
        }
    } catch (e) {
        log.error('Failed to load initial config.yml for Discord RPC:', e);
    }

    createSplashWindow();
    handleUpdates();
});

function syncConfig() {
    const defaultConfigPath = path.join(app.getAppPath(), 'config.yml');
    try {
        const result = syncConfigFile(configPath, defaultConfigPath);
        if (result.status === 'unparseable') {
            startupWarning = `[ERROR] config.yml has a syntax error and was left unchanged (backup: ${result.backupPath}). Fix it or delete it to restore defaults.`;
            log.error(startupWarning, result.error);
        } else if (result.status !== 'unchanged') {
            log.info(`config.yml ${result.status}.`);
        }
    } catch (e) {
        log.error('Failed to sync config.yml:', e);
    }
}

// New installs start with the bundled default aliases.
function initializeAliases() {
    try {
        fs.mkdirSync(path.dirname(aliasesPath), { recursive: true });
        if (!fs.existsSync(aliasesPath)) {
            const defaultsPath = path.join(app.getAppPath(), 'aliases.json');
            const defaults = fs.existsSync(defaultsPath) ? fs.readFileSync(defaultsPath, 'utf8') : '{}';
            log.info(`Initializing file: ${aliasesPath}`);
            writeFileAtomic(aliasesPath, defaults);
        }
    } catch (e) {
        log.error(`Failed to initialize file ${aliasesPath}:`, e);
    }
}

function readConfig() {
    return yaml.parse(fs.readFileSync(configPath, 'utf8')) || {};
}

async function createHypixelHandler() {
    try {
        const apiKey = await apiHandler.getApiKey();
        if (!apiKey) {
            log.warn('Hypixel API Key not available from backend. Stats/commands will be limited.');
        }
        const launcherProxy = {
            env: { apiKey },
            config: {},
            proxyChat: (msg) => log.info(`[STATS] ${formatter.stripColors(msg)}`)
        };
        statsHandler = new HypixelHandler(launcherProxy);
        log.info('Hypixel handler created with key from backend.');
    } catch (error) {
        log.error('Failed to create Hypixel handler:', error);
    }
}

async function ensureStatsHandler() {
    if (!statsHandler && jwtToken) {
        await createHypixelHandler();
    }
    return statsHandler;
}

app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
app.on('will-quit', () => { if (proxyProcess) proxyProcess.kill(); });

ipcMain.on('minimize-window', () => mainWindow?.minimize());
ipcMain.on('maximize-window', () => {
    if (!mainWindow) return;
    if (mainWindow.isMaximized()) {
        mainWindow.unmaximize();
    } else {
        mainWindow.maximize();
    }
});
ipcMain.on('close-window', () => app.quit());

ipcMain.on('start-login', () => {
    if (!localAuthCallbackUrl) {
        log.error('Local auth callback URL not available.');
        return;
    }
    const state = crypto.randomBytes(16).toString('hex');
    pendingLogin = { state, expires: Date.now() + LOGIN_WINDOW_MS };
    const authUrl = new URL(`${WEB_LINK_BASE_URL}/login`);
    authUrl.searchParams.set('redirect_uri', localAuthCallbackUrl);
    authUrl.searchParams.set('state', state);
    shell.openExternal(authUrl.toString());
});

ipcMain.on('restore-session', async (event, token) => {
    if (typeof token !== 'string' || !JWT_SHAPE.test(token)) {
        event.reply('session-state', { ok: false });
        return;
    }
    event.reply('session-state', await establishSession(token));
});

ipcMain.on('clear-jwt', () => {
    clearSession();
    log.info('JWT and stats handler have been cleared due to logout.');
});

ipcMain.on('get-app-version', (event) => {
    event.reply('app-version', app.getVersion());
});

ipcMain.on('check-for-updates', () => {
    if (!app.isPackaged) {
        sendUpdateStatus('Update checks only run in the installed app.');
        return;
    }
    autoUpdater.checkForUpdates().catch(err => {
        log.error('Manual update check failed:', err);
        sendUpdateStatus('Update check failed. See logs for details.');
    });
});

ipcMain.on('get-config', (event) => {
    try {
        const config = readConfig();
        event.reply('config-loaded', {
            auto_gg: config.auto_gg || {},
            discord_rpc: config.discord_rpc || {},
        });
    } catch (e) {
        log.error('Failed to read config.yml:', e);
        event.reply('config-loaded', null);
    }
});

ipcMain.on('save-settings', (event, settings) => {
    try {
        const autoGG = (settings && settings.auto_gg) || {};
        const delay = Number.parseInt(autoGG.delay, 10);
        updateConfigFile(configPath, (config) => {
            config.auto_gg = {
                enabled: autoGG.enabled === true,
                message: String(autoGG.message || 'gg').slice(0, 100),
                delay: Number.isFinite(delay) ? Math.min(Math.max(delay, 0), 60000) : 1500,
            };
        });
        event.reply('settings-saved-reply', true);
    } catch (e) {
        log.error('Failed to save settings:', e);
        event.reply('settings-saved-reply', false);
    }
});

function setDiscordRpcEnabled(enabled) {
    if (enabled) {
        discordRpc.login();
    } else {
        discordRpc.logout();
    }
}

ipcMain.on('toggle-discord-rpc', (event, enabled) => {
    enabled = enabled === true;
    setDiscordRpcEnabled(enabled);
    try {
        updateConfigFile(configPath, (config) => {
            config.discord_rpc = { ...(config.discord_rpc || {}), enabled };
        });
    } catch (e) {
        log.error('Failed to save Discord RPC setting:', e);
    }
});

ipcMain.on('save-api-key', async (event, apiKey) => {
    if (!apiHandler.jwt) {
        log.error('Cannot save API key without a JWT.');
        return event.reply('api-key-saved-reply', false);
    }
    try {
        await apiHandler.saveApiKey(String(apiKey).trim());
        log.info('Hypixel API key saved to backend.');
        await createHypixelHandler();
        event.reply('api-key-saved-reply', true);
    } catch (e) {
        log.error('Failed to save API key to backend:', e);
        event.reply('api-key-saved-reply', false);
    }
});

// Only reports whether a key is set; the key itself never goes to the renderer.
ipcMain.on('get-api-key-status', async (event) => {
    if (!apiHandler.jwt) {
        return event.reply('api-key-status', false);
    }
    try {
        event.reply('api-key-status', !!(await apiHandler.getApiKey()));
    } catch (e) {
        log.error('Failed to get API key from backend:', e);
        event.reply('api-key-status', false);
    }
});

ipcMain.on('get-aliases', (event) => {
    try {
        event.reply('aliases-loaded', JSON.parse(fs.readFileSync(aliasesPath, 'utf8')));
    } catch (e) {
        log.error('Failed to read aliases.json:', e);
        event.reply('aliases-loaded', {});
    }
});

ipcMain.on('save-aliases', (event, aliases) => {
    try {
        const clean = {};
        for (const [key, value] of Object.entries(aliases || {})) {
            const from = String(key).trim().toLowerCase();
            const to = String(value).trim();
            if (from.startsWith('/') && to.startsWith('/')) clean[from] = to;
        }
        writeFileAtomic(aliasesPath, JSON.stringify(clean, null, 4));
        event.reply('aliases-saved-reply', true);
    } catch (e) {
        log.error('Failed to save aliases.json:', e);
        event.reply('aliases-saved-reply', false);
    }
});

ipcMain.on('get-gamemode-list', (event) => {
    const uniqueGamemodes = new Map();
    for (const [key, modeInfo] of Object.entries(gameModeMap)) {
        if (!uniqueGamemodes.has(modeInfo.displayName)) {
            uniqueGamemodes.set(modeInfo.displayName, key);
        }
    }
    const availableGamemodes = Array.from(uniqueGamemodes, ([displayName, key]) => ({ text: displayName, value: key }));
    availableGamemodes.sort((a, b) => a.text.localeCompare(b.text));
    event.reply('gamemode-list-response', availableGamemodes);
});

const NOT_LOGGED_IN = { error: "Hypixel handler not initialized. Is user logged in?" };

// Replies with ready-to-render Minecraft-formatted lines (the renderer only displays them).
ipcMain.on('get-player-stats', async (event, request) => {
    const { name, gamemode } = request || {};
    const handler = await ensureStatsHandler();
    if (!handler) return event.reply('player-stats-result', NOT_LOGGED_IN);

    const result = await handler.getStatsForAPI(String(gamemode), String(name));
    if (result.error) return event.reply('player-stats-result', { error: result.error });

    const player = result.stats.player;
    const guild = result.guild ? ` §7${result.guild}` : '';
    const s = statFormat.getModeStats(player, result.game);
    const lines = [
        `§d§lPlayer Stats for ${result.game.displayName}`,
        `${formatter.formatRank(player)} ${formatter.getPlayerNameColor(player)}${result.username}${guild}`,
        ...(s.hasStats ? statFormat.formatStatLines(s, result.game.apiName) : ['§7No stats found for this game.']),
    ];
    if (s.hasStats && statFormat.hasWinstreaks(result.game.apiName)) {
        lines.push(statFormat.formatWinstreakLine(s));
    }
    event.reply('player-stats-result', { lines });
});

ipcMain.on('get-player-status', async (event, name) => {
    const handler = await ensureStatsHandler();
    if (!handler) return event.reply('player-status-result', NOT_LOGGED_IN);

    const result = await handler.getStatusForAPI(String(name));
    if (result.error) return event.reply('player-status-result', { error: result.error });

    const lines = [
        `§d§lPlayer Status for ${result.username}`,
        `${formatter.formatRank(result.player)} ${formatter.getPlayerNameColor(result.player)}${result.username}`,
    ];
    if (!result.online) {
        lines.push('§cOffline.');
    } else if (result.hidden) {
        lines.push('§aOnline.', '§7(Status is hidden, game info unavailable)');
    } else {
        lines.push('§aOnline.', `§fGame: §b${result.gameType}`);
        if (result.mode) lines.push(`§fMode: §e${result.mode}`);
        if (result.map) lines.push(`§fMap: §e${result.map}`);
    }
    event.reply('player-status-result', { lines });
});

function handleProxyLine(line) {
    if (!line) return;
    if (line.startsWith('[JAGPROX_CHAT]')) {
        sendTo(mainWindow, 'proxy-chat', line.slice('[JAGPROX_CHAT]'.length));
    } else if (line.startsWith('[JAGPROX_STATUS]')) {
        const status = line.slice('[JAGPROX_STATUS]'.length).trim();
        discordRpc.setActivity(status === 'playing'
            ? { ...PLAYING_ACTIVITY, startTimestamp: new Date() }
            : { ...discordRpc.IDLE_ACTIVITY, startTimestamp: new Date() });
    } else if (line.startsWith('[JAGPROX_DRPC]')) {
        setDiscordRpcEnabled(line.slice('[JAGPROX_DRPC]'.length).trim() === 'on');
    } else {
        sendTo(mainWindow, 'proxy-log', line);
    }
}

ipcMain.on('toggle-proxy', (event, request) => {
    const start = !!(request && request.start);
    if (start && !proxyProcess) {
        if (!jwtToken) {
            sendTo(mainWindow, 'proxy-log', '[SYSTEM] You must be logged in to launch the proxy.');
            return;
        }

        const iconPath = app.isPackaged
            ? path.join(process.resourcesPath, "server-icon.png")
            : path.join(__dirname, "..", "server-icon.png");

        const childEnv = {
            ...process.env,
            ELECTRON_RUN_AS_NODE: '1',
            USER_DATA_PATH: userDataPath,
            JAGPROX_JWT: jwtToken,
            ICON_PATH: iconPath
        };

        proxyProcess = spawn(process.execPath, [path.join(app.getAppPath(), 'main.js')], { env: childEnv });

        sendTo(mainWindow, 'proxy-status', 'running');
        // readline reassembles lines that arrive split across stdout chunks.
        readline.createInterface({ input: proxyProcess.stdout }).on('line', handleProxyLine);
        readline.createInterface({ input: proxyProcess.stderr }).on('line', handleProxyLine);

        proxyProcess.on('close', (code) => {
            sendTo(mainWindow, 'proxy-log', `[SYSTEM] Proxy process exited with code ${code}`);
            sendTo(mainWindow, 'proxy-status', 'stopped');
            proxyProcess = null;
            discordRpc.setActivity({ ...discordRpc.IDLE_ACTIVITY, startTimestamp: new Date() });
        });
    } else if (!start && proxyProcess) {
        proxyProcess.kill();
    }
});
