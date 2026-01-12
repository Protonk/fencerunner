#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scratch/inheritance/triage/gen_wrappers.sh [--rules PATH]

Regenerates every wrapper (*.sh) in scratch/inheritance/* from a single template
plus a generated Bash rules library, so class sweeps are one rules edit + regen.
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"

rules_path="${repo_root}/scratch/inheritance/triage/rules.json"

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --rules)
      rules_path="${2:-}"; shift 2 ;;
    -h|--help)
      usage; exit 0 ;;
    *)
      echo "Unknown argument: ${1}" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ ! -f "${rules_path}" ]]; then
  echo "error: missing rules file: ${rules_path}" >&2
  exit 2
fi

generated_rules="${repo_root}/scratch/inheritance/triage/triage_rules.generated.bash"

timeout_seconds="$(jq -r '.timeout_seconds // 15' "${rules_path}")"
if ! [[ "${timeout_seconds}" =~ ^[0-9]+$ ]] || [[ "${timeout_seconds}" -le 0 ]]; then
  echo "error: invalid timeout_seconds in ${rules_path}: ${timeout_seconds}" >&2
  exit 2
fi

emit_commitments="$(
  jq -r '
    (.rules[].then.emit_commitments[]?),
    (.quarantine_overrides // {} | .[] | .emit_commitments[]?),
    (.hazard_commitments // {} | .[] | .[])
  ' "${rules_path}" 2>/dev/null \
    | sed '/^$/d' \
    | sort -u \
    || true
)"
emit_commitments="$(
  printf '%s\n%s\n' "recommend.quarantine" "${emit_commitments}" \
    | sed '/^$/d' \
    | sort -u
)"
emit_commitments_json="$(printf '%s\n' "${emit_commitments}" | jq -R -s 'split("\n") | map(select(length>0))')"

tmp_rules="$(mktemp -t triage_rules.generated.XXXXXX)"
cat > "${tmp_rules}" <<EOF
#!/usr/bin/env bash
set -euo pipefail

# Generated from ${rules_path##*/}. Do not edit by hand.

triage_timeout_seconds=${timeout_seconds}

triage_is_quarantined_id() {
  local id="\${1:-}"
  case "\${id}" in
EOF

jq -r '.quarantine.script_ids // [] | .[]' "${rules_path}" | while IFS= read -r id; do
  [[ -n "${id}" ]] || continue
  printf '    %s) return 0 ;;\n' "${id}" >> "${tmp_rules}"
done

cat >> "${tmp_rules}" <<'EOF'
    *) return 1 ;;
  esac
}

triage__emit_unique() {
  local id="${1:-}"
  [[ -n "${id}" ]] || return 0
  local existing="${TRIAGE_EMIT_COMMITMENTS:-}"
  case " ${existing} " in
    *" ${id} "*) return 0 ;;
    *) TRIAGE_EMIT_COMMITMENTS="${existing} ${id}" ;;
  esac
}

