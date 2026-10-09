const formatter = require('../formatter.js');

// Hypixel Bed Wars team prefixes start with the team letter (after any § codes).
const BED_COLORS = {
    R: { code: '§c', int: 12 },
    B: { code: '§9', int: 9 },
    G: { code: '§a', int: 10 },
    Y: { code: '§e', int: 14 },
    A: { code: '§b', int: 11 },
    W: { code: '§f', int: 15 },
    P: { code: '§d', int: 13 },
    S: { code: '§8', int: 8 },
};
const DEFAULT_BED_COLOR = { code: '§f', int: 15 };

function bedColorFromPrefix(hypixelPrefix) {
    if (!hypixelPrefix) return DEFAULT_BED_COLOR;
    const letter = hypixelPrefix.replace(/^(§.)+/, '').charAt(0).toUpperCase();
    return BED_COLORS[letter] || DEFAULT_BED_COLOR;
}

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
        formatter.debug('Player Tag Manager reset.');
        this.playerTeamMap.clear();
        this.teamColorMap.clear();
        this.teamPrefixMap.clear();
        this.teamSuffixMap.clear();
        this.teamOurPrefix.clear();
        this.teamOurSuffix.clear();
        // teamCounter is deliberately never reset: the client may still hold our old
        // jpN teams, and creating a team that already exists kicks a 1.8 client.
    }

    removeTeam(team) {
        this.teamColorMap.delete(team);
        this.teamPrefixMap.delete(team);
        this.teamSuffixMap.delete(team);
        this.teamOurPrefix.delete(team);
        this.teamOurSuffix.delete(team);
        for (const [player, playerTeam] of this.playerTeamMap) {
            if (playerTeam === team) this.playerTeamMap.delete(player);
        }
    }

    handlePacket(data, meta) {
        // Hypixel sends a respawn on every server switch; the new server has its own teams.
        if (meta.name === 'login' || meta.name === 'respawn') {
            this.reset();
            return;
        }
        if (meta.name !== 'scoreboard_team') return;

        const mode = data.mode;

        if (mode === 1) {
            this.removeTeam(data.team);
            return;
        }

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

            // Keep our stat tags when Hypixel updates a team we've already tagged.
            const ourPrefix = this.teamOurPrefix.get(data.team);
            const ourSuffix = this.teamOurSuffix.get(data.team);

            if (ourSuffix) {
                const base = this.teamSuffixMap.get(data.team) || '';
                data.suffix = (base + ourSuffix).substring(0, 16);
            }

            if (ourPrefix) {
                data.prefix = ourPrefix.substring(0, 16);
                data.color = bedColorFromPrefix(this.teamPrefixMap.get(data.team)).int;
            }
        }
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
            // The player may have disconnected while we were fetching.
            const client = this.proxy.client;
            if (!playerData || !client) return;

            const hypixelTeam = this.playerTeamMap.get(name.toLowerCase());

            if (hypixelTeam) {
                let hypixelPrefix = this.teamPrefixMap.get(hypixelTeam) || '';
                if (!hypixelPrefix.trim()) {
                    await new Promise(r => setTimeout(r, 500));
                    hypixelPrefix = this.teamPrefixMap.get(hypixelTeam) || '';
                }

                const bedColor = bedColorFromPrefix(hypixelPrefix);
                const finalPrefix = (playerData.prefix + bedColor.code).substring(0, 16);
                const baseSuffix = this.teamSuffixMap.get(hypixelTeam) || '';
                const finalSuffix = (baseSuffix + playerData.suffix).substring(0, 16);

                this.teamOurPrefix.set(hypixelTeam, finalPrefix);
                this.teamOurSuffix.set(hypixelTeam, playerData.suffix);

                client.write('scoreboard_team', {
                    team:              hypixelTeam,
                    mode:              2,
                    name:              hypixelTeam,
                    prefix:            finalPrefix,
                    suffix:            finalSuffix,
                    friendlyFire:      0,
                    nameTagVisibility: 'always',
                    color:             bedColor.int,
                    players:           []
                });

                formatter.debug(`Tagged ${name}: prefix="${finalPrefix}" suffix="${finalSuffix}" color=${bedColor.int}`);
            } else {
                const teamName = `jp${this.teamCounter++}`;
                const finalPrefix = (playerData.prefix + DEFAULT_BED_COLOR.code).substring(0, 16);
                const finalSuffix = playerData.suffix.substring(0, 16);

                this.teamOurPrefix.set(teamName, finalPrefix);
                this.teamOurSuffix.set(teamName, playerData.suffix);

                client.write('scoreboard_team', {
                    team: teamName, mode: 0, name: teamName,
                    prefix: finalPrefix,
                    suffix: finalSuffix,
                    friendlyFire: 0, nameTagVisibility: 'always',
                    color: DEFAULT_BED_COLOR.int, players: []
                });
                client.write('scoreboard_team', { team: teamName, mode: 3, players: [name] });
            }
        } catch (e) {
            formatter.log(`Error in createOrUpdatePlayerTag for "${name}": ${e.message}`);
        }
    }
}

module.exports = TabManager;
