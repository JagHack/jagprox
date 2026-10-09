const fs = require('fs');
const path = require('path');
const yaml = require('yaml');

// Write to a temp file in the same directory, then rename over the target, so a crash
// mid-write never leaves a half-written config behind.
function writeFileAtomic(filePath, content) {
    const tmpPath = path.join(path.dirname(filePath), `.${path.basename(filePath)}.${process.pid}.tmp`);
    fs.writeFileSync(tmpPath, content, 'utf8');
    fs.renameSync(tmpPath, filePath);
}

function backupFile(filePath) {
    const stamp = new Date().toISOString().replace(/[:.]/g, '-');
    const backupPath = `${filePath}.broken-${stamp}.bak`;
    fs.copyFileSync(filePath, backupPath);
    return backupPath;
}

// Adds keys that exist in `source` but are missing from `target` (recursively for plain objects).
// Never overwrites values the user already has. Returns true if anything was added.
function mergeMissingKeys(target, source) {
    let updated = false;
    for (const key of Object.keys(source)) {
        const value = source[key];
        if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
            if (target[key] === undefined || target[key] === null || typeof target[key] !== 'object' || Array.isArray(target[key])) {
                target[key] = {};
                updated = true;
            }
            if (mergeMissingKeys(target[key], value)) updated = true;
        } else if (target[key] === undefined) {
            target[key] = value;
            updated = true;
        }
    }
    return updated;
}

// Brings the user's config.yml up to date with keys added to the bundled default.
// A user config that fails to parse is backed up and left untouched, never replaced.
// Returns { status: 'created' | 'updated' | 'unchanged' | 'unparseable', backupPath? }.
function syncConfigFile(userPath, defaultPath) {
    const defaultConfig = yaml.parse(fs.readFileSync(defaultPath, 'utf8')) || {};

    if (!fs.existsSync(userPath)) {
        writeFileAtomic(userPath, yaml.stringify(defaultConfig));
        return { status: 'created' };
    }

    let userConfig;
    try {
        userConfig = yaml.parse(fs.readFileSync(userPath, 'utf8'));
    } catch (e) {
        return { status: 'unparseable', backupPath: backupFile(userPath), error: e };
    }
    if (userConfig === null || userConfig === undefined) userConfig = {};
    if (typeof userConfig !== 'object' || Array.isArray(userConfig)) {
        return { status: 'unparseable', backupPath: backupFile(userPath), error: new Error('config.yml is not a mapping') };
    }

    if (mergeMissingKeys(userConfig, defaultConfig)) {
        writeFileAtomic(userPath, yaml.stringify(userConfig));
        return { status: 'updated' };
    }
    return { status: 'unchanged' };
}

// Read-modify-write of config.yml. Throws (and writes nothing) if the file can't be parsed.
function updateConfigFile(configPath, mutate) {
    const config = yaml.parse(fs.readFileSync(configPath, 'utf8')) || {};
    mutate(config);
    writeFileAtomic(configPath, yaml.stringify(config));
    return config;
}

module.exports = { writeFileAtomic, backupFile, mergeMissingKeys, syncConfigFile, updateConfigFile };
