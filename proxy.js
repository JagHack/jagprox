const mc = require("minecraft-protocol");
const formatter = require("./formatter.js");
const CommandHandler = require("./modules/commandHandler.js");
const HypixelHandler = require("./modules/hypixelHandler.js");
const QueueStatsHandler = require("./modules/queueStatsHandler.js");
const EntityManager = require("./modules/entityManager.js");
const TabManager = require("./modules/tabManager.js");
const TabAlerter = require("./modules/tabAlerter.js");
const AutoGGHandler = require("./modules/autoGGHandler.js");
const GametrackApiHandler = require("./modules/gametrackApiHandler.js");
const GametrackClientHandler = require("./modules/gametrackClientHandler.js");
const path = require("path");
const fs = require("fs");
const discordRpc = require('./modules/discordRpcHandler.js');
const iconPath = process.env.ICON_PATH || path.join(__dirname, "server-icon.png");
const icon = fs.readFileSync(iconPath);
const favicon = "data:image/png;base64," + icon.toString("base64");

function replaceNamesInComponent(component, nicknames) {
    if (!component) return;
    if (typeof component === 'string') return;

    const realName = Object.keys(nicknames).find(name => (component.text && component.text.includes(name)));
    if (realName) {
        component.text = component.text.replace(new RegExp(realName, 'g'), nicknames[realName]);
    }
    if (component.extra && Array.isArray(component.extra)) {
        component.extra.forEach(part => replaceNamesInComponent(part, nicknames));
    }
}

class JagProx {
    constructor(config, env) {
        this.config = config;
        this.env = env;
        this.client = null;
        this.target = null;
        this.lastPlayCommand = null;
        this.mc_uuid = null;
        this.ownJagproxRank = null;
        this.gametrackApiHandler = new GametrackApiHandler(this.env.jwt);
        this.gametrackClientHandler = null;

        this.hypixel = new HypixelHandler(this);
        this.commands = new CommandHandler(this);
        this.queueStats = new QueueStatsHandler(this);
        this.entityManager = new EntityManager(this);
        this.tabManager = new TabManager(this);
        this.tabAlerter = new TabAlerter(this);
        this.autoGG = new AutoGGHandler(this);

        if (this.config.discord_rpc && this.config.discord_rpc.enabled) {
            discordRpc.login();
        }

        this.server = mc.createServer({
            "online-mode": true,
            port: this.config.port || 25565,
            version: '1.8.9',
            motd: ' '.repeat(20) + '§8[§5jag§dprox§8] §d1.8-1.21\n' + ' '.repeat(13) + '§5§lHypixel Proxy §8- §7by §dJagHack',
            favicon: favicon
        });

        this.server.on("login", (client) => {
            if (this.client) {
                client.end('Ein Spieler ist bereits mit diesem Proxy verbunden.');
                return;
            }
            this.client = client;
            this.handleLogin();
        });

        this.server.on("error", (err) => formatter.log(`Proxy server error: ${err.message}`));
        formatter.log(`JagProx server started on port ${this.config.port || 25565}.`);
    }

