const fs = require('fs');
const path = require('path');
const yaml = require('yaml');
const fetch = require('node-fetch');
const aliasManager = require('../aliasManager.js');
const formatter = require('../formatter.js');
const { gameModeMap, quickQueueMap, duelsPlayerCountMap, duelsStatMap, duelsDivisions, romanNumerals, duelsTitleColors } = require('../utils/constants.js');
const { getStatValue, statAliases } = require('../utils/stat-helper.js');
const discordRpc = require('./discordRpcHandler.js');
const { API_BASE_URL, WEB_LINK_BASE_URL } = require('../utils/api_constants.js');

class CommandHandler {
    constructor(proxy) {
        this.proxy = proxy;
    }

    async handle(message) {
        const aliases = aliasManager.getAliases();
        const aliasedCommand = aliases[message.toLowerCase()];

        if (aliasedCommand) {
            if (aliasedCommand.toLowerCase().startsWith('/play ')) {
                this.proxy.lastPlayCommand = aliasedCommand;
                formatter.log(`Captured last play command (from alias): ${this.proxy.lastPlayCommand}`);
            }
            this.proxy.proxyChat(`§dAlias §8» §f${aliasedCommand}`);
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
                this.proxy.hypixel.statcheck(gamemode, username); 

                (async () => { 
                    try {
                        const gameInfo = gameModeMap[gamemode.toLowerCase()];
                        if (!gameInfo) {
                            
                            return;
                        }

                        const cleanUsername = this.proxy.hypixel.cleanRankPrefix(username);
                        const mojangData = await this.proxy.hypixel.getMojangUUID(cleanUsername);
                        if (!mojangData) {
                            
                            return;
                        }

                        const stats = await this.proxy.hypixel.getStats(mojangData.uuid);
                        if (!stats || !stats.player.stats || !stats.player.stats[gameInfo.apiName]) {
                            
                            return;
                        }

                        const win_count = this.getWinCount(stats, gameInfo);

                        const backendApiUrl = 'https://jagprox.jaghack.com';
                        if (!backendApiUrl) {
                            console.warn("backend_api_url is not configured in config.yml. Skipping telemetry dispatch.");
                            return;
                        }

                        const ingestUrl = `${backendApiUrl}/api/ingest`;
                        const payload = {
                            username: mojangData.username,
                            mode: gameInfo.displayName, 
                            win_count: win_count
                        };

                        const response = await fetch(ingestUrl, {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify(payload)
                        });

                        if (!response.ok) {
                            const errorText = await response.text();
                            console.error(`Telemetry dispatch failed: ${response.status} - ${errorText}`);
                        } else {
                            console.log(`Telemetry dispatched for ${mojangData.username} (${gameInfo.displayName}): ${win_count} wins`);
                        }
                    } catch (error) {
                        
                        console.error("Error dispatching telemetry:", error);
                    }
                })();
                return true;
            }
            case 'jtitles':
                this.handleJTitlesCommand(args);
                return true;
            case 'playercount':
                this.handlePlayerCountCommand(args);
                return true;
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
            case 'spread':
                this.handleSpreadCommand();
                return true;
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
                    this.proxy.proxyChat(`§dRe-queuing §8» §f${this.proxy.lastPlayCommand}`);
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
            case 'gametrack':
            case 'gt':
                this.handleGametrackCommand(args);
                return true;
            case 'leaderboard': {
                const gameMode = args.shift()?.toLowerCase();
                const statType = args.join(' ').toLowerCase(); 

                if (gameMode !== 'duels') {
                    this.proxy.proxyChat("§cCurrently, leaderboards are only available for 'duels'.");
                    this.proxy.proxyChat("§cUsage: /leaderboard duels <monthly wins|weekly wins>");
                    return true;
                }

                let leaderboardType = '';
                if (statType === 'monthly wins') {
                    leaderboardType = 'Monthly Wins';
                } else if (statType === 'weekly wins') {
                    leaderboardType = 'Weekly Wins';
                } else {
                    this.proxy.proxyChat("§cInvalid leaderboard type. Supported types for Duels: 'monthly wins', 'weekly wins'.");
                    this.proxy.proxyChat("§cUsage: /leaderboard duels <monthly wins|weekly wins>");
                    return true;
                }

                this.proxy.proxyChat(`§dFetching §f${leaderboardType} §dleaderboard for §5Duels§d...`);
                this.proxy.hypixel.getLeaderboard('DUELS', leaderboardType).then(result => {
                    if (result.error) {
                        this.proxy.proxyChat(`§cError: ${result.error}`);
                    } else {
                        let message = `§5§m----------------------------------------------------\n`;
                        message += `  §5§l${result.title}\n \n`;
                        if (result.leaders.length === 0) {
                            message += `    §8No leaders found for this category.`;
                        } else {
                            result.leaders.slice(0, 10).forEach((player, index) => {
                                message += `  §d${index + 1}. §f${player}\n`;
                            });
                        }
                        message += `\n§5§m----------------------------------------------------`;
                        this.proxy.proxyChat(message);
                    }
                });
                return true;
            }
            default:
                return false;
        }
    }

    async handleJTitlesCommand(args) {
        let username = args[0];
        let uuid = this.proxy.client.uuid;
        let displayName = this.proxy.client.username;

        if (username) {
            this.proxy.proxyChat(`§eFetching titles for §d${username}§e...`);
            const resolvedName = this.proxy.hypixel.resolveNickname(username);
            const mojangData = await this.proxy.hypixel.getMojangUUID(resolvedName);
            if (!mojangData) return this.proxy.proxyChat("§cInvalid playername!");
            uuid = mojangData.uuid;
            displayName = mojangData.username;
        }

        if (!uuid) return this.proxy.proxyChat("§cInvalid playername!");

        const stats = await this.proxy.hypixel.getStats(uuid);
        if (!stats || !stats.player || !stats.player.stats || !stats.player.stats.Duels) {
            return this.proxy.proxyChat(`§cCould not fetch Duels stats for ${displayName}.`);
        }

        const d = stats.player.stats.Duels;
        const activeTitles = [];
        let maxModeLen = 0;

        for (const [modeDisplayName, prefix] of Object.entries(duelsStatMap)) {
            const wins = d[`${prefix}_wins`] || 0;
            if (wins >= 50) {
                let bestRank = "No Title";
                let bestDivision = "";
                let bestColor = "§7";

                for (const [rank, data] of Object.entries(duelsDivisions)) {
                    if (wins >= data.wins) {
                        bestRank = rank.charAt(0).toUpperCase() + rank.slice(1);
                        bestColor = duelsTitleColors[rank] || "§7";
                        
                        const winsInRank = wins - data.wins;
                        const divisionIndex = Math.floor(winsInRank / data.step);
                        const actualDivision = Math.min(divisionIndex, data.levels - 1);
                        bestDivision = romanNumerals[actualDivision];
                    }
                }

                if (bestRank !== "No Title") {
                    const formattedTitle = `${bestColor}${bestRank} ${bestDivision}`;
                    activeTitles.push({
                        name: modeDisplayName,
                        title: formattedTitle
                    });
                    if (modeDisplayName.length > maxModeLen) maxModeLen = modeDisplayName.length;
                }
            }
        }

        if (activeTitles.length === 0) {
            return this.proxy.proxyChat(`§c${displayName} doesn't have any Duels titles yet (minimum 50 wins in a mode).`);
        }

        this.proxy.proxyChat("§5§m----------------------------------------------------");
        this.proxy.proxyChat(`  §5§lDuels Mode Titles for ${displayName}`);

        for (const entry of activeTitles) {
            const paddedName = entry.name.padEnd(maxModeLen, ' ');
            this.proxy.proxyChat(`  §5${paddedName} §8: ${entry.title}`);
        }

        this.proxy.proxyChat("§5§m----------------------------------------------------");
    }

    async handleSpreadCommand() {
        const uuid = this.proxy.client.uuid;
        if (!uuid) return this.proxy.proxyChat("§cInvalid playername!");

        const stats = await this.proxy.hypixel.getStats(uuid);
        if (!stats || !stats.player || !stats.player.stats || !stats.player.stats.Duels) {
            return this.proxy.proxyChat("§cCould not fetch your Duels stats.");
        }

        const d = stats.player.stats.Duels;
        const activeModes = [];
        let maxModeLen = 0;
        let maxWinsLen = 0;

        for (const [displayName, prefix] of Object.entries(duelsStatMap)) {
            const wins = d[`${prefix}_wins`] || 0;
            const losses = d[`${prefix}_losses`] || 0;
            if (wins > 0 || losses > 0) {
                const winsStr = wins.toLocaleString();
                activeModes.push({
                    name: displayName,
                    wins: winsStr,
                    losses: losses.toLocaleString()
                });
                if (displayName.length > maxModeLen) maxModeLen = displayName.length;
                if (winsStr.length > maxWinsLen) maxWinsLen = winsStr.length;
            }
        }

        if (activeModes.length === 0) {
            return this.proxy.proxyChat("§cNo Duels stats found for any mode.");
        }

        this.proxy.proxyChat("§5§m----------------------------------------------------");
        this.proxy.proxyChat("  §5§lDuels Mode Spread");

        for (const mode of activeModes) {
            const paddedName = mode.name.padEnd(maxModeLen, ' ');
            const paddedWins = mode.wins.padStart(maxWinsLen, ' ');
            this.proxy.proxyChat(`  §5${paddedName} §8: §a${paddedWins} §8| §4${mode.losses}`);
        }

        this.proxy.proxyChat("§5§m----------------------------------------------------");
    }

    async handlePlayerCountCommand(args) {
        const queryMode = args.join(' ').toLowerCase();

        if (!queryMode || queryMode === '?') {
            let helpMessage = "§5§m----------------------------------------------------\n";
            helpMessage += "§r  §5§lAvailable Duels Modes for /playercount\n \n";
            
            const uniqueModes = [...new Set(Object.values(duelsPlayerCountMap))];
            const displayNames = {};
            
            for (const [alias, apiKey] of Object.entries(duelsPlayerCountMap)) {
                if (!displayNames[apiKey] || alias.length > displayNames[apiKey].length) {
                    displayNames[apiKey] = alias;
                }
            }

            const sortedKeys = Object.keys(displayNames).sort((a, b) => displayNames[a].localeCompare(displayNames[b]));
            
            sortedKeys.forEach(apiKey => {
                helpMessage += `§r  §d${displayNames[apiKey].split(' ').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ')}\n`;
            });

            helpMessage += "\n§5§m----------------------------------------------------";
            this.proxy.proxyChat(helpMessage);
            return;
        }

        const apiKeyMap = duelsPlayerCountMap[queryMode];
        if (!apiKeyMap) {
            this.proxy.proxyChat(`§cUnknown duels mode: ${queryMode}`);
            this.proxy.proxyChat("§eUse /playercount ? to see all modes.");
            return;
        }

        const games = await this.proxy.hypixel.getPlayerCounts();
        
        if (!games || !games.DUELS || !games.DUELS.modes) {
            this.proxy.proxyChat("§cCould not retrieve player counts from Hypixel.");
            return;
        }

        const count = games.DUELS.modes[apiKeyMap] || 0;
        this.proxy.proxyChat(`§5Duels §8» §f${count.toLocaleString()} Players §8(§d${queryMode}§8)`);
    }

    async handleGametrackCommand(args) {
        const subCommand = args[0] ? args[0].toLowerCase() : 'hour';
        const gametrackApiHandler = this.proxy.gametrackApiHandler;

        const sendLine = () => this.proxy.proxyChat("§5§m----------------------------------------------------");

        try {
            switch (subCommand) {
                case 'hour': {
                    const hours = args[1] ? parseInt(args[1], 10) : 1;
                    if (isNaN(hours) || hours <= 0) {
                        return this.proxy.proxyChat("§cInvalid number of hours.");
                    }
                    
                    const data = await gametrackApiHandler.getStats('hour', hours);
                    const playerData = data[this.proxy.mc_uuid];

                    sendLine();
                    this.proxy.proxyChat(`  §5§lGame Stats for the Last ${hours} Hour(s)`);
                    if (!playerData || Object.keys(playerData).length === 0) {
                        this.proxy.proxyChat("    §8No game data found for your account in this period.");
                    } else {
                        for (const [mode, stats] of Object.entries(playerData)) {
                            const formattedMode = mode.charAt(0).toUpperCase() + mode.slice(1);
                            const wlr = stats.losses === 0
                                ? (stats.wins > 0 ? 'Infinite' : 'N/A')
                                : (stats.wins / stats.losses).toFixed(2);
                            const wlrString = `§8| §d${wlr} WLR`;

                            this.proxy.proxyChat(`  §d${formattedMode} §8- §f${stats.wins} Wins §8| §f${stats.losses} Losses ${wlrString}`);
                        }
                    }
                    sendLine();
                    break;
                }
                
                case 'day': {
                    const data = await gametrackApiHandler.getStats('day');
                    const playerData = data[this.proxy.mc_uuid];
                    sendLine();
                    this.proxy.proxyChat("  §5§lGame Stats For Today");
                     if (!playerData || Object.keys(playerData).length === 0) {
                        this.proxy.proxyChat("    §8No game data found for your account today.");
                    } else {
                        for (const [mode, stats] of Object.entries(playerData)) {
                            const formattedMode = mode.charAt(0).toUpperCase() + mode.slice(1);
                            const wlr = stats.losses === 0
                                ? (stats.wins > 0 ? 'Infinite' : 'N/A')
                                : (stats.wins / stats.losses).toFixed(2);
                            const wlrString = `§8| §d${wlr} WLR`;
                            this.proxy.proxyChat(`  §d${formattedMode} §8- §f${stats.wins} Wins §8| §f${stats.losses} Losses ${wlrString}`);
                        }
                    }
                    sendLine();
                    break;
                }
                
                case 'log': {
                    const logData = await gametrackApiHandler.getStats('log');
                    sendLine();
                    this.proxy.proxyChat("  §5§lRecent Game Log");
                    if (!logData || logData.length === 0) {
                        this.proxy.proxyChat("    §8No recent games found.");
                    } else {
                        logData.slice(0, 10).forEach(entry => {
                            const resultColor = entry.result === 'win' ? '§d' : '§8';
                            const timestamp = new Date(entry.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
                            const formattedMode = entry.mode.charAt(0).toUpperCase() + entry.mode.slice(1);
                            this.proxy.proxyChat(`  §8[${timestamp}] §d${formattedMode} §8» ${resultColor}${entry.result.toUpperCase()}`);
                        });
                    }
                    sendLine();
                    break;
                }
                
                default:
                    this.proxy.proxyChat("§cInvalid /gametrack command. Use: hour, day, or log.");
                    break;
            }
        } catch (e) {
            this.proxy.proxyChat(`§c[GameTrack] Error: ${e.message}`);
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
            let helpMessage = "§5§m----------------------------------------------------\n";
            helpMessage += "§r  §5§lAvailable Quick Queue Commands (/q)\n \n";
            for (const alias in quickQueueMap) {
                const modeInfo = quickQueueMap[alias];
                helpMessage += `§r  §d${alias} §8- §f${modeInfo.name}\n`;
            }
            helpMessage += "\n§5§m----------------------------------------------------";
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
                if (!uuid) return this.proxy.proxyChat("§cInvalid playername! Please relog.");
    
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
                if (!uuid) return this.proxy.proxyChat("§cInvalid playername! Please relog.");
                
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
                const bar = `§d${'█'.repeat(filledLength)}§8${'█'.repeat(progressBarLength - filledLength)}`;
    
                this.proxy.proxyChat(`§5§m----------------------------------------------------`);
                this.proxy.proxyChat(`  §5§lGoal: ${goal.name} in ${goal.gamemode}`);
                this.proxy.proxyChat(`  §7${goal.initial.toLocaleString()} §8» §f${goal.target.toLocaleString()}`);
                this.proxy.proxyChat(` `);
                this.proxy.proxyChat(`  §fProgress: ${bar} §d${percentage.toFixed(2)}%`);
                this.proxy.proxyChat(`  §dCurrent: §f${currentValue.toLocaleString()} §8(Remaining: §f${remaining.toLocaleString()}§8)`);
                this.proxy.proxyChat(`§5§m----------------------------------------------------`);
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

        let helpMessage = "§5§m----------------------------------------------------\n";
        helpMessage += "§r  §5§lAvailable Statcheck Gamemodes\n \n";

        const sortedCategories = Object.keys(modesByCategory).sort();

        for (const category of sortedCategories) {
            const aliases = modesByCategory[category].join(', ');
            helpMessage += `§r  §d${category} §8- §f${aliases}\n`;
        }
        
        helpMessage += "\n§5§m----------------------------------------------------";
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
            { syntax: '/playercount <mode>', desc: 'Checks how many players are queuing in a mode. Use /playercount ? for list.' },
            { syntax: '/spread', desc: 'Shows your Duels wins and losses spread for all modes.' },
            { syntax: '/jtitles [player]', desc: 'Shows Duels mode-specific titles for you or another player.' },
            { syntax: '/leaderboard <game> <type>', desc: 'Displays top players for a game leaderboard (e.g., duels monthly wins|weekly wins).' },
            { syntax: '/link', desc: 'Generates a link to connect your Minecraft account with your JagProx account.' },
            { syntax: '/jagprox', desc: 'Displays this help message.' }
        ];

        let helpMessage = "§5§m----------------------------------------------------\n";
        helpMessage += "§r  §5§lJagProx §8- §7Available Commands\n \n";

        commandList.sort((a,b) => a.syntax.localeCompare(b.syntax)).forEach(c => {
            const parts = c.syntax.split(' ');
            const cmd = parts.shift();
            const args = parts.join(' ');
            const coloredSyntax = `§5${cmd} §d${args}`;

            helpMessage += `§r  ${coloredSyntax}\n`;
            helpMessage += `§r    §8- §7${c.desc}\n \n`;
        });

        helpMessage = helpMessage.trimEnd();
        helpMessage += "\n§r\n§5§m----------------------------------------------------";

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
                    this.proxy.proxyChat("§dYou have no nicknames set.");
                } else {
                    this.proxy.proxyChat("§5Your nicknames:");
                    keys.forEach(realName => {
                        this.proxy.proxyChat(`§8- §d${realName} §8» §f${nicknames[realName]}`);
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
                this.proxy.proxyChat("§dYour alert list is empty.");
            } else {
                this.proxy.proxyChat("§5Players on your alert list:");
                alertList.forEach(name => this.proxy.proxyChat(`§8- §d${name}`));
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
                this.proxy.proxyChat("§dYou have no super friends set.");
            } else {
                this.proxy.proxyChat("§5Your super friends:");
                keys.forEach(name => {
                    this.proxy.proxyChat(`§8- §d${name} §7(${friends[name].join(', ')})`);
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

    getWinCount(stats, gameInfo) {
        const d = stats.player.stats[gameInfo.apiName] || {};
        switch (gameInfo.apiName) {
            case "Bedwars":
                return d.wins_bedwars || 0;
            case "SkyWars":
                return d.wins || 0;
            case "Duels":
                const prefix = gameInfo.prefix || '';
                const winsKey = prefix ? `${prefix}_wins` : 'wins';
                return d[winsKey] || 0;
            case "Walls3":
                return d.wins || 0;
            case "Quake":
                return d.wins || 0;
            case "HungerGames":
                return d.wins || 0;
            case "UHC":
                return d.wins || 0;
            case "MurderMystery":
                return d.wins || 0;
            case "BuildBattle":
                return d.wins || 0;
            case "WoolGames":
                const ww = d.wool_wars || {};
                const wwStats = ww.stats || {};
                return wwStats.wins || 0;
            default:
                return d.wins || 0;
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

        this.proxy.proxyChat("§dGenerating account linking code...");

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
                this.proxy.proxyChat("§5Link §8» §fClick here to connect your account", {
                    action: 'open_url',
                    value: linkUrl
                });
                this.proxy.proxyChat("§dAlternatively, you can manually open this URL in your browser:");
                this.proxy.proxyChat(`§f${linkUrl}`);
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
