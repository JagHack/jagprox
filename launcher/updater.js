const { autoUpdater } = require('electron-updater');
const log = require('electron-log');
const { ipcMain, app } = require('electron');

// Configure logging
autoUpdater.logger = log;
autoUpdater.logger.transports.file.level = 'info';

// Disable auto-download to control the flow
autoUpdater.autoDownload = false;

function initUpdater(mainWindow) {
    log.info('Initializing auto-updater...');

    // IPC to trigger restart and install
    ipcMain.on('restart-app', () => {
        log.info('Restarting app to install update...');
        autoUpdater.quitAndInstall();
    });

    // Helper to send status to renderer
    const sendStatusToWindow = (status, data = {}) => {
        log.info(`Update Status: ${status}`, data);
        if (mainWindow && !mainWindow.isDestroyed()) {
            mainWindow.webContents.send('update-message', { status, ...data });
        }
    };

    autoUpdater.on('checking-for-update', () => {
        sendStatusToWindow('checking-for-update');
    });

    autoUpdater.on('update-available', (info) => {
        sendStatusToWindow('update-available', { version: info.version });
        // Start background download automatically
        log.info('Update available. Downloading in background...');
        autoUpdater.downloadUpdate();
    });

    autoUpdater.on('update-not-available', (info) => {
        sendStatusToWindow('update-not-available', { version: info.version });
    });

    autoUpdater.on('error', (err) => {
        sendStatusToWindow('error', { message: err.message });
    });

    autoUpdater.on('download-progress', (progressObj) => {
        sendStatusToWindow('download-progress', {
            percent: progressObj.percent,
            transferred: progressObj.transferred,
            total: progressObj.total
        });
    });

    autoUpdater.on('update-downloaded', (info) => {
        sendStatusToWindow('update-downloaded', { version: info.version });
    });

    // Only check for updates in packaged app
    if (app.isPackaged) {
        autoUpdater.checkForUpdatesAndNotify();
        
        // Check for updates every 2 hours
        setInterval(() => {
            autoUpdater.checkForUpdates();
        }, 1000 * 60 * 60 * 2);
    } else {
        log.info('App is not packaged. Skipping update check.');
    }
}

module.exports = { initUpdater };
