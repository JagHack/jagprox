const formatter = require("../formatter.js");
const { gameModeMap, GAME_START_MESSAGES } = require('../utils/constants.js');
const { parseOnlineLine, parseTeamLine } = require('../utils/chatParsers.js');

const GAME_OVER_KEYWORDS = ['VICTORY!', 'GAME END', 'You died!', 'You have been eliminated!', 'You won!', 'Draw!'];
const WHO_CAPTURE_TIMEOUT_MS = 5000;

class QueueStatsHandler {
    constructor(proxy) {
        this.proxy = proxy;
        this.currentGameKey = null;
        this.hasTriggeredForGame = false;
        this.awaitingTeleportForWho = false;

        this.currentMapName = null;

        this.isCapturingWho = false;
        this.whoPlayers = [];
        this.whoTimeout = null;

        // Scoreboard title -> game key. When several keys share a display name the last
        // one wins (e.g. "BED WARS" -> "bw", "SKYWARS" -> "sw"); gametrack modes depend on these keys.
        this.titleToKeyMap = new Map();
        for (const [key, modeInfo] of Object.entries(gameModeMap)) {
            if (modeInfo && modeInfo.displayName) {
                this.titleToKeyMap.set(modeInfo.displayName.toUpperCase(), key);
            }
        }
    }

    reset() {
        formatter.debug('Queue Stats Handler reset.');
        this.currentGameKey = null;
        this.hasTriggeredForGame = false;
        this.awaitingTeleportForWho = false;
        this.stopWhoCapture();
        this.currentMapName = null;
    }

    resetTrigger() {
        if (this.hasTriggeredForGame) {
            formatter.log(`Game over detected. Re-arming queue stats trigger for the next game.`);
            this.hasTriggeredForGame = false;
            this.awaitingTeleportForWho = false;
            this.currentMapName = null;
        }
    }

    resetForNewGame(newGameKey) {
        formatter.log(`Game context set to: "${newGameKey}".`);
        this.currentGameKey = newGameKey;
        this.hasTriggeredForGame = false;
        this.awaitingTeleportForWho = false;
        this.currentMapName = null;
        this.proxy.onGameChanged(newGameKey);
    }

    // queue_stats can be keyed by any alias of the game ("skywars" or "sw", "bedwars" or "bw").
    isEnabledFor(gameKey) {
        const queueStats = this.proxy.config.queue_stats || {};
        const info = gameModeMap[gameKey];
        if (!info) return false;
        return Object.entries(gameModeMap).some(([key, modeInfo]) =>
            modeInfo.displayName === info.displayName && queueStats[key]);
    }

    isDuelsMode(gameKey) {
        return !!gameKey && gameModeMap[gameKey]?.apiName === 'Duels';
    }

    handlePacket(data, meta) {
        if (meta.name === 'kick_disconnect' || meta.name === 'disconnect') {
            this.currentGameKey = null;
            this.hasTriggeredForGame = false;
            this.awaitingTeleportForWho = false;
            this.stopWhoCapture();
            return;
        }

        if (meta.name === 'position' && this.awaitingTeleportForWho) {
            this.awaitingTeleportForWho = false;

            // /who is only useful for Bed Wars and SkyWars; Duels uses the "Opponent:" line instead.
            if (this.isDuelsMode(this.currentGameKey)) {
                formatter.log("Skipping /who for Duels mode.");
                this.hasTriggeredForGame = true;
            } else {
                formatter.log("Teleport confirmed. Executing /who after a short safety delay.");
                setTimeout(() => {
                    if (!this.proxy.target) return;
                    this.startWhoCapture();
                    this.proxy.target.write('chat', { message: '/who' });
                }, 500);
            }
        }

        if (meta.name === 'scoreboard_objective' && data.action === 0 && data.displayText) {
            const newTitle = formatter.stripColors(data.displayText).trim().toUpperCase();
            const foundGameKey = this.titleToKeyMap.get(newTitle);
            if (foundGameKey && this.currentGameKey !== foundGameKey) {
                this.resetForNewGame(foundGameKey);
            }
            if (foundGameKey) {
                this.hasTriggeredForGame = false;
            }
        }

        if (meta.name === 'chat') {
            this.handleChat(data);
        }
    }

