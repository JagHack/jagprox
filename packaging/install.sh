#!/bin/sh
# Installs the jagprox command (CLI and headless proxy) on any Linux:
#   curl -fsSL https://raw.githubusercontent.com/JagHack/jagprox/main/packaging/install.sh | sh
# It goes to ~/.local/bin (or $JAGPROX_BIN_DIR). Update it later with `jagprox update`.
set -eu

repo="JagHack/jagprox"
bin_dir="${JAGPROX_BIN_DIR:-$HOME/.local/bin}"

fail() {
    echo "jagprox install: $*" >&2
    exit 1
}

case "$(uname -s)" in
    Linux) ;;
    Darwin) fail "on macOS, install the JagProx app from https://github.com/$repo/releases and use Settings > Install Command." ;;
    *) fail "this installer is for Linux. Windows: use the installer from https://github.com/$repo/releases" ;;
esac

case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) fail "no build for $(uname -m) yet" ;;
esac

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -qO "$2" "$1"; }
else
    fail "needs curl or wget"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

name="jagprox-linux-$arch.tar.gz"
base="${JAGPROX_DOWNLOAD_URL:-https://github.com/$repo/releases/latest/download}"
echo "Downloading $name..."
fetch "$base/$name" "$tmp/$name" || fail "download failed ($base/$name)"

if fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" 2>/dev/null; then
    expected=$(grep " $name\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$tmp/$name" | cut -d' ' -f1)
    else
        actual=$(shasum -a 256 "$tmp/$name" | cut -d' ' -f1)
    fi
    [ -n "$expected" ] && [ "$expected" = "$actual" ] || fail "$name doesn't match SHA256SUMS"
fi

tar -xzf "$tmp/$name" -C "$tmp" jagprox
mkdir -p "$bin_dir"
cp "$tmp/jagprox" "$bin_dir/jagprox.new"
chmod 755 "$bin_dir/jagprox.new"
mv -f "$bin_dir/jagprox.new" "$bin_dir/jagprox"

echo "Installed $("$bin_dir/jagprox" --version) to $bin_dir/jagprox"
case ":$PATH:" in
    *":$bin_dir:"*) echo "Try: jagprox --help" ;;
    *)
        echo
        echo "$bin_dir isn't on your PATH yet. To add it, run:"
        echo "  $bin_dir/jagprox install-cli"
        ;;
esac
