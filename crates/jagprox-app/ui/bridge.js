// The launcher pages' only way to talk to the app: the same window.jagprox API the
// Electron preload offered, on top of Tauri's invoke and events.
(() => {
    const { invoke } = window.__TAURI__.core;
    const { listen } = window.__TAURI__.event;
    // listen() registers asynchronously; requests wait for it so no reply is missed.
    const registering = [];

    window.jagprox = {
        send(channel, payload) {
            Promise.all(registering)
                .then(() => invoke('ipc', { channel, payload: payload ?? null }))
                .catch((e) => console.error(`IPC ${channel} failed:`, e));
        },
        on(channel, callback) {
            registering.push(listen(channel, (event) => callback(event.payload)));
        },
    };
})();
