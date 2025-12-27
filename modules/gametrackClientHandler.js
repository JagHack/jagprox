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
        
        if (!message.includes('WINNER!')) {
            return;
        }

        if (Date.now() - this.lastEventTimestamp < this.debouncePeriod) {
            return;
        }
        
        if (!this.mc_uuid || !this.localPlayerName || !this.gametrackApiHandler) {
            console.error('[GameTrack] Missing UUID, Player Name, or API Handler. Cannot track game.');
            return;
        }

        const winnerIndex = message.indexOf('WINNER!');
        
        if (!this.currentGame || this.currentGame === 'limbo') {
            return;
        }
        
        if (message.includes('Lobby') || message.includes('Replay') || message.includes('Spectator')) {
            return;
        }

        let result = null;
        const beforeText = message.substring(0, winnerIndex).trim();
        const parts = beforeText.split(' ').filter(p => p); // Split by space and remove empty parts

        if (parts.length > 0) {
            const winnerName = parts[parts.length - 1]; // The last name before "WINNER!"
            
            if (winnerName === this.localPlayerName) {
                result = 'win';
            } else if (message.includes(this.localPlayerName)) {

                result = 'loss';
            }
        }
        
        if (result) {
            this.lastEventTimestamp = Date.now();
            console.log(`[GameTrack] Detected ${result} in ${this.currentGame} for ${this.localPlayerName}`);
            
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
