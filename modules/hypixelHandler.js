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
        this.uuidCache = new Map();
    }

    reset() {
        this.avatarCache.clear();
        this.uuidCache.clear();
        formatter.log('HypixelHandler avatar cache reset.');
    }

    cleanRankPrefix(username) {
        return username.replace(/\[[A-Z+]+\]\s?|§./g, '').trim();
    }

    async getApiKey() {
        if (this.proxy.env.apiKey) return this.proxy.env.apiKey;
        if (this.apiKeyCache && this.apiKeyCacheTime && (Date.now() - this.apiKeyCacheTime < 60000)) return this.apiKeyCache;
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
                }
            } catch (e) {
                this.proxy.proxyChat(`§cError fetching API Key: ${e.message}`);
            }
        }
        return null;
    }

    resolveNickname(name) {
        const nicknames = this.proxy.config.nicknames || {};
        const lowerName = name.toLowerCase();
        for (const realName in nicknames) {
            if (nicknames[realName].toLowerCase() === lowerName) return realName;
        }
        return Object.keys(nicknames).find(key => key.toLowerCase() === lowerName) || name;
    }

    async getMojangUUID(username) {
        if (!username) return null;
        const lowerName = username.toLowerCase();
        if (this.uuidCache.has(lowerName)) return { uuid: this.uuidCache.get(lowerName), username };
        try {
            const response = await fetch(`https://api.mojang.com/users/profiles/minecraft/${username}`);
            if (!response.ok) return null;
            const data = await response.json();
            this.uuidCache.set(lowerName, data.id);
            return { uuid: data.id, username: data.name };
        } catch (err) {
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
            if (player.rank && player.rank !== 'NORMAL') rank = player.rank;
            else if (player.monthlyPackageRank === 'SUPERSTAR') rank = 'MVP_PLUS_PLUS';
            else if (player.newPackageRank) rank = player.newPackageRank;
            
            const guild = await this.getGuild(uuid);

            return { player, rank, guild, properties: player.properties || [] };
        } catch (err) {
            return null;
        }
    }

    async getTabDataForPlayer(name, gamemodeKey) {
        try {
            const mojangData = await this.getMojangUUID(name);
            if (!mojangData) return null;
            const stats = await this.getStats(mojangData.uuid);
            if (!stats) return null;
            const { gameModeMap } = require('../utils/constants');
            const gameInfo = gameModeMap[gamemodeKey];
            if (!gameInfo) return null;
            const d = stats.player?.stats?.[gameInfo.apiName];
            if (!d) return null;

            if (gameInfo.apiName === 'Bedwars') {
                const fk = d.final_kills_bedwars || 0;
                const fd = d.final_deaths_bedwars || 1;
                const fkdrValue = (fk / fd).toFixed(2);
                const fkdrColor = this.getFkdrColor(parseFloat(fkdrValue));
                const level = stats.player?.achievements?.bedwars_level ?? 0;

                let lvlColor;
                if      (level >= 500) lvlColor = '\u00A75';
                else if (level >= 350) lvlColor = '\u00A74';
                else if (level >= 200) lvlColor = '\u00A7c';
                else if (level >= 100) lvlColor = '\u00A7e';
                else if (level >= 50)  lvlColor = '\u00A7f';
                else                   lvlColor = '\u00A77';

                const prefix = lvlColor + '[' + level + '\u2605] ';
                const suffix = ' \u00A78|' + fkdrColor + fkdrValue;

                return {
                    suffix: suffix.substring(0, 16),
                    prefix: prefix.substring(0, 16),
                    level,
                    fkdr: fkdrValue,
                    fkdrColor
                };
            }
            return null;
        } catch (e) {
            return null;
        }
    }

    getLevelColorStatic(lvl) {
        if (lvl >= 1000) return '§c';
        if (lvl >= 900)  return '§5';
        if (lvl >= 800)  return '§d';
        if (lvl >= 700)  return '§9';
        if (lvl >= 600)  return '§b';
        if (lvl >= 500)  return '§3';
        if (lvl >= 400)  return '§2';
        if (lvl >= 300)  return '§e';
        if (lvl >= 200)  return '§f';
        if (lvl >= 100)  return '§6';
        return '§7';
    }

    getFkdrColor(fkdr) {
        if (fkdr >= 20) return '\u00A75';
        if (fkdr >= 10) return '\u00A74';
        if (fkdr >= 5)  return '\u00A7c';
        if (fkdr >= 3)  return '\u00A7e';
        if (fkdr >= 1)  return '\u00A7f';
        return '\u00A77';
    }

    async processQueueAndPrintBulk(playerNames, gamemodeKey) {
        const { gameModeMap } = require('../utils/constants');
        const gameInfo = gameModeMap[gamemodeKey] || gameModeMap['bedwars'];
        const cleanNames = playerNames.map(n => n.replace(/§./g, '').replace(/\[.*?\]/g, '').trim()).filter(n => n);
        this.proxy.proxyChat(`§dAuto-checking §f${cleanNames.length} §dplayers...`);
        this.proxy.tabManager.updatePlayerTags(cleanNames, gamemodeKey);
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

    async autoStatCheckDuels(username, gamemode) {
        formatter.log(`Auto-checking Duels stats for ${username}...`);
        return this.statcheck(gamemode, username);
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
                this.avatarCache.set(uuid, asciiLines);
            }

            const p = stats.player;
            const d = p.stats[gameInfo.apiName] || {};

            const rank = formatter.formatRank(p);
            const guild = stats.guild ? ` §5${stats.guild}` : "";
            const prefix = "§8[§5jag§dprox§8] §r";

            asciiLines.forEach(line => {
                this.proxy.client.write("chat", { 
                    message: JSON.stringify({ text: prefix + line }), 
                    position: 1 
                });
            });

            if (gameInfo.apiName === "Bedwars") {
                const wins = d.wins_bedwars || 0;
                const losses = d.losses_bedwars || 0;
                const kills = d.kills_bedwars || 0;
                const deaths = d.deaths_bedwars || 0;
                const finals = d.final_kills_bedwars || 0;
                const fdeaths = d.final_deaths_bedwars || 0;
                const bedsBroken = d.beds_broken_bedwars || 0;
                const bedsLost = d.beds_lost_bedwars || 0;
                const winstreak = d.winstreak || 0;
                let bestWinstreak = d.winstreak_best || 0;
                if (winstreak > bestWinstreak) bestWinstreak = winstreak;

                const wlr = (wins / (losses || 1)).toFixed(2);
                const kdr = (kills / (deaths || 1)).toFixed(2);
                const fkdr = (finals / (fdeaths || 1)).toFixed(2);
                const bblr = (bedsBroken / (bedsLost || 1)).toFixed(2);

                this.proxy.proxyChat(`§8[§dBedwars§8]`);
                const nameColor = formatter.getPlayerNameColor(p);
                this.proxy.proxyChat(`${rank} ${nameColor}${username}${guild}`);
                this.proxy.proxyChat(`§dWins §5» §f${wins.toLocaleString()}    §8| §dLosses §5» §f${losses.toLocaleString()}`);
                this.proxy.proxyChat(`§dKills §5» §f${kills.toLocaleString()}   §8| §dDeaths §5» §f${deaths.toLocaleString()}`);
                this.proxy.proxyChat(`§dFinals §5» §f${finals.toLocaleString()}   §8| §dF-Deaths §5» §f${fdeaths.toLocaleString()}`);
                this.proxy.proxyChat(`§dWLR   §5» §f${wlr} | §dKDR §5» §f${kdr}`);
                this.proxy.proxyChat(`§dFKDR  §5» §f${fkdr} | §dBBLR §5» §f${bblr}`);
                if (parseFloat(wlr) > 1.01 && bestWinstreak === 0) {
                    this.proxy.proxyChat(`§cWINSTREAK API DISABLED`);
                } else {
                    this.proxy.proxyChat(`§dWinstreak §5» §f${winstreak.toLocaleString()} | §dBest §5» §f${bestWinstreak.toLocaleString()}`);
                }
            } else {
                this.proxy.proxyChat(`§8[§5${gameInfo.displayName}§8]`);

                const nameColor = formatter.getPlayerNameColor(p);
                this.proxy.proxyChat(`${rank} ${nameColor}${username}${guild}`);

                let wins = 0, losses = 1, kills = 0, deaths = 1;
                let currentWinstreak = 0, bestWinstreak = 0;

                switch (gameInfo.apiName) {
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
                if (currentWinstreak > bestWinstreak) bestWinstreak = currentWinstreak;

                this.proxy.proxyChat(`§dWins §5» §f${wins.toLocaleString()} §8| §5Losses §5» §f${losses.toLocaleString()}`);
                this.proxy.proxyChat(`§dKills §5» §f${kills.toLocaleString()} §8| §5Deaths §5» §f${deaths.toLocaleString()}`);
                this.proxy.proxyChat(`§dWLR §5» §f${wlr} §8| §dKDR §5» §f${kdr}`);
                
                if (bestWinstreak === 0 && parseFloat(wlr) > 1.01) {
                    this.proxy.proxyChat(`§cWINSTREAK API DISABLED`);
                } else {
                    this.proxy.proxyChat(`§dWinstreak §5» §f${currentWinstreak.toLocaleString()} §8| §5Best §5» §f${bestWinstreak.toLocaleString()}`);
                }
            }

        } catch (err) {
            formatter.log(`displayFormattedStats Error: ${err.message}`);
            this.proxy.proxyChat(`§cError displaying stats.`);
        }
    }
}

module.exports = HypixelHandler;
