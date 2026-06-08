{ mkDerivation, base, bytestring, containers, diagnostician
, directory, filepath, fnotation, hashable, lib, ordered-containers
, prettyprinter, tasty, tasty-golden, temporary, text, vector
, vector-hashtables
}:
mkDerivation {
  pname = "coln-compiler";
  version = "0.1.0.0";
  src = ./.;
  libraryHaskellDepends = [
    base bytestring containers diagnostician directory filepath
    fnotation hashable ordered-containers prettyprinter text vector
    vector-hashtables
  ];
  testHaskellDepends = [
    base bytestring containers diagnostician filepath fnotation
    ordered-containers prettyprinter tasty tasty-golden temporary text
    vector
  ];
  license = "(Apache-2.0 OR MIT)";
}
