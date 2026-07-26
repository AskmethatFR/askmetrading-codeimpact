#!/usr/bin/env bash
set -euo pipefail

# scripts/mutation.sh — cargo-mutants recipe for CodeImpact's [[test]]-crate
# layout (#114). Single source of truth for any local or future CI use, in
# the same spirit as scripts/coverage.sh (ADR-0025): one recipe, so a human
# running this locally and any automation that later wraps it can never
# measure something different from each other.
#
# THE TRAP THIS SCRIPT DEFENDS AGAINST: every test lives in a separate
# `[[test]]` binary crate (hexagon.unit_test, secondaries.integration_test,
# primaries.e2e_test), each declaring `[lib] test = false`. Left
# unconfigured, `cargo mutants` mutates codeimpact_hexagon / _secondaries /
# _primaries and by default runs only THAT package's own tests -- which
# range from partial to genuinely empty. The actual fix lives in
# `.cargo/mutants.toml` (`test_workspace = true`), picked up automatically
# by ANY `cargo mutants` invocation from the workspace root -- including
# `mutation_gate.py`'s own direct subprocess call, which never goes through
# this script. This script additionally refuses to run at all when the
# tool itself is missing (AC6), and refuses to report success when a run
# validly examines zero mutants (AC3's belt-and-suspenders local check --
# `mutation_gate.py` has its own independent "empty" verdict for the
# gate-driven path; this one covers a human running this script directly).
#
# Usage:
#   scripts/mutation.sh                     diff-scoped vs origin/main (default)
#   scripts/mutation.sh --diff <base-ref>   diff-scoped vs an explicit ref
#   scripts/mutation.sh --full              whole-workspace campaign (slow)
#   scripts/mutation.sh ... -- <args>       extra args forwarded to `cargo mutants`
#
# --full and --diff are mutually exclusive (rejected in either order, not
# "last flag silently wins"). Every invocation passes `--timeout 300` to
# cargo-mutants (parity with mutation_gate.py's own default) UNLESS the
# caller's own `-- <extra args>` already contains one of the following
# clap-accepted timeout spellings, in which case the script's own default
# is OMITTED entirely -- not appended alongside it: `--timeout <n>`,
# `--timeout=<n>`, `-t <n>`, the attached short form `-t<n>` (e.g. `-t60`),
# or clap's own mutually-exclusive alternative `--timeout-multiplier <n>` /
# `--timeout-multiplier=<n>`. clap rejects a repeated `--timeout` ("cannot
# be used multiple times") and rejects `--timeout` alongside
# `--timeout-multiplier` ("cannot be used with") -- verified against real
# cargo-mutants 27.1.0 for every spelling above -- so each one must
# suppress the default, not just the bare `--timeout`/`-t` forms (#114
# Dev-B N2 found the original header's "just works" claim false for
# `-- --timeout 60`; retry-3's review (NF1/NF2) found the fixed pattern
# still missed the attached `-t60` form and `--timeout-multiplier`
# entirely). No other clap timeout spelling exists today; this list is
# exhaustive against 27.1.0, not a best-effort subset. Diff-mode's patch
# lives at `.mutation-gate/mutation-sh-changes.patch` -- a name distinct
# from mutation_gate.py's own `.mutation-gate/changes.patch` so a
# concurrent manual run and gate run never clobber each other's file.
#
# CALLER-SUPPLIED `--` SEPARATOR IN EXTRAS (#114 retry-3 Dev-B NF3): a
# caller passing their own `--` inside `-- <extra args>` (e.g.
# `mutation.sh --full -- -- --nocapture`) produces
# `cargo test ... -- --nocapture -- --test-threads=4`; libtest reads the
# SECOND `--` as end-of-options, so `--test-threads=4` becomes a test-name
# filter instead of a harness flag and zero tests run -- a mutant this
# script would otherwise catch flips to MISSED. Verified this is
# cargo-mutants' own pre-existing behavior, not a regression introduced by
# this script's `additional_cargo_test_args` (`cargo mutants --no-config
# -- -- --nocapture` already exits 4 the same way). Fails loud (a real
# missed-mutant report, never a false green) -- recorded here so the next
# reader does not rediscover it from scratch. Do not pass a bare `--`
# inside your own extra args.
#
# Exit codes: propagates cargo-mutants' own exit code (0 all caught, 2 one
# or more missed -- see mutants.out/outcomes.json for the breakdown) for
# any run that validly examined >=1 mutant; non-zero (1) when
# cargo-mutants is absent, an argument is unrecognized, or the campaign
# examines ZERO mutants (the vacuous case) -- this last check always runs,
# even when cargo-mutants itself exited non-zero, since a spuriously
# vacuous run (zero tests executed) also reports every mutant "missed"
# and so also exits non-zero, indistinguishable from a genuine miss at
# the exit-code layer alone.
#
# STALE REPORTS (#114 Dev-B B1): a vacuous invocation (e.g. `--in-diff`
# over a patch touching no Rust source) exits 0 WITHOUT touching, rotating
# or deleting mutants.out/ -- so a leftover report from a PREVIOUS,
# genuinely-successful run would still be sitting there. The vacuity check
# below would otherwise read that stale report and launder it as THIS
# run's success. `mutants.out/outcomes.json` is therefore deleted
# immediately at script start -- before the red-workspace-suite gate, the
# cargo-mutants presence check, or the cargo-mutants invocation itself
# (#114 Dev-B retry-2 nit: a red-suite refusal used to leave a stale report
# on disk since rm -f ran only after the gate) -- not detected via an
# mtime comparison (`[[ file -nt marker ]]` is second-granular under
# bash 3.2 and ties on a fast run).
#
# RED WORKSPACE SUITE (#114 Dev-B B2): `test_workspace = true`
# (.cargo/mutants.toml) does NOT make cargo-mutants run
# `cargo test --workspace` for its OWN baseline check -- measured against
# cargo-mutants 27.1.0, the baseline still runs `cargo test --package=<the
# mutated package>` while every mutant afterwards runs the full workspace
# suite. If that workspace suite is red for a reason unrelated to any
# mutant (a pre-existing failing test in a sibling package), cargo-mutants'
# own baseline still passes (it never sees the red test), so EVERY mutant
# is then reported "caught" against a suite that fails unconditionally --
# an all-caught false green. This script therefore runs its own
# `cargo test --workspace` gate before invoking cargo-mutants at all and
# refuses to proceed when it is red. THIS PROTECTION DOES NOT COVER
# `mutation_gate.py`: that tool invokes `cargo mutants` directly as a
# subprocess and never goes through this script, so the exact same
# all-caught false green remains possible on that path. No `cargo-mutants`
# config key closes this gap (tried: `test_workspace` alone,
# `test_workspace` + `--workspace` on the CLI, `test_workspace` +
# `test_package = [...]` -- the baseline stayed package-scoped in all
# three).
#
# GATE CONTENTION (#114 Dev-B retry-2 N1): the gate above ran
# `cargo test --workspace` with no parallelism cap -- ADR-0025
# (docs/ADR-0025-coverage-tooling-recipe.md, lines 19, 26) already
# diagnosed phantom SourceTooComplex failures in syn_code_parser as
# wall-clock contention starving a probe subprocess past its
# PROBE_TIMEOUT, closed precisely by `--test-threads=4` -- this gate was
# ADR-0025's recipe minus that mitigation. Worse than a flaky refusal: once
# `test_workspace = true` re-runs this same uncapped suite for EVERY
# mutant (see .cargo/mutants.toml's `additional_cargo_test_args`), a
# contention flake during a mutant run reads as "caught" -- this ticket's
# own false-green class, through another door. `--test-threads=4` is
# passed to THIS gate's own `cargo test` below; the cap that applies to
# cargo-mutants' per-mutant runs lives in `.cargo/mutants.toml` instead,
# since `cargo mutants -- --test-threads=4` fails the baseline (`--test-threads`
# is a harness argument, not a cargo one -- verified: EXIT=4).

