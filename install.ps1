Param(
  [string]$Repo = "ilnli/komari-monitor-rs",
  [string]$HttpServer,
  [string]$WsServer,
  [string]$Token,
  [string]$AutoDiscovery,
  [int]$Fake = 1,
  [int]$RealtimeInfoInterval = 1000,
  [int]$BillingDay = 1,
  [int]$AutoUpdate = 0,
  [switch]$Tls,
  [switch]$IgnoreUnsafeCert,
  [switch]$Terminal,
  [string]$Proxy,
  [switch]$Upgrade,
  [switch]$Uninstall,
  [switch]$Manage
)

$ErrorActionPreference = 'Stop'

function Write-Info($msg) { Write-Host "[信息] $msg" -ForegroundColor Green }
function Write-Warn($msg) { Write-Host "[警告] $msg" -ForegroundColor Yellow }
function Write-Err($msg) { Write-Host "[错误] $msg" -ForegroundColor Red }
function Write-Step($msg) { Write-Host "[步骤] $msg" -ForegroundColor Cyan }

# --- 配置 ---
$InstallDir = Join-Path $Env:ProgramFiles 'komari-monitor-rs'
$InstallPath = Join-Path $InstallDir 'komari-monitor-rs.exe'
$ConfigDir = Join-Path $Env:ProgramData 'komari-monitor-rs'
$ConfigPath = Join-Path $ConfigDir 'config'
$ServiceName = 'KomariAgentRs'
$DataDir = Join-Path $ConfigDir 'data'

function Ensure-Admin {
  $id = [Security.Principal.WindowsIdentity]::GetCurrent()
  $p = New-Object Security.Principal.WindowsPrincipal($id)
  if (-not $p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Err '需要以管理员权限运行。请在提升权限的 PowerShell 中执行。'
    exit 1
  }
}

function Get-ArchFile {
  $arch = $Env:PROCESSOR_ARCHITECTURE
  switch ($arch.ToLower()) {
    'amd64' { return 'komari-monitor-rs-windows-x86_64.exe' }
    'arm64' { return 'komari-monitor-rs-windows-aarch64.exe' }
    default { Write-Err "不支持的架构: $arch"; exit 1 }
  }
}

function Download-File($Url, $OutFile) {
  if ($Proxy) {
    $client = New-Object System.Net.WebClient
    $client.Proxy = New-Object System.Net.WebProxy($Proxy, $true)
    $client.DownloadFile($Url, $OutFile)
  } else {
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $OutFile
  }
}

function Auto-Discover($Endpoint, $Key) {
  $hostname = $Env:COMPUTERNAME
  $api = "$Endpoint/api/clients/register?name=$hostname"
  Write-Info "自动发现: $api"
  $headers = @{ 'Content-Type' = 'application/json'; 'Authorization' = "Bearer $Key" }
  $body = @{ key = $Key } | ConvertTo-Json
  $resp = Invoke-RestMethod -Method Post -Uri $api -Headers $headers -Body $body -ErrorAction Stop
  if ($resp.status -ne 'success') { Write-Err "自动发现失败: $($resp | ConvertTo-Json -Compress)"; exit 1 }
  if (-not $resp.data.token) { Write-Err '响应中缺少 token'; exit 1 }
  return $resp.data.token
}

function Save-Config($cfg) {
  New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null
  $lines = @()
  $lines += '# Komari Monitor RS 配置文件'
  $lines += '# 由安装/管理脚本生成'
  $lines += "http_server = \"$($cfg.http_server)\""
  if ($cfg.ws_server) { $lines += "ws_server = \"$($cfg.ws_server)\"" }
  $lines += "token = \"$($cfg.token)\""
  $lines += "ip_provider = \"$($cfg.ip_provider)\""
  $lines += "terminal = $($cfg.terminal)"
  $lines += "tls = $($cfg.tls)"
  $lines += "ignore_unsafe_cert = $($cfg.ignore_unsafe_cert)"
  $lines += "fake = $($cfg.fake)"
  $lines += "realtime_info_interval = $($cfg.realtime_info_interval)"
  $lines += "billing_day = $($cfg.billing_day)"
  $lines += "log_level = \"$($cfg.log_level)\""
  $lines += "auto_update = $($cfg.auto_update)"
  $lines += "update_repo = \"$($cfg.update_repo)\""
  Set-Content -Path $ConfigPath -Value ($lines -join "`n") -Encoding UTF8
}

