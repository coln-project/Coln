{ mkDerivation, base, bytestring, containers, diagnostician
, filepath, hashable, lib, prettyprinter, QuickCheck, tasty
, tasty-golden, tasty-hunit, tasty-quickcheck, temporary, text
, vector, vector-hashtables
}:
mkDerivation {
  pname = "fnotation";
  version = "0.1.1.0";
  src = ./.;
  libraryHaskellDepends = [
    base containers diagnostician hashable prettyprinter text vector
    vector-hashtables
  ];
  testHaskellDepends = [
    base bytestring containers diagnostician filepath prettyprinter
    QuickCheck tasty tasty-golden tasty-hunit tasty-quickcheck
    temporary text vector
  ];
  license = "(Apache-2.0 OR MIT)";
}
