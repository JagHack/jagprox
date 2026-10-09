const formatter = require('../formatter.js');

// Display names arrive as JSON chat components; fall back to the raw string if not.
function displayNameText(displayName) {
    try {
        return formatter.extractText(JSON.parse(displayName));
    } catch (e) {
        return displayName;
    }
}

class TabAlerter {
    constructor(proxy) {
        this.proxy = proxy;
        this.lobbyPlayers = new Map();
        this.alertedThisSession = new Set();
        this.selfPosition = { x: 0, y: 0, z: 0 };
    }

    reset() {
        formatter.debug('Tab Alerter reset.');
        this.lobbyPlayers.clear();
        this.alertedThisSession.clear();
    }

    handlePacket(data, meta) {
        if (meta.name === 'kick_disconnect' || meta.name === 'disconnect') {
            this.reset();
            return;
        }

        if (meta.name === 'position') {
            this.selfPosition = { x: data.x, y: data.y, z: data.z };
        }

        if (meta.name !== 'player_info') return;

        switch (data.action) {
            case 'add_player':
                for (const player of data.data) {
                    this.lobbyPlayers.set(player.uuid, {
                        rawName: player.name,
                        displayName: player.displayName ? displayNameText(player.displayName) : player.name
                    });
                    this.checkForAlert(player.uuid);
                }
                break;

            case 'update_display_name':
                for (const player of data.data) {
                    if (this.lobbyPlayers.has(player.uuid) && player.displayName) {
                        this.lobbyPlayers.get(player.uuid).displayName = displayNameText(player.displayName);
                        this.checkForAlert(player.uuid);
                    }
                }
                break;

            case 'remove_player':
                for (const player of data.data) {
                    this.lobbyPlayers.delete(player.uuid);
                    for (const key of this.alertedThisSession) {
                        if (key.endsWith(`@${player.uuid}`)) this.alertedThisSession.delete(key);
                    }
                }
                break;
        }
    }

    checkForAlert(uuid) {
        const playerData = this.lobbyPlayers.get(uuid);
        if (!playerData || !playerData.displayName) return;

        const alertList = Array.isArray(this.proxy.config.tab_alerts) ? this.proxy.config.tab_alerts : [];
        if (alertList.length === 0) return;

        const cleanDisplayName = formatter.stripColors(playerData.displayName);
        const alertKey = `${cleanDisplayName}@${uuid}`;
        if (this.alertedThisSession.has(alertKey)) return;

        const lowerName = cleanDisplayName.toLowerCase();
        if (alertList.some(targetName => lowerName.includes(String(targetName).toLowerCase()))) {
            this.triggerAlert(cleanDisplayName, alertKey);
        }
    }

    triggerAlert(fullPlayerName, alertKey) {
        formatter.log(`Alerting for player: ${fullPlayerName}`);
        this.proxy.proxyChat(`§dFound player §5${fullPlayerName}§d!`);
        this.alertedThisSession.add(alertKey);

        for (const delay of [0, 200, 400, 600]) {
            setTimeout(() => this.playSoundEffect('random.orb'), delay);
        }
    }

    // 1.8 sound names (e.g. "random.orb"); coordinates are fixed-point (x8), pitch 63 = 1.0.
    playSoundEffect(soundName) {
        if (!this.proxy.client) return;
        this.proxy.client.write('named_sound_effect', {
            soundName,
            x: Math.round(this.selfPosition.x * 8),
            y: Math.round(this.selfPosition.y * 8),
            z: Math.round(this.selfPosition.z * 8),
            volume: 1.0,
            pitch: 63
        });
    }
}

module.exports = TabAlerter;
