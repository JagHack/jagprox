const formatter = require('../formatter.js');

class AutoGGHandler {
    constructor(proxy) {
        this.proxy = proxy;
        this.ggSentInGame = false;
    }

    reset() {
        this.ggSentInGame = false;
    }

    handlePacket(data, meta) {
        if (meta.name !== 'title' || !this.proxy.config.auto_gg || !this.proxy.config.auto_gg.enabled) {
            return;
        }

        if (data.action === 0) {
            let cleanTitle = '';
            try {
                const titleObject = JSON.parse(data.text);
                cleanTitle = formatter.extractText(titleObject).trim();
            } catch (e) {
                cleanTitle = data.text.replace(/§[0-9a-fk-or]/g, '').trim();
            }

            const gameOverKeywords = [
                'VICTORY!', 'YOU WIN!', 'GAME OVER', 'DRAW!', '1ST PLACE',
                '#1 VICTORY', 'WINNER', 'GAMEOVER!', 'GAME OVER', 'DEFEAT', 'DEFEAT!'
            ];

            if (!this.ggSentInGame && gameOverKeywords.some(keyword => cleanTitle.toUpperCase().includes(keyword))) {
                this.ggSentInGame = true;
                const delay = this.proxy.config.auto_gg.delay || 1500;
                const message = this.proxy.config.auto_gg.message || "gg";

                setTimeout(() => {
                    if (this.proxy.target) {
                        this.proxy.target.write('chat', { message: '/ac ' + message });
                        formatter.log(`AutoGG (Title) sent: "${message}"`);
                    }
                }, delay);
            }
        }
    }
}

module.exports = AutoGGHandler;