{
  inputs = {
    nixpkgs.url = "github:georgefst/nixpkgs/ghc-wasm";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };
  outputs =
    inputs@{
      self,
      nixpkgs,
      rust-overlay,
      ...
    }:
    inputs.flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-darwin" ] (
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

          build-sync-demo = pkgs.writeShellApplication {
            name = "build-sync-demo";
            runtimeInputs = [
              coln-cli
              pkgs.binaryen
              pkgs.esbuild
              pkgs.nodejs_24
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

              pnpm --dir examples/sync-demo install --frozen-lockfile --store-dir "$pnpm_store_dir"
              pnpm --dir examples/sync-demo build

              echo "Built sync demo at $repo_root/examples/sync-demo/dist"
            '';
          };

          # TODO find some way to DRY this with `just examples/build-web-compiler`
          # TODO do the combined web-demo build in Nix, removing logic from YAML
          web-compiler = pkgs.runCommand "coln-web-compiler"
            {
              nativeBuildInputs = [
                pkgs.nodejs
                pkgs.jq
              ];
            }
            ''
              dist=$out/dist
              mkdir -p $dist
              cp ${wasmColnPackages.coln-compiler-wasm}/bin/coln-compiler-wasm* $dist/coln.wasm
              libdir=$(${wasmHaskellPackages.ghc}/bin/${wasmHaskellPackages.ghc.targetPrefix}ghc --print-libdir)
              node "$libdir/post-link.mjs" --input $dist/coln.wasm --output $dist/ghc_wasm_jsffi.js
              cp ${./packages/coln-compiler-wasm/loadHaskellWasm.js} $dist/loadHaskellWasm.js
              cp ${./examples/compiler-demo/index.html} $out/index.html
              cp ${./examples/style.css} $out/style.css
              mkdir -p $out/examples
              cp ${./packages/coln-compiler/test/golden}/*.coln $out/examples/
              ls -1 $out/examples/*.coln | xargs -n1 basename | jq -nR '[inputs]' > $out/examples/index.json
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

          vscode-extension = pkgs.buildNpmPackage {
            pname = "coln-vscode-extension";
            version = "0.1.0";

            src = ./packages/coln-ls/client;

            npmDeps = lsClientNpmDeps;
            npmConfigHook = pkgs.importNpmLock.npmConfigHook;

            postUnpack = ''
              cp ${coln-cli}/bin/coln $sourceRoot/
              cp -r ${./LICENSES} $sourceRoot/LICENSES
              cat ${./LICENSES}/Apache-2.0.txt ${./LICENSES}/MIT.txt > $sourceRoot/LICENSE
            '';

            postPatch = ''
              substituteInPlace package.json \
                --replace-fail "cp -r ../../../LICENSES LICENSES" "true"
            '';

            nativeBuildInputs = [ pkgs.vsce ];
            dontNpmBuild = true;

            buildPhase = ''
              vsce package --allow-missing-repository
            '';

            installPhase = ''
              cp *.vsix $out
            '';
          };

          default = coln-cli;
        };


        inherit (packages) forester coln-manual-dev;
        # TODO DRY with native builds
        wasmHaskellPackages = pkgs.pkgsCross.wasi32.haskell.packages.ghc9141.override {
          overrides = _: prev: {
            # mirrors cabal.project
            # TODO these are actually about GHC 9.14 compatibility rather than Wasm
            ordered-containers = pkgs.haskell.lib.doJailbreak prev.ordered-containers;
            prettyprinter-lucid = pkgs.haskell.lib.doJailbreak prev.prettyprinter-lucid;
          };
        };
        wasmColnPackages = rec {
          diagnostician = wasmHaskellPackages.callPackage ./packages/diagnostician { };
          diagnostician-html = wasmHaskellPackages.callPackage ./packages/diagnostician-html {
            inherit diagnostician;
          };
          fnotation = wasmHaskellPackages.callPackage ./packages/fnotation {
            inherit diagnostician;
          };
          coln-compiler = wasmHaskellPackages.callPackage ./packages/coln-compiler {
            inherit diagnostician fnotation;
          };
          coln-compiler-wasm = wasmHaskellPackages.callPackage ./packages/coln-compiler-wasm {
            inherit coln-compiler diagnostician diagnostician-html fnotation;
          };
        };
        lsTsDir = ./packages/coln-ls/client;
        lsClientNpmDeps = pkgs.importNpmLock {
          npmRoot = lsTsDir;
        };
        lsClientNodeModules = pkgs.importNpmLock.buildNodeModules {
          npmRoot = lsTsDir;
          nodejs = pkgs.nodejs_24;
        };
      in
      {
        inherit packages;
        apps = let
          buildSyncDemo = {
            type = "app";
            program = "${pkgs.lib.getExe packages.build-sync-demo}";
          };
        in {
          build-sync-demo = buildSyncDemo;
          sync-demo = buildSyncDemo;
        };
        devShells.default = pkgs.mkShell {
          name = "coln";
          buildInputs = with pkgs; [
            cabal-install
            cabal2nix
            cargo-llvm-cov
            clippy
            coln-manual-dev
            forester
            fourmolu
            esbuild
            wasmHaskellPackages.ghc
            (pkgs.writeShellScriptBin "wasm32-unknown-wasi-cabal" ''
              exec ${pkgs.cabal-install}/bin/cabal --with-compiler=${wasmHaskellPackages.ghc.unprefixed}/bin/ghc "$@"
            '')
            haskell.compiler.ghc912
            haskell.packages.ghc912.haskell-language-server
            haskellPackages.cabal-gild
            jq
            just
            nodejs_24
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
            vtsls
            zlib
            zlib.dev
          ];
          shellHook = ''
            # GCC 15 (nixos-26.05) defaults to -std=gnu23 which removed ATOMIC_VAR_INIT.
            # This breaks mimalloc-rust-sys, which is a dependency of dbsp.
            export CFLAGS="''${CFLAGS:+$CFLAGS }-std=gnu17"
          '';
        };
        devShells.vscode-extension = pkgs.mkShell {
          name = "coln-vscode-extension";
          buildInputs = with pkgs; [
            nodejs_24
            typescript
            vtsls
          ];
          shellHook = ''
            ln -sfn ${lsClientNodeModules}/node_modules "$PWD"/node_modules
          '';
        };
      });
  nixConfig = {
    extra-substituters = [ "https://coln.cachix.org" ];
    extra-trusted-public-keys = [ "coln.cachix.org-1:xplHZrvUVve3NSquwwW5QRl6MYbDBHx3rw3Np69kjw4=" ];
  };
}
