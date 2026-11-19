# JagProx with the launcher window: `jagprox`, `jagprox-launcher` and the menu entry.
#   build:  nix-build nix -A jagprox
#   NixOS:  environment.systemPackages = [ (import /path/to/jagprox/nix {}).jagprox ];
{
  lib,
  rustPlatform,
  pkg-config,
  wrapGAppsHook3,
  copyDesktopItems,
  makeDesktopItem,
  webkitgtk_4_1,
  gtk3,
  glib,
  glib-networking,
  libsoup_3,
  cairo,
  pango,
  gdk-pixbuf,
  atk,
  librsvg,
  openssl,
  dbus,
}:

rustPlatform.buildRustPackage {
  pname = "jagprox";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      # Embedded into the binaries.
      ../config.yml
      ../aliases.json
      ../server-icon.png
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook3
    copyDesktopItems
  ];
  buildInputs = [
    webkitgtk_4_1
    gtk3
    glib
    glib-networking
    libsoup_3
    cairo
    pango
    gdk-pixbuf
    atk
    librsvg
    openssl
    dbus
  ];

  cargoBuildFlags = [ "-p" "jagprox-cli" "-p" "jagprox-app" ];
  cargoTestFlags = [ "-p" "mcproto" "-p" "jagprox-core" "-p" "jagprox-cli" ];

  desktopItems = [
    (makeDesktopItem {
      name = "jagprox";
      desktopName = "JagProx";
      comment = "Hypixel proxy with in-game stats";
      exec = "jagprox-launcher";
      icon = "jagprox-launcher";
      categories = [ "Game" ];
      startupWMClass = "jagprox-launcher";
    })
  ];

  postInstall = ''
    icons=crates/jagprox-app/icons
    install -Dm644 $icons/32x32.png $out/share/icons/hicolor/32x32/apps/jagprox-launcher.png
    install -Dm644 $icons/128x128.png $out/share/icons/hicolor/128x128/apps/jagprox-launcher.png
    install -Dm644 $icons/128x128@2x.png $out/share/icons/hicolor/256x256/apps/jagprox-launcher.png
  '';

  meta = {
    description = "JagProx - a Hypixel proxy, with its launcher window";
    homepage = "https://github.com/JagHack/jagprox";
    license = lib.licenses.gpl3Plus;
    mainProgram = "jagprox";
    platforms = lib.platforms.linux;
  };
}
