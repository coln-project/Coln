{ mkDerivation, base, bytestring, directory, djot, extra, lib
, shake, text
}:
mkDerivation {
  pname = "coln-do";
  version = "0.1.0.0";
  src = ./.;
  isLibrary = false;
  isExecutable = true;
  executableHaskellDepends = [
    base bytestring directory djot extra shake text
  ];
  license = "(Apache-2.0 OR MIT)";
  mainProgram = "coln-do";
}
