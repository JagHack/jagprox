const DiscordRPC = require('discord-rpc');

const clientId = '1420457169400238121';

const IDLE_ACTIVITY = {
  details: 'Idling',
  state: 'In Launcher',
  largeImageKey: 'icon',
  largeImageText: 'JagProx',
  instance: false,
};

let rpc = null;
let isActive = false;
let currentActivity = { ...IDLE_ACTIVITY, startTimestamp: new Date() };

// A destroyed discord-rpc client can't log in again, so every login gets a fresh client.
function login() {
  if (rpc) return;
  const client = new DiscordRPC.Client({ transport: 'ipc' });
  rpc = client;

  client.on('ready', () => {
    if (rpc !== client) return;
    isActive = true;
    console.log('Discord RPC connected');
    client.setActivity(currentActivity).catch(e => console.error('Failed to set Discord activity:', e.message));
  });

  client.on('disconnected', () => {
    if (rpc !== client) return;
    isActive = false;
    rpc = null;
    console.log('Discord RPC disconnected');
  });

  client.login({ clientId }).catch(e => {
    console.error('Failed to connect Discord RPC:', e.message);
    if (rpc === client) {
      rpc = null;
      isActive = false;
    }
  });
}

function logout() {
  const client = rpc;
  rpc = null;
  isActive = false;
  if (client) {
    client.destroy()
      .then(() => console.log('Discord RPC disconnected'))
      .catch(e => console.error('Failed to disconnect Discord RPC:', e.message));
  }
}

// Remembers the activity even while disconnected, so it's applied on the next login.
async function setActivity(activity) {
  currentActivity = activity;
  if (!isActive || !rpc) return;
  try {
    await rpc.setActivity(activity);
  } catch (e) {
    console.error('Failed to set Discord activity:', e.message);
  }
}

module.exports = {
  IDLE_ACTIVITY,
  setActivity,
  login,
  logout,
  isActive: () => isActive
};
