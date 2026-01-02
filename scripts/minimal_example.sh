#!/bin/bash
# -----------------------------------------------------------------------------
# scripts/minimal_example.sh
#
# A deliberately tiny script that demonstrates the fencerunner script contract.
#
# Contract highlights:
# - Bash strict mode (`set -euo pipefail`).
# - Source the runner-owned script library (required):
#     source "${FENCERUNNER_ROOT}/lib/library.sh"
# - Do exactly one observable thing.
# - Emit exactly one boundary object on stdout (and nothing else).
#
# Notes:
# - After sourcing the library, `commit_help_me` is available as a Bash
#   function. It records enrollments for `emit-record` to serialize into
#   `/context/commitments`.
# -----------------------------------------------------------------------------
set -euo pipefail

# Source the runner-owned script library. This provides:
# - `commit_help_me <ensure|detect|emit> <commitment.id>` (enrollment recording)
source "${FENCERUNNER_ROOT}/lib/library.sh"

# Script identity is the filename stem (<script_id>.sh). The emitted record must
# use the same value for `--script-name`.
script_id="$(basename "${BASH_SOURCE[0]}" .sh)"

# Record the command we intend to run in a human-readable way.
command_executed="true"

# Operation metadata is project-defined. Keep it stable and specific so
# downstream consumers can filter and aggregate reliably.
operation_kind="proc.exec"
target="true"

# One observable action (no output).
true

# This script relies on the runner's boundary object emitter. Enroll in it as an
# explicit authoring-time commitment so it is recorded under /context/commitments.
commit_help_me emit emit.record

# Emit exactly one boundary object and print nothing else to stdout.
# The default `scripts/boundaries.json` requires:
# - `script.id`
# - `result.outcome`
# - `/context/commitments` (empty allowed; this script will include emit.record)
emit-record \
  --script-name "${script_id}" \
  --command "${command_executed}" \
  --operation-kind "${operation_kind}" \
  --target "${target}" \
  --outcome success \
  --exit-code 0 \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw-field "example" "minimal"
