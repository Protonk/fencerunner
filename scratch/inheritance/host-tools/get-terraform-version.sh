#!/bin/bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"

script_id="$(basename "${BASH_SOURCE[0]}" .sh)"
legacy="./${script_id}.legacy"

triage_rules_path="${FENCERUNNER_RUN_DIR%/*}/triage/triage_rules.generated.bash"
if [[ ! -f "${triage_rules_path}" ]]; then
  echo "wrapper: missing triage rules: ${triage_rules_path}" >&2
  exit 1
fi
source "${triage_rules_path}"

stdout_file="$(mktemp -t "${script_id}.stdout.XXXXXX")"
stderr_file="$(mktemp -t "${script_id}.stderr.XXXXXX")"

commit_help_me ensure policy.read_only
commit_help_me emit emit.record

if triage_is_quarantined_id "${script_id}"; then
  TRIAGE_EMIT_COMMITMENTS=""
  triage__emit_unique "recommend.quarantine"
  triage_apply_hazard_commitments "${script_id}"
  TRIAGE_OPERATION_KIND="legacy.quarantined"
  TRIAGE_OUTCOME="partial"
  TRIAGE_MESSAGE="quarantined"
  while IFS= read -r hazard; do
    [[ -n "${hazard}" ]] || continue
    case "${hazard}" in
      compat.bash4)
        TRIAGE_MESSAGE="bash>=4"
        break
        ;;
      timebox.sleep)
        TRIAGE_MESSAGE="timeboxed"
        ;;
      *) ;;
    esac
  done < <(triage_hazards_for_id "${script_id}")
  triage_apply_quarantine_overrides "${script_id}"
  TRIAGE_EMIT_COMMITMENTS="${TRIAGE_EMIT_COMMITMENTS}"

  printf '%s\n' "${TRIAGE_MESSAGE}" >"${stderr_file}"
  for commitment_id in ${TRIAGE_EMIT_COMMITMENTS}; do
    commit_help_me emit "${commitment_id}"
  done

  emit-record \
    --script-name "${script_id}" \
    --command "bash ${legacy}" \
    --operation-kind "${TRIAGE_OPERATION_KIND}" \
    --target "${legacy}" \
    --outcome "${TRIAGE_OUTCOME}" \
    --exit-code 0 \
    --message "${TRIAGE_MESSAGE}" \
    --payload-stdout-file "${stdout_file}" \
    --payload-stderr-file "${stderr_file}"
  exit 0
fi

# Timebox legacy execution to avoid hangs.
timeout_flag="$(mktemp -t "${script_id}.timed_out.XXXXXX")"
rm -f "${timeout_flag}"

set +e
wrapper_pgid="$(ps -o pgid= -p $$ 2>/dev/null | tr -d ' ')"

set -m
bash "${legacy}" >"${stdout_file}" 2>"${stderr_file}" </dev/null &
pid="$!"
set +m
(
  sleep "${triage_timeout_seconds}"
  if kill -0 "${pid}" 2>/dev/null; then
    echo "timed_out" > "${timeout_flag}"
    child_pgid="$(ps -o pgid= -p "${pid}" 2>/dev/null | tr -d ' ')"

    # Try to kill the child process group when it's distinct from our own,
    # otherwise fall back to killing the leaf pid so we don't self-terminate.
    if [[ -n "${child_pgid}" && -n "${wrapper_pgid}" && "${child_pgid}" != "${wrapper_pgid}" ]]; then
      kill -TERM -- "-${child_pgid}" 2>/dev/null || true
    else
      kill -TERM -- "${pid}" 2>/dev/null || true
    fi

    sleep 1
    if kill -0 "${pid}" 2>/dev/null; then
      if [[ -n "${child_pgid}" && -n "${wrapper_pgid}" && "${child_pgid}" != "${wrapper_pgid}" ]]; then
        kill -KILL -- "-${child_pgid}" 2>/dev/null || true
      else
        kill -KILL -- "${pid}" 2>/dev/null || true
      fi
    fi
  fi
) &
watcher="$!"

wait "${pid}"
exit_code="$?"
kill "${watcher}" 2>/dev/null || true
wait "${watcher}" 2>/dev/null || true
set -e

timed_out=0
if [[ -s "${timeout_flag}" ]]; then
  timed_out=1
  exit_code=124
fi
rm -f "${timeout_flag}"

triage_apply_rules "${script_id}" "${stdout_file}" "${stderr_file}" "${exit_code}" "${timed_out}"

for commitment_id in ${TRIAGE_EMIT_COMMITMENTS}; do
  commit_help_me emit "${commitment_id}"
done

emit_args=(
  --script-name "${script_id}"
  --command "bash ${legacy}"
  --operation-kind "${TRIAGE_OPERATION_KIND}"
  --target "${legacy}"
  --outcome "${TRIAGE_OUTCOME}"
  --exit-code "${exit_code}"
  --payload-stdout-file "${stdout_file}"
  --payload-stderr-file "${stderr_file}"
)
if [[ -n "${TRIAGE_MESSAGE}" ]]; then
  emit_args+=(--message "${TRIAGE_MESSAGE}")
fi

emit-record "${emit_args[@]}"
