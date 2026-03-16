const formatter = require('../formatter.js');

class TabManager {
    constructor(proxy) {
        this.proxy = proxy;
        this.playerTeamMap  = new Map();
        this.teamColorMap   = new Map();
        this.teamPrefixMap  = new Map();
        this.teamSuffixMap  = new Map();
        this.teamOurPrefix  = new Map();
        this.teamOurSuffix  = new Map();
        this.teamCounter    = 0;
    }

    reset() {
        formatter.log('Player Tag Manager reset.');
        this.teamOurPrefix.clear();
        this.teamOurSuffix.clear();
        this.teamCounter = 0;
    }

    handlePacket(data, meta) {
        if (meta.name === 'scoreboard_team') {
            const mode = data.mode;

            if ((mode === 0 || mode === 3) && Array.isArray(data.players)) {
                for (const p of data.players) {
                    this.playerTeamMap.set(p.toLowerCase(), data.team);
                }
            }
            if (mode === 4 && Array.isArray(data.players)) {
                for (const p of data.players) {
                    this.playerTeamMap.delete(p.toLowerCase());
                }
            }

            if (mode === 0 || mode === 2) {
                if (data.prefix !== undefined) this.teamPrefixMap.set(data.team, data.prefix);
                if (data.suffix !== undefined) this.teamSuffixMap.set(data.team, data.suffix);
                if (data.color  !== undefined) this.teamColorMap.set(data.team, data.color);

                const ourPrefix = this.teamOurPrefix.get(data.team);
                const ourSuffix = this.teamOurSuffix.get(data.team);

                if (ourSuffix) {
                    const base = this.teamSuffixMap.get(data.team) || '';
                    data.suffix = (base + ourSuffix).substring(0, 16);
                }

                if (ourPrefix) {
                    data.prefix = ourPrefix.substring(0, 16);
                    const hypixelRawPrefix = this.teamPrefixMap.get(data.team) || '';
                    data.color = this.getBedColorInt(hypixelRawPrefix);
                }
            }
        }
    }

    getPlayerNameByUUID(uuid) { return null; }

    getBedColorFromPrefix(hypixelPrefix) {
        if (!hypixelPrefix) return '\u00A7f';
        let str = hypixelPrefix;
        while (str.length >= 2 && str.charCodeAt(0) === 167) {
            str = str.substring(2);
        }
        const letter = str.charAt(0).toUpperCase();
        const map = {
            'R': '\u00A7c',
            'B': '\u00A79',
            'G': '\u00A7a',
            'Y': '\u00A7e',
            'A': '\u00A7b',
            'W': '\u00A7f',
            'P': '\u00A7d',
            'S': '\u00A77',
        };
        return map[letter] || '\u00A7f';
    }

    getBedColorInt(hypixelPrefix) {
        if (!hypixelPrefix) return 15;
        let str = hypixelPrefix;
        while (str.length >= 2 && str.charCodeAt(0) === 167) {
            str = str.substring(2);
        }
        const letter = str.charAt(0).toUpperCase();
        const map = {
            'R': 12,
            'B': 9,
            'G': 10,
            'Y': 14,
            'A': 11,
            'W': 15,
            'P': 13,
            'S': 7,
        };
        return map[letter] ?? 15;
    }

    async updatePlayerTags(playerNames, gamemodeKey) {
        if (!this.proxy.client || this.proxy.client.state !== 'play') return;
        formatter.log(`Updating player tab display for ${playerNames.length} players...`);
        for (const name of playerNames) {
            await this.createOrUpdatePlayerTag(name, gamemodeKey);
            await new Promise(r => setTimeout(r, 150));
        }
        formatter.log('Finished updating player tags.');
    }

    async createOrUpdatePlayerTag(name, gamemodeKey) {
        if (!this.proxy.client || this.proxy.client.state !== 'play') return;
        if (!name || typeof name !== 'string') return;

        try {
            const playerData = await this.proxy.hypixel.getTabDataForPlayer(name, gamemodeKey);
            if (!playerData) return;

            const hypixelTeam = this.playerTeamMap.get(name.toLowerCase());

            if (hypixelTeam) {
                let hypixelPrefix = this.teamPrefixMap.get(hypixelTeam) || '';
                if (!hypixelPrefix || hypixelPrefix.trim().length === 0) {
                    await new Promise(r => setTimeout(r, 500));
                    hypixelPrefix = this.teamPrefixMap.get(hypixelTeam) || '';
                }
                
                const bedColorInt = this.getBedColorInt(hypixelPrefix);
                const bedColorCode = this.getBedColorFromPrefix(hypixelPrefix);
                
                const finalPrefix = (playerData.prefix + bedColorCode).substring(0, 16);
                const baseSuffix = this.teamSuffixMap.get(hypixelTeam) || '';
                const finalSuffix = (baseSuffix + playerData.suffix).substring(0, 16);

                this.teamOurPrefix.set(hypixelTeam, finalPrefix);
                this.teamOurSuffix.set(hypixelTeam, playerData.suffix);

                this.proxy.client.write('scoreboard_team', {
                    team:              hypixelTeam,
                    mode:              2,
                    name:              hypixelTeam,
                    prefix:            finalPrefix,
                    suffix:            finalSuffix,
                    friendlyFire:      0,
                    nameTagVisibility: 'always',
                    color:             bedColorInt,
                    players:           []
                });

                formatter.log(`Tagged ${name}: prefix="${finalPrefix}" suffix="${finalSuffix}" color=${bedColorInt}`);
            } else {
                const teamName = `jp${this.teamCounter++}`;
                const finalPrefix = (playerData.prefix + '\u00A7f').substring(0, 16);
                const finalSuffix = playerData.suffix.substring(0, 16);

                this.teamOurPrefix.set(teamName, finalPrefix);
                this.teamOurSuffix.set(teamName, playerData.suffix);

                const bedColorInt = 15;
                this.proxy.client.write('scoreboard_team', {
                    team: teamName, mode: 0, name: teamName,
                    prefix: finalPrefix, 
                    suffix: finalSuffix,
                    friendlyFire: 0, nameTagVisibility: 'always',
                    color: bedColorInt, players: []
                });
                this.proxy.client.write('scoreboard_team', { team: teamName, mode: 3, players: [name] });
            }
        } catch (e) {
            formatter.log(`Error in createOrUpdatePlayerTag for "${name}": ${e.message}`);
        }
    }
}

module.exports = TabManager;
