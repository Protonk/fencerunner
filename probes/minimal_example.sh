#!/bin/bash
# -----------------------------------------------------------------------------
# probes/minimal_example.sh
#
# A deliberately tiny probe that demonstrates the fencerunner probe contract.
#
# Contract highlights:
# - Bash strict mode (`set -euo pipefail`).
# - Source the runner-owned probe library (required):
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

# Source the runner-owned probe library. This provides:
# - `commit_help_me <ensure|detect|emit> <commitment.id>` (enrollment recording)
source "${FENCERUNNER_ROOT}/lib/library.sh"

# Probe identity is the filename stem (<probe_id>.sh). The emitted record must
# use the same value for `--probe-name`.
probe_id="$(basename "${BASH_SOURCE[0]}" .sh)"

# Record the command we intend to run in a human-readable way.
command_executed="true"

# Operation metadata is project-defined. Keep it stable and specific so
# downstream consumers can filter and aggregate reliably.
operation_kind="proc.exec"
target="true"

# One observable action (no output).
true

# This probe relies on the runner's boundary object emitter. Enroll in it as an
# explicit authoring-time commitment so it is recorded under /context/commitments.
commit_help_me emit emit.record

# Emit exactly one boundary object and print nothing else to stdout.
# The default `probes/boundaries.json` requires:
# - `probe.id`
# - `result.outcome`
# - `/context/commitments` (empty allowed; this probe will include emit.record)
emit-record \
  --probe-name "${probe_id}" \
  --command "${command_executed}" \
  --operation-kind "${operation_kind}" \
  --target "${target}" \
  --outcome success \
  --exit-code 0 \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw-field "example" "minimal"
