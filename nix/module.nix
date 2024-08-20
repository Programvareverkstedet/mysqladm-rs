{ config, pkgs, lib, ... }:
let
  cfg = config.services.mysqladm-rs;
  format = pkgs.formats.toml { };
in
{
  options.services.mysqladm-rs = {
    enable = lib.mkEnableOption "Enable mysqladm-rs";

    package = lib.mkPackageOption pkgs "mysqladm-rs" { };

    createLocalDatabaseUser = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Create a local database user for mysqladm-rs";
    };

    logLevel = lib.mkOption {
      type = lib.types.enum [ "quiet" "error" "warn" "info" "debug" "trace" ];
      default = "debug";
      description = "Log level for mysqladm-rs";
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
              default = "/var/run/mysqladm/mysqladm.sock";
              description = "Path to the MySQL socket";
            };
          };

          mysql = {
            socket_path = lib.mkOption {
              type = with lib.types; nullOr path;
              default = "/var/run/mysqld/mysqld.sock";
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
              default = "mysqladm";
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

  config = let
    nullStrippedConfig = lib.filterAttrsRecursive (_: v: v != null) cfg.settings;
    configFile = format.generate "mysqladm-rs.conf" nullStrippedConfig;
  in lib.mkIf config.services.mysqladm-rs.enable {
    environment.systemPackages = [ cfg.package ];

    services.mysql.ensureUsers = lib.mkIf cfg.createLocalDatabaseUser [
      {
        name = cfg.settings.mysql.username;
        ensurePermissions = {
          "mysql.*" = "SELECT, INSERT, UPDATE, DELETE";
          "*.*" = "GRANT OPTION, CREATE, DROP";
        };
      }
    ];

    systemd.services."mysqladm@" = {
      description = "MySQL administration tool for non-admin users";
      environment.RUST_LOG = "debug";
      serviceConfig = {
        Type = "notify";
        ExecStart = "${lib.getExe cfg.package} ${cfg.logLevel} server --systemd socket-activate --config ${configFile}";

        WatchdogSec = 15;

        User = "mysqladm";
        Group = "mysqladm";
        DynamicUser = true;

        # This is required to read unix user/group details.
        PrivateUsers = false;

        # Needed to communicate with MySQL.
        PrivateNetwork = false;

        IPAddressDeny =
          lib.optionals (lib.elem cfg.settings.mysql.host [ null "localhost" "127.0.0.1" ]) [ "any" ];

        RestrictAddressFamilies = [ "AF_UNIX" ]
          ++ (lib.optionals (cfg.settings.mysql.host != null) [ "AF_INET" "AF_INET6" ]);

        AmbientCapabilities = [ "" ];
        CapabilityBoundingSet = [ "" ];
        DeviceAllow = [ "" ];
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateDevices = true;
        PrivateMounts = true;
        PrivateTmp = "yes";
        ProcSubset = "pid";
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        RemoveIPC = true;
        UMask = "0777";
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SocketBindDeny = [ "any" ];
        SystemCallFilter = [
          "@system-service"
          "~@privileged"
          "~@resources"
        ];
      };
    };

    systemd.sockets."mysqladm" = {
      description = "MySQL administration tool for non-admin users";
      wantedBy = [ "sockets.target" ];
      restartTriggers = [ configFile ];
      socketConfig = {
        ListenStream = cfg.settings.server.socket_path;
        Accept = "yes";
        PassCredentials = true;
      };
    };
  };
}