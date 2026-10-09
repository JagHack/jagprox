// Small in-memory cache with per-entry expiry and a size cap (oldest entry is evicted first).
class TTLCache {
    constructor({ ttl, maxSize = 500 }) {
        this.ttl = ttl;
        this.maxSize = maxSize;
        this.map = new Map();
    }

    get(key) {
        const entry = this.map.get(key);
        if (!entry) return undefined;
        if (Date.now() > entry.expires) {
            this.map.delete(key);
            return undefined;
        }
        return entry.value;
    }

    set(key, value) {
        this.map.delete(key);
        this.map.set(key, { value, expires: Date.now() + this.ttl });
        if (this.map.size > this.maxSize) {
            this.map.delete(this.map.keys().next().value);
        }
    }

    delete(key) {
        this.map.delete(key);
    }

    clear() {
        this.map.clear();
    }

    // Returns the cached promise for `key`, or starts `loader()` and caches its promise.
    // Concurrent callers share one in-flight request. null/undefined results and
    // rejections are not kept, so the next call retries.
    getOrLoad(key, loader) {
        const cached = this.get(key);
        if (cached !== undefined) return cached;

        const promise = Promise.resolve()
            .then(loader)
            .then(
                (value) => {
                    if (value == null && this.map.get(key)?.value === promise) this.map.delete(key);
                    return value;
                },
                (err) => {
                    if (this.map.get(key)?.value === promise) this.map.delete(key);
                    throw err;
                }
            );
        this.set(key, promise);
        return promise;
    }
}

// Runs `fn` over `items` with at most `limit` calls in flight, preserving result order.
async function mapLimit(items, limit, fn) {
    const results = new Array(items.length);
    let next = 0;
    const workers = Array.from({ length: Math.min(limit, items.length) }, async () => {
        while (next < items.length) {
            const index = next++;
            results[index] = await fn(items[index], index);
        }
    });
    await Promise.all(workers);
    return results;
}

module.exports = { TTLCache, mapLimit };
