const fs = require('fs');
const path = require('path');
const yaml = require('yaml');
const aliasManager = require('../aliasManager.js');
const formatter = require('../formatter.js');
const { gameModeMap } = require('../utils/constants.js');
const { getStatValue, statAliases } = require('../utils/stat-helper.js');

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
            if (aliasedCommand.toLowerCase().startsWith('/play ')) {
                this.proxy.lastPlayCommand = aliasedCommand;
                formatter.log(`Captured last play command (from alias): ${this.proxy.lastPlayCommand}`);
            }
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
                if (gamemode === '?') {
                    this.handleShowModes();
                    return true;
                }
                const username = args.slice(1).join(' ');
                if (!gamemode || !username) {
                    this.proxy.proxyChat("§cUsage: /sc <gamemode> <username>");
                    this.proxy.proxyChat("§eUse /sc ? to see all available gamemodes.");
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
            case 'goal':
                this.handleGoalCommand(args);
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
            case 'nickname':
                this.handleNicknameCommand(args);
                return true;
            case 'rq':
                if (this.proxy.lastPlayCommand) {
                    this.proxy.proxyChat(`§eRe-queuing: §f${this.proxy.lastPlayCommand}`);
                    this.proxy.target.write('chat', { message: this.proxy.lastPlayCommand });
                } else {
                    this.proxy.proxyChat("§cNo last game found to re-queue for.");
                }
                return true;
            case 'jagprox':
                this.handleHelpCommand();
                return true;
            default:
                return false;
        }
    }

    async handleGoalCommand(args) {
        const action = args.shift()?.toLowerCase();
        const userDataPath = process.env.USER_DATA_PATH || '.';
        const goalPath = path.join(userDataPath, 'goal.json');
    
        const readGoal = () => fs.existsSync(goalPath) ? JSON.parse(fs.readFileSync(goalPath, 'utf8')) : null;
        const saveGoal = (goal) => fs.writeFileSync(goalPath, JSON.stringify(goal, null, 4), 'utf8');
        const cancelGoal = () => fs.existsSync(goalPath) && fs.unlinkSync(goalPath);
    
        switch (action) {
            case 'set': {
                const gamemode = args.shift()?.toLowerCase();
                const statAlias = args.shift()?.toLowerCase();
                const target = parseFloat(args.shift());
    
                if (!gamemode || !statAlias || isNaN(target)) {
                    this.proxy.proxyChat("§cUsage: /goal set <game> <stat> <target>");
                    this.proxy.proxyChat("§eExample: /goal set bedwars fkdr 5");
                    return;
                }
    
                if (!statAliases[gamemode] || !statAliases[gamemode][statAlias]) {
                    this.proxy.proxyChat(`§cInvalid stat. Available for ${gamemode}: §e${Object.keys(statAliases[gamemode]).join(', ')}`);
                    return;
                }
    
                this.proxy.proxyChat("§eFetching your current stats to set the goal...");
                const uuid = this.proxy.client.uuid;
                if (!uuid) return this.proxy.proxyChat("§cCould not identify your UUID. Please relog.");
    
                const stats = await this.proxy.hypixel.getStats(uuid);
                if (!stats || !stats.player) return this.proxy.proxyChat("§cCould not fetch your Hypixel stats.");
    
                const statResult = getStatValue(stats.player, gamemode, statAlias);
                if (statResult === null) return this.proxy.proxyChat("§cAn error occurred while retrieving the specific stat.");
    
                const initialValue = statResult.value;
                if (target <= initialValue) return this.proxy.proxyChat(`§cYour target of ${target.toLocaleString()} is not higher than your current ${statResult.name} of ${initialValue.toLocaleString()}!`);
    
                const goal = { gamemode, statAlias, target, initial: initialValue, name: statResult.name, setAt: Date.now() };
                saveGoal(goal);
                this.proxy.proxyChat(`§aGoal set! Track your progress for §e${statResult.name} in ${gamemode}§a to reach §6${target.toLocaleString()}§a.`);
                break;
            }
    
            case 'view': {
                const goal = readGoal();
                if (!goal) return this.proxy.proxyChat("§eYou do not have an active goal. Use /goal set <game> <stat> <target>.");
    
                this.proxy.proxyChat("§eChecking your goal progress...");
                const uuid = this.proxy.client.uuid;
                if (!uuid) return this.proxy.proxyChat("§cCould not identify your UUID. Please relog.");
                
                const stats = await this.proxy.hypixel.getStats(uuid);
                if (!stats || !stats.player) return this.proxy.proxyChat("§cCould not fetch your Hypixel stats.");
    
                const statResult = getStatValue(stats.player, goal.gamemode, goal.statAlias);
                const currentValue = statResult.value;
    
                const progress = currentValue - goal.initial;
                const totalNeeded = goal.target - goal.initial;
                const percentage = Math.max(0, Math.min(100, (progress / totalNeeded) * 100));
                const remaining = Math.max(0, goal.target - currentValue);
                
                const progressBarLength = 20;
                const filledLength = Math.round((progressBarLength * percentage) / 100);
                const bar = `§a${'█'.repeat(filledLength)}§7${'█'.repeat(progressBarLength - filledLength)}`;
    
                this.proxy.proxyChat(`§d§m----------------------------------------------------`);
                this.proxy.proxyChat(`  §d§lGoal: ${goal.name} in ${goal.gamemode}`);
                this.proxy.proxyChat(`  §7${goal.initial.toLocaleString()} §f-> §6${goal.target.toLocaleString()}`);
                this.proxy.proxyChat(` `);
                this.proxy.proxyChat(`  §fProgress: ${bar} §e${percentage.toFixed(2)}%`);
                this.proxy.proxyChat(`  §aCurrent: ${currentValue.toLocaleString()} §c(Remaining: ${remaining.toLocaleString()})`);
                this.proxy.proxyChat(`§d§m----------------------------------------------------`);
                break;
            }
            
            case 'cancel': {
                if (!readGoal()) return this.proxy.proxyChat("§eYou do not have an active goal to cancel.");
                cancelGoal();
                this.proxy.proxyChat("§aYour active goal has been cancelled.");
                break;
            }
            
            default:
                this.proxy.proxyChat("§cInvalid subcommand. Use /goal <set|view|cancel>.");
                break;
        }
    }

    handleShowModes() {
        const modesByCategory = {};
        for (const alias in gameModeMap) {
            const modeInfo = gameModeMap[alias];
            if (!modesByCategory[modeInfo.displayName]) {
                modesByCategory[modeInfo.displayName] = [];
            }
            modesByCategory[modeInfo.displayName].push(alias);
        }

        let helpMessage = "§d§m----------------------------------------------------\n";
        helpMessage += "§r  §d§lAvailable Statcheck Gamemodes\n \n";

        const sortedCategories = Object.keys(modesByCategory).sort();

        for (const category of sortedCategories) {
            const aliases = modesByCategory[category].join(', ');
            helpMessage += `§r  §e${category}: §b${aliases}\n`;
        }
        
        helpMessage += "\n§d§m----------------------------------------------------";
        this.proxy.proxyChat(helpMessage);
    }

    handleHelpCommand() {
        const configCmds = this.proxy.config.commands || {};
        const scAlias = configCmds.statcheck || 'sc';
        const statusAlias = configCmds.status || 'status';

        const commandList = [
            { syntax: `/${scAlias} <game> <player>`, desc: 'Checks Hypixel stats for a player.' },
            { syntax: `/${statusAlias} <player>`, desc: "Shows a player's online status." },
            { syntax: '/goal <set|view|cancel> [args]', desc: 'Manages your personal stat goals.' },
            { syntax: '/psc', desc: 'Runs a stat check for all party members.' },
            { syntax: '/rq', desc: 'Re-queues your last played game.' },
            { syntax: '/alert <add|rem|list> [player]', desc: 'Manages in-game alerts for players.' },
            { syntax: '/nickname <add|rem|list> [args]', desc: 'Sets local nicknames for players.' },
            { syntax: '/superf <add|rem|list> [args]', desc: "Tracks friends' game activity." },
            { syntax: '/jagprox', desc: 'Displays this help message.' }
        ];

        let helpMessage = "§d§m----------------------------------------------------\n";
        helpMessage += "§r  §d§lJagProx §8- §7Available Commands\n \n";

        commandList.forEach(c => {
            const parts = c.syntax.split(' ');
            const cmd = parts.shift();
            const args = parts.join(' ');
            const coloredSyntax = `§d${cmd} §e${args}`;

            helpMessage += `§r  ${coloredSyntax}\n`;
            helpMessage += `§r    §8- §7${c.desc}\n \n`;
        });

        helpMessage = helpMessage.trimEnd();
        helpMessage += "\n§r\n§d§m----------------------------------------------------";

        this.proxy.proxyChat(helpMessage);
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

        if (!this.proxy.config.super_friends) {
            this.proxy.config.super_friends = {};
        }

        if (action === 'list') {
            const friends = this.proxy.config.super_friends;
            const keys = Object.keys(friends);
            if (keys.length === 0) {
                this.proxy.proxyChat("§eYou have no super friends set.");
            } else {
                this.proxy.proxyChat("§aYour super friends:");
                keys.forEach(name => {
                    this.proxy.proxyChat(`§8- §f${name} §7(${friends[name].join(', ')})`);
                });
            }
            return;
        }

        const username = args.shift();

        if (!action || !username) {
            this.proxy.proxyChat("§cUsage: /superf <add|remove|list> <username> [gamemodes...]");
            return;
        }

        switch(action) {
            case 'add': {
                const gamemodes = args;
                if (gamemodes.length === 0) {
                    this.proxy.proxyChat("§cYou must specify at least one gamemode (e.g., bedwars).");
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
                    this.proxy.proxyChat(`§cPlayer '${username}' is not currently being tracked.`);
                }
                break;
            }
            default:
                this.proxy.proxyChat("§cInvalid action. Use 'add', 'remove', or 'list'.");
        }
    }

    saveConfig() {
        const userDataPath = process.env.USER_DATA_PATH || '.';
        const configPath = path.join(userDataPath, 'config.yml');
        try {
            const fileConfig = yaml.parse(fs.readFileSync(configPath, 'utf8'));
            fileConfig.super_friends = this.proxy.config.super_friends;
            fileConfig.tab_alerts = this.proxy.config.tab_alerts;
            fileConfig.nicknames = this.proxy.config.nicknames;
            fs.writeFileSync(configPath, yaml.stringify(fileConfig), 'utf8');
        } catch (e) {
            this.proxy.proxyChat("§cError saving configuration to file.");
            console.error("Config save error:", e);
        }
    }
}

module.exports = CommandHandler;