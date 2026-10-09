const test = require('node:test');
const assert = require('node:assert');
const { matchesTrackedModes } = require('../modules/superFriendTracker.js');

test('matches gamemode aliases against Hypixel game types', () => {
    assert.ok(matchesTrackedModes({ gameType: 'BEDWARS' }, ['bw']));
    assert.ok(matchesTrackedModes({ gameType: 'SKYWARS' }, ['skywars', 'duels']));
    assert.ok(matchesTrackedModes({ gameType: 'PIT' }, ['any']));
    assert.ok(!matchesTrackedModes({ gameType: 'DUELS' }, ['bedwars']));
    assert.ok(!matchesTrackedModes({ gameType: 'DUELS' }, []));
});
