const { API_BASE_URL } = require('../utils/api_constants.js');

// Returns the JSON body, or throws with the server's message (or the status) on failure.
async function parseResponse(response) {
    let data = null;
    try {
        data = await response.json();
    } catch (e) {
        // Non-JSON body (e.g. an HTML error page).
    }
    if (!response.ok) {
        throw new Error((data && data.message) || `API returned status ${response.status}`);
    }
    return data;
}

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

        return parseResponse(response);
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

        return parseResponse(response);
    }

    async getStats(period, hours = 1) {
        let url;
        switch (period) {
            case 'hour':
                url = `${API_BASE_URL}/gametrack/hour?hours=${encodeURIComponent(hours)}`;
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
        return parseResponse(response);
    }
}

module.exports = GametrackApiHandler;
