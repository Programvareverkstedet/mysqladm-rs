{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";

    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, rust-overlay, crane }:
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
      toolchain = rust-bin.stable.latest.default.override {
        extensions = [ "rust-src" ];
      };
    in f system pkgs toolchain);
  in {
    apps = let
      mkApp = program: { type = "app"; program = toString program; };
    in forAllSystems (system: pkgs: _: {
      muscl = mkApp (lib.getExe self.packages.${system}.muscl);
      coverage = mkApp (pkgs.writeScript "muscl-coverage" ''
        ${lib.getExe pkgs.python3} -m http.server -d "${self.packages.${system}.coverage}/html/src"
      '');
      vm = mkApp "${self.nixosConfigurations.vm.config.system.build.vm}/bin/run-nixos-vm";
    });

    devShell = forAllSystems (system: pkgs: toolchain: pkgs.mkShell {
      nativeBuildInputs = with pkgs; [
        toolchain
        mariadb.client
        cargo-nextest
        cargo-edit
        cargo-deny
        cargo-deb
        dpkg
      ];

      RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
    });

    overlays = {
      default = self.overlays.muscl;
      muscl = final: prev: {
        inherit (self.packages.${prev.stdenv.hostPlatform.system}) muscl;
      };
      muscl-crane = final: prev: {
        muscl = self.packages.${prev.stdenv.hostPlatform.system}.muscl-crane;
      };
    };

    nixosModules = {
      default = self.nixosModules.muscl;
      muscl = import ./nix/module.nix;
    };

    packages = forAllSystems (system: pkgs: _:
      let
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      cargoLock = ./Cargo.lock;
      craneLib = crane.mkLib pkgs;
      src = lib.fileset.toSource {
        root = ./.;
        fileset = lib.fileset.unions [
          (craneLib.fileset.commonCargoSources ./.)
        ];
      };
    in {
      default = self.packages.${system}.muscl-crane;
      muscl = pkgs.callPackage ./nix/default.nix { inherit cargoToml cargoLock src; };
      muscl-crane = pkgs.callPackage ./nix/default.nix {
        useCrane = true;
        inherit cargoToml cargoLock src craneLib;
      };
      coverage = pkgs.callPackage ./nix/coverage.nix { inherit cargoToml cargoLock src; };
      filteredSource = pkgs.runCommandLocal "filtered-source" { } ''
        ln -s ${src} $out
      '';
    });

    nixosConfigurations.vm = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        system = "x86_64-linux";
        overlays = [
          self.overlays.muscl-crane
        ];
      };
      modules = [
        "${nixpkgs}/nixos/modules/virtualisation/qemu-vm.nix"
        "${nixpkgs}/nixos/tests/common/user-account.nix"

        self.nixosModules.default

        ({ config, pkgs, ... }: {
          system.stateVersion = config.system.nixos.release;
          virtualisation.graphics = false;

          users = {
            groups = {
              a = { };
              b = { };
            };
            users.alice.extraGroups = [
              "a"
              "b"
              "wheel"
              "systemd-journal"
            ];
            extraUsers.root.password = "root";
          };

          services.getty.autologinUser = "alice";

          users.motd = ''
            =================================
            Welcome to the muscl vm!

            Try running:
                ${config.services.muscl.package.meta.mainProgram}

            Password for alice is 'foobar'
            Password for root is 'root'

            To exit, press Ctrl+A, then X
            =================================
          '';

          services.mysql = {
            enable = true;
            package = pkgs.mariadb;
          };
          services.muscl = {
            enable = true;
            logLevel = "trace";
            createLocalDatabaseUser = true;
          };

          programs.vim = {
            enable = true;
            defaultEditor = true;
          };
        })
      ];
    };
  };
}
