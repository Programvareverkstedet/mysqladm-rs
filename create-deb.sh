#!/usr/bin/env bash

cargo build --release

mkdir -p assets/completions

./target/release/muscl generate-completions --shell bash > assets/completions/muscl.bash
./target/release/muscl generate-completions --shell zsh > assets/completions/_muscl
./target/release/muscl generate-completions --shell fish > assets/completions/muscl.fish

./target/release/muscl generate-completions --shell bash --command mysql-dbadm > assets/completions/mysql-dbadm.bash
./target/release/muscl generate-completions --shell zsh --command mysql-dbadm > assets/completions/_mysql-dbadm
./target/release/muscl generate-completions --shell fish --command mysql-dbadm > assets/completions/mysql-dbadm.fish

./target/release/muscl generate-completions --shell bash --command mysql-useradm > assets/completions/mysql-useradm.bash
./target/release/muscl generate-completions --shell zsh --command mysql-useradm > assets/completions/_mysql-useradm
./target/release/muscl generate-completions --shell fish --command mysql-useradm > assets/completions/mysql-useradm.fish

cargo deb
