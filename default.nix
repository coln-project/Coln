{ system ? builtins.currentSystem }:

let
  pkgs = import ./nix/nixpkgs.nix { inherit system; };
  inherit (pkgs) lib colnHaskellPackages;
in
rec {
  inherit pkgs lib;

  forester = pkgs.callPackage ./nix/forester.nix {};

  diagnostician = colnHaskellPackages.callPackage ./packages/diagnostician {};
  fnotation = colnHaskellPackages.callPackage ./packages/fnotation {
    inherit diagnostician;
  };
  coln-compiler = colnHaskellPackages.callPackage ./packages/coln-compiler {
    inherit diagnostician fnotation;
  };

  checks = pkgs.stdenv.mkDerivation {
    name = "checks";
    buildInputs = [
      diagnostician
      fnotation
      # coln-compiler
    ];
  };
}
