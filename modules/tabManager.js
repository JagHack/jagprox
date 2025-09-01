const formatter = require('../formatter.js');

class TabManager {
    constructor(proxy) {
        this.proxy = proxy;
        this.teamCounter = 0;
    }

    reset() {
        formatter.log(`Player Tag Manager reset signal received.`);
        this.teamCounter = 0;
    }

    handlePacket(data, meta) {}

    async updatePlayerTags(playerNames, gamemodeKey) {
        formatter.log(`Updating player name tags for ${playerNames.length} players...`);
        if (!this.proxy.client || this.proxy.client.state !== 'play') return;

        for (const name of playerNames) {
            await this.createOrUpdatePlayerTag(name, gamemodeKey);
            await new Promise(resolve => setTimeout(resolve, 50));
        }

        formatter.log(`Finished processing player name tags.`);
    }

    async createOrUpdatePlayerTag(name, gamemodeKey) {
        try {
            if (!name || typeof name !== 'string' || name.length < 3) return;

            const playerData = await this.proxy.hypixel.getTabDataForPlayer(name, gamemodeKey);

            const suffix = (playerData && playerData.suffix) ? playerData.suffix : '';

            if (!suffix) {
                return;
            }

            const teamName = `jp${this.teamCounter++}`;

            // 1. Create team
            this.proxy.client.write('scoreboard_team', {
                team: teamName,
                mode: 0,
                name: teamName,
                friendlyFire: 0,
                nameTagVisibility: 'always',
                color: 7, // Sets the name color to GRAY
                prefix: '', // IMPORTANT: Empty prefix to overwrite the Hypixel rank
                suffix: suffix,
                players: []
            });

            // 2. Add player to team
            this.proxy.client.write('scoreboard_team', {
                team: teamName,
                mode: 3,
                players: [name]
            });

        } catch (error) {
            console.error(`A non-fatal error occurred while updating tag for player "${name}":`, error.message, error.code || '');
        }
    }
}

module.exports = TabManager;