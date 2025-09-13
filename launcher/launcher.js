const { app, BrowserWindow, ipcMain, Notification } = require('electron');
const path = require('path');
const fs = require('fs');
const yaml = require('yaml');
const { spawn } = require('child_process');
const { autoUpdater } = require('electron-updater');
const log = require('electron-log');
const HypixelHandler = require('../modules/hypixelHandler.js');
const { gameModeMap } = require('../utils/constants.js');

let mainWindow;
let proxyProcess;
let statsHandler;
let userDataPath;
let aliasesPath;
let envPath;
let configPath;

function createWindow() {
    log.info('Creating main window...');
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
    createHypixelHandler();
}

app.whenReady().then(() => {
    userDataPath = app.getPath('userData');
    aliasesPath = path.join(userDataPath, 'aliases.json');
    envPath = path.join(userDataPath, '.env');
    configPath = path.join(userDataPath, 'config.yml');
    
    log.transports.file.resolvePath = () => path.join(userDataPath, 'logs/main.log');
    autoUpdater.logger = log;
    autoUpdater.logger.transports.file.level = "info";
    autoUpdater.autoDownload = false;

    log.info('App is ready.');

    initializeFile(aliasesPath, '{}');
    initializeFile(envPath, 'HYPIXEL_API_KEY=');

    createWindow();
});


function initializeFile(filePath, defaultContent) {
    try {
        if (!fs.existsSync(filePath)) {
            log.info(`Initializing file: ${filePath}`);
            fs.writeFileSync(filePath, defaultContent, 'utf8');
        }
    } catch (e) {
        log.error(`Failed to initialize file ${filePath}:`, e);
    }
}

