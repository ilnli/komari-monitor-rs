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
                    configuration for komari-monitor-rs, `http-server` and `token` must be specified,
                    key is the long name of the available parameters for komari-monitor-rs, except for `--help` and `--version`, and does not have a `--` prefix.
                    value is the value of the parameter, if the parameter is a flag (eg. `--tls`), then it's value is a boolean value.
                    see <https://github.com/ilnli/komari-monitor-rs#usage> for supported options.
                  '';
                  example = literalExpression ''
                    {
                      http-server = "https://komari.example.com:12345";
                      ws-server = "ws://ws-komari.example.com:54321";
                      token = "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";
                      ip-provider = "ipinfo";
                      terminal = true;
                      terminal-entry = "default";
                      fake = 1;
                      realtime-info-interval = 1000;
                      tls = true;
                      ignore-unsafe-cert = false;
                      log-level = "info";
                    }
                  '';
                };
              };
              config = mkIf cfg.enable {
                assertions = [{
                  assertion = (cfg.settings != null)
                    && (cfg.settings.http-server != null)
                    && (cfg.settings.token != null);
                  message =
                    "Both `settings.http-server` and `settings.token` should be specified for komari-monitor-rs.";
                }];
                systemd.services.komari-monitor-rs = {
                  description = "Komari Monitor RS Service";
                  after = [ "network.target" ];
                  wantedBy = [ "multi-user.target" ];
                  serviceConfig = {
                    Type = "simple";
                    User = "root";
                    ExecStart = "${cfg.package}/bin/komari-monitor-rs "
                      + builtins.concatStringsSep " " (builtins.attrValues
                        (builtins.mapAttrs (k: v:
                          if v == true then
                            "--${k}"
                          else if v == false then
                            ""
                          else
                            ''--${k} "${builtins.toString v}"'') cfg.settings));
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
