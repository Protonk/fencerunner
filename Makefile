# codex-fence Makefile -- orchestrates probe runs, tests, and metadata checks.
# Targets are intentionally thin wrappers so the harness stays portable.

# Force GNU Make to run every recipe with the baseline shell we test against.
SHELL := /bin/bash
CARGO ?= cargo
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin

PROBE ?=

.PHONY: all probe install build-bin

# Default invocation currently acts as a lightweight reminder of available targets.
all:
	@printf "Available targets: build-bin, install, probe (requires PROBE=<id>).\n"

# Fast loop for a single probe. Requires PROBE=<probe_id_or_path>.
probe:
	@if [[ -z "$(PROBE)" ]]; then \
		echo "Usage: make probe PROBE=<probe_id_or_path>"; \
		exit 1; \
	fi
	tools/validate_contract_gate.sh --probe "$(PROBE)"

build-bin:
# Refresh the repo-local helper binaries built from src/bin/.
	tools/sync_bin_helpers.sh

# Install the CLI + Rust helpers to $(BINDIR), building a release binary first.
install: build-bin
	install -d "$(BINDIR)"
	install -m 755 bin/codex-fence "$(BINDIR)/codex-fence"
