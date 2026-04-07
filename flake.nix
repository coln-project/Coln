{
  inputs = {
    haskell-nix.url = "github:input-output-hk/haskell.nix";
    nixpkgs.follows = "haskell-nix/nixpkgs-2511";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      haskell-nix,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
      let
        pkgs = import nixpkgs { inherit system; };
        extra-tools = pkgs: with (pkgs); [
          nodejs
          typescript
        ];
      in {
        devShells.default = pkgs.mkShell {
          name = "geolog";
          packages = with pkgs; extra-tools pkgs ++ [
            haskell.compiler.ghc912
            cabal-install
            zlib
            zlib.dev
            pkg-config
          ];
        };
        devShells.haskell-nix =
          let
            overlays = [
              haskell-nix.overlay
              (final: _prev: {
                hixProject = final.haskell-nix.hix.project {
                  src = ./.;
                  evalSystem = "x86_64-linux";
                  name = "geolog";
                  compiler-nix-name = "ghc9122";
                  shell.tools.cabal = "latest";
                  shell.withHoogle = false;
                  shell.tools.haskell-language-server = "latest";
                  shell.nativeBuildInputs = extra-tools final;
                };
              })
            ];
            pkgs-hnix = import nixpkgs {
              inherit system overlays;
              inherit (haskell-nix) config;
            };
            project = pkgs-hnix.hixProject.flake { };
          in
            project.devShells.default;
    });
}
