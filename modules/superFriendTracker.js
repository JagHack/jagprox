const formatter = require('../formatter.js');
const { gameModeMap } = require('../utils/constants.js');

const POLL_INTERVAL_MS = 60 * 1000;
const FIRST_POLL_DELAY_MS = 10 * 1000;

// Hypixel status gameType for a /superf gamemode argument ("bedwars", "sw", "duels", ...).
// Unknown arguments are compared against the gameType directly, so "SURVIVAL_GAMES" also works.
function gameTypeFor(mode) {
    const info = gameModeMap[String(mode).toLowerCase()];
    return (info ? info.apiName : String(mode)).toUpperCase();
}

function matchesTrackedModes(session, modes) {
    if (!Array.isArray(modes) || modes.length === 0) return false;
    return modes.some(mode => {
        const lower = String(mode).toLowerCase();
        return lower === 'any' || lower === 'all' || gameTypeFor(mode) === String(session.gameType).toUpperCase();
    });
}

// Polls the Hypixel status of everyone in config.super_friends once a minute while
// a player is connected, and announces when one of them enters a tracked game.
class SuperFriendTracker {
    constructor(proxy) {
        this.proxy = proxy;
        this.interval = null;
        this.firstPoll = null;
        this.polling = false;
        this.lastSeen = new Map();
    }

    start() {
        if (this.interval) return;
        this.firstPoll = setTimeout(() => this.poll(), FIRST_POLL_DELAY_MS);
        this.interval = setInterval(() => this.poll(), POLL_INTERVAL_MS);
    }

    stop() {
        clearTimeout(this.firstPoll);
        clearInterval(this.interval);
        this.firstPoll = null;
        this.interval = null;
        this.lastSeen.clear();
    }

    async poll() {
        if (this.polling || !this.proxy.client) return;
        this.polling = true;
        try {
            const friends = this.proxy.config.super_friends || {};
            for (const [name, modes] of Object.entries(friends)) {
                if (!this.proxy.client) break;
                await this.checkFriend(name, modes);
            }
        } catch (e) {
            formatter.log(`Super friend poll failed: ${e.message}`);
        } finally {
            this.polling = false;
        }
    }

    async checkFriend(name, modes) {
        const mojangData = await this.proxy.hypixel.getMojangUUID(name);
        if (!mojangData) return;
        const session = await this.proxy.hypixel.getSessionStatus(mojangData.uuid);
        if (!session) return;

        const key = session.online ? `${session.gameType}|${session.mode}|${session.map}` : 'offline';
        const previous = this.lastSeen.get(name);
        this.lastSeen.set(name, key);

        // The first poll only records a baseline; announce changes after that.
        if (previous === undefined || previous === key || !session.online) return;
        if (!matchesTrackedModes(session, modes)) return;

        const details = [session.mode, session.map].filter(Boolean).join(' §8- §f');
        this.proxy.proxyChat(`§dSuper friend §5${mojangData.username} §djoined §f${session.gameType}${details ? ` §8(§f${details}§8)` : ''}`);
    }
}

module.exports = SuperFriendTracker;
module.exports.matchesTrackedModes = matchesTrackedModes;
