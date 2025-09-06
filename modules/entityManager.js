const formatter = require('../formatter.js');

class EntityManager {
    constructor(proxy) {
        this.proxy = proxy;
        this.entities = new Map();
        this.selfPosition = { x: 0, y: 0, z: 0 };
    }

    reset() {
        this.entities.clear();
    }

    handlePacket(data, meta) {
        if (meta.name === 'position') {
            this.selfPosition = { x: data.x, y: data.y, z: data.z };
        }

        if (meta.name === 'named_entity_spawn') {
            const playerName = this.proxy.tabManager.getPlayerNameByUUID(data.playerUUID);
            if (playerName) {
                this.entities.set(data.entityId, {
                    uuid: data.playerUUID,
                    name: playerName,
                    x: data.x, y: data.y, z: data.z
                });
            }
        } else if (meta.name === 'entity_teleport') {
            if (this.entities.has(data.entityId)) {
                const entity = this.entities.get(data.entityId);
                entity.x = data.x;
                entity.y = data.y;
                entity.z = data.z;
            }
        } else if (meta.name === 'rel_entity_move' || meta.name === 'entity_move_look') {
            if (this.entities.has(data.entityId)) {
                const entity = this.entities.get(data.entityId);
                entity.x += data.dX;
                entity.y += data.dY;
                entity.z += data.dZ;
            }
        } else if (meta.name === 'entity_destroy') {
            data.entityIds.forEach(id => this.entities.delete(id));
        }
    }

    getNearbyPlayers(radius = 150) {
        const nearby = [];
        const radiusSq = radius * radius;
        for (const entity of this.entities.values()) {
            const dx = this.selfPosition.x - entity.x;
            const dy = this.selfPosition.y - entity.y;
            const dz = this.selfPosition.z - entity.z;
            if (dx * dx + dy * dy + dz * dz < radiusSq) {
                nearby.push(entity.name);
            }
        }
        return nearby;
    }
}

module.exports = EntityManager;