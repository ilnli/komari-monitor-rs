#!/bin/bash

#================================================================================
# Komari Monitor RS 安装脚本
#
# 功能:
#   - 检查 Root 权限
#   - 自动安装依赖 (wget)
#   - 自动检测系统架构并下载对应程序
#   - 通过命令行参数或交互式提问配置程序
#   - 创建并启用 systemd 服务实现后台保活和开机自启
#
# 使用方法:
#   1. 直接运行: bash install.sh
#   2. 带参数运行:
#      bash install.sh --http-server "http://your.server:port" --ws-server "ws://your.server:port" --token "your_token" [--terminal]
#================================================================================

# --- 配置 ---
# GitHub 仓库信息
GITHUB_REPO="GenshinMinecraft/komari-monitor-rs"
# 安装路径
INSTALL_PATH="/usr/local/bin/komari-monitor-rs"
# 服务名称
SERVICE_NAME="komari-agent-rs"
# systemd 服务文件路径
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

# --- 颜色定义 ---
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# --- 日志函数 ---
log_info() {
    echo -e "${GREEN}[信息] $1${NC}"
}

log_warn() {
    echo -e "${YELLOW}[警告] $1${NC}"
}

log_error() {
    echo -e "${RED}[错误] $1${NC}"
}

# --- 脚本核心函数 ---

# 1. 检查是否以 Root 用户运行
check_root() {
    if [ "$(id -u)" -ne 0 ]; then
        log_error "此脚本需要 root 权限才能创建 systemd 服务。请使用 'sudo bash install.sh' 运行。"
        exit 1
    fi
}

# 2. 安装必要的依赖 (wget)
install_dependencies() {
    if command -v wget &> /dev/null; then
        log_info "依赖 'wget' 已安装。"
        return
    fi

    log_info "正在尝试安装 'wget'..."
    if command -v apt-get &> /dev/null; then
        apt-get update && apt-get install -y wget
    elif command -v yum &> /dev/null; then
        yum install -y wget
    elif command -v dnf &> /dev/null; then
        dnf install -y wget
    elif command -v pacman &> /dev/null; then
        pacman -Sy --noconfirm wget
    else
        log_error "未找到支持的包管理器 (apt/yum/dnf/pacman)。请手动安装 'wget' 后再运行此脚本。"
        exit 1
    fi

    if ! command -v wget &> /dev/null; then
        log_error "安装 'wget' 失败。请检查您的包管理器配置。"
        exit 1
    fi
    log_info "'wget' 安装成功。"
}

# 3. 检测系统架构
get_arch() {
    ARCH=$(uname -m)
    case $ARCH in
        x86_64)
            # 优先选择 gnu，因为通用性更强
            echo "komari-monitor-rs-linux-x86_64-gnu"
            ;;
        i686)
            echo "komari-monitor-rs-linux-i686-gnu"
            ;;
        aarch64)
            echo "komari-monitor-rs-linux-aarch64-gnu"
            ;;
        armv7l)
            # armv7l 通常对应 armv7-gnueabihf
            echo "komari-monitor-rs-linux-armv7-gnueabihf"
            ;;
        armv5tejl)
            echo "komari-monitor-rs-linux-armv5te-gnueabi"
            ;;
        *)
            log_error "不支持的系统架构: $ARCH"
            log_error "请从以下列表中手动选择并下载: https://github.com/${GITHUB_REPO}/releases/latest"
            exit 1
            ;;
    esac
}

