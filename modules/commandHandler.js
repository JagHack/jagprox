const fs = require('fs');
const yaml = require('yaml');
const aliasManager = require('../aliasManager.js');

class CommandHandler {
    constructor(proxy) {
        this.proxy = proxy;
    }

    handle(message) {
        const aliases = aliasManager.getAliases();
        const aliasedCommand = aliases[message.toLowerCase()];

        if (aliasedCommand) {
            this.proxy.proxyChat(`§eAlias executing: §f${aliasedCommand}`);
            this.proxy.target.write('chat', { message: aliasedCommand });
            return true;
        }

        const args = message.slice(1).split(' ');
        const command = args.shift().toLowerCase();

        const configAlias = Object.entries(this.proxy.config.commands || {})
            .find(([_, alias]) => alias === command);
        const baseCommand = configAlias ? configAlias[0] : command;

        switch (baseCommand) {
            case 'statcheck':
                this.proxy.hypixel.statcheck(args[0], args.slice(1).join(' '));
                return true;

            case 'status':
                this.proxy.hypixel.getPlayerStatus(args[0]);
                return true;

            case 'superf':
                this.handleSuperFriend(args);
                return true;

            case 'psc':
                this.proxy.hypixel.handlePartyStatCheck();
                return true;

            case 'alert':
                this.handleAlertCommand(args);
                return true;
            
            default:
                return false;
        }
    }

    handleAlertCommand(args) {
        const action = args.shift()?.toLowerCase();
        
        if (!this.proxy.config.tab_alerts) {
            this.proxy.config.tab_alerts = [];
        }
        const alertList = this.proxy.config.tab_alerts;

        if (action === 'list') {
            if (alertList.length === 0) {
                this.proxy.proxyChat("§eYour alert list is empty.");
            } else {
                this.proxy.proxyChat("§aPlayers on your alert list:");
                alertList.forEach(name => this.proxy.proxyChat(`§8- §f${name}`));
            }
            return;
        }

        const username = args.shift();
        if (!action || !username) {
            this.proxy.proxyChat("§cUsage: /alert <add|remove|list> [username]");
            return;
        }

        const playerIndex = alertList.findIndex(p => p.toLowerCase() === username.toLowerCase());

        switch (action) {
            case 'add':
                if (playerIndex !== -1) {
                    this.proxy.proxyChat(`§cPlayer '${username}' is already on the alert list.`);
                    return;
                }
                alertList.push(username);
                this.saveConfig();
                this.proxy.proxyChat(`§aAdded '${username}' to the alert list.`);
                break;
            case 'remove':
                if (playerIndex === -1) {
                    this.proxy.proxyChat(`§cPlayer '${username}' is not on the alert list.`);
                    return;
                }
                const removedPlayer = alertList.splice(playerIndex, 1);
                this.saveConfig();
                this.proxy.proxyChat(`§aRemoved '${removedPlayer[0]}' from the alert list.`);
                break;
            default:
                this.proxy.proxyChat("§cInvalid action. Use 'add', 'remove', or 'list'.");
        }
    }

    async handleSuperFriend(args) {
        const action = args.shift()?.toLowerCase();
        const username = args.shift();

        if (!username) {
            this.proxy.proxyChat("§cUsage: /superf <add|remove> <username> [gamemodes...]");
            return;
        }

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

                const mojangData = await this.proxy.hypixel.getMojangUUID(username);
                if (!mojangData) {
                    this.proxy.proxyChat(`§cPlayer '${username}' not found.`);
                    return;
                }
                const correctUsername = mojangData.username;

                this.proxy.config.super_friends[correctUsername] = gamemodes;
                this.saveConfig();
                this.proxy.proxyChat(`§aNow tracking ${correctUsername} for: §e${gamemodes.join(', ')}§a.`);
                break;
            }
            case 'remove': {
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

    saveConfig() {
        try {
            const fileConfig = yaml.parse(fs.readFileSync('./config.yml', 'utf8'));
            fileConfig.super_friends = this.proxy.config.super_friends;
            fileConfig.tab_alerts = this.proxy.config.tab_alerts;
            fs.writeFileSync('./config.yml', yaml.stringify(fileConfig), 'utf8');
        } catch (e) {
            this.proxy.proxyChat("§cError saving configuration to file.");
            console.error("Config save error:", e);
        }
    }
}

module.exports = CommandHandler;