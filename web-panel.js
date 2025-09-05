const express = require('express');
const fs = require('fs');
const yaml = require('yaml');
const path = require('path');
const formatter = require('./formatter.js');
const { gameModeMap } = require('./utils/constants.js');

function startWebPanel(port, config, env, proxy) {
    const app = express();
    app.use(express.json());
    app.use(express.static(path.join(__dirname, 'public')));

    const configPath = path.join(__dirname, 'config.yml');
    const envPath = path.join(__dirname, '.env');

    app.get('/', (req, res) => {
        res.sendFile(path.join(__dirname, 'public', 'index.html'));
    });

    app.get('/api/get-config', (req, res) => {
        const uniqueGamemodes = new Map();
        for (const [key, modeInfo] of Object.entries(gameModeMap)) {
            if (!uniqueGamemodes.has(modeInfo.displayName)) {
                uniqueGamemodes.set(modeInfo.displayName, key);
            }
        }

        const availableGamemodes = Array.from(uniqueGamemodes, ([displayName, apiKey]) => ({
            text: displayName,
            value: apiKey
        }));

        availableGamemodes.sort((a, b) => a.text.localeCompare(b.text));

        res.json({
            apiKeyIsSet: !!env.apiKey,
            aliases: config.aliases || {},
            availableGamemodes: availableGamemodes
        });
    });

    app.post('/api/save-config', (req, res) => {
        try {
            const receivedConfig = req.body;
            const changesMade = [];

            if (receivedConfig.apiKey) {
                env.apiKey = receivedConfig.apiKey;
                proxy.env.apiKey = receivedConfig.apiKey;
                let envContent = fs.existsSync(envPath) ? fs.readFileSync(envPath, 'utf8') : '';
                const key = 'HYPIXEL_API_KEY';
                if (envContent.includes(key)) {
                    envContent = envContent.replace(new RegExp(`^${key}=.*$`, 'm'), `${key}=${receivedConfig.apiKey}`);
                } else {
                    envContent += `\n${key}=${receivedConfig.apiKey}`;
                }
                fs.writeFileSync(envPath, envContent.trim(), 'utf8');
                changesMade.push('API Key');
            }

            config.aliases = receivedConfig.aliases || {};
            const fileConfig = yaml.parse(fs.readFileSync(configPath, 'utf8'));
            fileConfig.aliases = config.aliases;
            fs.writeFileSync(configPath, yaml.stringify(fileConfig), 'utf8');
            changesMade.push('Aliases');

            const message = `Configuration for ${changesMade.join(' & ')} saved and applied successfully.`;
            formatter.log(`Configuration updated via web panel: ${changesMade.join(', ')}`);
            res.json({ success: true, message: message });

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
        if (!env.apiKey) {
            return res.status(400).json({ error: 'HYPIXEL API KEY is not set on the server.' });
        }

        const stats = await proxy.hypixel.getStatsForAPI(gamemode, name);
        if (stats && stats.error) {
            return res.status(404).json(stats);
        }
        res.json(stats);
    });

    app.listen(port, () => {
        formatter.log(`Web panel active at http://localhost:${port}`);
    });
}

module.exports = { startWebPanel };