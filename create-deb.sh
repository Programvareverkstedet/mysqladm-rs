#!/usr/bin/env bash

set -euo pipefail

cargo build --release

mkdir -p assets/completions

(
  PATH="./target/release:$PATH"

  COMPLETE=bash muscl > assets/completions/muscl.bash
  COMPLETE=zsh muscl > assets/completions/_muscl
  COMPLETE=fish muscl > assets/completions/muscl.fish

  COMPLETE=bash mysql-dbadm > assets/completions/mysql-dbadm.bash
  COMPLETE=zsh mysql-dbadm > assets/completions/_mysql-dbadm
  COMPLETE=fish mysql-dbadm > assets/completions/mysql-dbadm.fish

  COMPLETE=bash mysql-useradm > assets/completions/mysql-useradm.bash
  COMPLETE=zsh mysql-useradm > assets/completions/_mysql-useradm
  COMPLETE=fish mysql-useradm > assets/completions/mysql-useradm.fish
)

# See https://github.com/clap-rs/clap/issues/1764
sed -i 's/muscl/mysql-dbadm/g' assets/completions/{mysql-dbadm.bash,mysql-dbadm.fish,_mysql-dbadm}
sed -i 's/muscl/mysql-useradm/g' assets/completions/{mysql-useradm.bash,mysql-useradm.fish,_mysql-useradm}

DEFAULT_CARGO_DEB_ARGS=(
  --no-build
)

cargo deb "${DEFAULT_CARGO_DEB_ARGS[@]}" "$@"
