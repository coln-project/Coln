{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    inputs@{
      self,
      nixpkgs,
      ...
    }:
    inputs.flake-utils.lib.eachSystem [ "x86_64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            (import ./nix/haskell-packages.nix)
          ];
        };

        inherit (pkgs) colnHaskellPackages;

        packages = let
          nuShellCheck = inputs: f: pkgs.stdenv.mkDerivation {
          name = "nuShellCheck";
          src = ./.;
          nativeBuildInputs = [pkgs.nushell] ++ inputs;
          buildPhase = ''
            nu ${f}
          '';
          installPhase = ''
            touch $out
          '';
        };
        in rec {
          forester = pkgs.callPackage ./nix/forester.nix { };

          coln-do = colnHaskellPackages.callPackage ./packages/coln-do { };
          diagnostician = colnHaskellPackages.callPackage ./packages/diagnostician { };
          fnotation = colnHaskellPackages.callPackage ./packages/fnotation {
            inherit diagnostician;
          };
          coln-compiler = colnHaskellPackages.callPackage ./packages/coln-compiler {
            inherit diagnostician fnotation;
          };
          coln-repl = colnHaskellPackages.callPackage ./packages/coln-repl {
            inherit coln-compiler diagnostician fnotation;
          };
          coln-ls = colnHaskellPackages.callPackage ./packages/coln-ls {
            inherit coln-compiler diagnostician fnotation;
          };
          coln-manual-dev = colnHaskellPackages.callPackage ./packages/coln-manual-dev {};
          coln-cli = colnHaskellPackages.callPackage ./packages/coln-cli {
            inherit coln-compiler coln-repl coln-ls diagnostician fnotation;
          };

          haskell-tests = pkgs.writeScript "haskell-tests" ''
            echo "built diagnostician: ${diagnostician}"
            echo "built fnotation: ${fnotation}"
            echo "built coln-compiler: ${coln-compiler}"
            echo "built coln-repl: ${coln-repl}"
            echo "built coln-ls: ${coln-ls}"
            echo "built coln-cli: ${coln-cli}"
          '';

          format-hs = nuShellCheck [pkgs.fourmolu] ./nix/checks/format-hs.nu;
          format-cabal = nuShellCheck [pkgs.haskellPackages.cabal-gild] ./nix/checks/format-cabal.nu;

          manual = pkgs.stdenv.mkDerivation {
            name = "coln-manual";

            src = ./manual;

            buildPhase = ''
              ${forester}/bin/forester build
            '';

            installPhase = ''
              cp -r output $out
            '';
          };

          default = coln-cli;
        };

        inherit (packages) forester coln-manual-dev;
      in
      {
        inherit packages;
        devShells.default = pkgs.mkShell {
          name = "coln";
          buildInputs = with pkgs; [
            cabal-install
            cabal2nix
            coln-manual-dev
            forester
            fourmolu
            haskell.compiler.ghc912
            haskell.packages.ghc912.haskell-language-server
            haskellPackages.cabal-gild
            nodejs
            pkg-config
            tectonic
            typescript
            zlib
            zlib.dev
          ];
        };
      });
  nixConfig = {
    extra-substituters = [ "https://coln.cachix.org" ];
    extra-trusted-public-keys = [ "coln.cachix.org-1:xplHZrvUVve3NSquwwW5QRl6MYbDBHx3rw3Np69kjw4=" ];
  };
}