function Load-Config {
  if (-not (Test-Path $ConfigPath)) { return $null }
  $cfg = @{ }
  Get-Content $ConfigPath | ForEach-Object {
    if ($_ -match '^\s*#') { return }
    if ($_ -match '^\s*$') { return }
    $kv = $_ -split '=', 2
    if ($kv.Length -ne 2) { return }
    $k = ($kv[0]).Trim()
    $v = ($kv[1]).Trim()
    $v = $v.Trim('"')
    $cfg[$k] = $v
  }
  if (-not $cfg['ip_provider']) { $cfg['ip_provider'] = 'ipinfo' }
  if (-not $cfg['terminal']) { $cfg['terminal'] = 'false' }
  if (-not $cfg['tls']) { $cfg['tls'] = 'false' }
  if (-not $cfg['ignore_unsafe_cert']) { $cfg['ignore_unsafe_cert'] = 'false' }
  if (-not $cfg['fake']) { $cfg['fake'] = '1' }
  if (-not $cfg['realtime_info_interval']) { $cfg['realtime_info_interval'] = '1000' }
  if (-not $cfg['billing_day']) { $cfg['billing_day'] = '1' }
  if (-not $cfg['log_level']) { $cfg['log_level'] = 'info' }
  if (-not $cfg['auto_update']) { $cfg['auto_update'] = '0' }
  if (-not $cfg['update_repo']) { $cfg['update_repo'] = $Repo }
  return $cfg
}

function Ensure-Service {
  if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) { return }
  New-Service -Name $ServiceName -BinaryPathName "\"$InstallPath\" --config \"$ConfigPath\"" -DisplayName 'Komari Monitor RS' -StartupType Automatic | Out-Null
}

function Install {
  Write-Step '安装程序'
  New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
  $archFile = Get-ArchFile
  $url = "https://github.com/$Repo/releases/download/latest/$archFile"
  if ($Proxy) { $url = "$Proxy/$url" }
  Write-Info "下载: $url"
  $tmp = New-TemporaryFile
  Download-File $url $tmp.FullName
  Move-Item $tmp.FullName $InstallPath -Force
  Write-Info "安装到: $InstallPath"
}

function Upgrade-Binary {
  if (-not (Test-Path $InstallPath)) { Write-Err '未安装程序'; exit 1 }
  Write-Step '升级程序'
  $archFile = Get-ArchFile
  $url = "https://github.com/$Repo/releases/download/latest/$archFile"
  if ($Proxy) { $url = "$Proxy/$url" }
  Write-Info "下载: $url"
  $tmp = New-TemporaryFile
  Download-File $url $tmp.FullName
  Stop-Service -Name $ServiceName -ErrorAction SilentlyContinue
  Move-Item $tmp.FullName $InstallPath -Force
  Start-Service -Name $ServiceName -ErrorAction SilentlyContinue
  Write-Info '升级完成'
}

function Uninstall-All {
  Write-Step '卸载程序'
  Stop-Service -Name $ServiceName -ErrorAction SilentlyContinue
  if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    sc.exe delete $ServiceName | Out-Null
  }
  if (Test-Path $InstallPath) { Remove-Item $InstallPath -Force }
  if (Test-Path $ConfigPath) { Remove-Item $ConfigPath -Force }
  Write-Info '卸载完成'
}

