const fs = require('fs');
const path = require('path');
const formatter = require('./formatter.js');

const aliasesPath = path.join(__dirname, 'aliases.json');
let aliases = {};

function loadAliases() {
    try {
        if (fs.existsSync(aliasesPath)) {
            let fileContent = fs.readFileSync(aliasesPath, 'utf8');
            if (fileContent.charCodeAt(0) === 0xFEFF) {
                fileContent = fileContent.slice(1);
            }
            aliases = JSON.parse(fileContent);
            formatter.log(`Successfully loaded ${Object.keys(aliases).length} aliases from aliases.json.`);
        } else {
            formatter.log(`aliases.json not found. Creating a new one.`);
            saveAliases({});
        }
    } catch (error) {
        formatter.log(`[ERROR] Failed to load or parse aliases.json: ${error.message}`);
        aliases = {};
    }
}

function saveAliases(newAliases) {
    try {
        aliases = newAliases;
        fs.writeFileSync(aliasesPath, JSON.stringify(newAliases, null, 4), 'utf8');
    } catch (error) {
        formatter.log(`[ERROR] Failed to save aliases.json: ${error.message}`);
    }
}

function getAliases() {
    return aliases;
}

loadAliases();

module.exports = {
    loadAliases,
    saveAliases,
    getAliases
};
