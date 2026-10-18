# Komari-Monitor-rs

![](https://hitscounter.dev/api/hit?url=https%3A%2F%2Fgithub.com%2Frsbench%2Frsbench&label=&icon=github&color=%23160d27)
![komari-monitor-rs](https://socialify.git.ci/GenshinMinecraft/komari-monitor-rs/image?custom_description=Komari+%E7%AC%AC%E4%B8%89%E6%96%B9+Agent+%7C+%E9%AB%98%E6%80%A7%E8%83%BD&description=1&font=KoHo&forks=1&issues=1&language=1&name=1&owner=1&pattern=Floating+Cogs&pulls=1&stargazers=1&theme=Auto)

## About

`Komari-Monitor-rs` 是一个适用于 [komari-monitor](https://github.com/komari-monitor) 监控服务的第三方**高性能**监控
Agent

致力于实现[原版 Agent](https://github.com/komari-monitor/komari-agent) 的所有功能，并拓展更多功能

版本变更记录见 [CHANGELOG.md](CHANGELOG.md)。

## 手动编译与运行

服务端需要支持 v2 JSON-RPC，主通道为 `/api/clients/v2/rpc`。
Agent 首选 WebSocket，连接失败达到 `--max-retries` 后使用 HTTP POST 上报，
并按 `--reconnect-interval` 尝试恢复 WebSocket。HTTP 回退期间的任务接收速度取决于上报间隔，终端不可用。

编译和运行：

```bash
cargo build --release --features ureq-support --locked
./target/release/komari-monitor-rs --endpoint https://komari.example.com --token YOUR_TOKEN --interval 1
```

HTTPS 地址会自动使用 WSS。现有 `--http-server` / `--ws-server` 配置仍然有效。

协议测试：`cargo test --features ureq-support --locked`。集成测试启动本地 HTTP/WebSocket 模拟服务，
验证回退、恢复、任务去重及结果回传；不需要真实服务端令牌。

## Windows 通知

默认情况下，在 Windows 系统下运行时会发送系统 Toast 通知，内容为:

```
Komari-monitor-rs Is Running!
Komari-monitor-rs is an application used to monitor your system, granting it near-complete access to your computer. If you did not actively install this program, please check your system immediately. If you have intentionally used this software on your system, please ignore this message or add `--disable-toast-notify` to your startup parameters.
```

可以通过 `--disable-toast-notify` 参数关闭

## 数据预览

仅提供 `--dry-run` 参数即可预览监控数据，无需配置面板地址和 Token。

每次正常运行前也将获取一次数据，若有误监控的项目请发送 DryRun 的输出到 Issue 中，比如各种不应该读取的硬盘、虚拟网卡等

```
The following is the equipment that will be put into operation and monitored:
CPU: AMD EPYC 7763 64-Core Processor, Cores: 4
Memory: 2092 MB / 16773 MB
Swap: 0 MB / 0 MB
Load: 0.36 / 0.65 / 0.37

Hard drives will be monitored:
/dev/root | ext4 | /usr/sbin/docker-init | 8 GB / 31 GB

Network interfaces will be monitored:
eth0 | 00:22:48:58:ca:62 | UP: 0 GB / DOWN: 7 GB
CONNS: TCP: 12 | UDP: 4
```

## 周期流量累计与重置

流量统计分别上报本周期累计上传和下载字节数，两者之和是本周期总用量。
设置每月 15 号重置，表示 **服务器本地时间每月 15 日 00:00 开始新周期**，
例如 10 月 15 日零点到 11 月 15 日零点前属于同一周期。若设置 31 号而该月不足 31 天，则使用当月最后一天。

手动运行时，应同时指定自然月模式：

```bash
./komari-monitor-rs --endpoint https://komari.example.com --token YOUR_TOKEN \
  --network-statistics-mode natural --traffic-period month --traffic-reset-day 15
```

仅传 `--traffic-reset-day` 不会改变默认的 `fixed` 模式。管理脚本会自动配置自然月模式，
升级时会修正已有配置中遗漏的模式参数，显式配置的 `fixed` 模式保持不变。

首次启用从零开始，无法回溯安装前的本周期流量；更改周期规则会开始新周期。
Agent 重启保留同一周期的累计值，系统重启从最近保存的数据继续累计。
默认每分钟保存一次，突然断电可能丢失尚未保存的部分；程序停机跨过重置日的流量无法精确补算。
启动日志会输出下次重置时间，保存失败也会明确记录。

相关参数:

- `--disable-network-statistics`: 禁用周期流量统计，上报的总流量回退到原来自网卡启动以来的总流量，默认关闭
- `--network-statistics-mode`: 流量统计周期的模式，可选 fixed / natural, 默认 fixed.   
               fixed: 固定一段时间，时间到后就重置流量    
               natural: 以自然日期为周期，例如每月1号重置流量    
- `--network-save-path`: 周期流量统计 的文件保存地址，在 Windows 下默认为 `C:\komari-network.conf`，非 Windows 默认为 `/etc/komari-network.conf` (root) 或 `$HOME/.config/komari-network.conf` (非 root)
- `--network-interval`: 周期流量统计存内存的间隔长度，单位 sec，默认 10
- `--network-interval-number`: 周期流量统计 的保存到磁盘间隔次数，默认 6, 即一分钟落盘一次。  
- `--network-duration`: 仅仅用于 fixed 模式, 周期流量统计 的统计长度，单位 sec，默认 864000 (10 Days).  
- `--traffic-period`: 仅仅用于 natural 模式，流量统计清零的周期长度，可选: week/month/year，默认 month (以自然月为周期清零总流量)   
- `--traffic-reset-day`: 仅仅用于 natural 模式，流量统计重置的日期, 默认在每月1号清零。  
              清零周期是一周时，可选 1...7, 对应在周1..7重置。  
              清零周期是一月时，可选 1...31, 对应在下月的1...31日重置。  
              清零周期是一年时，格式为12/31，对应在下年的12/31日重置。  

## 管理脚本

提供 Linux 管理脚本，支持安装、升级、配置、服务管理等功能。

### 下载脚本

```bash
# 下载管理脚本
wget -O komari-manager.sh https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/komari-manager.sh
chmod +x komari-manager.sh
```

### 常用命令

```bash
# 安装 (交互式配置)
sudo ./komari-manager.sh install

# 使用代理安装 (中国大陆用户)
sudo ./komari-manager.sh install --proxy

# 升级前请先重新下载上面的管理脚本；升级会保留配置
sudo ./komari-manager.sh upgrade

# 修改配置
sudo ./komari-manager.sh config

# 查看状态
sudo ./komari-manager.sh status

# 启动/停止/重启服务
sudo ./komari-manager.sh start
sudo ./komari-manager.sh stop
sudo ./komari-manager.sh restart

# 查看日志
sudo ./komari-manager.sh logs
sudo ./komari-manager.sh logs -f    # 实时跟踪

# 卸载
sudo ./komari-manager.sh uninstall
```

### 通过脚本自动发现

执行 `sudo ./komari-manager.sh install`，在交互配置的“接入方式”中选择 **2. 自动发现**，
填写 Komari 面板提供的自动发现密钥即可，无需提前创建节点或手工获取 Token。
面板须已配置自动发现密钥；协议说明见[官方自动发现文档](https://komari-document.pages.dev/install/agent-ad)。

脚本使用本机主机名向面板注册，取得节点 Token 后写入现有的 systemd 服务配置，
Agent 继续使用 `--token` 上报。自动发现密钥输入时不回显，也不会写入服务配置。
自动发现使用 `curl` 或 `wget`，并按需安装用于解析注册响应的 `jq`。
注册请求沿用配置中是否忽略证书验证的选择；注册失败不会覆盖原服务配置。

重启和升级会复用已保存的 Token，不会重复注册。
已有安装可执行 `sudo ./komari-manager.sh config`，选择自动发现注册一个新节点；
只修改其他设置时，保留默认的 **1. 手动 Token** 并在 Token 提示处回车，即可复用当前节点。
删除服务配置后重新自动发现会创建新节点。

## 与原版的差异

当前支持基础监控、Ping、远程命令和终端；以下官方功能尚未实现：

- GPU 信息检测
- 文件管理、远程启动参数管理和版本切换
- gzip 传输压缩（目前始终发送未压缩 JSON，服务端支持；`--disable-compression` 为兼容参数）
- `--memory-mode-available` 内存统计模式（仅接受参数，不改变统计方式）

除此之外，还有希望添加的功能:

- 自动更新
- ~~自动安装~~
- ~~Bash / PWSH 一键脚本~~

## 下载

在本仓库的 [Release 界面](https://github.com/ilnli/komari-monitor-rs/releases/tag/latest) 即可下载，按照架构选择即可。
当前自动构建 Linux x86_64/aarch64 GNU、Windows x86_64 和 macOS arm64；管理脚本仅支持对应的 Linux 架构，
下载后会检查 `SHA256SUMS` 和程序可运行性，验证通过才停止旧服务。

自动 CI 仅在推送或 PR 包含 `.rs` 文件变更时运行，纯脚本或文档修改不会触发。
包含 `.rs` 变更的 `main` 推送会经过两种 HTTP 后端测试与管理脚本测试、四个平台构建，
随后统一更新滚动 `latest` Release；其他分支和 PR 只测试、构建。
也可通过 Actions 的手动运行入口触发构建，选择 `main` 时会发布。Nix 包版本自动读取 `Cargo.toml`。

后缀有 `musl` 字样的可以在任何 Linux 系统下运行

后缀有 `gnu` 字样的仅可以在较新的，通用的，带有 `Glibc` 的 Linux 系统下运行，占用会小一些

## Usage

```
komari-monitor-rs is a third-party high-performance monitoring agent for the komari monitoring service.

Usage: komari-monitor-rs [OPTIONS]

Options:
      --http-server <HTTP_SERVER>
          Set Main Server endpoint address (alias: --endpoint)

      --ws-server <WS_SERVER>
          Set Main Server WebSocket Address

  -t, --token <TOKEN>
          Set Token

  -f, --fake <FAKE>
          Set Fake Multiplier
          [default: 1]

      --tls
          Legacy TLS flag (HTTPS/WSS enable TLS automatically)
          [default: false]

      --ignore-unsafe-cert
          Ignore Certificate Verification
          [default: false]

  -d, --dry-run
          Dry Run
          [default: false]

      --log-level <LOG_LEVEL>
          Set Log Level (Enable Debug or Trace for issue reporting)
          [default: info]

      --ip-provider <IP_PROVIDER>
          Public IP Provider
          [default: ipinfo]

      --terminal
          Enable Terminal (default disabled)
          [default: false]

      --terminal-entry <TERMINAL_ENTRY>
          Custom Terminal Entry
          [default: default]

      --realtime-info-interval <REALTIME_INFO_INTERVAL>
          Set Real-Time Info Upload Interval (ms) (legacy option)
          [default: 1000]

      --interval <INTERVAL>
          Set Real-Time Info Upload Interval (seconds)

      --info-report-interval <INFO_REPORT_INTERVAL>
          Set Basic Info Upload Interval (minutes)
          [default: 5]

      --disable-toast-notify
          Disable Windows Toast Notification (Only Windows)
          [default: false]

      --disable-network-statistics
          Disable Network Statistics
          [default: false]

      --network-statistics-mode <NETWORK_STATISTICS_MODE>
          Network statistics calculation mode.
          'fixed' is based on a fixed duration, such as 10 days
          'natural' is based on natural datetime
          [default: fixed]

      --network-save-path <NETWORK_SAVE_PATH>
          Network Statistics Save Path

      --network-interval <NETWORK_INTERVAL>
          Network Statistics Save Interval (s)
          [default: 10]

      --network-duration <NETWORK_DURATION>
          For 'fixed' mode only
          Duration for one cycle of network statistics in seconds.
          [default: 864000]

      --network-interval-number <NETWORK_INTERVAL_NUMBER>
          Number of intervals to save network statistics to disk.
          [default: 6]

      --traffic-period <TRAFFIC_PERIOD>
          Network statistics reset period, for 'natural' mode only.
          [default: month]

      --traffic-reset-day <TRAFFIC_RESET_DAY>
          Network statistics reset day, for 'natural' mode only.
            For 'week', accepts 1-7 (Mon-Sun) or names like 'mon', 'tue'.
            For 'month', accepts a day number like 1-31.
            For 'year', accepts a date in 'MM/DD' format, e.g., '12/31'.
          [default: 1]
```

必须设置 `--endpoint`（或兼容的 `--http-server`）/ `--token`。Agent 使用 v2 JSON-RPC，主通道为 `/api/clients/v2/rpc`。
`--ip-provider` 接受 `cloudflare` / `ipinfo`
`--log-level` 接受 `error`, `warn`, `info`, `debug`, `trace`

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
