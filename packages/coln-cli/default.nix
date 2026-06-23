{ mkDerivation, base, coln-compiler, coln-ls, coln-repl
, diagnostician, diagnostician-terminal, fnotation, lib
, optparse-applicative, text
}:
mkDerivation {
  pname = "coln-cli";
  version = "0.1.0.0";
  src = ./.;
  isLibrary = false;
  isExecutable = true;
  executableHaskellDepends = [
    base coln-compiler coln-ls coln-repl diagnostician
    diagnostician-terminal fnotation optparse-applicative text
  ];
  license = "(Apache-2.0 OR MIT)";
  mainProgram = "coln";
}
