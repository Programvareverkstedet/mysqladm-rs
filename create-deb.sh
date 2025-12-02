#!/usr/bin/env bash

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

cargo deb