cd "$(dirname "${BASH_SOURCE[0]}")/.."

mode="diff"
base_ref="origin/main"
extra_args=()
diff_flag=""
full_flag=""
default_timeout=300

while [[ $# -gt 0 ]]; do
  case "$1" in
    --diff)
      mode="diff"
      diff_flag=1
      if [[ $# -ge 2 && "$2" != --* ]]; then
        base_ref="$2"
        shift
      fi
      ;;
    --full)
      mode="full"
      full_flag=1
      ;;
    --)
      shift
      extra_args=("$@")
      break
      ;;
    *)
      echo "mutation.sh: unknown argument: $1" >&2
      exit 1
      ;;
  esac
  shift
done

if [[ -n "${diff_flag}" && -n "${full_flag}" ]]; then
  echo "mutation.sh: --full and --diff are mutually exclusive -- pass exactly one" >&2
  exit 1
fi

# cargo-mutants exits non-zero (2) whenever a mutant is MISSED, not only
# on a genuine tool error -- a real, expected outcome we want reported
# (not swallowed), never a reason to abort BEFORE the vacuity check below.
# Captured explicitly so `set -e` does not short-circuit past it: doing so
# would skip the one check that matters most in exactly this case, since
# a spuriously-vacuous run (zero tests executed) also reports every
# mutant "missed" and so ALSO exits 2 -- indistinguishable from a genuine
# missed mutant at the exit-code layer alone.
outcomes_file="mutants.out/outcomes.json"

