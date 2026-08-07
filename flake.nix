{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    ghc-wasm-meta.url = "gitlab:haskell-wasm/ghc-wasm-meta?host=gitlab.haskell.org";
    # TODO when the library is available on Hackage, we'll want to use that
    # https://github.com/well-typed/hs-bindgen/issues/945
    # though we may still need it for `hsBindgenHook` which sets up LLVM/Clang paths
    hs-bindgen-src = {
      url = "github:well-typed/hs-bindgen";
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
            (import "${inputs.hs-bindgen-src}/nix/overlay" { inherit (nixpkgs) lib; }).default
            (import ./nix/haskell-packages.nix)
          ];
        };

        inherit (pkgs) colnHaskellPackages;
        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
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
          # TODO Claude-generated - the details could do with some more careful review
          coln-store-hs =
            let
              coln-store-ffi =
                (pkgs.makeRustPlatform {
                  cargo = rustToolchain;
                  rustc = rustToolchain;
                }).buildRustPackage
                  {
                    pname = "coln-store-ffi";
                    version = "0.1.0";
                    nativeBuildInputs = [ pkgs.cargo-expand ];
                    src = pkgs.lib.fileset.toSource {
                      root = ./.;
                      fileset = pkgs.lib.fileset.unions (
                        [
                          ./Cargo.toml
                          ./Cargo.lock
                        ]
                        ++ map (member: ./. + "/${member}") (
                          (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.members
                        )
                      );
                    };
                    cargoHash = "sha256-y9eGnr9zU5fmJA/T21XWFX8gXOqCXguoKFFpMJoJJNE=";
                    buildAndTestSubdir = "packages/coln-store-ffi";
                    doCheck = false;
                    installPhase = ''
                      runHook preInstall
                      mkdir -p $out/lib $out/include
                      cp "$(find target -name libCcoln_store_ffi.a | head -n1)" $out/lib/
                      cp "$(find target -name coln_store.h | head -n1)" $out/include/
                      runHook postInstall
                    '';
                  };
            in
            pkgs.haskell.lib.compose.overrideCabal
              (drv: {
                preConfigure = (drv.preConfigure or "") + ''
                  mkdir -p include
                  cp ${coln-store-ffi}/include/coln_store.h include/
                  mkdir -p ../coln-compiler/test/golden
                  cp ${./packages/coln-compiler/test/golden}/*.coln ../coln-compiler/test/golden/
                '';
                preBuild = (drv.preBuild or "") + ''
                  mkdir -p dist/build
                  cp ${coln-store-ffi}/lib/libCcoln_store_ffi.a dist/build/
                '';
                buildDepends = (drv.buildDepends or [ ]) ++ [ pkgs.hsBindgenHook ];
              })
              (
                colnHaskellPackages.callPackage ./packages/coln-store-hs {
                  inherit coln-compiler diagnostician fnotation;
                }
              );

          haskell-tests = pkgs.writeScript "haskell-tests" ''
            echo "built diagnostician: ${diagnostician}"
            echo "built diagnostician-terminal: ${diagnostician-terminal}"
            echo "built diagnostician-html: ${diagnostician-html}"
            echo "built fnotation: ${fnotation}"
            echo "built coln-compiler: ${coln-compiler}"
            echo "built coln-repl: ${coln-repl}"
            echo "built coln-ls: ${coln-ls}"
            echo "built coln-cli: ${coln-cli}"
            echo "built coln-store-hs: ${coln-store-hs}"
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
            cargo-expand
            cargo-llvm-cov
            cargo-nextest
            coln-manual-dev
            doxygen
            forester
            fourmolu
            esbuild
            haskell-wasm.wasm32-wasi-ghc-9_14
            haskell-wasm.wasm32-wasi-cabal-9_14
            haskell.compiler.ghc912
            haskell.packages.ghc912.haskell-language-server
            haskellPackages.cabal-gild
            hsBindgenHook
            jq
            just
            llvmPackages.llvm
            llvmPackages.libclang
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
