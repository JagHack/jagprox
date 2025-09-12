const mc = require("minecraft-protocol");
const formatter = require("./formatter.js");
const CommandHandler = require("./modules/commandHandler.js");
const HypixelHandler = require("./modules/hypixelHandler.js");
const QueueStatsHandler = require("./modules/queueStatsHandler.js");
const EntityManager = require("./modules/entityManager.js");
const TabManager = require("./modules/tabManager.js");
const TabAlerter = require("./modules/tabAlerter.js");
const path = require("path");
const fs = require("fs");

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

        this.hypixel = new HypixelHandler(this);
        this.commands = new CommandHandler(this);
        this.queueStats = new QueueStatsHandler(this);
        this.entityManager = new EntityManager(this);
        this.tabManager = new TabManager(this);
        this.tabAlerter = new TabAlerter(this);

        this.server = mc.createServer({
            "online-mode": true,
            port: this.config.port || 25565,
            version: this.config.version,
            motd: ' '.repeat(20) + '§a§lJagProx §c§l[1.8-1.21]\n' + ' '.repeat(13) + '§6§lHypixel Proxy §c§l- made by JagHack'
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
        this.lastPlayCommand = null;

        const userDataPath = process.env.USER_DATA_PATH || '.';
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
            version: this.config.version,
            profile: this.client.profile,
            profilesFolder: cachePath
        });

        this.target.on("connect", () => formatter.log(`Client connected to target: ${this.target.username}`));

        this.client.on("packet", (data, meta) => {
            if (meta.name === "chat" && data.message && data.message.startsWith("/")) {
                if (this.commands.handle(data.message)) {
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
                try {
                    let chatObject = JSON.parse(data.message);
                    if (hasNicknames) {
                        replaceNamesInComponent(chatObject, nicknames);
                    }
                    if (data.position === 0 || data.position === 1) {
                        console.log(`[JAGPROX_CHAT]${formatter.reconstructLegacyText(chatObject)}`);
                    }
                    data.message = JSON.stringify(chatObject);
                } catch(e) {}
            } else if (hasNicknames && meta.name === 'player_info' && (data.action === 'add_player' || data.action === 'update_display_name')) {
                data.data.forEach(player => {
                    const nickname = nicknames[player.name];
                    if (nickname) {
                        if (player.displayName) {
                            try {
                                let component = JSON.parse(player.displayName);
                                replaceNamesInComponent(component, { [player.name]: nickname });
                                player.displayName = JSON.stringify(component);
                            } catch (e) {
                                player.displayName = player.displayName.replace(player.name, nickname);
                            }
                        } else {
                            player.displayName = JSON.stringify({ text: nickname });
                        }
                    }
                });
            }

            this.queueStats.handlePacket(data, meta);
            this.entityManager.handlePacket(data, meta);
            this.tabManager.handlePacket(data, meta);
            this.tabAlerter.handlePacket(data, meta);

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
                formatter.log(`Connection error: ${err.message}`);
            }
            formatter.log(`Client disconnected: ${this.client ? this.client.username : 'Unknown'}`);

            if (this.target) this.target.end();
            if (this.client) this.client.end();

            this.queueStats.reset();
            this.entityManager.reset();
            this.tabManager.reset();
            this.tabAlerter.reset();

            this.client = null;
            this.target = null;
        };

        this.client.on("error", onEndOrError);
        this.client.on("end", onEndOrError);
        this.target.on("error", onEndOrError);
        this.target.on("end", onEndOrError);
    }

    proxyChat(message) {
        if (!this.client) return;
        this.client.write("chat", {
            message: JSON.stringify({ text: `§dJagProx §8» §r${message}` }),
            position: 0
        });
    }
}

module.exports = JagProx;