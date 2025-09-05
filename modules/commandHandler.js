const fs = require('fs');
const yaml = require('yaml');

class CommandHandler {
    constructor(proxy) {
        this.proxy = proxy;
    }

    handle(message) {
        const args = message.slice(1).split(' ');
        const command = args.shift().toLowerCase();

        // Map config aliases to the actual command
        const alias = Object.entries(this.proxy.config.commands || {})
            .find(([_, alias]) => alias === command);

        const baseCommand = alias ? alias[0] : command;

        switch (baseCommand) {
            case 'statcheck':
                // Usage: /sc <gamemode> <player>
                this.proxy.hypixel.statcheck(args[0], args.slice(1).join(' '));
                return true;

            case 'status':
                // Usage: /status <player>
                this.proxy.hypixel.getPlayerStatus(args[0]);
                return true;

            // --- NEW: Super Friends Command ---
            case 'superf':
                // Usage: /superf <add|remove> <username> [gamemodes...]
                this.handleSuperFriend(args);
                return true;

            // --- NEW: Party Stat Check Command ---
            case 'psc':
                // Usage: /psc
                this.proxy.hypixel.handlePartyStatCheck();
                return true;
        }
        return false; // Command not handled by the proxy
    }

    /**
     * Handles the logic for adding/removing tracked "super friends".
     * @param {string[]} args - The arguments for the command.
     */
    async handleSuperFriend(args) { // Made async to await API calls
        const action = args.shift()?.toLowerCase();
        const username = args.shift();

        if (!username) {
            this.proxy.proxyChat("§cUsage: /superf <add|remove> <username> [gamemodes...]");
            return;
        }

        // Ensure the config section exists to avoid errors
        if (!this.proxy.config.super_friends) {
            this.proxy.config.super_friends = {};
        }

        switch(action) {
            case 'add': {
                const gamemodes = args;
                if (gamemodes.length === 0) {
                    this.proxy.proxyChat("§cYou must specify at least one gamemode to track (e.g., bedwars).");
                    return;
                }

                // Get correct username casing from Mojang API
                const mojangData = await this.proxy.hypixel.getMojangUUID(username);
                if (!mojangData) {
                    this.proxy.proxyChat(`§cPlayer '${username}' not found.`);
                    return;
                }
                const correctUsername = mojangData.username;

                // Use the player's name with correct capitalization for the key
                this.proxy.config.super_friends[correctUsername] = gamemodes;
                this.saveConfig();
                this.proxy.proxyChat(`§aNow tracking ${correctUsername} for: §e${gamemodes.join(', ')}§a.`);
                break;
            }
            case 'remove': {
                // Find the key in the config, ignoring case, to allow for easy removal
                const keyToRemove = Object.keys(this.proxy.config.super_friends)
                    .find(key => key.toLowerCase() === username.toLowerCase());

                if (keyToRemove) {
                    delete this.proxy.config.super_friends[keyToRemove];
                    this.saveConfig();
                    this.proxy.proxyChat(`§aRemoved ${keyToRemove} from tracked friends.`);
                } else {
                    this.proxy.proxyChat(`§cPlayer ${username} is not currently being tracked.`);
                }
                break;
            }
            default:
                this.proxy.proxyChat("§cInvalid action. Use 'add' or 'remove'.");
        }
    }

    /**
     * Saves the current in-memory config back to the config.yml file.
     */
    saveConfig() {
        try {
            // Read the full config file to preserve comments and other structure
            const fileConfig = yaml.parse(fs.readFileSync('./config.yml', 'utf8'));
            // Update only the super_friends section
            fileConfig.super_friends = this.proxy.config.super_friends;
            fs.writeFileSync('./config.yml', yaml.stringify(fileConfig), 'utf8');
        } catch (e) {
            this.proxy.proxyChat("§cError saving configuration to file.");
            console.error("Config save error:", e);
        }
    }
}

module.exports = CommandHandler;