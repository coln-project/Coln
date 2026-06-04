{ system ? builtins.currentSystem }:

let
  coln = import ./. { inherit system; };
  inherit (coln) pkgs coln-do;
in
pkgs.mkShell {
  name = "coln";

  buildInputs = with pkgs; [
    nodejs
    fourmolu
    tectonic
    typescript
    haskellPackages.cabal-gild
    haskell.compiler.ghc912
    cabal-install
    zlib
    zlib.dev
    pkg-config
    coln-do
  ];
}
