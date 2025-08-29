const { gameModeMap } = require("../utils/constants"); // Removed commandAliases

class CommandHandler {
    constructor(proxy) {
        this.proxy = proxy;
    }

    handle(command) {
        const lowerCommand = command.toLowerCase();
        if (this.proxy.config.aliases && this.proxy.config.aliases[lowerCommand]) {
            this.proxy.target.write("chat", { message: this.proxy.config.aliases[lowerCommand] });
            return true;
        }

        const parts = command.trim().split(" ").filter(Boolean);
        const cmd = parts[0].toLowerCase();
        const scPrefix = `/${this.proxy.config.commands.statcheck}`;

        if (cmd === scPrefix) {
            if (parts.length === 2 && parts[1].toLowerCase() === 'help') {
                this.proxy.proxyChat("§e§lAvailable Game Modes for /sc:");
                const groupedModes = {};
                for (const alias in gameModeMap) {
                    const mode = gameModeMap[alias];
                    if (!groupedModes[mode.displayName]) {
                        groupedModes[mode.displayName] = [];
                    }
                    groupedModes[mode.displayName].push(alias);
                }
                for (const displayName in groupedModes) {
                    const aliases = groupedModes[displayName].join("§7, §a");
                    this.proxy.proxyChat(`§f${displayName}: §a${aliases}`);
                }
                return true;
            }
            if (parts.length < 3) {
                this.proxy.proxyChat(`§cUsage: ${scPrefix} <gamemode> <player>`);
                this.proxy.proxyChat(`§cUse "${scPrefix} help" to see all available gamemodes.`);
                return true;
            }
            const gamemode = parts[1].toLowerCase();
            const username = parts[2];
            this.proxy.hypixel.statcheck(gamemode, username);
            return true;
        }

        const statusPrefix = `/${this.proxy.config.commands.status}`;
        if (cmd === statusPrefix) {
            if (parts.length < 2) {
                this.proxy.proxyChat(`§cUsage: ${statusPrefix} <player>`);
                return true;
            }
            const username = parts[1];
            this.proxy.hypixel.getPlayerStatus(username);
            return true;
        }

        return false;
    }
}

module.exports = CommandHandler;