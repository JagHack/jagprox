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

                let result = null;
                const upperTitle = cleanTitle.toUpperCase();

                if (upperTitle.includes('WINNER!')) {
                    if (upperTitle.endsWith('WINNER!')) {

                        const before = upperTitle.replace('WINNER!', '').trim();
                        const words = before.split(' ').filter(w => w.length > 0);
                        if (words.length >= 2) {
                            result = 'loss';
                        }
                    } else {

                        const parts = upperTitle.split('WINNER!');
                        if (parts.length === 2 && parts[0].trim().length > 0 && parts[1].trim().length > 0) {
                            result = 'win';
                        }
                    }
                }

                if (result) {
                    this.proxy.gametrack.recordEvent(result);
                }


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
