{ mkDerivation, async, base, bytestring, directory, extra, fsnotify
, lib, process, servant, servant-event-stream, servant-server
, temporary, text, warp
}:
mkDerivation {
  pname = "coln-manual-dev";
  version = "0.1.0.0";
  src = ./.;
  isLibrary = false;
  isExecutable = true;
  executableHaskellDepends = [
    async base bytestring directory extra fsnotify process servant
    servant-event-stream servant-server temporary text warp
  ];
  license = "(Apache-2.0 OR MIT)";
  mainProgram = "coln-manual-dev";
}
