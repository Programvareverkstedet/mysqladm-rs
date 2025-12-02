#!/usr/bin/env bash

cargo build --release

mkdir -p assets/completions

COMPLETE=bash ./target/release/muscl > assets/completions/muscl.bash
COMPLETE=zsh ./target/release/muscl > assets/completions/_muscl
COMPLETE=fish ./target/release/muscl > assets/completions/muscl.fish

COMPLETE=bash ./target/release/mysql-dbadm > assets/completions/mysql-dbadm.bash
COMPLETE=zsh ./target/release/mysql-dbadm > assets/completions/_mysql-dbadm
COMPLETE=fish ./target/release/mysql-dbadm > assets/completions/mysql-dbadm.fish

COMPLETE=bash ./target/release/mysql-useradm > assets/completions/mysql-useradm.bash
COMPLETE=zsh ./target/release/mysql-useradm > assets/completions/_mysql-useradm
COMPLETE=fish ./target/release/mysql-useradm > assets/completions/mysql-useradm.fish

cargo deb