function getApiKeyFromEnv() {
    if (!fs.existsSync(envPath)) return null;
    const envContent = fs.readFileSync(envPath, 'utf8');
    const match = envContent.match(/^HYPIXEL_API_KEY=(.*)$/m);
    return match ? match[1] : null;
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

function createHypixelHandler() {
    const apiKey = getApiKeyFromEnv();
    const mockProxy = {
        env: { apiKey: apiKey },
        proxyChat: (msg) => console.log(`[MOCK_PROXY_CHAT] ${msg}`)
    };
    statsHandler = new HypixelHandler(mockProxy);
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

ipcMain.on('get-app-version', (event) => {
    event.reply('app-version', app.getVersion());
});

ipcMain.on('check-for-updates', () => {
    log.info('User triggered update check.');
    mainWindow.webContents.send('update-status', 'Checking for updates...');
    autoUpdater.checkForUpdates();
});

autoUpdater.on('update-not-available', () => {
    log.info('Update not available.');
    mainWindow.webContents.send('update-status', `You are on the latest version: v${app.getVersion()}`);
    new Notification({
        title: 'JagProx Updater',
        body: 'You are already running the latest version.'
    }).show();
});

autoUpdater.on('update-available', (info) => {
    log.info(`Update available: ${info.version}`);
    mainWindow.webContents.send('update-status', `Update v${info.version} found!`);
    const notification = new Notification({
        title: 'Update available!',
        body: `JagProx v${info.version} is available. Click to download.`,
        actions: [{ type: 'button', text: 'Download' }]
    });
    
    notification.on('click', () => {
        log.info('User clicked notification to download update.');
        mainWindow.webContents.send('update-status', 'Downloading update...');
        autoUpdater.downloadUpdate();
    });
    
    notification.show();
});

autoUpdater.on('download-progress', (progressObj) => {
    const percent = progressObj.percent.toFixed(2);
    const transferred = (progressObj.transferred / 1024 / 1024).toFixed(2);
    const total = (progressObj.total / 1024 / 1024).toFixed(2);
    log.info(`Download progress: ${percent}%`);
    mainWindow.webContents.send('update-status', `Downloading... ${percent}% (${transferred}MB / ${total}MB)`);
});

autoUpdater.on('update-downloaded', (info) => {
    log.info(`Update downloaded: ${info.version}`);
    mainWindow.webContents.send('update-status', `Update v${info.version} downloaded. Ready to install.`);
    const notification = new Notification({
        title: 'Download complete!',
        body: 'Click to install the update now. The application will restart.',
        actions: [{ type: 'button', text: 'Install & Restart' }]
    });

    notification.on('click', () => {
        log.info('User clicked notification to install update.');
        autoUpdater.quitAndInstall();
    });

    notification.show();
});

autoUpdater.on('error', (err) => {
    log.error('Error in auto-updater.', err);
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
        log.error('Failed to load config.yml:', e);
    }
});

ipcMain.on('save-settings', (event, settings) => {
    try {
        let config = {};
        if (fs.existsSync(configPath)) {
            config = yaml.parse(fs.readFileSync(configPath, 'utf8'));
        }
        config.auto_gg = settings.auto_gg;
        fs.writeFileSync(configPath, yaml.stringify(config));
        event.reply('settings-saved-reply', true);
    } catch (e) {
        log.error('Failed to save settings to config.yml:', e);
        event.reply('settings-saved-reply', false);
    }
});

ipcMain.on('get-api-key', (event) => { event.reply('api-key-loaded', getApiKeyFromEnv()); });
ipcMain.on('save-api-key', (event, apiKey) => {
    try {
        let envContent = fs.existsSync(envPath) ? fs.readFileSync(envPath, 'utf8') : '';
        const key = 'HYPIXEL_API_KEY';
        if (envContent.includes(key)) {
            envContent = envContent.replace(new RegExp(`^${key}=.*$`, 'm'), `${key}=${apiKey}`);
        } else {
            envContent += `\n${key}=${apiKey}`;
        }
        fs.writeFileSync(envPath, envContent.trim());
        createHypixelHandler();
        event.reply('api-key-saved-reply', true);
    } catch (e) {
        log.error('Failed to save API key:', e);
        event.reply('api-key-saved-reply', false);
    }
});

ipcMain.on('get-aliases', (event) => {
    try {
        const data = fs.readFileSync(aliasesPath, 'utf8');
        event.reply('aliases-loaded', JSON.parse(data));
    } catch (e) {
        log.error('Failed to load aliases:', e);
        event.reply('aliases-loaded', {});
    }
});

ipcMain.on('save-aliases', (event, aliases) => {
    try {
        fs.writeFileSync(aliasesPath, JSON.stringify(aliases, null, 4));
        event.reply('aliases-saved-reply', true);
    } catch (e) {
        log.error('Failed to save aliases:', e);
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
    if (!statsHandler) return event.reply('player-stats-result', { error: "Hypixel handler not initialized." });
    const result = await statsHandler.getStatsForAPI(gamemode, name);
    if (result && !result.error) {
        result.skinUrl = extractSkinUrl(result.stats.properties);
    }
    event.reply('player-stats-result', result);
});

ipcMain.on('get-player-status', async (event, name) => {
    if (!statsHandler) return event.reply('player-status-result', { error: "Hypixel handler not initialized." });
    const result = await statsHandler.getStatusForAPI(name);
    if (result && !result.error) {
        const playerFull = await statsHandler.getStats(result.uuid, '');
        result.skinUrl = extractSkinUrl(playerFull.properties);
    }
    event.reply('player-status-result', result);
});

ipcMain.on('toggle-proxy', (event, start) => {
    if (start && !proxyProcess) {
        const electronExecutable = process.execPath;
        const appPath = app.getAppPath();
        const mainScriptPath = path.join(appPath, 'main.js');

        const childEnv = {
            ...process.env,
            ELECTRON_RUN_AS_NODE: '1',
            USER_DATA_PATH: userDataPath
        };

        proxyProcess = spawn(electronExecutable, [mainScriptPath], {
            env: childEnv
        });

        mainWindow.webContents.send('proxy-status', 'running');
        const handleData = (data) => {
            const lines = data.toString().split('\n').filter(line => line.length > 0);
            lines.forEach(line => {
                if (line.startsWith('[JAGPROX_CHAT]')) {
                    mainWindow.webContents.send('proxy-chat', line.replace('[JAGPROX_CHAT]', ''));
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
        });
    } else if (!start && proxyProcess) {
        proxyProcess.kill();
    }
});