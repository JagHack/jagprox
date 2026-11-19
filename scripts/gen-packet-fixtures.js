// Regenerates crates/mcproto/tests/fixtures/packets.json: every packet the Rust codec
// handles, encoded by node-minecraft-protocol (the library the JS proxy uses).
// Run: node scripts/gen-packet-fixtures.js
const fs = require('fs');
const path = require('path');
const mc = require('minecraft-protocol');

const VERSION = '1.8.9';
const uuidA = '069a79f4-44e9-4726-a5be-fca90e38aaf5';
const uuidB = '853c80ef-3c37-49fd-aa49-938b674adae6';
const chat = JSON.stringify({ text: '§aHello ', extra: [{ text: 'world ✫', color: 'gold' }] });

// [state, isServer (true = clientbound), name, params]
const cases = [
    ['handshaking', false, 'set_protocol', { protocolVersion: 47, serverHost: 'mc.hypixel.net', serverPort: 25565, nextState: 2 }],
    ['status', false, 'ping_start', {}],
    ['status', false, 'ping', { time: [0x01020304, 0x05060708] }],
    ['status', true, 'server_info', { response: '{"version":{"name":"1.8.9","protocol":47}}' }],
    ['status', true, 'ping', { time: [0, 42] }],
    ['login', false, 'login_start', { username: 'JagHack' }],
    ['login', false, 'encryption_begin', { sharedSecret: Buffer.alloc(128, 7), verifyToken: Buffer.alloc(128, 9) }],
    ['login', true, 'disconnect', { reason: '{"text":"bye"}' }],
    ['login', true, 'encryption_begin', { serverId: '', publicKey: Buffer.alloc(162, 1), verifyToken: Buffer.from([1, 2, 3, 4]) }],
    ['login', true, 'success', { uuid: uuidA, username: 'Notch' }],
    ['login', true, 'compress', { threshold: 256 }],
    ['play', true, 'login', { entityId: 1234, gameMode: 2, dimension: -1, difficulty: 1, maxPlayers: 100, levelType: 'default', reducedDebugInfo: false }],
    ['play', true, 'chat', { message: chat, position: 1 }],
    ['play', true, 'respawn', { dimension: 1, difficulty: 2, gamemode: 0, levelType: 'flat' }],
    ['play', true, 'position', { x: 1.5, y: 64, z: -1000.25, yaw: 90, pitch: -12.5, flags: 0 }],
    ['play', true, 'entity_destroy', { entityIds: [1, 300, 70000] }],
    ['play', true, 'named_sound_effect', { soundName: 'random.orb', x: 80, y: 512, z: -24, volume: 1, pitch: 63 }],
    ['play', true, 'player_info', { action: 'add_player', data: [
        { uuid: uuidA, name: 'Notch', properties: [{ name: 'textures', value: 'dGV4', signature: 'c2ln' }], gamemode: 1, ping: 35, displayName: chat },
        { uuid: uuidB, name: 'jeb_', properties: [{ name: 'textures', value: 'eA==' }], gamemode: 0, ping: 120 },
    ] }],
    ['play', true, 'player_info', { action: 'update_game_mode', data: [{ uuid: uuidA, gamemode: 3 }] }],
    ['play', true, 'player_info', { action: 'update_latency', data: [{ uuid: uuidA, ping: 500 }, { uuid: uuidB, ping: 1 }] }],
    ['play', true, 'player_info', { action: 'update_display_name', data: [{ uuid: uuidA, displayName: chat }, { uuid: uuidB }] }],
    ['play', true, 'player_info', { action: 'remove_player', data: [{ uuid: uuidA }, { uuid: uuidB }] }],
    ['play', true, 'scoreboard_objective', { name: 'sidebar', action: 0, displayText: '§e§lBED WARS', type: 'integer' }],
    ['play', true, 'scoreboard_objective', { name: 'sidebar', action: 1 }],
    ['play', true, 'scoreboard_team', { team: 'jp1', mode: 0, name: 'jp1', prefix: '§7[12✫] ', suffix: ' §8|§f1.50', friendlyFire: 3, nameTagVisibility: 'always', color: 15, players: ['Notch', 'jeb_'] }],
    ['play', true, 'scoreboard_team', { team: 'jp1', mode: 1 }],
    ['play', true, 'scoreboard_team', { team: '§cRed', mode: 2, name: 'Red', prefix: '§c', suffix: '', friendlyFire: 0, nameTagVisibility: 'hideForOtherTeams', color: 12 }],
    ['play', true, 'scoreboard_team', { team: 'jp1', mode: 3, players: ['Dinnerbone'] }],
    ['play', true, 'scoreboard_team', { team: 'jp1', mode: 4, players: [] }],
    ['play', true, 'custom_payload', { channel: 'MC|Brand', data: Buffer.from('\x07Hypixel BungeeCord') }],
    ['play', true, 'kick_disconnect', { reason: '{"text":"Kicked"}' }],
    ['play', true, 'title', { action: 0, text: '{"text":"VICTORY!"}' }],
    ['play', true, 'title', { action: 1, text: '{"text":"sub"}' }],
    ['play', true, 'title', { action: 2, fadeIn: 10, stay: 70, fadeOut: 20 }],
    ['play', true, 'title', { action: 3 }],
    ['play', true, 'title', { action: 4 }],
    ['play', true, 'playerlist_header', { header: '{"text":"Hypixel"}', footer: '{"text":"store.hypixel.net"}' }],
    ['play', false, 'chat', { message: '/sc bw Notch' }],
    ['play', false, 'custom_payload', { channel: 'MC|Brand', data: Buffer.from('\x05lunar') }],
];

const fixtures = cases.map(([state, isServer, name, params]) => {
    const serializer = mc.createSerializer({ state, isServer, version: VERSION });
    const buffer = serializer.createPacketBuffer({ name, params });
    return { state, clientbound: isServer, name, hex: buffer.toString('hex') };
});

// A real named_entity_spawn (only the leading entity id + UUID are decoded in Rust).
const spawn = mc.createSerializer({ state: 'play', isServer: true, version: VERSION }).createPacketBuffer({
    name: 'named_entity_spawn',
    params: { entityId: 4242, playerUUID: uuidB, x: 32, y: 2048, z: -64, yaw: 0, pitch: 0, currentItem: 0, metadata: [{ key: 6, type: 3, value: 20 }] },
});
fixtures.push({ state: 'play', clientbound: true, name: 'named_entity_spawn', hex: spawn.toString('hex') });

const out = path.join(__dirname, '../crates/mcproto/tests/fixtures/packets.json');
fs.writeFileSync(out, JSON.stringify(fixtures, null, 1) + '\n');
console.log(`wrote ${fixtures.length} packets to ${out}`);
