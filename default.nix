{ system ? builtins.currentSystem }:

let
  pkgs = import ./nix/nixpkgs.nix { inherit system; };
  inherit (pkgs) lib;

  coln-do = pkgs.callPackage ./nix/coln-do.nix { };
in
{
  inherit pkgs lib coln-do;
}
# pkgs.stdenv.mkDerivation rec {
#   name = "coln";

#   buildInputs = [ coln-do ];
# }
