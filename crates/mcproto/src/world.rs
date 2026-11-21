//! Extra 1.8.9 (protocol 47) packets needed to track and rebuild the world for a
//! live spectator: chunks, block changes, entities and their movement.
//!
//! Most of these are only ever *replayed* to a fresh spectator, so for those we
//! keep the raw payload exactly as Hypixel sent it and decode only the key fields
//! (entity id, position) we need to track. Full encoders exist only for the few
//! packets JagProx has to *synthesise* itself: [`EntityTeleport`] (to place a
//! tracked entity at its current position after replaying its spawn), the block
//! overlay ([`MultiBlockChange`]), and the broadcaster's own player model
//! ([`SpawnPlayer`], the relative-move/look packets and [`Animation`]).

use crate::buf::{Read, Write};
use crate::error::{Error, Result};
use crate::packets::Packet;

// Clientbound play packet ids (protocol 47) that the broadcaster tracks.
pub mod id {
    pub const TIME_UPDATE: i32 = 0x03;
    pub const ENTITY_EQUIPMENT: i32 = 0x04;
    pub const SPAWN_POSITION: i32 = 0x05;
    pub const ANIMATION: i32 = 0x0B;
    pub const SPAWN_PLAYER: i32 = 0x0C;
    pub const COLLECT_ITEM: i32 = 0x0D;
    pub const SPAWN_OBJECT: i32 = 0x0E;
    pub const SPAWN_MOB: i32 = 0x0F;
    pub const SPAWN_PAINTING: i32 = 0x10;
    pub const SPAWN_EXPERIENCE_ORB: i32 = 0x11;
    pub const ENTITY_VELOCITY: i32 = 0x12;
    pub const DESTROY_ENTITIES: i32 = 0x13;
    pub const ENTITY: i32 = 0x14;
    pub const ENTITY_RELATIVE_MOVE: i32 = 0x15;
    pub const ENTITY_LOOK: i32 = 0x16;
    pub const ENTITY_LOOK_AND_RELATIVE_MOVE: i32 = 0x17;
    pub const ENTITY_TELEPORT: i32 = 0x18;
    pub const ENTITY_HEAD_LOOK: i32 = 0x19;
    pub const ENTITY_METADATA: i32 = 0x1C;
    pub const ENTITY_EFFECT: i32 = 0x1D;
    pub const REMOVE_ENTITY_EFFECT: i32 = 0x1E;
    pub const ENTITY_PROPERTIES: i32 = 0x20;
    pub const CHUNK_DATA: i32 = 0x21;
    pub const MULTI_BLOCK_CHANGE: i32 = 0x22;
    pub const BLOCK_CHANGE: i32 = 0x23;
    pub const MAP_CHUNK_BULK: i32 = 0x26;
    pub const HELD_ITEM_CHANGE: i32 = 0x09;
    pub const CHANGE_GAME_STATE: i32 = 0x2B;
    pub const SET_SLOT: i32 = 0x2F;
    pub const WINDOW_ITEMS: i32 = 0x30;
    pub const UPDATE_BLOCK_ENTITY: i32 = 0x35;
    pub const WORLD_BORDER: i32 = 0x44;
}

/// Reads the packet id and the entity id that follows it (both VarInts), the shape
/// of nearly every entity packet. Used to key a stored raw payload by entity.
pub fn leading_entity_id(payload: &[u8]) -> Result<i32> {
    let mut r = payload;
    let _id = r.varint()?;
    r.varint()
}

// ---------------------------------------------------------------------------
// Block positions
// ---------------------------------------------------------------------------

/// A packed 1.8 block position: x (26 bits) | y (12 bits) | z (26 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Position {
    pub fn decode(value: i64) -> Self {
        // Arithmetic shifts on the i64 sign-extend each field, as BlockPos.fromLong does.
        let x = (value >> 38) as i32;
        let y = (value << 26 >> 52) as i32;
        let z = (value << 38 >> 38) as i32;
        Self { x, y, z }
    }

    pub fn encode(self) -> i64 {
        ((self.x as i64 & 0x3FF_FFFF) << 38)
            | ((self.y as i64 & 0xFFF) << 26)
            | (self.z as i64 & 0x3FF_FFFF)
    }

    /// The chunk column this block is in.
    pub fn chunk(self) -> (i32, i32) {
        (self.x >> 4, self.z >> 4)
    }
}

