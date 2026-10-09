const fs = require("fs");
const path = require("path");
const yaml = require("yaml");
const formatter = require("./formatter.js");
const aliasManager = require("./aliasManager.js");
const { DEFAULT_PROXY_PORT } = require("./utils/constants.js");
const JagProx = require(path.join(__dirname, 'proxy.js'));

// A bug in one handler should be logged, not kill the player's Hypixel session.
process.on('uncaughtException', (err) => {
    console.error('[ERROR] Uncaught exception (proxy keeps running):', err);
});

process.on('unhandledRejection', (reason) => {
    console.error('[ERROR] Unhandled promise rejection (proxy keeps running):', reason);
});

const userDataPath = process.env.USER_DATA_PATH || '.';

let config, env;
const configPath = path.join(userDataPath, "config.yml");
const defaultConfigPath = path.join(__dirname, "config.yml");

if (!fs.existsSync(configPath)) {
    console.log(`Configuration file not found in user data. Copying default config...`);
    try {
        fs.copyFileSync(defaultConfigPath, configPath);
        console.log(`Successfully copied default config to ${configPath}`);
    } catch (e) {
        console.error(`\n❌ FATAL ERROR: Could not copy default configuration file.`);
        console.error(`   Source: ${defaultConfigPath}`);
        console.error(`   Destination: ${configPath}`);
        console.error(`   Details: ${e.message}`);
        process.exit(1);
    }
}

function readConfig() {
    const parsed = yaml.parse(fs.readFileSync(configPath, "utf8"));
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        throw new Error("Parsed config is not a valid object.");
    }
    return parsed;
}

try {
    console.log(`Attempting to read config from ${configPath}...`);
    config = readConfig();
    formatter.setDebug(config.debug_mode);
    console.log("✓ config.yml loaded and parsed successfully.");
} catch (e) {
    console.error("\n❌ FATAL ERROR: Could not read or parse config.yml.");
    console.error("   Please ensure the file exists, is not empty, and uses valid YAML syntax.");
    console.error("   Details:", e.message);
    process.exit(1);
}

env = { jwt: process.env.JAGPROX_JWT };
if (!env.jwt || env.jwt.trim() === "" || env.jwt === "undefined") {
    console.error(`\n❌ FATAL ERROR: JAGPROX_JWT not found in your environment.`);
    console.error("   This is required for authenticating with the backend to get a Hypixel API key.");
    console.error("   Please ensure it's provided by the launcher.");
    process.exit(1);
} else {
    console.log("✓ JWT loaded successfully.");
}

const port = config.port || DEFAULT_PROXY_PORT;
try {
    console.log(`Initializing JagProx Minecraft proxy on port ${port}...`);
    new JagProx(config, env);
    console.log("✓ Minecraft proxy initialized.");
} catch (err) {
    console.error("\n❌ FATAL ERROR: An error occurred during server startup.");
    console.error("   Details:", err.message);
    process.exit(1);
}

// Pick up settings saved by the launcher (Auto GG, alerts, ...) without a restart.
// The proxy holds this same object, so it's updated in place. Port/host need a restart.
fs.watchFile(configPath, { interval: 2000 }, (curr, prev) => {
    if (curr.mtimeMs === prev.mtimeMs) return;
    try {
        const fresh = readConfig();
        for (const key of Object.keys(config)) {
            if (!(key in fresh)) delete config[key];
        }
        Object.assign(config, fresh);
        formatter.setDebug(config.debug_mode);
        formatter.log('config.yml changed on disk, settings reloaded.');
    } catch (e) {
        formatter.log(`[ERROR] config.yml changed but could not be reloaded (keeping old settings): ${e.message}`);
    }
}).unref();
aliasManager.watchAliases();
