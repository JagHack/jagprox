const fs = require('fs');
const path = require('path');
const formatter = require('./formatter.js');
const { writeFileAtomic } = require('./utils/fileUtil.js');

// The bundled file is only the default seed; the user's copy lives next to config.yml
// (the same file the launcher's Aliases page edits).
const defaultAliasesPath = path.join(__dirname, 'aliases.json');
const aliasesPath = process.env.USER_DATA_PATH
    ? path.join(process.env.USER_DATA_PATH, 'aliases.json')
    : defaultAliasesPath;
let aliases = {};

function normalize(rawAliases) {
    const result = {};
    for (const [key, value] of Object.entries(rawAliases || {})) {
        if (typeof value === 'string') result[key.toLowerCase()] = value;
    }
    return result;
}

function loadAliases() {
    try {
        if (!fs.existsSync(aliasesPath)) {
            formatter.log(`aliases.json not found. Creating it from the defaults.`);
            const defaults = fs.existsSync(defaultAliasesPath) ? fs.readFileSync(defaultAliasesPath, 'utf8') : '{}';
            writeFileAtomic(aliasesPath, defaults);
        }
        let fileContent = fs.readFileSync(aliasesPath, 'utf8');
        if (fileContent.charCodeAt(0) === 0xFEFF) {
            fileContent = fileContent.slice(1);
        }
        aliases = normalize(JSON.parse(fileContent));
        formatter.log(`Successfully loaded ${Object.keys(aliases).length} aliases from aliases.json.`);
    } catch (error) {
        // Keep the previously loaded aliases if the file is mid-edit or invalid.
        formatter.log(`[ERROR] Failed to load or parse aliases.json: ${error.message}`);
    }
}

function saveAliases(newAliases) {
    try {
        writeFileAtomic(aliasesPath, JSON.stringify(newAliases, null, 4));
        aliases = normalize(newAliases);
    } catch (error) {
        formatter.log(`[ERROR] Failed to save aliases.json: ${error.message}`);
    }
}

function getAliases() {
    return aliases;
}

// Reload when the launcher saves new aliases, so no proxy restart is needed.
function watchAliases() {
    fs.watchFile(aliasesPath, { interval: 2000 }, (curr, prev) => {
        if (curr.mtimeMs !== prev.mtimeMs) loadAliases();
    }).unref();
}

loadAliases();

module.exports = {
    loadAliases,
    saveAliases,
    getAliases,
    watchAliases,
};
