const fetch = require('node-fetch');
const Jimp = require('jimp');
const formatter = require("../formatter.js");
const { findClosestMinecraftColor, gameModeMap } = require("../utils/constants");

class HypixelHandler {
    constructor(proxy) {
        this.proxy = proxy;
        this.apiKeyCache = null;
        this.apiKeyCacheTime = null;
        if (this.proxy.env.jwt) {
            const ApiHandler = require('../utils/apiHandler.js');
            this.apiHandler = new ApiHandler({ jwt: this.proxy.env.jwt });
        } else {
            this.apiHandler = null;
        }
        this.avatarCache = new Map();
    }

    reset() {
        this.avatarCache.clear();
        formatter.log('HypixelHandler avatar cache reset.');
    }

    cleanRankPrefix(username) {



        return username.replace(/\[[A-Z+]+\]\s?|§./g, '').trim();
    }

    async getApiKey() {
        if (this.proxy.env.apiKey) {
            return this.proxy.env.apiKey;
        }

        if (this.apiKeyCache && this.apiKeyCacheTime && (Date.now() - this.apiKeyCacheTime < 60000)) {
            return this.apiKeyCache;
        }

        if (this.proxy.env.jwt) {
            if (!this.apiHandler) {
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
                    formatter.log('Could not retrieve Hypixel API key from backend. It might not be set.');
                    return null;
                }
            } catch (e) {
                this.proxy.proxyChat(`§cError fetching API Key: ${e.message}`);
                return null;
            }
        }
        
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
            } catch(e) { 
                this.proxy.target.removeListener('packet', partyListener); // Ensure listener is removed on parse error
                return; 
            }

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

        this.proxy.proxyChat(`§dFound §f${partyMembers.length} §dmembers. Fetching stats for §5${gameInfo.displayName}§d...`);

        const statBlocks = [];
        for (const username of partyMembers) {
            const block = await this.getAndFormatPartyPlayerStats(username.replace(/\[.*?\]\s/g, ''), gameInfo);
            if (block) {
                statBlocks.push(block);
            }
            await new Promise(resolve => setTimeout(resolve, 300)); // Add a delay between requests
        }

        let finalMessage = `§5§m----------------------------------------------------\n`;
        finalMessage += `  §5§lParty Stats for §d${gameInfo.displayName}\n \n`;
        finalMessage += statBlocks.filter(block => block).join('\n \n');
        finalMessage += `\n§5§m----------------------------------------------------`;
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
            if (!stats) {
                return `  §7${mojangData.username} §7- No stats found.`;
            }

            const rank = formatter.formatRank(stats.rank);

            if (!stats.player.stats || !stats.player.stats[gameInfo.apiName]) {
                return `  ${rank} ${mojangData.username} §7- No stats found.`;
            }
            
            const p = stats.player;
            const d = p.stats[gameInfo.apiName] || {};
            const a = p.achievements || {};

            let statLines = [];
            let header = `  ${rank} ${mojangData.username}`;

            switch (gameInfo.apiName) {
                case "Bedwars":
                    header += ` §8[§d${a.bedwars_level || 0}✫§8]`;
                    statLines.push(`    §dFKDR §8» §f${((d.final_kills_bedwars || 0) / (d.final_deaths_bedwars || 1)).toFixed(2)} §8| §dWLR §8» §f${((d.wins_bedwars || 0) / (d.losses_bedwars || 1)).toFixed(2)}`);
                    break;
                case "SkyWars":
                    header += ` §8[§d${p.stats.SkyWars.levelFormatted || '0✫'}§8]`;
                    statLines.push(`    §dKDR §8» §f${((d.kills || 0) / (d.deaths || 1)).toFixed(2)} §8| §dWLR §8» §f${((d.wins || 0) / (d.losses || 1)).toFixed(2)}`);
                    break;
                case "Walls3":
                    statLines.push(`    §dWins §8» §f${(d.wins || 0).toLocaleString()} §8| §dFKDR §8» §f${((d.final_kills || 0) / (d.final_deaths || 1)).toFixed(2)}`);
                    break;
                case "Duels":
                     const wins = d.wins || 0; const losses = d.losses || 1;
                     const kills = d.kills || 0; const deaths = d.deaths || 1;
                     statLines.push(`    §dWLR §8» §f${(wins/losses).toFixed(2)} §8| §dKDR §8» §f${(kills/deaths).toFixed(2)}`);
                     break;
                case "UHC":
                    header += ` §8[§d${(a.uhc_champion || 0)}✫§8]`;
                    statLines.push(`    §dWins §8» §f${(d.wins || 0).toLocaleString()} §8| §dKDR §8» §f${((d.kills || 0) / (d.deaths || 1)).toFixed(2)}`);
                    break;
                default:
                    statLines.push(`    §dWins §8» §f${(d.wins || 'N/A').toLocaleString()} §8| §dKills §8» §f${(d.kills || 'N/A').toLocaleString()}`);
            }
            
            return `${header}\n${statLines.join('\n')}`;

        } catch (err) {
            formatter.log(`Party stat check error for ${username}: ${err.message}`);
            return `  §cError fetching stats for ${username}.`;
        }
    }

    async autoStatCheckDuels(username, gamemode) {


        formatter.log(`Auto-checking Duels stats for ${username}...`);
        return this.statcheck(gamemode, username);
    }

    async getLeaderboard(game, type) {
        const apiKey = await this.getApiKey();
        if (!apiKey) return { error: "API Key not configured." };

        try {
            const response = await fetch(`https://api.hypixel.net/v2/leaderboards?key=${apiKey}`);
            if (!response.ok) {
                return { error: `Failed to fetch leaderboards: ${response.statusText}` };
            }
            const data = await response.json();

            if (!data.success) {
                return { error: `Hypixel API Error: ${data.cause || 'Unknown error'}` };
            }

            const gameLeaderboards = data.leaderboards[game.toUpperCase()];
            if (!gameLeaderboards) {
                return { error: `No leaderboards found for game: ${game}` };
            }

            const targetLeaderboard = gameLeaderboards.find(lb => 
                lb.prefix && lb.prefix.toLowerCase() === type.split(' ')[0].toLowerCase() && 
                lb.title && lb.title.toLowerCase() === type.split(' ')[1].toLowerCase()
            );

            if (!targetLeaderboard) {
                return { error: `No '${type}' leaderboard found for ${game}.` };
            }

            const leaders = [];
            for (const uuid of targetLeaderboard.leaders) {
                const username = await this.getUsernameFromUUID(uuid);
                if (username) {
                    leaders.push(username);
                } else {
                    leaders.push(uuid); // Fallback to UUID if username not found
                }
            }
            return { success: true, title: `${targetLeaderboard.prefix} ${targetLeaderboard.title} for ${game}`, leaders: leaders };

        } catch (err) {
            formatter.log(`getLeaderboard Error: ${err.message}`);
            return { error: 'An internal error occurred while fetching leaderboards.' };
        }
    }

    async getUsernameFromUUID(uuid) {
        const apiKey = await this.getApiKey();
        if (!apiKey) return null;
        try {
            const response = await fetch(`https://api.hypixel.net/v2/player?key=${apiKey}&uuid=${uuid}`);
            if (!response.ok) return null;
            const data = await response.json();
            if (!data.success || !data.player) return null;
            return data.player.displayname;
        } catch (err) {
            formatter.log(`getUsernameFromUUID Error: ${err.message}`);
            return null;
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
            if (!data.guild) return null;
            return data.guild.tag ? `[${data.guild.tag}]` : `[${data.guild.name}]`;
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
            this.proxy.proxyChat("§5§m----------------------------------------");
            if (status.online) {
                this.proxy.proxyChat(`${formatter.formatRank(status.rank)} ${mojangData.username} §dOnline`);
                if (status.hidden) {
                    this.proxy.proxyChat(`§8(Status is hidden, game info unavailable)`);
                } else {
                    this.proxy.proxyChat(`§dGame §8» §f${status.gameType}`);
                    if (status.mode) this.proxy.proxyChat(`§dMode §8» §f${status.mode}`);
                    if (status.map) this.proxy.proxyChat(`§dMap §8» §f${status.map}`);
                }
            } else {
                this.proxy.proxyChat(`${formatter.formatRank(status.rank)} ${mojangData.username} §8Offline`);
            }
            this.proxy.proxyChat("§5§m----------------------------------------");
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

        try {
            const cleanUsername = this.cleanRankPrefix(username);
            const resolvedName = this.resolveNickname(cleanUsername);
            const mojangData = await this.getMojangUUID(resolvedName);
            if (!mojangData) return this.proxy.proxyChat(`§cPlayer '${resolvedName}' not found.`);
            const stats = await this.getStats(mojangData.uuid);
            if (!stats) {
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

    async getPlayerCounts() {
        const apiKey = await this.getApiKey();
        if (!apiKey) return null;
        try {
            const response = await fetch(`https://api.hypixel.net/v2/counts?key=${apiKey}`);
            if (!response.ok) return null;
            const data = await response.json();
            if (!data.success) return null;
            return data.games;
        } catch (err) {
            formatter.log(`getPlayerCounts Error: ${err.message}`);
            return null;
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

            let rank = "NONE";
            if (player.rank && player.rank !== 'NORMAL') {
                rank = player.rank;
            } else if (player.monthlyPackageRank === 'SUPERSTAR') {
                rank = 'MVP_PLUS_PLUS';
            } else if (player.newPackageRank) {
                rank = player.newPackageRank;
            }

            return {
                player: player,
                rank: rank, // Now unformatted
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
            let asciiLines;
            if (this.avatarCache.has(uuid)) {
                asciiLines = this.avatarCache.get(uuid);
            } else {
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

                asciiLines = [];
                for (let y = 0; y < 8; y++) {
                    let line = "";
                    for (let x = 0; x < 8; x++) {
                        const pixel = Jimp.intToRGBA(image.getPixelColor(x, y));
                        line += (pixel.a > 128) ? findClosestMinecraftColor(pixel.r, pixel.g, pixel.b) + '█' : " ";
                    }
                    asciiLines.push(line);
                }
                this.avatarCache.set(uuid, asciiLines); // Cache the generated ASCII lines
            }

            const p = stats.player;
            const d = p.stats[gameInfo.apiName] || {};

            const rank = formatter.formatRank(p);
            const guild = stats.guild ? ` §5${stats.guild}` : "";
            const prefix = "§8[§5jag§dprox§8] §r";

            // 1. Skin lines with prefix
            asciiLines.forEach(line => {
                this.proxy.client.write("chat", { 
                    message: JSON.stringify({ text: prefix + line }), 
                    position: 1 
                });
            });

            // 2. [MODE]
            this.proxy.proxyChat(`§8[§5${gameInfo.displayName}§8]`);

            // 3. [Rank] Name [Guild]
            const nameColor = formatter.getPlayerNameColor(p);
            this.proxy.proxyChat(`${rank} ${nameColor}${username}${guild}`);

            let wins = 0, losses = 1, kills = 0, deaths = 1;
            let currentWinstreak = 0, bestWinstreak = 0;

            switch (gameInfo.apiName) {
                case "Bedwars":
                    wins = d.wins_bedwars || 0;
                    losses = d.losses_bedwars || 1;
                    kills = d.final_kills_bedwars || 0;
                    deaths = d.final_deaths_bedwars || 1;
                    currentWinstreak = d.winstreak || 0;
                    bestWinstreak = d.winstreak_best || 0;
                    break;
                case "SkyWars":
                    wins = d.wins || 0;
                    losses = d.losses || 1;
                    kills = d.kills || 0;
                    deaths = d.deaths || 1;
                    currentWinstreak = d.winstreak || 0;
                    bestWinstreak = d.winstreak_best || 0;
                    break;
                case "Duels":
                    const dgPrefix = gameInfo.prefix || '';
                    wins = d[dgPrefix ? `${dgPrefix}_wins` : 'wins'] || 0;
                    losses = d[dgPrefix ? `${dgPrefix}_losses` : 'losses'] || 1;
                    kills = d[dgPrefix ? `${dgPrefix}_kills` : 'kills'] || 0;
                    deaths = d[dgPrefix ? `${dgPrefix}_deaths` : 'deaths'] || 1;
                    if (dgPrefix) {
                        currentWinstreak = d[`current_winstreak_mode_${dgPrefix}`] || 0;
                        bestWinstreak = d[`best_winstreak_mode_${dgPrefix}`] || 0;
                    } else {
                        currentWinstreak = d.current_winstreak || 0;
                        bestWinstreak = d.best_winstreak || 0;
                    }
                    break;
                case "Walls3":
                    wins = d.wins || 0;
                    losses = d.losses || 1;
                    kills = d.final_kills || 0;
                    deaths = d.final_deaths || 1;
                    break;
                default:
                    wins = d.wins || 0;
                    losses = d.losses || 1;
                    kills = d.kills || 0;
                    deaths = d.deaths || 1;
            }

            const wlr = (wins / losses).toFixed(2);
            const kdr = (kills / deaths).toFixed(2);

            this.proxy.proxyChat(`§dWins §5» §f${wins.toLocaleString()} §8| §5Losses §5» §f${losses.toLocaleString()}`);
            this.proxy.proxyChat(`§dKills §5» §f${kills.toLocaleString()} §8| §5Deaths §5» §f${deaths.toLocaleString()}`);
            this.proxy.proxyChat(`§dWLR §5» §f${wlr} §8| §dKDR §5» §f${kdr}`);
            
            if (bestWinstreak === 0 && parseFloat(wlr) > 1.01) {
                this.proxy.proxyChat(`§cWINSTREAK DISABLED`);
            } else {
                this.proxy.proxyChat(`§dWinstreak §5» §f${currentWinstreak.toLocaleString()} §8| §5Best §5» §f${bestWinstreak.toLocaleString()}`);
            }

        } catch (err) {
            formatter.log(`displayFormattedStats Error: ${err.message}`);
            this.proxy.proxyChat(`§cError displaying stats.`);
        }
    }
}

module.exports = HypixelHandler;
