const test = require('node:test');
const assert = require('node:assert');
const formatter = require('../formatter.js');

test('getRank prefers staff rank over purchased rank', () => {
    assert.strictEqual(formatter.getRank({ rank: 'ADMIN', newPackageRank: 'MVP_PLUS' }), 'ADMIN');
    assert.strictEqual(formatter.getRank({ rank: 'NORMAL', newPackageRank: 'MVP_PLUS' }), 'MVP_PLUS');
    assert.strictEqual(formatter.getRank({ monthlyPackageRank: 'SUPERSTAR', newPackageRank: 'MVP_PLUS' }), 'MVP_PLUS_PLUS');
    assert.strictEqual(formatter.getRank({}), 'NONE');
    assert.strictEqual(formatter.getRank(null), 'NONE');
});

test('formatRank renders the plus color', () => {
    assert.strictEqual(formatter.formatRank({ newPackageRank: 'MVP_PLUS', rankPlusColor: 'GOLD' }), '§b[MVP§6+§b]');
    assert.strictEqual(formatter.formatRank({ newPackageRank: 'VIP' }), '§a[VIP]');
    assert.strictEqual(formatter.formatRank({}), '§7');
});

test('extractText walks nested extra arrays', () => {
    assert.strictEqual(formatter.extractText({ text: 'a', extra: [{ text: 'b', extra: ['c'] }] }), 'abc');
    assert.strictEqual(formatter.chatToCleanText('{"text":"§aHi §bthere"}'), 'Hi there');
    assert.strictEqual(formatter.chatToCleanText('not json'), null);
});

test('reconstructLegacyText understands the "underlined" property', () => {
    assert.strictEqual(formatter.reconstructLegacyText({ text: 'x', color: 'red', underlined: true }), '§c§nx');
});
