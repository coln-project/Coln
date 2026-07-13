{ mkDerivation, base, coln-compiler, containers, diagnostician
, exceptions, fnotation, lens, lib, lsp, megaparsec, mtl
, prettyprinter, stm, text, transformers, vector
}:
mkDerivation {
  pname = "coln-ls";
  version = "0.1.0.0";
  src = ./.;
  libraryHaskellDepends = [
    base coln-compiler containers diagnostician exceptions fnotation
    lens lsp megaparsec mtl prettyprinter stm text transformers vector
  ];
  license = "(Apache-2.0 OR MIT)";
}
