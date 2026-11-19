{ pkgs ? import <nixpkgs> {} }:
{
  jagprox-cli = pkgs.callPackage ./jagprox-cli.nix {};
  jagprox-cli-static = pkgs.pkgsStatic.callPackage ./jagprox-cli.nix {};
}
