#!/bin/bash
#
# Komari Monitor RS 安装/管理脚本
# https://github.com/ilnli/komari-monitor-rs
#

# --- 配置 ---
DEFAULT_GITHUB_REPO="ilnli/komari-monitor-rs"
INSTALL_PATH="/usr/local/bin/komari-monitor-rs"
CONFIG_DIR="/etc/komari-monitor-rs"
CONFIG_PATH="${CONFIG_DIR}/config"
SERVICE_NAME="komari-agent-rs"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
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

# 加载现有配置
load_config() {
    if [ ! -f "${CONFIG_PATH}" ]; then
        return 1
    fi

    HTTP_SERVER=$(grep -E '^\s*http_server\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*http_server\s*=\s*"?([^"#]+)"?.*$/\1/')
    WS_SERVER=$(grep -E '^\s*ws_server\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*ws_server\s*=\s*"?([^"#]+)"?.*$/\1/')
    TOKEN=$(grep -E '^\s*token\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*token\s*=\s*"?([^"#]+)"?.*$/\1/')
    IP_PROVIDER=$(grep -E '^\s*ip_provider\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*ip_provider\s*=\s*"?([^"#]+)"?.*$/\1/')
    TERMINAL_ENABLED=$(grep -E '^\s*terminal\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*terminal\s*=\s*(true|false).*$/\1/')
    TLS_ENABLED=$(grep -E '^\s*tls\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*tls\s*=\s*(true|false).*$/\1/')
    IGNORE_CERT_ENABLED=$(grep -E '^\s*ignore_unsafe_cert\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*ignore_unsafe_cert\s*=\s*(true|false).*$/\1/')
    FAKE=$(grep -E '^\s*fake\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*fake\s*=\s*([0-9.]+).*$/\1/')
    INTERVAL=$(grep -E '^\s*realtime_info_interval\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*realtime_info_interval\s*=\s*([0-9]+).*$/\1/')
    BILLING_DAY=$(grep -E '^\s*billing_day\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*billing_day\s*=\s*([0-9]+).*$/\1/')
    LOG_LEVEL=$(grep -E '^\s*log_level\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*log_level\s*=\s*"?([^"#]+)"?.*$/\1/')
    AUTO_UPDATE=$(grep -E '^\s*auto_update\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*auto_update\s*=\s*([0-9]+).*$/\1/')
    GITHUB_REPO=$(grep -E '^\s*update_repo\s*=' "${CONFIG_PATH}" | sed -E 's/^\s*update_repo\s*=\s*"?([^"#]+)"?.*$/\1/')

    # 设置默认值
    : "${IP_PROVIDER:=ipinfo}"
    : "${TERMINAL_ENABLED:=false}"
    : "${TLS_ENABLED:=false}"
    : "${IGNORE_CERT_ENABLED:=false}"
    : "${FAKE:=1}"
    : "${INTERVAL:=1000}"
    : "${BILLING_DAY:=1}"
    : "${LOG_LEVEL:=info}"
    : "${AUTO_UPDATE:=0}"
    : "${GITHUB_REPO:=${DEFAULT_GITHUB_REPO}}"
}

# 保存配置到文件
save_config() {
    mkdir -p "${CONFIG_DIR}"
    cat > "${CONFIG_PATH}" <<EOF
# Komari Monitor RS 配置文件
# 由安装/管理脚本生成

# 主端地址 (必需)
http_server = "${HTTP_SERVER}"
EOF

    if [ -n "${WS_SERVER}" ]; then
        echo "ws_server = \"${WS_SERVER}\"" >> ${CONFIG_PATH}
    fi

    cat >> ${CONFIG_PATH} <<EOF
token = "${TOKEN}"

# IP 提供商 (ipinfo / cloudflare)
ip_provider = "${IP_PROVIDER}"

# 功能开关
terminal = ${TERMINAL_ENABLED}
tls = ${TLS_ENABLED}
ignore_unsafe_cert = ${IGNORE_CERT_ENABLED}

# 性能设置
fake = ${FAKE}
realtime_info_interval = ${INTERVAL}
billing_day = ${BILLING_DAY}

# 日志等级 (error / warn / info / debug / trace)
log_level = "${LOG_LEVEL}"

# 自动升级 (0 = 禁用，其他数字为检查间隔小时数)
auto_update = ${AUTO_UPDATE}
update_repo = "${GITHUB_REPO}"
EOF

    chmod 600 "${CONFIG_PATH}"
}

