const test = require('node:test');
const assert = require('node:assert');
const { replaceNamesInText, replaceNamesInComponent } = require('../utils/nicknames.js');

test('replaces whole names only', () => {
    assert.strictEqual(replaceNamesInText('Bob and Bobby', { Bob: 'Nick' }), 'Nick and Bobby');
});

test('regex characters in names are treated literally', () => {
    assert.strictEqual(replaceNamesInText('a+ said hi', { 'a+': 'X' }), 'X said hi');
    assert.doesNotThrow(() => replaceNamesInText('text', { '(': 'X' }));
});

test('replaces every configured name throughout a component tree', () => {
    const component = { text: 'Alice vs ', extra: [{ text: 'Bob' }] };
    replaceNamesInComponent(component, { Alice: 'A', Bob: 'B' });
    assert.deepStrictEqual(component, { text: 'A vs ', extra: [{ text: 'B' }] });
});
