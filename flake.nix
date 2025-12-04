{
  description = "Komari Monitor Agent in Rust";
  inputs = {
    utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "nixpkgs/nixos-unstable";
  };
  outputs = { self, nixpkgs, utils, ... }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        toolchain = pkgs.rustPlatform;
      in rec {
        packages = let
          p = {
            pname = "komari-monitor-rs";
            version = "0.2.7";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildType = "minimal";
            # For other makeRustPlatform features see:
            # https://github.com/NixOS/nixpkgs/blob/master/doc/languages-frameworks/rust.section.md#cargo-features-cargo-features
          };
        in {
          default = packages.ureq;
          ureq = toolchain.buildRustPackage
            (p // { buildFeatures = [ "ureq-support" ]; });
          nyquest-support = toolchain.buildRustPackage
            (p // { buildFeatures = [ "nyquest-support" ]; });
        };

        # Executed by `nix run`
        apps.default = utils.lib.mkApp { drv = packages.default; };

        # Used by `nix develop`
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            (with toolchain; [ cargo rustc rustLibSrc ])
            clippy
            rustfmt
            pkg-config
          ];

          # Specify the rust-src path (many editors rely on this)
          RUST_SRC_PATH = "${toolchain.rustLibSrc}";
        };
      }) // { # Used by NixOS
        nixosModules = {
          default = self.nixosModules.komari-monitor-rs;
          komari-monitor-rs = { config, lib, pkgs, ... }:
            let
              cfg = config.services.komari-monitor-rs;
              inherit (lib)
                mkEnableOption mkOption types literalExpression mkIf;
              # 将设置转换为配置文件内容
              settingsToConfig = settings: let
                formatValue = v:
                  if builtins.isBool v then
                    (if v then "true" else "false")
                  else if builtins.isInt v || builtins.isFloat v then
                    builtins.toString v
                  else
                    ''"${builtins.toString v}"'';
                formatLine = k: v: 
                  let
                    # 将 kebab-case 转换为 snake_case
                    snakeKey = builtins.replaceStrings ["-"] ["_"] k;
                  in "${snakeKey} = ${formatValue v}";
              in builtins.concatStringsSep "\n" (lib.mapAttrsToList formatLine settings);
              configFile = pkgs.writeText "komari-monitor-rs-config" (settingsToConfig cfg.settings);
            in {
              options.services.komari-monitor-rs = {
                enable = mkEnableOption "Komari Monitor Agent in Rust";
                package = mkOption {
                  type = types.package;
                  default = self.packages.${pkgs.system}.default;
                  defaultText =
                    literalExpression "self.packages.${pkgs.system}.default";
                  description = "komari-monitor-rs package to use.";
                };
                settings = mkOption {
                  type = types.nullOr (types.attrsOf types.unspecified);
                  default = null;
                  description = ''
                    configuration for komari-monitor-rs, `http_server` and `token` must be specified.
                    key is the config key name (use underscores or hyphens, both work).
                    value is the value of the parameter.
                    see <https://github.com/ilnli/komari-monitor-rs#usage> for supported options.
                  '';
                  example = literalExpression ''
                    {
                      http_server = "https://komari.example.com:12345";
                      ws_server = "ws://ws-komari.example.com:54321";
                      token = "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";
                      ip_provider = "ipinfo";
                      terminal = true;
                      terminal_entry = "default";
                      fake = 1;
                      realtime_info_interval = 1000;
                      tls = true;
                      ignore_unsafe_cert = false;
                      log_level = "info";
                      billing_day = 1;
                      auto_update = 0;
                    }
                  '';
                };
              };
              config = mkIf cfg.enable {
                assertions = [{
                  assertion = (cfg.settings != null)
                    && (cfg.settings.http_server or cfg.settings.http-server or null) != null
                    && (cfg.settings.token or null) != null;
                  message =
                    "Both `settings.http_server` and `settings.token` should be specified for komari-monitor-rs.";
                }];
                systemd.services.komari-monitor-rs = {
                  description = "Komari Monitor RS Service";
                  after = [ "network.target" ];
                  wantedBy = [ "multi-user.target" ];
                  serviceConfig = {
                    Type = "simple";
                    User = "root";
                    ExecStart = "${cfg.package}/bin/komari-monitor-rs --config ${configFile}";
                    Restart = "always";
                    RestartSec = 5;
                    StandardOutput = "journal";
                    StandardError = "journal";
                  };
                };
              };
            };
        };
      };
}
