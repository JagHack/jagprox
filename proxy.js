// ========================================================================= //
// ============ HYPROXY - KORRIGIERTE & OPTIMIERTE VERSION ================= //
// ========================================================================= //

// Stellt sicher, dass die .env-Datei geladen wird.
require("dotenv").config({ quiet: true });
if (!process.env.HYPIXEL_API_KEY) {
    console.log("Hypixel API key not found in .env file.");
    process.exit();
}

const fs = require("fs");
const yaml = require("yaml");
const mc = require("minecraft-protocol");
const fetch = require('node-fetch');
const Jimp = require('jimp');
const formatter = require("./formatter.js"); // Stellt sicher, dass diese Datei existiert.

let config;
try {
    config = yaml.parse(fs.readFileSync("./config.yml", "utf8"));
} catch (e) {
    console.log("Could not read or parse config.yml:", e.message);
    process.exit();
}


// --- Konstanten und Hilfsfunktionen ---

const commandAliases = {
    "/play solobw": "/play bedwars_eight_one",
    "/play doublesbw": "/play bedwars_eight_two",
    "/play 3sbw": "/play bedwars_four_three",
    "/play 4sbw": "/play bedwars_four_four",
    "/play 4v4bw": "/play bedwars_two_four",
    "/play castlebw": "/play bedwars_castle",
    "/play solosw": "/play skywars_solo_normal",
    "/play doublesinsanesw": "/play skywars_teams_insane",
    "/play doublesnormalsw": "/play skywars_teams_normal",
    "/play classicduels": "/play duels_classic_duel",
    "/play bridgeduels": "/play duels_bridge_duel",
    "/play uhcduels": "/play duels_uhc_duel",
    "/play skywarsduels": "/play duels_sw_duel",
    "/play sumoduels": "/play duels_sumo_duel",
    "/play bowduels": "/play duels_bow_duel",
    "/play comboduels": "/play duels_combo_duel",
    "/play opduels": "/play duels_op_duel",
};

const mcColors = [
    { code: '§0', rgb: [0, 0, 0] }, { code: '§1', rgb: [0, 0, 170] },
    { code: '§2', rgb: [0, 170, 0] }, { code: '§3', rgb: [0, 170, 170] },
    { code: '§4', rgb: [170, 0, 0] }, { code: '§5', rgb: [170, 0, 170] },
    { code: '§6', rgb: [255, 170, 0] }, { code: '§7', rgb: [170, 170, 170] },
    { code: '§8', rgb: [85, 85, 85] }, { code: '§9', rgb: [85, 85, 255] },
    { code: '§a', rgb: [85, 255, 85] }, { code: '§b', rgb: [85, 255, 255] },
    { code: '§c', rgb: [255, 85, 85] }, { code: '§d', rgb: [255, 85, 255] },
    { code: '§e', rgb: [255, 255, 85] }, { code: '§f', rgb: [255, 255, 255] }
];

function findClosestMinecraftColor(r, g, b) {
    let closest = mcColors[0];
    let minDistance = Infinity;
    for (const color of mcColors) {
        const distance = Math.sqrt(Math.pow(r - color.rgb[0], 2) + Math.pow(g - color.rgb[1], 2) + Math.pow(b - color.rgb[2], 2));
        if (distance < minDistance) {
            minDistance = distance;
            closest = color;
        }
    }
    return closest.code;
}

const gameModeMap = {
    "bw": { apiName: "Bedwars", displayName: "Bed Wars" },
    "bedwars": { apiName: "Bedwars", displayName: "Bed Wars" },
    "sw": { apiName: "SkyWars", displayName: "SkyWars" },
    "skywars": { apiName: "SkyWars", displayName: "SkyWars" },
    "duels": { apiName: "Duels", displayName: "Duels" },
};


// --- Hauptklasse für den Proxy ---

class HyProxy {
    constructor() {
        this.server = mc.createServer({
            "online-mode": true,
            port: config.port || 25565,
            version: config.version,
            motd: "HyProxy"
        });
        this.server.on("login", (client) => {
            this.client = client;
            this.handleLogin();
        });
        this.server.on("error", (err) => formatter.log(`Proxy server error: ${err.message}`));
        formatter.log(`Proxy server started on port ${config.port || 25565}.`);
        
        // HINWEIS: Dieser Cache wird initialisiert, aber nirgends im Code verwendet.
        // Du könntest hier eine Caching-Logik für API-Anfragen implementieren, um Ratenlimits zu schonen.
        this.statCache = new Map();
    }

