{ mkDerivation, base, bytestring, coln-compiler, containers
, diagnostician, diagnostician-terminal, directory, filepath
, fnotation, hashable, haskeline, lib, mtl, ordered-containers
, prettyprinter, repline, singletons, text, transformers, vector
}:
mkDerivation {
  pname = "coln-repl";
  version = "0.1.0.0";
  src = ./.;
  libraryHaskellDepends = [
    base bytestring coln-compiler containers diagnostician
    diagnostician-terminal directory filepath fnotation hashable
    haskeline mtl ordered-containers prettyprinter repline singletons
    text transformers vector
  ];
  license = "(Apache-2.0 OR MIT)";
}
