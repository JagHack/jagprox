const express = require('express');
const fs = require('fs');
const yaml = require('yaml');
const path = require('path');
const formatter = require('./formatter.js');
const { gameModeMap } = require('./utils/constants.js');

// Receive the proxy instance to access its methods
function startWebPanel(port, config, env, proxy) { 
    const app = express();
    app.use(express.json());
    app.use(express.static(path.join(__dirname, 'public')));

    const configPath = path.join(__dirname, 'config.yml');

    app.get('/api/get-config', (req, res) => {
        // Create a simple list of gamemode names for the dropdown
        const availableGamemodes = Object.keys(gameModeMap);
        res.json({
            apiKey: env.apiKey ? '********' : '',
            aliases: config.aliases || {},
            availableGamemodes: availableGamemodes
        });
    });

    // --- KORRIGIERT: Speichert jetzt nur noch die Aliase ---
    app.post('/api/save-config', (req, res) => {
        try {
            const receivedConfig = req.body;

            // Update live config
            if (receivedConfig.aliases) {
                config.aliases = receivedConfig.aliases;
            }

            // Update config file
            const fileConfig = yaml.parse(fs.readFileSync(configPath, 'utf8'));
            // Update only the aliases section in the file
            fileConfig.aliases = receivedConfig.aliases || {};

            fs.writeFileSync(configPath, yaml.stringify(fileConfig), 'utf8');
            formatter.log('Configuration updated via web panel.');
            res.json({ success: true, message: 'Configuration saved and applied successfully.' });

        } catch (error) {
            formatter.log(`Error saving configuration: ${error.message}`);
            res.status(500).json({ success: false, message: 'Error saving configuration file.' });
        }
    });

    app.get('/api/player/:gamemode/:name', async (req, res) => {
        const { gamemode, name } = req.params;
        if (!proxy || !proxy.hypixel) {
            return res.status(500).json({ error: 'Proxy not available' });
        }
        
        const stats = await proxy.hypixel.getStatsForAPI(gamemode, name);
        if (stats && stats.error) { // Check for stats existence before accessing error
            return res.status(404).json(stats);
        }
        res.json(stats);
    });

    app.listen(port, () => {
        formatter.log(`Web panel active at http://localhost:${port}`);
    });
}

module.exports = { startWebPanel };