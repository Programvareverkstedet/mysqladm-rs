{
  lib
, rustPlatform
, cargoToml
, cargoLock
, src
, installShellFiles
}:
let
  mainProgram = (lib.head cargoToml.bin).name;
in
rustPlatform.buildRustPackage {
  pname = cargoToml.package.name;
  version = cargoToml.package.version;
  inherit src;

  cargoLock.lockFile = cargoLock;

  nativeBuildInputs = [ installShellFiles ];
  postInstall = let
    commands = lib.mapCartesianProduct ({ shell, command }: ''
      "$out/bin/${mainProgram}" generate-completions --shell "${shell}" --command "${command}" > "$TMP/mysqladm.${shell}"
      installShellCompletion "--${shell}" --cmd "${command}" "$TMP/mysqladm.${shell}"
    '') {
      shell = [ "bash" "zsh" "fish" ];
      command = [ "mysqladm" "mysql-dbadm" "mysql-useradm" ];
    };
  in lib.concatStringsSep "\n" commands;

  meta = with lib; {
    license = licenses.mit;
    platforms = platforms.linux ++ platforms.darwin;
    inherit mainProgram;
  };
}
