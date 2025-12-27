const { app, BrowserWindow, ipcMain, Notification, shell } = require('electron');
const path = require('path');
const fs = require('fs');
const yaml = require('yaml');
const { spawn } = require('child_process');
const { autoUpdater } = require('electron-updater');
const log = require('electron-log');
const http = require('http');
const url = require('url');
const HypixelHandler = require('../modules/hypixelHandler.js');
const { gameModeMap } = require('../utils/constants.js');
const discordRpc = require('../modules/discordRpcHandler.js');
const formatter = require('../formatter.js');
const ApiHandler = require('../utils/apiHandler.js');

let mainWindow;
let proxyProcess;
let statsHandler;
let userDataPath;
let aliasesPath;
let configPath;

let apiHandler;
let jwtToken = null;

let config = {};
let localAuthCallbackUrl = null;

function createWindow() {
    mainWindow = new BrowserWindow({
        width: 1000,
        height: 650,
        frame: false,
        webPreferences: {
            nodeIntegration: true,
            contextIsolation: false,
        },
        autoHideMenuBar: true,
        icon: path.join(__dirname, 'icon.ico')
    });

    mainWindow.loadFile(path.join(__dirname, 'index.html'));
    mainWindow.webContents.setWindowOpenHandler(({ url }) => {
        shell.openExternal(url);
        return { action: 'deny' };
    });
}

function startAuthServer() {
    let port = 8080;
    const MAX_PORT_ATTEMPTS = 10;
    let attempts = 0;

    const server = http.createServer((req, res) => {
        const parsedUrl = url.parse(req.url, true);

        if (parsedUrl.pathname === '/auth-callback') {
            const token = parsedUrl.query.token;

            if (token) {
                if (mainWindow && mainWindow.webContents) {
                    mainWindow.webContents.send('auth-token-received', token);
                }
                res.writeHead(200, { 'Content-Type': 'text/html' });
                res.end(`
                    <!DOCTYPE html>
                    <html>
                    <head><title>Authentication Success</title></head>
                    <body>
                        <h1>Login Successful!</h1>
                        <p>You can now close this browser window and return to the JagProx Launcher.</p>
                        <script>window.close();</script>
                    </body>
                    </html>
                `);
            } else {
                res.writeHead(400, { 'Content-Type': 'text/plain' });
                res.end('Error: No token provided.');
            }
        } else {
            res.writeHead(404, { 'Content-Type': 'text/plain' });
            res.end('Not Found');
        }
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

    server.listen(port, '127.0.0.1', () => {
        log.info(`Local auth callback server listening on http://127.0.0.1:${port}`);
        localAuthCallbackUrl = `http://127.0.0.1:${port}/auth-callback`;
    });
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
    initializeFile(aliasesPath, '{}');
    initializeFile(configPath, 'discord_rpc:\n  enabled: false');
    try {
        if (fs.existsSync(configPath)) {
            config = yaml.parse(fs.readFileSync(configPath, 'utf8'));
            if (config.discord_rpc && config.discord_rpc.enabled) {
                discordRpc.login();
            }
        }
    } catch (e) {
        log.error('Failed to load initial config.yml for Discord RPC:', e);
    }

    createWindow();
    startAuthServer();
});
function initializeFile(filePath, defaultContent) {
    try {
        const dir = path.dirname(filePath);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }
        if (!fs.existsSync(filePath)) {
            log.info(`Initializing file: ${filePath}`);
            fs.writeFileSync(filePath, defaultContent, 'utf8');
        }
    } catch (e) {
        log.error(`Failed to initialize file ${filePath}:`, e);
    }
}

function extractSkinUrl(properties) {
    try {
        const texturesProp = properties.find(p => p.name === 'textures');
        if (!texturesProp) return null;
        const texturesJson = Buffer.from(texturesProp.value, 'base64').toString('utf8');
        const textures = JSON.parse(texturesJson);
        return textures.textures?.SKIN?.url || null;
    } catch (e) {
        return null;
    }
}

async function createHypixelHandler() {
    try {
        const apiKey = await apiHandler.getApiKey();
        if (!apiKey) {
            log.warn('Hypixel API Key not available from backend. Stats/commands will be limited.');
        }
        const mockProxy = {
            env: { apiKey: apiKey },
            proxyChat: (msg) => log(`[MOCK_PROXY_CHAT] ${msg}`)
        };
        statsHandler = new HypixelHandler(mockProxy);
        log.info('Hypixel handler created with key from backend.');
    } catch (error) {
        log.error('Failed to create Hypixel handler:', error);
    }
}

app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
app.on('will-quit', () => { if (proxyProcess) proxyProcess.kill(); });
ipcMain.on('minimize-window', () => mainWindow.minimize());
ipcMain.on('maximize-window', () => {
    if (mainWindow.isMaximized()) {
        mainWindow.unmaximize();
    } else {
        mainWindow.maximize();
    }
});
ipcMain.on('close-window', () => app.quit());
ipcMain.on('open-external-url', (event, url) => {
    shell.openExternal(url);
});
ipcMain.on('set-jwt', (event, token) => {
    jwtToken = token;
    apiHandler.setJwt(token);
    log.info('JWT has been set in the launcher main process.');
    createHypixelHandler();
});

