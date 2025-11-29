{ config, pkgs, lib, ... }:
let
  cfg = config.services.muscl;
  format = pkgs.formats.toml { };
in
{
  options.services.muscl = {
    enable = lib.mkEnableOption "Enable muscl";

    package = lib.mkPackageOption pkgs "muscl" { };

    createLocalDatabaseUser = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Create a local database user for muscl";
    };

    logLevel = lib.mkOption {
      type = lib.types.enum [ "quiet" "error" "warn" "info" "debug" "trace" ];
      default = "info";
      description = "Log level for muscl";
      apply = level: {
        "quiet" = "-q";
        "error" = "";
        "warn" = "-v";
        "info" = "-vv";
        "debug" = "-vvv";
        "trace" = "-vvvv";
      }.${level};
    };

    settings = lib.mkOption {
      default = { };
      type = lib.types.submodule {
        freeformType = format.type;
        options  = {
          server = {
            socket_path = lib.mkOption {
              type = lib.types.path;
              default = "/run/muscl/muscl.sock";
              description = "Path to the muscl socket";
            };
          };

          mysql = {
            socket_path = lib.mkOption {
              type = with lib.types; nullOr path;
              default = "/run/mysqld/mysqld.sock";
              description = "Path to the MySQL socket";
            };
            host = lib.mkOption {
              type = with lib.types; nullOr str;
              default = null;
              description = "MySQL host";
            };
            port = lib.mkOption {
              type = with lib.types; nullOr port;
              default = 3306;
              description = "MySQL port";
            };
            username = lib.mkOption {
              type = lib.types.str;
              default = "muscl";
              description = "MySQL username";
            };
            passwordFile = lib.mkOption {
              type = with lib.types; nullOr path;
              default = null;
              description = "Path to a file containing the MySQL password";
            };
            timeout = lib.mkOption {
              type = lib.types.ints.positive;
              default = 2;
              description = "Number of seconds to wait for a response from the MySQL server";
            };
          };
        };
      };
    };
  };

  config = lib.mkIf config.services.muscl.enable {
    environment.systemPackages = [ cfg.package ];

    environment.etc."muscl/config.toml".source = let
      nullStrippedConfig = lib.filterAttrsRecursive (_: v: v != null) cfg.settings;
    in format.generate "muscl.conf" nullStrippedConfig;

    services.mysql.ensureUsers = lib.mkIf cfg.createLocalDatabaseUser [
      {
        name = cfg.settings.mysql.username;
        ensurePermissions = {
          "mysql.*" = "SELECT, INSERT, UPDATE, DELETE";
          "*.*" = "GRANT OPTION, CREATE, DROP";
        };
      }
    ];

    systemd.packages = [ cfg.package ];

    systemd.sockets."muscl".wantedBy = [ "sockets.target" ];

    systemd.services."muscl" = {
      restartTriggers = [ config.environment.etc."muscl/config.toml".source ];
      serviceConfig = {
        ExecStart = [
          ""
          "${lib.getExe cfg.package} ${cfg.logLevel} server --systemd socket-activate"
        ];

        IPAddressDeny = "any";
        IPAddressAllow = [
          "127.0.0.0/8"
        ] ++ lib.optionals (cfg.settings.mysql.host != null) [
          cfg.settings.mysql.host
        ];

        RestrictAddressFamilies = [ "AF_UNIX" ]
          ++ (lib.optionals (cfg.settings.mysql.host != null) [ "AF_INET" "AF_INET6" ]);
      };
    };
  };
}
