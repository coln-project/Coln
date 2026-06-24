{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    ghc-wasm-meta.url = "gitlab:haskell-wasm/ghc-wasm-meta?host=gitlab.haskell.org";
  };
  outputs =
    inputs@{
      self,
      nixpkgs,
      rust-overlay,
      ...
    }:
    inputs.flake-utils.lib.eachSystem [ "x86_64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            rust-overlay.overlays.default
            (import ./nix/haskell-packages.nix)
          ];
        };

        inherit (pkgs) colnHaskellPackages;
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "wasm32-unknown-unknown" ];
        };

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
          diagnostician-terminal = colnHaskellPackages.callPackage ./packages/diagnostician-terminal {
            inherit diagnostician;
          };
          diagnostician-html = colnHaskellPackages.callPackage ./packages/diagnostician-html {
            inherit diagnostician;
          };
          fnotation = colnHaskellPackages.callPackage ./packages/fnotation {
            inherit diagnostician;
          };
          coln-compiler = colnHaskellPackages.callPackage ./packages/coln-compiler {
            inherit diagnostician fnotation;
          };
          coln-repl = colnHaskellPackages.callPackage ./packages/coln-repl {
            inherit coln-compiler diagnostician diagnostician-terminal fnotation;
          };
          coln-ls = colnHaskellPackages.callPackage ./packages/coln-ls {
            inherit coln-compiler diagnostician fnotation;
          };
          coln-manual-dev = colnHaskellPackages.callPackage ./packages/coln-manual-dev {};
          coln-cli = colnHaskellPackages.callPackage ./packages/coln-cli {
            inherit coln-compiler coln-repl coln-ls diagnostician diagnostician-terminal fnotation;
          };

          haskell-tests = pkgs.writeScript "haskell-tests" ''
            echo "built diagnostician: ${diagnostician}"
            echo "built diagnostician-terminal: ${diagnostician-terminal}"
            echo "built diagnostician-html: ${diagnostician-html}"
            echo "built fnotation: ${fnotation}"
            echo "built coln-compiler: ${coln-compiler}"
            echo "built coln-repl: ${coln-repl}"
            echo "built coln-ls: ${coln-ls}"
            echo "built coln-cli: ${coln-cli}"
          '';

          wasm-bodge = pkgs.rustPlatform.buildRustPackage rec {
            pname = "wasm-bodge";
            version = "0.3.1";

            src = pkgs.fetchCrate {
              inherit pname version;
              hash = "sha256-Vr+ribYXO7+TpXzH8nlbp5cPg5I0lcxXjTfQNwkg3/Y=";
            };

            cargoHash = "sha256-tARojdKFjnkCeJIhgpMFEvfxrOTOH8L3cAvE2UQm0jY=";

            doCheck = false;
          };

          wasm-bindgen-cli = pkgs.rustPlatform.buildRustPackage rec {
            pname = "wasm-bindgen-cli";
            version = "0.2.125";

            src = pkgs.fetchCrate {
              inherit pname version;
              hash = "sha256-zRawtjxMOdTMX+mZaiNR3YYfTiZJhf9qj7kXSSeMxrc=";
            };

            cargoHash = "sha256-aZCfgR23Qb0Pn4Mm4ToMtuuRQqSJjXCR9li/VvP5CTM=";

            doCheck = false;
          };

          build-web-demo = pkgs.writeShellApplication {
            name = "build-web-demo";
            runtimeInputs = [
              coln-cli
              pkgs.binaryen
              pkgs.esbuild
              pkgs.nodejs
              pkgs.pnpm
              rustToolchain
              wasm-bindgen-cli
              wasm-bodge
            ];
            text = ''
              repo_root="''${1:-$PWD}"
              cd "$repo_root"

              export CI="''${CI:-1}"
              pnpm_store_dir="''${PNPM_STORE_DIR:-$repo_root/.pnpm-store}"

              npm ci --prefix packages/coln-js-runtime
              npm run --prefix packages/coln-js-runtime build

              pnpm --dir examples/web-demo install --frozen-lockfile --store-dir "$pnpm_store_dir"
              pnpm --dir examples/web-demo build

              echo "Built web demo at $repo_root/examples/web-demo/dist"
            '';
          };

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
        haskell-wasm = inputs.ghc-wasm-meta.packages.${system}.default;
      in
      {
        inherit packages;
        apps = let
          buildWebDemo = {
            type = "app";
            program = "${pkgs.lib.getExe packages.build-web-demo}";
          };
        in {
          build-web-demo = buildWebDemo;
          web-demo = buildWebDemo;
        };
        devShells.default = pkgs.mkShell {
          name = "coln";
          buildInputs = with pkgs; [
            cabal-install
            cabal2nix
            coln-manual-dev
            forester
            fourmolu
            esbuild
            haskell-wasm
            haskell.compiler.ghc912
            haskell.packages.ghc912.haskell-language-server
            haskellPackages.cabal-gild
            jq
            just
            nodejs
            pnpm
            packages.wasm-bodge
            rustToolchain
            packages.wasm-bindgen-cli
            binaryen
            openssl
            pkg-config
            reuse
            simple-http-server
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