ipcMain.on('clear-jwt', () => {
    jwtToken = null;
    apiHandler.setJwt(null);
    statsHandler = null;
    log.info('JWT and stats handler have been cleared due to logout.');
});
ipcMain.on('get-local-auth-callback-url', (event) => {
    event.returnValue = localAuthCallbackUrl;
});

ipcMain.on('get-app-version', (event) => {
    event.reply('app-version', app.getVersion());
});

ipcMain.on('check-for-updates', () => {
    mainWindow.webContents.send('update-status', 'Checking for updates...');
    autoUpdater.checkForUpdates();
});

autoUpdater.on('update-not-available', () => {
    mainWindow.webContents.send('update-status', `You are on the latest version: v${app.getVersion()}`);
    new Notification({
        title: 'JagProx Updater',
        body: 'You are already running the latest version.'
    }).show();
});

autoUpdater.on('update-available', (info) => {
    mainWindow.webContents.send('update-status', `Update v${info.version} found!`);
    const notification = new Notification({
        title: 'Update available!',
        body: `JagProx v${info.version} is available. Click to download.`,
        actions: [{ type: 'button', text: 'Download' }]
    });
    
    notification.on('click', () => {
        mainWindow.webContents.send('update-status', 'Downloading update...');
        autoUpdater.downloadUpdate();
    });
    
    notification.show();
});

autoUpdater.on('download-progress', (progressObj) => {
    const percent = progressObj.percent.toFixed(2);
    const transferred = (progressObj.transferred / 1024 / 1024).toFixed(2);
    const total = (progressObj.total / 1024 / 1024).toFixed(2);
    mainWindow.webContents.send('update-status', `Downloading... ${percent}% (${transferred}MB / ${total}MB)`);
});

autoUpdater.on('update-downloaded', (info) => {
    mainWindow.webContents.send('update-status', `Update v${info.version} downloaded. Ready to install.`);
    const notification = new Notification({
        title: 'Download complete!',
        body: 'Click to install the update now. The application will restart.',
        actions: [{ type: 'button', text: 'Install & Restart' }]
    });

    notification.on('click', () => {
        autoUpdater.quitAndInstall();
    });

    notification.show();
});

autoUpdater.on('error', (err) => {
    mainWindow.webContents.send('update-status', 'Error checking for updates.');
    new Notification({
        title: 'Update Error',
        body: `An error occurred: ${err.message}`
    }).show();
});
ipcMain.on('get-config', (event) => {
    try {
        if (fs.existsSync(configPath)) {
            const config = yaml.parse(fs.readFileSync(configPath, 'utf8'));
            event.reply('config-loaded', config);
        }
    } catch (e) {
    }
});

ipcMain.on('save-settings', (event, settings) => {
    try {
        if (fs.existsSync(configPath)) {
            config = yaml.parse(fs.readFileSync(configPath, 'utf8'));
        }
        config.auto_gg = settings.auto_gg;
        fs.writeFileSync(configPath, yaml.stringify(config));
        event.reply('settings-saved-reply', true);
    } catch (e) {
        event.reply('settings-saved-reply', false);
    }
});

ipcMain.on('toggle-discord-rpc', (event, enabled) => {
    try {
        if (enabled) {
            discordRpc.login();
        } else {
            discordRpc.logout();
        }
        if (fs.existsSync(configPath)) {
            config = yaml.parse(fs.readFileSync(configPath, 'utf8'));
        }
        config.discord_rpc = { enabled };
        fs.writeFileSync(configPath, yaml.stringify(config));
    } catch (e) {
    }
});
ipcMain.on('save-api-key', async (event, apiKey) => {
    if (!apiHandler.jwt) {
        log.error('Cannot save API key without a JWT.');
        return event.reply('api-key-saved-reply', false);
    }
    try {
        await apiHandler.saveApiKey(apiKey);
        log.info('Hypixel API key saved to backend.');
        await createHypixelHandler();
        event.reply('api-key-saved-reply', true);
    } catch (e) {
        log.error('Failed to save API key to backend:', e);
        event.reply('api-key-saved-reply', false);
    }
});

ipcMain.on('get-api-key', async (event) => {
    if (!apiHandler.jwt) {
        log.error('Cannot get API key without a JWT.');
        return event.reply('api-key-loaded', null);
    }
    try {
        const apiKey = await apiHandler.getApiKey();
        event.reply('api-key-loaded', apiKey);
    } catch (e) {
        log.error('Failed to get API key from backend:', e);
        event.reply('api-key-loaded', null);
    }
});
ipcMain.on('get-aliases', (event) => {
    try {
        const data = fs.readFileSync(aliasesPath, 'utf8');
        event.reply('aliases-loaded', JSON.parse(data));
    } catch (e) {
        event.reply('aliases-loaded', {});
    }
});

