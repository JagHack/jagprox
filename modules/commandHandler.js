const fs = require('fs');
const path = require('path');
const yaml = require('yaml');
const aliasManager = require('../aliasManager.js');
const formatter = require('../formatter.js');
const { gameModeMap, quickQueueMap } = require('../utils/constants.js');
const { getStatValue, statAliases } = require('../utils/stat-helper.js');
const discordRpc = require('./discordRpcHandler.js');
const { API_BASE_URL, WEB_LINK_BASE_URL } = require('../utils/api_constants.js');

class CommandHandler {
    constructor(proxy) {
        this.proxy = proxy;
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
                const realName = this.proxy.hypixel.resolveNickname(username);
                this.proxy.hypixel.statcheck(gamemode, realName);
                return true;
            }
            case 'status': {
                const username = args[0];
                if (!username) {
                    this.proxy.proxyChat("§cUsage: /status <username>");
                    return true;
                }
                const realName = this.proxy.hypixel.resolveNickname(username);
                this.proxy.hypixel.getPlayerStatus(realName);
                return true;
            }
            case 'q':
                this.handleQuickQueue(args);
                return true;
            case 'goal':
                this.handleGoalCommand(args);
                return true;
            case 'superf':
                this.handleSuperFriend(args);
                return true;
            case 'psc':
                this.proxy.hypixel.handlePartyStatCheck(args[0]);
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
            case 'drpc':
                this.handleDrpcCommand();
                return true;
            case 'link':
                this.handleLinkCommand();
                return true;
            default:
                return false;
        }
    }

    handleQuickQueue(args) {
        const mode = args[0]?.toLowerCase();
        if (!mode) {
            this.proxy.proxyChat("§cUsage: /q <mode>");
            this.proxy.proxyChat("§eUse /q ? to see all available modes.");
            return;
        }

        if (mode === '?') {
            let helpMessage = "§d§m----------------------------------------------------\n";
            helpMessage += "§r  §d§lAvailable Quick Queue Commands (/q)\n \n";
            for (const alias in quickQueueMap) {
                const modeInfo = quickQueueMap[alias];
                helpMessage += `§r  §e${alias} §8- §b${modeInfo.name}\n`;
            }
            helpMessage += "\n§d§m----------------------------------------------------";
            this.proxy.proxyChat(helpMessage);
            return;
        }

        const queue = quickQueueMap[mode];
        if (queue) {
            this.proxy.proxyChat(`§eJoining ${queue.name}...`);
            this.proxy.target.write('chat', { message: queue.command });
            if (queue.command.toLowerCase().startsWith('/play ')) {
                this.proxy.lastPlayCommand = queue.command;
            }
        } else {
            this.proxy.proxyChat(`§cUnknown mode '${mode}'. Use /q ? to see available modes.`);
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
            { syntax: '/q <mode>', desc: 'Quickly joins a game mode. Use /q ? for a list.' },
            { syntax: '/psc [game]', desc: 'Runs a stat check for all party members.' },
            { syntax: '/rq', desc: 'Re-queues your last played game.' },
            { syntax: '/alert <add|rem|list> [player]', desc: 'Manages in-game alerts for players.' },
            { syntax: '/nickname <add|rem|list> [args]', desc: 'Sets local nicknames for players.' },
            { syntax: '/superf <add|rem|list> [args]', desc: "Tracks friends' game activity." },
            { syntax: '/drpc', desc: 'Toggles the Discord Rich Presence.' },
            { syntax: '/link', desc: 'Generates a link to connect your Minecraft account with your JagProx account.' },
            { syntax: '/jagprox', desc: 'Displays this help message.' }
        ];

        let helpMessage = "§d§m----------------------------------------------------\n";
        helpMessage += "§r  §d§lJagProx §8- §7Available Commands\n \n";

        commandList.sort((a,b) => a.syntax.localeCompare(b.syntax)).forEach(c => {
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

    handleDrpcCommand() {
        const drpcConfig = this.proxy.config.discord_rpc || { enabled: true };
        drpcConfig.enabled = !drpcConfig.enabled;
        this.proxy.config.discord_rpc = drpcConfig;

        if (drpcConfig.enabled) {
            discordRpc.login();
            this.proxy.proxyChat("§aDiscord RPC has been enabled.");
        } else {
            discordRpc.logout();
            this.proxy.proxyChat("§cDiscord RPC has been disabled.");
        }
        this.saveConfig();
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
            fileConfig.discord_rpc = this.proxy.config.discord_rpc;
            fs.writeFileSync(configPath, yaml.stringify(fileConfig), 'utf8');
        } catch (e) {
            this.proxy.proxyChat("§cError saving configuration to file.");
            console.error("Config save error:", e);
        }
    }

    async handleLinkCommand() {
        const mc_uuid = this.proxy.client.uuid;
        const mc_username = this.proxy.client.username;

        if (!mc_uuid || !mc_username) {
            this.proxy.proxyChat("§cCould not retrieve your Minecraft UUID or username. Please ensure you are logged in.");
            return;
        }

        this.proxy.proxyChat("§eGenerating account linking code...");

        const apiUrl = `${API_BASE_URL}/generate-link-code`;
        console.log(`Attempting to generate link code from: ${apiUrl}`);

        try {
            const response = await fetch(apiUrl, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ mc_uuid, mc_username })
            });

            const contentType = response.headers.get('content-type');
            if (!contentType || !contentType.includes('application/json')) {
                const errorText = await response.text();
                console.error('Server responded with non-JSON content:', errorText);
                this.proxy.proxyChat(`§cError: Server did not respond with JSON. Status: ${response.status}. Response: ${errorText.substring(0, 100)}...`);
                return;
            }

            const data = await response.json();

            if (response.ok) {
                const linkUrl = data.link_url || `${WEB_LINK_BASE_URL}/link.html?code=${data.code}`;
                this.proxy.proxyChat(`§aYour account linking URL: §b${linkUrl}`);
                this.proxy.proxyChat("§ePlease open this URL in your browser to complete the linking process.");
            } else {
                this.proxy.proxyChat(`§cError generating link: ${data.message || 'Unknown error.'}`);
            }
        } catch (error) {
            console.error('Error generating link code:', error);
            this.proxy.proxyChat("§cNetwork error or issue connecting to the JagProx authentication server.");
        }
    }
}

module.exports = CommandHandler;