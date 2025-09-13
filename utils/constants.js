const mcColors = [
    { code: '§0', rgb: [0, 0, 0] }, { code: '§1', rgb: [0, 0, 170] },
    { code: '§2', rgb: [0, 170, 0] }, { code: '§3', rgb: [0, 170, 170] },
    { code: '§4', rgb: [170, 0, 0] }, { code: '§5', rgb: [170, 0, 170] },
    { code: '§6', rgb: [255, 170, 0] }, { code: '§7', rgb: [170, 170, 170] },
    { code: '§8', rgb: [85, 85, 85] }, { code: '§9', rgb: [85, 85, 255] },
    { code: '§a', rgb: [85, 255, 85] }, { code: '§b', rgb: [85, 255, 255] },
    { code: '§c', rgb: [255, 85, 85] }, { code: '§d', rgb: [255, 85, 255] },
    { code: '§e', rgb: [255, 255, 85] }, { code: '§f', rgb: [255, 255, 255] }
];

function findClosestMinecraftColor(r, g, b) {
    let closest = mcColors[0];
    let minDistance = Infinity;
    for (const color of mcColors) {
        const distance = Math.sqrt(Math.pow(r - color.rgb[0], 2) + Math.pow(g - color.rgb[1], 2) + Math.pow(b - color.rgb[2], 2));
        if (distance < minDistance) {
            minDistance = distance;
            closest = color;
        }
    }
    return closest.code;
}

const commandAliases = {
    "/play solobw": "/play bedwars_eight_one",
    "/play doublesbw": "/play bedwars_eight_two",
    "/play 3sbw": "/play bedwars_four_three",
    "/play 4sbw": "/play bedwars_four_four",
    "/play 4v4bw": "/play bedwars_two_four",
    "/play castlebw": "/play bedwars_castle",
    "/play solosw": "/play skywars_solo_normal",
    "/play doublesinsanesw": "/play skywars_teams_insane",
    "/play doublesnormalsw": "/play skywars_teams_normal",
    "/play classicduels": "/play duels_classic_duel",
    "/play bridgeduels": "/play duels_bridge_duel",
    "/play uhcduels": "/play duels_uhc_duel",
    "/play skywarsduels": "/play duels_sw_duel",
    "/play sumoduels": "/play duels_sumo_duel",
    "/play bowduels": "/play duels_bow_duel",
    "/play comboduels": "/play duels_combo_duel",
    "/play opduels": "/play duels_op_duel",
};

const duelsModes = {
    general: { apiName: "Duels", displayName: "Duels", prefix: "" },
    classic: { apiName: "Duels", displayName: "Classic Duels", prefix: "classic_duel" },
    bridge: { apiName: "Duels", displayName: "Bridge Duels", prefix: "bridge_duel" },
    uhc: { apiName: "Duels", displayName: "UHC Duels", prefix: "uhc_duel" },
    skywars: { apiName: "Duels", displayName: "SkyWars Duels", prefix: "sw_duel" },
    sumo: { apiName: "Duels", displayName: "Sumo Duels", prefix: "sumo_duel" },
    bow: { apiName: "Duels", displayName: "Bow Duels", prefix: "bow_duel" },
    combo: { apiName: "Duels", displayName: "Combo Duels", prefix: "combo_duel" },
    op: { apiName: "Duels", displayName: "OP Duels", prefix: "op_duel" }
};