# 交互式编辑配置
manage_edit_config() {
    if ! load_config; then
        log_warn "未找到配置文件: ${CONFIG_PATH}"
        read -p "是否创建新的配置文件? (y/N): " create_cfg
        create_cfg=$(echo "$create_cfg" | tr '[:upper:]' '[:lower:]')
        if [[ "$create_cfg" == "y" || "$create_cfg" == "yes" ]]; then
            read -p "请输入主端 Http 地址: " HTTP_SERVER
            read -p "请输入 Token: " TOKEN
            WS_SERVER=""
            IP_PROVIDER="ipinfo"
            TERMINAL_ENABLED="false"
            TLS_ENABLED="false"
            IGNORE_CERT_ENABLED="false"
            FAKE="1"
            INTERVAL="1000"
            BILLING_DAY="1"
            LOG_LEVEL="info"
            AUTO_UPDATE="0"
            GITHUB_REPO="${DEFAULT_GITHUB_REPO}"
        else
            return
        fi
    fi

    while true; do
        echo ""
        echo "当前配置:"
        echo " 1) http_server            = ${HTTP_SERVER}"
        echo " 2) ws_server              = ${WS_SERVER:-(自动推断)}"
        echo " 3) token                  = ********"
        echo " 4) ip_provider            = ${IP_PROVIDER}"
        echo " 5) terminal               = ${TERMINAL_ENABLED}"
        echo " 6) tls                    = ${TLS_ENABLED}"
        echo " 7) ignore_unsafe_cert     = ${IGNORE_CERT_ENABLED}"
        echo " 8) fake                   = ${FAKE}"
        echo " 9) realtime_info_interval = ${INTERVAL}"
        echo "10) billing_day            = ${BILLING_DAY}"
        echo "11) log_level              = ${LOG_LEVEL}"
        echo "12) auto_update            = ${AUTO_UPDATE}"
        echo "13) update_repo            = ${GITHUB_REPO}"
        echo " s) 保存并返回   c) 取消并返回"
        read -p "请选择要修改的项 [1-13/s/c]: " choice
        case "$choice" in
            1) read -p "http_server: " HTTP_SERVER ;;
            2) read -p "ws_server (留空自动推断): " WS_SERVER ;;
            3) read -p "token: " TOKEN ;;
            4) read -p "ip_provider (ipinfo/cloudflare): " IP_PROVIDER ;;
            5) read -p "terminal (true/false): " TERMINAL_ENABLED ;;
            6) read -p "tls (true/false): " TLS_ENABLED ;;
            7) read -p "ignore_unsafe_cert (true/false): " IGNORE_CERT_ENABLED ;;
            8) read -p "fake (整数): " FAKE ;;
            9) read -p "realtime_info_interval (ms): " INTERVAL ;;
           10) read -p "billing_day (1-31): " BILLING_DAY ;;
           11) read -p "log_level (error/warn/info/debug/trace): " LOG_LEVEL ;;
           12) read -p "auto_update (小时, 0=禁用): " AUTO_UPDATE ;;
           13) read -p "update_repo (owner/repo): " GITHUB_REPO ;;
            s|S)
                save_config
                log_info "配置已保存: ${CONFIG_PATH}"
                return
                ;;
            c|C)
                log_warn "已取消修改。"
                return
                ;;
            *) log_warn "无效选择" ;;
        esac
    done
}

# 管理菜单
manage_menu() {
    log_step "进入管理模式"
    while true; do
        echo ""
        echo "管理操作:"
        echo " 1) 编辑配置"
        echo " 2) 重启服务"
        echo " 3) 查看服务状态"
        echo " 4) 实时查看日志"
        echo " 5) 退出"
        read -p "请选择 [1-5]: " m
        case "$m" in
            1) manage_edit_config ;;
            2) systemctl restart ${SERVICE_NAME} && log_info "服务已重启" ;;
            3) systemctl status ${SERVICE_NAME} ;;
            4) journalctl -u ${SERVICE_NAME} -f ;;
            5) return ;;
            *) log_warn "无效选择" ;;
        esac
    done
}

# 检查是否以 Root 用户运行
check_root() {
    if [ "$(id -u)" -ne 0 ]; then
        log_error "此脚本需要 root 权限。请使用 'sudo bash install.sh' 或以 root 用户运行。"
        exit 1
    fi
}

# 安装必要的依赖
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

# 下载文件
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

# 检测系统架构
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

# 卸载
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
    
    # 删除配置文件
    if [ -f "${CONFIG_PATH}" ]; then
        log_info "正在删除配置文件..."
        rm -f "${CONFIG_PATH}"
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

# 升级
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

# 自动发现
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
https://github.com/ilnli/komari-monitor-rs

用法: bash install.sh [选项]

安装选项:
  --http-server <地址>      主端 Http 地址
  -t, --token <token>       认证 Token
  --ws-server <地址>        WebSocket 地址 (可选，自动推断)
  --auto-discovery <key>    自动发现密钥
  --terminal                启用 Web Terminal
  --tls                     启用 TLS
  --ignore-unsafe-cert      忽略证书验证
  --billing-day <1-31>      计费日 (默认: 1)
  --proxy [地址]            使用代理下载