    handleLogin() {
        formatter.log(`Client connected to proxy: ${this.client.username}`);
        this.target = mc.createClient({
            host: "mc.hypixel.net",
            port: 25565,
            username: this.client.username,
            auth: "microsoft",
            version: config.version,
            profile: this.client.profile,
            profilesFolder: config.cache_folder
        });

        this.target.on("connect", () => formatter.log(`Client connected to target: ${this.target.username}`));

        this.client.on("packet", (data, meta) => {
            if (meta.name === "chat" && data.message && data.message.startsWith("/") && this.handleCommand(data.message)) {
                return;
            }
            if (this.target.state === meta.state) {
                // VERBESSERUNG: Fehler werden nun geloggt, anstatt sie zu ignorieren.
                try {
                    this.target.write(meta.name, data);
                } catch (e) {
                    formatter.log(`Error writing packet to target: ${e.message}`);
                }
            }
        });

        this.target.on("packet", (data, meta) => {
            if (this.client.state === meta.state) {
                // VERBESSERUNG: Fehler werden nun geloggt, anstatt sie zu ignorieren.
                try {
                    this.client.write(meta.name, data);
                } catch (e) {
                    formatter.log(`Error writing packet to client: ${e.message}`);
                }
            }
        });

        this.client.on("error", (err) => {
            formatter.log(`Client error: ${err.message}`);
            this.target.end();
        });
        this.target.on("error", (err) => {
            formatter.log(`Target error: ${err.message}`);
            this.client.end();
        });
        this.client.on("end", () => {
            formatter.log(`Client disconnected: ${this.client.username}`);
            this.target.end();
        });
        this.target.on("end", () => {
            formatter.log(`Disconnected from target: ${this.client.username}`);
            this.client.end();
        });
    }

    handleCommand(command) {
        const lowerCommand = command.toLowerCase();
        if (commandAliases[lowerCommand]) {
            this.target.write("chat", { message: commandAliases[lowerCommand] });
            return true;
        }

        const parts = command.trim().split(" ").filter(Boolean);
        const cmd = parts[0].toLowerCase();

        const scPrefix = `/${config.commands.statcheck}`;
        if (cmd === scPrefix) {
            if (parts.length < 3) {
                this.proxyChat(`§cBenutzung: ${scPrefix} <gamemode> <Spielername>`);
                return true;
            }
            const gamemode = parts[1].toLowerCase();
            const username = parts[2];
            this.statcheck(gamemode, username);
            return true;
        }

        const statusPrefix = `/${config.commands.status}`;
        if (cmd === statusPrefix) {
            if (parts.length < 2) {
                this.proxyChat(`§cBenutzung: ${statusPrefix} <Spielername>`);
                return true;
            }
            const username = parts[1];
            this.getPlayerStatus(username);
            return true;
        }

        return false;
    }

    proxyChat(message) {
        this.client.write("chat", {
            message: JSON.stringify({ text: `${config.tag_prefix}${message}` }),
            position: 1
        });
    }

    async getPlayerStatus(username) {
        this.proxyChat(`§ePrüfe Status für ${username}...`);
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) {
                return this.proxyChat(`§cSpieler '${username}' nicht gefunden.`);
            }

            const status = await this.getHypixelStatus(mojangData.uuid);
            if (!status) {
                return this.proxyChat(`§cKonnte Status für '${mojangData.username}' nicht abrufen.`);
            }

