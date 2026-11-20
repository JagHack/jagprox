# JagProx with the launcher window: `jagprox`, `jagprox-launcher` and the menu entry.
#   build:  nix-build nix -A jagprox
#   NixOS:  environment.systemPackages = [ (import /path/to/jagprox/nix {}).jagprox ];
#
# The launcher is a native egui/eframe app (OpenGL via glow), so it needs libGL,
# libxkbcommon, Wayland and X11 at run time rather than a browser engine. eframe
# dlopen's those, so the binary is wrapped with an LD_LIBRARY_PATH to find them.
{
  lib,
  rustPlatform,
  pkg-config,
  makeWrapper,
  copyDesktopItems,
  makeDesktopItem,
  libGL,
  libxkbcommon,
  wayland,
  xorg,
  fontconfig,
  freetype,
  openssl,
  dbus,
}:

let
  runtimeLibs = [
    libGL
    libxkbcommon
    wayland
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
    fontconfig
    freetype
  ];
in
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
      ../LICENSE
      ../NOTICE
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    makeWrapper
    copyDesktopItems
  ];
  buildInputs = [
    openssl
    dbus
  ] ++ runtimeLibs;

  cargoBuildFlags = [
    "-p"
    "jagprox-cli"
    "-p"
    "jagprox-gui"
  ];
  cargoTestFlags = [
    "-p"
    "mcproto"
    "-p"
    "jagprox-core"
    "-p"
    "jagprox-cli"
  ];

  desktopItems = [
    (makeDesktopItem {
      name = "jagprox-launcher";
      desktopName = "JagProx";
      comment = "Hypixel proxy with in-game stats";
      exec = "jagprox-launcher";
      icon = "jagprox-launcher";
      categories = [ "Game" ];
      startupWMClass = "jagprox-launcher";
    })
  ];

  postInstall = ''
    install -Dm644 -t $out/share/licenses/jagprox LICENSE NOTICE
    icons=crates/jagprox-gui/icons
    install -Dm644 $icons/32x32.png $out/share/icons/hicolor/32x32/apps/jagprox-launcher.png
    install -Dm644 $icons/128x128.png $out/share/icons/hicolor/128x128/apps/jagprox-launcher.png
    install -Dm644 $icons/128x128@2x.png $out/share/icons/hicolor/256x256/apps/jagprox-launcher.png

    # eframe loads GL/Wayland/X11 at run time; point the launcher at them.
    wrapProgram $out/bin/jagprox-launcher \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
  '';

  meta = {
    description = "JagProx - a Hypixel proxy, with its native launcher window";
    homepage = "https://github.com/JagHack/jagprox";
    license = lib.licenses.asl20;
    mainProgram = "jagprox";
    platforms = lib.platforms.linux;
  };
}
