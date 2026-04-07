{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in {
        devShells.default = pkgs.mkShell {
          name = "geolog";
          packages = with pkgs; with haskell.packages.ghc912; [
            haskell.compiler.ghc912
            cabal-install
            zlib
            pkg-config
            nodejs
          ];
        };
    });
}
