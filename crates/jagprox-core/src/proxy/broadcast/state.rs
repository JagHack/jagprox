//! Tracks the broadcaster's world from the packets Hypixel sends, so a spectator who
//! joins mid-game can be given a complete snapshot and then the live stream.
//!
//! Only the packets needed to rebuild the world are parsed; everything is stored as
//! the raw payload Hypixel sent (so there is no re-encoding or loss), keyed by what
//! we need to replace or remove it later (chunk column, entity id, score name …).
//! A fresh spectator gets [`GameState::snapshot`]: chunks + their block-change
//! overlay, entities placed at their current positions, the tab list and the
//! scoreboard. The live stream after that is forwarded by the host with transforms.

use std::collections::HashMap;

use indexmap::IndexMap;
use mcproto::buf::Read;
use mcproto::packets::{Packet, clientbound as cb};
use mcproto::world::{self, EntityPos};

/// Maps a window-0 inventory slot to the armor piece's EntityEquipment slot, or
/// `None` if it isn't armor. Inventory: 5 helmet, 6 chestplate, 7 leggings, 8 boots;
/// EntityEquipment: 4 helmet, 3 chestplate, 2 leggings, 1 boots.
fn armor_equipment_slot(inventory_slot: i16) -> Option<i16> {
    match inventory_slot {
        5 => Some(4),
        6 => Some(3),
        7 => Some(2),
        8 => Some(1),
        _ => None,
    }
}

/// One tracked entity: its spawn packet (replayed as-is), current position, and the
/// latest metadata/equipment/properties so a late spectator sees it correctly.
#[derive(Default, Clone)]
struct Entity {
    spawn: Vec<u8>,
    pos: EntityPos,
    metadata: Option<Vec<u8>>,
    equipment: HashMap<i16, Vec<u8>>,
    properties: Option<Vec<u8>>,
}

/// A scoreboard team, rebuilt into a single create packet for a snapshot.
#[derive(Default, Clone)]
struct Team {
    info: Option<cb::TeamInfo>,
    players: Vec<String>,
}

#[derive(Default)]
struct Scoreboard {
    /// objective name -> raw ScoreboardObjective payload.
    objectives: IndexMap<String, Vec<u8>>,
    /// display slot -> raw DisplayScoreboard payload.
    display: HashMap<i8, Vec<u8>>,
    /// (objective, entry) -> raw UpdateScore payload.
    scores: HashMap<(String, String), Vec<u8>>,
    teams: IndexMap<String, Team>,
}

/// The broadcaster's world, built up from Hypixel's packets.
#[derive(Default)]
pub struct GameState {
    /// Raw JoinGame payload; also the source of the broadcaster's own entity id.
    pub join_game: Option<Vec<u8>>,
    pub self_eid: i32,
    pub dimension: i32,
    /// The broadcaster's last absolute position (from server position corrections).
    pub position: Option<(f64, f64, f64, f32, f32)>,
    spawn_position: Option<Vec<u8>>,
    time: Option<Vec<u8>>,
    world_border: Option<Vec<u8>>,
    /// Raw ChunkData payload per column.
    chunks: IndexMap<(i32, i32), Vec<u8>>,
    /// Non-ground-up ChunkData (section updates) since each column was loaded,
    /// replayed in order after the full column.
    partials: HashMap<(i32, i32), Vec<Vec<u8>>>,
    /// Block changes since each column was (re)loaded: (local x<<4|z, y) -> block.
    overlay: HashMap<(i32, i32), HashMap<(u8, u8), i32>>,
    /// Raw UpdateBlockEntity payload per block position.
    block_entities: HashMap<i64, Vec<u8>>,
    entities: IndexMap<i32, Entity>,
    /// uuid -> the player's tab-list entry.
    tab: IndexMap<u128, cb::PlayerAdd>,
    scoreboard: Scoreboard,
    /// The broadcaster's own equipment (EntityEquipment slot -> raw item bytes:
    /// 0 held, 1 boots, 2 leggings, 3 chestplate, 4 helmet), derived from their
    /// inventory. Hypixel never sends equipment for the player themselves, so
    /// without this a spectator sees the broadcaster's model naked.
    self_equipment: HashMap<i16, Vec<u8>>,
    /// Hotbar contents (inventory slots 36..=44) as raw item bytes, and the selected
    /// index, so the held item (equipment slot 0) can be kept in sync.
    self_hotbar: HashMap<u8, Vec<u8>>,
    self_held_index: u8,
}

