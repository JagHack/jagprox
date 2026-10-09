const test = require('node:test');
const assert = require('node:assert');
const { parseOnlineLine, parseTeamLine, parsePartyLine, detectGameResult } = require('../utils/chatParsers.js');

test('parseOnlineLine splits /who output and strips ranks and trailing dots', () => {
    assert.deepStrictEqual(parseOnlineLine('ONLINE: Alice, [MVP+] Bob, Carol.'), ['Alice', 'Bob', 'Carol']);
    assert.strictEqual(parseOnlineLine('Something else'), null);
});

test('parseTeamLine reads team-mode /who lines', () => {
    assert.deepStrictEqual(parseTeamLine('Team #1: Alice, Bob'), ['Alice', 'Bob']);
    assert.strictEqual(parseTeamLine('ONLINE: Alice'), null);
});

test('parsePartyLine handles ● separators and rank prefixes', () => {
    assert.deepStrictEqual(parsePartyLine('Party Leader: [MVP+] Leader ●'), ['Leader']);
    assert.deepStrictEqual(parsePartyLine('Party Members: [VIP] Alice ● Bob ● [MVP++] Carol ●'), ['Alice', 'Bob', 'Carol']);
    assert.strictEqual(parsePartyLine('Party Members (3)'), null);
});

test('detectGameResult classifies WINNER! lines', () => {
    assert.strictEqual(detectGameResult('Red Team WINNER!'), 'loss');
    assert.strictEqual(detectGameResult('Steve WINNER!'), 'win');
    assert.strictEqual(detectGameResult('WINNER! Steve'), 'win');
    assert.strictEqual(detectGameResult('[MVP+] Steve: WINNER!'), null, 'player chat is ignored');
    assert.strictEqual(detectGameResult('Click to return to LOBBY WINNER!'), null);
    assert.strictEqual(detectGameResult('Just a normal message'), null);
});
