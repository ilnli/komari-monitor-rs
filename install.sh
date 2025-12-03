#!/bin/bash

#================================================================================
# Komari Monitor RS 安装/卸载脚本
#
# 功能:
#   - 检查 Root 权限
#   - 自动安装依赖 (wget/curl)
#   - 自动检测系统架构并下载对应程序
#   - 通过命令行参数或交互式提问配置程序
#   - 创建并启用 systemd 服务实现后台保活和开机自启
#   - 支持卸载功能
#
# 使用方法:
#   一键安装 (需自行修改参数):
#     bash <(curl -sL https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/install.sh) \
#       --http-server "http://your.server:port" --ws-server "ws://your.server:port" --token "your_token"
#
#   交互式安装:
#     bash install.sh
#
#   带参数安装:
#     bash install.sh --http-server "http://your.server:port" --ws-server "ws://your.server:port" --token "your_token" [--terminal] [--proxy]
#
#   卸载:
#     bash install.sh --uninstall
#================================================================================

# --- 配置 ---
# GitHub 仓库信息 (默认值，可通过 --repo 参数覆盖)
DEFAULT_GITHUB_REPO="ilnli/komari-monitor-rs"
# 安装路径
INSTALL_PATH="/usr/local/bin/komari-monitor-rs"
# 服务名称
SERVICE_NAME="komari-agent-rs"
# systemd 服务文件路径
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
# 流量统计数据目录
DATA_DIR="/var/lib/komari-monitor"

# --- 颜色定义 ---
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# --- 日志函数 ---
log_info() {
    echo -e "${GREEN}[信息]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[警告]${NC} $1"
}

log_error() {
    echo -e "${RED}[错误]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[步骤]${NC} $1"
}

# --- 脚本核心函数 ---

# 1. 检查是否以 Root 用户运行
check_root() {
    if [ "$(id -u)" -ne 0 ]; then
        log_error "此脚本需要 root 权限。请使用 'sudo bash install.sh' 或以 root 用户运行。"
        exit 1
    fi
}

# 2. 安装必要的依赖 (wget 或 curl)
install_dependencies() {
    if command -v wget &> /dev/null || command -v curl &> /dev/null; then
        log_info "下载工具已就绪。"
        return
    fi

    log_info "正在尝试安装下载工具..."
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
        log_error "未找到支持的包管理器。请手动安装 'wget' 或 'curl' 后再运行此脚本。"
        exit 1
    fi

    if ! command -v wget &> /dev/null && ! command -v curl &> /dev/null; then
        log_error "安装下载工具失败。请手动安装 'wget' 或 'curl'。"
        exit 1
    fi
    log_info "下载工具安装成功。"
}

# 3. 下载文件 (优先使用 curl，其次 wget)
download_file() {
    local url="$1"
    local output="$2"
    
    if command -v curl &> /dev/null; then
        curl -fsSL -o "$output" "$url"
    elif command -v wget &> /dev/null; then
        wget -q -O "$output" "$url"
    else
        log_error "未找到 curl 或 wget，无法下载文件。"
        return 1
    fi
}

# 4. 检测系统架构
get_arch() {
    local arch
    arch=$(uname -m)
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
            log_error "请从以下列表中手动选择并下载: https://github.com/${GITHUB_REPO}/releases/latest"
            exit 1
            ;;
    esac
}

# 5. 卸载函数
uninstall() {
    log_step "开始卸载 Komari Monitor RS..."
    
    # 停止并禁用服务
    if systemctl is-active --quiet ${SERVICE_NAME} 2>/dev/null; then
        log_info "正在停止服务..."
        systemctl stop ${SERVICE_NAME}
    fi
    
    if systemctl is-enabled --quiet ${SERVICE_NAME} 2>/dev/null; then
        log_info "正在禁用服务..."
        systemctl disable ${SERVICE_NAME}
    fi
    
    # 删除服务文件
    if [ -f "${SERVICE_FILE}" ]; then
        log_info "正在删除服务文件..."
        rm -f "${SERVICE_FILE}"
        systemctl daemon-reload
    fi
    
    # 删除程序文件
    if [ -f "${INSTALL_PATH}" ]; then
        log_info "正在删除程序文件..."
        rm -f "${INSTALL_PATH}"
    fi
    
    # 询问是否删除数据目录
    if [ -d "${DATA_DIR}" ]; then
        read -p "是否删除流量统计数据? (y/N): " delete_data
        delete_data_lower=$(echo "$delete_data" | tr '[:upper:]' '[:lower:]')
        if [[ "$delete_data_lower" == "y" || "$delete_data_lower" == "yes" ]]; then
            log_info "正在删除数据目录..."
            rm -rf "${DATA_DIR}"
        else
            log_info "保留数据目录: ${DATA_DIR}"
        fi
    fi
    
    log_info "卸载完成！"
    exit 0
}