triage_apply_rules() {
  local script_id="$1"
  local stdout_file="$2"
  local stderr_file="$3"
  local exit_code="$4"
  local timed_out="$5"

  TRIAGE_OPERATION_KIND="legacy.exec"
  TRIAGE_OUTCOME="success"
  TRIAGE_MESSAGE=""
  TRIAGE_EMIT_COMMITMENTS=""

  if [[ "${exit_code}" -ne 0 ]]; then
    TRIAGE_OUTCOME="error"
  fi

  if [[ "${timed_out}" -eq 1 ]]; then
    TRIAGE_OPERATION_KIND="legacy.quarantined"
    TRIAGE_OUTCOME="partial"
    TRIAGE_MESSAGE="timed_out"
    triage__emit_unique "recommend.quarantine"
    return 0
  fi

EOF

rules_len="$(jq -r '.rules | length' "${rules_path}")"
if ! [[ "${rules_len}" =~ ^[0-9]+$ ]]; then
  echo "error: invalid rules array in ${rules_path}" >&2
  exit 2
fi

for ((i=0; i<rules_len; i++)); do
  rule_id="$(jq -r ".rules[${i}].id" "${rules_path}")"
  stderr_re="$(jq -r ".rules[${i}].when.stderr_regex // \"\"" "${rules_path}")"
  stdout_re="$(jq -r ".rules[${i}].when.stdout_regex // \"\"" "${rules_path}")"
  then_outcome="$(jq -r ".rules[${i}].then.outcome // \"\"" "${rules_path}")"
  then_message="$(jq -r ".rules[${i}].then.message // \"\"" "${rules_path}")"
  then_message_from="$(jq -r ".rules[${i}].then.message_from // \"\"" "${rules_path}")"
  emit_list="$(jq -r ".rules[${i}].then.emit_commitments // [] | .[]" "${rules_path}" | tr '\n' ' ')"

  if [[ -z "${stderr_re}" && -z "${stdout_re}" ]]; then
    continue
  fi

  {
    printf '\n  # rule: %s\n' "${rule_id}"
    if [[ -n "${stderr_re}" && -n "${stdout_re}" ]]; then
      printf '  if grep -qiE %s "${stderr_file}" 2>/dev/null || grep -qiE %s "${stdout_file}" 2>/dev/null; then\n' \
        "$(printf '%q' "${stderr_re}")" \
        "$(printf '%q' "${stdout_re}")"
    elif [[ -n "${stderr_re}" ]]; then
      printf '  if grep -qiE %s "${stderr_file}" 2>/dev/null; then\n' "$(printf '%q' "${stderr_re}")"
    else
      printf '  if grep -qiE %s "${stdout_file}" 2>/dev/null; then\n' "$(printf '%q' "${stdout_re}")"
    fi
    if [[ -n "${then_outcome}" ]]; then
      printf '    TRIAGE_OUTCOME=%q\n' "${then_outcome}"
    fi
    if [[ -n "${then_message_from}" ]]; then
      case "${then_message_from}" in
        stdout.command_path)
          cat <<'EOF'
    dep="$(awk '/is not in your command path/ {print $1; exit}' "${stdout_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
EOF
          ;;
        stderr.command_not_found)
          cat <<'EOF'
    dep="$(awk '
      /command not found/ {
        if (match($0, /([^[:space:]:]+): command not found/, m)) {
          print m[1]
          exit
        }
      }
    ' "${stderr_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
EOF
          ;;
        stderr.env_missing)
          cat <<'EOF'
    dep="$(awk '
      /env: .*No such file or directory|env: .*no such file or directory/ {
        if (match($0, /^env: ([^:[:space:]]+): [Nn]o such file or directory/, m)) {
          print m[1]
          exit
        }
      }
    ' "${stderr_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
