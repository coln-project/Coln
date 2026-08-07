{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    ghc-wasm-meta.url = "gitlab:haskell-wasm/ghc-wasm-meta?host=gitlab.haskell.org";
    wasm-bindgen-hs = {
      url = "github:georgefst/wasm-bindgen-hs/72c12029c6f3f7a21d7cfe4396f898f3f7a38313";
      flake = false;
    };
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
          extensions = [ "rust-src" "rust-analyzer" ];
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
          # `cabal npm`, the NPM packager from wasm-bindgen-hs. There's no Nix
          # expression upstream yet, so we spell out the dependencies here.
          cabal-npm = colnHaskellPackages.callPackage (
            { mkDerivation, aeson, aeson-pretty, base, bytestring, directory
            , filepath, optparse-applicative, process, temporary, text
            }:
            mkDerivation {
              pname = "cabal-npm";
              version = "0.1.0.0";
              src = inputs.wasm-bindgen-hs;
              postUnpack = "sourceRoot+=/cabal-npm";
              isLibrary = false;
              isExecutable = true;
              executableHaskellDepends = [
                aeson aeson-pretty base bytestring directory filepath
                optparse-applicative process temporary text
              ];
              license = "unknown";
              mainProgram = "cabal-npm";
            }
          ) { };

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

          # The demo site, built the same way a developer would with `just`:
          # these only add the toolchains that the dev shell would otherwise
          # provide. Each package writes straight in to its own slot under
          # `_build/web`, so there's nothing to assemble afterwards.
          build-sync-demo = webDemoBuilder {
            name = "build-sync-demo";
            toolchain = syncDemoToolchain;
            recipe = "build-web-sync";
            built = "sync demo";
            outDir = "_build/web/sync";
          };
          build-web-demos = webDemoBuilder {
            name = "build-web-demos";
            toolchain = syncDemoToolchain ++ compilerDemoToolchain;
            recipe = "build-web";
            built = "demo site";
            outDir = "_build/web";
            hackageIndexState = "2026-07-15T17:07:49Z";
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
        haskell-wasm = inputs.ghc-wasm-meta.packages.${system};

        # Everything `just examples/build-web-sync` shells out to: the Coln CLI
        # for `compile.sh`, and the Rust toolchain behind `coln-js-runtime`.
        syncDemoToolchain = [
          packages.coln-cli
          packages.wasm-bindgen-cli
          packages.wasm-bodge
          pkgs.binaryen
          pkgs.esbuild
          rustToolchain
        ];
        # ...and what the compiler demo adds: GHC's Wasm backend, plus `cabal
        # npm` from wasm-bindgen-hs. `git` because cabal fetches that as a
        # `source-repository-package`.
        compilerDemoToolchain = [
          packages.cabal-npm
          pkgs.cabal-install
          pkgs.git
          haskell-wasm.wasm32-wasi-cabal-9_14
          haskell-wasm.wasm32-wasi-ghc-9_14
        ];
        # The build logic itself lives in `examples/justfile`, so that CI and a
        # developer in the dev shell run exactly the same thing.
        webDemoBuilder =
          { name, toolchain, recipe, built, outDir, hackageIndexState ? null }:
          pkgs.writeShellApplication {
            inherit name;
            runtimeInputs = toolchain ++ [
              pkgs.bash
              pkgs.coreutils
              # pnpm's `node_modules/.bin` shims call out to `sed`
              pkgs.gnused
              pkgs.just
              pkgs.nodejs_24
              pkgs.pnpm
            ];
            text = ''
              repo_root="''${1:-$PWD}"
              cd "$repo_root"

              # pnpm treats a lockfile as frozen when CI is set, which is what
              # we want here whoever is running it
              export CI="''${CI:-1}"
              export PNPM_STORE_DIR="''${PNPM_STORE_DIR:-$repo_root/.pnpm-store}"
              # GCC 15 (nixos-26.05) defaults to -std=gnu23, which removed
              # ATOMIC_VAR_INIT, breaking mimalloc-rust-sys via dbsp. Mirrors
              # the dev shell's shellHook.
              export CFLAGS="''${CFLAGS:+$CFLAGS }-std=gnu17"

              ${pkgs.lib.optionalString (hackageIndexState != null) ''
                wasm32-wasi-cabal update 'hackage.haskell.org,${hackageIndexState}'
              ''}
              just examples/${recipe}

              echo "Built ${built} at $repo_root/${outDir}"
            '';
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
          app = p: { type = "app"; program = "${pkgs.lib.getExe p}"; };
          buildSyncDemo = app packages.build-sync-demo;
          buildWebDemos = app packages.build-web-demos;
        in {
          build-sync-demo = buildSyncDemo;
          sync-demo = buildSyncDemo;
          build-web-demos = buildWebDemos;
          web-demos = buildWebDemos;
        };
        devShells.default = pkgs.mkShell {
          name = "coln";
          buildInputs = with pkgs; [
            cabal-install
            cabal2nix
            packages.cabal-npm
            cargo-llvm-cov
            cargo-nextest
            coln-manual-dev
            forester
            fourmolu
            esbuild
            haskell-wasm.wasm32-wasi-ghc-9_14
            haskell-wasm.wasm32-wasi-cabal-9_14
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