# 6. 升级函数
upgrade() {
    log_step "开始升级 Komari Monitor RS..."
    
    # 检查程序是否已安装
    if [ ! -f "${INSTALL_PATH}" ]; then
        log_error "程序未安装，请先运行安装。"
        exit 1
    fi
    
    # 获取当前版本（如果程序支持 --version）
    CURRENT_VERSION=""
    if ${INSTALL_PATH} --version &>/dev/null; then
        CURRENT_VERSION=$(${INSTALL_PATH} --version 2>/dev/null | head -n1)
        log_info "当前版本: ${CURRENT_VERSION}"
    fi
    
    # 检测架构
    ARCH_FILE=$(get_arch)
    
    # 构建下载 URL
    GITHUB_URL="https://github.com/${GITHUB_REPO}/releases/download/latest/${ARCH_FILE}"
    if [ -n "$USE_PROXY" ]; then
        PROXY="${PROXY_URL:-$DEFAULT_PROXY}"
        PROXY="${PROXY%/}"
        DOWNLOAD_URL="${PROXY}/${GITHUB_URL}"
        log_info "使用代理: ${PROXY}"
    else
        DOWNLOAD_URL="${GITHUB_URL}"
    fi
    
    log_info "下载地址: ${DOWNLOAD_URL}"
    
    # 下载到临时文件
    TEMP_FILE=$(mktemp)
    if ! download_file "${DOWNLOAD_URL}" "${TEMP_FILE}"; then
        log_error "下载失败！"
        rm -f "${TEMP_FILE}"
        if [ -z "$USE_PROXY" ]; then
            log_warn "如果您在中国大陆，可以尝试添加 --proxy 参数使用代理下载。"
        fi
        exit 1
    fi
    
    # 停止服务
    if systemctl is-active --quiet ${SERVICE_NAME} 2>/dev/null; then
        log_info "正在停止服务..."
        systemctl stop ${SERVICE_NAME}
    fi
    
    # 备份旧版本
    if [ -f "${INSTALL_PATH}" ]; then
        BACKUP_PATH="${INSTALL_PATH}.backup"
        log_info "备份旧版本到: ${BACKUP_PATH}"
        cp "${INSTALL_PATH}" "${BACKUP_PATH}"
    fi
    
    # 替换程序文件
    mv "${TEMP_FILE}" "${INSTALL_PATH}"
    chmod +x "${INSTALL_PATH}"
    log_info "程序已更新到: ${INSTALL_PATH}"
    
    # 获取新版本
    if ${INSTALL_PATH} --version &>/dev/null; then
        NEW_VERSION=$(${INSTALL_PATH} --version 2>/dev/null | head -n1)
        log_info "新版本: ${NEW_VERSION}"
    fi
    
    # 重启服务
    if [ -f "${SERVICE_FILE}" ]; then
        log_info "正在重启服务..."
        systemctl start ${SERVICE_NAME}
        
        sleep 2
        if systemctl is-active --quiet ${SERVICE_NAME}; then
            log_info "=========================================="
            log_info "  升级成功！服务已重新启动"
            log_info "=========================================="
            
            # 如果升级失败可以回滚
            if [ -f "${BACKUP_PATH}" ]; then
                log_info "如需回滚，备份文件位于: ${BACKUP_PATH}"
            fi
        else
            log_error "服务启动失败！正在尝试回滚..."
            if [ -f "${BACKUP_PATH}" ]; then
                mv "${BACKUP_PATH}" "${INSTALL_PATH}"
                chmod +x "${INSTALL_PATH}"
                systemctl start ${SERVICE_NAME}
                log_warn "已回滚到旧版本。"
            fi
            exit 1
        fi
    else
        log_info "升级完成！(未找到服务文件，请手动启动)"
    fi
    
    exit 0
}

