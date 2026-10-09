const { PNG } = require('pngjs');
const formatter = require("../formatter.js");
const ApiHandler = require('../utils/apiHandler.js');
const { TTLCache, mapLimit } = require('../utils/cache.js');
const { findClosestMinecraftColor, gameModeMap } = require("../utils/constants.js");
const { parsePartyLine } = require('../utils/chatParsers.js');
const statFormat = require('../utils/statFormat.js');

const HYPIXEL_API = 'https://api.hypixel.net/v2';
const API_KEY_TTL_MS = 60 * 1000;
const NOTICE_COOLDOWN_MS = 30 * 1000;
const LOOKUP_CONCURRENCY = 4;
const DIVIDER = '§5§m----------------------------------------------------';

async function fetchBuffer(url) {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`HTTP ${response.status} for ${url}`);
    return Buffer.from(await response.arrayBuffer());
}

function pixelAt(png, x, y) {
    const idx = (png.width * y + x) << 2;
    return { r: png.data[idx], g: png.data[idx + 1], b: png.data[idx + 2], a: png.data[idx + 3] };
}

class HypixelHandler {
    constructor(proxy) {
        this.proxy = proxy;
        this.apiHandler = this.proxy.env.jwt ? new ApiHandler({ jwt: this.proxy.env.jwt }) : null;

        this.apiKeyCache = null;
        this.apiKeyCacheTime = 0;
        this.apiKeyRequest = null;
        this.lastNotice = new Map();

        // Caches survive game switches; entries expire on their own.
        this.uuidCache = new TTLCache({ ttl: 60 * 60 * 1000, maxSize: 2000 });
        this.profileCache = new TTLCache({ ttl: 60 * 60 * 1000, maxSize: 2000 });
        this.statsCache = new TTLCache({ ttl: 5 * 60 * 1000, maxSize: 500 });
        this.guildCache = new TTLCache({ ttl: 10 * 60 * 1000, maxSize: 500 });
        this.avatarCache = new TTLCache({ ttl: 60 * 60 * 1000, maxSize: 200 });
    }

    reset() {
        // Nothing session-specific to drop: caches are keyed by player and expire by TTL.
    }

    // Sends a chat notice at most once per cooldown window per key (avoids spam during bulk lookups).
    notifyOnce(key, message) {
        const now = Date.now();
        if (now - (this.lastNotice.get(key) || 0) < NOTICE_COOLDOWN_MS) return;
        this.lastNotice.set(key, now);
        this.proxy.proxyChat(message);
    }

    cleanRankPrefix(username) {
        return username.replace(/\[[A-Z+]+\]\s?|§./g, '').trim();
    }

    async getApiKey() {
        if (this.proxy.env.apiKey) {
            return this.proxy.env.apiKey;
        }
        if (this.apiKeyCache && Date.now() - this.apiKeyCacheTime < API_KEY_TTL_MS) {
            return this.apiKeyCache;
        }
        if (!this.apiHandler) {
            formatter.log('API Key is not configured in any context.');
            return null;
        }
        // Share one backend request between all concurrent callers.
        if (!this.apiKeyRequest) {
            this.apiKeyRequest = this.apiHandler.getApiKey()
                .then((apiKey) => {
                    if (apiKey) {
                        this.apiKeyCache = apiKey;
                        this.apiKeyCacheTime = Date.now();
                        return apiKey;
                    }
                    this.notifyOnce('api-key', '§cNo Hypixel API key is set for your account. Add one in the launcher settings.');
                    return null;
                })
                .catch((e) => {
                    this.notifyOnce('api-key', `§cError fetching API Key: ${e.message}`);
                    return null;
                })
                .finally(() => {
                    this.apiKeyRequest = null;
                });
        }
        return this.apiKeyRequest;
    }

