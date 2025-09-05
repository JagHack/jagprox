const mc = require("minecraft-protocol");
const formatter = require("./formatter.js");
const CommandHandler = require("./modules/commandHandler.js");
const HypixelHandler = require("./modules/hypixelHandler.js");

class JagProx {
    constructor(config, env) {
        this.config = config;
        this.env = env;
        this.client = null;
        this.target = null;
        
        this.hypixel = new HypixelHandler(this);
        this.commands = new CommandHandler(this);

        this.server = mc.createServer({
            "online-mode": true,
            port: this.config.port || 25565,
            version: this.config.version,
            motd: "A JagProx Server"
        });

        this.server.on("login", (client) => {
            this.client = client;
            this.handleLogin();
        });

        this.server.on("error", (err) => formatter.log(`Proxy server error: ${err.message}`));
        formatter.log(`JagProx server started on port ${this.config.port || 25565}.`);
    }

    handleLogin() {
        formatter.log(`Client connected to proxy: ${this.client.username}`);
        this.target = mc.createClient({
            host: "mc.hypixel.net",
            port: 25565,
            username: this.client.username,
            auth: "microsoft",
            version: this.config.version,
            profile: this.client.profile,
            profilesFolder: this.config.cache_folder
        });

        this.target.on("connect", () => formatter.log(`Client connected to target: ${this.target.username}`));

        this.client.on("packet", (data, meta) => {
            if (meta.name === "custom_payload" && data.channel === "MC|Brand") {
                data.data = Buffer.from("\x07vanilla");
            }
            if (meta.name === "chat" && data.message && data.message.startsWith("/") && this.commands.handle(data.message)) {
                return;
            }
            if (this.target.state === meta.state) {
                try { this.target.write(meta.name, data); }
                catch (e) { formatter.log(`Error writing packet to target: ${e.message}`); }
            }
        });

        this.target.on("packet", (data, meta) => {
            if (meta.name === "custom_payload" && data.channel === "MC|Brand") {
                data.data = Buffer.from("\x07vanilla");
            }
            if (this.client.state === meta.state) {
                try { this.client.write(meta.name, data); }
                catch (e) { formatter.log(`Error writing packet to client: ${e.message}`); }
            }
        });

        this.client.on("error", (err) => { formatter.log(`Client error: ${err.message}`); this.target.end(); });
        this.target.on("error", (err) => { formatter.log(`Target error: ${err.message}`); this.client.end(); });
        this.client.on("end", () => { formatter.log(`Client disconnected: ${this.client.username}`); this.target.end(); });
        this.target.on("end", () => { formatter.log(`Disconnected from target: ${this.client.username}`); this.client.end(); });
    }

    proxyChat(message) {
        if (!this.client) return;
        this.client.write("chat", {
            message: JSON.stringify({ text: `${this.config.tag_prefix}${message}` }),
            position: 1
        });
    }
}

module.exports = JagProx;