ipcMain.on('save-aliases', (event, aliases) => {
    try {
        fs.writeFileSync(aliasesPath, JSON.stringify(aliases, null, 4));
        event.reply('aliases-saved-reply', true);
    } catch (e) {
        event.reply('aliases-saved-reply', false);
    }
});
ipcMain.on('get-gamemodes', (event) => {
    const uniqueGamemodes = new Map();
    for (const [key, modeInfo] of Object.entries(gameModeMap)) {
        if (!uniqueGamemodes.has(modeInfo.displayName)) {
            uniqueGamemodes.set(modeInfo.displayName, key);
        }
    }
    const availableGamemodes = Array.from(uniqueGamemodes, ([displayName, apiKey]) => ({ text: displayName, value: apiKey }));
    availableGamemodes.sort((a, b) => a.text.localeCompare(b.text));
    event.reply('gamemodes-loaded', availableGamemodes);
});

ipcMain.on('get-player-stats', async (event, { name, gamemode }) => {
    if (!statsHandler) {
        await createHypixelHandler();
    }
    if (!statsHandler) {
        return event.reply('player-stats-result', { error: "Hypixel handler not initialized. Is user logged in?" });
    }
    const result = await statsHandler.getStatsForAPI(gamemode, name);
    if (result && !result.error && result.stats) {
        result.skinUrl = extractSkinUrl(result.stats.properties);
    }
    event.reply('player-stats-result', result);
});
ipcMain.on('get-player-status', async (event, name) => {
    if (!statsHandler) {
        await createHypixelHandler();
    }
    if (!statsHandler) {
        return event.reply('player-status-result', { error: "Hypixel handler not initialized. Is user logged in?" });
    }
    const result = await statsHandler.getStatusForAPI(name);
    if (result && !result.error) {
        const playerFull = await statsHandler.getStats(result.uuid, '');
        if (playerFull) {
            result.skinUrl = extractSkinUrl(playerFull.properties);
        }
    }
    event.reply('player-status-result', result);
});

ipcMain.on('get-gamemode-list', (event) => {
    const uniqueGamemodes = new Map();
    for (const [key, modeInfo] of Object.entries(gameModeMap)) {
        if (!uniqueGamemodes.has(modeInfo.displayName)) {
            uniqueGamemodes.set(modeInfo.displayName, key);
        }
    }
    const availableGamemodes = Array.from(uniqueGamemodes, ([displayName, apiKey]) => ({ text: displayName, value: apiKey }));
    availableGamemodes.sort((a, b) => a.text.localeCompare(b.text));
    event.reply('gamemode-list-response', availableGamemodes);
});
ipcMain.on('toggle-proxy', (event, { start, token }) => {
    if (start && !proxyProcess) {
        if (token && !jwtToken) {
            jwtToken = token;
            apiHandler.setJwt(token);
            log.info('JWT has been set from toggle-proxy.');
            if (!statsHandler) createHypixelHandler();
        }

        const electronExecutable = process.execPath;
        const appPath = app.getAppPath();
        const mainScriptPath = path.join(appPath, 'main.js');

        const childEnv = {
            ...process.env,
            ELECTRON_RUN_AS_NODE: '1',
            USER_DATA_PATH: userDataPath,
            JAGPROX_JWT: token,
        };

        proxyProcess = spawn(electronExecutable, [mainScriptPath], {
            env: childEnv
        });

        if (discordRpc.isActive()) {
            discordRpc.setActivity({
                details: 'Playing',
                state: 'In Game',
                largeImageKey: 'icon',
                largeImageText: 'JagProx',
                instance: false,
                startTimestamp: new Date()
            });
        }

        mainWindow.webContents.send('proxy-status', 'running');
        const handleData = (data) => {
            const lines = data.toString().split('\n').filter(line => line.length > 0);
            lines.forEach(line => {
                if (line.startsWith('[JAGPROX_CHAT]')) {
                    mainWindow.webContents.send('proxy-chat', line.replace('[JAGPROX_CHAT]', ''));
                } else if (line.startsWith('[JAGPROX_RAW_CHAT]')) {
                    try {
                        const rawJson = line.replace('[JAGPROX_RAW_CHAT]', '');
                        const legacyText = formatter.reconstructLegacyText(rawJson);
                        mainWindow.webContents.send('proxy-chat', legacyText);
                    } catch (e) {
                        log.error('Failed to parse raw chat JSON:', e);
                        mainWindow.webContents.send('proxy-log', `[ERROR] Bad raw chat message: ${line}`);
                    }
                } else {
                    mainWindow.webContents.send('proxy-log', line);
                }
            });
        };
        proxyProcess.stdout.on('data', handleData);
        proxyProcess.stderr.on('data', handleData);
        proxyProcess.on('close', (code) => {
            mainWindow.webContents.send('proxy-log', `[SYSTEM] Proxy process exited with code ${code}`);
            mainWindow.webContents.send('proxy-status', 'stopped');
            proxyProcess = null;
            if (discordRpc.isActive()) {
                discordRpc.setActivity({
                    details: 'Idling',
                    state: 'In Launcher',
                    largeImageKey: 'icon',
                    largeImageText: 'JagProx',
                    instance: false,
                    startTimestamp: new Date()
                });
            }
        });
    } else if (!start && proxyProcess) {
        proxyProcess.kill();
    }
});
