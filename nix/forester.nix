{ pkgs, stdenv }:

let
  version =  "5.0-6e68237";
  os = "linux";
  arch = "x86_64";
  spec = "${version}-${os}-${arch}";
  url = "http://forester-builds.s3-website.us-east-2.amazonaws.com/forester-${spec}.tar.gz";
in stdenv.mkDerivation {
  pname = "forester";
  inherit version;

  src = pkgs.fetchzip {
    inherit url;
    hash = "sha256-Yt/XbUsLcfHjvIhYMW1lKIKl1U8aN6LKdUEYkvUuBbE=";
  };

  installPhase = ''
    mkdir -p $out
    cp -r . $out/
  '';
}
