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
  coln-manual-dev = colnHaskellPackages.callPackage ./packages/coln-manual-dev {};

  haskell-tests = pkgs.writeScript "haskell-tests" ''
    echo "built diagnostician: ${diagnostician}"
    echo "built fnotation: ${fnotation}"
    echo "built coln-compiler: ${coln-compiler}"
  '';
}