function Manage-Menu {
  Write-Step '进入管理模式'
  $cfg = Load-Config
  if (-not $cfg) {
    Write-Warn "未找到配置文件: $ConfigPath"
    $create = Read-Host '是否创建新的配置文件? (y/N)'
    if ($create -match '^(y|Y)') {
      $cfg = @{ }
      $cfg.http_server = Read-Host 'http_server'
      $cfg.ws_server = ''
      $cfg.token = Read-Host 'token'
      $cfg.ip_provider = 'ipinfo'
      $cfg.terminal = 'false'
      $cfg.tls = 'false'
      $cfg.ignore_unsafe_cert = 'false'
      $cfg.fake = '1'
      $cfg.realtime_info_interval = '1000'
      $cfg.billing_day = '1'
      $cfg.log_level = 'info'
      $cfg.auto_update = '0'
      $cfg.update_repo = $Repo
      Save-Config $cfg
    } else { return }
  }

  while ($true) {
    Write-Host ''
    Write-Host "当前配置:" -ForegroundColor Cyan
    Write-Host " 1) http_server            = $($cfg.http_server)"
    Write-Host " 2) ws_server              = $($cfg.ws_server)"
    Write-Host " 3) token                  = ********"
    Write-Host " 4) ip_provider            = $($cfg.ip_provider)"
    Write-Host " 5) terminal               = $($cfg.terminal)"
    Write-Host " 6) tls                    = $($cfg.tls)"
    Write-Host " 7) ignore_unsafe_cert     = $($cfg.ignore_unsafe_cert)"
    Write-Host " 8) fake                   = $($cfg.fake)"
    Write-Host " 9) realtime_info_interval = $($cfg.realtime_info_interval)"
    Write-Host "10) billing_day            = $($cfg.billing_day)"
    Write-Host "11) log_level              = $($cfg.log_level)"
    Write-Host "12) auto_update            = $($cfg.auto_update)"
    Write-Host "13) update_repo            = $($cfg.update_repo)"
    Write-Host " s) 保存并返回   c) 取消并返回"
    $choice = Read-Host '请选择要修改的项 [1-13/s/c]'
    switch ($choice) {
      '1' { $cfg.http_server = Read-Host 'http_server' }
      '2' { $cfg.ws_server = Read-Host 'ws_server (留空=自动推断)' }
      '3' { $cfg.token = Read-Host 'token' }
      '4' { $cfg.ip_provider = Read-Host 'ip_provider (ipinfo/cloudflare)' }
      '5' { $cfg.terminal = Read-Host 'terminal (true/false)' }
      '6' { $cfg.tls = Read-Host 'tls (true/false)' }
      '7' { $cfg.ignore_unsafe_cert = Read-Host 'ignore_unsafe_cert (true/false)' }
      '8' { $cfg.fake = Read-Host 'fake (整数)' }
      '9' { $cfg.realtime_info_interval = Read-Host 'realtime_info_interval (ms)' }
      '10' { $cfg.billing_day = Read-Host 'billing_day (1-31)' }
      '11' { $cfg.log_level = Read-Host 'log_level (error/warn/info/debug/trace)' }
      '12' { $cfg.auto_update = Read-Host 'auto_update (小时, 0=禁用)' }
      '13' { $cfg.update_repo = Read-Host 'update_repo (owner/repo)' }
      's' { Save-Config $cfg; Write-Info "配置已保存: $ConfigPath"; return }
      'S' { Save-Config $cfg; Write-Info "配置已保存: $ConfigPath"; return }
      'c' { Write-Warn '已取消修改'; return }
      'C' { Write-Warn '已取消修改'; return }
      default { Write-Warn '无效选择' }
    }
  }
}

function Ensure-Running {
  Ensure-Service
  Start-Service -Name $ServiceName -ErrorAction SilentlyContinue
  Write-Info '服务已启动'
}

Ensure-Admin

if ($Uninstall) { Uninstall-All; exit 0 }
if ($Upgrade) { Upgrade-Binary; exit 0 }
if ($Manage) { Manage-Menu; exit 0 }

Write-Step '开始安装'

if (-not $HttpServer -or (-not $Token -and -not $AutoDiscovery)) {
  Write-Host '请选择连接模式:'
  Write-Host '  1) 传统模式 (输入 Http 与 Token)'
  Write-Host '  2) 自动发现模式 (从主端注册获取 Token)'
  $mode = Read-Host '请选择 [1/2] (默认 1)'
  if ($mode -eq '2') {
    if (-not $HttpServer) { $HttpServer = Read-Host 'HttpServer (例如 http://127.0.0.1:8080)' }
    if (-not $AutoDiscovery) { $AutoDiscovery = Read-Host '自动发现密钥' }
  } else {
    if (-not $HttpServer) { $HttpServer = Read-Host 'HttpServer (例如 http://127.0.0.1:8080)' }
    if (-not $WsServer) { $WsServer = Read-Host 'WsServer (留空自动推断)' }
    if (-not $Token) { $Token = Read-Host 'Token' }
  }
}

if ($AutoDiscovery -and $HttpServer) {
  $Token = Auto-Discover $HttpServer $AutoDiscovery
}

Install

$cfg = @{ 
  http_server = $HttpServer; ws_server = $WsServer; token = $Token;
  ip_provider = 'ipinfo'; terminal = ($Terminal.IsPresent ? 'true' : 'false');
  tls = ($Tls.IsPresent ? 'true' : 'false'); ignore_unsafe_cert = ($IgnoreUnsafeCert.IsPresent ? 'true' : 'false');
  fake = $Fake; realtime_info_interval = $RealtimeInfoInterval; billing_day = $BillingDay;
  log_level = 'info'; auto_update = $AutoUpdate; update_repo = $Repo 
}
Save-Config $cfg
Write-Info "配置文件: $ConfigPath"

Ensure-Running

Write-Host ''
Write-Info '安装成功！'
Write-Host "  查看服务: Get-Service -Name $ServiceName"
Write-Host "  查看日志: Get-EventLog -LogName Application -Newest 200 | Where-Object { $_.Message -like '*komari*' }"
Write-Host "  管理配置: powershell -ExecutionPolicy Bypass -File .\\install.ps1 -Manage"
