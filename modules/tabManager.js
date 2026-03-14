const formatter = require('../formatter.js');

class TabManager {
    constructor(proxy) {
        this.proxy = proxy;
        this.uuidToNameMap = new Map();
        this.playerTeamMap = new Map();
        this.teamSuffixMap = new Map();
        this.teamCounter = 0;
    }

    reset() {
        formatter.log(`Player Tag Manager and UUID cache reset.`);
        this.uuidToNameMap.clear();
        this.playerTeamMap.clear();
        this.teamSuffixMap.clear();
        this.teamCounter = 0;
    }

    normalizeUUID(uuid) {
        if (!uuid) return null;
        if (Buffer.isBuffer(uuid)) return uuid;
        if (typeof uuid === 'string') {
            const clean = uuid.replace(/-/g, '');
            if (clean.length === 32) return Buffer.from(clean, 'hex');
        }
        return uuid;
    }

    handlePacket(data, meta) {
        if (meta.name === 'player_info') {
            const action = data.action;
            const players = data.data;
            if (!players || !Array.isArray(players)) return;

            for (const player of players) {
                const rawUUID = player.UUID || player.uuid;
                if (!rawUUID) continue;

                const uuidBuffer = this.normalizeUUID(rawUUID);
                if (!uuidBuffer || uuidBuffer.length !== 16) continue;
                
                const uuidHex = uuidBuffer.toString('hex');

                if (action === 'add_player' || action === 0) {
                    if (player.name) {
                        this.uuidToNameMap.set(uuidHex, player.name);
                    }
                } else if (action === 'remove_player' || action === 4) {
                    this.uuidToNameMap.delete(uuidHex);
                }
            }
        }

        if (meta.name === 'scoreboard_team') {
            if ((data.mode === 0 || data.mode === 3) && data.players) {
                for (const player of data.players) {
                    this.playerTeamMap.set(player, data.team);
                }
            }
            if (data.mode === 4 && data.players) {
                for (const player of data.players) {
                    this.playerTeamMap.delete(player);
                }
            }

            if ((data.mode === 0 || data.mode === 2) && this.teamSuffixMap.has(data.team)) {
                data.suffix = this.teamSuffixMap.get(data.team);
            }
        }
    }

    getPlayerNameByUUID(uuid) {
        if (!uuid) return null;
        const normalized = this.normalizeUUID(uuid);
        if (!normalized || normalized.length !== 16) return null;
        return this.uuidToNameMap.get(normalized.toString('hex')) || null;
    }

    async updatePlayerTags(playerNames, gamemodeKey) {
        if (!this.proxy.client || this.proxy.client.state !== 'play') return;
        formatter.log(`Updating player tab display for ${playerNames.length} players...`);

        for (const name of playerNames) {
            await this.createOrUpdatePlayerTag(name, gamemodeKey);
            await new Promise(r => setTimeout(r, 100));
        }
    }

    async createOrUpdatePlayerTag(name, gamemodeKey) {
        if (!this.proxy.client || this.proxy.client.state !== 'play') return;

        try {
            if (!name || typeof name !== 'string') return;

            const playerData = await this.proxy.hypixel.getTabDataForPlayer(name, gamemodeKey);
            if (!playerData || !playerData.suffix || typeof playerData.suffix !== 'string') return;

            const suffix = playerData.suffix.substring(0, 16);
            const hypixelTeam = this.playerTeamMap.get(name);

            if (hypixelTeam) {
                this.teamSuffixMap.set(hypixelTeam, suffix);

                this.proxy.client.write('scoreboard_team', {
                    team: hypixelTeam,
                    mode: 2,
                    name: hypixelTeam,
                    prefix: '',
                    suffix: suffix,
                    friendlyFire: 0,
                    nameTagVisibility: 'always',
                    color: 15,
                    players: []
                });
            } else {
                const teamName = `jp${this.teamCounter++}`.substring(0, 16);
                this.proxy.client.write('scoreboard_team', {
                    team: teamName,
                    mode: 0,
                    name: teamName,
                    prefix: '',
                    suffix: suffix,
                    friendlyFire: 0,
                    nameTagVisibility: 'always',
                    color: 15,
                    players: []
                });
                this.proxy.client.write('scoreboard_team', {
                    team: teamName,
                    mode: 3,
                    players: [name]
                });
            }

            formatter.log(`Updated tag for ${name} (team: ${hypixelTeam || 'fallback'}) suffix: ${suffix}`);

        } catch (error) {
            formatter.log(`Error updating team for "${name}": ${error.message}`);
        }
    }
}

module.exports = TabManager;