    // GET a Hypixel v2 endpoint. Returns the parsed body, or null on any failure.
    async hypixelGet(endpoint, params = {}) {
        const apiKey = await this.getApiKey();
        if (!apiKey) return null;
        const url = new URL(`${HYPIXEL_API}/${endpoint}`);
        for (const [key, value] of Object.entries(params)) url.searchParams.set(key, value);
        try {
            const response = await fetch(url, { headers: { 'API-Key': apiKey } });
            if (response.status === 429) {
                this.notifyOnce('hypixel-429', '§cHypixel API rate limit reached. Try again in a minute.');
                return null;
            }
            if (!response.ok) {
                formatter.log(`Hypixel /${endpoint} returned ${response.status}`);
                return null;
            }
            const data = await response.json();
            return data.success ? data : null;
        } catch (err) {
            formatter.log(`Hypixel /${endpoint} error: ${err.message}`);
            return null;
        }
    }

    resolveNickname(name) {
        const nicknames = (this.proxy.config && this.proxy.config.nicknames) || {};
        const lowerName = name.toLowerCase();
        for (const realName in nicknames) {
            if (String(nicknames[realName]).toLowerCase() === lowerName) {
                return realName;
            }
        }
        const realName = Object.keys(nicknames).find(key => key.toLowerCase() === lowerName);
        return realName || name;
    }

    async getMojangUUID(username) {
        if (!username) return null;
        return this.uuidCache.getOrLoad(username.toLowerCase(), async () => {
            try {
                const response = await fetch(`https://api.mojang.com/users/profiles/minecraft/${encodeURIComponent(username)}`);
                if (!response.ok) {
                    if (response.status === 429) this.notifyOnce('mojang-429', '§cMojang API rate limit reached.');
                    return null;
                }
                const data = await response.json();
                return { uuid: data.id, username: data.name };
            } catch (err) {
                formatter.log(`Mojang API Error: ${err.message}`);
                return null;
            }
        });
    }

    // Mojang session profile: current name + skin textures. Cached for an hour.
    async getSessionProfile(uuid) {
        return this.profileCache.getOrLoad(uuid, async () => {
            try {
                const response = await fetch(`https://sessionserver.mojang.com/session/minecraft/profile/${encodeURIComponent(uuid)}`);
                if (!response.ok) return null;
                return await response.json();
            } catch (err) {
                formatter.log(`Session profile error for ${uuid}: ${err.message}`);
                return null;
            }
        });
    }

    async getUsernameFromUUID(uuid) {
        const profile = await this.getSessionProfile(uuid);
        return profile ? profile.name : null;
    }

    async getSkinUrl(uuid) {
        const profile = await this.getSessionProfile(uuid);
        const texturesProp = profile && (profile.properties || []).find(p => p.name === 'textures');
        if (!texturesProp) return null;
        try {
            const textures = JSON.parse(Buffer.from(texturesProp.value, 'base64').toString('utf8'));
            return textures.textures?.SKIN?.url || null;
        } catch (e) {
            return null;
        }
    }

    // Player data, cached for 5 minutes. Concurrent lookups of the same player share one request.
    async getStats(uuid) {
        if (!uuid) return null;
        return this.statsCache.getOrLoad(uuid, async () => {
            const data = await this.hypixelGet('player', { uuid });
            if (!data || !data.player) return null;
            return { player: data.player, rank: formatter.getRank(data.player) };
        });
    }

    async getGuild(uuid) {
        return this.guildCache.getOrLoad(uuid, async () => {
            const data = await this.hypixelGet('guild', { player: uuid });
            if (!data) return null;
            // Cache "no guild" as an empty string so it isn't re-fetched.
            if (!data.guild) return '';
            return data.guild.tag ? `[${data.guild.tag}]` : `[${data.guild.name}]`;
        });
    }

    async getHypixelStatus(uuid) {
        const [statusData, stats] = await Promise.all([
            this.hypixelGet('status', { uuid }),
            this.getStats(uuid),
        ]);
        if (!stats) return null;
        const { player, rank } = stats;
        if (statusData && statusData.session && statusData.session.online) {
            const { gameType, mode, map } = statusData.session;
            return { online: true, hidden: false, gameType, mode, map, rank, player };
        }
        if ((player.lastLogin || 0) > (player.lastLogout || 0)) {
            return { online: true, hidden: true, rank, player };
        }
        return { online: false, rank, player };
    }

