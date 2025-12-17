# Compiling and packaging

This document describes how to compile `muscl` from source code, along with other related tasks.

## Build

To just compile `muscl`, there is not many special steps needed.
You need to have a working [Rust toolchain](https://www.rust-lang.org/tools/install) installed.

```bash
# Compile in debug mode
cargo build
ls target/debug # muscl, mysql-dbadm, mysql-useradm, ...

# Compile in release mode
cargo build --release
ls target/release # muscl, mysql-dbadm, mysql-useradm, ...

# Compile in release mode with link time optimization (only used for distribution builds)
cargo build --profile release-lto
ls target/release-lto # muscl, mysql-dbadm, mysql-useradm, ...
```

## Generating completions

> [!NOTE]
> This happens automatically when building the deb package, so you can skip this step if that's the goal.

In order to generate shell completions that work correctly, you need to put `muscl` (or alias symlinks) in your `$PATH`.

```bash
cargo build --release
(
  PATH="$(pwd)/target/release:$PATH"
  mkdir -p completions/bash
  mkdir -p completions/zsh
  mkdir -p completions/fish

  muscl completions --shell bash > completions/bash/muscl.bash
  muscl completions --shell zsh > completions/zsh/_muscl
  muscl completions --shell fish > completions/fish/muscl.fish
)
```

Due to a [bug in clap](https://github.com/clap-rs/clap/issues/1764), you will also need to edit the completion files for the aliases.

```bash
sed -i 's/muscl/mysql-dbadm/g' assets/completions/{mysql-dbadm.bash,mysql-dbadm.fish,_mysql-dbadm}
sed -i 's/muscl/mysql-useradm/g' assets/completions/{mysql-useradm.bash,mysql-useradm.fish,_mysql-useradm}
```

## Bundling into a deb package

We have a script that automates the process of building a deb package for Debian-based systems.

Before running this, you will need to install `cargo-deb` and make sure you have `dpkg-deb` available on your system.

```bash
# Install cargo-deb if you don't have it already
cargo install cargo-deb

# Run the script to create the deb package
./scripts/create-deb.sh

# Inspect the resulting deb package
dpkg --contents target/debian/muscl_*.deb
dpkg --info target/debian/muscl_*.deb
```

The program will be built with the `release-lto` profile, so it can be a bit slower to build than a normal build.

## Compiling with CI

We have a pipeline that builds the deb package for a set of different distributions.

If you have access, you can trigger a build manually here: https://git.pvv.ntnu.no/Projects/muscl/actions?workflow=publish-deb.yml
