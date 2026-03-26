{
  inputs = {
    haskellNix.url = "github:input-output-hk/haskell.nix";
    nixpkgs.follows = "haskellNix/nixpkgs-2511";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      haskellNix,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" ] (
      system:
      let
        overlays = [
          haskellNix.overlay
          (final: _prev: {
            hixProject = final.haskell-nix.hix.project {
              src = ./.;
              evalSystem = "x86_64-linux";
              name = "geolog";
              compiler-nix-name = "ghc9122";
              shell.tools.cabal = "latest";
              shell.withHoogle = false;
              shell.tools.haskell-language-server = "latest";
            };
          })
        ];
        pkgs = import nixpkgs {
          inherit system overlays;
          inherit (haskellNix) config;
        };
        project = pkgs.hixProject.flake { };
        haskellDirs = "^geolog-lang/";
      in
      {
        devShells.default = pkgs.mkShell {
          name = "geolog";
          inputsFrom = [
            project.devShells.default
          ];
          packages = with pkgs; [
            ghcid
            nodejs
          ];
        };

        checks = (project.checks // {
          formatting = pkgs.stdenv.mkDerivation {
          name = "check formatting";
          src = self;

          nativeBuildInputs = [ pkgs.haskellPackages.ormolu ];
          doCheck = true;

          checkPhase = /* sh */ ''
            failed=()
            succeeded=0

            readarray -t files < <(find . -name '*.hs' -printf '%P\n'| grep -E '${haskellDirs}')

            for file in "''${files[@]}"; do
              if ! ormolu -m 'check' "$file" >/dev/null 2>&1; then
                failed+=("$file")
              else
                ((succeeded+=1))
              fi
            done

            printf '%d files succeeded.\n' "''${succeeded}"
            printf '%d files failed:\n' "''${#failed[@]}"
            printf '%s\n' "''${failed[@]}"

            if [ "''${#failed[@]}" -ne 0 ]; then
              exit 1
            fi
          '';

          installPhase = ''
            touch $out
          '';
        };
      });

        formatter = (
          pkgs.writeShellApplication {
            name = "format-haskell";
            runtimeInputs = [
              pkgs.haskellPackages.ormolu
              pkgs.git
            ];
            text = ''
              failed=()
              succeeded=0

              readarray -t files < <(git ls-files '*.hs' | grep -E '${haskellDirs}')

              for file in "''${files[@]}"; do
                if ! ormolu -m 'check' "$file" >/dev/null 2>&1; then
                  if ! ormolu -i "$file"; then
                    failed+=("$file")
                    printf 'Failed to format %s\n' "$file"
                  else
                    ((succeeded+=1))
                    printf 'Formatted %s\n' "$file"
                  fi
                fi
              done

              printf 'Formatted %d files' "''${succeeded}"

              if [ "''${#failed[@]}" -ne 0 ]; then
                printf '%d Files failed to format:\n' "''${#failed[@]}"
                printf '%s\n' "''${failed[@]}"
              fi
            '';
          }
        );
        packages.geolog-lsp = project.packages."geolog-lsp:exe:geolog-lsp";
    });
}
