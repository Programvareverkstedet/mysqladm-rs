{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay }@inputs:
  let
    inherit (nixpkgs) lib;

    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
      "armv7l-linux"
    ];

    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          (import rust-overlay)
        ];
      };

      rust-bin = rust-overlay.lib.mkRustBin { } pkgs.buildPackages;
      toolchain = rust-bin.stable.latest.default;
    in f system pkgs toolchain);
  in {

    apps = let
      mkApp = program: { type = "app"; program = toString program; };
    in forAllSystems (system: pkgs: _: {
      mysqladm-rs = mkApp (lib.getExe self.packages.${system}.mysqladm-rs);
      coverage = mkApp (pkgs.writeScript "mysqladm-rs-coverage" ''
        ${lib.getExe pkgs.python3} -m http.server -d "${self.packages.${system}.coverage}/html/src"
      '');
    });

    devShell = forAllSystems (system: pkgs: toolchain: pkgs.mkShell {
      nativeBuildInputs = with pkgs; [
        toolchain
        mysql-client
        cargo-nextest
      ];

      RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
    });

    overlays = {
      default = self.overlays.mysqladm-rs;
      greg-ng = final: prev: {
        inherit (self.packages.${prev.system}) mysqladm-rs;
      };
    };

    packages = let
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      cargoLock = ./Cargo.lock;
      src = builtins.filterSource (path: type: let
        baseName = baseNameOf (toString path);
      in !(lib.any (b: b) [
          (!(lib.cleanSourceFilter path type))
          (baseName == "target" && type == "directory")
          (baseName == "nix" && type == "directory")
          (baseName == "flake.nix" && type == "regular")
          (baseName == "flake.lock" && type == "regular")
        ])) ./.;
    in forAllSystems (system: pkgs: _: {
      default = self.packages.${system}.mysqladm-rs;
      mysqladm-rs = pkgs.callPackage ./nix/default.nix { inherit cargoToml cargoLock src; };
      coverage = pkgs.callPackage ./nix/coverage.nix { inherit cargoToml cargoLock src; };
      filteredSource = pkgs.runCommandLocal "filtered-source" { } ''
        ln -s ${src} $out
      '';
    });
  };
}
