{
  description = "JagProx - a Hypixel proxy, with its launcher and the jagprox command";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      # The launcher is a native OpenGL (egui) app, so only Linux here.
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
          pkgs = pkgsFor system;
          pkg = self.packages.${system}.jagprox;
          launcher = {
            type = "app";
            program = "${pkg}/bin/jagprox-launcher";
          };
          # A `nix profile` install has no self-update (the store is read-only), so
          # `nix run github:JagHack/jagprox#enable-auto-update` installs a systemd
          # user timer that runs `nix profile upgrade` daily instead. Idempotent.
          enableAutoUpdate = {
            type = "app";
            program = "${pkgs.writeShellScript "jagprox-enable-auto-update" ''
              set -eu
              dir="$HOME/.config/systemd/user"
              mkdir -p "$dir"
              cat > "$dir/jagprox-update.service" <<EOF
              [Unit]
              Description=Update the nix-profile JagProx install
              [Service]
              Type=oneshot
              ExecStart=${pkgs.nix}/bin/nix profile upgrade --refresh jagprox
              Nice=15
              IOSchedulingClass=idle
              TimeoutStartSec=30min
              EOF
              cat > "$dir/jagprox-update.timer" <<EOF
              [Unit]
              Description=Check for JagProx updates daily
              [Timer]
              OnCalendar=daily
              Persistent=true
              RandomizedDelaySec=2h
              [Install]
              WantedBy=timers.target
              EOF
              ${pkgs.systemd}/bin/systemctl --user daemon-reload
              ${pkgs.systemd}/bin/systemctl --user enable --now jagprox-update.timer
              echo "JagProx will now update daily via nix profile upgrade."
              echo "Stop it with: systemctl --user disable --now jagprox-update.timer"
            ''}";
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
          enable-auto-update = enableAutoUpdate;
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
              cargo-packager
              cargo-generate-rpm
            ];
            # eframe loads these at run time; make `cargo run -p jagprox-gui` work in the shell.
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (
              with pkgs;
              [
                libGL
                libxkbcommon
                wayland
                xorg.libX11
                xorg.libXcursor
                xorg.libXi
                xorg.libXrandr
                fontconfig
                freetype
              ]
            );
          };
        }
      );

      formatter = forEach (system: (pkgsFor system).nixfmt-rfc-style);
    };
}