const gameModeMap = {
    "arcade": { apiName: "Arcade", displayName: "Arcade" },
    "arena": { apiName: "Arena", displayName: "Arena Brawl" },
    "bedwars": { apiName: "Bedwars", displayName: "Bed Wars" },
    "bw": { apiName: "Bedwars", displayName: "Bed Wars" },
    "blitz": { apiName: "HungerGames", displayName: "Blitz SG" },
    "sg": { apiName: "HungerGames", displayName: "Blitz SG" },
    "buildbattle": { apiName: "BuildBattle", displayName: "Build Battle" },
    "bb": { apiName: "BuildBattle", displayName: "Build Battle" },
    "cvc": { apiName: "MCGO", displayName: "Cops and Crims" },
    "crazywalls": { apiName: "TrueCombat", displayName: "Crazy Walls" },
    "cw": { apiName: "TrueCombat", displayName: "Crazy Walls" },
    "duels": duelsModes.general,
    "classic": duelsModes.classic, "classicduels": duelsModes.classic,
    "bridge": duelsModes.bridge, "bridgeduels": duelsModes.bridge,
    "uhc": duelsModes.uhc, "uhcduels": duelsModes.uhc,
    "skywarsduels": duelsModes.skywars,
    "sumo": duelsModes.sumo, "sumoduels": duelsModes.sumo,
    "bow": duelsModes.bow, "bowduels": duelsModes.bow,
    "combo": duelsModes.combo, "comboduels": duelsModes.combo,
    "op": duelsModes.op, "opduels": duelsModes.op,
    "megawalls": { apiName: "Walls3", displayName: "Mega Walls" },
    "mw": { apiName: "Walls3", displayName: "Mega Walls" },
    "murder": { apiName: "MurderMystery", displayName: "Murder Mystery" },
    "mm": { apiName: "MurderMystery", displayName: "Murder Mystery" },
    "paintball": { apiName: "Paintball", displayName: "Paintball" },
    "pit": { apiName: "Pit", displayName: "The Pit" },
    "quake": { apiName: "Quake", displayName: "Quake" },
    "quakecraft": { apiName: "Quake", displayName: "Quake" },
    "skyclash": { apiName: "SkyClash", displayName: "SkyClash" },
    "skywars": { apiName: "SkyWars", displayName: "SkyWars" },
    "sw": { apiName: "SkyWars", displayName: "SkyWars" },
    "smash": { apiName: "SuperSmash", displayName: "Smash Heroes" },
    "speeduhc": { apiName: "SpeedUHC", displayName: "Speed UHC" },
    "tnt": { apiName: "TNTGames", displayName: "TNT Games" },
    "tkr": { apiName: "GingerBread", displayName: "Turbo Kart Racers" },
    "turbokartracers": { apiName: "GingerBread", displayName: "Turbo Kart Racers" },
    "uhcchampions": { apiName: "UHC", displayName: "UHC Champions" },
    "vampirez": { apiName: "VampireZ", displayName: "VampireZ" },
    "walls": { apiName: "Walls", displayName: "Walls" },
    "warlords": { apiName: "Battleground", displayName: "Warlords" },
    "wool": { apiName: "WoolGames", displayName: "Wool Wars" },
    "woolwars": { apiName: "WoolGames", displayName: "Wool Wars" },
};

const quickQueueMap = {
    'bw': { name: 'Bed Wars Solos', command: '/play bedwars_eight_one' },
    'bwd': { name: 'Bed Wars Doubles', command: '/play bedwars_eight_two' },
    'sw': { name: 'SkyWars Solos', command: '/play skywars_solo_normal' },
    'swd': { name: 'SkyWars Doubles', command: '/play skywars_teams_normal' },
    'mw': { name: 'Mega Walls', command: '/play mega_walls' },
    'quake': { name: 'Quakecraft Solo', command: '/play quake_solo' },
    'blitz': { name: 'Blitz SG Solo', command: '/play blitz_solo_no_kit' },
    'sg': { name: 'Blitz SG Solo', command: '/play blitz_solo_no_kit' },
    'uhc': { name: 'UHC Champions', command: '/play uhc_solo' },
    'speeduhc': { name: 'Speed UHC', command: '/play speed_uhc_solo_normal' },
    'mm': { name: 'Murder Mystery', command: '/play murder_mystery_classic' },
    'bb': { name: 'Build Battle Solo', command: '/play build_battle_solo' },
    'duels': { name: 'Duels Lobby', command: '/lobby duels' },
    'wool': { name: 'Wool Wars', command: '/play wool_wars_two_four' },
    'pit': { name: 'The Pit', command: '/play pit' },
    'cvc': { name: 'Cops and Crims', command: '/play mcgo' },
    'tnt': { name: 'TNT Games', command: '/lobby tntgames' },
};

module.exports = {
    mcColors,
    findClosestMinecraftColor,
    commandAliases,
    gameModeMap,
    quickQueueMap
};