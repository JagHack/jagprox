const DiscordRPC = require('discord-rpc');

const clientId = '1420457169400238121';
DiscordRPC.register(clientId);

const rpc = new DiscordRPC.Client({ transport: 'ipc' });

async function setActivity(activity) {
  if (!rpc) {
    return;
  }
  rpc.setActivity(activity);
}

rpc.on('ready', () => {
  console.log('Discord RPC connected');
});

rpc.login({ clientId }).catch(console.error);

module.exports = {
  setActivity,
};
