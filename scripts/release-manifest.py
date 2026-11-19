#!/usr/bin/env python3
"""Writes the release metadata next to the built files in DIR:

- jagprox-release.json  what `jagprox update` and the launcher read (signed afterwards)
- SHA256SUMS            for people and install.sh
- latest.yml / latest-linux.yml
                        what JagProx 2.x (Electron) reads, pointing it at the new
                        Windows installer and AppImage, so it updates itself to 3.x

usage: release-manifest.py VERSION DIR
"""

import base64
import datetime
import hashlib
import json
import pathlib
import re
import sys

# kind -> file name pattern ({v} = version). Every built file must match one.
KINDS = {
    "cli-linux-x86_64": "jagprox-linux-x86_64.tar.gz",
    "cli-linux-aarch64": "jagprox-linux-aarch64.tar.gz",
    "appimage-x86_64": "JagProx-{v}-x86_64.AppImage",
    "appimage-aarch64": "JagProx-{v}-aarch64.AppImage",
    "deb-x86_64": "jagprox_{v}_amd64.deb",
    "deb-aarch64": "jagprox_{v}_arm64.deb",
    "rpm-x86_64": "jagprox-{v}-1.x86_64.rpm",
    "rpm-aarch64": "jagprox-{v}-1.aarch64.rpm",
    "windows-setup-x86_64": "JagProx-Setup-{v}.exe",
    "macos-dmg-universal": "JagProx-{v}-universal.dmg",
    "macos-app-universal": "JagProx-{v}-universal.app.tar.gz",
}


def digest(path, algorithm):
    h = hashlib.new(algorithm)
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h


def electron_yml(path, version, date):
    sha512 = base64.b64encode(digest(path, "sha512").digest()).decode()
    size = path.stat().st_size
    return (
        f"version: {version}\n"
        f"files:\n"
        f"  - url: {path.name}\n"
        f"    sha512: {sha512}\n"
        f"    size: {size}\n"
        f"path: {path.name}\n"
        f"sha512: {sha512}\n"
        f"releaseDate: '{date}'\n"
    )


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    version, folder = sys.argv[1], pathlib.Path(sys.argv[2])
    if not re.fullmatch(r"\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?", version):
        sys.exit(f"not a version: {version}")

    assets = []
    names = {}
    for kind, pattern in KINDS.items():
        name = pattern.format(v=version)
        path = folder / name
        if not path.is_file():
            sys.exit(f"missing {name} ({kind})")
        names[kind] = path
        assets.append(
            {
                "kind": kind,
                "name": name,
                "sha256": digest(path, "sha256").hexdigest(),
                "size": path.stat().st_size,
            }
        )
    expected = {p.name for p in names.values()}
    extra = sorted(p.name for p in folder.iterdir() if p.is_file() and p.name not in expected)
    if extra:
        sys.exit(f"unexpected files: {', '.join(extra)}")

    manifest = {"version": version, "assets": assets}
    (folder / "jagprox-release.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (folder / "SHA256SUMS").write_text(
        "".join(f"{a['sha256']}  {a['name']}\n" for a in assets)
    )

    # Electron's updater only looks at full releases; pre-releases skip the bridge.
    if "-" not in version:
        date = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")
        (folder / "latest.yml").write_text(
            electron_yml(names["windows-setup-x86_64"], version, date)
        )
        (folder / "latest-linux.yml").write_text(
            electron_yml(names["appimage-x86_64"], version, date)
        )


if __name__ == "__main__":
    main()
