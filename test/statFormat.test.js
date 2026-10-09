const test = require('node:test');
const assert = require('node:assert');
const { getModeStats, formatCompactLine, formatWinstreakLine } = require('../utils/statFormat.js');
const { getStatValue } = require('../utils/stat-helper.js');
const { gameModeMap } = require('../utils/constants.js');

const player = {
    achievements: { bedwars_level: 123 },
    stats: {
        Bedwars: { wins_bedwars: 10, losses_bedwars: 5, final_kills_bedwars: 30, final_deaths_bedwars: 10, winstreak: 3, winstreak_best: 2 },
        Duels: { classic_duel_wins: 8, classic_duel_losses: 2, current_classic_duel_winstreak: 4 },
    },
};

test('getModeStats reads Bed Wars fields', () => {
    const s = getModeStats(player, gameModeMap.bedwars);
    assert.strictEqual(s.hasStats, true);
    assert.strictEqual(s.finalKills, 30);
    assert.strictEqual(s.levelText, '123✫');
    assert.strictEqual(s.bestWinstreak, 3, 'current streak above best counts as best');
    assert.match(formatCompactLine(s, 'Bedwars'), /FKDR §8» §f3\.00/);
});

test('getModeStats reads prefixed Duels fields', () => {
    const s = getModeStats(player, gameModeMap.classic);
    assert.strictEqual(s.wins, 8);
    assert.strictEqual(s.winstreak, 4);
});

test('missing game data yields zeros and hasStats=false', () => {
    const s = getModeStats(player, gameModeMap.skywars);
    assert.strictEqual(s.hasStats, false);
    assert.strictEqual(s.wins, 0);
});

test('winstreak line flags a disabled winstreak API', () => {
    assert.strictEqual(formatWinstreakLine({ wins: 10, losses: 1, winstreak: 0, bestWinstreak: 0 }), '§cWINSTREAK API DISABLED');
});

test('getStatValue computes ratios and returns null for unknown stats', () => {
    assert.deepStrictEqual(getStatValue(player, 'bedwars', 'fkdr'), { value: 3, name: 'FKDR' });
    assert.strictEqual(getStatValue(player, 'nope', 'fkdr'), null);
});
