#!/bin/bash

#================================================================================
# Komari Monitor RS 管理脚本
#
# 功能:
#   - 安装/升级/卸载程序
#   - 启动/停止/重启服务
#   - 修改配置
#   - 查看日志和状态
#   - 兼容旧版安装
#
# 使用方法:
#   ./komari-manager.sh [命令] [选项]
#================================================================================

set -e

# --- 配置 ---
# 原始仓库: GenshinMinecraft/komari-monitor-rs
# Fork 仓库: ilnli/komari-monitor-rs
DEFAULT_GITHUB_REPO="ilnli/komari-monitor-rs"
INSTALL_PATH="/usr/local/bin/komari-monitor-rs"
SERVICE_NAME="komari-agent-rs"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
DATA_DIR="/var/lib/komari-monitor"
NETWORK_STATS_FILE="/etc/komari-network.conf"

# --- 颜色定义 ---
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# --- 日志函数 ---
log_info() { echo -e "${GREEN}[信息]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[警告]${NC} $1"; }
log_error() { echo -e "${RED}[错误]${NC} $1"; }
log_step() { echo -e "${BLUE}[步骤]${NC} $1"; }

# --- 基础检查函数 ---
check_root() {
    if [ "$(id -u)" -ne 0 ]; then
        log_error "此脚本需要 root 权限运行"
        exit 1
    fi
}

check_installed() {
    [ -f "${INSTALL_PATH}" ]
}

check_service_exists() {
    [ -f "${SERVICE_FILE}" ]
}

check_service_running() {
    systemctl is-active --quiet ${SERVICE_NAME} 2>/dev/null
}

# --- 依赖安装 ---
install_dependencies() {
    if command -v wget &> /dev/null || command -v curl &> /dev/null; then
        return
    fi

    log_info "正在安装下载工具..."
    if command -v apt-get &> /dev/null; then
        apt-get update && apt-get install -y wget
    elif command -v yum &> /dev/null; then
        yum install -y wget
    elif command -v dnf &> /dev/null; then
        dnf install -y wget
    elif command -v pacman &> /dev/null; then
        pacman -Sy --noconfirm wget
    elif command -v apk &> /dev/null; then
        apk add wget
    else
        log_error "未找到支持的包管理器，请手动安装 wget 或 curl"
        exit 1
    fi
}

# --- 下载函数 ---
download_file() {
    local url="$1"
    local output="$2"

    if command -v curl &> /dev/null; then
        curl -fsSL -o "$output" "$url"
    elif command -v wget &> /dev/null; then
        wget -q -O "$output" "$url"
    else
        log_error "未找到 curl 或 wget"
        return 1
    fi
}

# --- 架构检测 ---
get_arch() {
    local arch=$(uname -m)
    case $arch in
        x86_64)
            echo "komari-monitor-rs-linux-x86_64-gnu"
            ;;
        i686|i386)
            echo "komari-monitor-rs-linux-i686-gnu"
            ;;
        aarch64|arm64)
            echo "komari-monitor-rs-linux-aarch64-gnu"
            ;;
        armv7l|armv7)
            echo "komari-monitor-rs-linux-armv7-gnueabihf"
            ;;
        armv5tejl|armv5te)
            echo "komari-monitor-rs-linux-armv5te-gnueabi"
            ;;
        *)
            log_error "不支持的系统架构: $arch"
            exit 1
            ;;
    esac
}

# --- 从现有服务文件解析配置 ---
parse_existing_config() {
    if [ ! -f "${SERVICE_FILE}" ]; then
        return 1
    fi

    local exec_line=$(grep "^ExecStart=" "${SERVICE_FILE}" | sed 's/^ExecStart=//')

    # 解析各参数
    CURRENT_HTTP_SERVER=$(echo "$exec_line" | grep -oP '(?<=--http-server ")[^"]+' || echo "")
    CURRENT_WS_SERVER=$(echo "$exec_line" | grep -oP '(?<=--ws-server ")[^"]+' || echo "")
    CURRENT_TOKEN=$(echo "$exec_line" | grep -oP '(?<=--token ")[^"]+' || echo "")
    CURRENT_FAKE=$(echo "$exec_line" | grep -oP '(?<=--fake ")[^"]+' || echo "1")
    CURRENT_INTERVAL=$(echo "$exec_line" | grep -oP '(?<=--realtime-info-interval ")[^"]+' || echo "1000")
    CURRENT_BILLING_DAY=$(echo "$exec_line" | grep -oP '(?<=--billing-day ")[^"]+' || echo "1")

    # 检查布尔标志
    [[ "$exec_line" == *"--tls"* ]] && CURRENT_TLS="true" || CURRENT_TLS="false"
    [[ "$exec_line" == *"--ignore-unsafe-cert"* ]] && CURRENT_IGNORE_CERT="true" || CURRENT_IGNORE_CERT="false"
    [[ "$exec_line" == *"--terminal"* ]] && CURRENT_TERMINAL="true" || CURRENT_TERMINAL="false"

    return 0
}

