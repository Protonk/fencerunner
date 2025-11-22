#!/usr/bin/env bash
set -euo pipefail

# Synchronize compiled Rust helpers into bin/ so callers can rely on a
# stable location that does not depend on target/{debug,release}. This is a thin
# wrapper around `cargo build --release --bins` with the repository root wired
# into CODEX_FENCE_ROOT_HINT.

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root=$(cd "${script_dir}/.." >/dev/null 2>&1 && pwd)
bin_dir="${repo_root}/bin"

CARGO_BINARIES=(
  codex-fence
  detect-stack
  emit-record
  fence-bang
  fence-listen
  fence-run
  fence-test
  portable-path
  json-extract
)

CARGO=${CARGO:-cargo}
BUILD_FLAGS=(--release --bins)

CODEX_FENCE_ROOT_HINT="${repo_root}" "${CARGO}" build "${BUILD_FLAGS[@]}" "$@"

mkdir -p "${bin_dir}"
for binary in "${CARGO_BINARIES[@]}"; do
  source_path="${repo_root}/target/release/${binary}"
  if [[ ! -x "${source_path}" ]]; then
    echo "sync_bin_helpers: missing ${source_path}. Did the build fail?" >&2
    exit 1
  fi
  install -m 755 "${source_path}" "${bin_dir}/${binary}"
  echo "synced ${binary} -> ${bin_dir}/${binary}"
done

echo "Rust helpers synchronized to ${bin_dir}"
