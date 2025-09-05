const fetch = require('node-fetch');
const Jimp = require('jimp');
const formatter = require("../formatter.js");
const { findClosestMinecraftColor, gameModeMap } = require("../utils/constants");

class HypixelHandler {
    constructor(proxy) {
        this.proxy = proxy;
    }

    async getStatsForAPI(gamemode, username) {
        const gameInfo = gameModeMap[gamemode.toLowerCase()];
        if (!gameInfo) {
            return { error: `Unknown game mode: ${gamemode}` };
        }
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) return { error: `Player '${username}' not found.` };

            const stats = await this.getStats(mojangData.uuid, gameInfo.apiName);
            if (!stats) return { error: `No ${gameInfo.displayName} stats found for '${mojangData.username}'.` };

            return {
                username: mojangData.username,
                uuid: mojangData.uuid,
                game: gameInfo,
                stats: stats
            };
        } catch (err) {
            formatter.log(`API Statcheck error: ${err.message}`);
            return { error: 'An internal error occurred.' };
        }
    }

    handlePartyStatCheck() {
        this.proxy.proxyChat("§eRequesting party member list...");
        const partyMembers = new Set();
        let capturing = false;

        const partyListener = (data, meta) => {
            if (meta.name !== 'chat') return;

            let chatMessage = '';
            try {
                const chatData = JSON.parse(data.message);
                chatMessage = (chatData.extra || []).map(part => part.text).join('');
            } catch(e) { return; }

            const cleanMessage = chatMessage.replace(/§[0-9a-fk-or]/g, '').trim();

            if (cleanMessage.includes('You are not currently in a party.')) {
                this.proxy.proxyChat("§cYou are not in a party.");
                this.proxy.target.removeListener('packet', partyListener);
                return;
            }

            if (cleanMessage.startsWith('Party Members:')) {
                capturing = true;
            }

            if (capturing && (cleanMessage.startsWith('Party Leader:') || cleanMessage.startsWith('Party Moderators:') || cleanMessage.startsWith('Party Members:'))) {
                 const players = cleanMessage.split(':')[1].split(',').map(p => p.trim());
                 players.forEach(p => partyMembers.add(p));
            }

            if (capturing && cleanMessage.startsWith('Total Members:')) {
                capturing = false;
                this.proxy.target.removeListener('packet', partyListener);
                this.processPartyMembers(Array.from(partyMembers));
            }
        };

        this.proxy.target.on('packet', partyListener);
        this.proxy.target.write('chat', { message: '/party list' });

        setTimeout(() => {
            this.proxy.target.removeListener('packet', partyListener);
        }, 5000);
    }

    async processPartyMembers(partyMembers) {
        if (partyMembers.length === 0) {
            this.proxy.proxyChat("§cCould not find any party members.");
            return;
        }

        this.proxy.proxyChat(`§aFound ${partyMembers.length} members. Fetching Bedwars stats...`);
        this.proxy.proxyChat("§7§m----------------------------------------");

        const statPromises = partyMembers.map(username =>
            this.getAndFormatPlayerStats(username.replace(/\[.*?\]\s/g, ''), gameModeMap.bedwars)
        );
        const statLines = await Promise.all(statPromises);

        statLines.forEach(line => {
            if (line) this.proxy.proxyChat(line);
        });
        this.proxy.proxyChat("§7§m----------------------------------------");
    }

    async getAndFormatPlayerStats(username, gameInfo) {
        try {
            const cleanUsername = username.replace(/§[0-9a-fk-or]/g, '').replace(/\[.*?\]\s/g, '');
            const mojangData = await this.getMojangUUID(cleanUsername);
            if (!mojangData) return `§c§o'${cleanUsername}' not found.`;
            const stats = await this.getStats(mojangData.uuid, gameInfo.apiName);
            if (!stats || !stats.data) {
                return `§7No ${gameInfo.displayName} stats for ${formatter.formatRank(null)} ${mojangData.username}§7.`;
            }
            return this.formatSinglePlayerQueueStats(mojangData.username, stats, gameInfo);
        } catch (err) {
            formatter.log(`Queue stat check error for ${username}: ${err.message}`);
            return `§cError for ${username}.`;
        }
    }

    formatSinglePlayerQueueStats(username, stats, gameInfo) {
        const d = stats.data;
        const a = stats.achievements;
        let statLine = "";
        const rank = formatter.formatRank(stats.rank);
        switch (gameInfo.apiName) {
            case "Bedwars":
                const fkdr = ((d.final_kills_bedwars || 0) / (d.final_deaths_bedwars || 1)).toFixed(2);
                const wlrBw = ((d.wins_bedwars || 0) / (d.losses_bedwars || 1)).toFixed(2);
                statLine = `§7[§f${a.bedwars_level || 0}✫§7] ${rank} ${username} §8- §fFKDR: §6${fkdr} §8| §fWLR: §6${wlrBw}`;
                break;
            default:
                const wins = d.wins || 0;
                statLine = `${rank} ${username} §8- §fWins: §a${wins.toLocaleString()}`;
        }
        return statLine;
    }

    async getMojangUUID(username) {
        try {
            const response = await fetch(`https://api.mojang.com/users/profiles/minecraft/${username}`);
            if (!response.ok) {
                if (response.status === 429) this.proxy.proxyChat("§cMojang API Rate Limit.");
                return null;
            }
            const data = await response.json();
            return { uuid: data.id, username: data.name };
        } catch (err) {
            formatter.log(`Mojang API Error: ${err.message}`);
            return null;
        }
    }

    async getGuild(uuid) {
        try {
            const response = await fetch(`https://api.hypixel.net/v2/guild?key=${this.proxy.env.apiKey}&player=${uuid}`);
            if (!response.ok) return null;
            const data = await response.json();
            return data.guild ? data.guild.name : null;
        } catch (err) {
            formatter.log(`getGuild Error: ${err.message}`);
            return null;
        }
    }

    async getPlayerStatus(username) {
        this.proxy.proxyChat(`§eChecking status for ${username}...`);
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) return this.proxy.proxyChat(`§cPlayer '${username}' not found.`);
            const status = await this.getHypixelStatus(mojangData.uuid);
            if (!status) return this.proxy.proxyChat(`§cCould not retrieve status for '${mojangData.username}'.`);
            this.proxy.proxyChat("§7§m----------------------------------------");
            if (status.online) {
                this.proxy.proxyChat(`${formatter.formatRank(status.rank)} ${mojangData.username} §ais Online.`);
                if (status.hidden) {
                    this.proxy.proxyChat(`§7(Status is hidden, game info unavailable)`);
                } else {
                    this.proxy.proxyChat(`§fGame: §b${status.gameType}`);
                    if (status.mode) this.proxy.proxyChat(`§fMode: §e${status.mode}`);
                    if (status.map) this.proxy.proxyChat(`§fMap: §e${status.map}`);
                }
            } else {
                this.proxy.proxyChat(`${formatter.formatRank(status.rank)} ${mojangData.username} §cis Offline.`);
            }
            this.proxy.proxyChat("§7§m----------------------------------------");
        } catch (err) {
            formatter.log(`Status check error: ${err.message}`);
            this.proxy.proxyChat(`§cAn error occurred.`);
        }
    }

    async getHypixelStatus(uuid) {
        try {
            const [statusResponse, playerResponse] = await Promise.all([
                fetch(`https://api.hypixel.net/v2/status?key=${this.proxy.env.apiKey}&uuid=${uuid}`),
                fetch(`https://api.hypixel.net/v2/player?key=${this.proxy.env.apiKey}&uuid=${uuid}`)
            ]);
            const statusData = await statusResponse.json();
            const playerData = await playerResponse.json();
            if (!playerData.success || !playerData.player) return null;
            const player = playerData.player;
            const rank = (player.monthlyPackageRank && player.monthlyPackageRank === "SUPERSTAR") ? "MVP_PLUS_PLUS" : (player.newPackageRank || "NONE");
            if (statusData.success && statusData.session.online) {
                return { online: true, hidden: false, gameType: statusData.session.gameType, mode: statusData.session.mode, map: statusData.session.map, rank };
            }
            if ((player.lastLogin || 0) > (player.lastLogout || 0)) {
                return { online: true, hidden: true, rank };
            }
            return { online: false, rank };
        } catch (err) {
            formatter.log(`getHypixelStatus Error: ${err.message}`);
            return null;
        }
    }

    async statcheck(gamemode, username) {
        const gameInfo = gameModeMap[gamemode.toLowerCase()];
        if (!gameInfo) {
            return this.proxy.proxyChat(`§cUnknown game mode: ${gamemode}`);
        }
        this.proxy.proxyChat(`§eChecking ${gameInfo.displayName} stats for ${username}...`);
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) return this.proxy.proxyChat(`§cPlayer '${username}' not found.`);
            const stats = await this.getStats(mojangData.uuid, gameInfo.apiName);
            if (!stats) return this.proxy.proxyChat(`§cNo ${gameInfo.displayName} stats found for '${mojangData.username}'.`);
            this.displayFormattedStats(mojangData.username, mojangData.uuid, stats, gameInfo);
        } catch (err) {
            formatter.log(`Statcheck error: ${err.message}`);
            this.proxy.proxyChat(`§cAn error occurred.`);
        }
    }

    async getStats(uuid, gameApiName) {
        try {
            const response = await fetch(`https://api.hypixel.net/v2/player?key=${this.proxy.env.apiKey}&uuid=${uuid}`);
            if (!response.ok) return null;
            const data = await response.json();
            if (!data.success || !data.player) return null;
            const player = data.player;
            const gameData = player.stats ? player.stats[gameApiName] : {};
            return {
                rank: (player.monthlyPackageRank && player.monthlyPackageRank === "SUPERSTAR") ? "MVP_PLUS_PLUS" : (player.newPackageRank || "NONE"),
                guild: await this.getGuild(uuid),
                data: gameData || {},
                achievements: player.achievements || {}
            };
        } catch (err) {
            formatter.log(`getStats Error: ${err.message}`);
            return null;
        }
    }

    async displayFormattedStats(username, uuid, stats, gameInfo) {
        try {
            const image = await Jimp.read(`https://crafatar.com/avatars/${uuid}?size=8&overlay=true`);
            const asciiLines = [];
            for (let y = 0; y < 8; y++) {
                let line = "";
                for (let x = 0; x < 8; x++) {
                    const pixel = Jimp.intToRGBA(image.getPixelColor(x, y));
                    line += (pixel.a > 128) ? findClosestMinecraftColor(pixel.r, pixel.g, pixel.b) + '█' : " ";
                }
                asciiLines.push(line);
            }
            const rank = formatter.formatRank(stats.rank);
            const guild = stats.guild ? ` §e[${stats.guild}]` : "";
            this.proxy.proxyChat("§7§m----------------------------------------");
            asciiLines.forEach(line => this.proxy.client.write("chat", { message: JSON.stringify({ text: line }), position: 1 }));
            this.proxy.proxyChat(" ");
            const d = stats.data;
            const a = stats.achievements;
            let lines = [];
            switch (gameInfo.apiName) {
                case "Bedwars":
                    lines.push(`${rank} ${username} §7[§f${a.bedwars_level || 0}✫§7]${guild}`);
                    lines.push(`§fWins: §a${(d.wins_bedwars || 0).toLocaleString()} §8| §fLosses: §c${(d.losses_bedwars || 1).toLocaleString()}`);
                    lines.push(`§fFinal Kills: §a${(d.final_kills_bedwars || 0).toLocaleString()} §8| §fFinal Deaths: §c${(d.final_deaths_bedwars || 1).toLocaleString()}`);
                    const fkdr = ((d.final_kills_bedwars || 0) / (d.final_deaths_bedwars || 1)).toFixed(2);
                    const bblr = ((d.beds_broken_bedwars || 0) / (d.beds_lost_bedwars || 1)).toFixed(2);
                    const wlrBw = ((d.wins_bedwars || 0) / (d.losses_bedwars || 1)).toFixed(2);
                    lines.push(`§fFKDR: §6${fkdr} §8| §fBBLR: §6${bblr} §8| §fWLR: §6${wlrBw}`);
                    break;
                case "Duels":
                    const prefix = gameInfo.prefix || '';
                    const winsKey = prefix ? `${prefix}_wins` : 'wins';
                    const lossesKey = prefix ? `${prefix}_losses` : 'losses';
                    const killsKey = prefix ? `${prefix}_kills` : 'kills';
                    const deathsKey = prefix ? `${prefix}_deaths` : 'deaths';
                    const wins = d[winsKey] || 0;
                    const losses = d[lossesKey] || 1;
                    const kills = d[killsKey] || 0;
                    const deaths = d[deathsKey] || 1;
                    lines.push(`§f[${gameInfo.displayName}] ${rank} ${username} §7[§f${wins.toLocaleString()} Wins§7]${guild}`);
                    lines.push(`§fWins: §a${wins.toLocaleString()} §8| §fLosses: §c${losses.toLocaleString()}`);
                    lines.push(`§fKills: §a${kills.toLocaleString()} §8| §fDeaths: §c${deaths.toLocaleString()}`);
                    const wlr = (wins / losses).toFixed(2);
                    const kdr = (kills / deaths).toFixed(2);
                    lines.push(`§fWLR: §6${wlr} §8| §fKDR: §6${kdr}`);
                    break;
                case "SkyWars":
                    lines.push(`${rank} ${username} §7[§f${d.levelFormatted || '0✫'}§7]${guild}`);
                    lines.push(`§fWins: §a${(d.wins || 0).toLocaleString()} §8| §fLosses: §c${(d.losses || 1).toLocaleString()}`);
                    const wlrSw = ((d.wins || 0) / (d.losses || 1)).toFixed(2);
                    const KDRSw = ((d.kills || 0) / (d.deaths || 1)).toFixed(2);
                    lines.push(`§fKDR: §6${KDRSw} §8| §fWLR: §6${wlrSw}`);
                    break;
                default:
                    lines.push(`${rank} ${username}${guild}`);
                    lines.push(`§cStat display for ${gameInfo.displayName} is not implemented yet.`);
                    lines.push(`§eWins: §a${(d.wins || 'N/A').toLocaleString()}`);
            }
            lines.forEach(line => this.proxy.proxyChat(line));
            this.proxy.proxyChat("§7§m----------------------------------------");
        } catch (err) {
            formatter.log(`displayFormattedStats Error: ${err.message}`);
            this.proxy.proxyChat(`§cError displaying stats (could not load avatar?).`);
        }
    }
}

module.exports = HypixelHandler;