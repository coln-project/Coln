{ mkDerivation, aeson, base, bytestring, containers, diagnostician
, directory, filepath, fnotation, hashable, lib, mtl
, ordered-containers, prettyprinter, tasty, tasty-expected-failure
, tasty-golden, temporary, text, vector, vector-hashtables
}:
mkDerivation {
  pname = "coln-compiler";
  version = "0.1.0.0";
  src = ./.;
  libraryHaskellDepends = [
    aeson base bytestring containers diagnostician directory filepath
    fnotation hashable mtl ordered-containers prettyprinter text vector
    vector-hashtables
  ];
  testHaskellDepends = [
    base bytestring containers diagnostician filepath fnotation
    ordered-containers prettyprinter tasty tasty-expected-failure
    tasty-golden temporary text vector
  ];
  license = "(Apache-2.0 OR MIT)";
}
