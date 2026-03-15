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
let splashWindow;
let updatePromptWindow;
let proxyProcess;
let statsHandler;
let userDataPath;
let aliasesPath;
let configPath;

let apiHandler;
let jwtToken = null;

let config = {};
let localAuthCallbackUrl = null;

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

function createSplashWindow() {
    splashWindow = new BrowserWindow({
        width: 400,
        height: 350,
        frame: false,
        resizable: false,
        show: false,
        webPreferences: {
            nodeIntegration: true,
            contextIsolation: false,
        },
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

    updatePromptWindow = new BrowserWindow({
        width: 450,
        height: 220,
        frame: false,
        resizable: false,
        parent: splashWindow,
        modal: true,
        webPreferences: {
            nodeIntegration: true,
            contextIsolation: false,
        },
        backgroundColor: '#1e1e2e'
    });

    updatePromptWindow.loadFile(path.join(__dirname, 'update-prompt.html'));
    updatePromptWindow.webContents.on('did-finish-load', () => {
        updatePromptWindow.webContents.send('version-info', version);
    });

    ipcMain.once('update-response', (event, response) => {
        if (response === 'update') {
            autoUpdater.downloadUpdate();
            if (splashWindow) splashWindow.webContents.send('update-status', 'Downloading update...');
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
            splashWindow.webContents.send('update-status', 'Launching JagProx...');
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
    mainWindow = new BrowserWindow({
        width: 1000,
        height: 650,
        frame: false,
        show: false, 
        webPreferences: {
            nodeIntegration: true,
            contextIsolation: false,
        },
        autoHideMenuBar: true,
        icon: path.join(__dirname, 'icon.png')
    });

    mainWindow.loadFile(path.join(__dirname, 'index.html'));
    
    mainWindow.once('ready-to-show', () => {

    });

    mainWindow.webContents.setWindowOpenHandler(({ url }) => {
        shell.openExternal(url);
        return { action: 'deny' };
    });
}

function handleUpdates() {
    autoUpdater.on('checking-for-update', () => {
        if (splashWindow) splashWindow.webContents.send('update-status', 'Checking for updates...');
    });

    autoUpdater.on('update-available', (info) => {
        if (splashWindow) splashWindow.webContents.send('update-status', 'Update available!');
        createUpdatePromptWindow(info.version);
    });

    autoUpdater.on('update-not-available', () => {
        launchApp();
    });

    autoUpdater.on('error', (err) => {
        log.error('Update error:', err);
        launchApp();
    });

    autoUpdater.on('download-progress', (progressObj) => {
        if (splashWindow) {
            splashWindow.webContents.send('update-status', `Downloading update (${Math.round(progressObj.percent)}%)`);
            splashWindow.webContents.send('update-progress', progressObj.percent);
        }
    });

    autoUpdater.on('update-downloaded', () => {
        if (splashWindow) splashWindow.webContents.send('update-status', 'Restarting...');
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
    initializeFile(aliasesPath, '{}');
    const defaultConfigPath = path.join(app.getAppPath(), 'config.yml');
    syncConfig(configPath, defaultConfigPath);
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

    createSplashWindow();
    handleUpdates();
});

function syncConfig(userPath, defaultPath) {
    try {
        if (!fs.existsSync(defaultPath)) {
            log.warn(`Default config not found at ${defaultPath}`);
            return;
        }

        const defaultYaml = fs.readFileSync(defaultPath, 'utf8');
        const defaultConfig = yaml.parse(defaultYaml);

        let userConfig = {};
        if (fs.existsSync(userPath)) {
            const userYaml = fs.readFileSync(userPath, 'utf8');
            try {
                userConfig = yaml.parse(userYaml) || {};
            } catch (e) {
                log.error(`Failed to parse user config at ${userPath}:`, e);
                userConfig = {};
            }
        }

        let updated = false;

        function deepMerge(target, source) {
            for (const key in source) {
                if (source[key] !== null && typeof source[key] === 'object' && !Array.isArray(source[key])) {
                    if (target[key] === undefined || target[key] === null || typeof target[key] !== 'object' || Array.isArray(target[key])) {
                        target[key] = {};
                        updated = true;
                    }
                    if (deepMerge(target[key], source[key])) {
                        updated = true;
                    }
                } else {
                    if (target[key] === undefined) {
                        target[key] = source[key];
                        updated = true;
                    }
                }
            }
            return updated;
        }

        if (deepMerge(userConfig, defaultConfig)) {
            log.info(`Syncing config.yml with new/default keys...`);
            fs.writeFileSync(userPath, yaml.stringify(userConfig), 'utf8');
        } else if (!fs.existsSync(userPath)) {
            log.info(`Initializing config.yml from default...`);
            fs.writeFileSync(userPath, yaml.stringify(defaultConfig), 'utf8');
        }
    } catch (e) {
        log.error('Failed to sync config.yml:', e);
    }
}

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

        const iconPath = app.isPackaged
            ? path.join(process.resourcesPath, "server-icon.png")
            : path.join(__dirname, "..", "server-icon.png");

        const childEnv = {
            ...process.env,
            ELECTRON_RUN_AS_NODE: '1',
            USER_DATA_PATH: userDataPath,
            JAGPROX_JWT: token,
            ICON_PATH: iconPath
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