// ---------------------------------------------------------------------------
// Entities: position tracking + synthesised movement
// ---------------------------------------------------------------------------

/// An entity's position in 1.8 fixed-point (blocks × 32) plus its angles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntityPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub yaw: i8,
    pub pitch: i8,
    pub head_yaw: i8,
}

impl EntityPos {
    pub fn apply_relative(&mut self, dx: i8, dy: i8, dz: i8) {
        self.x += dx as i32;
        self.y += dy as i32;
        self.z += dz as i32;
    }
}

/// Decodes the absolute position from a spawn packet whose coordinates start right
/// after an extra prefix (`extra` bytes: 0 for mob/object after the type, 16+? for
/// player). The caller passes a reader positioned right before the x coordinate.
fn read_fixed_pos(r: &mut &[u8], with_head: bool) -> Result<(i32, i32, i32, i8, i8, i8)> {
    let x = r.i32()?;
    let y = r.i32()?;
    let z = r.i32()?;
    let a = r.i8()?;
    let b = r.i8()?;
    let head = if with_head { r.i8()? } else { 0 };
    Ok((x, y, z, a, b, head))
}

/// Decodes the entity id and starting position from a 0x0F Spawn Mob payload.
pub fn spawn_mob_pos(payload: &[u8]) -> Result<(i32, EntityPos)> {
    let mut r = payload;
    if r.varint()? != id::SPAWN_MOB {
        return Err(Error::Malformed("not spawn mob".into()));
    }
    let eid = r.varint()?;
    let _type = r.u8()?;
    let (x, y, z, yaw, pitch, head_yaw) = read_fixed_pos(&mut r, true)?;
    Ok((
        eid,
        EntityPos {
            x,
            y,
            z,
            yaw,
            pitch,
            head_yaw,
        },
    ))
}

/// Decodes the entity id and starting position from a 0x0E Spawn Object payload.
/// Object order is x,y,z,pitch,yaw (pitch before yaw), no head yaw.
pub fn spawn_object_pos(payload: &[u8]) -> Result<(i32, EntityPos)> {
    let mut r = payload;
    if r.varint()? != id::SPAWN_OBJECT {
        return Err(Error::Malformed("not spawn object".into()));
    }
    let eid = r.varint()?;
    let _type = r.i8()?;
    let x = r.i32()?;
    let y = r.i32()?;
    let z = r.i32()?;
    let pitch = r.i8()?;
    let yaw = r.i8()?;
    Ok((
        eid,
        EntityPos {
            x,
            y,
            z,
            yaw,
            pitch,
            head_yaw: yaw,
        },
    ))
}

/// Decodes the entity id, UUID and starting position from a 0x0C Spawn Player payload.
pub fn spawn_player_pos(payload: &[u8]) -> Result<(i32, u128, EntityPos)> {
    let mut r = payload;
    if r.varint()? != id::SPAWN_PLAYER {
        return Err(Error::Malformed("not spawn player".into()));
    }
    let eid = r.varint()?;
    let uuid = r.uuid()?;
    let (x, y, z, yaw, pitch, _) = read_fixed_pos(&mut r, false)?;
    Ok((
        eid,
        uuid,
        EntityPos {
            x,
            y,
            z,
            yaw,
            pitch,
            head_yaw: yaw,
        },
    ))
}

/// Relative-move deltas (0x15 / 0x17), in fixed-point bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelMove {
    pub eid: i32,
    pub dx: i8,
    pub dy: i8,
    pub dz: i8,
    /// Present for 0x17 (look + move) and 0x16 (look).
    pub yaw: Option<i8>,
    pub pitch: Option<i8>,
}

impl RelMove {
    pub fn decode_rel(payload: &[u8]) -> Result<Self> {
        let mut r = payload;
        let _ = r.varint()?;
        let eid = r.varint()?;
        Ok(Self {
            eid,
            dx: r.i8()?,
            dy: r.i8()?,
            dz: r.i8()?,
            yaw: None,
            pitch: None,
        })
    }

    pub fn decode_look_rel(payload: &[u8]) -> Result<Self> {
        let mut r = payload;
        let _ = r.varint()?;
        let eid = r.varint()?;
        Ok(Self {
            eid,
            dx: r.i8()?,
            dy: r.i8()?,
            dz: r.i8()?,
            yaw: Some(r.i8()?),
            pitch: Some(r.i8()?),
        })
    }

