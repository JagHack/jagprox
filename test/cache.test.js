const test = require('node:test');
const assert = require('node:assert');
const { TTLCache, mapLimit } = require('../utils/cache.js');

test('getOrLoad shares one in-flight request', async () => {
    const cache = new TTLCache({ ttl: 1000 });
    let calls = 0;
    const loader = async () => { calls++; return 'value'; };
    const [a, b] = await Promise.all([cache.getOrLoad('k', loader), cache.getOrLoad('k', loader)]);
    assert.strictEqual(a, 'value');
    assert.strictEqual(b, 'value');
    assert.strictEqual(calls, 1);
});

test('null results and errors are not cached', async () => {
    const cache = new TTLCache({ ttl: 1000 });
    let calls = 0;
    await cache.getOrLoad('k', async () => { calls++; return null; });
    await cache.getOrLoad('k', async () => { calls++; return null; });
    assert.strictEqual(calls, 2);
    await assert.rejects(cache.getOrLoad('e', async () => { throw new Error('boom'); }));
    assert.strictEqual(await cache.getOrLoad('e', async () => 'ok'), 'ok');
});

test('entries expire and the size cap evicts the oldest', async () => {
    const cache = new TTLCache({ ttl: 5, maxSize: 2 });
    cache.set('a', 1);
    cache.set('b', 2);
    cache.set('c', 3);
    assert.strictEqual(cache.get('a'), undefined);
    await new Promise(r => setTimeout(r, 10));
    assert.strictEqual(cache.get('c'), undefined);
});

test('mapLimit keeps order and respects the limit', async () => {
    let running = 0, peak = 0;
    const result = await mapLimit([1, 2, 3, 4, 5], 2, async (n) => {
        running++; peak = Math.max(peak, running);
        await new Promise(r => setTimeout(r, 5));
        running--;
        return n * 2;
    });
    assert.deepStrictEqual(result, [2, 4, 6, 8, 10]);
    assert.ok(peak <= 2);
});