# 7. 自动发现函数 - 调用 API 获取 token
auto_discover() {
    local endpoint="$1"
    local ad_key="$2"
    local hostname
    
    # 获取主机名
    hostname=$(hostname)
    
    # 构建 API URL
    local api_url="${endpoint}/api/clients/register?name=${hostname}"
    
    log_info "正在进行自动发现注册..."
    log_info "API: ${api_url}"
    
    local response
    local http_code
    
    # 调用 API
    if command -v curl &> /dev/null; then
        response=$(curl -s -w "\n%{http_code}" -X POST "${api_url}" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer ${ad_key}" \
            -d "{\"key\": \"${ad_key}\"}")
    elif command -v wget &> /dev/null; then
        # wget 不方便获取 HTTP 状态码，使用临时文件
        local tmp_file=$(mktemp)
        wget -q -O "${tmp_file}" --method=POST \
            --header="Content-Type: application/json" \
            --header="Authorization: Bearer ${ad_key}" \
            --body-data="{\"key\": \"${ad_key}\"}" \
            "${api_url}" 2>/dev/null
        if [ $? -eq 0 ]; then
            response=$(cat "${tmp_file}")
            response="${response}\n200"
        else
            response="\n000"
        fi
        rm -f "${tmp_file}"
    else
        log_error "未找到 curl 或 wget，无法进行自动发现。"
        return 1
    fi
    
    # 解析响应
    http_code=$(echo "$response" | tail -n1)
    local body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" != "200" ]; then
        log_error "自动发现失败！HTTP 状态码: ${http_code}"
        log_error "响应: ${body}"
        return 1
    fi
    
    # 解析 JSON 响应获取 token
    # 响应格式: {"status": "success", "message": "...", "data": {"uuid": "...", "token": "..."}}
    local status
    local token
    
    # 使用 grep 和 sed 解析 JSON (兼容没有 jq 的环境)
    status=$(echo "$body" | grep -o '"status"[[:space:]]*:[[:space:]]*"[^"]*"' | sed 's/.*"\([^"]*\)"$/\1/')
    token=$(echo "$body" | grep -o '"token"[[:space:]]*:[[:space:]]*"[^"]*"' | sed 's/.*"\([^"]*\)"$/\1/')
    
    if [ "$status" != "success" ]; then
        log_error "自动发现失败！状态: ${status}"
        log_error "响应: ${body}"
        return 1
    fi
    
    if [ -z "$token" ]; then
        log_error "自动发现失败！无法从响应中获取 token。"
        log_error "响应: ${body}"
        return 1
    fi
    
    log_info "自动发现成功！已获取 Token。"
    
    # 返回 token
    echo "$token"
    return 0
}

# 显示帮助信息
show_help() {
    cat <<EOF
Komari Monitor RS 安装脚本

用法:
  bash install.sh [选项]

选项:
  --repo <owner/repo>       GitHub 仓库 (默认: ilnli/komari-monitor-rs)
  --http-server <地址>      主端 Http 地址 (必需)
  --ws-server <地址>        主端 WebSocket 地址 (可选，默认自动推断)
  -t, --token <token>       认证 Token (使用 --auto-discovery 时可省略)
  --auto-discovery <key>    自动发现密钥 (用于批量注册 Agent)
  -f, --fake <倍率>         虚假倍率 (默认: 1)
  --realtime-info-interval <ms>  上传间隔毫秒 (默认: 1000)
  --billing-day <日期>      计费日，每月第几号 (默认: 1)
  --auto-update <小时>      自动升级检查间隔 (默认: 0，禁用)
  --tls                     启用 TLS
  --ignore-unsafe-cert      忽略不安全的证书
  --terminal                启用 Web Terminal 功能
  --proxy [地址]            使用代理下载，可指定代理地址 (默认: https://ghfast.top)
  --upgrade, --update       升级程序到最新版本
  --uninstall, --remove     卸载程序
  -h, --help                显示此帮助信息

示例:
  # 一键安装 (交互式)
  bash install.sh

  # 带参数安装
  bash install.sh --http-server "http://example.com:8080" --token "your_token"

  # 使用自动发现安装 (无需 token)
  bash install.sh --http-server "http://example.com:8080" --auto-discovery "your_ad_key"

  # 使用内置代理下载
  bash install.sh --http-server "http://example.com:8080" --token "your_token" --proxy

  # 使用自定义代理下载
  bash install.sh --http-server "http://example.com:8080" --token "your_token" --proxy "https://my-proxy.com"

  # 升级程序
  bash install.sh --upgrade

  # 使用代理升级
  bash install.sh --upgrade --proxy

  # 从自定义仓库安装/升级
  bash install.sh --repo "ilnli/komari-monitor-rs" --http-server "http://example.com:8080" --token "your_token"
  bash install.sh --upgrade --repo "ilnli/komari-monitor-rs"

  # 卸载
  bash install.sh --uninstall

一键安装命令 (需替换参数):
  bash <(curl -sL https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/install.sh) \\
    --http-server "http://your.server:port" --token "your_token"

  # 使用自动发现:
  bash <(curl -sL https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/install.sh) \\
    --http-server "http://your.server:port" --auto-discovery "your_ad_key"

  # 从 fork 仓库安装:
  bash <(curl -sL https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/install.sh) \\
    --repo "your-username/komari-monitor-rs" --http-server "http://your.server:port" --token "your_token"

EOF
}

# --- 主程序 ---
main() {
    # --- 参数初始化 ---
    GITHUB_REPO="${DEFAULT_GITHUB_REPO}"
    HTTP_SERVER=""
    WS_SERVER=""
    TOKEN=""
    AUTO_DISCOVERY=""
    FAKE="1"
    INTERVAL="1000"
    BILLING_DAY="1"
    AUTO_UPDATE="0"
    TLS_FLAG=""
    IGNORE_CERT_FLAG=""
    TERMINAL_FLAG=""
    USE_PROXY=""
    PROXY_URL=""
    DO_UNINSTALL=""
    DO_UPGRADE=""

    # 默认代理地址
    DEFAULT_PROXY="https://ghfast.top"

    # --- 解析命令行参数 ---
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --repo) GITHUB_REPO="$2"; shift 2;;
            --http-server) HTTP_SERVER="$2"; shift 2;;
            --ws-server) WS_SERVER="$2"; shift 2;;
            -t|--token) TOKEN="$2"; shift 2;;
            --auto-discovery) AUTO_DISCOVERY="$2"; shift 2;;
            -f|--fake) FAKE="$2"; shift 2;;
            --realtime-info-interval) INTERVAL="$2"; shift 2;;
            --billing-day) BILLING_DAY="$2"; shift 2;;
            --auto-update) AUTO_UPDATE="$2"; shift 2;;
            --tls) TLS_FLAG="--tls"; shift 1;;
            --ignore-unsafe-cert) IGNORE_CERT_FLAG="--ignore-unsafe-cert"; shift 1;;
            --terminal) TERMINAL_FLAG="--terminal"; shift 1;;
            --proxy)
                USE_PROXY="1"
                # 检查下一个参数是否是代理地址（不以 -- 开头）
                if [ -n "$2" ] && [[ ! "$2" =~ ^-- ]]; then
                    PROXY_URL="$2"
                    shift 2
                else
                    shift 1
                fi
                ;;
            --upgrade|--update) DO_UPGRADE="1"; shift 1;;
            --uninstall|--remove) DO_UNINSTALL="1"; shift 1;;
            -h|--help) show_help; exit 0;;
            *) log_warn "未知的参数: $1"; shift 1;;
        esac
    done

    # 检查 root 权限
    check_root

    # 如果是卸载模式
    if [ -n "$DO_UNINSTALL" ]; then
        uninstall
    fi

    # 如果是升级模式
    if [ -n "$DO_UPGRADE" ]; then
        install_dependencies
        upgrade
    fi

    log_info "Komari Monitor RS 安装程序已启动。"
    echo ""

    # --- 交互式询问缺失的必要参数 ---
    # 检查是否使用自动发现模式
    if [ -n "$AUTO_DISCOVERY" ] && [ -n "$HTTP_SERVER" ]; then
        # 自动发现模式，有 HTTP_SERVER 和 AUTO_DISCOVERY，需要调用 API 获取 TOKEN
        log_info "使用自动发现模式"
    elif [ -n "$AUTO_DISCOVERY" ] && [ -z "$HTTP_SERVER" ]; then
        # 提供了 AUTO_DISCOVERY 但没有 HTTP_SERVER
        read -p "请输入主端地址 (例如 http://127.0.0.1:8080): " HTTP_SERVER
    elif [ -z "$HTTP_SERVER" ] && [ -z "$TOKEN" ]; then
        # 询问使用哪种模式
        echo "请选择连接模式:"
        echo "  1) 传统模式 (手动输入 Http 地址和 Token)"
        echo "  2) 自动发现模式 (通过主端自动注册获取 Token)"
        read -p "请选择 [1/2] (默认: 1): " mode_choice
        
        if [ "$mode_choice" = "2" ]; then
            # 自动发现模式
            read -p "请输入主端地址 (例如 http://127.0.0.1:8080): " HTTP_SERVER
            read -p "请输入自动发现密钥: " AUTO_DISCOVERY
        else
            # 传统模式
            read -p "请输入主端 Http 地址 (例如 http://127.0.0.1:8080): " HTTP_SERVER
            read -p "请输入主端 WebSocket 地址 (例如 ws://127.0.0.1:8080，直接回车则自动从 Http 地址推断): " WS_SERVER
            read -p "请输入 Token: " TOKEN
        fi
    else
        # 传统模式补充缺失参数
        if [ -z "$HTTP_SERVER" ]; then
            read -p "请输入主端 Http 地址 (例如 http://127.0.0.1:8080): " HTTP_SERVER
        fi
        if [ -z "$WS_SERVER" ]; then
            read -p "请输入主端 WebSocket 地址 (例如 ws://127.0.0.1:8080，直接回车则自动从 Http 地址推断): " WS_SERVER
        fi
        if [ -z "$TOKEN" ]; then
            read -p "请输入 Token: " TOKEN
        fi
    fi

    # 交互式询问 --terminal (仅当命令行未提供时)
    if [ -z "$TERMINAL_FLAG" ]; then
        read -p "是否启用 Web Terminal 功能? (y/N): " enable_terminal
        enable_terminal_lower=$(echo "$enable_terminal" | tr '[:upper:]' '[:lower:]')
        if [[ "$enable_terminal_lower" == "y" || "$enable_terminal_lower" == "yes" ]]; then
            TERMINAL_FLAG="--terminal"
            log_info "Web Terminal 功能已启用。"
        fi
    fi

    # 交互式询问计费日
    read -p "请输入计费日 (每月第几号开始统计流量，默认为 1，直接回车使用默认值): " input_billing_day
    if [ -n "$input_billing_day" ]; then
        if [[ "$input_billing_day" =~ ^[0-9]+$ ]] && [ "$input_billing_day" -ge 1 ] && [ "$input_billing_day" -le 31 ]; then
            BILLING_DAY="$input_billing_day"
            log_info "计费日已设置为每月 ${BILLING_DAY} 号。"
        else
            log_warn "无效的计费日，使用默认值 1。"
        fi
    fi

    # 验证输入
    if [ -n "$AUTO_DISCOVERY" ]; then
        # 自动发现模式，验证 HTTP_SERVER 和 AUTO_DISCOVERY
        if [ -z "$HTTP_SERVER" ] || [ -z "$AUTO_DISCOVERY" ]; then
            log_error "自动发现模式下，主端地址和自动发现密钥不能为空。"
            exit 1
        fi
    else
        # 传统模式，验证 HTTP_SERVER 和 TOKEN
        if [ -z "$HTTP_SERVER" ] || [ -z "$TOKEN" ]; then
            log_error "Http 地址和 Token 不能为空。"
            exit 1
        fi
    fi

    echo ""
    log_info "配置信息确认:"
    if [ -n "$AUTO_DISCOVERY" ]; then
        echo "  - 模式: 自动发现"
        echo "  - Http Server: $HTTP_SERVER"
        echo "  - 自动发现密钥: ********"
    else
        echo "  - 模式: 传统"
        echo "  - Http Server: $HTTP_SERVER"
        echo "  - WS Server: ${WS_SERVER:-(自动推断)}"
        echo "  - Token: ********"
    fi
    echo "  - 虚假倍率: $FAKE"
    echo "  - 上传间隔: $INTERVAL ms"
    echo "  - 计费日: 每月 $BILLING_DAY 号"
    echo "  - 自动升级: ${AUTO_UPDATE:-0} 小时"
    echo "  - 启用 TLS: ${TLS_FLAG:--}"
    echo "  - 忽略证书: ${IGNORE_CERT_FLAG:--}"
    echo "  - 启用 Terminal: ${TERMINAL_FLAG:--}"
    echo "  - 使用代理: ${USE_PROXY:--}"
    echo "  - 仓库: ${GITHUB_REPO}"
    echo ""

    # --- 安装流程 ---
    log_step "检查依赖..."
    install_dependencies

    ARCH_FILE=$(get_arch)
    
    # 构建下载 URL (默认直连 GitHub，可选代理)
    GITHUB_URL="https://github.com/${GITHUB_REPO}/releases/download/latest/${ARCH_FILE}"
    if [ -n "$USE_PROXY" ]; then
        # 使用自定义代理地址或默认代理
        PROXY="${PROXY_URL:-$DEFAULT_PROXY}"
        # 移除代理地址末尾的斜杠
        PROXY="${PROXY%/}"
        DOWNLOAD_URL="${PROXY}/${GITHUB_URL}"
        log_info "使用代理: ${PROXY}"
    else
        DOWNLOAD_URL="${GITHUB_URL}"
    fi

    log_step "下载程序..."
    log_info "检测到系统架构: $(uname -m)"
    log_info "下载地址: ${DOWNLOAD_URL}"

    if ! download_file "${DOWNLOAD_URL}" "${INSTALL_PATH}"; then
        log_error "下载失败！"
        if [ -z "$USE_PROXY" ]; then
            log_warn "如果您在中国大陆，可以尝试添加 --proxy 参数使用代理下载。"
        fi
        exit 1
    fi

    chmod +x "${INSTALL_PATH}"
    log_info "程序已成功下载并安装到: ${INSTALL_PATH}"

    # --- 如果是自动发现模式，调用 API 获取 token ---
    if [ -n "$AUTO_DISCOVERY" ]; then
        log_step "执行自动发现..."
        AD_TOKEN=$(auto_discover "$HTTP_SERVER" "$AUTO_DISCOVERY")
        if [ $? -ne 0 ]; then
            log_error "自动发现失败，无法继续安装。"
            exit 1
        fi
        TOKEN="$AD_TOKEN"
        log_info "已通过自动发现获取 Token。"
    fi

    # --- 创建 systemd 服务 ---
    log_step "配置 systemd 服务..."

    # 构建启动命令 (始终使用传统模式参数，因为 token 已获取)
    EXEC_START_CMD="${INSTALL_PATH} --http-server \"${HTTP_SERVER}\""
    
    if [ -n "$WS_SERVER" ]; then
        EXEC_START_CMD="$EXEC_START_CMD --ws-server \"${WS_SERVER}\""
    fi
    
    EXEC_START_CMD="$EXEC_START_CMD --token \"${TOKEN}\""
    
    # 通用参数
    EXEC_START_CMD="$EXEC_START_CMD --fake \"${FAKE}\" --realtime-info-interval \"${INTERVAL}\" --billing-day \"${BILLING_DAY}\""
    
    # 自动升级参数
    if [ -n "$AUTO_UPDATE" ] && [ "$AUTO_UPDATE" != "0" ]; then
        EXEC_START_CMD="$EXEC_START_CMD --auto-update \"${AUTO_UPDATE}\" --update-repo \"${GITHUB_REPO}\""
    fi
    
    if [ -n "$TLS_FLAG" ]; then
        EXEC_START_CMD="$EXEC_START_CMD $TLS_FLAG"
    fi
    if [ -n "$IGNORE_CERT_FLAG" ]; then
        EXEC_START_CMD="$EXEC_START_CMD $IGNORE_CERT_FLAG"
    fi
    if [ -n "$TERMINAL_FLAG" ]; then
        EXEC_START_CMD="$EXEC_START_CMD $TERMINAL_FLAG"
    fi

    # 创建服务文件
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

    log_info "服务文件已创建: ${SERVICE_FILE}"

    # --- 启用并启动服务 ---
    log_step "启动服务..."
    systemctl daemon-reload
    systemctl enable ${SERVICE_NAME} >/dev/null 2>&1
    systemctl restart ${SERVICE_NAME}

    # --- 检查服务状态 ---
    sleep 2
    if systemctl is-active --quiet ${SERVICE_NAME}; then
        echo ""
        log_info "=========================================="
        log_info "  安装成功！服务已启动并正在运行"
        log_info "=========================================="
        echo ""
        echo "  常用命令:"
        echo "    查看状态: systemctl status ${SERVICE_NAME}"
        echo "    查看日志: journalctl -u ${SERVICE_NAME} -f"
        echo "    重启服务: systemctl restart ${SERVICE_NAME}"
        echo "    停止服务: systemctl stop ${SERVICE_NAME}"
        echo "    卸载程序: bash install.sh --uninstall"
        echo ""
    else
        log_error "服务启动失败！请检查配置是否正确。"
        log_error "使用以下命令查看详细错误:"
        echo "    systemctl status ${SERVICE_NAME}"
        echo "    journalctl -u ${SERVICE_NAME} -n 50"
        exit 1
    fi
}

# --- 执行主程序 ---
main "$@"