impl GameState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a world has been loaded (we have a JoinGame to base a spectator on).
    pub fn ready(&self) -> bool {
        self.join_game.is_some()
    }

    /// Feeds one clientbound payload (id + data) from Hypixel into the tracker.
    /// Unknown ids are ignored. Never fails: a packet that doesn't parse is skipped.
    pub fn observe(&mut self, payload: &[u8]) {
        let _ = self.try_observe(payload);
    }

    fn try_observe(&mut self, payload: &[u8]) -> mcproto::Result<()> {
        let mut r: &[u8] = payload;
        let id = mcproto::buf::Read::varint(&mut r)?;
        match id {
            cb::JoinGame::ID => {
                let jg = cb::JoinGame::from_payload(payload)?;
                self.self_eid = jg.entity_id;
                self.dimension = jg.dimension as i32;
                self.join_game = Some(payload.to_vec());
                // A new world: forget the old one.
                self.chunks.clear();
                self.partials.clear();
                self.overlay.clear();
                self.block_entities.clear();
                self.entities.clear();
            }
            cb::Respawn::ID => {
                let rp = cb::Respawn::from_payload(payload)?;
                self.dimension = rp.dimension;
                self.chunks.clear();
                self.partials.clear();
                self.overlay.clear();
                self.block_entities.clear();
                self.entities.clear();
            }
            cb::PlayerPositionAndLook::ID => {
                let p = cb::PlayerPositionAndLook::from_payload(payload)?;
                // Hypixel sends absolute teleports (flags 0); good enough as a base.
                self.position = Some((p.x, p.y, p.z, p.yaw, p.pitch));
            }
            world::id::SPAWN_POSITION => self.spawn_position = Some(payload.to_vec()),
            world::id::TIME_UPDATE => self.time = Some(payload.to_vec()),
            world::id::WORLD_BORDER => self.world_border = Some(payload.to_vec()),
            world::id::CHUNK_DATA => self.on_chunk_data(payload)?,
            world::id::MAP_CHUNK_BULK => self.on_map_chunk_bulk(payload)?,
            world::id::BLOCK_CHANGE => self.on_block_change(payload)?,
            world::id::MULTI_BLOCK_CHANGE => self.on_multi_block_change(payload)?,
            world::id::UPDATE_BLOCK_ENTITY => self.on_block_entity(&mut r, payload)?,
            world::id::SPAWN_PLAYER => self.on_spawn_player(payload)?,
            world::id::SPAWN_MOB => self.on_spawn(payload, world::spawn_mob_pos(payload)?)?,
            world::id::SPAWN_OBJECT => self.on_spawn(payload, world::spawn_object_pos(payload)?)?,
            world::id::SPAWN_PAINTING | world::id::SPAWN_EXPERIENCE_ORB => {
                let eid = world::leading_entity_id(payload)?;
                self.entities.entry(eid).or_default().spawn = payload.to_vec();
            }
            world::id::ENTITY_TELEPORT => {
                let tp = mcproto::world::EntityTeleport::from_payload(payload)?;
                if let Some(e) = self.entities.get_mut(&tp.eid) {
                    e.pos.x = tp.x;
                    e.pos.y = tp.y;
                    e.pos.z = tp.z;
                    e.pos.yaw = tp.yaw;
                    e.pos.pitch = tp.pitch;
                }
            }
            world::id::ENTITY_RELATIVE_MOVE => self.on_move(world::RelMove::decode_rel(payload)?),
            world::id::ENTITY_LOOK_AND_RELATIVE_MOVE => {
                self.on_move(world::RelMove::decode_look_rel(payload)?)
            }
            world::id::ENTITY_LOOK => self.on_move(world::RelMove::decode_look(payload)?),
            world::id::ENTITY_HEAD_LOOK => {
                let hl = mcproto::world::EntityHeadLook::from_payload(payload)?;
                if let Some(e) = self.entities.get_mut(&hl.eid) {
                    e.pos.head_yaw = hl.head_yaw;
                }
            }
            world::id::ENTITY_METADATA => {
                let eid = world::leading_entity_id(payload)?;
                if let Some(e) = self.entities.get_mut(&eid) {
                    e.metadata = Some(payload.to_vec());
                }
            }
            world::id::ENTITY_EQUIPMENT => {
                let mut rr: &[u8] = payload;
                let _ = mcproto::buf::Read::varint(&mut rr)?;
                let eid = mcproto::buf::Read::varint(&mut rr)?;
                let slot = mcproto::buf::Read::i16(&mut rr)?;
                if let Some(e) = self.entities.get_mut(&eid) {
                    e.equipment.insert(slot, payload.to_vec());
                }
            }
            world::id::ENTITY_PROPERTIES => {
                let eid = world::leading_entity_id(payload)?;
                if let Some(e) = self.entities.get_mut(&eid) {
                    e.properties = Some(payload.to_vec());
                }
            }
            world::id::WINDOW_ITEMS => self.on_window_items(&mut r)?,
            world::id::SET_SLOT => self.on_set_slot(&mut r)?,
            world::id::HELD_ITEM_CHANGE => {
                self.self_held_index = r.i8()?.clamp(0, 8) as u8;
                self.refresh_held();
            }
            cb::DestroyEntities::ID => {
                let d = cb::DestroyEntities::from_payload(payload)?;
                for eid in d.entity_ids {
                    self.entities.shift_remove(&eid);
                }
            }
            cb::PlayerListItem::ID => self.on_tab(cb::PlayerListItem::from_payload(payload)?),
            cb::ScoreboardObjective::ID => self.on_objective(payload, &mut r)?,
            cb::DisplayScoreboard::ID => self.on_display(payload)?,
            cb::UpdateScore::ID => self.on_score(payload)?,
            cb::Teams::ID => self.on_teams(cb::Teams::from_payload(payload)?),
            _ => {}
        }
        Ok(())
    }

    fn on_chunk_data(&mut self, payload: &[u8]) -> mcproto::Result<()> {
        let h = world::ChunkDataHeader::parse(payload)?;
        let key = (h.x, h.z);
        if h.is_unload() {
            self.chunks.shift_remove(&key);
            self.partials.remove(&key);
            self.overlay.remove(&key);
            self.block_entities
                .retain(|&loc, _| world::Position::decode(loc).chunk() != key);
        } else if h.ground_up {
            self.chunks.insert(key, payload.to_vec());
            // A fresh column supersedes any accumulated section updates/block changes.
            self.partials.remove(&key);
            self.overlay.remove(&key);
        } else if self.chunks.contains_key(&key) {
            // A section update: replayed after the base column, and it supersedes
            // earlier updates and block changes in the sections it covers.
            let partials = self.partials.entry(key).or_default();
            partials.retain(|old| {
                world::ChunkDataHeader::parse(old).is_ok_and(|o| o.bitmask & !h.bitmask != 0)
            });
            partials.push(payload.to_vec());
            if let Some(overlay) = self.overlay.get_mut(&key) {
                overlay.retain(|&(_, y), _| h.bitmask & (1 << (y >> 4)) == 0);
            }
        }
        Ok(())
    }

    fn on_map_chunk_bulk(&mut self, payload: &[u8]) -> mcproto::Result<()> {
        for col in world::split_map_chunk_bulk(payload)? {
            let key = (col.x, col.z);
            self.chunks.insert(key, col.chunk_data);
            self.partials.remove(&key);
            self.overlay.remove(&key);
        }
        Ok(())
    }

    fn on_block_change(&mut self, payload: &[u8]) -> mcproto::Result<()> {
        let bc = mcproto::world::BlockChange::from_payload(payload)?;
        let col = bc.position.chunk();
        if self.chunks.contains_key(&col) {
            let local = (
                (((bc.position.x & 15) << 4) | (bc.position.z & 15)) as u8,
                bc.position.y as u8,
            );
            self.overlay.entry(col).or_default().insert(local, bc.block);
        }
        Ok(())
    }

    fn on_multi_block_change(&mut self, payload: &[u8]) -> mcproto::Result<()> {
        let mbc = mcproto::world::MultiBlockChange::from_payload(payload)?;
        let col = (mbc.chunk_x, mbc.chunk_z);
        if self.chunks.contains_key(&col) {
            let overlay = self.overlay.entry(col).or_default();
            for rec in mbc.records {
                overlay.insert((rec.horizontal, rec.y), rec.block);
            }
        }
        Ok(())
    }

    fn on_block_entity(&mut self, r: &mut &[u8], payload: &[u8]) -> mcproto::Result<()> {
        let location = mcproto::buf::Read::i64(r)?;
        self.block_entities.insert(location, payload.to_vec());
        Ok(())
    }

    /// The full player inventory (Window Items, window 0): read the armor slots
    /// (5..=8) and hotbar (36..=44) so the broadcaster's model can be equipped.
    fn on_window_items(&mut self, r: &mut &[u8]) -> mcproto::Result<()> {
        let window = r.u8()?;
        let count = r.i16()?;
        for slot in 0..count {
            let item = world::read_slot(r)?.to_vec();
            if window == 0 {
                self.store_inventory_slot(slot, item);
            }
        }
        self.refresh_held();
        Ok(())
    }

    /// A single changed inventory slot (Set Slot). Only window 0 (the player's own
    /// inventory) carries their armor/hotbar.
    fn on_set_slot(&mut self, r: &mut &[u8]) -> mcproto::Result<()> {
        let window = r.i8()?;
        let slot = r.i16()?;
        let item = world::read_slot(r)?.to_vec();
        if window == 0 {
            self.store_inventory_slot(slot, item);
            self.refresh_held();
        }
        Ok(())
    }

    /// Files one window-0 inventory slot into the armor map or the hotbar.
    fn store_inventory_slot(&mut self, slot: i16, item: Vec<u8>) {
        // Inventory armor slots 5..=8 map to EntityEquipment slots 4..=1.
        if let Some(eq) = armor_equipment_slot(slot) {
            self.self_equipment.insert(eq, item);
        } else if (36..=44).contains(&slot) {
            self.self_hotbar.insert((slot - 36) as u8, item);
        }
    }

    /// Recompute the held item (EntityEquipment slot 0) from the selected hotbar slot.
    fn refresh_held(&mut self) {
        if let Some(item) = self.self_hotbar.get(&self.self_held_index) {
            self.self_equipment.insert(0, item.clone());
        }
    }

    /// EntityEquipment packets that dress the broadcaster's own player model, so a
    /// spectator sees their armor and held item rather than a naked model.
    pub fn self_equipment_packets(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for slot in 0..=4i16 {
            if let Some(item) = self.self_equipment.get(&slot) {
                out.push(world::entity_equipment(self.self_eid, slot, item));
            }
        }
        out
    }

    /// The broadcaster changed their selected hotbar slot (serverbound); keep the
    /// held item in sync. Returns the refreshed equipment packets to fan out live.
    pub fn set_held_index(&mut self, index: u8) -> Vec<Vec<u8>> {
        self.self_held_index = index.min(8);
        self.refresh_held();
        self.self_equipment_packets()
    }

    fn on_spawn(&mut self, payload: &[u8], parsed: (i32, EntityPos)) -> mcproto::Result<()> {
        let (eid, pos) = parsed;
        let e = self.entities.entry(eid).or_default();
        e.spawn = payload.to_vec();
        e.pos = pos;
        Ok(())
    }

    fn on_spawn_player(&mut self, payload: &[u8]) -> mcproto::Result<()> {
        let (eid, _uuid, pos) = world::spawn_player_pos(payload)?;
        let e = self.entities.entry(eid).or_default();
        e.spawn = payload.to_vec();
        e.pos = pos;
        Ok(())
    }

    fn on_move(&mut self, m: world::RelMove) {
        if let Some(e) = self.entities.get_mut(&m.eid) {
            e.pos.apply_relative(m.dx, m.dy, m.dz);
            if let Some(yaw) = m.yaw {
                e.pos.yaw = yaw;
            }
            if let Some(pitch) = m.pitch {
                e.pos.pitch = pitch;
            }
        }
    }

    fn on_tab(&mut self, item: cb::PlayerListItem) {
        match item.action {
            cb::PlayerListAction::Add(players) => {
                for p in players {
                    self.tab.insert(p.uuid, p);
                }
            }
            cb::PlayerListAction::UpdateGameMode(updates) => {
                for (uuid, gm) in updates {
                    if let Some(p) = self.tab.get_mut(&uuid) {
                        p.game_mode = gm;
                    }
                }
            }
            cb::PlayerListAction::UpdateLatency(updates) => {
                for (uuid, ping) in updates {
                    if let Some(p) = self.tab.get_mut(&uuid) {
                        p.ping = ping;
                    }
                }
            }
            cb::PlayerListAction::UpdateDisplayName(updates) => {
                for (uuid, name) in updates {
                    if let Some(p) = self.tab.get_mut(&uuid) {
                        p.display_name = name;
                    }
                }
            }
            cb::PlayerListAction::Remove(uuids) => {
                for uuid in uuids {
                    self.tab.shift_remove(&uuid);
                }
            }
        }
    }

    fn on_objective(&mut self, payload: &[u8], r: &mut &[u8]) -> mcproto::Result<()> {
        let name = mcproto::buf::Read::string(r)?;
        let mode = mcproto::buf::Read::i8(r)?;
        if mode == 1 {
            self.scoreboard.objectives.shift_remove(&name);
        } else {
            self.scoreboard.objectives.insert(name, payload.to_vec());
        }
        Ok(())
    }

    fn on_display(&mut self, payload: &[u8]) -> mcproto::Result<()> {
        let d = cb::DisplayScoreboard::from_payload(payload)?;
        self.scoreboard.display.insert(d.position, payload.to_vec());
        Ok(())
    }

    fn on_score(&mut self, payload: &[u8]) -> mcproto::Result<()> {
        let s = cb::UpdateScore::from_payload(payload)?;
        let key = (s.objective.clone(), s.name.clone());
        if s.action == 1 {
            self.scoreboard.scores.remove(&key);
        } else {
            self.scoreboard.scores.insert(key, payload.to_vec());
        }
        Ok(())
    }

    fn on_teams(&mut self, teams: cb::Teams) {
        match teams.mode {
            1 => {
                self.scoreboard.teams.shift_remove(&teams.team);
            }
            0 => {
                self.scoreboard.teams.insert(
                    teams.team,
                    Team {
                        info: teams.info,
                        players: teams.players.unwrap_or_default(),
                    },
                );
            }
            2 => {
                if let Some(t) = self.scoreboard.teams.get_mut(&teams.team) {
                    t.info = teams.info;
                }
            }
            3 => {
                if let Some(t) = self.scoreboard.teams.get_mut(&teams.team) {
                    t.players.extend(teams.players.unwrap_or_default());
                }
            }
            4 => {
                if let Some(t) = self.scoreboard.teams.get_mut(&teams.team) {
                    let removed = teams.players.unwrap_or_default();
                    t.players.retain(|p| !removed.contains(p));
                }
            }
            _ => {}
        }
    }

    /// Everything a fresh spectator needs to recreate the current world, in order.
    /// The caller sends its own JoinGame (with the spectator's entity id) first and
    /// spawns the broadcaster's model separately.
    pub fn snapshot(&self) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = Vec::new();
        if let Some(p) = &self.spawn_position {
            out.push(p.clone());
        }
        if let Some(p) = &self.time {
            out.push(p.clone());
        }
        if let Some(p) = &self.world_border {
            out.push(p.clone());
        }
        // Chunks, each followed by the block changes that have happened since.
        for (col, data) in &self.chunks {
            out.push(data.clone());
            for partial in self.partials.get(col).into_iter().flatten() {
                out.push(partial.clone());
            }
            if let Some(overlay) = self.overlay.get(col)
                && !overlay.is_empty()
            {
                let records = overlay
                    .iter()
                    .map(|(&(horizontal, y), &block)| mcproto::world::BlockRecord {
                        horizontal,
                        y,
                        block,
                    })
                    .collect();
                out.push(
                    mcproto::world::MultiBlockChange {
                        chunk_x: col.0,
                        chunk_z: col.1,
                        records,
                    }
                    .to_payload(),
                );
            }
        }
        for be in self.block_entities.values() {
            out.push(be.clone());
        }
        // Tab list: one Add with every current entry.
        if !self.tab.is_empty() {
            out.push(
                cb::PlayerListItem {
                    action: cb::PlayerListAction::Add(self.tab.values().cloned().collect()),
                }
                .to_payload(),
            );
        }
        // Entities: spawn, then place at the current position, then metadata/gear.
        for (&eid, e) in &self.entities {
            if e.spawn.is_empty() {
                continue;
            }
            out.push(e.spawn.clone());
            out.push(
                mcproto::world::EntityTeleport {
                    eid,
                    x: e.pos.x,
                    y: e.pos.y,
                    z: e.pos.z,
                    yaw: e.pos.yaw,
                    pitch: e.pos.pitch,
                    on_ground: true,
                }
                .to_payload(),
            );
            out.push(
                mcproto::world::EntityHeadLook {
                    eid,
                    head_yaw: e.pos.head_yaw,
                }
                .to_payload(),
            );
            if let Some(m) = &e.metadata {
                out.push(m.clone());
            }
            for eq in e.equipment.values() {
                out.push(eq.clone());
            }
            if let Some(p) = &e.properties {
                out.push(p.clone());
            }
        }
        // Scoreboard: objectives, then the display slots, scores and teams.
        for o in self.scoreboard.objectives.values() {
            out.push(o.clone());
        }
        for d in self.scoreboard.display.values() {
            out.push(d.clone());
        }
        for s in self.scoreboard.scores.values() {
            out.push(s.clone());
        }
        for (name, team) in &self.scoreboard.teams {
            out.push(
                cb::Teams {
                    team: name.clone(),
                    mode: 0,
                    info: team.info.clone(),
                    players: Some(team.players.clone()),
                }
                .to_payload(),
            );
        }
        out
    }

    /// The broadcaster's tab-list entry, for building their player model (skin).
    pub fn tab_entry(&self, uuid: u128) -> Option<&cb::PlayerAdd> {
        self.tab.get(&uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcproto::world;

    fn join_game(eid: i32) -> Vec<u8> {
        cb::JoinGame {
            entity_id: eid,
            game_mode: 0,
            dimension: 0,
            difficulty: 2,
            max_players: 1,
            level_type: "default".into(),
            reduced_debug_info: false,
        }
        .to_payload()
    }

    #[test]
    fn broadcaster_armor_and_held_item_become_equipment() {
        use mcproto::buf::Write;
        let mut gs = GameState::new();
        gs.observe(&join_game(7)); // self_eid = 7

        // Window Items (window 0, 45 slots): a helmet in armor slot 5, a sword in
        // hotbar slot 36, everything else empty.
        let mut wi = Vec::new();
        wi.varint(world::id::WINDOW_ITEMS);
        wi.u8(0);
        wi.i16(45);
        for slot in 0..45i16 {
            match slot {
                5 | 36 => {
                    wi.i16(if slot == 5 { 310 } else { 267 });
                    wi.i8(1);
                    wi.i16(0);
                    wi.u8(0); // no NBT
                }
                _ => wi.i16(-1), // empty
            }
        }
        gs.observe(&wi);

        // Every packet is an EntityEquipment for the broadcaster's own entity, and
        // the set includes the held item (slot 0) and the helmet (slot 4).
        let packets = gs.self_equipment_packets();
        let mut slots = Vec::new();
        for p in &packets {
            let mut r: &[u8] = p;
            assert_eq!(Read::varint(&mut r).unwrap(), world::id::ENTITY_EQUIPMENT);
            assert_eq!(Read::varint(&mut r).unwrap(), 7);
            slots.push(Read::i16(&mut r).unwrap());
        }
        assert!(slots.contains(&0), "held item is equipped");
        assert!(slots.contains(&4), "helmet is equipped");
    }

    #[test]
    fn tracks_world_and_replays_snapshot() {
        let mut gs = GameState::new();
        assert!(!gs.ready());
        gs.observe(&join_game(42));
        assert!(gs.ready());
        assert_eq!(gs.self_eid, 42);

        // A chunk, then a block change inside it.
        let chunk = world::chunk_data_payload(0, 0, true, 0b1, &[0u8; 100]);
        gs.observe(&chunk);
        let bc = world::BlockChange {
            position: world::Position { x: 3, y: 70, z: 5 },
            block: 1 << 4,
        };
        gs.observe(&bc.to_payload());

        // A mob that then moves.
        let mut spawn = Vec::new();
        mcproto::buf::Write::varint(&mut spawn, world::id::SPAWN_MOB);
        mcproto::buf::Write::varint(&mut spawn, 100); // eid
        mcproto::buf::Write::u8(&mut spawn, 54); // type
        for v in [32 * 10i32, 32 * 64, 32 * 20] {
            mcproto::buf::Write::i32(&mut spawn, v);
        }
        for _ in 0..3 {
            mcproto::buf::Write::i8(&mut spawn, 0);
        } // yaw/pitch/head
        for _ in 0..3 {
            mcproto::buf::Write::i16(&mut spawn, 0);
        } // velocity
        mcproto::buf::Write::u8(&mut spawn, 0x7f); // empty metadata terminator
        gs.observe(&spawn);
        // A relative move of +1 block on x (32 in fixed-point).
        let mut mv = Vec::new();
        mcproto::buf::Write::varint(&mut mv, world::id::ENTITY_RELATIVE_MOVE);
        mcproto::buf::Write::varint(&mut mv, 100);
        mcproto::buf::Write::i8(&mut mv, 32);
        mcproto::buf::Write::i8(&mut mv, 0);
        mcproto::buf::Write::i8(&mut mv, 0);
        mcproto::buf::Write::bool(&mut mv, true);
        gs.observe(&mv);

        let snap = gs.snapshot();
        // The snapshot contains the chunk and a multi-block-change overlay.
        assert!(
            snap.iter()
                .any(|p| p.first() == Some(&(world::id::CHUNK_DATA as u8)))
        );
        assert!(
            snap.iter()
                .any(|p| p.first() == Some(&(world::id::MULTI_BLOCK_CHANGE as u8)))
        );
        // And an entity teleport placing the mob at x = 11 blocks (10 + 1).
        let tp = snap
            .iter()
            .filter_map(|p| mcproto::world::EntityTeleport::from_payload(p).ok())
            .find(|t| t.eid == 100)
            .expect("mob teleport in snapshot");
        assert_eq!(tp.x, 32 * 11);
    }

    #[test]
    fn partial_chunk_keeps_base_and_unload_drops_block_entities() {
        let mut gs = GameState::new();
        gs.observe(&join_game(1));
        let base = world::chunk_data_payload(0, 0, true, 0b11, &[1u8; 64]);
        gs.observe(&base);
        // A block change in section 1 (y 16..32), then a section-1 update covering it.
        let bc = world::BlockChange {
            position: world::Position { x: 1, y: 20, z: 1 },
            block: 1 << 4,
        };
        gs.observe(&bc.to_payload());
        let partial = world::chunk_data_payload(0, 0, false, 0b10, &[2u8; 32]);
        gs.observe(&partial);
        let snap = gs.snapshot();
        let base_at = snap.iter().position(|p| *p == base).expect("base kept");
        let partial_at = snap
            .iter()
            .position(|p| *p == partial)
            .expect("partial kept");
        assert!(base_at < partial_at);
        // The block change was superseded by the section update.
        assert!(
            !snap
                .iter()
                .any(|p| p.first() == Some(&(world::id::MULTI_BLOCK_CHANGE as u8)))
        );

        // A block entity in the column goes away with the column.
        let mut be = Vec::new();
        mcproto::buf::Write::varint(&mut be, world::id::UPDATE_BLOCK_ENTITY);
        mcproto::buf::Write::i64(&mut be, world::Position { x: 2, y: 64, z: 3 }.encode());
        mcproto::buf::Write::u8(&mut be, 1);
        mcproto::buf::Write::u8(&mut be, 0);
        gs.observe(&be);
        assert_eq!(gs.block_entities.len(), 1);
        gs.observe(&world::chunk_data_payload(0, 0, true, 0, &[]));
        assert!(gs.block_entities.is_empty());
        assert!(gs.snapshot().iter().all(|p| *p != partial));
    }

    #[test]
    fn destroy_removes_entity_from_snapshot() {
        let mut gs = GameState::new();
        gs.observe(&join_game(1));
        let mut spawn = Vec::new();
        mcproto::buf::Write::varint(&mut spawn, world::id::SPAWN_EXPERIENCE_ORB);
        mcproto::buf::Write::varint(&mut spawn, 7);
        gs.observe(&spawn);
        assert!(gs.entities.contains_key(&7));
        gs.observe(
            &cb::DestroyEntities {
                entity_ids: vec![7],
            }
            .to_payload(),
        );
        assert!(!gs.entities.contains_key(&7));
    }
}
