{ mkDerivation, base, diagnostician, lib, lucid, prettyprinter
, prettyprinter-lucid, text
}:
mkDerivation {
  pname = "diagnostician-html";
  version = "0.1.0.0";
  src = ./.;
  libraryHaskellDepends = [
    base diagnostician lucid prettyprinter prettyprinter-lucid text
  ];
  license = "(Apache-2.0 OR MIT)";
}
