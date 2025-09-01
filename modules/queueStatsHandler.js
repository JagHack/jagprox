const formatter = require("../formatter.js");
const { gameModeMap } = require('../utils/constants');

class QueueStatsHandler {
    constructor(proxy) {
        this.proxy = proxy;
        this.currentGameKey = null;
        this.hasTriggeredForGame = false;
        this.awaitingTeleportForWho = false; // NEU: Flag, um auf den Teleport zu warten

        this.isCapturingWho = false;
        this.whoPlayers = [];

        this.titleToKeyMap = new Map();
        for (const [key, modeInfo] of Object.entries(gameModeMap)) {
            if (modeInfo && modeInfo.displayName) {
                this.titleToKeyMap.set(modeInfo.displayName.toUpperCase(), key);
            }
        }
    }

    resetTrigger() {
        if (this.hasTriggeredForGame) {
            formatter.log(`Game over detected. Re-arming queue stats trigger for the next game.`);
            this.hasTriggeredForGame = false;
            this.awaitingTeleportForWho = false; // Wichtig: auch hier zurücksetzen
        }
    }

    resetForNewGame(newGameKey) {
        formatter.log(`Game context set to: "${newGameKey}". Awaiting start message.`);
        this.currentGameKey = newGameKey;
        this.hasTriggeredForGame = false;
        this.awaitingTeleportForWho = false;
    }

    handlePacket(data, meta) {
        if (meta.name === 'kick_disconnect' || meta.name === 'disconnect') {
            this.currentGameKey = null;
            this.hasTriggeredForGame = false;
            this.awaitingTeleportForWho = false;
            return false;
        }

        // --- KORREKTUR: Teleport ist jetzt der AUSLÖSER ---
        if (meta.name === 'position' && this.awaitingTeleportForWho) {
            this.awaitingTeleportForWho = false; // Trigger sofort entschärfen, um Mehrfachauslösung zu verhindern
            formatter.log("Teleport confirmed. Executing /who after a short safety delay.");

            // Eine winzige Verzögerung nach dem Teleport, um sicherzugehen, dass der Client "angekommen" ist
            setTimeout(() => {
                this.isCapturingWho = true;
                this.whoPlayers = [];
                this.proxy.target.write('chat', { message: '/who' });
            }, 500); // 500ms Puffer nach dem Teleport
        }

        if (meta.name === 'scoreboard_objective' && data.action === 0 && data.displayText) {
            const newTitle = data.displayText.replace(/§[0-9a-fk-or]/g, '').trim().toUpperCase();
            const foundGameKey = this.titleToKeyMap.get(newTitle);
            if (foundGameKey && this.currentGameKey !== foundGameKey) {
                this.resetForNewGame(foundGameKey);
            }
        }

        if (meta.name === 'chat') {
            let chatJson, fullMessage = '', cleanMessage = '';
            try {
                chatJson = JSON.parse(data.message);
                const getFullText = (component) => {
                    let text = component.text || '';
                    if (component.extra) text += component.extra.map(getFullText).join('');
                    return text;
                };
                fullMessage = getFullText(chatJson);
                cleanMessage = fullMessage.replace(/§[0-9a-fk-or]/g, '').trim();
            } catch (e) { /* ignore */ }
            
            const gameOverKeywords = ['VICTORY!', 'GAME END', 'You died!', 'You have been eliminated!', 'You won!', 'Draw!'];
            if (gameOverKeywords.some(keyword => cleanMessage.includes(keyword))) {
                this.resetTrigger();
            }
            
            if (this.isCapturingWho) {
                if (cleanMessage.startsWith('ONLINE: ')) {
                    const players = cleanMessage.replace('ONLINE: ', '').split(', ');
                    this.whoPlayers.push(...players.map(p => p.trim().replace('.', '')));
                    if (cleanMessage.endsWith('.')) this.finishWhoCapture();
                    return true;
                }
                if (cleanMessage.startsWith('Team #')) {
                    const parts = cleanMessage.split(':');
                    if (parts.length > 1) {
                        const players = parts.slice(1).join(':').trim().split(', ');
                        this.whoPlayers.push(...players.map(p => p.trim()));
                    }
                    return true;
                }
                if (this.whoPlayers.length > 0 && cleanMessage === '') {
                    this.finishWhoCapture();
                    return true;
                }
            }
            
            if (this.hasTriggeredForGame || !this.currentGameKey) {
                return false;
            }
            
            const isBedwarsStart = cleanMessage.includes('Protect your bed and destroy the enemy beds.');
            const isDuelsStart = cleanMessage.includes('Eliminate your opponents!');
            const isSkywarsStart = cleanMessage.includes('Gather resources and equipment on your');
            
            if (isBedwarsStart || isDuelsStart || isSkywarsStart) {
                const isEnabled = this.proxy.config.queue_stats && this.proxy.config.queue_stats[this.currentGameKey];
                
                if (isEnabled) {
                    this.hasTriggeredForGame = true;
                    // --- KORREKTUR: Trigger nur "scharfschalten" ---
                    this.awaitingTeleportForWho = true;
                    formatter.log(`Game start message detected for '${this.currentGameKey}'. Awaiting teleport to execute /who.`);
                }
            }
        }

        return false;
    }

    finishWhoCapture() {
        this.isCapturingWho = false;
        formatter.log(`Captured ${this.whoPlayers.length} players from /who.`);
        if (this.proxy.hypixel && this.whoPlayers.length > 0) {
            this.proxy.hypixel.processQueueAndPrintBulk(this.whoPlayers, this.currentGameKey);
        }
        this.whoPlayers = [];
    }
}

module.exports = QueueStatsHandler;