{ nixpkgs ? import <nixpkgs> {}, compiler ? "ghc910" }:
  (import ./default.nix { inherit nixpkgs compiler; }).env
