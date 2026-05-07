{
  description = "geomerge Rust workspace";

  inputs = {
    # Workspace uses edition 2024 (needs rustc/cargo >= 1.85). nixos-24.11 tops out around 1.82.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          name = "geomerge";
          packages = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            cargo-llvm-cov
          ];
        };
      }
    );
}
