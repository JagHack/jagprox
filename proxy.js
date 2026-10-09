const mc = require("minecraft-protocol");
const path = require("path");
const fs = require("fs");
const formatter = require("./formatter.js");
const CommandHandler = require("./modules/commandHandler.js");
const HypixelHandler = require("./modules/hypixelHandler.js");
const QueueStatsHandler = require("./modules/queueStatsHandler.js");
const TabManager = require("./modules/tabManager.js");
const TabAlerter = require("./modules/tabAlerter.js");
const AutoGGHandler = require("./modules/autoGGHandler.js");
const RankTracker = require("./modules/rankTracker.js");
const SuperFriendTracker = require("./modules/superFriendTracker.js");
const GametrackApiHandler = require("./modules/gametrackApiHandler.js");
const GametrackClientHandler = require("./modules/gametrackClientHandler.js");
const { replaceNamesInComponent, replaceNamesInText } = require("./utils/nicknames.js");
const { DEFAULT_PROXY_PORT } = require("./utils/constants.js");

const iconPath = process.env.ICON_PATH || path.join(__dirname, "server-icon.png");
const favicon = "data:image/png;base64," + fs.readFileSync(iconPath).toString("base64");

// Runs one module's packet hook; a bug in one module must never take down the session.
function safely(label, fn) {
    try {
        fn();
    } catch (e) {
        formatter.log(`[ERROR] ${label}: ${e.stack || e.message}`);
    }
}

// Tells the launcher (which owns Discord RPC) what the player is doing.
function reportStatus(status) {
    console.log(`[JAGPROX_STATUS] ${status}`);
}

class JagProx {
    constructor(config, env) {
        this.config = config;
        this.env = env;
        this.client = null;
        this.target = null;
        this.lastPlayCommand = null;
        this.mc_uuid = null;
        this.gametrackApiHandler = new GametrackApiHandler(this.env.jwt);
        this.gametrackClientHandler = null;

        this.hypixel = new HypixelHandler(this);
        this.commands = new CommandHandler(this);
        this.queueStats = new QueueStatsHandler(this);
        this.tabManager = new TabManager(this);
        this.tabAlerter = new TabAlerter(this);
        this.autoGG = new AutoGGHandler(this);
        this.rankTracker = new RankTracker(this);
        this.superFriends = new SuperFriendTracker(this);

        // Order matters: queueStats must see a packet before the tab footer is rewritten.
        this.packetModules = [
            ['queueStats', this.queueStats],
            ['tabManager', this.tabManager],
            ['tabAlerter', this.tabAlerter],
            ['autoGG', this.autoGG],
            ['rankTracker', this.rankTracker],
        ];

        const port = this.config.port || DEFAULT_PROXY_PORT;
        // Local-only by default; set `host: 0.0.0.0` in config.yml to expose it on purpose.
        const host = this.config.host || '127.0.0.1';

        this.server = mc.createServer({
            "online-mode": true,
            host,
            port,
            version: '1.8.9',
            motd: ' '.repeat(20) + '§8[§5jag§dprox§8] §d1.8-1.21\n' + ' '.repeat(13) + '§5§lHypixel Proxy §8- §7by §dJagHack',
            favicon: favicon
        });

        this.server.on("login", (client) => {
            if (this.client) {
                client.end('Another player is already connected to this proxy.');
                return;
            }
            this.handleLogin(client);
        });

        this.server.on("error", (err) => {
            formatter.log(`Proxy server error: ${err.message}`);
            if (err.code === 'EADDRINUSE') {
                console.error(`\n❌ FATAL ERROR: Port ${port} is already in use by another application.`);
                process.exit(1);
            }
        });
        formatter.log(`JagProx server started on ${host}:${port}.`);
    }

    resetModules() {
        safely('queueStats.reset', () => this.queueStats.reset());
        safely('tabManager.reset', () => this.tabManager.reset());
        safely('tabAlerter.reset', () => this.tabAlerter.reset());
        safely('autoGG.reset', () => this.autoGG.reset());
        safely('rankTracker.reset', () => this.rankTracker.reset());
        safely('hypixel.reset', () => this.hypixel.reset());
    }

