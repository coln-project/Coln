{ mkDerivation, aeson, aeson-pretty, base, bytestring
, coln-compiler, containers, diagnostician, directory, extra
, filepath, fnotation, hs-bindgen, hs-bindgen-runtime, lib
, ordered-containers, tasty, tasty-golden, tasty-hunit
, template-haskell, temporary, text
}:
mkDerivation {
  pname = "coln-store-hs";
  version = "0.1.0.0";
  src = ./.;
  libraryHaskellDepends = [
    base bytestring directory extra hs-bindgen hs-bindgen-runtime
    template-haskell text
  ];
  testHaskellDepends = [
    aeson aeson-pretty base bytestring coln-compiler containers
    diagnostician directory filepath fnotation ordered-containers tasty
    tasty-golden tasty-hunit temporary text
  ];
  license = "(Apache-2.0 OR MIT)";
}
