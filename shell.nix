{ system ? builtins.currentSystem }:

let
  coln = import ./. { inherit system; };
  inherit (coln) pkgs forester coln-manual-dev;
in
pkgs.mkShell {
  name = "coln";

  buildInputs = with pkgs; [
    cabal-install
    cabal2nix
    coln-manual-dev
    forester
    fourmolu
    haskell.compiler.ghc912
    haskellPackages.cabal-gild
    haskellPackages.ghcid
    nodejs
    pkg-config
    tectonic
    typescript
    zlib
    zlib.dev
  ];
}
