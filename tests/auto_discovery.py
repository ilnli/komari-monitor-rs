"""Exercise the manager's registration flow against a local Komari HTTP fixture."""

import http.server
import json
import pathlib
import subprocess
import tempfile
import threading
import unittest
import urllib.parse


REPO_DIR = pathlib.Path(__file__).resolve().parent.parent


class RegistrationHandler(http.server.BaseHTTPRequestHandler):
    requests = []

    def do_POST(self):
        url = urllib.parse.urlsplit(self.path)
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        key = body.get("key")
        self.requests.append((url.path, urllib.parse.parse_qs(url.query), key))
        if (
            url.path != "/monitor/api/clients/register"
            or urllib.parse.parse_qs(url.query) != {"name": ["node &中文"]}
            or self.headers.get("Authorization") != f"Bearer {key}"
            or self.headers.get("Content-Type") != "application/json"
        ):
            status, response = 400, {"status": "error", "message": "Invalid request"}
        elif key == "wrong-key-1234":
            status, response = 403, {"status": "error", "message": "Invalid AutoDiscovery Key"}
        elif key == "invalid-response-key":
            status, response = 200, {"status": "success", "data": {"uuid": "node-uuid", "token": ""}}
        else:
            status, response = 200, {"status": "success", "data": {"uuid": "node-uuid", "token": "registered-token"}}
        data = json.dumps(response).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_args):
        pass


class AutoDiscoveryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), RegistrationHandler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.endpoint = f"http://127.0.0.1:{cls.server.server_port}/monitor/"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.service = pathlib.Path(self.temp_dir.name) / "agent.service"
        RegistrationHandler.requests.clear()

    def run_manager(self, commands, inputs):
        # Keep configuration and HTTP real; isolate systemd and executable downloads.
        prelude = '''
set -euo pipefail
source "$1/komari-manager.sh"
SERVICE_FILE="$2/agent.service"
INSTALL_PATH="$2/agent"
NETWORK_STATS_FILE="$2/traffic.conf"
systemctl() { :; }
check_root() { :; }
hostname() { printf '%s' 'node &中文'; }
get_arch() { echo komari-monitor-rs-linux-x86_64-gnu; }
check_service_running() { return 1; }
download_file() {
    if [[ "$1" == */SHA256SUMS ]]; then
        printf '%s  komari-monitor-rs-linux-x86_64-gnu\\n' "$checksum" > "$2"
    else
        cp "${SERVICE_FILE}.download" "$2"
    fi
}
printf '#!/bin/sh\\necho "komari-monitor-rs 0.4.0"\\n' > "${SERVICE_FILE}.download"
checksum=$(sha256sum "${SERVICE_FILE}.download" | cut -d ' ' -f 1)
'''
        return subprocess.run(
            ["bash", "-c", prelude + commands, "manager-test", str(REPO_DIR), self.temp_dir.name],
            input=inputs, text=True, capture_output=True, timeout=15,
        )

    def discovery_inputs(self, key):
        return f"{self.endpoint}\n\n2\n{key}\n15\nn\nn\n"

    def test_registers_and_persists_token_through_config_restart_and_upgrade(self):
        result = self.run_manager("main install\n", self.discovery_inputs("valid-key-1234"))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(len(RegistrationHandler.requests), 1)
        service = self.service.read_text()
        self.assertIn('--token "registered-token"', service)
        self.assertNotIn("valid-key-1234", service + result.stdout + result.stderr)
        self.assertNotIn("--auto-discovery", service)

        result = self.run_manager('''
do_config
do_restart
main upgrade
''', "y\n\n\n1\n\n\nn\nn\nn\n")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('--token "registered-token"', self.service.read_text())
        self.assertEqual(len(RegistrationHandler.requests), 1)

    def test_registration_failure_preserves_existing_service(self):
        original = f'ExecStart=/usr/local/bin/komari-monitor-rs --endpoint "{self.endpoint}" --token "existing-token"\n'
        self.service.write_text(original)
        result = self.run_manager("do_config\n", "y\n\n\n2\nwrong-key-1234\n\nn\nn\n")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(RegistrationHandler.requests), 1)
        self.assertEqual(self.service.read_text(), original)
        self.assertNotIn("wrong-key-1234", result.stdout + result.stderr)

    def test_missing_token_in_response_does_not_create_service(self):
        result = self.run_manager("do_config\n", self.discovery_inputs("invalid-response-key"))
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(RegistrationHandler.requests), 1)
        self.assertFalse(self.service.exists())

    def test_wget_only_install_can_register(self):
        result = self.run_manager('''
command() {
    if [[ "$1" == -v && "$2" == curl ]]; then return 1; fi
    builtin command "$@"
}
do_config
''', self.discovery_inputs("wget-key-1234"))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(len(RegistrationHandler.requests), 1)
        self.assertIn('--token "registered-token"', self.service.read_text())


if __name__ == "__main__":
    unittest.main()