    handleLogin() {
        formatter.log(`Client connected to proxy: ${this.client.username}`);
        this.mc_uuid = this.client.uuid;
        this.gametrackClientHandler = new GametrackClientHandler(this, this.client.uuid, this.client.username);
        this.lastPlayCommand = null;
        this.autoGG.reset();

        if (this.env.jwt && this.hypixel.apiHandler) {
            this.hypixel.apiHandler.getOwnProfile().then(profile => {
                this.ownJagproxRank = profile?.globalRank || null;
                formatter.log(`[JAGPROX] Own jagprox rank loaded: ${this.ownJagproxRank || 'null (no rank)'}`);
                formatter.log(`[JAGPROX] Profile data: ${JSON.stringify(profile)}`);

                if (this.ownJagproxRank && this.client) {
                    if (this.queueStats.currentGameKey) {
                        formatter.log(`[JAGPROX] Updating tab with jagprox rank for game: ${this.queueStats.currentGameKey}`);
                        setTimeout(() => {
                            if (this.client && this.queueStats.currentGameKey) {
                                this.tabManager.updatePlayerTags([this.client.username], this.queueStats.currentGameKey);
                            }
                        }, 500);
                    } else {
                        formatter.log(`[JAGPROX] Jagprox rank loaded but not in a game yet. Will apply when game starts.`);
                    }
                }
            }).catch(e => {
                formatter.log(`[JAGPROX] Could not fetch own jagprox rank: ${e.message}`);
                this.ownJagproxRank = null;
            });
        } else {
            formatter.log(`[JAGPROX] Skipping jagprox rank fetch: jwt=${!!this.env.jwt}, apiHandler=${!!this.hypixel.apiHandler}`);
        }

        discordRpc.setActivity({
            details: 'Playing',
            state: 'In Game',
            largeImageKey: 'icon',
            largeImageText: 'JagProx',
            instance: false,
            startTimestamp: new Date()
        });

        const userDataPath = process.env.USER_DATA_PATH || '.';
        const cacheFolder = this.config.cache_folder || './cache/profiles';
        const cachePath = path.isAbsolute(this.config.cache_folder)
            ? this.config.cache_folder
            : path.join(userDataPath, this.config.cache_folder);

        if (!fs.existsSync(cachePath)) {
            fs.mkdirSync(cachePath, { recursive: true });
        }

        this.target = mc.createClient({
            host: "mc.hypixel.net",
            port: 25565,
            username: this.client.username,
            auth: "microsoft",
            version: '1.8.9',
            profile: this.client.profile,
            profilesFolder: cachePath
        });

        this.target.on("connect", () => formatter.log(`Client connected to target: ${this.target.username}`));
       this.client.on("packet", async (data, meta) => {
            if (meta.name === "chat" && data.message && data.message.startsWith("/")) {
                const handledByJagProx = await this.commands.handle(data.message);
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
            if (this.target && this.target.state === meta.state) {
                try {
                    this.target.write(meta.name, data);
                } catch (e) {
                    formatter.log(`Error writing packet to target: ${e.message}`);
                }
            }
        });

        this.target.on("packet", (data, meta) => {
            const nicknames = this.config.nicknames || {};
            const hasNicknames = Object.keys(nicknames).length > 0;

            if (meta.name === 'chat' && data.message) {
                let chatObject;
                try {
                    chatObject = JSON.parse(data.message);
                    if (hasNicknames) {
                        replaceNamesInComponent(chatObject, nicknames);
                    }
                    if (data.position === 0 || data.position === 1) {
                        console.log(`[JAGPROX_CHAT]${formatter.reconstructLegacyText(chatObject)}`);
                    }
                    this.gametrackClientHandler.parseChatMessage(chatObject);
                    data.message = JSON.stringify(chatObject);
                } catch(e) {
                    this.gametrackClientHandler.parseChatMessage({text: data.message}); 
                    formatter.log(`Error parsing chat JSON for gametrack: ${e.message}. Passing raw message.`);
                }
            } else if (hasNicknames && meta.name === 'player_info' && (data.action === 'add_player' || data.action === 'update_display_name')) {
                data.data.forEach(player => {
                    const nickname = nicknames[player.name];
                    if (nickname) {
                        if (player.displayName) {
                            
                            if (player.displayName.includes(player.name)) {
                                try {
                                    let component = JSON.parse(player.displayName);
                                    replaceNamesInComponent(component, { [player.name]: nickname });
                                    player.displayName = JSON.stringify(component);
                                } catch (e) {
                                    
                                    player.displayName = player.displayName.replace(new RegExp(player.name, 'g'), nickname);
                                }
                            }
                        } else {
                            
                            player.displayName = JSON.stringify({ text: nickname });
                        }
                    }
                });
            }

            this.queueStats.handlePacket(data, meta);

            if (meta.name === 'playerlist_header') {
                try {
                    const q = this.queueStats;
                    if (q.currentMapName) {
                        let footerObj;
                        try {
                            footerObj = JSON.parse(data.footer);
                        } catch(e) {
                            footerObj = { text: data.footer || '' };
                        }

                        const myLine = { text: '\n§7You are currently playing on §5' + q.currentMapName };

                        if (footerObj.extra && Array.isArray(footerObj.extra)) {
                            footerObj.extra.push(myLine);
                        } else {
                            footerObj = { text: '', extra: [footerObj, myLine] };
                        }
                        data.footer = JSON.stringify(footerObj);
                    }
                } catch(e) {
                    formatter.log(`Error injecting into tab footer: ${e.message}`);
                }
            }

            this.entityManager.handlePacket(data, meta);
            this.tabManager.handlePacket(data, meta);
            this.tabAlerter.handlePacket(data, meta);
            this.autoGG.handlePacket(data, meta);

            if (meta.name === "custom_payload" && data.channel === "MC|Brand") {
                data.data = Buffer.from("\x07vanilla");
            }
            if (this.client && this.client.state === meta.state) {
                try {
                    this.client.write(meta.name, data);
                } catch (e) {
                    formatter.log(`Error writing packet to client: ${e.message}`);
                }
            }
        });

        const onEndOrError = (err) => {
            if (err) {
            formatter.log(`Connection error: ${err.message || JSON.stringify(err)}`);
            }
            formatter.log(`Client disconnected: ${this.client ? this.client.username : 'Unknown'}`);

            if (this.target) this.target.end();
            if (this.client) this.client.end();

            this.queueStats.reset();
            this.entityManager.reset();
            this.tabManager.reset();
            this.tabAlerter.reset();
            this.autoGG.reset();
            this.hypixel.reset();

            this.client = null;
            this.target = null;

            discordRpc.setActivity({
                details: 'Idling',
                state: 'In Launcher',
                largeImageKey: 'icon',
                largeImageText: 'JagProx',
                instance: false,
                startTimestamp: new Date()
            });
        };

        this.client.on("error", onEndOrError);
        this.client.on("end", onEndOrError);
        this.target.on("error", onEndOrError);
        this.target.on("end", onEndOrError);
    }

    onGameChanged(newGameKey) {
        if (this.gametrackClientHandler) {
            this.gametrackClientHandler.onGameChanged(newGameKey);
        }

        this.queueStats.resetTrigger();
        this.entityManager.reset();
        this.tabManager.reset();
        this.tabAlerter.reset();
        this.autoGG.reset();

        this.hypixel.reset();

        if (this.ownJagproxRank && this.client && newGameKey) {
            formatter.log(`[JAGPROX] Game changed to ${newGameKey}, applying jagprox tag to own player`);
            setTimeout(() => {
                if (this.client && this.queueStats.currentGameKey) {
                    this.tabManager.updatePlayerTags([this.client.username], this.queueStats.currentGameKey);
                }
            }, 1000);
        }
    }

    getOwnJagproxTag() {
        const formatter = require('./formatter.js');
        const tag = formatter.getJagproxRank(this.ownJagproxRank);
        formatter.log(`[JAGPROX] getOwnJagproxTag called: rank="${this.ownJagproxRank}", tag="${tag}"`);
        return tag;
    }

    proxyChat(message) {
        if (!this.client) return;
        this.client.write("chat", {
            message: JSON.stringify({ text: `§8[§5jag§dprox§8] §r${message}` }),
            position: 0
        });
    }
}

module.exports = JagProx;
