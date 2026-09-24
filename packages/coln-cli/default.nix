{ mkDerivation, aeson, base, coln-compiler, coln-ls, coln-repl
, diagnostician, diagnostician-terminal, filepath, fnotation, lib
, optparse-applicative, ordered-containers, text
}:
mkDerivation {
  pname = "coln-cli";
  version = "0.1.0.0";
  src = ./.;
  isLibrary = false;
  isExecutable = true;
  executableHaskellDepends = [
    aeson base coln-compiler coln-ls coln-repl diagnostician
    diagnostician-terminal filepath fnotation optparse-applicative
    ordered-containers text
  ];
  license = "(Apache-2.0 OR MIT)";
  mainProgram = "coln";
}