# A vacuous invocation (e.g. `--in-diff` over a patch with no Rust source
# files) exits 0 and does NOT touch/rotate/delete mutants.out/ -- so a
# leftover report from a PREVIOUS, genuinely-successful run would still be
# sitting here unchanged. Without this, the vacuity check below reads that
# stale report and launders it as THIS run's success (#114 Dev-B B1).
# Deleted here, before EITHER the presence check or the red-suite gate, so
# a refusal on any earlier check never leaves a stale report behind either
# (#114 Dev-B retry-2 nit).
rm -f "${outcomes_file}"

# `cargo mutants --version` (not `command -v cargo-mutants`): cargo itself
# resolves the `mutants` subcommand via PATH *and* $CARGO_HOME/bin, so a
# user with `~/.cargo/bin` off PATH but cargo-mutants installed there would
# get a spurious hard refusal from a PATH-only check (#114 Dev-B "ALSO
# FIX" nit).
if ! cargo mutants --version >/dev/null 2>&1; then
  echo "mutation.sh: cargo-mutants is not installed -- run: cargo install cargo-mutants" >&2
  exit 1
fi

# See "RED WORKSPACE SUITE" and "GATE CONTENTION" above: cargo-mutants'
# own baseline does not exercise the full workspace, so it cannot catch
# this itself, and the cap avoids reintroducing ADR-0025's contention
# flake in this gate's own run.
if ! cargo test --workspace --quiet -- --test-threads=4; then
  echo "mutation.sh: workspace test suite is RED -- a mutation run over a red suite reports every mutant caught (cargo-mutants' baseline check is package-scoped even under test_workspace, see .cargo/mutants.toml). Refusing." >&2
  exit 1
fi

# #114 Dev-B retry-2 N2: clap rejects a repeated `--timeout` ("cannot be
# used multiple times"), so the script's own default must be OMITTED
# entirely whenever the caller's own extra args already specify one --
# not appended alongside it. `-t` is `--timeout`'s short form (see
# `cargo mutants --help`).
has_timeout_override=""
for extra_arg in "${extra_args[@]+"${extra_args[@]}"}"; do
  case "${extra_arg}" in
    --timeout | --timeout=* | -t | -t[0-9]* | --timeout-multiplier | --timeout-multiplier=*)
      has_timeout_override=1
      break
      ;;
  esac
done

timeout_args=()
if [[ -z "${has_timeout_override}" ]]; then
  timeout_args=(--timeout "${default_timeout}")
fi

mutants_exit=0
if [[ "${mode}" == "diff" ]]; then
  # A distinct filename from mutation_gate.py's own `.mutation-gate/changes.patch`
  # (see `diff_patch_path()` in mutation_gate.py) -- concurrent runs of this
  # script and the gate would otherwise clobber each other's patch file.
  patch_file=".mutation-gate/mutation-sh-changes.patch"
  mkdir -p "$(dirname "${patch_file}")"
  git diff --end-of-options "${base_ref}..." >"${patch_file}"
  cargo mutants --in-diff "${patch_file}" "${timeout_args[@]+"${timeout_args[@]}"}" "${extra_args[@]+"${extra_args[@]}"}" || mutants_exit=$?
else
  cargo mutants --workspace "${timeout_args[@]+"${timeout_args[@]}"}" "${extra_args[@]+"${extra_args[@]}"}" || mutants_exit=$?
fi

if [[ ! -f "${outcomes_file}" ]]; then
  echo "mutation.sh: cargo-mutants produced no ${outcomes_file} -- did it run at all?" >&2
  echo "mutation.sh: cargo-mutants exited ${mutants_exit}" >&2
  exit 1
fi

validly_tested=$(python3 -c '
import json, sys
with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)
print(data.get("caught", 0) + data.get("missed", 0) + data.get("timeout", 0))
' "${outcomes_file}")

if [[ "${validly_tested}" -eq 0 ]]; then
  echo "mutation.sh: 0 mutants were validly examined (caught+missed+timeout == 0)" >&2
  echo "mutation.sh: refusing to report success on a vacuous run" >&2
  exit 1
fi

echo "mutation.sh: ${validly_tested} mutant(s) validly examined -- see ${outcomes_file}"
exit "${mutants_exit}"