    pub fn decode_look(payload: &[u8]) -> Result<Self> {
        let mut r = payload;
        let _ = r.varint()?;
        let eid = r.varint()?;
        Ok(Self {
            eid,
            dx: 0,
            dy: 0,
            dz: 0,
            yaw: Some(r.i8()?),
            pitch: Some(r.i8()?),
        })
    }
}

/// `entity_teleport` (0x18). The one entity packet JagProx both decodes and encodes:
/// decodes to track an entity's absolute position, encodes to place a tracked entity
/// at its current position right after replaying its spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityTeleport {
    pub eid: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub yaw: i8,
    pub pitch: i8,
    pub on_ground: bool,
}

impl Packet for EntityTeleport {
    const ID: i32 = id::ENTITY_TELEPORT;
    fn decode(r: &mut &[u8]) -> Result<Self> {
        Ok(Self {
            eid: r.varint()?,
            x: r.i32()?,
            y: r.i32()?,
            z: r.i32()?,
            yaw: r.i8()?,
            pitch: r.i8()?,
            on_ground: r.bool()?,
        })
    }
    fn encode(&self, w: &mut Vec<u8>) {
        w.varint(self.eid);
        w.i32(self.x);
        w.i32(self.y);
        w.i32(self.z);
        w.i8(self.yaw);
        w.i8(self.pitch);
        w.bool(self.on_ground);
    }
}

/// `entity_head_rotation` (0x19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityHeadLook {
    pub eid: i32,
    pub head_yaw: i8,
}

impl Packet for EntityHeadLook {
    const ID: i32 = id::ENTITY_HEAD_LOOK;
    fn decode(r: &mut &[u8]) -> Result<Self> {
        Ok(Self {
            eid: r.varint()?,
            head_yaw: r.i8()?,
        })
    }
    fn encode(&self, w: &mut Vec<u8>) {
        w.varint(self.eid);
        w.i8(self.head_yaw);
    }
}

/// `rel_entity_move_look` (0x17), encoded for the broadcaster's own model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityLookAndRelativeMove {
    pub eid: i32,
    pub dx: i8,
    pub dy: i8,
    pub dz: i8,
    pub yaw: i8,
    pub pitch: i8,
    pub on_ground: bool,
}

impl Packet for EntityLookAndRelativeMove {
    const ID: i32 = id::ENTITY_LOOK_AND_RELATIVE_MOVE;
    fn decode(r: &mut &[u8]) -> Result<Self> {
        Ok(Self {
            eid: r.varint()?,
            dx: r.i8()?,
            dy: r.i8()?,
            dz: r.i8()?,
            yaw: r.i8()?,
            pitch: r.i8()?,
            on_ground: r.bool()?,
        })
    }
    fn encode(&self, w: &mut Vec<u8>) {
        w.varint(self.eid);
        w.i8(self.dx);
        w.i8(self.dy);
        w.i8(self.dz);
        w.i8(self.yaw);
        w.i8(self.pitch);
        w.bool(self.on_ground);
    }
}

/// `animation` (0x0B), clientbound, for the broadcaster's arm swings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Animation {
    pub eid: i32,
    pub animation: u8,
}

impl Packet for Animation {
    const ID: i32 = id::ANIMATION;
    fn decode(r: &mut &[u8]) -> Result<Self> {
        Ok(Self {
            eid: r.varint()?,
            animation: r.u8()?,
        })
    }
    fn encode(&self, w: &mut Vec<u8>) {
        w.varint(self.eid);
        w.u8(self.animation);
    }
}

// ---------------------------------------------------------------------------
// Chunks
// ---------------------------------------------------------------------------

/// The header of a 0x21 Chunk Data packet (the data stays as raw bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkDataHeader {
    pub x: i32,
    pub z: i32,
    pub ground_up: bool,
    pub bitmask: u16,
}

