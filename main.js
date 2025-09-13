const fs = require("fs");
const path = require("path");
const yaml = require("yaml");
const JagProx = require(path.join(__dirname, 'proxy.js'));

const userDataPath = process.env.USER_DATA_PATH || '.';
require("dotenv").config({ path: path.join(userDataPath, '.env') });

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

try {
    console.log(`Attempting to read config from ${configPath}...`);
    const configFile = fs.readFileSync(configPath, "utf8");
    config = yaml.parse(configFile);
    if (typeof config !== 'object' || config === null) {
        throw new Error("Parsed config is not a valid object.");
    }
    console.log("✓ config.yml loaded and parsed successfully.");
} catch (e) {
    console.error("\n❌ FATAL ERROR: Could not read or parse config.yml.");
    console.error("   Please ensure the file exists, is not empty, and uses valid YAML syntax.");
    console.error("   Details:", e.message);
    process.exit(1);
}

env = { apiKey: process.env.HYPIXEL_API_KEY };
if (!env.apiKey || env.apiKey.trim() === "") {
    console.error(`\n❌ FATAL ERROR: HYPIXEL_API_KEY not found in your .env file in ${userDataPath}.`);
    console.error("   Please set it using the launcher UI.");
    process.exit(1);
} else {
    console.log("✓ Hypixel API key loaded successfully.");
}

try {
    console.log("Initializing JagProx Minecraft proxy on port " + (config.port || 2107) + "...");
    const proxy = new JagProx(config, env);
    console.log("✓ Minecraft proxy initialized.");

} catch (err) {
    console.error("\n❌ FATAL ERROR: An error occurred during server startup.");
    console.error("   Details:", err.message);
    console.error("   This could be because the proxy port (" + (config.port || 2107) + ") is already in use by another application.");
    process.exit(1);
}