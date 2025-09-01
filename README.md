# JagProx - A Powerful Hypixel Proxy

![GitHub repo size](https://img.shields.io/github/repo-size/JagHack/jagprox?style=for-the-badge&color=B33BFF) ![License](https://img.shields.io/github/license/JagHack/jagprox?style=for-the-badge&color=B33BFF)

**Disclaimer:** This is a third-party tool and is not affiliated with, endorsed, or supported by Hypixel Inc. Use at your own risk.

JagProx is a powerful, local Hypixel proxy built with Node.js that enhances your gameplay experience by adding custom quality-of-life features, advanced statistical analysis, and powerful automation tools directly to your Minecraft client.

---

## Key Features

- **In-Game Stat Command (`/sc`):**  
  Check detailed stats for any player in any gamemode directly from your chat, complete with an ASCII art avatar.

- **Advanced Friend Notifications (`/superf`):**  
  Track specific "super friends." Get custom, multi-line notifications when they log in or out. When they join, their stats for your predefined gamemodes are automatically fetched and displayed.

- **Party Stat Check (`/psc`):**  
  Instantly get a statistical overview (Bedwars stats) of all members in your current party to assess team strength.

- **Web Control Panel:**  
  A sleek, modern web interface running locally to manage your proxy's settings and use powerful tools without being in-game.
  - **Player Stat Search:** Look up detailed player stats for any gamemode directly from your browser.
  - **Command Alias Manager:** Create and manage your own custom command shortcuts (e.g., `/solobw` → `/play bedwars_eight_one`).

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
  1. Go to [Hypixel Developer Dashboard](https://developer.hypixel.net/)
  2. Register or login to your Hypixel Forums Account
  3. Copy the new API key securely.

### 2. Cloning the Repository

```bash
git clone https://github.com/JagHack/jagprox.git
````

### 3. Installing Dependencies

```bash
cd jagprox
npm install
```

### 4. Configuration

You need to create two configuration files in the root of the `jagprox` folder:

**`.env` file (for secrets)**
Create a file named `.env` and add your Hypixel API key like this:

```env
HYPIXEL_API_KEY=your_api_key_here
```

---

## How to Use

### 1. Starting the Proxy

```bash
node main.js
```

You should see messages indicating that the proxy server and web panel have started.

### 2. Connecting in Minecraft

1. Launch Minecraft (**1.8.9 recommended**).

2. Go to `Multiplayer` → `Add Server`.

3. For the **Server Address**, enter:

   ```
   localhost:2107
   ```

   (or whichever port you changed it to in the `config.yml` file)

4. Save the server and connect to it.
   You might need to authorize it. Don’t worry, everything is locally hosted.
   In the future you will be automatically logged into Hypixel through the proxy.

### 3. Using the Web Panel

While the proxy is running, open your browser and go to:

```
http://localhost:2108
```

Here you can manage your command aliases, set your API key, and use the player stat search tool.

### 4. In-Game Commands

* `/sc <gamemode> <player>`
  Check stats for a player.
  *Example:* `/sc bedwars Notch`

* `/status <player>`
  Check if a player is online and what game they are playing.
  *Example:* `/status Notch`

* `/superf add <player> <gamemode1> [gamemode2...]`
  Track a friend and their stats for specific games.
  *Example:* `/superf add Steve bedwars skywars duels`

* `/superf remove <player>`
  Stop tracking a friend.
  *Example:* `/superf remove Steve`

* `/psc`
  Fetch and display Bedwars stats for everyone in your current party.

---

## Contributing

Contributions are welcome! If you have ideas for new features or find bugs, feel free to open an issue or submit a pull request on the GitHub repository.

---

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
