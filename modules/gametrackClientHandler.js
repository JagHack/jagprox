const formatter = require('../formatter.js');
const { detectGameResult } = require('../utils/chatParsers.js');

class GametrackClientHandler {
    constructor(proxy, mc_uuid, localPlayerName) {
        this.proxy = proxy;
        this.gametrackApiHandler = proxy.gametrackApiHandler;

        this.mc_uuid = mc_uuid;
        this.localPlayerName = localPlayerName;
        this.currentGame = null;

        this.lastEventTimestamp = 0;
        this.debouncePeriod = 5000;
    }

    async onGameChanged(newGameKey) {
        if (!this.mc_uuid || !newGameKey || newGameKey === 'limbo') {
            this.currentGame = null;
            return;
        }

        formatter.log(`[GameTrack] Game changed to ${newGameKey}. Starting session.`);
        this.currentGame = newGameKey;

        try {
            await this.gametrackApiHandler.sendStartEvent({
                mc_uuid: this.mc_uuid,
                mode: this.currentGame
            });
        } catch (e) {
            formatter.log(`[GameTrack] Failed to start session: ${e.message}`);
            this.proxy.proxyChat(`§c[GameTrack] §8Error starting session: ${e.message}`);
        }
    }

    async parseChatMessage(chatObject) {
        if (!this.currentGame || !this.mc_uuid || !this.gametrackApiHandler) return;
        if (Date.now() - this.lastEventTimestamp < this.debouncePeriod) return;

        const result = detectGameResult(formatter.extractText(chatObject));
        if (!result) return;

        this.lastEventTimestamp = Date.now();
        formatter.log(`[GameTrack] Detected ${result} in ${this.currentGame}.`);

        try {
            await this.gametrackApiHandler.sendEvent({
                mc_uuid: this.mc_uuid,
                mode: this.currentGame,
                result: result
            });
            this.proxy.proxyChat(`§d[GameTrack] §8Recorded game result: §5${result.toUpperCase()}§8.`);
        } catch (e) {
            this.proxy.proxyChat(`§c[GameTrack] §8Error recording event: ${e.message}`);
            formatter.log(`[GameTrack] Failed to send gametrack event: ${e.message}`);
        }
    }
}

module.exports = GametrackClientHandler;
