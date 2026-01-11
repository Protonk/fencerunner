#!/bin/bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"
script_id="$(basename "${BASH_SOURCE[0]}" .sh)"
legacy="./${script_id}.legacy"

stdout_file="$(mktemp -t "${script_id}.stdout")"
stderr_file="$(mktemp -t "${script_id}.stderr")"

# This legacy script deletes git tags and may attempt network access. Treat it as
# quarantined by default; only run intentionally outside fencerunner.
msg="quarantined: legacy not executed (destructive git tag ops + possible network)"
printf '%s\n' "${msg}" >"${stderr_file}"

commit_help_me emit emit.record

emit-record \
  --script-name "${script_id}" \
  --command "bash ${legacy}" \
  --operation-kind "legacy.quarantined" \
  --operation-arg-json "quarantined" "true" \
  --target "${legacy}" \
  --outcome partial \
  --exit-code 0 \
  --message "${msg}" \
  --payload-stdout-file "${stdout_file}" \
  --payload-stderr-file "${stderr_file}"
