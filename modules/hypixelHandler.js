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
            return { player, rank, properties: player.properties || [] };
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
}

module.exports = HypixelHandler;
