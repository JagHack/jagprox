# The `jagprox` command (CLI + headless proxy).
#   NixOS / nix:   nix-build nix -A jagprox-cli
#   fully static:  nix-build nix -A jagprox-cli-static
{ lib, rustPlatform }:

rustPlatform.buildRustPackage {
  pname = "jagprox-cli";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../config.yml
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  cargoBuildFlags = [ "-p" "jagprox-cli" ];
  cargoTestFlags = [ "--workspace" ];

  meta = {
    description = "JagProx - a Hypixel proxy";
    homepage = "https://github.com/JagHack/jagprox";
    license = lib.licenses.mit;
    mainProgram = "jagprox";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}
