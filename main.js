require("dotenv").config({ quiet: true });
const fs = require("fs");
const yaml = require("yaml");
// Wir gehen davon aus, dass proxy.js nun FullHyProxy exportiert.
const FullHyProxy = require("./proxy.js"); 
const { startWebPanel } = require("./web-panel.js");

let config = yaml.parse(fs.readFileSync("./config.yml", "utf8"));
let env = { apiKey: process.env.HYPIXEL_API_KEY };
if (!env.apiKey) { console.log("API key not found in .env file."); process.exit(); }

// Die Konfiguration wird jetzt an den Konstruktor der richtigen Klasse übergeben.
const proxy = new FullHyProxy(config);
startWebPanel(2108, config, env);