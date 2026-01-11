#!/bin/bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"
script_id="$(basename "${BASH_SOURCE[0]}" .sh)"

stdout_file="$(mktemp -t "${script_id}.stdout")"
stderr_file="$(mktemp -t "${script_id}.stderr")"

set +e
bash "./${script_id}.legacy" >"${stdout_file}" 2>"${stderr_file}"
exit_code="$?"
set -e

outcome="success"
if [[ "${exit_code}" -ne 0 ]]; then
  outcome="error"
fi

message=""
if grep -qiE 'Terraform is not installed|Could not determine terraform version' "${stdout_file}" 2>/dev/null \
  || grep -qiE 'Terraform is not installed|Could not determine terraform version' "${stderr_file}" 2>/dev/null; then
  outcome="error"
  message="terraform unavailable (reported by legacy output)"
fi

commit_help_me ensure policy.read_only
commit_help_me emit emit.record

emit-record \
  --script-name "${script_id}" \
  --command "bash ./${script_id}.legacy" \
  --operation-kind "legacy.exec" \
  --target "./${script_id}.legacy" \
  --outcome "${outcome}" \
  --exit-code "${exit_code}" \
  --message "${message}" \
  --payload-stdout-file "${stdout_file}" \
  --payload-stderr-file "${stderr_file}"