# --- 主程序 ---
main() {
    check_root
    log_info "Komari Monitor RS 安装程序已启动。"

    # --- 参数初始化 ---
    HTTP_SERVER=""
    WS_SERVER=""
    TOKEN=""
    FAKE="1"
    INTERVAL="1000"
    BILLING_DAY="1"
    TLS_FLAG=""
    IGNORE_CERT_FLAG=""
    TERMINAL_FLAG="" # <-- 新增: 为 --terminal 参数初始化一个标志变量

    # --- 解析命令行参数 ---
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --http-server) HTTP_SERVER="$2"; shift 2;;
            --ws-server) WS_SERVER="$2"; shift 2;;
            -t|--token) TOKEN="$2"; shift 2;;
            -f|--fake) FAKE="$2"; shift 2;;
            --realtime-info-interval) INTERVAL="$2"; shift 2;;
            --billing-day) BILLING_DAY="$2"; shift 2;;
            --tls) TLS_FLAG="--tls"; shift 1;;
            --ignore-unsafe-cert) IGNORE_CERT_FLAG="--ignore-unsafe-cert"; shift 1;;
            --terminal) TERMINAL_FLAG="--terminal"; shift 1;; # <-- 新增: 识别 --terminal 参数
            *) log_warn "未知的参数: $1"; shift 1;;
        esac
    done

    # --- 交互式询问缺失的必要参数 ---
    if [ -z "$HTTP_SERVER" ]; then
        read -p "请输入主端 Http 地址 (例如 http://127.0.0.1:8080): " HTTP_SERVER
    fi
    if [ -z "$WS_SERVER" ]; then
        read -p "请输入主端 WebSocket 地址 (例如 ws://127.0.0.1:8080): " WS_SERVER
    fi
    if [ -z "$TOKEN" ]; then
        read -p "请输入 Token: " TOKEN
    fi

    # <-- 新增: 交互式询问 --terminal (仅当命令行未提供时)
    if [ -z "$TERMINAL_FLAG" ]; then
      read -p "是否启用 Web Terminal 功能? (y/N): " enable_terminal
      # 将输入转换为小写以方便比较
      enable_terminal_lower=$(echo "$enable_terminal" | tr '[:upper:]' '[:lower:]')
      if [[ "$enable_terminal_lower" == "y" || "$enable_terminal_lower" == "yes" ]]; then
          TERMINAL_FLAG="--terminal"
          log_info "Web Terminal 功能已启用。"
      else
          log_info "Web Terminal 功能未启用。"
      fi
    fi

    # 交互式询问计费日
    if [ "$BILLING_DAY" == "1" ]; then
      read -p "请输入计费日 (每月第几号开始统计流量，默认为 1，直接回车使用默认值): " input_billing_day
      if [ -n "$input_billing_day" ]; then
          # 验证输入是否为 1-31 之间的数字
          if [[ "$input_billing_day" =~ ^[0-9]+$ ]] && [ "$input_billing_day" -ge 1 ] && [ "$input_billing_day" -le 31 ]; then
              BILLING_DAY="$input_billing_day"
              log_info "计费日已设置为每月 ${BILLING_DAY} 号。"
          else
              log_warn "无效的计费日，使用默认值 1。"
              BILLING_DAY="1"
          fi
      else
          log_info "使用默认计费日: 每月 1 号。"
      fi
    fi

    # 验证输入
    if [ -z "$HTTP_SERVER" ] || [ -z "$WS_SERVER" ] || [ -z "$TOKEN" ]; then
        log_error "Http 地址, WebSocket 地址和 Token 不能为空。"
        exit 1
    fi

    log_info "配置信息确认:"
    echo "  - Http Server: $HTTP_SERVER"
    echo "  - WS Server: $WS_SERVER"
    echo "  - Token: ********" # 隐藏Token
    echo "  - 虚假倍率: $FAKE"
    echo "  - 上传间隔: $INTERVAL ms"
    echo "  - 计费日: 每月 $BILLING_DAY 号"
    echo "  - 启用 TLS: ${TLS_FLAG:--}"
    echo "  - 忽略证书: ${IGNORE_CERT_FLAG:--}"
    echo "  - 启用 Terminal: ${TERMINAL_FLAG:--}" # <-- 新增: 显示 terminal 状态
    echo ""

    # --- 安装流程 ---
    install_dependencies

    ARCH_FILE=$(get_arch)
    DOWNLOAD_URL="https://ghfast.top/https://github.com/${GITHUB_REPO}/releases/download/latest/${ARCH_FILE}"

    log_info "检测到系统架构: $(uname -m)"
    log_info "准备从以下地址下载文件: ${DOWNLOAD_URL}"

    if ! wget -O "${INSTALL_PATH}" "${DOWNLOAD_URL}"; then
        log_error "下载失败！请检查网络连接或确认该架构的文件是否存在。"
        exit 1
    fi

    chmod +x "${INSTALL_PATH}"
    log_info "程序已成功下载并安装到: ${INSTALL_PATH}"

    # --- 创建 systemd 服务 ---
    log_info "正在创建 systemd 服务..."

    # 构建启动命令
    EXEC_START_CMD="${INSTALL_PATH} --http-server \"${HTTP_SERVER}\" --ws-server \"${WS_SERVER}\" --token \"${TOKEN}\" --fake \"${FAKE}\" --realtime-info-interval \"${INTERVAL}\" --billing-day \"${BILLING_DAY}\""
    if [ -n "$TLS_FLAG" ]; then
        EXEC_START_CMD="$EXEC_START_CMD $TLS_FLAG"
    fi
    if [ -n "$IGNORE_CERT_FLAG" ]; then
        EXEC_START_CMD="$EXEC_START_CMD $IGNORE_CERT_FLAG"
    fi
    # <-- 新增: 将 --terminal 标志添加到启动命令
    if [ -n "$TERMINAL_FLAG" ]; then
        EXEC_START_CMD="$EXEC_START_CMD $TERMINAL_FLAG"
    fi

    # 使用 cat 和 EOF 创建服务文件
    cat > ${SERVICE_FILE} <<EOF
[Unit]
Description=Komari Monitor RS Service
After=network.target

[Service]
Type=simple
User=root
ExecStart=${EXEC_START_CMD}
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

    log_info "systemd 服务文件已创建: ${SERVICE_FILE}"

    # --- 启用并启动服务 ---
    log_info "正在重载 systemd, 启用并启动服务..."
    systemctl daemon-reload
    systemctl enable ${SERVICE_NAME}
    systemctl restart ${SERVICE_NAME} # 使用 restart 确保服务是最新的

    # --- 检查服务状态 ---
    sleep 2 # 等待服务启动
    if systemctl is-active --quiet ${SERVICE_NAME}; then
        log_info "服务 '${SERVICE_NAME}' 已成功启动并正在运行！"
        log_info "您可以使用 'systemctl status ${SERVICE_NAME}' 命令查看服务状态。"
        log_info "您可以使用 'journalctl -u ${SERVICE_NAME} -f' 命令查看实时日志。"
    else
        log_error "服务 '${SERVICE_NAME}' 启动失败！"
        log_error "请使用 'systemctl status ${SERVICE_NAME}' 和 'journalctl -u ${SERVICE_NAME}' 命令检查错误详情。"
        exit 1
    fi
}

# --- 执行主程序 ---
main "$@"