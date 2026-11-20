{
  description = "JagProx - a Hypixel proxy, with its launcher and the jagprox command";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      # The launcher needs WebKitGTK, so only Linux.
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forEach = nixpkgs.lib.genAttrs systems;
      pkgsFor = system: nixpkgs.legacyPackages.${system};
    in
    {
      packages = forEach (
        system:
        let
          pkgs = pkgsFor system;
        in
        rec {
          # The launcher + the jagprox command.
          jagprox = pkgs.callPackage ./nix/jagprox.nix { };
          default = jagprox;
          # Just the jagprox command (no GUI libraries).
          jagprox-cli = pkgs.callPackage ./nix/jagprox-cli.nix { };
        }
      );

      apps = forEach (
        system:
        let
          pkg = self.packages.${system}.jagprox;
          launcher = {
            type = "app";
            program = "${pkg}/bin/jagprox-launcher";
          };
        in
        {
          # `nix run github:JagHack/jagprox` opens the launcher.
          inherit launcher;
          default = launcher;
          # `nix run github:JagHack/jagprox#jagprox -- stats Notch`
          jagprox = {
            type = "app";
            program = "${pkg}/bin/jagprox";
          };
        }
      );

      # So an existing configuration.nix can add JagProx with a one-line overlay.
      overlays.default = final: _prev: {
        jagprox = final.callPackage ./nix/jagprox.nix { };
        jagprox-cli = final.callPackage ./nix/jagprox-cli.nix { };
      };

      devShells = forEach (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          # `nix develop` gives the launcher's build deps plus the dev tools, with the
          # same toolchain the package builds with.
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.jagprox ];
            packages = with pkgs; [
              clippy
              rustfmt
              cargo-tauri
              nodejs
            ];
          };
        }
      );

      formatter = forEach (system: (pkgsFor system).nixfmt-rfc-style);
    };
}
