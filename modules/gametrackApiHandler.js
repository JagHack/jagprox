const fetch = require('node-fetch');
const { API_BASE_URL } = require('../utils/api_constants.js');

class GametrackApiHandler {
    constructor(jwt) {
        if (!jwt) {
            throw new Error('GametrackApiHandler requires a JWT for authentication.');
        }
        this.jwt = jwt;
        this.headers = {
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${this.jwt}`,
        };
    }

    async sendStartEvent({ mc_uuid, mode }) {
        if (!mc_uuid || !mode) {
            throw new Error('Missing required parameters for gametrack start event.');
        }
        const response = await fetch(`${API_BASE_URL}/gametrack/start`, {
            method: 'POST',
            headers: this.headers,
            body: JSON.stringify({ mc_uuid, mode }),
        });

        if (!response.ok) {
            const errorData = await response.json();
            throw new Error(errorData.message || `API returned status ${response.status}`);
        }
        return response.json();
    }

    async sendEvent({ mc_uuid, mode, result }) {
        if (!mc_uuid || !mode || !result) {
            throw new Error('Missing required parameters for gametrack event.');
        }
        const response = await fetch(`${API_BASE_URL}/gametrack/event`, {
            method: 'POST',
            headers: this.headers,
            body: JSON.stringify({ mc_uuid, mode, result }),
        });

        if (!response.ok) {
            const errorData = await response.json();
            throw new Error(errorData.message || `API returned status ${response.status}`);
        }
        return response.json();
    }

    async getStats(period, hours = 1) {
        let url;
        switch (period) {
            case 'hour':
                url = `${API_BASE_URL}/gametrack/hour?hours=${hours}`;
                break;
            case 'day':
                url = `${API_BASE_URL}/gametrack/day`;
                break;
            case 'log':
                url = `${API_BASE_URL}/gametrack/log`;
                break;
            default:
                throw new Error('Invalid stats period specified.');
        }

        const response = await fetch(url, { headers: this.headers });
        const data = await response.json();
        if (!response.ok) {
            throw new Error(data.message || `Failed to fetch ${period} stats.`);
        }
        return data;
    }
}

module.exports = GametrackApiHandler;
