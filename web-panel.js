const express = require('express');
const bodyParser = require('body-parser');
const fs = require('fs');
const path = require('path');
const yaml = require('yaml');
const formatter = require("./formatter.js");

function startWebPanel(port, configRef, envRef) {
    const app = express();
    app.use(bodyParser.json());
    app.use(express.static(path.join(__dirname, 'public')));
    app.get('/api/get-config', (req, res) => {
        try {
            const currentConfig = yaml.parse(fs.readFileSync("./config.yml", "utf8"));
            const currentEnv = fs.readFileSync('.env', 'utf8').split('=')[1] || '';
            res.json({ apiKey: currentEnv, aliases: currentConfig.aliases });
        } catch (e) { res.status(500).json({ success: false, message: "Could not read config files."}) }
    });
    app.post('/api/save-config', (req, res) => {
        try {
            const { apiKey, aliases } = req.body;
            fs.writeFileSync('.env', `HYPIXEL_API_KEY=${apiKey}`);
            const currentConfig = yaml.parse(fs.readFileSync("./config.yml", "utf8"));
            currentConfig.aliases = aliases;
            fs.writeFileSync('config.yml', yaml.stringify(currentConfig));
            envRef.apiKey = apiKey;
            Object.assign(configRef, currentConfig);
            formatter.log("Configuration updated via web panel.");
            res.json({ success: true, message: "Configuration saved successfully!" });
        } catch (error) {
            formatter.log(`Error saving config: ${error.message}`);
            res.status(500).json({ success: false, message: "Failed to save configuration." });
        }
    });
    app.listen(port, () => {
        formatter.log(`Web control panel started on http://localhost:${port}`);
    });
}
module.exports = { startWebPanel };