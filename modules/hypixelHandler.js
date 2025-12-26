const fetch = require('node-fetch');
const Jimp = require('jimp');
const formatter = require("../formatter.js");
const { findClosestMinecraftColor, gameModeMap } = require("../utils/constants");

class HypixelHandler {
    constructor(proxy) {
        this.proxy = proxy;
        this.apiKeyCache = null;
        this.apiKeyCacheTime = null;
        this.apiHandler = null;
    }

    async getApiKey() {
        // Case 1: Launcher context with a pre-fetched key
        if (this.proxy.env.apiKey) {
            return this.proxy.env.apiKey;
        }

        // Case 2: Proxy context, check cache first
        if (this.apiKeyCache && this.apiKeyCacheTime && (Date.now() - this.apiKeyCacheTime < 60000)) { // 1-minute cache
            return this.apiKeyCache;
        }

        // Case 3: Proxy context, fetch from backend
        if (this.proxy.env.jwt) {
            if (!this.apiHandler) { // Initialize on first use
                const ApiHandler = require('../utils/apiHandler.js');
                this.apiHandler = new ApiHandler({ jwt: this.proxy.env.jwt });
            }
            try {
                const apiKey = await this.apiHandler.getApiKey();
                if (apiKey) {
                    this.apiKeyCache = apiKey;
                    this.apiKeyCacheTime = Date.now();
                    return apiKey;
                } else {
                    // This can happen if the user simply hasn't set a key yet.
                    // Only log it, don't spam the user's chat unless a command fails.
                    formatter.log('Could not retrieve Hypixel API key from backend. It might not be set.');
                    return null;
                }
            } catch (e) {
                this.proxy.proxyChat(`§cError fetching API Key: ${e.message}`);
                return null;
            }
        }
        
        // Case 4: No key or JWT available
        formatter.log('API Key is not configured in any context.');
        return null;
    }

    resolveNickname(name) {
        const nicknames = this.proxy.config.nicknames || {};
        const lowerName = name.toLowerCase();
        for (const realName in nicknames) {
            if (nicknames[realName].toLowerCase() === lowerName) {
                return realName;
            }
        }
        const realName = Object.keys(nicknames).find(key => key.toLowerCase() === lowerName);
        if (realName) {
            return realName;
        }

        return name;
    }

    async getStatusForAPI(username) {
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) return { error: `Player '${username}' not found.` };
            const status = await this.getHypixelStatus(mojangData.uuid);
            if (!status) return { error: `Could not retrieve status for '${mojangData.username}'.` };

