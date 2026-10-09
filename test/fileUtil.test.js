const test = require('node:test');
const assert = require('node:assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const yaml = require('yaml');
const { syncConfigFile, mergeMissingKeys } = require('../utils/fileUtil.js');

function tmpDir() {
    return fs.mkdtempSync(path.join(os.tmpdir(), 'jagprox-test-'));
}

test('mergeMissingKeys adds new keys without touching existing values', () => {
    const user = { port: 1234, auto_gg: { enabled: true } };
    const changed = mergeMissingKeys(user, { port: 2107, host: '127.0.0.1', auto_gg: { enabled: false, delay: 1500 } });
    assert.strictEqual(changed, true);
    assert.deepStrictEqual(user, { port: 1234, host: '127.0.0.1', auto_gg: { enabled: true, delay: 1500 } });
});

test('syncConfigFile backs up an unparseable config and leaves it untouched', () => {
    const dir = tmpDir();
    const defaults = path.join(dir, 'default.yml');
    const user = path.join(dir, 'config.yml');
    fs.writeFileSync(defaults, 'port: 2107\n');
    fs.writeFileSync(user, 'nicknames: {Steve: "Bob"\n  broken: [');

    const result = syncConfigFile(user, defaults);
    assert.strictEqual(result.status, 'unparseable');
    assert.ok(fs.existsSync(result.backupPath));
    assert.strictEqual(fs.readFileSync(user, 'utf8'), 'nicknames: {Steve: "Bob"\n  broken: [');
});

test('syncConfigFile creates and updates configs', () => {
    const dir = tmpDir();
    const defaults = path.join(dir, 'default.yml');
    const user = path.join(dir, 'config.yml');
    fs.writeFileSync(defaults, 'port: 2107\nhost: 127.0.0.1\n');

    assert.strictEqual(syncConfigFile(user, defaults).status, 'created');
    fs.writeFileSync(user, 'port: 9999\n');
    assert.strictEqual(syncConfigFile(user, defaults).status, 'updated');
    assert.deepStrictEqual(yaml.parse(fs.readFileSync(user, 'utf8')), { port: 9999, host: '127.0.0.1' });
    assert.strictEqual(syncConfigFile(user, defaults).status, 'unchanged');
});
