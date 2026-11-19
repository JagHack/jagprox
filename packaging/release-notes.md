JagProx @VERSION@

## Install

| System | Download | `jagprox` command |
|---|---|---|
| Windows | `JagProx-Setup-@VERSION@.exe` | added to PATH by the installer (open a new terminal) |
| macOS | `JagProx-@VERSION@-universal.dmg` (right-click the app → Open the first time) | Settings → Install Command |
| Debian, Ubuntu, Mint, Pop!_OS | `jagprox_@VERSION@_amd64.deb` (`arm64` for ARM) | included |
| Fedora, openSUSE, RHEL | `jagprox-@VERSION@-1.x86_64.rpm` (`aarch64` for ARM) | included |
| Arch, Manjaro, EndeavourOS | AUR: `jagprox-bin` | included |
| Any other Linux | `JagProx-@VERSION@-x86_64.AppImage` | Settings → Install Command |
| Terminal only (any Linux) | `curl -fsSL https://github.com/JagHack/jagprox/releases/latest/download/install.sh \| sh` | `~/.local/bin/jagprox` |

Already on JagProx 2? The launcher offers this update itself; your login, settings and aliases carry over.

Update later with `jagprox update`, or Settings → Check for Updates. Every download is listed in `SHA256SUMS`, and `jagprox-release.json` is signed with the JagProx release key (`jagprox-release.json.minisig`).