            return { username: mojangData.username, uuid: mojangData.uuid, ...status };
        } catch (err) {
            formatter.log(`API Status Check error: ${err.message}`);
            return { error: 'An internal error occurred.' };
        }
    }

    async getStatsForAPI(gamemode, username) {
        const gameInfo = gameModeMap[gamemode.toLowerCase()];
        if (!gameInfo) {
            return { error: `Unknown game mode: ${gamemode}` };
        }
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) return { error: `Player '${username}' not found.` };

            const stats = await this.getStats(mojangData.uuid);
            if (!stats) return { error: `Could not retrieve player data for '${mojangData.username}'.` };

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

    handlePartyStatCheck(gamemode) {
        this.proxy.proxyChat("§eRequesting party member list...");
        const partyMembers = new Set();
        let capturing = false;

        const partyListener = (data, meta) => {
            if (meta.name !== 'chat') return;

            let chatMessage = '';
            try {
                const chatData = JSON.parse(data.message);
                chatMessage = formatter.extractText(chatData);
            } catch(e) { return; }

            const cleanMessage = chatMessage.replace(/§[0-9a-fk-or]/g, '').trim();

            if (cleanMessage.includes('You are not currently in a party.')) {
                this.proxy.proxyChat("§cYou are not in a party.");
                this.proxy.target.removeListener('packet', partyListener);
                return;
            }

            if (cleanMessage.startsWith('Party Members (') || cleanMessage.startsWith('Party Leader:')) {
                capturing = true;
            }
            
            if (capturing && (cleanMessage.startsWith('Party Leader:') || cleanMessage.startsWith('Party Moderators:') || cleanMessage.startsWith('Party Members:'))) {
                 const playersString = cleanMessage.split(':')[1];
                 if (playersString) {
                     const players = playersString.split(',').map(p => {
                         const cleaned = p.replace('●', '').trim();
                         const parts = cleaned.split(' ');
                         return parts[parts.length - 1];
                     });
                     players.forEach(p => p && partyMembers.add(p));
                 }
            }

            if (capturing && cleanMessage.startsWith('--------------------------------')) {
                if (partyMembers.size > 0) {
                    capturing = false;
                    this.proxy.target.removeListener('packet', partyListener);
                    this.processPartyMembers(Array.from(partyMembers), gamemode);
                }
            }
        };

        this.proxy.target.on('packet', partyListener);
        this.proxy.target.write('chat', { message: '/party list' });

        setTimeout(() => {
            if (capturing) {
                this.proxy.target.removeListener('packet', partyListener);
                if (partyMembers.size > 0) {
                    this.processPartyMembers(Array.from(partyMembers), gamemode);
                }
            }
        }, 5000);
    }

    async processPartyMembers(partyMembers, gamemode) {
        if (partyMembers.length === 0) {
            this.proxy.proxyChat("§cCould not find any party members.");
            return;
        }

        let gameInfo = gameModeMap[gamemode] || gameModeMap[this.proxy.queueStats.currentGameKey] || gameModeMap.bedwars;

        this.proxy.proxyChat(`§aFound ${partyMembers.length} members. Fetching stats for §e${gameInfo.displayName}§a...`);

        const statPromises = partyMembers.map(username =>
            this.getAndFormatPartyPlayerStats(username.replace(/\[.*?\]\s/g, ''), gameInfo)
        );
        const statBlocks = await Promise.all(statPromises);

        let finalMessage = `§d§m----------------------------------------------------\n`;
        finalMessage += `  §d§lParty Stats for ${gameInfo.displayName}\n \n`;
        finalMessage += statBlocks.filter(block => block).join('\n \n');
        finalMessage += `\n§d§m----------------------------------------------------`;
        this.proxy.proxyChat(finalMessage);
    }

    async getAndFormatPartyPlayerStats(username, gameInfo) {
        try {
            const cleanUsername = username.replace(/§[0-9a-fk-or]/g, '').replace(/\[.*?\]\s/g, '');
            if (!cleanUsername) return null;

            const realUsername = this.resolveNickname(cleanUsername);

            const mojangData = await this.getMojangUUID(realUsername);
            if (!mojangData) return `  §c§o'${cleanUsername}' not found.`;
            
            const stats = await this.getStats(mojangData.uuid);
            if (!stats || !stats.player.stats || !stats.player.stats[gameInfo.apiName]) {
                return `  ${formatter.formatRank(stats.rank)} ${mojangData.username} §7- No stats found.`;
            }
            
            const p = stats.player;
            const d = p.stats[gameInfo.apiName] || {};
            const a = p.achievements || {};
            const rank = formatter.formatRank(stats.rank);

            let statLines = [];
            let header = `  ${rank} ${mojangData.username}`;

            switch (gameInfo.apiName) {
                case "Bedwars":
                    header += ` §7[§f${a.bedwars_level || 0}✫§7]`;
                    statLines.push(`    §fFKDR: §6${((d.final_kills_bedwars || 0) / (d.final_deaths_bedwars || 1)).toFixed(2)} §8| §fWLR: §6${((d.wins_bedwars || 0) / (d.losses_bedwars || 1)).toFixed(2)}`);
                    break;
                case "SkyWars":
                    header += ` §7[§f${p.stats.SkyWars.levelFormatted || '0✫'}§7]`;
                    statLines.push(`    §fKDR: §6${((d.kills || 0) / (d.deaths || 1)).toFixed(2)} §8| §fWLR: §6${((d.wins || 0) / (d.losses || 1)).toFixed(2)}`);
                    break;
                case "Walls3":
                    statLines.push(`    §fWins: §a${(d.wins || 0).toLocaleString()} §8| §fFKDR: §6${((d.final_kills || 0) / (d.final_deaths || 1)).toFixed(2)}`);
                    break;
                case "Duels":
                     const wins = d.wins || 0; const losses = d.losses || 1;
                     const kills = d.kills || 0; const deaths = d.deaths || 1;
                     statLines.push(`    §fWLR: §6${(wins/losses).toFixed(2)} §8| §fKDR: §6${(kills/deaths).toFixed(2)}`);
                     break;
                case "UHC":
                    header += ` §7[§f${(a.uhc_champion || 0)}✫§7]`;
                    statLines.push(`    §fWins: §a${(d.wins || 0).toLocaleString()} §8| §fKDR: §6${((d.kills || 0) / (d.deaths || 1)).toFixed(2)}`);
                    break;
                default:
                    statLines.push(`    §fWins: §a${(d.wins || 'N/A').toLocaleString()} §8| §fKills: §a${(d.kills || 'N/A').toLocaleString()}`);
            }
            
            return `${header}\n${statLines.join('\n')}`;

        } catch (err) {
            formatter.log(`Party stat check error for ${username}: ${err.message}`);
            return `  §cError fetching stats for ${username}.`;
        }
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
        const apiKey = await this.getApiKey();
        if (!apiKey) return null;
        try {
            const response = await fetch(`https://api.hypixel.net/v2/guild?key=${apiKey}&player=${uuid}`);
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
        const apiKey = await this.getApiKey();
        if (!apiKey) return null;
        try {
            const [statusResponse, playerResponse] = await Promise.all([
                fetch(`https://api.hypixel.net/v2/status?key=${apiKey}&uuid=${uuid}`),
                fetch(`https://api.hypixel.net/v2/player?key=${apiKey}&uuid=${uuid}`)
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
        if (!gamemode || !username) {
            this.proxy.proxyChat("§cUsage: /sc <gamemode> <username>");
            return;
        }

        const gameInfo = gameModeMap[gamemode.toLowerCase()];
        if (!gameInfo) {
            return this.proxy.proxyChat(`§cUnknown game mode: ${gamemode}`);
        }
        this.proxy.proxyChat(`§eChecking ${gameInfo.displayName} stats for ${username}...`);
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) return this.proxy.proxyChat(`§cPlayer '${username}' not found.`);
            const stats = await this.getStats(mojangData.uuid);
            if (!stats) {
                // The getStats method now handles its own error messages for API key issues
                return this.proxy.proxyChat(`§cCould not retrieve data for '${mojangData.username}'.`);
            }
            if (!stats.player.stats || !stats.player.stats[gameInfo.apiName]) {
                return this.proxy.proxyChat(`§cNo ${gameInfo.displayName} stats found for '${mojangData.username}'.`);
            }
            this.displayFormattedStats(mojangData.username, mojangData.uuid, stats, gameInfo);
        } catch (err) {
            formatter.log(`Statcheck error: ${err.message}`);
            this.proxy.proxyChat(`§cAn error occurred.`);
        }
    }

    async getStats(uuid) {
        const apiKey = await this.getApiKey();
        if (!apiKey) return null;
        try {
            const response = await fetch(`https://api.hypixel.net/v2/player?key=${apiKey}&uuid=${uuid}`);
            if (!response.ok) return null;
            const data = await response.json();
            if (!data.success || !data.player) return null;
            const player = data.player;
            return {
                player: player,
                rank: (player.monthlyPackageRank && player.monthlyPackageRank === "SUPERSTAR") ? "MVP_PLUS_PLUS" : (player.newPackageRank || "NONE"),
                guild: await this.getGuild(uuid),
                properties: player.properties || []
            };
        } catch (err) {
            formatter.log(`getStats Error: ${err.message}`);
            return null;
        }
    }

    async displayFormattedStats(username, uuid, stats, gameInfo) {
        try {
            let image;
            try {
                const texturesProp = stats.properties.find(p => p.name === 'textures');
                if (!texturesProp) throw new Error("No texture property found");

                const texturesJson = Buffer.from(texturesProp.value, 'base64').toString('utf8');
                const textures = JSON.parse(texturesJson);
                const skinUrl = textures.textures?.SKIN?.url;
                if (!skinUrl) throw new Error("No skin URL found");

                const skin = await Jimp.read(skinUrl);
                image = new Jimp(8, 8);
                image.blit(skin, 0, 0, 8, 8, 8, 8);
                image.blit(skin, 0, 0, 40, 8, 8, 8);
            } catch (err) {
                formatter.log(`Manual skin processing failed: ${err.message}. Falling back to Crafatar.`);
                image = await Jimp.read(`https://minotar.net/avatar/${uuid}/8.png`);
            }

            const asciiLines = [];
            for (let y = 0; y < 8; y++) {
                let line = "";
                for (let x = 0; x < 8; x++) {
                    const pixel = Jimp.intToRGBA(image.getPixelColor(x, y));
                    line += (pixel.a > 128) ? findClosestMinecraftColor(pixel.r, pixel.g, pixel.b) + '█' : " ";
                }
                asciiLines.push(line);
            }

            const p = stats.player;
            const d = p.stats[gameInfo.apiName] || {};
            const a = p.achievements || {};

            const rank = formatter.formatRank(stats.rank);
            const guild = stats.guild ? ` §e[${stats.guild}]` : "";
            this.proxy.proxyChat("§7§m----------------------------------------");
            asciiLines.forEach(line => this.proxy.client.write("chat", { message: JSON.stringify({ text: line }), position: 1 }));
            this.proxy.proxyChat(" ");
            
            let lines = [];
            switch (gameInfo.apiName) {
                case "Bedwars":
                    lines.push(`${rank} ${username} §7[§f${a.bedwars_level || 0}✫§7]${guild}`);
                    lines.push(`§fWins: §a${(d.wins_bedwars || 0).toLocaleString()} §8| §fLosses: §c${(d.losses_bedwars || 1).toLocaleString()}`);
                    lines.push(`§fFinal Kills: §a${(d.final_kills_bedwars || 0).toLocaleString()} §8| §fFinal Deaths: §c${(d.final_deaths_bedwars || 1).toLocaleString()}`);
                    lines.push(`§fFKDR: §6${((d.final_kills_bedwars || 0) / (d.final_deaths_bedwars || 1)).toFixed(2)} §8| §fWLR: §6${((d.wins_bedwars || 0) / (d.losses_bedwars || 1)).toFixed(2)}`);
                    break;
                case "SkyWars":
                    lines.push(`${rank} ${username} §7[§f${p.stats.SkyWars.levelFormatted || '0✫'}§7]${guild}`);
                    lines.push(`§fWins: §a${(d.wins || 0).toLocaleString()} §8| §fLosses: §c${(d.losses || 1).toLocaleString()}`);
                    lines.push(`§fKills: §a${(d.kills || 0).toLocaleString()} §8| §fDeaths: §c${(d.deaths || 1).toLocaleString()}`);
                    lines.push(`§fKDR: §6${((d.kills || 0) / (d.deaths || 1)).toFixed(2)} §8| §fWLR: §6${((d.wins || 0) / (d.losses || 1)).toFixed(2)}`);
                    break;
                case "Duels":
                    const prefix = gameInfo.prefix || '';
                    const winsKey = prefix ? `${prefix}_wins` : 'wins';
                    const lossesKey = prefix ? `${prefix}_losses` : 'losses';
                    const killsKey = prefix ? `${prefix}_kills` : 'kills';
                    const deathsKey = prefix ? `${prefix}_deaths` : 'deaths';
                    const wins = d[winsKey] || 0;
                    lines.push(`§f[${gameInfo.displayName}] ${rank} ${username} §7[§f${wins.toLocaleString()} Wins§7]${guild}`);
                    lines.push(`§fWins: §a${wins.toLocaleString()} §8| §fLosses: §c${(d[lossesKey] || 1).toLocaleString()}`);
                    lines.push(`§fKills: §a${(d[killsKey] || 0).toLocaleString()} §8| §fDeaths: §c${(d[deathsKey] || 1).toLocaleString()}`);
                    lines.push(`§fWLR: §6${(wins / (d[lossesKey] || 1)).toFixed(2)} §8| §fKDR: §6${((d[killsKey] || 0) / (d[deathsKey] || 1)).toFixed(2)}`);
                    break;
                case "Walls3":
                    lines.push(`${rank} ${username}${guild}`);
                    lines.push(`§fWins: §a${(d.wins || 0).toLocaleString()} §8| §fLosses: §c${(d.losses || 1).toLocaleString()}`);
                    lines.push(`§fFinal Kills: §a${(d.final_kills || 0).toLocaleString()} §8| §fFinal Deaths: §c${(d.final_deaths || 1).toLocaleString()}`);
                    lines.push(`§fFKDR: §6${((d.final_kills || 0) / (d.final_deaths || 1)).toFixed(2)} §8| §fWLR: §6${((d.wins || 0) / (d.losses || 1)).toFixed(2)}`);
                    break;
                case "Quake":
                    lines.push(`${rank} ${username}${guild}`);
                    lines.push(`§fWins: §a${(d.wins || 0).toLocaleString()} §8| §fHeadshots: §e${(d.headshots || 0).toLocaleString()}`);
                    lines.push(`§fKills: §a${(d.kills || 0).toLocaleString()} §8| §fDeaths: §c${(d.deaths || 1).toLocaleString()}`);
                    lines.push(`§fKDR: §6${((d.kills || 0) / (d.deaths || 1)).toFixed(2)}`);
                    break;
                case "HungerGames":
                    lines.push(`${rank} ${username}${guild}`);
                    lines.push(`§fWins: §a${(d.wins || 0).toLocaleString()}`);
                    lines.push(`§fKills: §a${(d.kills || 0).toLocaleString()} §8| §fDeaths: §c${(d.deaths || 1).toLocaleString()}`);
                    lines.push(`§fKDR: §6${((d.kills || 0) / (d.deaths || 1)).toFixed(2)}`);
                    break;
                case "UHC":
                    lines.push(`${rank} ${username} §7[§f${(a.uhc_champion || 0)}✫§7]${guild}`);
                    lines.push(`§fWins: §a${(d.wins || 0).toLocaleString()} §8| §fScore: §6${(d.score || 0).toLocaleString()}`);
                    lines.push(`§fKills: §a${(d.kills || 0).toLocaleString()} §8| §fDeaths: §c${(d.deaths || 1).toLocaleString()}`);
                    lines.push(`§fKDR: §6${((d.kills || 0) / (d.deaths || 1)).toFixed(2)}`);
                    break;
                case "MurderMystery":
                    lines.push(`${rank} ${username} §7[§f${(d.wins || 0).toLocaleString()} Wins§7]${guild}`);
                    lines.push(`§fGames: §a${(d.games || 0).toLocaleString()}`);
                    lines.push(`§fKills: §a${(d.kills || 0).toLocaleString()} §8| §fDeaths: §c${(d.deaths || 1).toLocaleString()}`);
                    lines.push(`§fWin Rate: §6${(((d.wins || 0) / (d.games || 1)) * 100).toFixed(2)}%`);
                    break;
                case "BuildBattle":
                    lines.push(`${rank} ${username} §7[§fScore: ${(d.score || 0).toLocaleString()}§7]${guild}`);
                    lines.push(`§fWins: §a${(d.wins || 0).toLocaleString()} §8| §fGames Played: §e${(d.games_played || 0).toLocaleString()}`);
                    lines.push(`§fWin Rate: §6${(((d.wins || 0) / (d.games_played || 1)) * 100).toFixed(2)}%`);
                    break;
                 case "WoolGames":
                    lines.push(`${rank} ${username}${guild}`);
                    const ww = d.wool_wars || {};
                    const stats = ww.stats || {};
                    lines.push(`§fWins: §a${(stats.wins || 0).toLocaleString()} §8| §fGames: §e${(stats.games_played || 0).toLocaleString()}`);
                    lines.push(`§fKills: §a${(stats.kills || 0).toLocaleString()} §8| §fAssists: §b${(stats.assists || 0).toLocaleString()}`);
                    lines.push(`§fWLR: §6${((stats.wins || 0) / ((stats.games_played - (stats.wins || 0)) || 1)).toFixed(2)}`);
                    break;
                case "Pit":
                    const pitProfile = p.stats.Pit ? p.stats.Pit.profile : {};
                    const pitStats = p.stats.Pit ? p.stats.Pit.pit_stats_ptl : {};
                    const prestige = pitProfile.prestiges ? pitProfile.prestiges.length : 0;
                    lines.push(`${rank} ${username} §7[§e${prestige}✫§7]${guild}`);
                    lines.push(`§fKills: §a${(pitStats.kills || 0).toLocaleString()} §8| §fDeaths: §c${(pitStats.deaths || 1).toLocaleString()}`);
                    lines.push(`§fKDR: §6${((pitStats.kills || 0) / (pitStats.deaths || 1)).toFixed(2)}`);
                    break;
                default:
                    lines.push(`${rank} ${username}${guild}`);
                    lines.push(`§fWins: §a${(d.wins || 'N/A').toLocaleString()}`);
                    lines.push(`§fKills: §a${(d.kills || 'N/A').toLocaleString()} §8| §fDeaths: §c${(d.deaths || 'N/A').toLocaleString()}`);
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