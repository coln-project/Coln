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
        tools = pkgs: with (pkgs); [
          nodejs
          ormolu
          tectonic
          typescript
        ];
      in {
        devShells.default =
          let
            nixpkgs-haskell-tools = with pkgs; [
              haskell.compiler.ghc912
              cabal-install
              zlib
              zlib.dev
              pkg-config
            ];
          in
          pkgs.mkShell {
            name = "geolog";
            packages = tools pkgs ++ nixpkgs-haskell-tools;
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
                  shell.nativeBuildInputs = tools final;
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
  nixConfig = {
    # Binary Cache for haskell.nix  
    trusted-public-keys = [
      "hydra.iohk.io:f/Ea+s+dFdN+3Y/G+FDgSq+a5NEWhJGzdjvKNGv0/EQ="
    ];
    substituters = [
      "https://cache.iog.io"
    ];   
  };
}
