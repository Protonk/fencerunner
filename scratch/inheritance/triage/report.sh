#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scratch/inheritance/triage/report.sh <TURN_DIR>

Consumes TURN_DIR/all.ndjson and produces two stable tables in TURN_DIR:
  items.tsv    One row per boundary record (script run).
  classes.tsv  Grouped counts by class key (+ derived buckets).
EOF
}

turn_dir="${1:-}"
if [[ -z "${turn_dir}" || "${turn_dir}" == "-h" || "${turn_dir}" == "--help" ]]; then
  usage
  exit 2
fi

ndjson="${turn_dir}/all.ndjson"
run_dirs_file="${turn_dir}/run_dirs.txt"

if [[ ! -f "${ndjson}" ]]; then
  echo "error: missing ${ndjson}" >&2
  exit 2
fi
if [[ ! -f "${run_dirs_file}" ]]; then
  echo "error: missing ${run_dirs_file} (expected output from run_all.sh)" >&2
  exit 2
fi

items_out="${turn_dir}/items.tsv"
classes_out="${turn_dir}/classes.tsv"
script_map_tsv="${turn_dir}/script_map.tsv"
script_map_json="${turn_dir}/script_map.json"

: > "${script_map_tsv}"
while IFS= read -r run_dir; do
  [[ -n "${run_dir}" ]] || continue
  for script_path in "${run_dir}"/*.sh; do
    [[ -e "${script_path}" ]] || continue
    script_id="$(basename "${script_path}" .sh)"
    printf '%s\t%s\t%s\n' "${script_id}" "${run_dir}" "${script_path}" >> "${script_map_tsv}"
  done
done < "${run_dirs_file}"

jq -Rn '
  reduce inputs as $line ({}; (
    ($line | split("\t")) as $parts
    | . + {($parts[0]): {run_dir: $parts[1], script_path: $parts[2]}}
  ))
' < "${script_map_tsv}" > "${script_map_json}"

jq -r --slurpfile map "${script_map_json}" -n '
  def run_dir_for($id):
    (.payload.raw.supervised.run_dir // $map[0][$id].run_dir // "");

  def first_line($text):
    ($text // "" | split("\n") | .[0] // "");

  ([
    "run_dir",
    "script.id",
    "operation.kind",
    "result.outcome",
    "exit_code",
    "message",
    "stderr_first_line",
    "commitments"
  ] | @tsv),
  (inputs
    | . as $rec
    | ($rec.script.id // "") as $id
    | [
        run_dir_for($id),
        $id,
        ($rec.operation.kind // ""),
        ($rec.result.outcome // ""),
        ($rec.result.details.exit_code // ""),
        ($rec.result.details.message // ""),
        first_line($rec.payload.stderr_snippet),
        (($rec.context.commitments // []) | map(.id) | join(","))
      ]
    | @tsv
  )
' "${ndjson}" > "${items_out}"

jq -r --slurpfile map "${script_map_json}" -n '
  def run_dir_for($id):
    (.payload.raw.supervised.run_dir // $map[0][$id].run_dir // "");

  def stderr($rec):
    ($rec.payload.stderr_snippet // "");

  def has_commitment($rec; $id):
    (($rec.context.commitments // []) | map(.id) | index($id)) != null;

  def bucket($rec):
    if ($rec.extensions.synthetic? != null) then
      "synthetic"
    elif ($rec.operation.kind == "legacy.quarantined") then
      if (has_commitment($rec; "recommend.destructive")) then
        "quarantined:destructive"
      elif (has_commitment($rec; "recommend.needs_privilege")) then
        "quarantined:needs_privilege"
      elif (has_commitment($rec; "recommend.needs_network")) then
        "quarantined:needs_network"
      elif (has_commitment($rec; "recommend.run_interactively")) then
        "quarantined:run_interactively"
      elif (has_commitment($rec; "recommend.timeboxed")) then
        "quarantined:timeboxed"
      elif (has_commitment($rec; "recommend.install_dependency")) then
        "quarantined:install_dependency"
      else
        "quarantined"
      end
    elif (stderr($rec) | test("local: -n: invalid option|declare: -g: invalid option|declare: -A: invalid option"; "i")) then
      "bash>=4 evidence"
    elif (has_commitment($rec; "recommend.install_dependency")) then
      "missing dependency"
    elif ($rec.result.outcome == "error" and ($rec.result.details.exit_code // 0) == 0) then
      "exit0_error"
    else
      "other"
    end;

  [inputs as $rec | {
    bucket: bucket($rec),
    operation_kind: ($rec.operation.kind // ""),
    outcome: ($rec.result.outcome // ""),
    message: ($rec.result.details.message // ""),
    id: ($rec.script.id // ""),
    run_dir: run_dir_for(($rec.script.id // ""))
  }]
  | map(. + { class_key: (.operation_kind + "|" + .outcome + "|" + .message) })
  | sort_by(.bucket + "|" + .class_key)
  | group_by(.bucket + "|" + .class_key)
  | map({
      count: length,
      bucket: .[0].bucket,
      operation_kind: .[0].operation_kind,
      outcome: .[0].outcome,
      message: .[0].message,
      example_ids: (map(.id) | unique | .[0:5] | join(",")),
      example_run_dirs: (map(.run_dir) | unique | .[0:3] | join(","))
    })
  | sort_by(-.count, .bucket, .operation_kind, .outcome, .message)
  | (["count","bucket","operation.kind","result.outcome","message","example_ids","example_run_dirs"] | @tsv),
    (.[] | [.count, .bucket, .operation_kind, .outcome, .message, .example_ids, .example_run_dirs] | @tsv)
' "${ndjson}" > "${classes_out}"
