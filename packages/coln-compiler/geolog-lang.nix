{ mkDerivation, base, bytestring, containers, directory, filepath
, hashable, lib, microlens-platform, mtl, prettyprinter, singletons
, singletons-th, symbolize, tasty, tasty-golden, temporary, text
, vector, vector-hashtables
}:
mkDerivation {
  pname = "geolog-lang";
  version = "0.1.0.0";
  src = ./.;
  libraryHaskellDepends = [
    base bytestring containers directory filepath hashable
    microlens-platform mtl prettyprinter singletons singletons-th
    symbolize text vector vector-hashtables
  ];
  testHaskellDepends = [
    base bytestring filepath prettyprinter tasty tasty-golden temporary
    text vector
  ];
  license = lib.licensesSpdx."BSD-3-Clause";
}
