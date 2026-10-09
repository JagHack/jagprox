const formatter = require('../formatter.js');

// "Name has joined (3/8)!" is printed in every Hypixel pre-game lobby, whatever the mode.
const QUEUE_JOIN_REGEX = /has joined \(\d+\/\d+\)!$/;
const GAME_START_MESSAGES = [
    'The game starts in 1 second!',
    'Protect your bed and destroy the enemy beds.',
    'Eliminate your opponents!',
    'Gather resources and equipment on your',
];

class RankTracker {
    constructor(proxy) {
        this.proxy = proxy;
        this.rankData = [];
        this.inQueue = false;
    }

    reset() {
        this.rankData = [];
        this.inQueue = false;
    }

    handlePacket(data, meta) {
        if (meta.name === 'login' || meta.name === 'respawn') {
            // Hypixel sends a respawn on every server switch: new lobby, new player list.
            this.reset();
        } else if (meta.name === 'chat') {
            this.handleChat(data);
        } else if (meta.name === 'player_info') {
            if (data.action === 'add_player') {
                for (const player of data.data) {
                    if (!player.name || !player.name.startsWith('§k')) continue;
                    const rankProfile = this.getRankProfileByIGN('§r' + player.name);
                    if (rankProfile) {
                        rankProfile.uuid = player.uuid;
                    } else {
                        this.updateRankData(player.uuid, '§r' + player.name, '', -1);
                    }
                }
            } else if (data.action === 'remove_player') {
                for (const player of data.data) {
                    const rankProfile = this.getRankProfileByUUID(player.uuid);
                    if (rankProfile) {
                        this.rankData.splice(this.rankData.indexOf(rankProfile), 1);
                    }
                }
            }
        } else if (meta.name === 'named_entity_spawn') {
            const rankProfile = this.getRankProfileByUUID(data.playerUUID);
            if (rankProfile) {
                rankProfile.entityId = data.entityId;
            }
        } else if (meta.name === 'scoreboard_team') {
            if (Array.isArray(data.players) && data.players.length !== 0 && data.team.startsWith('§')) {
                for (const ign of data.players) {
                    if (!ign.startsWith('§r§k')) continue;
                    let rankProfile = this.getRankProfileByIGN(ign);
                    if (rankProfile) {
                        rankProfile.rank = data.team;
                    } else {
                        rankProfile = { uuid: '', ign, rank: data.team, entityId: -1, announced: false };
                        this.rankData.push(rankProfile);
                    }
                    if (this.inQueue) this.announce(rankProfile);
                }
            }
        } else if (meta.name === 'entity_destroy') {
            const destroyed = new Set(data.entityIds);
            this.rankData = this.rankData.filter(r => !destroyed.has(r.entityId));
        }
    }

    handleChat(data) {
        let cleanMessage;
        try {
            cleanMessage = formatter.extractText(JSON.parse(data.message)).replace(/§./g, '').trim();
        } catch (e) {
            return;
        }

        if (QUEUE_JOIN_REGEX.test(cleanMessage)) {
            if (!this.inQueue) {
                this.inQueue = true;
                formatter.log('Rank tracker: queue lobby detected, tracking joining players.');
                // Players already in the lobby were sent before our own join message.
                this.rankData.forEach(r => this.announce(r));
            }
        } else if (this.inQueue && GAME_START_MESSAGES.some(msg => cleanMessage.includes(msg))) {
            this.inQueue = false;
            formatter.log('Rank tracker: game started, no longer tracking joining players.');
        }
    }

    announce(rankProfile) {
        if (rankProfile.announced || !rankProfile.rank) return;
        rankProfile.announced = true;
        this.proxy.proxyChat(`§7Hidden player in queue: ${rankProfile.rank}`);
    }

    updateRankData(uuid, ign, rank, entityId) {
        const existing = this.getRankProfileByUUID(uuid);
        if (existing) {
            existing.ign = ign;
            existing.rank = rank;
            existing.entityId = entityId;
            return;
        }
        this.rankData.push({ uuid, ign, rank, entityId, announced: false });
    }

    getRankProfileByIGN(ign) {
        return this.rankData.find(r => r.ign === ign) || null;
    }

    getRankProfileByUUID(uuid) {
        return this.rankData.find(r => r.uuid === uuid) || null;
    }

    getRankProfileByEntityId(entityId) {
        return this.rankData.find(r => r.entityId === entityId) || null;
    }
}

module.exports = RankTracker;
