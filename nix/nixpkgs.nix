{ system ? builtins.currentSystem , ...}@args:

let
  nixpkgsTarball = builtins.fetchTarball {
    name   = "nixpkgs";
    url    = "https://github.com/nixos/nixpkgs/archive/8c50a710ddca43d7a530fb805ad55bde8d0141c5.tar.gz";
    sha256 = "0am8xx09fx5yf2p0wb001v0jx1g5hrfb76h4r37xph378jgk7pcr";
  };
in import nixpkgsTarball ({
  overlays = [ (import ./haskell-packages.nix) ];
} // args)
