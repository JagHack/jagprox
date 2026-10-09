class RankTracker {
    constructor(proxy) {
        this.proxy = proxy;
        this.rankData = [];
    }

    reset() {
        this.rankData = [];
    }

    handlePacket(data, meta) {
        if (meta.name === 'player_info') {
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
                    const rankProfile = this.getRankProfileByIGN(ign);
                    if (rankProfile) {
                        rankProfile.rank = data.team;
                    } else {
                        this.rankData.push({ uuid: '', ign, rank: data.team, entityId: -1 });
                    }
                }
            }
        } else if (meta.name === 'entity_destroy') {
            const destroyed = new Set(data.entityIds);
            this.rankData = this.rankData.filter(r => !destroyed.has(r.entityId));
        }
    }

    updateRankData(uuid, ign, rank, entityId) {
        const existing = this.getRankProfileByUUID(uuid);
        if (existing) {
            existing.ign = ign;
            existing.rank = rank;
            existing.entityId = entityId;
            return;
        }
        this.rankData.push({ uuid, ign, rank, entityId });
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