impl ChunkDataHeader {
    /// Parses only the header of a Chunk Data payload.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let mut r = payload;
        if r.varint()? != id::CHUNK_DATA {
            return Err(Error::Malformed("not chunk data".into()));
        }
        Ok(Self {
            x: r.i32()?,
            z: r.i32()?,
            ground_up: r.bool()?,
            bitmask: r.u16()?,
        })
    }

    /// A chunk-data packet with `ground_up` set and an empty bitmask tells the client
    /// to unload the column.
    pub fn is_unload(&self) -> bool {
        self.ground_up && self.bitmask == 0
    }
}

/// Builds a 0x04 Entity Equipment payload: which slot of an entity holds which item.
///
/// `item` is the raw 1.8 item-stack bytes, taken verbatim from an inventory packet
/// (see [`read_slot`]). Slots: 0 held, 1 boots, 2 leggings, 3 chestplate, 4 helmet.
pub fn entity_equipment(eid: i32, slot: i16, item: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(item.len() + 8);
    out.varint(id::ENTITY_EQUIPMENT);
    out.varint(eid);
    out.i16(slot);
    out.put(item);
    out
}

/// Reads one 1.8 item-stack ("Slot") and returns its exact raw bytes, advancing the
/// cursor past it. An empty slot is a single `-1` short; a present item is
/// `id:i16, count:i8, damage:i16` followed by optional item NBT (a `0` byte for
/// none, otherwise a named TAG_Compound). Used to split inventory packets into
/// per-slot item bytes without decoding the NBT itself.
pub fn read_slot<'a>(r: &mut &'a [u8]) -> Result<&'a [u8]> {
    // Copy the reference so we keep the original 'a slice while `r` advances.
    let start = *r;
    let item_id = r.i16()?;
    if item_id >= 0 {
        let _count = r.i8()?;
        let _damage = r.i16()?;
        skip_nbt_tag(r, 0)?;
    }
    let consumed = start.len() - r.len();
    Ok(&start[..consumed])
}

/// Skips one *named* NBT tag (type byte, then name + payload), as found at the top
/// of an item's optional NBT. A `0` (TAG_End) type means "no NBT". `depth` guards
/// against a malformed packet recursing without bound.
fn skip_nbt_tag(r: &mut &[u8], depth: u32) -> Result<()> {
    if depth > 32 {
        return Err(Error::Malformed("NBT nested too deep".into()));
    }
    let tag = r.u8()?;
    if tag == 0 {
        return Ok(());
    }
    let name_len = r.u16()? as usize; // tag name
    r.take(name_len)?;
    skip_nbt_payload(r, tag, depth)
}

/// An NBT array length; negative is malformed (vanilla throws on it).
fn nbt_len(r: &mut &[u8]) -> Result<usize> {
    let n = r.i32()?;
    usize::try_from(n).map_err(|_| Error::Malformed(format!("negative NBT length {n}")))
}

/// Skips the payload of an NBT tag of the given type.
fn skip_nbt_payload(r: &mut &[u8], tag: u8, depth: u32) -> Result<()> {
    if depth > 32 {
        return Err(Error::Malformed("NBT nested too deep".into()));
    }
    match tag {
        1 => drop(r.take(1)?),     // byte
        2 => drop(r.take(2)?),     // short
        3 | 5 => drop(r.take(4)?), // int / float
        4 | 6 => drop(r.take(8)?), // long / double
        7 => {
            let n = nbt_len(r)?; // byte array
            r.take(n)?;
        }
        8 => {
            let n = r.u16()? as usize; // string
            r.take(n)?;
        }
        9 => {
            let elem = r.u8()?; // list: element type, length, then payloads
            let n = r.i32()?.max(0); // vanilla reads a negative list length as empty
            for _ in 0..n {
                skip_nbt_payload(r, elem, depth + 1)?;
            }
        }
        10 => {
            loop {
                // compound: named tags until TAG_End
                let child = r.u8()?;
                if child == 0 {
                    break;
                }
                let name_len = r.u16()? as usize;
                r.take(name_len)?;
                skip_nbt_payload(r, child, depth + 1)?;
            }
        }
        11 => {
            let n = nbt_len(r)?; // int array
            r.take(n.saturating_mul(4))?;
        }
        other => return Err(Error::Malformed(format!("unknown NBT tag {other}"))),
    }
    Ok(())
}

