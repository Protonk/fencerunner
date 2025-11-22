#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="net_localhost_loopback"
primary_capability_id="cap_net_localhost_only"

python_code=$(cat <<'PY'
import json
import socket
import threading
import time
import sys

result = {
    "port": None,
    "client_connected": False,
    "error": None,
}

server_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
server_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server_sock.bind(("127.0.0.1", 0))
server_sock.listen(1)
result["port"] = server_sock.getsockname()[1]

def server():
    try:
        conn, _ = server_sock.accept()
        conn.settimeout(2)
        data = conn.recv(64)
        if data:
            conn.sendall(b"ok")
            result["client_connected"] = True
        conn.close()
    except Exception as exc:
        result["error"] = f"server: {exc}"
    finally:
        server_sock.close()

thread = threading.Thread(target=server, daemon=True)
thread.start()

time.sleep(0.1)

try:
    client = socket.create_connection(("127.0.0.1", result["port"]), timeout=2)
    client.sendall(b"ping")
    client.recv(16)
    client.close()
    thread.join(timeout=2)
except Exception as exc:
    result["error"] = f"client: {exc}"
    print(json.dumps(result))
    sys.exit(1)

print(json.dumps(result))
sys.exit(0 if result["client_connected"] else 1)
PY
)

printf -v command_executed "python3 -c %q" "${python_code}"

stdout_tmp=$(mktemp)
stderr_tmp=$(mktemp)
trap 'rm -f "${stdout_tmp}" "${stderr_tmp}"' EXIT

status="error"
errno_value=""
message=""
raw_exit_code=""

set +e
python3 -c "${python_code}" >"${stdout_tmp}" 2>"${stderr_tmp}"
exit_code=$?
set -e

raw_exit_code="${exit_code}"
stdout_text=$(tr -d '\0' <"${stdout_tmp}")
stderr_text=$(tr -d '\0' <"${stderr_tmp}")

if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="Loopback TCP connection succeeded"
else
  lower_err=$(printf '%s' "${stderr_text}" | tr 'A-Z' 'a-z')
  if [[ "${lower_err}" == *"operation not permitted"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Loopback connection blocked by sandbox"
  elif [[ "${lower_err}" == *"permission denied"* ]]; then
    status="denied"
    errno_value="EACCES"
    message="Permission denied creating loopback connection"
  else
    status="error"
    errno_value=""
    message="Loopback connection failed"
  fi
fi

raw_json='{}'
if [[ -s "${stdout_tmp}" ]]; then
  raw_json=$(cat "${stdout_tmp}")
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "net" \
  --verb "connect" \
  --target "127.0.0.1" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw "${raw_json}" \
  --operation-arg "local_bind" "127.0.0.1" \
  --operation-arg "protocol" "tcp"
