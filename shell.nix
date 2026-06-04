{ system ? builtins.currentSystem }:

let
  coln = import ./. { inherit system; };
  inherit (coln) pkgs forester;
in
pkgs.mkShell {
  name = "coln";

  buildInputs = with pkgs; [
    cabal-install
    cabal2nix
    forester
    fourmolu
    haskell.compiler.ghc912
    haskellPackages.cabal-gild
    nodejs
    pkg-config
    tectonic
    typescript
    zlib
    zlib.dev
  ];
}