    // Lightweight status lookup (one request) for periodic polling.
    async getSessionStatus(uuid) {
        const data = await this.hypixelGet('status', { uuid });
        return data ? data.session : null;
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
        const gameInfo = gameModeMap[String(gamemode).toLowerCase()];
        if (!gameInfo) {
            return { error: `Unknown game mode: ${gamemode}` };
        }
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) return { error: `Player '${username}' not found.` };

            const [stats, guild] = await Promise.all([this.getStats(mojangData.uuid), this.getGuild(mojangData.uuid)]);
            if (!stats) return { error: `Could not retrieve player data for '${mojangData.username}'.` };

            return { username: mojangData.username, uuid: mojangData.uuid, game: gameInfo, stats, guild };
        } catch (err) {
            formatter.log(`API Statcheck error: ${err.message}`);
            return { error: 'An internal error occurred.' };
        }
    }

    handlePartyStatCheck(gamemode) {
        const target = this.proxy.target;
        if (!target) return;
        this.proxy.proxyChat("§eRequesting party member list...");

        const partyMembers = new Set();
        let capturing = false;
        let notInParty = false;
        let finished = false;

        const finish = () => {
            if (finished) return;
            finished = true;
            clearTimeout(timeout);
            target.removeListener('packet', partyListener);
            if (partyMembers.size > 0) {
                this.processPartyMembers(Array.from(partyMembers), gamemode).catch((e) =>
                    formatter.log(`Party stat check failed: ${e.message}`));
            } else if (!notInParty) {
                this.proxy.proxyChat("§cCould not find any party members.");
            }
        };

        const partyListener = (data, meta) => {
            if (meta.name !== 'chat') return;
            // Non-JSON chat is just unrelated noise; keep listening.
            const cleanMessage = formatter.chatToCleanText(data.message);
            if (cleanMessage === null) return;

            if (cleanMessage.includes('You are not currently in a party.')) {
                notInParty = true;
                this.proxy.proxyChat("§cYou are not in a party.");
                finish();
                return;
            }

            if (cleanMessage.startsWith('Party Members (') || cleanMessage.startsWith('Party Leader:')) {
                capturing = true;
            }

            if (capturing) {
                const names = parsePartyLine(cleanMessage);
                if (names) names.forEach(name => partyMembers.add(name));
                if (cleanMessage.startsWith('-----') && partyMembers.size > 0) finish();
            }
        };

        // Always unsubscribe, even if Hypixel never answers.
        const timeout = setTimeout(finish, 5000);
        target.on('packet', partyListener);
        target.write('chat', { message: '/party list' });
    }

    async processPartyMembers(partyMembers, gamemode) {
        if (partyMembers.length === 0) {
            this.proxy.proxyChat("§cCould not find any party members.");
            return;
        }

        const gameInfo = gameModeMap[gamemode] || gameModeMap[this.proxy.queueStats?.currentGameKey] || gameModeMap.bedwars;
        this.proxy.proxyChat(`§dFound §f${partyMembers.length} §dmembers. Fetching stats for §5${gameInfo.displayName}§d...`);

        const statBlocks = (await mapLimit(partyMembers, LOOKUP_CONCURRENCY, (name) =>
            this.getAndFormatPartyPlayerStats(name, gameInfo))).filter(Boolean);

        let finalMessage = `${DIVIDER}\n`;
        finalMessage += `  §5§lParty Stats for §d${gameInfo.displayName}\n \n`;
        finalMessage += statBlocks.join('\n \n');
        finalMessage += `\n${DIVIDER}`;
        this.proxy.proxyChat(finalMessage);
    }

    async getAndFormatPartyPlayerStats(username, gameInfo) {
        try {
            const cleanUsername = username.replace(/§./g, '').replace(/\[.*?\]\s/g, '').trim();
            if (!cleanUsername) return null;

            const realUsername = this.resolveNickname(cleanUsername);
            const mojangData = await this.getMojangUUID(realUsername);
            if (!mojangData) return `  §c§o'${cleanUsername}' not found.`;

            const stats = await this.getStats(mojangData.uuid);
            if (!stats) {
                return `  §7${mojangData.username} §7- No stats found.`;
            }

            const rank = formatter.formatRank(stats.player);
            const nameColor = formatter.getPlayerNameColor(stats.player);
            const s = statFormat.getModeStats(stats.player, gameInfo);
            let header = `  ${rank} ${nameColor}${mojangData.username}`;
            if (!s.hasStats) {
                return `${header} §7- No stats found.`;
            }
            if (s.levelText) header += ` §8[§d${s.levelText}§8]`;

            return `${header}\n    ${statFormat.formatCompactLine(s, gameInfo.apiName)}`;
        } catch (err) {
            formatter.log(`Party stat check error for ${username}: ${err.message}`);
            return `  §cError fetching stats for ${username}.`;
        }
    }

    async autoStatCheckDuels(username, gamemode) {
        formatter.log(`Auto-checking Duels stats for ${username}...`);
        return this.statcheck(gamemode, username);
    }

    // `type` is "<Prefix> <Title>", e.g. "Monthly Wins".
    async getLeaderboard(game, type, limit = 10) {
        const [prefix, ...titleWords] = String(type).split(' ');
        const title = titleWords.join(' ');
        if (!prefix || !title) return { error: `Invalid leaderboard type '${type}'.` };

        const data = await this.hypixelGet('leaderboards');
        if (!data) return { error: 'Failed to fetch leaderboards from Hypixel.' };

        const gameLeaderboards = data.leaderboards && data.leaderboards[game.toUpperCase()];
        if (!gameLeaderboards) {
            return { error: `No leaderboards found for game: ${game}` };
        }

        const targetLeaderboard = gameLeaderboards.find(lb =>
            lb.prefix && lb.prefix.toLowerCase() === prefix.toLowerCase() &&
            lb.title && lb.title.toLowerCase() === title.toLowerCase()
        );
        if (!targetLeaderboard) {
            return { error: `No '${type}' leaderboard found for ${game}.` };
        }

        const topUuids = targetLeaderboard.leaders.slice(0, limit);
        const leaders = await mapLimit(topUuids, LOOKUP_CONCURRENCY, async (uuid) =>
            (await this.getUsernameFromUUID(uuid)) || uuid);
        return { success: true, title: `${targetLeaderboard.prefix} ${targetLeaderboard.title} for ${game}`, leaders };
    }

    async getPlayerStatus(username) {
        this.proxy.proxyChat(`§eChecking status for ${username}...`);
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) return this.proxy.proxyChat(`§cPlayer '${username}' not found.`);
            const status = await this.getHypixelStatus(mojangData.uuid);
            if (!status) return this.proxy.proxyChat(`§cCould not retrieve status for '${mojangData.username}'.`);

            const nameLine = `${formatter.formatRank(status.player)} ${formatter.getPlayerNameColor(status.player)}${mojangData.username}`;
            this.proxy.proxyChat("§5§m----------------------------------------");
            if (status.online) {
                this.proxy.proxyChat(`${nameLine} §dOnline`);
                if (status.hidden) {
                    this.proxy.proxyChat(`§8(Status is hidden, game info unavailable)`);
                } else {
                    this.proxy.proxyChat(`§dGame §8» §f${status.gameType}`);
                    if (status.mode) this.proxy.proxyChat(`§dMode §8» §f${status.mode}`);
                    if (status.map) this.proxy.proxyChat(`§dMap §8» §f${status.map}`);
                }
            } else {
                this.proxy.proxyChat(`${nameLine} §8Offline`);
            }
            this.proxy.proxyChat("§5§m----------------------------------------");
        } catch (err) {
            formatter.log(`Status check error: ${err.message}`);
            this.proxy.proxyChat(`§cAn error occurred.`);
        }
    }

    // Prints a full stat breakdown. Returns { mojangData, stats, gameInfo } on success so
    // callers can reuse the fetched data, or null.
    async statcheck(gamemode, username) {
        if (!gamemode || !username) {
            this.proxy.proxyChat("§cUsage: /sc <gamemode> <username>");
            return null;
        }

        const gameInfo = gameModeMap[gamemode.toLowerCase()];
        if (!gameInfo) {
            this.proxy.proxyChat(`§cUnknown game mode: ${gamemode}`);
            return null;
        }

        try {
            const cleanUsername = this.cleanRankPrefix(username);
            const resolvedName = this.resolveNickname(cleanUsername);
            const mojangData = await this.getMojangUUID(resolvedName);
            if (!mojangData) {
                this.proxy.proxyChat(`§cPlayer '${resolvedName}' not found.`);
                return null;
            }
            const stats = await this.getStats(mojangData.uuid);
            if (!stats) {
                this.proxy.proxyChat(`§cCould not retrieve data for '${mojangData.username}'.`);
                return null;
            }
            if (!stats.player.stats || !stats.player.stats[gameInfo.apiName]) {
                this.proxy.proxyChat(`§cNo ${gameInfo.displayName} stats found for '${mojangData.username}'.`);
                return null;
            }
            await this.displayFormattedStats(mojangData.username, mojangData.uuid, stats, gameInfo);
            return { mojangData, stats, gameInfo };
        } catch (err) {
            formatter.log(`Statcheck error: ${err.message}`);
            this.proxy.proxyChat(`§cAn error occurred.`);
            return null;
        }
    }

    async getPlayerCounts() {
        const data = await this.hypixelGet('counts');
        return data ? data.games : null;
    }

    // 8x8 face (with hat layer) rendered as colored block characters.
    async getAvatarLines(uuid) {
        return this.avatarCache.getOrLoad(uuid, async () => {
            let pixel;
            try {
                const skinUrl = await this.getSkinUrl(uuid);
                if (!skinUrl) throw new Error("No skin URL found");
                const skin = PNG.sync.read(await fetchBuffer(skinUrl));
                pixel = (x, y) => {
                    const hat = pixelAt(skin, 40 + x, 8 + y);
                    return hat.a > 128 ? hat : pixelAt(skin, 8 + x, 8 + y);
                };
            } catch (err) {
                formatter.log(`Skin processing failed: ${err.message}. Falling back to Minotar.`);
                const avatar = PNG.sync.read(await fetchBuffer(`https://minotar.net/helm/${uuid}/8.png`));
                pixel = (x, y) => pixelAt(avatar, x, y);
            }

            const lines = [];
            for (let y = 0; y < 8; y++) {
                let line = "";
                for (let x = 0; x < 8; x++) {
                    const p = pixel(x, y);
                    line += (p.a > 128) ? findClosestMinecraftColor(p.r, p.g, p.b) + '█' : " ";
                }
                lines.push(line);
            }
            return lines;
        });
    }

    async displayFormattedStats(username, uuid, stats, gameInfo) {
        try {
            const [asciiLines, guildTag] = await Promise.all([
                this.getAvatarLines(uuid).catch((e) => {
                    formatter.log(`Avatar unavailable: ${e.message}`);
                    return [];
                }),
                this.getGuild(uuid),
            ]);

            const p = stats.player;
            const client = this.proxy.client;
            if (!client) return;

            const prefix = "§8[§5jag§dprox§8] §r";
            asciiLines.forEach(line => {
                client.write("chat", {
                    message: JSON.stringify({ text: prefix + line }),
                    position: 1
                });
            });

            const guild = guildTag ? ` §5${guildTag}` : "";
            const s = statFormat.getModeStats(p, gameInfo);

            this.proxy.proxyChat(`§8[§5${gameInfo.displayName}§8]`);
            this.proxy.proxyChat(`${formatter.formatRank(p)} ${formatter.getPlayerNameColor(p)}${username}${guild}`);
            statFormat.formatStatLines(s, gameInfo.apiName).forEach(line => this.proxy.proxyChat(line));
            if (statFormat.hasWinstreaks(gameInfo.apiName)) {
                this.proxy.proxyChat(statFormat.formatWinstreakLine(s));
            }
        } catch (err) {
            formatter.log(`displayFormattedStats Error: ${err.message}`);
            this.proxy.proxyChat(`§cError displaying stats.`);
        }
    }

    async getTabDataForPlayer(name, gamemodeKey) {
        try {
            const gameInfo = gameModeMap[gamemodeKey];
            if (!gameInfo) return null;
            const mojangData = await this.getMojangUUID(name);
            if (!mojangData) return null;
            const stats = await this.getStats(mojangData.uuid);
            if (!stats) return null;

            const s = statFormat.getModeStats(stats.player, gameInfo);
            if (!s.hasStats) return null;

            if (gameInfo.apiName === 'Bedwars') {
                const fkdr = statFormat.ratio(s.finalKills, s.finalDeaths).toFixed(2);
                const prefix = `${this.getLevelColor(s.level)}[${s.levelText}] `;
                const suffix = ` §8|${this.getFkdrColor(parseFloat(fkdr))}${fkdr}`;
                return { prefix: prefix.substring(0, 16), suffix: suffix.substring(0, 16) };
            }
            if (gameInfo.apiName === 'SkyWars') {
                const kdr = statFormat.ratio(s.kills, s.deaths).toFixed(2);
                const prefix = `${this.getLevelColor(s.level)}[${s.levelText}] `;
                const suffix = ` §8|${this.getFkdrColor(parseFloat(kdr))}${kdr}`;
                return { prefix: prefix.substring(0, 16), suffix: suffix.substring(0, 16) };
            }
            if (gameInfo.apiName === 'Duels') {
                const wlr = statFormat.ratio(s.wins, s.losses).toFixed(2);
                const suffix = ` §8|${this.getFkdrColor(parseFloat(wlr))}${wlr}`;
                return { prefix: '', suffix: suffix.substring(0, 16) };
            }
            return null;
        } catch (e) {
            formatter.log(`Tab data error for ${name}: ${e.message}`);
            return null;
        }
    }

    getLevelColor(level) {
        if (level >= 500) return '§5';
        if (level >= 350) return '§4';
        if (level >= 200) return '§c';
        if (level >= 100) return '§e';
        if (level >= 50)  return '§f';
        return '§7';
    }

    getFkdrColor(fkdr) {
        if (fkdr >= 20) return '§5';
        if (fkdr >= 10) return '§4';
        if (fkdr >= 5)  return '§c';
        if (fkdr >= 3)  return '§e';
        if (fkdr >= 1)  return '§f';
        return '§7';
    }

    async processQueueAndPrintBulk(playerNames, gamemodeKey) {
        const gameInfo = gameModeMap[gamemodeKey] || gameModeMap.bedwars;
        const ownName = this.proxy.client?.username;

        const cleanPlayerNames = playerNames
            .map(name => name.replace(/§./g, '').replace(/\[.*?\]\s?/g, '').trim())
            .filter(name => name && name !== ownName);

        if (cleanPlayerNames.length === 0) return;

        this.proxy.proxyChat(`§dAuto-checking §f${cleanPlayerNames.length} §dplayers for §5${gameInfo.displayName}§d...`);

        // Tags and the chat summary run side by side; the stats cache makes them share requests.
        this.proxy.tabManager.updatePlayerTags(cleanPlayerNames, gamemodeKey)
            .catch(e => formatter.log(`Tab tag update failed: ${e.message}`));

        const statBlocks = (await mapLimit(cleanPlayerNames, LOOKUP_CONCURRENCY, (name) =>
            this.getAndFormatPartyPlayerStats(name, gameInfo))).filter(Boolean);

        if (statBlocks.length === 0) return;

        let msg = `${DIVIDER}\n`;
        msg += `  §5§lGame Stats §8(${gameInfo.displayName})\n \n`;
        msg += statBlocks.join('\n \n');
        msg += `\n${DIVIDER}`;
        this.proxy.proxyChat(msg);
    }
}

module.exports = HypixelHandler;
