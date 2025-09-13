# JagProx - A Powerful Hypixel Proxy

![GitHub repo size](https://img.shields.io/github/repo-size/JagHack/jagprox?style=for-the-badge&color=B33BFF) ![License](https://img.shields.io/github/license/JagHack/jagprox?style=for-the-badge&color=B33BFF) ![CodeRabbit Pull Request Reviews](https://img.shields.io/coderabbit/prs/github/JagHack/jagprox?utm_source=oss&utm_medium=github&utm_campaign=JagHack%2Fjagprox&labelColor=171717&color=B33BFF&link=https%3A%2F%2Fcoderabbit.ai&label=CodeRabbit+Reviews)

**Disclaimer:** This is a third-party tool and is not affiliated with, endorsed, or supported by Hypixel Inc. Use at your own risk.

JagProx is a powerful, local Hypixel proxy built with Node.js that enhances your gameplay experience by adding custom quality-of-life features, advanced statistical analysis, and powerful automation tools directly to your Minecraft client, all managed through a sleek desktop launcher.

---

# Join the [Discord](https://discord.gg/ZQ46u4NhVt)!

---

## Key Features

- **In-Game Stat Command (`/sc`):**  
  Check detailed stats for any player in any gamemode directly from your chat, complete with an ASCII art avatar.

- **Advanced Friend Notifications (`/superf`):**  
  Track specific "super friends." Get custom, multi-line notifications when they log in or out. When they join, their stats for your predefined gamemodes are automatically fetched and displayed.

- **Party Stat Check (`/psc`):**  
  Instantly get a statistical overview (Bedwars stats) of all members in your current party to assess team strength.

- **Modern Desktop Launcher:**  
  A sleek, standalone launcher to manage the proxy, view logs, and access tools without cluttering your game.
  - **Start & Stop Control:** Easily launch and stop the proxy with a single click.
  - **Live Log & Chat Monitor:** Keep an eye on system logs and the in-game chat in real-time.
  - **Integrated Settings:** Manage your API key, command aliases, and other settings directly within the launcher.
  - **Player Search Tools:** Look up detailed player stats and online status for any player on Hypixel.

- **Secure & Local:**  
  The proxy runs entirely on your own machine. Your Minecraft account details are handled by the official `minecraft-protocol` library for authentication and are never sent to a third party.

---

## Installation Guide

Follow these steps to get JagProx up and running on your system.

### 1. Prerequisites

Before you begin, ensure you have the following installed:

- **Node.js:** Version 16.x or newer is recommended. [Download here](https://nodejs.org/).
- **Git:** Required to clone the repository. [Download here](https://git-scm.com/).
- **A valid Minecraft: Java Edition account.**
- **A Hypixel API Key:**
  1. Go to the [Hypixel Developer Dashboard](https://developer.hypixel.net/).
  2. Log in with your Minecraft account.
  3. Click on "Create a new API key" and copy the key securely.

### 2. Cloning the Repository

```bash
git clone https://github.com/JagHack/jagprox.git
```

### 3. Installing Dependencies

Navigate into the newly cloned folder and install the required packages.
```bash
cd jagprox
npm install
```

### 4. Configuration

You have two configuration files in the root of the `jagprox` folder:

**a) `config.yml` (for general settings)**

**b) `.env` file (for secrets)** (maybe create this one)

---

## How to Use

### 1. Starting JagProx

You have two ways to run the application:

**A) Using the Desktop Launcher (Recommended)**
This provides a full user interface to control the proxy and access all features.

```bash
npm run launcher
```

**B) Running the Proxy Only (Headless)**
This will start the proxy in your terminal without the user interface.

```bash
npm start
```

### 2. Connecting in Minecraft

1. Launch Minecraft (**1.8.9 recommended**).
2. Go to `Multiplayer` → `Add Server`.
3. For the **Server Address**, enter:
   ```
   localhost:2107
   ```
   (or whichever port you configured in `config.yml`)
4. Save the server and connect to it. You will be automatically logged into Hypixel through the proxy.

### 3. In-Game Commands

* `/sc <gamemode> <player>`
  Check stats for a player.
  *Example:* `/sc bedwars Notch`

* `/status <player>`
  Check if a player is online and what game they are playing.
  *Example:* `/status Notch`

* `/superf <add|remove|list> [player] [gamemodes...]`
  Track a friend and their stats for specific games.
  *Example:* `/superf add Steve bedwars skywars duels`

* `/psc`
  Fetch and display Bedwars stats for everyone in your current party.

* `/rq`
  Re-queues the last `/play` command you used.

* `/alert <add|remove|list> [player]`
  Get a notification when a specific player appears in your lobby.

* `/nickname <add|remove|list> [player] [nickname]`
  Set a local, client-side nickname for a player that appears in chat and the tab list.

* `/jagprox`
  Displays the list of available custom commands.

---

## Contributing

Contributions are welcome and have already been made! Huge shoutout to Yanice and TheMackabu for helping me with some stuff. If you have ideas for new features or find bugs, feel free to open an issue or submit a pull request on the GitHub repository.

---

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
