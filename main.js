const fs = require("fs");
const path = require("path");
const yaml = require("yaml");
const JagProx = require(path.join(__dirname, 'proxy.js'));

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

env = { jwt: process.env.JAGPROX_JWT };
if (!env.jwt || env.jwt.trim() === "") {
    console.error(`\n❌ FATAL ERROR: JAGPROX_JWT not found in your environment.`);
    console.error("   This is required for authenticating with the backend to get a Hypixel API key.");
    console.error("   Please ensure it's provided by the launcher.");
    process.exit(1);
} else {
    console.log("✓ JWT loaded successfully.");
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
