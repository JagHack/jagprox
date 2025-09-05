// main.js - REVISED AND MORE ROBUST
require("dotenv").config(); // Removed 'quiet' to see potential .env file errors
const fs = require("fs");
const yaml = require("yaml");
const JagProx = require("./proxy.js");
const { startWebPanel } = require("./web-panel.js");

let config, env;

// --- Step 1: Validate config.yml ---
try {
    console.log("Attempting to read config.yml...");
    const configFile = fs.readFileSync("./config.yml", "utf8");
    config = yaml.parse(configFile);
    if (typeof config !== 'object' || config === null) {
        throw new Error("Parsed config is not a valid object.");
    }
    console.log("✓ config.yml loaded and parsed successfully.");
} catch (e) {
    console.error("\n❌ FATAL ERROR: Could not read or parse config.yml.");
    console.error("   Please ensure the file exists, is not empty, and uses valid YAML syntax.");
    console.error("   Details:", e.message);
    process.exit(1); // Exit with an error code
}

// --- Step 2: Validate API Key from .env file ---
env = { apiKey: process.env.HYPIXEL_API_KEY };
if (!env.apiKey || env.apiKey.trim() === "") {
    console.error("\n❌ FATAL ERROR: HYPIXEL_API_KEY not found in your .env file.");
    console.error("   Please create a file named '.env' in the project directory.");
    console.error("   Inside the .env file, add this line:");
    console.error("   HYPIXEL_API_KEY=your_actual_api_key_here");
    process.exit(1);
} else {
    console.log("✓ Hypixel API key loaded successfully.");
}

// --- Step 3: Start the proxy and web server ---
try {
    console.log("Initializing JagProx Minecraft proxy on port " + (config.port || 2107) + "...");
    const proxy = new JagProx(config, env);
    console.log("✓ Minecraft proxy initialized.");

    console.log("Starting web panel...");
    // Pass the fully initialized proxy instance to the web panel
    startWebPanel(2108, config, env, proxy);

} catch (err) {
    console.error("\n❌ FATAL ERROR: An error occurred during server startup.");
    console.error("   Details:", err.message);
    console.error("   This could be because the proxy port (" + (config.port || 2107) + ") is already in use by another application.");
    process.exit(1);
}