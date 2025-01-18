#!/usr/bin/env bash

cargo build --release

mkdir -p assets/completions

./target/release/mysqladm generate-completions --shell bash > assets/completions/mysqladm.bash
./target/release/mysqladm generate-completions --shell zsh > assets/completions/_mysqladm
./target/release/mysqladm generate-completions --shell fish > assets/completions/mysqladm.fish

./target/release/mysqladm generate-completions --shell bash --command mysql-dbadm > assets/completions/mysql-dbadm.bash
./target/release/mysqladm generate-completions --shell zsh --command mysql-dbadm > assets/completions/_mysql-dbadm
./target/release/mysqladm generate-completions --shell fish --command mysql-dbadm > assets/completions/mysql-dbadm.fish

./target/release/mysqladm generate-completions --shell bash --command mysql-useradm > assets/completions/mysql-useradm.bash
./target/release/mysqladm generate-completions --shell zsh --command mysql-useradm > assets/completions/_mysql-useradm
./target/release/mysqladm generate-completions --shell fish --command mysql-useradm > assets/completions/mysql-useradm.fish

cargo deb