EOF
          ;;
        stderr.bad_interpreter)
          cat <<'EOF'
    dep="$(awk '
      /bad interpreter/ {
        if (match($0, /: ([^:[:space:]]+): bad interpreter/, m)) {
          interp=m[1]
          gsub(/\r$/, "", interp)
          gsub(/^.*\//, "", interp)
          print interp
          exit
        }
        if (match($0, /bad interpreter: *([^[:space:]]+)/, m)) {
          interp=m[1]
          gsub(/\r$/, "", interp)
          gsub(/^.*\//, "", interp)
          print interp
          exit
        }
      }
    ' "${stderr_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
EOF
          ;;
        stderr.dyld_library_not_loaded)
          cat <<'EOF'
    dep="$(awk '
      /Library not loaded:/ {
        if (match($0, /Library not loaded: ([^[:space:]]+)/, m)) {
          lib=m[1]
          gsub(/\r$/, "", lib)
          gsub(/^.*\//, "", lib)
          print lib
          exit
        }
      }
    ' "${stderr_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
EOF
          ;;
        stderr.python_module_missing)
          cat <<'EOF'
    dep="$(awk '
      /No module named/ {
        if (match($0, /No module named [\"\x27]?([^\"\x27[:space:]]+)[\"\x27]?/, m)) {
          print m[1]
          exit
        }
      }
    ' "${stderr_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
EOF
          ;;
        stderr.node_module_missing)
          cat <<'EOF'
    dep="$(awk '
      /Cannot find module/ {
        if (match($0, /Cannot find module [\"\x27]?([^\"\x27[:space:]]+)[\"\x27]?/, m)) {
          print m[1]
          exit
        }
      }
    ' "${stderr_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
EOF
          ;;
        stderr.ruby_load_error)
          cat <<'EOF'
    dep="$(awk '
      /cannot load such file --/ {
        if (match($0, /cannot load such file -- ([^[:space:]]+)/, m)) {
          file=m[1]
          gsub(/\r$/, "", file)
          gsub(/^.*\//, "", file)
          gsub(/[,;:]$/, "", file)
          print file
          exit
        }
      }
    ' "${stderr_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
EOF
          ;;
        stderr.exec_no_such_file_or_directory)
          cat <<'EOF'
    dep="$(awk '
      /No such file or directory|no such file or directory/ {
        if (match($0, /^(bash|sh|zsh): ([^:]+): [Nn]o such file or directory/, m)) {
          cmd=m[2]
          gsub(/\r$/, "", cmd)
          gsub(/^.*\//, "", cmd)
          print cmd
          exit
        }
        if (match($0, /: line [0-9]+: ([^:]+): [Nn]o such file or directory/, m)) {
          cmd=m[1]
          gsub(/\r$/, "", cmd)
          gsub(/^.*\//, "", cmd)
          print cmd
          exit
        }
      }
    ' "${stderr_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
EOF
          ;;
        *)
          if [[ -n "${then_message}" ]]; then
            printf '    TRIAGE_MESSAGE=%q\n' "${then_message}"
          fi
          ;;
      esac
    elif [[ -n "${then_message}" ]]; then
      printf '    TRIAGE_MESSAGE=%q\n' "${then_message}"
    fi
    for emit_id in ${emit_list}; do
      printf '    triage__emit_unique %q\n' "${emit_id}"
    done
    printf '  fi\n'
  } >> "${tmp_rules}"
done

cat >> "${tmp_rules}" <<'EOF'
}

EOF

hazards_pairs="$(mktemp -t triage_hazards.pairs.XXXXXX)"
if ! "${repo_root}/scratch/inheritance/triage/scan_legacy.sh" \
  --root "${repo_root}/scratch/inheritance" \
  --mode hazards \
  > "${hazards_pairs}.raw"; then
  echo "error: hazard scan failed while generating triage rules" >&2
  exit 2
fi
sed '1d' "${hazards_pairs}.raw" | cut -f1,2 | sort -u > "${hazards_pairs}"

cat >> "${tmp_rules}" <<'EOF'
triage_hazards_for_id() {
  local id="${1:-}"
  case "${id}" in
EOF

while IFS= read -r script_id; do
  [[ -n "${script_id}" ]] || continue
  printf '    %s)\n' "${script_id}" >> "${tmp_rules}"
  printf "      printf '%%s\\\\n'" >> "${tmp_rules}"
  while IFS= read -r hazard; do
    [[ -n "${hazard}" ]] || continue
    printf ' %q' "${hazard}" >> "${tmp_rules}"
  done < <(awk -F$'\t' -v id="${script_id}" '$1 == id {print $2}' "${hazards_pairs}")
  printf '\n      ;;\n' >> "${tmp_rules}"
done < <(cut -f1 "${hazards_pairs}" | sort -u || true)

cat >> "${tmp_rules}" <<'EOF'
    *) return 0 ;;
  esac
}

triage_apply_hazard_commitments() {
  local script_id="${1:-}"
  local hazard=""
  [[ -n "${script_id}" ]] || return 0
  while IFS= read -r hazard; do
    [[ -n "${hazard}" ]] || continue
    case "${hazard}" in
EOF

while IFS=$'\t' read -r hazard_key commitment_id; do
  [[ -n "${hazard_key}" ]] || continue
  [[ -n "${commitment_id}" ]] || continue
  if [[ "${hazard_key}" != "${prev_hazard_key:-}" ]]; then
    if [[ -n "${prev_hazard_key:-}" ]]; then
      printf '        ;;\n' >> "${tmp_rules}"
    fi
    printf '      %s)\n' "${hazard_key}" >> "${tmp_rules}"
    prev_hazard_key="${hazard_key}"
  fi
  printf '        triage__emit_unique %q\n' "${commitment_id}" >> "${tmp_rules}"
done < <(
  jq -r '
    .hazard_commitments // {}
    | to_entries[]
    | .key as $k
    | (.value // [])[]
    | "\($k)\t\(.)"
  ' "${rules_path}" | sort -u
)

if [[ -n "${prev_hazard_key:-}" ]]; then
  printf '        ;;\n' >> "${tmp_rules}"
fi

cat >> "${tmp_rules}" <<'EOF'
      *) ;;
    esac
  done < <(triage_hazards_for_id "${script_id}")
}

EOF

cat >> "${tmp_rules}" <<'EOF'
triage_apply_quarantine_overrides() {
  local script_id="${1:-}"
  [[ -n "${script_id}" ]] || return 0
  case "${script_id}" in
EOF

jq -r '.quarantine_overrides // {} | to_entries[] | "\(.key)\t\(.value.message // "")"' "${rules_path}" \
  | sort -u \
  | while IFS=$'\t' read -r script_id message; do
    [[ -n "${script_id}" ]] || continue
    printf '    %s)\n' "${script_id}" >> "${tmp_rules}"
    if [[ -n "${message}" ]]; then
      printf '      TRIAGE_MESSAGE=%q\n' "${message}" >> "${tmp_rules}"
    fi
    while IFS= read -r commit_id; do
      [[ -n "${commit_id}" ]] || continue
      printf '      triage__emit_unique %q\n' "${commit_id}" >> "${tmp_rules}"
    done < <(jq -r --arg id "${script_id}" '.quarantine_overrides[$id].emit_commitments[]? // empty' "${rules_path}")
    printf '      ;;\n' >> "${tmp_rules}"
  done

cat >> "${tmp_rules}" <<'EOF'
    *) ;;
  esac
}