/// Builds a 0x21 Chunk Data payload (id + fields) from a column's raw section data.
pub fn chunk_data_payload(x: i32, z: i32, ground_up: bool, bitmask: u16, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    out.varint(id::CHUNK_DATA);
    out.i32(x);
    out.i32(z);
    out.bool(ground_up);
    out.u16(bitmask);
    out.byte_array(data);
    out
}

/// One column split out of a Map Chunk Bulk packet, as an independent Chunk Data payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkColumn {
    pub x: i32,
    pub z: i32,
    pub bitmask: u16,
    /// A ready-to-send 0x21 Chunk Data payload.
    pub chunk_data: Vec<u8>,
}

/// Number of data bytes one bulk column occupies: 16×16×16 blocks (×2) + block light
/// (+ sky light in overworld) per present section, plus a 256-byte biome array (bulk
/// columns are always ground-up).
fn bulk_column_size(bitmask: u16, sky_light: bool) -> usize {
    let sections = bitmask.count_ones() as usize;
    let per_section = 4096 * 2 + 2048 + if sky_light { 2048 } else { 0 };
    sections * per_section + 256
}

/// Splits a 0x26 Map Chunk Bulk payload into independent Chunk Data columns.
pub fn split_map_chunk_bulk(payload: &[u8]) -> Result<Vec<BulkColumn>> {
    let mut r = payload;
    if r.varint()? != id::MAP_CHUNK_BULK {
        return Err(Error::Malformed("not map chunk bulk".into()));
    }
    let sky_light = r.bool()?;
    let count = r.length()?;
    let mut metas = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let x = r.i32()?;
        let z = r.i32()?;
        let bitmask = r.u16()?;
        metas.push((x, z, bitmask));
    }
    let mut columns = Vec::with_capacity(metas.len());
    for (x, z, bitmask) in metas {
        let size = bulk_column_size(bitmask, sky_light);
        let data = r.take(size)?;
        columns.push(BulkColumn {
            x,
            z,
            bitmask,
            chunk_data: chunk_data_payload(x, z, true, bitmask, data),
        });
    }
    Ok(columns)
}

// ---------------------------------------------------------------------------
// Block changes (for the per-column overlay replayed after each chunk)
// ---------------------------------------------------------------------------

/// A single 0x23 Block Change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockChange {
    pub position: Position,
    /// Global palette id (`block_id << 4 | metadata`).
    pub block: i32,
}

impl Packet for BlockChange {
    const ID: i32 = id::BLOCK_CHANGE;
    fn decode(r: &mut &[u8]) -> Result<Self> {
        Ok(Self {
            position: Position::decode(r.i64()?),
            block: r.varint()?,
        })
    }
    fn encode(&self, w: &mut Vec<u8>) {
        w.i64(self.position.encode());
        w.varint(self.block);
    }
}

/// One record inside a Multi Block Change: packed local x/z, y, and the new block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRecord {
    /// `(x & 15) << 4 | (z & 15)`.
    pub horizontal: u8,
    pub y: u8,
    pub block: i32,
}

/// A 0x22 Multi Block Change for one chunk column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiBlockChange {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub records: Vec<BlockRecord>,
}

