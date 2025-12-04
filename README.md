# Komari-Monitor-rs

![](https://hitscounter.dev/api/hit?url=https%3A%2F%2Fgithub.com%2Filnli%2Fkomari-monitor-rs&label=&icon=github&color=%23160d27)
![komari-monitor-rs](https://socialify.git.ci/ilnli/komari-monitor-rs/image?custom_description=Komari+%E7%AC%AC%E4%B8%89%E6%96%B9+Agent+%7C+%E9%AB%98%E6%80%A7%E8%83%BD&description=1&font=KoHo&forks=1&issues=1&language=1&name=1&owner=1&pattern=Floating+Cogs&pulls=1&stargazers=1&theme=Auto)

## About

`Komari-Monitor-rs` 是一个适用于 [komari-monitor](https://github.com/komari-monitor) 监控服务的第三方**高性能**监控
Agent

致力于实现[原版 Agent](https://github.com/komari-monitor/komari-agent) 的所有功能，并拓展更多功能

## 一键安装

### Linux

```bash
# 交互式安装
bash <(curl -sL https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/install.sh)

# 带参数安装
bash <(curl -sL https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/install.sh) \
  --http-server "http://your.server:port" --token "your_token"

# 管理模式
bash <(curl -sL https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/install.sh) --manage
```

### Windows (PowerShell 管理员)

```powershell
# 交互式安装
irm https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/install.ps1 -OutFile install.ps1; .\install.ps1

# 带参数安装
.\install.ps1 -HttpServer "http://your.server:port" -Token "your_token"

# 管理模式
.\install.ps1 -Manage
```

## 与原版的差异

测试项目均在 Redmi Book Pro 15 2022 锐龙版 + Arch Linux 最新版 + Rust Toolchain Stable 下测试

### Binary 体积

原版体积约 6.2M，本项目体积约 992K，相差约 7.1 倍

### 运行内存与 Cpu 占用

原版占用内存约 15.4 MiB，本项目占用内存约 5.53 MB，相差约 2.7 倍

原版峰值 Cpu 占用约 49.6%，本项目峰值 Cpu 占用约 4.8%

并且，本项目在堆上的内存仅 388 kB

### 实现功能

目前，本项目已经实现原版的大部分功能，但还有以下的差异:

- GPU Name 检测

## 下载

在本项目的 [Release 界面](https://github.com/ilnli/komari-monitor-rs/releases/tag/latest) 即可下载，按照架构选择即可

后缀有 `musl` 字样的可以在任何 Linux 系统下运行

后缀有 `gnu` 字样的仅可以在较新的，通用的，带有 `Glibc` 的 Linux 系统下运行，占用会小一些

## Usage

本项目使用配置文件进行配置。

- Linux: `/etc/komari-monitor-rs/config`
- Windows: `%ProgramData%\Komari\config`
- 手动运行时默认读取程序同目录下的 `config` 文件

### 命令行参数

```
Komari Monitor Agent in Rust

Usage: komari-monitor-rs [OPTIONS]

Options:
  -c, --config <配置文件路径>  指定配置文件路径 (默认: 程序同目录下的 config 文件)
  -h, --help                   显示帮助信息
  -V, --version                显示版本信息
```

### 配置文件格式

配置文件采用简单的 `key = value` 格式：

```ini
# Komari Monitor RS 配置文件

# 主端地址 (必需)
http_server = "http://your.server:port"
# WebSocket 地址 (可选，默认从 http_server 自动推断)
# ws_server = "ws://your.server:port"
token = "your_token"

# IP 提供商 (ipinfo / cloudflare)
ip_provider = "ipinfo"

# 功能开关
terminal = false
tls = false
ignore_unsafe_cert = false

# 性能设置
fake = 1
realtime_info_interval = 1000
billing_day = 1

# 日志等级 (error / warn / info / debug / trace)
log_level = "info"

# 自动升级 (0 = 禁用，其他数字为检查间隔小时数)
auto_update = 0
update_repo = "ilnli/komari-monitor-rs"
```

**必须设置 `http_server` 和 `token`**

## Nix 安装

如果你使用 Nix / NixOS，可以直接将本仓库作为 Flake 引入使用：

> [!WARNING]
> 以下是最小化示例配置，单独使用无法工作

```nix
{
  # 将 komari-monitor-rs 作为 flake 引入
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    komari-monitor-rs = {
      url = "github:ilnli/komari-monitor-rs";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };
  outputs = { nixpkgs, komari-monitor-rs, ... }: {
    nixosConfigurations."nixos" = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        komari-monitor-rs.nixosModules.default
        { pkgs, ...}: {
          # 开启并配置 komari-monitor-rs 服务
          services.komari-monitor-rs = {
            enable = true;
            settings = {
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
            };
          };
        }
      ];
    };
  };
}
```

## LICENSE

本项目根据 WTFPL 许可证开源

```
        DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE 
                    Version 2, December 2004 

 Copyright (C) 2004 Sam Hocevar <sam@hocevar.net> 

 Everyone is permitted to copy and distribute verbatim or modified 
 copies of this license document, and changing it is allowed as long 
 as the name is changed. 

            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE 
   TERMS AND CONDITIONS FOR COPYING, DISTRIBUTION AND MODIFICATION 

  0. You just DO WHAT THE FUCK YOU WANT TO.
```