# --- 显示当前配置 ---
show_current_config() {
    if ! parse_existing_config; then
        log_warn "未找到现有配置"
        return 1
    fi

    echo ""
    echo "当前配置:"
    echo "  HTTP Server: ${CURRENT_HTTP_SERVER}"
    echo "  WS Server: ${CURRENT_WS_SERVER:-(自动推断)}"
    echo "  Token: ********"
    echo "  虚假倍率: ${CURRENT_FAKE}"
    echo "  上传间隔: ${CURRENT_INTERVAL} ms"
    echo "  计费日: 每月 ${CURRENT_BILLING_DAY} 号"
    echo "  TLS: ${CURRENT_TLS}"
    echo "  忽略证书: ${CURRENT_IGNORE_CERT}"
    echo "  Terminal: ${CURRENT_TERMINAL}"
    echo ""
}

# --- 创建服务文件 ---
create_service_file() {
    local http_server="$1"
    local ws_server="$2"
    local token="$3"
    local fake="${4:-1}"
    local interval="${5:-1000}"
    local billing_day="${6:-1}"
    local tls_flag="$7"
    local ignore_cert_flag="$8"
    local terminal_flag="$9"

    # 构建启动命令
    local exec_cmd="${INSTALL_PATH} --http-server \"${http_server}\""

    [ -n "$ws_server" ] && exec_cmd="$exec_cmd --ws-server \"${ws_server}\""
    exec_cmd="$exec_cmd --token \"${token}\""
    exec_cmd="$exec_cmd --fake \"${fake}\""
    exec_cmd="$exec_cmd --realtime-info-interval \"${interval}\""
    exec_cmd="$exec_cmd --billing-day \"${billing_day}\""

    [ "$tls_flag" = "true" ] && exec_cmd="$exec_cmd --tls"
    [ "$ignore_cert_flag" = "true" ] && exec_cmd="$exec_cmd --ignore-unsafe-cert"
    [ "$terminal_flag" = "true" ] && exec_cmd="$exec_cmd --terminal"

    cat > "${SERVICE_FILE}" <<EOF
[Unit]
Description=Komari Monitor RS Service
After=network.target

[Service]
Type=simple
User=root
ExecStart=${exec_cmd}
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    log_info "服务文件已创建: ${SERVICE_FILE}"
}

# --- 交互式配置 ---
interactive_config() {
    local use_existing="${1:-false}"

    # 如果有现有配置，先加载
    if [ "$use_existing" = "true" ] && parse_existing_config; then
        HTTP_SERVER="${CURRENT_HTTP_SERVER}"
        WS_SERVER="${CURRENT_WS_SERVER}"
        TOKEN="${CURRENT_TOKEN}"
        FAKE="${CURRENT_FAKE}"
        INTERVAL="${CURRENT_INTERVAL}"
        BILLING_DAY="${CURRENT_BILLING_DAY}"
        TLS_FLAG="${CURRENT_TLS}"
        IGNORE_CERT_FLAG="${CURRENT_IGNORE_CERT}"
        TERMINAL_FLAG="${CURRENT_TERMINAL}"
    else
        HTTP_SERVER=""
        WS_SERVER=""
        TOKEN=""
        FAKE="1"
        INTERVAL="1000"
        BILLING_DAY="1"
        TLS_FLAG="false"
        IGNORE_CERT_FLAG="false"
        TERMINAL_FLAG="false"
    fi

    echo ""
    # HTTP Server
    read -p "HTTP 服务器地址 [${HTTP_SERVER}]: " input
    [ -n "$input" ] && HTTP_SERVER="$input"
    if [ -z "$HTTP_SERVER" ]; then
        log_error "HTTP 服务器地址不能为空"
        exit 1
    fi

    # WS Server
    read -p "WebSocket 服务器地址 (留空自动推断) [${WS_SERVER}]: " input
    [ -n "$input" ] && WS_SERVER="$input"

    # Token
    read -p "认证 Token [${TOKEN:+********}]: " input
    [ -n "$input" ] && TOKEN="$input"
    if [ -z "$TOKEN" ]; then
        log_error "Token 不能为空"
        exit 1
    fi

    # 计费日
    read -p "计费日 (每月第几号) [${BILLING_DAY}]: " input
    [ -n "$input" ] && BILLING_DAY="$input"

    # Terminal
    read -p "启用 Web Terminal? (y/N) [${TERMINAL_FLAG}]: " input
    if [ -n "$input" ]; then
        [[ "$input" =~ ^[Yy] ]] && TERMINAL_FLAG="true" || TERMINAL_FLAG="false"
    fi

    # TLS
    read -p "启用 TLS? (y/N) [${TLS_FLAG}]: " input
    if [ -n "$input" ]; then
        [[ "$input" =~ ^[Yy] ]] && TLS_FLAG="true" || TLS_FLAG="false"
    fi

    # 忽略证书
    if [ "$TLS_FLAG" = "true" ]; then
        read -p "忽略不安全证书? (y/N) [${IGNORE_CERT_FLAG}]: " input
        if [ -n "$input" ]; then
            [[ "$input" =~ ^[Yy] ]] && IGNORE_CERT_FLAG="true" || IGNORE_CERT_FLAG="false"
        fi
    fi
}

