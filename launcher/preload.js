// The only bridge between the (sandboxed, Node-less) launcher pages and the main process.
// Pages can use these IPC channels and nothing else.
const { contextBridge, ipcRenderer } = require('electron');

const SEND_CHANNELS = new Set([
    'minimize-window', 'maximize-window', 'close-window',
    'start-login', 'restore-session', 'clear-jwt',
    'get-app-version', 'check-for-updates', 'update-response',
    'get-config', 'save-settings', 'toggle-discord-rpc',
    'save-api-key', 'get-api-key-status',
    'get-aliases', 'save-aliases',
    'get-gamemode-list', 'get-player-stats', 'get-player-status',
    'toggle-proxy',
]);

const RECEIVE_CHANNELS = new Set([
    'session-state', 'app-version', 'update-status', 'update-progress', 'version-info',
    'config-loaded', 'settings-saved-reply',
    'api-key-saved-reply', 'api-key-status',
    'aliases-loaded', 'aliases-saved-reply',
    'gamemode-list-response', 'player-stats-result', 'player-status-result',
    'proxy-status', 'proxy-log', 'proxy-chat',
]);

contextBridge.exposeInMainWorld('jagprox', {
    send(channel, payload) {
        if (!SEND_CHANNELS.has(channel)) throw new Error(`Blocked IPC channel: ${channel}`);
        ipcRenderer.send(channel, payload);
    },
    on(channel, callback) {
        if (!RECEIVE_CHANNELS.has(channel)) throw new Error(`Blocked IPC channel: ${channel}`);
        ipcRenderer.on(channel, (_event, ...args) => callback(...args));
    },
});
