const { API_BASE_URL } = require("../utils/api_constants.js");

async function makeApiRequest(endpoint, method = 'GET', body = null) {
    const token = process.env.JAGPROX_JWT;
    if (!token) {
        // This error will be logged in the proxy's console, not sent to the player.
        // The command handler will inform the player.
        console.error('Gametrack API Error: JWT token not found in environment.');
        return { success: false, message: 'Authentication token is missing.' };
    }

    const headers = {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json'
    };

    const options = {
        method,
        headers,
    };

    if (body) {
        options.body = JSON.stringify(body);
    }

    try {
        const response = await fetch(`${API_BASE_URL}/gametrack${endpoint}`, options);
        // It's possible for the backend to send a non-JSON error response
        if (!response.headers.get('content-type')?.includes('application/json')) {
            const errorText = await response.text();
            return { success: false, message: `Server returned non-JSON response: ${errorText}` };
        }
        const data = await response.json();

        if (response.ok) {
            return { success: true, data };
        } else {
            return { success: false, message: data.message || `API Error ${response.status}` };
        }
    } catch (error) {
        console.error(`Gametrack API network error for ${endpoint}:`, error);
        return { success: false, message: 'Network error connecting to JagProx services.' };
    }
}

// All functions call the generic request handler
const startTracking = (mc_uuid, mode) => makeApiRequest('/start', 'POST', { mc_uuid, mode });
const stopTracking = (mc_uuid, mode) => makeApiRequest('/stop', 'POST', { mc_uuid, mode });
const pushGametrackEvent = (mc_uuid, mode, result) => makeApiRequest('/event', 'POST', { mc_uuid, mode, result });
const fetchHourlyAggregates = (hours = 1) => makeApiRequest(`/hour?hours=${hours}`);
const fetchDailyAggregates = () => makeApiRequest('/day');
const fetchFullLog = () => makeApiRequest('/log');

module.exports = {
    startTracking,
    stopTracking,
    pushGametrackEvent,
    fetchHourlyAggregates,
    fetchDailyAggregates,
    fetchFullLog,
};