EOF

rm -f "${hazards_pairs}" "${hazards_pairs}.raw"

mv "${tmp_rules}" "${generated_rules}"

wrapper_template="$(mktemp -t triage_wrapper.XXXXXX)"
cat > "${wrapper_template}" <<'EOF'
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
EOF

# Rewrite wrappers across the corpus.
while IFS= read -r run_dir; do
  [[ -d "${run_dir}" ]] || continue
  if [[ -f "${run_dir}/boundaries.json" && -f "${run_dir}/gates.json" && -f "${run_dir}/commitments.json" ]]; then
    jq -n --argjson ids "${emit_commitments_json}" '
      {
        schema_version: "commitments_v1",
        commitments: ([
          {
            id: "emit.record",
            provider: "runner",
            helps: ["emit"],
            is: "Boundary record emitter",
            at: "emit-record",
            version: "v1"
          },
          {
            id: "policy.read_only",
            provider: "user",
            helps: ["ensure"],
            is: "Run is intended to be non-destructive",
            at: "runbook:triage",
            version: "v1"
          }
        ] + (
          $ids
          | map(select(. != "emit.record" and . != "policy.read_only"))
          | map({
              id: .,
              provider: "user",
              helps: ["emit"],
              is: (if . == "recommend.install_dependency" then
                     "Install a missing dependency"
                   elif . == "recommend.quarantine" then
                     "Quarantine (do not auto-exec)"
                   else
                     "Triage recommendation signal"
                   end),
              at: "runbook:triage",
              version: "v1"
            })
        ))
      }
    ' > "${run_dir}/commitments.json"

    for script_path in "${run_dir}"/*.sh; do
      [[ -e "${script_path}" ]] || continue
      script_id="$(basename "${script_path}" .sh)"
      legacy_path="${run_dir}/${script_id}.legacy"
      if [[ ! -f "${legacy_path}" ]]; then
        mv "${script_path}" "${legacy_path}"
      fi

      cp "${wrapper_template}" "${script_path}"
      chmod +x "${script_path}"
    done
  fi
done < <(find "${repo_root}/scratch/inheritance" -mindepth 1 -maxdepth 1 -type d -print | sort)

rm -f "${wrapper_template}"

echo "generated: ${generated_rules}"
