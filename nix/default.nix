{
  lib
, rustPlatform
, cargoToml
, cargoLock
, src
}:
let
in
rustPlatform.buildRustPackage {
  pname = cargoToml.package.name;
  version = cargoToml.package.version;
  inherit src;

  cargoLock.lockFile = cargoLock;

  meta = with lib; {
    license = licenses.mit;
    platforms = platforms.linux ++ platforms.darwin;
    mainProgram = (lib.head cargoToml.bin).name;
  };
}
