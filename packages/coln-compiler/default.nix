{ mkDerivation, aeson, base, bytestring, containers, diagnostician
, directory, filepath, fnotation, hashable, keys, lib, mtl
, ordered-containers, prettyprinter, tasty, tasty-expected-failure
, tasty-golden, tasty-hunit, temporary, text, vector
, vector-hashtables
}:
mkDerivation {
  pname = "coln-compiler";
  version = "0.1.0.0";
  src = ./.;
  libraryHaskellDepends = [
    aeson base bytestring containers diagnostician directory filepath
    fnotation hashable keys mtl ordered-containers prettyprinter text
    vector vector-hashtables
  ];
  testHaskellDepends = [
    base bytestring containers diagnostician directory filepath
    fnotation ordered-containers prettyprinter tasty
    tasty-expected-failure tasty-golden tasty-hunit temporary text
    vector
  ];
  license = "(Apache-2.0 OR MIT)";
}
