# JagProx - A Powerful Hypixel Proxy

![GitHub repo size](https://img.shields.io/github/repo-size/JagHack/jagprox?style=for-the-badge&color=B33BFF) ![License](https://img.shields.io/github/license/JagHack/jagprox?style=for-the-badge&color=B33BFF) ![CodeRabbit Pull Request Reviews](https://img.shields.io/coderabbit/prs/github/JagHack/jagprox?utm_source=oss&utm_medium=github&utm_campaign=JagHack%2Fjagprox&labelColor=171717&color=B33BFF&link=https%3A%2F%2Fcoderabbit.ai&label=CodeRabbit+Reviews)

**Disclaimer:** This is a third-party tool and is not affiliated with, endorsed, or supported by Hypixel Inc. Use at your own risk.

JagProx is a powerful, local Hypixel proxy that enhances your gameplay experience by adding custom quality-of-life features, advanced statistical analysis, and powerful automation tools directly to your Minecraft client, all managed through a sleek desktop launcher or the `jagprox` command. Since 3.0 it is written in Rust: a ~10 MB download instead of ~150 MB, a fraction of the memory, and it runs on Windows, macOS and Linux.

---

# Join the [Discord](https://discord.gg/ZQ46u4NhVt)!

---

## Key Features

- **In-Game Stat Command (`/sc`):**  
  Check detailed stats for any player in any gamemode directly from your chat, complete with an ASCII art avatar.

- **Advanced Friend Notifications (`/superf`):**  
  Track specific "super friends." Get custom, multi-line notifications when they log in or out. When they join, their stats for your predefined gamemodes are automatically fetched and displayed.

- **Party Stat Check (`/psc`):**

  Checks the stats of all players in your party for a specified game mode. If no game mode is provided, it defaults to the current game or Bedwars. This is useful for quickly assessing your party's skill level before starting a game.

  * `/psc [gamemode]`

- **Modern Desktop Launcher:**  
  A sleek, standalone launcher to manage the proxy, view logs, and access tools without cluttering your game.
  - **Start & Stop Control:** Easily launch and stop the proxy with a single click.
  - **Live Log & Chat Monitor:** Keep an eye on system logs and the in-game chat in real-time.
  - **Integrated Settings:** Manage your API key, command aliases, and other settings directly within the launcher.
  - **Player Search Tools:** Look up detailed player stats and online status for any player on Hypixel.

- **The `jagprox` command:**  
  `jagprox stats Notch`, `jagprox status Notch`, `jagprox launch proxy` and more, from any terminal on any OS.

- **Secure & Local:**  
  The proxy runs entirely on your own machine. You sign in to your Minecraft account directly with Microsoft; your login is never sent to a third party.

---

## Installation Guide

Download JagProx from the [latest release](https://github.com/JagHack/jagprox/releases/latest):

| System | Download | `jagprox` command |
|---|---|---|
| Windows 10/11 | `JagProx-Setup-<version>.exe` | added to PATH by the installer (open a new terminal) |
| macOS 10.15+ | `JagProx-<version>-universal.dmg` (right-click the app → Open the first time) | Settings → Install Command |
| Debian, Ubuntu, Mint, Pop!_OS | `jagprox_<version>_amd64.deb` (`arm64` for ARM) | included |
| Fedora, openSUSE, RHEL | `jagprox-<version>-1.x86_64.rpm` (`aarch64` for ARM) | included |
| Arch, Manjaro, EndeavourOS | the `PKGBUILD` from the release (AUR: `jagprox-bin`) | included |
| NixOS / Nix | `nix profile install github:JagHack/jagprox` — or `nix run github:JagHack/jagprox` to try it | included |
| Any other Linux | `JagProx-<version>-x86_64.AppImage` | Settings → Install Command |
| Terminal only, any Linux | `curl -fsSL https://github.com/JagHack/jagprox/releases/latest/download/install.sh \| sh` | `~/.local/bin/jagprox` |

**On NixOS**, use the flake above rather than the AppImage — it runs natively with no `appimage-run` or FHS setup. `nix run github:JagHack/jagprox` to try it, `nix profile install github:JagHack/jagprox` to keep it, or add it as a flake input / via `overlays.default`.

Already on JagProx 2? It offers the update itself; your login, settings and aliases carry over. Updates after that come from the launcher (Settings → Check for Updates) or `jagprox update`.

You also need a Minecraft: Java Edition account, and for stats a Hypixel API key from the [Hypixel Developer Dashboard](https://developer.hypixel.net/) (save it in the launcher's Settings or with `jagprox key <key>`).

### The `jagprox` command

```
jagprox                      open the launcher
jagprox launch proxy         run the proxy in this terminal (Ctrl+C stops it)
jagprox stats Notch [mode]   stats (Bed Wars by default; `jagprox modes` lists the rest)
jagprox status Notch         is a player online, and where?
jagprox playercount sumo     players in a Duels mode
jagprox gametrack day        your tracked wins and losses
jagprox login | logout       your JagProx account
jagprox config               open config.yml
jagprox update               update to the latest version
jagprox version 3.0.0        install a specific version
```

`jagprox --help` lists everything; add `--json` for script-friendly output.

### Configuration

On first launch, `config.yml` and `aliases.json` are created in the JagProx data folder (`%APPDATA%\jagprox-launcher` on Windows, `~/.config/jagprox-launcher` on Linux, `~/Library/Application Support/jagprox-launcher` on macOS). The copies in this repository are only the defaults. Auto GG, Discord RPC, your API key and aliases can be edited from the launcher; changes reach a running proxy within a few seconds.

Notable `config.yml` options:

- `port` / `host`: where the proxy listens (default `127.0.0.1:2107`, so only your own PC can connect). Changing them needs a proxy restart.
- `queue_stats`: per-game automatic lobby stat checks (`bw`, `skywars`, `duels`).
- `debug_mode`: extra diagnostic lines in the log.
- `stats_telemetry`: off by default. When on, `/sc` shares the looked-up player's win count with the JagProx website.

### Building from source

With [Rust](https://rustup.rs) installed (and on Linux WebKitGTK 4.1, e.g. `libwebkit2gtk-4.1-dev`):

```bash
git clone https://github.com/JagHack/jagprox.git
cd jagprox
cargo test --workspace                     # tests
cargo run -p jagprox-cli -- --help         # the jagprox command
cargo run -p jagprox-app                   # the launcher
```

---

## Detailed Usage Guide

For a comprehensive guide on all features of the Jagprox Launcher, please refer to the [GUIDE.md](GUIDE.md) file.

---

### 3. In-Game Commands

* `/sc <gamemode> <player>`
  Check stats for a player.
  *Example:* `/sc bedwars Notch`

* `/status <player>`
  Check if a player is online and what game they are playing.
  *Example:* `/status Notch`

* `/superf <add|remove|list> [player] [gamemodes...]`
  Get notified when a friend joins one of the given games (checked once a minute; use `any` for every game).
  *Example:* `/superf add Steve bedwars skywars duels`

* `/psc [gamemode]`
  Fetch and display stats for everyone in your current party.

* `/rq`
  Re-queues the last `/play` command you used.

* `/alert <add|remove|list> [player]`
  Get a notification when a specific player appears in your lobby.

* `/nickname <add|remove|list> [player] [nickname]`
  Set a local, client-side nickname for a player that appears in chat and the tab list.

* `/drpc`
  Toggles the discord rich presence.

* `/goal <set|view|cancel>`
  Sets a stat goal, e.g. `/goal set bedwars fkdr 5`.

* `/jagprox`
  Displays the list of available custom commands.

* `/gt | /gametrack [hour|day|log] [hour count if used with hour]`
  Shows you the stats for the given hours/day or logs your last games.

---

## Contributing

Contributions are welcome and have already been made! Huge shoutout to Yanice and TheMackabu for helping me with some stuff. If you have ideas for new features or find bugs, feel free to open an issue or submit a pull request on the GitHub repository.

---

## Credits

Made by JagHack. Thanks to Yanice, whose tips and help shaped JagProx in its early days.

## License

JagProx is licensed under the [Apache License 2.0](LICENSE): use, change and share it however you like, including commercially. If you redistribute it or build on it, keep the [NOTICE](NOTICE) file so JagHack stays credited.
