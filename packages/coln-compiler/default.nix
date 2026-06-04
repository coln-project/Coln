{ nixpkgs ? import <nixpkgs> {}, compiler ? "ghc912" }:
  nixpkgs.pkgs.haskell.packages.${compiler}.callPackage ./geolog-lang.nix {}
