{ pkgs ? import <nixpkgs> {} }:
{
  # The full app: jagprox + jagprox-launcher.
  jagprox = pkgs.callPackage ./jagprox.nix {};
  # Only the jagprox command (no GUI libraries).
  jagprox-cli = pkgs.callPackage ./jagprox-cli.nix {};
  jagprox-cli-static = pkgs.pkgsStatic.callPackage ./jagprox-cli.nix {};
}
