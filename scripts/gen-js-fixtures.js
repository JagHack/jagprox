// Regenerates crates/jagprox-core/tests/fixtures/js_numbers.json from the real JS
// implementations, so the Rust port is checked against Node itself.
// Run: node scripts/gen-js-fixtures.js
const fs = require('fs');
const path = require('path');

const toFixed = [];
for (let n = 0; n <= 120; n++) {
    for (let d = 0; d <= 64; d++) {
        const v = n / (d || 1);
        toFixed.push([v, v.toFixed(2)]);
    }
}
for (const v of [0.005, 0.015, 0.025, 1.005, 2.675, 1.255, 10.235, 99.995, 1e6 / 3, 123456.785]) {
    toFixed.push([v, v.toFixed(2)]);
}

const toLocale = [];
for (const v of [0, 1, 12, 999, 1000, 1001, 12345, 999999, 1000000, 1234567, 98765432, 1.5, 1234.5678,
                 0.0005, 0.0015, 2.0005, 1e9 + 0.25, -1500, -0.25]) {
    toLocale.push([v, v.toLocaleString('en-US')]);
}

const out = path.join(__dirname, '../crates/jagprox-core/tests/fixtures/js_numbers.json');
fs.writeFileSync(out, JSON.stringify({ toFixed, toLocale }) + '\n');
console.log(`wrote ${toFixed.length + toLocale.length} cases to ${out}`);
