{
  lib
, rustPlatform
, cargoToml
, cargoLock
, src
, installShellFiles

, useCrane ? false
, craneLib ? null
}:
let
  mainProgram = (lib.head cargoToml.bin).name;
  buildFunction = if useCrane then craneLib.buildPackage else rustPlatform.buildRustPackage;
  cargoLock' = if useCrane then cargoLock else { lockFile = cargoLock; };
  pname = if useCrane then "${cargoToml.package.name}-crane" else cargoToml.package.name;
in
buildFunction {
  pname = pname;
  version = cargoToml.package.version;
  inherit src;

  cargoLock = cargoLock';

  nativeBuildInputs = [ installShellFiles ];
  postInstall = let
    commands = lib.mapCartesianProduct ({ shell, command }: ''
      "$out/bin/${mainProgram}" generate-completions --shell "${shell}" --command "${command}" > "$TMP/muscl.${shell}"
      installShellCompletion "--${shell}" --cmd "${command}" "$TMP/muscl.${shell}"
    '') {
      shell = [ "bash" "zsh" "fish" ];
      command = [ "muscl" "mysql-dbadm" "mysql-useradm" ];
    };
  in lib.concatStringsSep "\n" commands + ''
    install -Dm444 assets/systemd/muscl.socket -t "$out/lib/systemd/system"
    install -Dm644 assets/systemd/muscl.service -t "$out/lib/systemd/system"
    substituteInPlace "$out/lib/systemd/system/muscl.service" \
      --replace-fail '/usr/bin/muscl' "$out/bin/muscl"
  '';

  meta = with lib; {
    license = licenses.mit;
    platforms = platforms.linux ++ platforms.darwin;
    inherit mainProgram;
  };
}
