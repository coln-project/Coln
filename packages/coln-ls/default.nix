{ mkDerivation, base, coln-compiler, containers, diagnostician
, exceptions, fnotation, lens, lib, lsp, lsp-test, megaparsec, mtl
, prettyprinter, process, stm, tasty, tasty-hunit, text
, transformers, vector
}:
mkDerivation {
  pname = "coln-ls";
  version = "0.1.0.0";
  src = ./.;
  isLibrary = true;
  isExecutable = true;
  libraryHaskellDepends = [
    base coln-compiler containers diagnostician exceptions fnotation
    lens lsp megaparsec mtl prettyprinter stm text transformers vector
  ];
  executableHaskellDepends = [ base ];
  testHaskellDepends = [
    base lsp-test process tasty tasty-hunit text
  ];
  license = "(Apache-2.0 OR MIT)";
  mainProgram = "coln-language-server";
}
