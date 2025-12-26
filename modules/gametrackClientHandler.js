const formatter = require('../formatter.js');
const { gameModeMap } = require('../utils/constants.js');
const api = require('./gametrackApiHandler.js');

class GametrackClientHandler {
    constructor(proxy) {
        this.proxy = proxy;
        this.activeSession = null; // { mode: 'gamemode' }
    }

    // A check to ensure the user is logged in via the launcher.
    isAuthenticated() {
        if (!process.env.JAGPROX_JWT) {
            this.proxy.proxyChat("§cThis command requires you to be logged into the JagProx launcher.");
            return false;
        }
        if (!this.proxy.mc_uuid) {
            this.proxy.proxyChat("§cCould not identify your Minecraft account. Please try relogging to the proxy.");
            return false;
        }
        return true;
    }

    handle(message) {
        const args = message.slice('/gametrack '.length).split(' ');
        const subCommand = args.shift().toLowerCase();

        // Make the methods async so they can await API calls
        (async () => {
            switch (subCommand) {
                case 'start':
                    await this.handleStart(args);
                    break;
                case 'stop':
                    await this.handleStop();
                    break;
                case 'stats':
                    await this.handleStats(args);
                    break;
                case 'help':
                    this.showHelp();
                    break;
                default:
                    this.proxy.proxyChat("§cUnknown /gametrack command. Use '/gametrack help' for a list of commands.");
                    break;
            }
        })();
        
        return true; // Command was handled
    }

    async handleStart(args) {
        if (!this.isAuthenticated()) return;
        const modeArg = args[0]?.toLowerCase();
        if (!modeArg) {
            this.proxy.proxyChat("§cUsage: /gametrack start <gamemode>");
            this.proxy.proxyChat("§eUse /sc ? to see a list of valid gamemodes.");
            return;
        }

        const gameModeInfo = Object.values(gameModeMap).find(g => g.key.toLowerCase() === modeArg || g.displayName.toLowerCase() === modeArg);
        const mode = gameModeInfo ? gameModeInfo.key : modeArg.toUpperCase();

        if (this.activeSession) {
            this.proxy.proxyChat(`§cAlready tracking a session for §e${this.activeSession.mode}§c. Use '/gametrack stop' first.`);
            return;
        }

        const result = await api.startTracking(this.proxy.mc_uuid, mode);
        if (result.success) {
            this.activeSession = { mode };
            this.proxy.proxyChat(`§aStarted tracking game session for: §e${this.activeSession.mode}`);
        } else {
            this.proxy.proxyChat(`§cError starting session: ${result.message}`);
        }
    }

    async handleStop() {
        if (!this.isAuthenticated()) return;
        if (!this.activeSession) {
            this.proxy.proxyChat("§cNo active game session to stop.");
            return;
        }
        const stoppedMode = this.activeSession.mode;
        const result = await api.stopTracking(this.proxy.mc_uuid, stoppedMode);

        if (result.success) {
            this.proxy.proxyChat(`§aStopped tracking game session for: §e${stoppedMode}`);
            this.activeSession = null;
        } else {
            this.proxy.proxyChat(`§cError stopping session: ${result.message}`);
        }
    }

    async handleStats(args) {
        if (!this.isAuthenticated()) return;
        const type = args[0]?.toLowerCase() || 'day'; // Default to 'day'
        
        this.proxy.proxyChat("§eFetching gametrack stats...");
        let result;
        let timePeriod = "today's";

        switch (type) {
            case 'day':
                result = await api.fetchDailyAggregates();
                break;
            case 'hour':
                const hours = parseInt(args[1], 10) || 1;
                timePeriod = `the last ${hours} hour(s)`;
                result = await api.fetchHourlyAggregates(hours);
                break;
            case 'log':
                result = await api.fetchFullLog();
                this.displayLog(result);
                return; // displayLog has its own formatting
            default:
                this.proxy.proxyChat("§cInvalid stats type. Use 'day', 'hour [number]', or 'log'.");
                return;
        }
        this.displayStats(result, timePeriod);
    }
    