impl Packet for MultiBlockChange {
    const ID: i32 = id::MULTI_BLOCK_CHANGE;
    fn decode(r: &mut &[u8]) -> Result<Self> {
        let chunk_x = r.i32()?;
        let chunk_z = r.i32()?;
        let count = r.length()?;
        let mut records = Vec::with_capacity(count.min(16 * 16 * 256));
        for _ in 0..count {
            records.push(BlockRecord {
                horizontal: r.u8()?,
                y: r.u8()?,
                block: r.varint()?,
            });
        }
        Ok(Self {
            chunk_x,
            chunk_z,
            records,
        })
    }
    fn encode(&self, w: &mut Vec<u8>) {
        w.i32(self.chunk_x);
        w.i32(self.chunk_z);
        w.length(self.records.len());
        for record in &self.records {
            w.u8(record.horizontal);
            w.u8(record.y);
            w.varint(record.block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_slot_delimits_empty_present_and_nbt_items() {
        // An empty slot (-1), a plain item (id 5, count 1, dmg 0, no NBT), and an
        // item carrying a small NBT compound, concatenated. read_slot must split
        // them at the right byte boundaries, NBT and all.
        let mut buf = Vec::new();
        buf.i16(-1); // empty
        buf.i16(5);
        buf.i8(1);
        buf.i16(0);
        buf.u8(0); // no NBT
        buf.i16(276);
        buf.i8(1);
        buf.i16(3);
        // NBT: compound { byte "Unbreakable" = 1 } then TAG_End.
        buf.u8(10); // TAG_Compound
        buf.u16(0); // empty name
        buf.u8(1); // TAG_Byte
        buf.u16(11);
        buf.put(b"Unbreakable");
        buf.u8(1); // value
        buf.u8(0); // end compound

        let mut r: &[u8] = &buf;
        let empty = read_slot(&mut r).unwrap();
        assert_eq!(empty, &[0xFF, 0xFF]); // -1 as i16
        let plain = read_slot(&mut r).unwrap();
        assert_eq!(plain.len(), 2 + 1 + 2 + 1); // id, count, dmg, no-nbt byte
        let with_nbt = read_slot(&mut r).unwrap();
        assert_eq!(with_nbt.len(), buf.len() - empty.len() - plain.len());
        assert!(r.is_empty(), "all three slots consumed exactly");
    }

    /// A present item (id 1, count 1, damage 0) carrying `nbt` as its tag bytes.
    fn item_with_nbt(nbt: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.i16(1);
        buf.i8(1);
        buf.i16(0);
        buf.put(nbt);
        buf
    }

    #[test]
    fn read_slot_skips_lists_and_int_arrays() {
        let mut nbt = Vec::new();
        nbt.u8(10); // root compound
        nbt.u16(0);
        nbt.u8(9); // list "ench" of 2 compounds { short "id" }
        nbt.u16(4);
        nbt.put(b"ench");
        nbt.u8(10);
        nbt.i32(2);
        for id in [16i16, 34] {
            nbt.u8(2);
            nbt.u16(2);
            nbt.put(b"id");
            nbt.i16(id);
            nbt.u8(0);
        }
        nbt.u8(11); // int array "a" of 3
        nbt.u16(1);
        nbt.put(b"a");
        nbt.i32(3);
        for v in [1i32, 2, 3] {
            nbt.i32(v);
        }
        nbt.u8(0); // end root
        let mut buf = item_with_nbt(&nbt);
        buf.put(b"tail");
        let mut r: &[u8] = &buf;
        assert_eq!(read_slot(&mut r).unwrap().len(), buf.len() - 4);
        assert_eq!(r, b"tail");
    }

    #[test]
    fn read_slot_rejects_nesting_past_depth_limit() {
        // Unnamed-element lists nested 40 deep: list<list<...>> each holding one element.
        let mut nbt = vec![10, 0, 0, 9, 0, 0];
        for _ in 0..40 {
            nbt.u8(9);
            nbt.i32(1);
        }
        nbt.u8(1);
        nbt.i32(1);
        nbt.u8(0);
        assert!(matches!(
            read_slot(&mut &item_with_nbt(&nbt)[..]),
            Err(Error::Malformed(_))
        ));
        // Compounds nested 40 deep.
        let mut nbt = vec![10, 0, 0];
        for _ in 0..40 {
            nbt.put(&[10, 0, 0]);
        }
        nbt.extend(std::iter::repeat_n(0, 41));
        assert!(matches!(
            read_slot(&mut &item_with_nbt(&nbt)[..]),
            Err(Error::Malformed(_))
        ));
    }

    #[test]
    fn read_slot_rejects_negative_nbt_lengths() {
        for tag in [7u8, 9, 11] {
            let mut nbt = vec![10, 0, 0, tag, 0, 0];
            if tag == 9 {
                nbt.u8(1); // element type
            }
            nbt.i32(-1);
            nbt.u8(0);
            // Arrays throw in vanilla; a list just loops zero times.
            assert_eq!(
                read_slot(&mut &item_with_nbt(&nbt)[..]).is_err(),
                tag != 9,
                "tag {tag}"
            );
        }
    }

    #[test]
    fn read_slot_rejects_truncated_input() {
        let mut nbt = vec![10, 0, 0, 11, 0, 1, b'a'];
        nbt.i32(3);
        nbt.i32(1);
        nbt.i32(2);
        nbt.i32(3);
        nbt.u8(0);
        let buf = item_with_nbt(&nbt);
        for cut in 0..buf.len() {
            assert!(read_slot(&mut &buf[..cut]).is_err(), "cut at {cut}");
        }
        // A huge declared length must fail on truncation, not allocate.
        let mut nbt = vec![10, 0, 0, 11, 0, 0];
        nbt.i32(i32::MAX);
        assert!(matches!(
            read_slot(&mut &item_with_nbt(&nbt)[..]),
            Err(Error::Truncated)
        ));
    }

    #[test]
    fn position_round_trips_with_negatives() {
        for p in [
            Position { x: 0, y: 0, z: 0 },
            Position {
                x: 100,
                y: 64,
                z: -200,
            },
            Position {
                x: -33_554_432,
                y: 255,
                z: 33_554_431,
            },
            Position { x: 0, y: -1, z: 0 },
            Position {
                x: -1,
                y: -2048,
                z: -33_554_432,
            },
            Position {
                x: -7,
                y: 2047,
                z: -1,
            },
        ] {
            assert_eq!(Position::decode(p.encode()), p, "{p:?}");
        }
    }

    #[test]
    fn entity_teleport_round_trips() {
        let tp = EntityTeleport {
            eid: 12345,
            x: 32 * 10,
            y: 32 * 64,
            z: -32 * 20,
            yaw: 64,
            pitch: -10,
            on_ground: true,
        };
        assert_eq!(EntityTeleport::from_payload(&tp.to_payload()).unwrap(), tp);
    }

    #[test]
    fn multi_block_change_round_trips() {
        let mbc = MultiBlockChange {
            chunk_x: 3,
            chunk_z: -4,
            records: vec![
                BlockRecord {
                    horizontal: 0x5A,
                    y: 70,
                    block: 1 << 4,
                },
                BlockRecord {
                    horizontal: 0x00,
                    y: 0,
                    block: 35 << 4 | 14,
                },
            ],
        };
        assert_eq!(
            MultiBlockChange::from_payload(&mbc.to_payload()).unwrap(),
            mbc
        );
    }

    #[test]
    fn leading_entity_id_reads_past_packet_id() {
        // packet id 0x18, entity id 300 (varint 0xAC 0x02)
        let payload = [0x18, 0xAC, 0x02];
        assert_eq!(leading_entity_id(&payload).unwrap(), 300);
    }

    #[test]
    fn chunk_unload_detected() {
        let payload = chunk_data_payload(1, 2, true, 0, &[]);
        let header = ChunkDataHeader::parse(&payload).unwrap();
        assert!(header.is_unload());
        let loaded = chunk_data_payload(1, 2, true, 0b1, &[0u8; 100]);
        assert!(!ChunkDataHeader::parse(&loaded).unwrap().is_unload());
    }

    #[test]
    fn bulk_column_size_matches_popcount() {
        // Two sections, with sky light: 2 * (8192 + 2048 + 2048) + 256.
        assert_eq!(
            bulk_column_size(0b101, true),
            2 * (8192 + 2048 + 2048) + 256
        );
        // One section, no sky light (nether): 1 * (8192 + 2048) + 256.
        assert_eq!(bulk_column_size(0b1, false), 8192 + 2048 + 256);
    }

    #[test]
    fn split_bulk_into_columns() {
        let sky = true;
        let cols = [(0i32, 0i32, 0b1u16), (1, -1, 0b11)];
        let mut payload = Vec::new();
        payload.varint(id::MAP_CHUNK_BULK);
        payload.bool(sky);
        payload.length(cols.len());
        for (x, z, bm) in cols {
            payload.i32(x);
            payload.i32(z);
            payload.u16(bm);
        }
        let mut total = 0;
        for (_, _, bm) in cols {
            total += bulk_column_size(bm, sky);
        }
        payload.extend(std::iter::repeat_n(7u8, total));
        let split = split_map_chunk_bulk(&payload).unwrap();
        assert_eq!(split.len(), 2);
        assert_eq!((split[0].x, split[0].z, split[0].bitmask), (0, 0, 0b1));
        assert_eq!((split[1].x, split[1].z, split[1].bitmask), (1, -1, 0b11));
        // Each split column is a valid Chunk Data payload.
        let h = ChunkDataHeader::parse(&split[1].chunk_data).unwrap();
        assert_eq!((h.x, h.z, h.ground_up, h.bitmask), (1, -1, true, 0b11));
    }
}
