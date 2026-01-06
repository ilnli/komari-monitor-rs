# Komari-Monitor-rs

![](https://hitscounter.dev/api/hit?url=https%3A%2F%2Fgithub.com%2Frsbench%2Frsbench&label=&icon=github&color=%23160d27)
![komari-monitor-rs](https://socialify.git.ci/GenshinMinecraft/komari-monitor-rs/image?custom_description=Komari+%E7%AC%AC%E4%B8%89%E6%96%B9+Agent+%7C+%E9%AB%98%E6%80%A7%E8%83%BD&description=1&font=KoHo&forks=1&issues=1&language=1&name=1&owner=1&pattern=Floating+Cogs&pulls=1&stargazers=1&theme=Auto)

## About

`Komari-Monitor-rs` 是一个适用于 [komari-monitor](https://github.com/komari-monitor) 监控服务的第三方**高性能**监控
Agent

致力于实现[原版 Agent](https://github.com/komari-monitor/komari-agent) 的所有功能，并拓展更多功能

## 近期更新

### Windows Toast Notify

由于安全性问题，现在默认情况下在 Windows 系统下运行时会发送 Windows 系统 Toast 通知，内容为:

```
Komari-monitor-rs Is Running!
Komari-monitor-rs is an application used to monitor your system, granting it near-complete access to your computer. If you did not actively install this program, please check your system immediately. If you have intentionally used this software on your system, please ignore this message or add `--disable-toast-notify` to your startup parameters.
```

可以通过 `--disable-toast-notify` 参数关闭

### Dry Run 支持

现在可以不提供任何参数，仅提供 `--dry-run` 参数，以事先获取监控数据

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

### 已支持周期流量统计 / 清零

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

该功能暂未稳定，有问题请及时反馈

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

# 升级到最新版本 (保留配置)
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

## 与原版的差异

目前，本项目已经实现原版的大部分功能，但还有以下的差异:

- GPU Name 检测

除此之外，还有希望添加的功能:

- 自动更新
- ~~自动安装~~
- ~~Bash / PWSH 一键脚本~~

## 下载

在本项目的 [Release 界面](https://github.com/GenshinMinecraft/komari-monitor-rs/releases/tag/latest) 即可下载，按照架构选择即可

后缀有 `musl` 字样的可以在任何 Linux 系统下运行

后缀有 `gnu` 字样的仅可以在较新的，通用的，带有 `Glibc` 的 Linux 系统下运行，占用会小一些

## Usage

```
komari-monitor-rs is a third-party high-performance monitoring agent for the komari monitoring service.

Usage: komari-monitor-rs [OPTIONS]

Options:
      --http-server <HTTP_SERVER>
          Set Main Server Http Address

      --ws-server <WS_SERVER>
          Set Main Server WebSocket Address

  -t, --token <TOKEN>
          Set Token

  -f, --fake <FAKE>
          Set Fake Multiplier
          [default: 1]

      --tls
          Enable TLS (default disabled)
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
          Set Real-Time Info Upload Interval (ms)
          [default: 1000]

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

必须设置 `--http-server` / `--token`
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
      url = "github:GenshinMinecraft/komari-monitor-rs";
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