# --- 安装/升级 ---
do_install() {
    local upgrade_only="${1:-false}"
    local use_proxy="${2:-}"
    local proxy_url="${3:-https://ghfast.top}"

    log_step "开始${upgrade_only:+升级}${upgrade_only:-安装} Komari Monitor RS..."

    install_dependencies

    # 检测架构并构建下载URL
    local arch_file=$(get_arch)
    # GitHub releases 最新版本下载格式: /releases/latest/download/{filename}
    local github_url="https://github.com/${GITHUB_REPO:-$DEFAULT_GITHUB_REPO}/releases/latest/download/${arch_file}"
    local download_url="$github_url"

    if [ -n "$use_proxy" ]; then
        download_url="${proxy_url%/}/${github_url}"
        log_info "使用代理: ${proxy_url}"
    fi

    log_info "下载地址: ${download_url}"

    # 下载到临时文件
    local temp_file=$(mktemp)
    if ! download_file "${download_url}" "${temp_file}"; then
        log_error "下载失败!"
        rm -f "${temp_file}"
        [ -z "$use_proxy" ] && log_warn "可尝试使用 --proxy 参数"
        exit 1
    fi

    # 如果服务正在运行，先停止
    if check_service_running; then
        log_info "停止现有服务..."
        systemctl stop ${SERVICE_NAME}
    fi

    # 备份旧版本
    if check_installed; then
        local backup="${INSTALL_PATH}.backup"
        cp "${INSTALL_PATH}" "${backup}"
        log_info "已备份旧版本到: ${backup}"
    fi

    # 安装新版本
    mv "${temp_file}" "${INSTALL_PATH}"
    chmod +x "${INSTALL_PATH}"
    log_info "程序已安装到: ${INSTALL_PATH}"
}

# --- 卸载 ---
do_uninstall() {
    log_step "开始卸载 Komari Monitor RS..."

    if ! check_installed && ! check_service_exists; then
        log_error "未检测到安装"
        exit 1
    fi

    # 停止并禁用服务
    if check_service_running; then
        log_info "停止服务..."
        systemctl stop ${SERVICE_NAME}
    fi

    if systemctl is-enabled --quiet ${SERVICE_NAME} 2>/dev/null; then
        log_info "禁用服务..."
        systemctl disable ${SERVICE_NAME}
    fi

    # 删除文件
    [ -f "${SERVICE_FILE}" ] && rm -f "${SERVICE_FILE}"
    [ -f "${INSTALL_PATH}" ] && rm -f "${INSTALL_PATH}"
    [ -f "${INSTALL_PATH}.backup" ] && rm -f "${INSTALL_PATH}.backup"
    systemctl daemon-reload

    # 询问是否删除数据
    read -p "是否删除流量统计数据? (y/N): " del_data
    if [[ "$del_data" =~ ^[Yy] ]]; then
        [ -d "${DATA_DIR}" ] && rm -rf "${DATA_DIR}"
        [ -f "${NETWORK_STATS_FILE}" ] && rm -f "${NETWORK_STATS_FILE}"
        log_info "数据已删除"
    fi

    log_info "卸载完成!"
}

# --- 服务控制 ---
do_start() {
    if ! check_installed; then
        log_error "程序未安装"
        exit 1
    fi
    if check_service_running; then
        log_warn "服务已在运行"
        return
    fi
    systemctl start ${SERVICE_NAME}
    log_info "服务已启动"
}

do_stop() {
    if ! check_service_running; then
        log_warn "服务未在运行"
        return
    fi
    systemctl stop ${SERVICE_NAME}
    log_info "服务已停止"
}

do_restart() {
    if ! check_installed; then
        log_error "程序未安装"
        exit 1
    fi
    systemctl restart ${SERVICE_NAME}
    log_info "服务已重启"
}

# --- 日志查看 ---
do_logs() {
    local lines="${1:-50}"
    local follow="${2:-false}"

    if [ "$follow" = "true" ]; then
        journalctl -u ${SERVICE_NAME} -f
    else
        journalctl -u ${SERVICE_NAME} -n "$lines" --no-pager
    fi
}

