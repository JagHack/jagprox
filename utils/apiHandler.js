const fetch = require('node-fetch');

class ApiHandler {
    constructor(options = {}) {
        this.baseUrl = options.baseUrl || 'https://jagprox.jaghack.com/api/v1';
        this.jwt = options.jwt || null;
    }

    setJwt(jwt) {
        this.jwt = jwt;
    }

    async getApiKey() {
        if (!this.jwt) {
            throw new Error('Authentication token (JWT) is not set.');
        }

        try {
            const response = await fetch(`${this.baseUrl}/user/api-key`, {
                headers: {
                    'Authorization': `Bearer ${this.jwt}`
                }
            });

            if (response.status === 404) {
                return null;
            }

            if (!response.ok) {
                throw new Error(`Failed to fetch API key. Status: ${response.status}`);
            }

            const data = await response.json();
            return data.apiKey;
        } catch (error) {
            console.error('Error fetching API key from backend:', error);
            throw error;
        }
    }

    async saveApiKey(apiKey) {
        if (!this.jwt) {
            throw new Error('Authentication token (JWT) is not set.');
        }

        try {
            const response = await fetch(`${this.baseUrl}/user/api-key`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                    'Authorization': `Bearer ${this.jwt}`
                },
                body: JSON.stringify({ apiKey })
            });

            if (!response.ok) {
                throw new Error(`Failed to save API key. Status: ${response.status}`);
            }

            return await response.json();
        } catch (error) {
            console.error('Error saving API key to backend:', error);
            throw error;
        }
    }
}

module.exports = ApiHandler;

