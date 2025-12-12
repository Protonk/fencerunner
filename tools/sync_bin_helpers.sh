#!/usr/bin/env bash
set -euo pipefail

# Synchronize compiled Rust helpers into bin/ so callers can rely on a stable
# location that does not depend on target/{debug,release}. This is a thin
# wrapper around `cargo build --release --bins` with the repository root wired
# into FENCE_ROOT. Helper membership is governed by tools/helpers.manifest.json;
# add new helpers there when extending the toolchain.

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)
repo_root=$(cd "${script_dir}/.." >/dev/null 2>&1 && pwd)
bin_dir="${repo_root}/bin"
manifest="${repo_root}/tools/helpers.manifest.json"

CARGO_BINARIES=()
while IFS= read -r name; do
  CARGO_BINARIES+=("${name}")
done < <(python3 - "$manifest" <<'PY'
import json, sys
manifest = json.load(open(sys.argv[1]))
for entry in manifest:
    name = entry.get("name")
    if name:
        print(name)
PY
)

CARGO=${CARGO:-cargo}
BUILD_FLAGS=(--release --bins)

FENCE_ROOT="${repo_root}" "${CARGO}" build "${BUILD_FLAGS[@]}" "$@"

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

# Recreate the probe-contract-gate wrapper so contract tooling is always present.
cat > "${bin_dir}/probe-contract-gate" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
exec "${repo_root}/tools/validate_contract_gate.sh" "$@"
WRAPPER
chmod 755 "${bin_dir}/probe-contract-gate"

# Validate manifest matches bin contents to catch unregistered helpers.
registered="$(python3 - "$manifest" <<'PY'
import json, sys
manifest = json.load(open(sys.argv[1]))
names = sorted([entry.get("name") for entry in manifest if entry.get("name")])
print("\n".join(names))
PY
)"
current="$(find "${bin_dir}" -maxdepth 1 -type f ! -name '.gitkeep' ! -name 'probe-contract-gate' -print0 | xargs -0 -n1 basename | sort)"
if [[ "${registered}" != "${current}" ]]; then
  echo "sync_bin_helpers: helper manifest mismatch" >&2
  echo "  registered: ${registered}" >&2
  echo "  in bin/:    ${current}" >&2
  exit 1
fi
