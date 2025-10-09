const DiscordRPC = require('discord-rpc');

const clientId = '1420457169400238121';
DiscordRPC.register(clientId);

const rpc = new DiscordRPC.Client({ transport: 'ipc' });
let isActive = false;

async function setActivity(activity) {
  if (!isActive) return;
  try {
    await rpc.setActivity(activity);
  } catch (e) {
    console.error('Failed to set Discord activity:', e);
  }
}

function login() {
  if (isActive) return;
  rpc.login({ clientId }).then(() => {
    isActive = true;
    console.log('Discord RPC connected');
    setActivity({
      details: 'Idling',
      state: 'In Launcher',
      largeImageKey: 'icon',
      largeImageText: 'JagProx',
      instance: false,
    });
  }).catch(e => {
    console.error('Failed to connect Discord RPC:', e);
  });
}

function logout() {
  if (!isActive) return;
  rpc.destroy().then(() => {
    isActive = false;
    console.log('Discord RPC disconnected');
  }).catch(e => {
    console.error('Failed to disconnect Discord RPC:', e);
  });
}

rpc.on('ready', () => {
  isActive = true;
  console.log('Discord RPC ready');
});

rpc.on('disconnected', () => {
  isActive = false;
  console.log('Discord RPC disconnected');
});

module.exports = {
  setActivity,
  login,
  logout,
  isActive: () => isActive
};
