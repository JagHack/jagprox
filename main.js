require("dotenv").config({ quiet: true });
const fs = require("fs");
const yaml = require("yaml");
const JagProx = require("./proxy.js");
const { startWebPanel } = require("./web-panel.js");

let config;
try {
    config = yaml.parse(fs.readFileSync("./config.yml", "utf8"));
} catch (e) {
    console.log("Could not read or parse config.yml:", e.message);
    process.exit();
}

let env = { apiKey: process.env.HYPIXEL_API_KEY };
if (!env.apiKey) {
    console.log("API key not found in .env file.");
    process.exit();
}

const proxy = new JagProx(config, env);
startWebPanel(2108, config, env);