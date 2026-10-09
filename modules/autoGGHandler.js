const formatter = require('../formatter.js');

const GAME_OVER_KEYWORDS = [
    'VICTORY!', 'YOU WIN!', 'GAME OVER', 'GAMEOVER!', 'DRAW!', '1ST PLACE',
    '#1 VICTORY', 'WINNER', 'DEFEAT'
];

class AutoGGHandler {
    constructor(proxy) {
        this.proxy = proxy;
        this.ggSentInGame = false;
    }

    reset() {
        this.ggSentInGame = false;
    }

    handlePacket(data, meta) {
        const autoGG = this.proxy.config.auto_gg;
        if (meta.name !== 'title' || data.action !== 0 || !autoGG || !autoGG.enabled || this.ggSentInGame) {
            return;
        }

        let cleanTitle;
        try {
            cleanTitle = formatter.stripColors(formatter.extractText(JSON.parse(data.text))).trim();
        } catch (e) {
            cleanTitle = formatter.stripColors(data.text || '').trim();
        }

        const upperTitle = cleanTitle.toUpperCase();
        if (!GAME_OVER_KEYWORDS.some(keyword => upperTitle.includes(keyword))) return;

        // Win/loss recording is GametrackClientHandler's job; this only sends the message.
        this.ggSentInGame = true;
        const delay = autoGG.delay ?? 1500;
        const message = autoGG.message || "gg";

        setTimeout(() => {
            if (this.proxy.target) {
                this.proxy.target.write('chat', { message: '/ac ' + message });
                formatter.log(`AutoGG (Title) sent: "${message}"`);
            }
        }, delay);
    }
}

module.exports = AutoGGHandler;