管理选项:
  --manage                  管理模式 (编辑配置/管理服务)
  --upgrade, --update       升级程序
  --uninstall, --remove     卸载程序

示例:
  bash install.sh                                           # 交互式安装
  bash install.sh --http-server "http://x:8080" -t "token"  # 带参数安装
  bash install.sh --manage                                  # 管理配置
  bash install.sh --upgrade                                 # 升级程序

一键安装:
  bash <(curl -sL https://raw.githubusercontent.com/ilnli/komari-monitor-rs/main/install.sh)
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
    IP_PROVIDER="ipinfo"
    LOG_LEVEL="info"
    TLS_ENABLED="false"
    IGNORE_CERT_ENABLED="false"
    TERMINAL_ENABLED="false"
    USE_PROXY=""
    PROXY_URL=""
    DO_UNINSTALL=""
    DO_UPGRADE=""
    DO_MANAGE=""

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
            --tls) TLS_ENABLED="true"; shift 1;;
            --ignore-unsafe-cert) IGNORE_CERT_ENABLED="true"; shift 1;;
            --terminal) TERMINAL_ENABLED="true"; shift 1;;
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
            --manage) DO_MANAGE="1"; shift 1;;
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

    # 如果是管理模式
    if [ -n "$DO_MANAGE" ]; then
        manage_menu
        exit 0
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
    if [ "$TERMINAL_ENABLED" != "true" ]; then
        read -p "是否启用 Web Terminal 功能? (y/N): " enable_terminal
        enable_terminal_lower=$(echo "$enable_terminal" | tr '[:upper:]' '[:lower:]')
        if [[ "$enable_terminal_lower" == "y" || "$enable_terminal_lower" == "yes" ]]; then
            TERMINAL_ENABLED="true"
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
    echo "  - 启用 TLS: ${TLS_ENABLED}"
    echo "  - 忽略证书: ${IGNORE_CERT_ENABLED}"
    echo "  - 启用 Terminal: ${TERMINAL_ENABLED}"
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

    # 创建配置文件
    log_step "生成配置文件..."
    mkdir -p "${CONFIG_DIR}"

    cat > "${CONFIG_PATH}" <<EOF
# Komari Monitor RS 配置文件
# 由安装脚本自动生成

# 主端地址 (必需)
http_server = "${HTTP_SERVER}"
EOF

    if [ -n "$WS_SERVER" ]; then
        echo "ws_server = \"${WS_SERVER}\"" >> "${CONFIG_PATH}"
    fi

    cat >> "${CONFIG_PATH}" <<EOF
token = "${TOKEN}"

# IP 提供商 (ipinfo / cloudflare)
ip_provider = "${IP_PROVIDER}"

# 功能开关
terminal = ${TERMINAL_ENABLED}
tls = ${TLS_ENABLED}
ignore_unsafe_cert = ${IGNORE_CERT_ENABLED}

# 性能设置
fake = ${FAKE}
realtime_info_interval = ${INTERVAL}
billing_day = ${BILLING_DAY}

# 日志等级 (error / warn / info / debug / trace)
log_level = "${LOG_LEVEL}"

# 自动升级 (0 = 禁用，其他数字为检查间隔小时数)
auto_update = ${AUTO_UPDATE}
update_repo = "${GITHUB_REPO}"
EOF

    chmod 600 "${CONFIG_PATH}"
    log_info "配置文件已创建: ${CONFIG_PATH}"

    # 创建 systemd 服务
    log_step "配置 systemd 服务..."

    cat > "${SERVICE_FILE}" <<EOF
[Unit]
Description=Komari Monitor RS Service
After=network.target

[Service]
Type=simple
User=root
ExecStart=${INSTALL_PATH} --config ${CONFIG_PATH}
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
        echo "  配置文件: ${CONFIG_PATH}"
        echo "  程序路径: ${INSTALL_PATH}"
        echo ""
        echo "  常用命令:"
        echo "    查看状态: systemctl status ${SERVICE_NAME}"
        echo "    查看日志: journalctl -u ${SERVICE_NAME} -f"
        echo "    重启服务: systemctl restart ${SERVICE_NAME}"
        echo "    停止服务: systemctl stop ${SERVICE_NAME}"
        echo "    编辑配置: nano ${CONFIG_PATH}"
        echo "    卸载程序: bash install.sh --uninstall"
        echo ""
    else
        log_error "服务启动失败！请检查配置是否正确。"
        log_error "配置文件: ${CONFIG_PATH}"
        log_error "使用以下命令查看详细错误:"
        echo "    systemctl status ${SERVICE_NAME}"
        echo "    journalctl -u ${SERVICE_NAME} -n 50"
        exit 1
    fi
}

# --- 执行主程序 ---
main "$@"
