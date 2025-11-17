#!/usr/bin/env bash

# Helper utilities that probes and tools can source to access portable path
# helpers without duplicating interpreter detection logic. Keep these helpers
# pure (no global state or side effects) so probes remain single-purpose.

extract_probe_var() {
  local file="$1"
  local var="$2"
  local line value trimmed first last value_length
  line=$(grep -E "^[[:space:]]*${var}=" "$file" | head -n1 || true)
  if [[ -z "${line}" ]]; then
    return 1
  fi
  value=${line#*=}
  value=${value%%#*}
  value=$(printf '%s' "${value}" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')
  if [[ -n "${value}" ]]; then
    first=${value:0:1}
    last=${value: -1}
    value_length=${#value}
    if [[ "${first}" == '"' && "${last}" == '"' && ${value_length} -ge 2 ]]; then
      value=${value:1:value_length-2}
    elif [[ "${first}" == "'" && "${last}" == "'" && ${value_length} -ge 2 ]]; then
      value=${value:1:value_length-2}
    fi
  fi
  printf '%s\n' "${value}"
}

portable_realpath() {
  local target="$1"
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$target" <<'PY'
import os
import sys
path = sys.argv[1]
try:
    print(os.path.realpath(path))
except OSError:
    print("")
PY
    return
  fi
  if command -v python >/dev/null 2>&1; then
    python - "$target" <<'PY'
import os
import sys
path = sys.argv[1]
try:
    print(os.path.realpath(path))
except OSError:
    print("")
PY
    return
  fi
  if command -v perl >/dev/null 2>&1; then
    perl -MCwd=abs_path -e 'my $p = shift; my $rp = eval { abs_path($p) }; print defined($rp) ? $rp : ""' "$target"
    return
  fi
  printf ''
}

portable_relpath() {
  local target="$1"
  local base="$2"
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$target" "$base" <<'PY'
import os
import sys
print(os.path.relpath(sys.argv[1], sys.argv[2]))
PY
    return
  fi
  if command -v python >/dev/null 2>&1; then
    python - "$target" "$base" <<'PY'
import os
import sys
print(os.path.relpath(sys.argv[1], sys.argv[2]))
PY
    return
  fi
  if command -v perl >/dev/null 2>&1; then
    perl -MFile::Spec -e 'print File::Spec->abs2rel($ARGV[0], $ARGV[1])' "$target" "$base"
    return
  fi
  printf '%s' "${target}"
}

resolve_probe_script_path() {
  local repo_root="$1"
  local identifier="$2"
  local trimmed attempts=() candidate resolved
  if [[ -z "${identifier}" || -z "${repo_root}" ]]; then
    return 1
  fi
  if [[ "${identifier}" == /* ]]; then
    attempts+=("${identifier}")
  else
    trimmed=${identifier#./}
    attempts+=("${repo_root}/${trimmed}")
    attempts+=("${repo_root}/probes/${trimmed}")
    if [[ "${trimmed}" != *.sh ]]; then
      attempts+=("${repo_root}/probes/${trimmed}.sh")
    fi
  fi
  for candidate in "${attempts[@]}"; do
    if [[ -f "${candidate}" ]]; then
      resolved=$(portable_realpath "${candidate}")
      if [[ -z "${resolved}" ]]; then
        resolved=$(cd "$(dirname "${candidate}")" >/dev/null 2>&1 && pwd)/$(basename "${candidate}")
      fi
      printf '%s\n' "${resolved}"
      return 0
    fi
  done
  return 1
}