    handleLogin(client) {
        this.client = client;
        formatter.log(`Client connected to proxy: ${client.username}`);
        this.mc_uuid = client.uuid;
        this.gametrackClientHandler = new GametrackClientHandler(this, client.uuid, client.username);
        this.lastPlayCommand = null;
        this.autoGG.reset();
        reportStatus('playing');

        const userDataPath = process.env.USER_DATA_PATH || '.';
        const cacheFolder = this.config.cache_folder || './cache/profiles';
        const cachePath = path.isAbsolute(cacheFolder) ? cacheFolder : path.join(userDataPath, cacheFolder);
        fs.mkdirSync(cachePath, { recursive: true });

        const target = mc.createClient({
            host: "mc.hypixel.net",
            port: 25565,
            username: client.username,
            auth: "microsoft",
            version: '1.8.9',
            profile: client.profile,
            profilesFolder: cachePath
        });
        this.target = target;
        this.superFriends.start();

        target.on("connect", () => formatter.log(`Client connected to target: ${target.username}`));

        client.on("packet", (data, meta) => {
            if (meta.name === "chat" && typeof data.message === 'string' && data.message.startsWith("/")) {
                let handledByJagProx = false;
                safely('commands.handle', () => {
                    handledByJagProx = this.commands.handle(data.message);
                });
                if (handledByJagProx) {
                    return;
                }
                if (data.message.toLowerCase().startsWith('/play ')) {
                    this.lastPlayCommand = data.message;
                    formatter.log(`Captured last play command: ${this.lastPlayCommand}`);
                }
            }
            if (meta.name === "custom_payload" && data.channel === "MC|Brand") {
                data.data = Buffer.from("\x07vanilla");
            }
            if (target.state === meta.state) {
                try {
                    target.write(meta.name, data);
                } catch (e) {
                    formatter.log(`Error writing packet to target: ${e.message}`);
                }
            }
        });

        target.on("packet", (data, meta) => {
            const nicknames = this.config.nicknames || {};
            const hasNicknames = Object.keys(nicknames).length > 0;

            if (meta.name === 'chat' && data.message) {
                safely('chat', () => this.handleServerChat(data, nicknames, hasNicknames));
            } else if (hasNicknames && meta.name === 'player_info' && (data.action === 'add_player' || data.action === 'update_display_name')) {
                safely('nicknames', () => this.applyNicknamesToPlayerInfo(data, nicknames));
            }

            for (const [label, module] of this.packetModules) {
                safely(label, () => module.handlePacket(data, meta));
            }

            if (meta.name === 'playerlist_header') {
                safely('tab footer', () => this.injectMapIntoFooter(data));
            }

            if (meta.name === "custom_payload" && data.channel === "MC|Brand") {
                data.data = Buffer.from("\x07vanilla");
            }
            if (client.state === meta.state) {
                try {
                    client.write(meta.name, data);
                } catch (e) {
                    formatter.log(`Error writing packet to client: ${e.message}`);
                }
            }
        });

        // end/error fire on both sockets, often several times; tear down exactly once,
        // and only reset shared state if this session is still the current one.
        let closed = false;
        const onEndOrError = (err) => {
            if (err) {
                formatter.log(`Connection error: ${err.message || JSON.stringify(err)}`);
            }
            if (closed) return;
            closed = true;
            formatter.log(`Client disconnected: ${client.username}`);

            try { target.end(); } catch (e) { /* already closed */ }
            try { client.end(); } catch (e) { /* already closed */ }

            if (this.client !== client) return;
            this.superFriends.stop();
            this.resetModules();
            this.client = null;
            this.target = null;
            this.gametrackClientHandler = null;
            reportStatus('idle');
        };

        client.on("error", onEndOrError);
        client.on("end", () => onEndOrError());
        target.on("error", onEndOrError);
        target.on("end", () => onEndOrError());
    }

    handleServerChat(data, nicknames, hasNicknames) {
        let chatObject;
        try {
            chatObject = JSON.parse(data.message);
        } catch (e) {
            formatter.debug(`Non-JSON chat message, passing through: ${e.message}`);
            this.gametrackClientHandler?.parseChatMessage({ text: data.message });
            return;
        }
        if (hasNicknames) {
            replaceNamesInComponent(chatObject, nicknames);
            data.message = JSON.stringify(chatObject);
        }
        if (data.position === 0 || data.position === 1) {
            console.log(`[JAGPROX_CHAT]${formatter.reconstructLegacyText(chatObject)}`);
        }
        this.gametrackClientHandler?.parseChatMessage(chatObject);
    }

    applyNicknamesToPlayerInfo(data, nicknames) {
        data.data.forEach(player => {
            const nickname = nicknames[player.name];
            if (!nickname) return;
            if (!player.displayName) {
                player.displayName = JSON.stringify({ text: nickname });
                return;
            }
            if (!player.displayName.includes(player.name)) return;
            const mapping = { [player.name]: nickname };
            try {
                const component = JSON.parse(player.displayName);
                replaceNamesInComponent(component, mapping);
                player.displayName = JSON.stringify(component);
            } catch (e) {
                player.displayName = replaceNamesInText(player.displayName, mapping);
            }
        });
    }

    injectMapIntoFooter(data) {
        const mapName = this.queueStats.currentMapName;
        if (!mapName) return;
        let footerObj;
        try {
            footerObj = JSON.parse(data.footer);
        } catch (e) {
            footerObj = { text: data.footer || '' };
        }

        const myLine = { text: '\n§7You are currently playing on §5' + mapName };
        if (footerObj && Array.isArray(footerObj.extra)) {
            footerObj.extra.push(myLine);
        } else {
            footerObj = { text: '', extra: [footerObj, myLine] };
        }
        data.footer = JSON.stringify(footerObj);
    }

    onGameChanged(newGameKey) {
        if (this.gametrackClientHandler) {
            this.gametrackClientHandler.onGameChanged(newGameKey)
                .catch(e => formatter.log(`[GameTrack] ${e.message}`));
        }
        // Team/tab state is cleared on the server-switch respawn (TabManager/RankTracker).
        safely('queueStats.resetTrigger', () => this.queueStats.resetTrigger());
        safely('tabAlerter.reset', () => this.tabAlerter.reset());
        safely('autoGG.reset', () => this.autoGG.reset());
    }

    proxyChat(message, clickEvent) {
        if (!this.client) return;

        const chatComponent = {
            text: `§8[§5jag§dprox§8] §r${message}`
        };

        if (clickEvent) {
            chatComponent.clickEvent = clickEvent;
            chatComponent.underlined = true;
            chatComponent.color = 'light_purple';
        }

        try {
            this.client.write("chat", {
                message: JSON.stringify(chatComponent),
                position: 0
            });
        } catch (e) {
            formatter.log(`Error writing chat to client: ${e.message}`);
        }
    }
}

module.exports = JagProx;
