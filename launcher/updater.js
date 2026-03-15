const { autoUpdater } = require('electron-updater');
const log = require('electron-log');
const { ipcMain, app } = require('electron');

autoUpdater.logger = log;
autoUpdater.logger.transports.file.level = 'info';

autoUpdater.autoDownload = false;

function initUpdater(mainWindow) {
    log.info('Initializing auto-updater...');

    ipcMain.on('restart-app', () => {
        log.info('Restarting app to install update...');
        autoUpdater.quitAndInstall();
    });

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

    if (app.isPackaged) {
        autoUpdater.checkForUpdatesAndNotify();

        setInterval(() => {
            autoUpdater.checkForUpdates();
        }, 1000 * 60 * 60 * 2);
    } else {
        log.info('App is not packaged. Skipping update check.');
    }
}

module.exports = { initUpdater };