# --- 状态显示 ---
do_status() {
    echo ""
    echo "========== Komari Monitor RS 状态 =========="

    if check_installed; then
        local version=$(${INSTALL_PATH} --version 2>/dev/null | head -n1 || echo "未知")
        log_info "安装状态: 已安装"
        log_info "版本: ${version}"
        log_info "程序路径: ${INSTALL_PATH}"
    else
        log_warn "安装状态: 未安装"
        echo "============================================"
        return
    fi

    if check_service_running; then
        log_info "服务状态: 运行中"
    else
        log_warn "服务状态: 已停止"
    fi

    if check_service_exists; then
        show_current_config
    fi

    echo "============================================"
}

# --- 修改配置 ---
do_config() {
    if ! check_service_exists; then
        log_warn "未找到服务配置，将创建新配置"
        interactive_config false
    else
        show_current_config
        read -p "是否修改配置? (y/N): " modify
        if [[ "$modify" =~ ^[Yy] ]]; then
            interactive_config true
        else
            return
        fi
    fi

    # 保存配置
    create_service_file "$HTTP_SERVER" "$WS_SERVER" "$TOKEN" \
        "$FAKE" "$INTERVAL" "$BILLING_DAY" \
        "$TLS_FLAG" "$IGNORE_CERT_FLAG" "$TERMINAL_FLAG"

    # 询问是否重启
    if check_installed; then
        read -p "是否重启服务以应用配置? (Y/n): " restart
        if [[ ! "$restart" =~ ^[Nn] ]]; then
            do_restart
        fi
    fi
}

# --- 帮助信息 ---
show_help() {
    cat <<EOF
Komari Monitor RS 管理脚本

用法: $0 <命令> [选项]

命令:
  install     安装程序 (已安装则升级)
  upgrade     升级到最新版本
  uninstall   卸载程序
  start       启动服务
  stop        停止服务
  restart     重启服务
  status      查看状态
  config      修改配置
  logs        查看日志
  help        显示帮助

选项:
  --proxy [地址]    使用代理下载 (默认: https://ghfast.top)
  --repo <仓库>     指定 GitHub 仓库

日志选项:
  logs [行数]       显示最近 N 行日志 (默认 50)
  logs -f           实时跟踪日志

示例:
  $0 install                    # 安装
  $0 install --proxy            # 使用代理安装
  $0 upgrade                    # 升级
  $0 config                     # 修改配置
  $0 logs 100                   # 查看最近 100 行日志
  $0 logs -f                    # 实时查看日志
  $0 uninstall                  # 卸载

EOF
}

# --- 主入口 ---
main() {
    local cmd="${1:-}"
    shift || true

    # 解析全局选项
    USE_PROXY=""
    PROXY_URL="https://ghfast.top"
    GITHUB_REPO="$DEFAULT_GITHUB_REPO"

    while [ "$#" -gt 0 ]; do
        case "$1" in
            --proxy)
                USE_PROXY="1"
                if [ -n "${2:-}" ] && [[ ! "$2" =~ ^-- ]]; then
                    PROXY_URL="$2"
                    shift
                fi
                shift
                ;;
            --repo)
                GITHUB_REPO="$2"
                shift 2
                ;;
            *)
                break
                ;;
        esac
    done

    check_root

    case "$cmd" in
        install)
            do_install false "$USE_PROXY" "$PROXY_URL"
            if ! check_service_exists; then
                interactive_config false
                create_service_file "$HTTP_SERVER" "$WS_SERVER" "$TOKEN" \
                    "$FAKE" "$INTERVAL" "$BILLING_DAY" \
                    "$TLS_FLAG" "$IGNORE_CERT_FLAG" "$TERMINAL_FLAG"
            fi
            systemctl enable ${SERVICE_NAME} >/dev/null 2>&1
            systemctl restart ${SERVICE_NAME}
            sleep 2
            do_status
            ;;
        upgrade|update)
            do_install true "$USE_PROXY" "$PROXY_URL"
            if check_service_exists; then
                systemctl restart ${SERVICE_NAME}
                log_info "服务已重启"
            fi
            ;;
        uninstall|remove)
            do_uninstall
            ;;
        start)
            do_start
            ;;
        stop)
            do_stop
            ;;
        restart)
            do_restart
            ;;
        status)
            do_status
            ;;
        config)
            do_config
            ;;
        logs)
            if [ "${1:-}" = "-f" ]; then
                do_logs 50 true
            elif [ -n "${1:-}" ]; then
                do_logs "$1"
            else
                do_logs
            fi
            ;;
        help|--help|-h)
            show_help
            ;;
        *)
            show_help
            exit 1
            ;;
    esac
}

main "$@"
