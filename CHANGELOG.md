# 变更记录

## 未发布

- 管理脚本的安装和配置流程支持自动发现：使用面板密钥注册节点，保存返回的 Token，重启和升级时复用。
- 自动 CI 仅在推送或 PR 包含 `.rs` 文件变更时运行；纯脚本和文档变更不触发构建或发布，保留手动运行入口。

## 0.4.0

### 协议与传输

依据[官方 Agent 文档](https://komari-monitor.github.io/komari-document/dev/agent.html)及
[Komari 1.5.1 协议定义](https://github.com/komari-monitor/komari/blob/1.5.1/protocol/v2/jsonrpc.go)，
基础信息、实时监控、Ping 和命令结果统一使用 `/api/clients/v2/rpc` 的 JSON-RPC 2.0 协议，不再使用 v1 上报接口。

- 首选 WebSocket，连接失败达到 `--max-retries` 后使用 HTTP POST 上报；后台按 `--reconnect-interval` 尝试恢复 WebSocket。
- HTTP 回退通过上报响应中的 `result.events` 接收任务，并在后续上报中确认；没有额外的 `agent.pull` 长轮询，任务接收速度取决于上报间隔。独立终端 WebSocket 在 HTTP 回退期间不可用。
- 命令按任务 ID 去重并缓存最近 256 个结果，避免上传重试或连接切换重复执行。缓存位于内存，重启后不保留。
- `--endpoint` 是 `--http-server` 的别名；`--interval` 使用秒（支持小数），优先于原有的毫秒参数 `--realtime-info-interval`。
- 基础信息默认每 5 分钟更新，可用 `--info-report-interval` 调整；远程命令和终端需显式启用 `--terminal`，`--disable-web-ssh` 可强制关闭。

### 周期流量统计

- 修复首次计数、跨周期重启及采样配置变更影响周期的问题。
- 自然周期按服务器本地时间重置；重置日超出当月天数时使用当月最后一天。
- 管理脚本升级时修正旧配置中遗漏的自然月模式参数，保留显式配置的 `fixed` 模式。

### 构建与安装

- 发布前测试两种 HTTP 后端及管理脚本，四个平台构建通过后统一更新滚动 `latest` Release。
- Linux 管理脚本在停止旧服务前校验下载文件的 `SHA256SUMS` 和程序可运行性。
- Nix 包版本自动读取 `Cargo.toml`。
