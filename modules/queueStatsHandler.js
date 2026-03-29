const formatter = require("../formatter.js");
const { gameModeMap } = require('../utils/constants');

class QueueStatsHandler {
    constructor(proxy) {
        this.proxy = proxy;
        this.currentGameKey = null;
        this.hasTriggeredForGame = false;
        this.awaitingTeleportForWho = false;

        this.currentMapName = null;

        this.isCapturingWho = false;
        this.whoPlayers = [];
        this.lastOpponentStatCheckTime = 0; 

        this.titleToKeyMap = new Map();
        for (const [key, modeInfo] of Object.entries(gameModeMap)) {
            if (modeInfo && modeInfo.displayName) {
                this.titleToKeyMap.set(modeInfo.displayName.toUpperCase(), key);
            }
        }
    }

    reset() {
        formatter.log('Queue Stats Handler reset.');
        this.currentGameKey = null;
        this.hasTriggeredForGame = false;
        this.awaitingTeleportForWho = false;
        this.isCapturingWho = false;
        this.whoPlayers = [];
        this.lastOpponentStatCheckTime = 0;
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

    handlePacket(data, meta) {
        if (meta.name === 'kick_disconnect' || meta.name === 'disconnect') {
            this.currentGameKey = null;
            this.hasTriggeredForGame = false;
            this.awaitingTeleportForWho = false;
            return false;
        }

        if (meta.name === 'position' && this.awaitingTeleportForWho) {
            this.awaitingTeleportForWho = false;

            // Skip /who for Duels modes - only run for Bedwars and SkyWars
            const { duelsModes } = require('../utils/constants');
            const isDuelsMode = this.currentGameKey && (
                this.currentGameKey === 'duels' ||
                Object.keys(duelsModes).includes(this.currentGameKey)
            );

            if (isDuelsMode) {
                formatter.log("Skipping /who for Duels mode.");
                this.hasTriggeredForGame = true;
            } else {
                formatter.log("Teleport confirmed. Executing /who after a short safety delay.");
                setTimeout(() => {
                    this.isCapturingWho = true;
                    this.whoPlayers = [];
                    this.proxy.target.write('chat', { message: '/who' });
                }, 500);
            }
        }

		if (meta.name === 'scoreboard_objective' && data.action === 0 && data.displayText) {
		    const newTitle = data.displayText.replace(/§[0-9a-fk-or]/g, '').trim().toUpperCase();
		    const foundGameKey = this.titleToKeyMap.get(newTitle);
		    if (foundGameKey && this.currentGameKey !== foundGameKey) {
			this.resetForNewGame(foundGameKey);
		    }
		    if (foundGameKey) {
			this.hasTriggeredForGame = false;
		    }
		}

		if (meta.name === 'chat') {
		    let chatJson, fullMessage = '', cleanMessage = '';
		    try {
			chatJson = JSON.parse(data.message);
			const getFullText = (component) => {
			    let text = component.text || '';
			    if (component.extra) text += component.extra.map(getFullText).join('');
			    return text;
			};
			fullMessage = getFullText(chatJson);
			cleanMessage = fullMessage.replace(/§[0-9a-fk-or]/g, '').trim();
		    } catch (e) { }
		    const gameOverKeywords = ['VICTORY!', 'GAME END', 'You died!', 'You have been eliminated!', 'You won!', 'Draw!'];
		    if (gameOverKeywords.some(keyword => cleanMessage.includes(keyword))) {
		    this.resetTrigger();
		    }

		    const mapMatch = cleanMessage.match(/You are currently playing on (?:map: )?(.+)/);
		    if (mapMatch) {
		        this.currentMapName = mapMatch[1].trim();
		        formatter.log(`Captured map name: "${this.currentMapName}"`);
		        return true;
		    }

		    const COOLDOWN_MS = 0; 
				if (Date.now() - this.lastOpponentStatCheckTime > COOLDOWN_MS) {

                            let opponentNameMatch = cleanMessage.match(/Opponent: (.+)/);

                            if (opponentNameMatch) {

                                formatter.log('DEBUG: Opponent name match attempt. cleanMessage: "' + cleanMessage + '"');

                                formatter.log('DEBUG: opponentNameMatch result: ' + JSON.stringify(opponentNameMatch));

                                let extractedName = opponentNameMatch[1];

                                if (extractedName) {

                                    extractedName = extractedName.trim();

                                    formatter.log('DEBUG: Extracted name: "' + extractedName + '"');

                                    if (extractedName.length > 0 && extractedName !== this.proxy.client.username) {

                                        this.lastOpponentStatCheckTime = Date.now(); 

                                        formatter.log('DEBUG: Current game key: "' + this.currentGameKey + '"');

                                        formatter.log('DEBUG: Calling autoStatCheckDuels with: ' + extractedName + ', ' + this.currentGameKey);

                                        this.proxy.hypixel.autoStatCheckDuels(extractedName, this.currentGameKey);

                                    }

                                }

                            }

                        }

            if (cleanMessage.startsWith('ONLINE: ')) {
                const players = cleanMessage.replace('ONLINE: ', '').split(/,\s*/);
                const cleanedPlayers = players.map(p => p.trim().replace(/\.$/, '')).filter(Boolean);
                if (this.isCapturingWho) {
                    this.whoPlayers.push(...cleanedPlayers);
                    this.finishWhoCapture();
                } else if (this.currentGameKey === 'bw') {
                    
                    formatter.log(`ONLINE: update received, refreshing tab display for ${cleanedPlayers.length} players.`);
                    if (this.proxy.hypixel && cleanedPlayers.length > 0) {
                        this.proxy.tabManager.updatePlayerTags(cleanedPlayers, this.currentGameKey);
                    }
                }
                return true;
            }

            if (this.isCapturingWho) {
                if (cleanMessage.startsWith('Team #')) {
                    const parts = cleanMessage.split(':');
                    if (parts.length > 1) {
                        const players = parts.slice(1).join(':').trim().split(/,\s*/);
                        this.whoPlayers.push(...players.map(p => p.trim().replace(/\.$/, '')));
                    }
                    return true;
                }
                if (this.whoPlayers.length > 0 && cleanMessage === '') {
                    this.finishWhoCapture();
                    return true;
                }
            }

            if (!this.hasTriggeredForGame && this.currentGameKey) {
                const isBedwarsStart = cleanMessage.includes('Protect your bed and destroy the enemy beds.');
                const isDuelsStart = cleanMessage.includes('Eliminate your opponents!');
                const isSkywarsStart = cleanMessage.includes('Gather resources and equipment on your');

                if (isBedwarsStart || isDuelsStart || isSkywarsStart) {
                    const isEnabled = this.proxy.config.queue_stats && this.proxy.config.queue_stats[this.currentGameKey];
                    if (isEnabled) {
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
            }
        }

        return false;
    }

    finishWhoCapture() {
        this.isCapturingWho = false;
        formatter.log(`Captured ${this.whoPlayers.length} players from /who.`);
        if (this.proxy.hypixel && this.whoPlayers.length > 0) {
            this.proxy.hypixel.processQueueAndPrintBulk(this.whoPlayers, this.currentGameKey);
        }
        this.whoPlayers = [];
    }
}

module.exports = QueueStatsHandler;
