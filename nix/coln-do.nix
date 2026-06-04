{ pkgs, stdenv, lib, colnHaskellPackages}:

let
  root = ../packages/coln-do;
in colnHaskellPackages.callCabal2nix "coln-do" root {}
