#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
# Load functions without invoking the root-only CLI.
source "$repo_dir/komari-manager.sh"
test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT
SERVICE_FILE="$test_dir/agent.service"
INSTALL_PATH="$test_dir/agent"
actions="$test_dir/actions"
systemctl() { echo "$*" >> "$actions"; }

cat > "$SERVICE_FILE" <<'EOF'
ExecStart=/usr/local/bin/komari-monitor-rs --endpoint "https://example.test/monitor" --token "test-token" --interval "1.5" --terminal --disable-web-ssh
EOF
parse_existing_config
[[ "$CURRENT_HTTP_SERVER" == 'https://example.test/monitor' ]]
[[ "$CURRENT_INTERVAL" == 1500 ]]
[[ "$CURRENT_TERMINAL" == false ]]

cat > "$SERVICE_FILE" <<'EOF'
ExecStart=/usr/local/bin/komari-monitor-rs --http-server "http://example.test" --token "test-token" --realtime-info-interval "2000" --terminal
EOF
parse_existing_config
[[ "$CURRENT_HTTP_SERVER" == 'http://example.test' ]]
[[ "$CURRENT_INTERVAL" == 2000 ]]
[[ "$CURRENT_TERMINAL" == true ]]

create_service_file 'https://example.test' '' test-token 1 1000 15 false false false >/dev/null
exec_line=$(sed -n 's/^ExecStart=//p' "$SERVICE_FILE")
[[ "$(get_option_value "$exec_line" network-statistics-mode)" == natural ]]
[[ "$(get_option_value "$exec_line" traffic-period)" == month ]]
[[ "$(get_option_value "$exec_line" traffic-reset-day)" == 15 ]]
create_service_file 'https://example.test' '' test-token 1 1000 15 false false false "$test_dir/existing-traffic.conf" >/dev/null
exec_line=$(sed -n 's/^ExecStart=//p' "$SERVICE_FILE")
[[ "$(get_option_value "$exec_line" network-save-path)" == "$test_dir/existing-traffic.conf" ]]

cat > "$SERVICE_FILE" <<'EOF'
ExecStart=/usr/local/bin/komari-monitor-rs --http-server "https://example.test" --token "test-token" --traffic-reset-day "15"
EOF
migrate_monthly_config >/dev/null
exec_line=$(sed -n 's/^ExecStart=//p' "$SERVICE_FILE")
[[ "$(get_option_value "$exec_line" network-statistics-mode)" == natural ]]
[[ "$(get_option_value "$exec_line" traffic-period)" == month ]]
cp "$SERVICE_FILE" "$test_dir/migrated"
migrate_monthly_config >/dev/null
cmp "$SERVICE_FILE" "$test_dir/migrated"
sed -i 's/--network-statistics-mode natural/--network-statistics-mode fixed/' "$SERVICE_FILE"
migrate_monthly_config >/dev/null
[[ "$(get_option_value "$(cat "$SERVICE_FILE")" network-statistics-mode)" == fixed ]]

uname() { echo armv7l; }
if (get_arch) > /dev/null 2>&1; then
    echo 'Unsupported architecture was accepted' >&2
    exit 1
fi
uname() { echo x86_64; }

install_dependencies() { :; }
check_service_running() { return 0; }
download_file() {
    if [[ "$1" == */SHA256SUMS ]]; then
        printf '%s  komari-monitor-rs-linux-x86_64-gnu\n' "$checksum" > "$2"
    else
        cp "$test_dir/download" "$2"
    fi
}
printf '#!/bin/sh\necho "komari-monitor-rs 0.4.0"\n' > "$test_dir/download"
: > "$actions"
checksum=$(sha256sum "$test_dir/download" | cut -d ' ' -f 1)
do_install true > /dev/null
[[ -x "$INSTALL_PATH" ]]
[[ "$($INSTALL_PATH --version)" == 'komari-monitor-rs 0.4.0' ]]
[[ "$(cat "$actions")" == "stop $SERVICE_NAME" ]]

# A corrupt download must leave the running service and installed file alone.
cp "$INSTALL_PATH" "$test_dir/previous"
: > "$actions"
printf '<html>download error</html>\n' > "$test_dir/download"
if (do_install true) > /dev/null 2>&1; then
    echo 'Corrupt download was accepted' >&2
    exit 1
fi
[[ ! -s "$actions" ]]
cmp "$INSTALL_PATH" "$test_dir/previous"

# Even a matching checksum must not replace a working binary with invalid content.
checksum=$(sha256sum "$test_dir/download" | cut -d ' ' -f 1)
if (do_install true) > /dev/null 2>&1; then
    echo 'Non-executable download was accepted' >&2
    exit 1
fi
[[ ! -s "$actions" ]]
cmp "$INSTALL_PATH" "$test_dir/previous"
echo 'Manager tests passed'
