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
      type = lib.types.enum [ "quiet" "info" "debug" "trace" ];
      default = "info";
      description = "Log level for muscl";
      apply = level: {
        "quiet" = "-q";
        "info" = "";
        "debug" = "-v";
        "trace" = "-vv";
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

          authorization = {
             group_denylist = lib.mkOption {
               type = with lib.types; nullOr (listOf str);
               default = [ "wheel" ];
               description = "List of groups that are denied access";
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

    environment.etc."muscl/config.toml".source = lib.pipe cfg.settings [
      # Handle group_denylist_file
      (conf: lib.recursiveUpdate conf {
         authorization.group_denylist_file = if (conf.authorization.group_denylist != [ ]) then "/etc/muscl/group-denylist" else null;
         authorization.group_denylist = null;
      })

      # Remove nulls
      (lib.filterAttrsRecursive (_: v: v != null))

      # Load mysql.passwordFile via LoadCredentials
      (conf:
        if conf.mysql.passwordFile or null != null
          then lib.recursiveUpdate conf { mysql.passwordFile = "/run/credentials/muscl.service/mysql-password"; }
          else conf
      )

      # Render file
      (format.generate "muscl.conf")
    ];

    environment.etc."muscl/group-denylist" = lib.mkIf (cfg.settings.authorization.group_denylist != [ ]) {
      text = lib.concatMapStringsSep "\n" (group: "group:${group}") cfg.settings.authorization.group_denylist;
    };

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
      reloadTriggers = [ config.environment.etc."muscl/config.toml".source ];
      serviceConfig = {
        ExecStart = [
          ""
          "${lib.getExe cfg.package} ${cfg.logLevel} server --systemd --disable-landlock socket-activate"
        ];

        ExecReload = [
           ""
           "${lib.getExe' pkgs.coreutils "kill"} -HUP $MAINPID"
        ];

        RuntimeDirectory = "muscl/root-mnt";
        RuntimeDirectoryMode = "0700";
        RootDirectory = "/run/muscl/root-mnt";
        BindReadOnlyPaths = [
          builtins.storeDir
          "/etc"
        ]
        ++ lib.optionals (cfg.settings.mysql.socket_path != null) [
          cfg.settings.mysql.socket_path
        ];

        ImportCredential = "";
        LoadCredential = lib.mkIf (cfg.settings.mysql.passwordFile != null) [
          "mysql-password:${cfg.settings.mysql.passwordFile}"
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
