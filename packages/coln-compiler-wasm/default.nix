{ mkDerivation, aeson, base, coln-compiler, containers
, diagnostician, diagnostician-html, fnotation, ghc-experimental
, lib, lucid, ordered-containers, prettyprinter, text
}:
mkDerivation {
  pname = "coln-compiler-wasm";
  version = "0.1";
  src = ./.;
  isLibrary = false;
  isExecutable = true;
  executableHaskellDepends = [
    aeson base coln-compiler containers diagnostician
    diagnostician-html fnotation ghc-experimental lucid
    ordered-containers prettyprinter text
  ];
  license = "(Apache-2.0 OR MIT)";
}
