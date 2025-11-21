#!/usr/bin/env bash
# Shared path helpers for contract tooling. Source this file to populate
# REPO_ROOT plus portable realpath + probe resolution utilities.

if [[ -n "${CODEX_FENCE_RESOLVE_PATHS_SOURCED:-}" ]]; then
  return 0 2>/dev/null || exit 0
fi
CODEX_FENCE_RESOLVE_PATHS_SOURCED=1

if [[ -z "${REPO_ROOT:-}" ]]; then
  REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
fi

portable_path_helper="${REPO_ROOT}/bin/portable-path"
if [[ ! -x "${portable_path_helper}" ]]; then
  echo "tools/resolve_paths: missing portable-path helper at ${portable_path_helper}" >&2
  return 1 2>/dev/null || exit 1
fi

portable_realpath() {
  "${portable_path_helper}" realpath "$1"
}

resolve_probe_script_path() {
  local repo_root="$1"
  local identifier="$2"
  local attempts=() trimmed candidate canonical_path
  if [[ -z "${identifier}" ]]; then
    return 1
  fi
  if [[ "${identifier}" == /* ]]; then
    attempts+=("${identifier}")
  else
    trimmed=${identifier#./}
    attempts+=("${repo_root}/${trimmed}")
    if [[ "${trimmed}" != *.sh ]]; then
      attempts+=("${repo_root}/${trimmed}.sh")
    fi
    attempts+=("${repo_root}/probes/${trimmed}")
    if [[ "${trimmed}" != *.sh ]]; then
      attempts+=("${repo_root}/probes/${trimmed}.sh")
    fi
  fi
  for candidate in "${attempts[@]}"; do
    if [[ -f "${candidate}" ]]; then
      canonical_path=$(portable_realpath "${candidate}")
      if [[ -n "${canonical_path}" && "${canonical_path}" == "${repo_root}/probes/"* ]]; then
        printf '%s\n' "${canonical_path}"
        return 0
      fi
    fi
  done
  return 1
}
