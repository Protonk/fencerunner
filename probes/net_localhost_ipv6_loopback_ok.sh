#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"

run_mode="${FENCE_RUN_MODE:-baseline}"
probe_name="net_localhost_ipv6_loopback_ok"
probe_version="1"
primary_capability_id="cap_net_localhost_only"
network_disabled_marker="${CODEX_SANDBOX_NETWORK_DISABLED:-}"

python_code=$(cat <<'PY'
import json
import socket
import threading
import urllib.request
import sys

result = {
    "port": None,
    "server": {
        "ipv4": {"bind": "127.0.0.1", "served": False, "error": None},
        "ipv6": {"bind": "::1", "served": False, "error": None},
    },
    "requests": {
        "ipv4": {"url": None, "ok": False, "status": None, "error": None},
        "ipv6": {"url": None, "ok": False, "status": None, "error": None},
    },
}

urllib.request.install_opener(
    urllib.request.build_opener(urllib.request.ProxyHandler({}))
)

last_bind_error = None
ipv4_sock = None
ipv6_sock = None
for _ in range(10):
    try:
        ipv6_sock = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
        ipv6_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            ipv6_sock.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
        except (OSError, AttributeError):
            pass
        ipv6_sock.bind(("::1", 0))
        ipv6_sock.listen(5)
        port = ipv6_sock.getsockname()[1]

        ipv4_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        ipv4_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        ipv4_sock.bind(("127.0.0.1", port))
        ipv4_sock.listen(5)
        result["port"] = port
        break
    except OSError as exc:
        last_bind_error = str(exc)
        if ipv6_sock is not None:
            ipv6_sock.close()
        if ipv4_sock is not None:
            ipv4_sock.close()
        ipv4_sock = None
        ipv6_sock = None
else:
    result["bind_error"] = last_bind_error
    print(json.dumps(result))
    sys.exit(1)

stop_event = threading.Event()


def serve(sock, label):
    info = result["server"][label]
    try:
        sock.settimeout(0.5)
        while not stop_event.is_set():
            try:
                conn, addr = sock.accept()
            except socket.timeout:
                continue
            try:
                conn.settimeout(2)
                _ = conn.recv(4096)
                body = f"{label}-ok".encode()
                response = (
                    b"HTTP/1.1 200 OK\r\n"
                    + f"Content-Length: {len(body)}\r\n".encode()
                    + b"Content-Type: text/plain\r\nConnection: close\r\n\r\n"
                    + body
                )
                conn.sendall(response)
                info["served"] = True
                info["peer"] = addr[0]
                break
            except Exception as exc:  # noqa: PERF203
                info["error"] = f"conn: {exc}"
                break
            finally:
                try:
                    conn.close()
                except Exception:
                    pass
    except Exception as exc:  # noqa: PERF203
        info["error"] = f"accept: {exc}"
    finally:
        try:
            sock.close()
        except Exception:
            pass


threads = []
for label, sock in (("ipv6", ipv6_sock), ("ipv4", ipv4_sock)):
    thread = threading.Thread(target=serve, args=(sock, label), daemon=True)
    thread.start()
    threads.append(thread)

urls = {
    "ipv4": f"http://127.0.0.1:{result['port']}/" if result["port"] else None,
    "ipv6": f"http://[::1]:{result['port']}/" if result["port"] else None,
}
result["requests"]["ipv4"]["url"] = urls["ipv4"]
result["requests"]["ipv6"]["url"] = urls["ipv6"]

def fetch(label, url):
    if not url:
        return False
    info = result["requests"][label]
    try:
        with urllib.request.urlopen(url, timeout=3) as resp:
            body = resp.read().decode(errors="replace")
            info["status"] = resp.status
            info["body_snippet"] = body[:200]
            info["ok"] = resp.status == 200
            return info["ok"]
    except Exception as exc:  # noqa: PERF203
        info["error"] = str(exc)
        return False

ipv4_ok = fetch("ipv4", urls["ipv4"])
ipv6_ok = fetch("ipv6", urls["ipv6"])

stop_event.set()
for thread in threads:
    thread.join(timeout=1)

result["summary"] = {
    "ipv4_success": ipv4_ok,
    "ipv6_success": ipv6_ok,
}
result["requests"]["ipv4"]["ok"] = ipv4_ok
result["requests"]["ipv6"]["ok"] = ipv6_ok

exit_code = 0 if ipv4_ok and ipv6_ok else 1
print(json.dumps(result))
sys.exit(exit_code)
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

raw_json='{}'
if [[ -s "${stdout_tmp}" ]]; then
  raw_json=$(cat "${stdout_tmp}")
fi

ipv4_ok="false"
ipv6_ok="false"
port_value=""
if [[ -s "${stdout_tmp}" ]]; then
  ipv4_ok=$("${repo_root}/bin/json-extract" --stdin --pointer "/requests/ipv4/ok" --type bool --default "false" <"${stdout_tmp}" 2>/dev/null || echo "false")
  ipv6_ok=$("${repo_root}/bin/json-extract" --stdin --pointer "/requests/ipv6/ok" --type bool --default "false" <"${stdout_tmp}" 2>/dev/null || echo "false")
  port_value=$("${repo_root}/bin/json-extract" --stdin --pointer "/port" --type number --default "null" <"${stdout_tmp}" 2>/dev/null || echo "")
fi

lower_all=$(printf '%s\n%s' "${stdout_text}" "${stderr_text}" | tr 'A-Z' 'a-z')
if [[ ${exit_code} -eq 0 ]]; then
  status="success"
  message="IPv4 and IPv6 loopback HTTP requests succeeded"
else
  if [[ "${ipv4_ok}" == "true" || "${ipv6_ok}" == "true" ]]; then
    status="partial"
    message="IPv4 success=${ipv4_ok}, IPv6 success=${ipv6_ok}"
  elif [[ -n "${network_disabled_marker}" ]]; then
    status="denied"
    errno_value="EPERM"
    message="Loopback network disabled via marker"
  elif [[ "${lower_all}" == *"permission denied"* ]] || [[ "${lower_all}" == *"operation not permitted"* ]] || [[ "${lower_all}" == *"network is unreachable"* ]] || [[ "${lower_all}" == *"connection refused"* ]] || [[ "${lower_all}" == *"couldn't connect"* ]] || [[ "${lower_all}" == *"failed to connect"* ]]; then
    status="denied"
    errno_value="EPERM"
    message="Loopback network access denied"
  else
    status="error"
    message="Loopback IPv6 probe failed"
  fi
fi

target_label="loopback_dual_stack"
if [[ -n "${port_value}" ]]; then
  target_label="127.0.0.1:${port_value},[::1]:${port_value}"
fi

ipv4_payload="null"
ipv6_payload="null"
summary_payload="null"
if [[ -s "${stdout_tmp}" ]]; then
  ipv4_payload=$("${repo_root}/bin/json-extract" --stdin --pointer "/requests/ipv4" --type object --default "null" <"${stdout_tmp}" 2>/dev/null || echo "null")
  ipv6_payload=$("${repo_root}/bin/json-extract" --stdin --pointer "/requests/ipv6" --type object --default "null" <"${stdout_tmp}" 2>/dev/null || echo "null")
  summary_payload=$("${repo_root}/bin/json-extract" --stdin --pointer "/summary" --type object --default "null" <"${stdout_tmp}" 2>/dev/null || echo "null")
fi

payload_marker="null"
if [[ -n "${network_disabled_marker}" ]]; then
  payload_marker=$(printf '"%s"' "${network_disabled_marker}")
fi

"${emit_record_bin}" \
  --run-mode "${run_mode}" \
  --probe-name "${probe_name}" \
  --probe-version "${probe_version}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "net" \
  --verb "localhost_dual_stack_probe" \
  --target "${target_label}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-stdout "${stdout_text}" \
  --payload-stderr "${stderr_text}" \
  --payload-raw "${raw_json}" \
  --operation-arg-json "port" "${port_value:-null}" \
  --operation-arg-json "ipv4" "${ipv4_payload}" \
  --operation-arg-json "ipv6" "${ipv6_payload}" \
  --operation-arg-json "summary" "${summary_payload}" \
  --operation-arg-json "network_disabled_marker" "${payload_marker}"