    async recordEvent(result) {
        if (!this.activeSession) return;
        if (!this.isAuthenticated()) return;

        formatter.log(`Gametrack: Attempting to record '${result}' for ${this.activeSession.mode}`);
        const apiResult = await api.pushGametrackEvent(this.proxy.mc_uuid, this.activeSession.mode, result);
        
        if (apiResult.success) {
            this.proxy.proxyChat(`§7(Gametrack: Recorded §${result === 'win' ? 'a' : 'c'}${result}§7 for ${this.activeSession.mode})`);
        } else {
            this.proxy.proxyChat(`§c(Gametrack Error: ${apiResult.message})`);
            formatter.log(`Gametrack API Error on event push: ${apiResult.message}`);
        }
    }

    displayStats(result, timePeriod) {
        if (!result.success) {
            this.proxy.proxyChat(`§cCould not fetch stats: ${result.message}`);
            return;
        }
        this.proxy.proxyChat(`§d§m----------------------------------------------------`);
        this.proxy.proxyChat(`  §d§lGametrack Stats §8- §7${timePeriod}`);
        this.proxy.proxyChat(" ");

        const stats = result.data.stats; // API returns { "stats": { "MODE": ... } }
        if (!stats || Object.keys(stats).length === 0) {
            this.proxy.proxyChat("  §eNo game data found for this period.");
            this.proxy.proxyChat(`§d§m----------------------------------------------------`);
            return;
        }

        for (const [mode, data] of Object.entries(stats)) {
            const wins = data.wins || 0;
            const losses = data.losses || 0;
            const total = wins + losses;
            const wlr = losses > 0 ? (wins / losses).toFixed(2) : '∞';
            this.proxy.proxyChat(`  §e${mode}: §a${wins}W §c${losses}L §7- Total: ${total}, WLR: ${wlr}`);
        }
        this.proxy.proxyChat(`§d§m----------------------------------------------------`);
    }

    displayLog(result) {
        if (!result.success) {
            this.proxy.proxyChat(`§cCould not fetch log: ${result.message}`);
            return;
        }
        this.proxy.proxyChat(`§d§m----------------------------------------------------`);
        this.proxy.proxyChat(`  §d§lGametrack Log §8- §7Newest First`);
        this.proxy.proxyChat(" ");

        const log = result.data.events;
        if (!log || log.length === 0) {
            this.proxy.proxyChat("  §eNo game data found.");
            this.proxy.proxyChat(`§d§m----------------------------------------------------`);
            return;
        }

        // Display up to a reasonable number of recent logs in chat
        log.slice(0, 15).forEach(entry => {
            const timestamp = new Date(entry.timestamp).toLocaleTimeString();
            const resultText = entry.result === 'win' ? '§aWin' : '§cLoss';
            this.proxy.proxyChat(`  §7[${timestamp}] §e${entry.mode}: ${resultText}`);
        });
        if (log.length > 15) {
            this.proxy.proxyChat("  §7...and more.");
        }
        this.proxy.proxyChat(`§d§m----------------------------------------------------`);
    }

    showHelp() {
        const help = [
            "§d§m----------------------------------------------------",
            "  §d§lGametrack §8- §7Game Session Tracking",
            " ",
            "  §d/gametrack start <gamemode> §8-§r §7Starts tracking a new session.",
            "  §d/gametrack stop §8-§r §7Stops the current session.",
            "  §d/gametrack stats [day|hour|log] §8-§r §7Shows session stats.",
            "  §d/gametrack help §8-§r §7Shows this help message.",
            " ",
            "§d§m----------------------------------------------------",
        ];
        help.forEach(line => this.proxy.proxyChat(line));
    }
}

module.exports = GametrackClientHandler;

