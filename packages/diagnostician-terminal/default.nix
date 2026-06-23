{ mkDerivation, ansi-terminal, base, diagnostician, lib
, prettyprinter, prettyprinter-ansi-terminal, text
}:
mkDerivation {
  pname = "diagnostician-terminal";
  version = "0.1.1.0";
  src = ./.;
  libraryHaskellDepends = [
    ansi-terminal base diagnostician prettyprinter
    prettyprinter-ansi-terminal text
  ];
  license = "(Apache-2.0 OR MIT)";
}
