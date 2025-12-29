const formatter = require('../formatter.js');

class GametrackClientHandler {
    constructor(proxy, mc_uuid, localPlayerName) {
        this.proxy = proxy;
        this.gametrackApiHandler = proxy.gametrackApiHandler; 
        
        this.mc_uuid = mc_uuid;
        this.localPlayerName = localPlayerName;

        this.lastEventTimestamp = 0;
        this.debouncePeriod = 5000;
    }

    async onGameChanged(newGameKey) {
        if (!this.mc_uuid || !newGameKey || newGameKey === 'limbo') {
            this.currentGame = null;
            return;
        }
        
        console.log(`[GameTrack] Game changed to ${newGameKey}. Starting session.`);
        this.currentGame = newGameKey;
        
        try {
            await this.gametrackApiHandler.sendStartEvent({
                mc_uuid: this.mc_uuid,
                mode: this.currentGame
            });
        } catch (e) {
            console.error(`[GameTrack] Failed to start session:`, e);
            this.proxy.proxyChat(`§c[GameTrack] Error starting session: ${e.message}`);
        }
    }

    async parseChatMessage(chatObject) {
        const message = formatter.extractText(chatObject);
        const upperMessage = message.replace(/§[0-9a-fk-or]/g, '').toUpperCase().trim();
        
        if (!upperMessage.includes('WINNER!')) {
            return;
        }

        if (message.includes(':')) {
            return;
        }

        if (Date.now() - this.lastEventTimestamp < this.debouncePeriod) {
            return;
        }
        
        if (!this.mc_uuid || !this.gametrackApiHandler) {
            console.error('[GameTrack] Missing UUID or API Handler. Cannot track game.');
            return;
        }
        
        if (!this.currentGame || this.currentGame === 'limbo') {
            return;
        }
        
        if (upperMessage.includes('LOBBY') || upperMessage.includes('REPLAY') || upperMessage.includes('SPECTATOR')) {
            return;
        }

        let result = null;
        
        const parts = upperMessage.split('WINNER!');
        const before = parts[0].trim();
        const after = parts.length > 1 ? parts[1].trim() : '';

        if (after.length > 0) {
            result = 'win';
        } else if (before.length > 0) {
            const wordsBefore = before.split(' ').filter(w => w.length > 0);
            if (wordsBefore.length >= 2) {
                result = 'loss';
            } else if (wordsBefore.length === 1) {
                result = 'win';
            }
        }
        
        if (result) {
            this.lastEventTimestamp = Date.now();
            console.log(`[GameTrack] Detected ${result} in ${this.currentGame} for nicked player.`);
            
            try {
                await this.gametrackApiHandler.sendEvent({
                    mc_uuid: this.mc_uuid,
                    mode: this.currentGame,
                    result: result
                });
                this.proxy.proxyChat(`§a[GameTrack] §7Recorded game result: §e${result.toUpperCase()}§7.`);
            } catch (e) {
                this.proxy.proxyChat(`§c[GameTrack] Error recording event: ${e.message}`);
                console.error(`[GameTrack] Failed to send gametrack event:`, e);
            }
        }
    }
}

module.exports = GametrackClientHandler;