    handleChat(data) {
        const cleanMessage = formatter.chatToCleanText(data.message);
        if (cleanMessage === null) return;

        if (GAME_OVER_KEYWORDS.some(keyword => cleanMessage.includes(keyword))) {
            this.resetTrigger();
        }

        const mapMatch = cleanMessage.match(/You are currently playing on (?:map: )?(.+)/);
        if (mapMatch) {
            this.currentMapName = mapMatch[1].trim();
            formatter.log(`Captured map name: "${this.currentMapName}"`);
            return;
        }

        const opponentMatch = cleanMessage.match(/Opponent: (.+)/);
        if (opponentMatch) {
            const opponentName = opponentMatch[1].trim();
            formatter.debug(`Opponent line: "${cleanMessage}" -> "${opponentName}" (game "${this.currentGameKey}")`);
            if (opponentName.length > 0 && opponentName !== this.proxy.client?.username) {
                this.proxy.hypixel.autoStatCheckDuels(opponentName, this.currentGameKey)
                    .catch(e => formatter.log(`Opponent stat check failed: ${e.message}`));
            }
        }

        const onlinePlayers = parseOnlineLine(cleanMessage);
        if (onlinePlayers) {
            if (this.isCapturingWho) {
                this.whoPlayers.push(...onlinePlayers);
                this.finishWhoCapture();
            } else if (this.currentGameKey === 'bw' && onlinePlayers.length > 0) {
                formatter.log(`ONLINE: update received, refreshing tab display for ${onlinePlayers.length} players.`);
                this.proxy.tabManager.updatePlayerTags(onlinePlayers, this.currentGameKey)
                    .catch(e => formatter.log(`Tab tag update failed: ${e.message}`));
            }
            return;
        }

        if (this.isCapturingWho) {
            const teamPlayers = parseTeamLine(cleanMessage);
            if (teamPlayers) {
                this.whoPlayers.push(...teamPlayers);
                return;
            }
            if (this.whoPlayers.length > 0 && cleanMessage === '') {
                this.finishWhoCapture();
                return;
            }
        }

        if (!this.hasTriggeredForGame && this.currentGameKey
            && GAME_START_MESSAGES.some(msg => cleanMessage.includes(msg))
            && this.isEnabledFor(this.currentGameKey)) {
            this.hasTriggeredForGame = true;
            this.awaitingTeleportForWho = true;
            formatter.log(`Game start message detected for '${this.currentGameKey}'. Awaiting teleport to execute /who.`);

            setTimeout(() => {
                if (this.proxy.target) {
                    this.proxy.target.write('chat', { message: '/wtfmap' });
                    formatter.log(`Game start detected. Sending /wtfmap to capture map name.`);
                }
            }, 100);
        }
    }

    startWhoCapture() {
        this.stopWhoCapture();
        this.isCapturingWho = true;
        this.whoPlayers = [];
        // If Hypixel never answers, stop capturing instead of swallowing later chat.
        this.whoTimeout = setTimeout(() => this.finishWhoCapture(), WHO_CAPTURE_TIMEOUT_MS);
    }

    stopWhoCapture() {
        clearTimeout(this.whoTimeout);
        this.whoTimeout = null;
        this.isCapturingWho = false;
        this.whoPlayers = [];
    }

    finishWhoCapture() {
        const players = this.whoPlayers;
        this.stopWhoCapture();
        formatter.log(`Captured ${players.length} players from /who.`);
        if (this.proxy.hypixel && players.length > 0) {
            this.proxy.hypixel.processQueueAndPrintBulk(players, this.currentGameKey)
                .catch(e => formatter.log(`Queue stat check failed: ${e.message}`));
        }
    }
}

module.exports = QueueStatsHandler;
