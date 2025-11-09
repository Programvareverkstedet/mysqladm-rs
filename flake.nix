{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay }:
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
      mysqladm-rs = mkApp (lib.getExe self.packages.${system}.mysqladm-rs);
      coverage = mkApp (pkgs.writeScript "mysqladm-rs-coverage" ''
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
      ];

      RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
    });

    overlays = {
      default = self.overlays.mysqladm-rs;
      mysqladm-rs = final: prev: {
        inherit (self.packages.${prev.stdenv.hostPlatform.system}) mysqladm-rs;
      };
    };

    nixosModules = {
      default = self.nixosModules.mysqladm-rs;
      mysqladm-rs = import ./nix/module.nix;
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

    nixosConfigurations.vm = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        system = "x86_64-linux";
        overlays = [
          self.overlays.default
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
            Welcome to the mysqladm-rs vm!

            Try running:
                ${config.services.mysqladm-rs.package.meta.mainProgram}

            Password for alice is 'foobar'
            Password for root is 'root'

            To exit, press Ctrl+A, then X
            =================================
          '';

          services.mysql = {
            enable = true;
            package = pkgs.mariadb;
          };
          services.mysqladm-rs = {
            enable = true;
            createLocalDatabaseUser = true;
          };

          systemd.services."mysqladm@".environment.RUST_LOG = "debug";
        })
      ];
    };
  };
}
