#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${REPO_ROOT:-}" ]]; then
  REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." >/dev/null 2>&1 && pwd)
fi

helpers_lib="${REPO_ROOT}/tools/lib/helpers.sh"
# shellcheck source=tools/lib/helpers.sh
source "${helpers_lib}"
