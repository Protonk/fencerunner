# codex-fence Makefile -- orchestrates probe runs, tests, and metadata checks.
# Targets are intentionally thin wrappers so the harness stays portable.

# Force GNU Make to run every recipe with the baseline shell we test against.
SHELL := /bin/bash
CARGO ?= cargo
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin

# Resolve the set of probe scripts once and expose handy projections:
# - ALL_PROBE_SCRIPTS: every `probes/*.sh` file on disk
# - PROBES: names requested by the caller (`make PROBES=foo,bar` / space-separated, or default all)
# - PROBE_SCRIPTS: normalized list of scripts we will attempt to run
# - MISSING_PROBES: any requested probe ids without a matching script
ALL_PROBE_SCRIPTS := $(sort $(wildcard probes/*.sh))
comma := ,
PROBES_RAW ?= $(patsubst probes/%.sh,%,$(ALL_PROBE_SCRIPTS))
PROBES := $(strip $(subst $(comma), ,$(PROBES_RAW)))
PROBE_SCRIPTS := $(foreach probe,$(PROBES),$(wildcard probes/$(probe).sh))
MISSING_PROBES := $(filter-out $(patsubst probes/%.sh,%,$(PROBE_SCRIPTS)),$(PROBES))

# Fail fast if the caller names a probe that does not exist—otherwise we would
# silently skip it and produce misleading output.
ifneq ($(strip $(MISSING_PROBES)),)
$(error Missing probe scripts: $(MISSING_PROBES))
endif
OUTDIR := out
PROBE ?=

# Automatically decide which run modes to exercise. If the Codex CLI is on
# PATH we cover sandbox/full modes; otherwise we stay in baseline so the harness
# still works on machines without Codex installed.
HAS_CODEX := $(shell command -v codex >/dev/null 2>&1 && echo yes || true)
ifeq ($(HAS_CODEX),yes)
DEFAULT_MODES := baseline codex-sandbox codex-full
else
DEFAULT_MODES := baseline
endif

# Callers may override MODES, otherwise we use the defaults above.
MODES ?= $(DEFAULT_MODES)

# Compute every `<probe>.<mode>.json` boundary object we need to materialize.
# matrix uses this list to fan out runs and to count the finished records.
MATRIX_TARGETS := $(foreach mode,$(MODES),$(addprefix $(OUTDIR)/,$(addsuffix .$(mode).json,$(PROBES))))

# These targets do not correspond to files on disk.
.PHONY: all matrix clean test validate-capabilities probe install

# Default invocation runs the full probe matrix.
all: matrix

# Run every requested probe across every requested mode, then summarize.
matrix: $(OUTDIR) $(MATRIX_TARGETS)
	@printf "Wrote %s records to %s\n" "$(words $(MATRIX_TARGETS))" "$(OUTDIR)"

# Ensure the output directory exists before probes begin writing JSON records.
$(OUTDIR):
	mkdir -p $@

# Template that wires each probe script to per-mode targets. We lean on explicit
# targets instead of pattern rules so new modes can be added in one place.
define PROBE_template
$(OUTDIR)/$(1).baseline.json: $(2) | $(OUTDIR)
	bin/fence-run baseline $(2) > $$@

$(OUTDIR)/$(1).codex-sandbox.json: $(2) | $(OUTDIR)
	bin/fence-run codex-sandbox $(2) > $$@

$(OUTDIR)/$(1).codex-full.json: $(2) | $(OUTDIR)
	bin/fence-run codex-full $(2) > $$@
endef

# Instantiate the template for every resolved probe script.
$(foreach script,$(PROBE_SCRIPTS), \
  $(eval $(call PROBE_template,$(notdir $(basename $(script))),$(script))) \
)

# Remove all recorded boundary objects—a clean slate for subsequent runs.
clean:
	rm -rf $(OUTDIR)

# Run the full lint + second-tier test suite (see tests/AGENTS.md for details).
test:
	tests/run.sh

# Fast loop for a single probe. Requires PROBE=<probe_id_or_path>.
probe:
	@if [[ -z "$(PROBE)" ]]; then \
		echo "Usage: make probe PROBE=<probe_id_or_path>"; \
		exit 1; \
	fi
	tests/run.sh --probe "$(PROBE)"

# Confirm capability metadata (schema, adapters, fixtures) remain in sync.
validate-capabilities:
	tools/validate_capabilities.sh

# Install the CLI + Rust helpers to $(BINDIR), building a release binary first.
install:
	CODEX_FENCE_ROOT_HINT="$(CURDIR)" $(CARGO) build --release
	install -d "$(BINDIR)"
	install -m 755 bin/codex-fence "$(BINDIR)/codex-fence"
	install -m 755 target/release/fence-bang "$(BINDIR)/fence-bang"
	install -m 755 target/release/fence-listen "$(BINDIR)/fence-listen"
	install -m 755 target/release/fence-test "$(BINDIR)/fence-test"
