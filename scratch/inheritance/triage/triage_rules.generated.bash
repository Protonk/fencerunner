#!/usr/bin/env bash
set -euo pipefail

# Generated from rules.json. Do not edit by hand.

triage_timeout_seconds=3

triage_is_quarantined_id() {
  local id="${1:-}"
  case "${id}" in
    check-prerequisites) return 0 ;;
    countdown) return 0 ;;
    get-confirmation) return 0 ;;
    get-terraform-version) return 0 ;;
    rollingback) return 0 ;;
    sudo-librarian-puppet) return 0 ;;
    untag) return 0 ;;
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


  # rule: bash_ge_4_required
  if grep -qiE local:\ -n:\ invalid\ option\|declare:\ -g:\ invalid\ option\|declare:\ -A:\ invalid\ option "${stderr_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
    TRIAGE_MESSAGE=bash\>=4
    triage__emit_unique recommend.install_dependency
  fi

  # rule: terraform_missing
  if grep -qiE Terraform\ is\ not\ installed\|Could\ not\ determine\ terraform\ version "${stderr_file}" 2>/dev/null || grep -qiE Terraform\ is\ not\ installed\|Could\ not\ determine\ terraform\ version "${stdout_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
    TRIAGE_MESSAGE=terraform
    triage__emit_unique recommend.install_dependency
  fi

  # rule: missing_command_in_path
  if grep -qiE is\ not\ in\ your\ command\ path "${stdout_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
    dep="$(awk '/is not in your command path/ {print $1; exit}' "${stdout_file}")"
    if [[ -n "${dep}" ]]; then
      TRIAGE_MESSAGE="${dep}"
    else
      TRIAGE_MESSAGE="missing_dependency"
    fi
    triage__emit_unique recommend.install_dependency
  fi

  # rule: command_not_found
  if grep -qiE command\ not\ found "${stderr_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
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
    triage__emit_unique recommend.install_dependency
  fi

  # rule: env_missing_interpreter
  if grep -qiE env:\ .\*No\ such\ file\ or\ directory "${stderr_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
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
    triage__emit_unique recommend.install_dependency
  fi

  # rule: bad_interpreter_missing
  if grep -qiE bad\ interpreter:.\*\(No\ such\ file\ or\ directory\|no\ such\ file\ or\ directory\) "${stderr_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
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
    triage__emit_unique recommend.install_dependency
  fi

  # rule: dyld_library_not_loaded
  if grep -qiE dyld:\ Library\ not\ loaded:\|Library\ not\ loaded: "${stderr_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
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
    triage__emit_unique recommend.install_dependency
  fi

  # rule: python_module_missing
  if grep -qiE ModuleNotFoundError:\ No\ module\ named\|ImportError:\ No\ module\ named "${stderr_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
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
    triage__emit_unique recommend.install_dependency
  fi

  # rule: node_module_missing
  if grep -qiE Cannot\ find\ module "${stderr_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
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
    triage__emit_unique recommend.install_dependency
  fi

  # rule: ruby_load_error
  if grep -qiE cannot\ load\ such\ file\ -- "${stderr_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
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
    triage__emit_unique recommend.install_dependency
  fi

  # rule: exec_no_such_file_or_directory
  if grep -qiE \^\(bash\|sh\|zsh\):\ .\*:\ \(No\ such\ file\ or\ directory\|no\ such\ file\ or\ directory\)\|:\ line\ \[0-9\]+:\ .\*:\ \(No\ such\ file\ or\ directory\|no\ such\ file\ or\ directory\) "${stderr_file}" 2>/dev/null; then
    TRIAGE_OUTCOME=error
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
    triage__emit_unique recommend.install_dependency
  fi
}

triage_hazards_for_id() {
  local id="${1:-}"
  case "${id}" in
    countdown)
      printf '%s\n' timebox.sleep
      ;;
    get-confirmation)
      printf '%s\n' interactive.read_prompt
      ;;
    rollingback)
      printf '%s\n' timebox.sleep
      ;;
    sudo-librarian-puppet)
      printf '%s\n' privilege.sudo
      ;;
    untag)
      printf '%s\n' git.push git.tag_delete
      ;;
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
      compat.bash4)
        triage__emit_unique recommend.install_dependency
        ;;
      destructive.dd)
        triage__emit_unique recommend.destructive
        ;;
      destructive.disk)
        triage__emit_unique recommend.destructive
        ;;
      destructive.rm_rf)
        triage__emit_unique recommend.destructive
        ;;
      git.clean_fdx)
        triage__emit_unique recommend.destructive
        ;;
      git.push)
        triage__emit_unique recommend.needs_network
        ;;
      git.reset_hard)
        triage__emit_unique recommend.destructive
        ;;
      git.tag_delete)
        triage__emit_unique recommend.destructive
        ;;
      interactive.read_prompt)
        triage__emit_unique recommend.run_interactively
        ;;
      network.curl_wget)
        triage__emit_unique recommend.needs_network
        ;;
      network.ssh_scp)
        triage__emit_unique recommend.needs_network
        ;;
      privilege.sudo)
        triage__emit_unique recommend.needs_privilege
        ;;
      timebox.sleep)
        triage__emit_unique recommend.timeboxed
        ;;
      *) ;;
    esac
  done < <(triage_hazards_for_id "${script_id}")
}

triage_apply_quarantine_overrides() {
  local script_id="${1:-}"
  [[ -n "${script_id}" ]] || return 0
  case "${script_id}" in
    check-prerequisites)
      TRIAGE_MESSAGE=IwillFail
      triage__emit_unique recommend.install_dependency
      ;;
    get-terraform-version)
      TRIAGE_MESSAGE=terraform
      triage__emit_unique recommend.install_dependency
      ;;
    *) ;;
  esac
}

