{ mkDerivation, base, containers, lib, prettyprinter, text, vector
}:
mkDerivation {
  pname = "diagnostician";
  version = "0.1.1.0";
  src = ./.;
  libraryHaskellDepends = [
    base containers prettyprinter text vector
  ];
  testHaskellDepends = [ base containers ];
  license = "(Apache-2.0 OR MIT)";
}
