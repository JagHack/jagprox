const formatter = require('../formatter.js');

function extractTextFromComponent(component) {
    if (typeof component === 'string') {
        return component;
    }
    let text = component.text || '';
    if (component.extra) {
        text += component.extra.map(extractTextFromComponent).join('');
    }
    return text;
}

class TabAlerter {
    constructor(proxy) {
        this.proxy = proxy;
        this.lobbyPlayers = new Map();
        this.alertedThisSession = new Set();
        this.selfPosition = { x: 0, y: 0, z: 0 };
    }

    reset() {
        formatter.log('Tab Alerter reset.');
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
                        displayName: player.displayName ? extractTextFromComponent(JSON.parse(player.displayName)) : player.name
                    });
                    this.checkForAlert(player.uuid);
                }
                break;

            case 'update_display_name':
                for (const player of data.data) {
                    if (this.lobbyPlayers.has(player.uuid) && player.displayName) {
                        const playerData = this.lobbyPlayers.get(player.uuid);
                        try {
                            playerData.displayName = extractTextFromComponent(JSON.parse(player.displayName));
                        } catch(e) {
                            playerData.displayName = player.displayName;
                        }
                        this.checkForAlert(player.uuid);
                    }
                }
                break;

            case 'remove_player':
                for (const player of data.data) {
                    this.lobbyPlayers.delete(player.uuid);
                    const alertedKey = Array.from(this.alertedThisSession).find(name => name.includes(player.uuid));
                    if (alertedKey) {
                        this.alertedThisSession.delete(alertedKey);
                    }
                }
                break;
        }
    }

    checkForAlert(uuid) {
        if (!this.lobbyPlayers.has(uuid)) return;

        const alertList = this.proxy.config.tab_alerts || [];
        if (alertList.length === 0) return;

        const playerData = this.lobbyPlayers.get(uuid);
        const cleanDisplayName = playerData.displayName.replace(/§[0-9a-fk-or]/g, '');

        const alertKey = `${cleanDisplayName}@${uuid}`;
        if (this.alertedThisSession.has(alertKey)) {
            return;
        }

        for (const targetName of alertList) {
            if (cleanDisplayName.toLowerCase().includes(targetName.toLowerCase())) {
                this.triggerAlert(cleanDisplayName, alertKey);
                break;
            }
        }
    }

    triggerAlert(fullPlayerName, alertKey) {
        formatter.log(`Alerting for player: ${fullPlayerName}`);
        this.proxy.proxyChat(`§aFound player §e${fullPlayerName}§a!`);
        this.alertedThisSession.add(alertKey);

        this.playSoundEffect('entity.experience_orb.pickup');
        setTimeout(() => this.playSoundEffect('entity.experience_orb.pickup'), 200);
        setTimeout(() => this.playSoundEffect('entity.experience_orb.pickup'), 400);
        setTimeout(() => this.playSoundEffect('entity.experience_orb.pickup'), 600);
    }

    playSoundEffect(soundName) {
        if (!this.proxy.client) return;
        this.proxy.client.write('named_sound_effect', {
            soundName: soundName, soundCategory: 0,
            x: this.selfPosition.x * 8, y: this.selfPosition.y * 8, z: this.selfPosition.z * 8,
            volume: 1.0, pitch: 63
        });
    }
}

module.exports = TabAlerter;