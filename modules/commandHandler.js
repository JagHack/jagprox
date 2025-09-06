const fs = require('fs');
const yaml = require('yaml');
const aliasManager = require('../aliasManager.js');

class CommandHandler {
    constructor(proxy) {
        this.proxy = proxy;
    }

    resolveNickname(name) {
        const nicknames = this.proxy.config.nicknames || {};
        const lowerName = name.toLowerCase();

        for (const realName in nicknames) {
            if (nicknames[realName].toLowerCase() === lowerName) {
                return realName;
            }
        }
        return name;
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
            case 'statcheck': {
                const gamemode = args[0];
                const username = args.slice(1).join(' ');
                if (!gamemode || !username) {
                    this.proxy.proxyChat("§cUsage: /sc <gamemode> <username>");
                    return true;
                }
                const realName = this.resolveNickname(username);
                this.proxy.hypixel.statcheck(gamemode, realName);
                return true;
            }
            case 'status': {
                const username = args[0];
                if (!username) {
                    this.proxy.proxyChat("§cUsage: /status <username>");
                    return true;
                }
                const realName = this.resolveNickname(username);
                this.proxy.hypixel.getPlayerStatus(realName);
                return true;
            }
            case 'superf':
                this.handleSuperFriend(args);
                return true;

            case 'psc':
                this.proxy.hypixel.handlePartyStatCheck();
                return true;

            case 'alert':
                this.handleAlertCommand(args);
                return true;
            
            case 'nickname':
                this.handleNicknameCommand(args);
                return true;

            default:
                return false;
        }
    }

    handleNicknameCommand(args) {
        const action = args.shift()?.toLowerCase();
        
        if (!this.proxy.config.nicknames) {
            this.proxy.config.nicknames = {};
        }

        if (!action) {
            this.proxy.proxyChat("§cUsage: /nickname <add|remove|list>");
            this.proxy.proxyChat("§cAdd: /nickname add <real_name> <nickname>");
            this.proxy.proxyChat("§cRemove: /nickname remove <real_name_or_nickname>");
            return;
        }

        switch(action) {
            case 'add': {
                const realName = args.shift();
                const nickname = args.join(' ');
                if (!realName || !nickname) {
                    this.proxy.proxyChat("§cUsage: /nickname add <real_name> <nickname>");
                    return;
                }
                this.proxy.config.nicknames[realName] = nickname;
                this.saveConfig();
                this.proxy.proxyChat(`§aSet nickname for '${realName}' to '${nickname}'.`);
                break;
            }
            case 'remove': {
                const nameToRemove = args.join(' ');
                if (!nameToRemove) {
                    this.proxy.proxyChat("§cUsage: /nickname remove <real_name_or_nickname>");
                    return;
                }
                
                let found = false;
                const lowerNameToRemove = nameToRemove.toLowerCase();

                for (const realName in this.proxy.config.nicknames) {
                    const nickname = this.proxy.config.nicknames[realName];
                    if (realName.toLowerCase() === lowerNameToRemove || nickname.toLowerCase() === lowerNameToRemove) {
                        delete this.proxy.config.nicknames[realName];
                        this.proxy.proxyChat(`§aRemoved nickname for '${realName}'.`);
                        found = true;
                        break;
                    }
                }

                if (found) {
                    this.saveConfig();
                } else {
                    this.proxy.proxyChat(`§cNickname or player '${nameToRemove}' not found.`);
                }
                break;
            }
            case 'list': {
                const nicknames = this.proxy.config.nicknames;
                const keys = Object.keys(nicknames);
                if (keys.length === 0) {
                    this.proxy.proxyChat("§eYou have no nicknames set.");
                } else {
                    this.proxy.proxyChat("§aYour nicknames:");
                    keys.forEach(realName => {
                        this.proxy.proxyChat(`§8- §f${realName} §7-> §e${nicknames[realName]}`);
                    });
                }
                break;
            }
            default:
                this.proxy.proxyChat("§cInvalid action. Use 'add', 'remove', or 'list'.");
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
            fileConfig.nicknames = this.proxy.config.nicknames;
            fs.writeFileSync('./config.yml', yaml.stringify(fileConfig), 'utf8');
        } catch (e) {
            this.proxy.proxyChat("§cError saving configuration to file.");
            console.error("Config save error:", e);
        }
    }
}

module.exports = CommandHandler;