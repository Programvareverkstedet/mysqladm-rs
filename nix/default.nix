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
    installShellCompletions = lib.mapCartesianProduct ({ shell, command }: ''
      (
        export PATH="$out/bin:$PATH"
        export COMPLETE="${shell}"
        "${command}" > "$TMP/${command}.${shell}"
      )
      installShellCompletion "--${shell}" --cmd "${command}" "$TMP/${command}.${shell}"
    '') {
      shell = [ "bash" "zsh" "fish" ];
      command = [ "muscl" "mysql-dbadm" "mysql-useradm" ];
    };
  in ''
    ln -sr "$out/bin/muscl" "$out/bin/mysql-dbadm"
    ln -sr "$out/bin/muscl" "$out/bin/mysql-useradm"

    ${lib.concatStringsSep "\n" installShellCompletions}

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