            this.proxyChat("§7§m----------------------------------------");
            if (status.online) {
                this.proxyChat(`${formatter.formatRank(status.rank)} ${mojangData.username} §aist Online.`);
                if (status.hidden) {
                    this.proxyChat(`§7(Status ist verborgen, Spiel-Infos nicht verfügbar)`);
                } else {
                    this.proxyChat(`§fSpiel: §b${status.gameType}`);
                    if (status.mode) this.proxyChat(`§fModus: §e${status.mode}`);
                    if (status.map) this.proxyChat(`§fMap: §e${status.map}`);
                }
            } else {
                this.proxyChat(`${formatter.formatRank(status.rank)} ${mojangData.username} §cist Offline.`);
            }
            this.proxyChat("§7§m----------------------------------------");

        } catch (err) {
            formatter.log(`Status check error: ${err.message}`);
            this.proxyChat(`§cEin Fehler ist aufgetreten.`);
        }
    }

    async getHypixelStatus(uuid) {
        try {
            // Zwei parallele Anfragen für bessere Performance
            const [statusResponse, playerResponse] = await Promise.all([
                fetch(`https://api.hypixel.net/v2/status?key=${process.env.HYPIXEL_API_KEY}&uuid=${uuid}`),
                fetch(`https://api.hypixel.net/v2/player?key=${process.env.HYPIXEL_API_KEY}&uuid=${uuid}`)
            ]);

            const statusData = await statusResponse.json();
            const playerData = await playerResponse.json();

            if (!playerData.success || !playerData.player) return null;

            const player = playerData.player;
            const rank = (player.monthlyPackageRank && player.monthlyPackageRank === "SUPERSTAR") ? "MVP_PLUS_PLUS" : (player.newPackageRank || "NONE");

            if (statusData.success && statusData.session.online) {
                return {
                    online: true,
                    hidden: false,
                    gameType: statusData.session.gameType,
                    mode: statusData.session.mode,
                    map: statusData.session.map,
                    rank: rank,
                };
            }

            const lastLogin = player.lastLogin || 0;
            const lastLogout = player.lastLogout || 0;

            if (lastLogin > lastLogout) {
                return {
                    online: true,
                    hidden: true,
                    rank: rank,
                };
            }

            return { online: false, rank: rank };

        } catch (err) {
            formatter.log(`getHypixelStatus Error: ${err.message}`);
            return null;
        }
    }

    async statcheck(gamemode, username) {
        const gameInfo = gameModeMap[gamemode];
        if (!gameInfo) {
            return this.proxyChat(`§cUnbekannter Modus.`);
        }
        this.proxyChat(`§ePrüfe ${gameInfo.displayName}-Stats für ${username}...`);
        try {
            const mojangData = await this.getMojangUUID(username);
            if (!mojangData) {
                return this.proxyChat(`§cSpieler '${username}' nicht gefunden.`);
            }
            const stats = await this.getStats(mojangData.uuid, gameInfo.apiName);
            if (!stats) {
                return this.proxyChat(`§cKeine ${gameInfo.displayName}-Stats für '${mojangData.username}' gefunden.`);
            }
            this.displayFormattedStats(mojangData.username, mojangData.uuid, stats, gameInfo.displayName);
        } catch (err) {
            formatter.log(`Statcheck error: ${err.message}`);
            this.proxyChat(`§cEin Fehler ist aufgetreten.`);
        }
    }

    async getStats(uuid, gameApiName) {
        try {
            const response = await fetch(`https://api.hypixel.net/v2/player?key=${process.env.HYPIXEL_API_KEY}&uuid=${uuid}`);
            if (!response.ok) return null;

            const data = await response.json();
            if (!data.success || !data.player) return null;

            const player = data.player;
            const gameData = player.stats ? player.stats[gameApiName] : null;
            if (!gameData) return null;

            const res = {
                rank: (player.monthlyPackageRank && player.monthlyPackageRank === "SUPERSTAR") ? "MVP_PLUS_PLUS" : (player.newPackageRank || "NONE"),
                guild: await this.getGuild(uuid)
            };

            switch (gameApiName) {
                case "Bedwars":
                    res.level = player.achievements.bedwars_level || 0;
                    res.wins = gameData.wins_bedwars || 0;
                    res.losses = gameData.losses_bedwars || 1;
                    res.final_kills = gameData.final_kills_bedwars || 0;
                    res.final_deaths = gameData.final_deaths_bedwars || 1;
                    res.beds_broken = gameData.beds_broken_bedwars || 0;
                    res.beds_lost = gameData.beds_lost_bedwars || 1;
                    break;
                case "SkyWars":
                    res.level = player.achievements.skywars_you_re_a_star || 0;
                    res.wins = gameData.wins || 0;
                    res.losses = gameData.losses || 1;
                    break;
                case "Duels":
                    res.wins = gameData.wins || 0;
                    res.losses = gameData.losses || 1;
                    res.kills = gameData.kills || 0;
                    res.deaths = gameData.deaths || 1;
                    break;
                default:
                    return null;
            }
            return res;
        } catch (err) {
            formatter.log(`getStats Error: ${err.message}`);
            return null;
        }
    }

    async displayFormattedStats(username, uuid, stats, gameDisplayName) {
        try {
            const image = await Jimp.read(`https://crafatar.com/avatars/${uuid}?size=8&overlay=true`);
            const asciiLines = [];
            for (let y = 0; y < 8; y++) {
                let line = "";
                for (let x = 0; x < 8; x++) {
                    const pixel = Jimp.intToRGBA(image.getPixelColor(x, y));
                    if (pixel.a > 128) { // Nur Pixel zeichnen, die nicht transparent sind
                        line += findClosestMinecraftColor(pixel.r, pixel.g, pixel.b) + '█';
                    } else {
                        line += " "; // Fügt ein Leerzeichen für transparente Pixel hinzu
                    }
                }
                asciiLines.push(line);
            }

            const rank = formatter.formatRank(stats.rank);
            const guild = stats.guild ? ` §e[${stats.guild}]` : "";

            this.proxyChat("§7§m----------------------------------------");
            
            // Sendet den ASCII-Avatar Zeile für Zeile ohne den Chat-Präfix
            asciiLines.forEach(line => {
                this.client.write("chat", { message: JSON.stringify({ text: line }), position: 1 });
            });

            this.proxyChat(" "); // Leerzeile für Abstand

            switch (gameDisplayName) {
                case "Bed Wars":
                    const fkdr = (stats.final_kills / stats.final_deaths).toFixed(2);
                    const bblr = (stats.beds_broken / stats.beds_lost).toFixed(2);
                    const wlrBw = (stats.wins / stats.losses).toFixed(2);
                    this.proxyChat(`${rank} ${username} §7[§f${stats.level}✫§7]${guild}`);
                    this.proxyChat(`§fWins: §a${stats.wins.toLocaleString()} §8| §fLosses: §c${stats.losses.toLocaleString()}`);
                    this.proxyChat(`§fFinal Kills: §a${stats.final_kills.toLocaleString()} §8| §fFinal Deaths: §c${stats.final_deaths.toLocaleString()}`);
                    this.proxyChat(`§fFKDR: §6${fkdr} §8| §fBBLR: §6${bblr} §8| §fWLR: §6${wlrBw}`);
                    break;
                case "Duels":
                    const wlrDuels = (stats.wins / stats.losses).toFixed(2);
                    const kdrDuels = (stats.kills / stats.deaths).toFixed(2);
                    this.proxyChat(`${rank} ${username} §7[§f${stats.wins.toLocaleString()} Wins§7]${guild}`);
                    this.proxyChat(`§fWins: §a${stats.wins.toLocaleString()} §8| §fLosses: §c${stats.losses.toLocaleString()}`);
                    this.proxyChat(`§fKills: §a${stats.kills.toLocaleString()} §8| §fDeaths: §c${stats.deaths.toLocaleString()}`);
                    this.proxyChat(`§fWLR: §6${wlrDuels} §8| §fKDR: §6${kdrDuels}`);
                    break;
                case "SkyWars":
                    const wlrSw = (stats.wins / stats.losses).toFixed(2);
                    this.proxyChat(`${rank} ${username} §7[§f${stats.level}✫§7]${guild}`);
                    this.proxyChat(`§fWins: §a${stats.wins.toLocaleString()} §8| §fLosses: §c${stats.losses.toLocaleString()}`);
                    this.proxyChat(`§fWLR: §6${wlrSw}`);
                    break;
            }
            this.proxyChat("§7§m----------------------------------------");
        } catch (err) {
            formatter.log(`displayFormattedStats Error: ${err.message}`);
            this.proxyChat(`§cFehler beim Anzeigen der Stats (konnte Avatar nicht laden?).`);
        }
    }

    async getMojangUUID(username) {
        try {
            const response = await fetch(`https://api.mojang.com/users/profiles/minecraft/${username}`);
            if (!response.ok) {
                if (response.status === 429) {
                    this.proxyChat("§cMojang API Rate Limit.");
                }
                return null;
            }
            const data = await response.json();
            return { uuid: data.id, username: data.name };
        } catch (err) {
            formatter.log(`Fehler bei Mojang API: ${err.message}`);
            return null;
        }
    }

    async getGuild(uuid) {
        try {
            const response = await fetch(`https://api.hypixel.net/v2/guild?key=${process.env.HYPIXEL_API_KEY}&player=${uuid}`);
            if (!response.ok) return null;
            const data = await response.json();
            return data.guild ? data.guild.name : null;
        } catch (err) {
            formatter.log(`getGuild Error: ${err.message}`);
            return null;
        }
    }
}

module.exports = HyProxy;