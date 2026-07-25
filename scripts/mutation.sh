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
# Exit codes: propagates cargo-mutants' own exit code (0 all caught, 2 one
# or more missed -- see mutants.out/outcomes.json for the breakdown) for
# any run that validly examined >=1 mutant; non-zero (1) when
# cargo-mutants is absent, an argument is unrecognized, or the campaign
# examines ZERO mutants (the vacuous case) -- this last check always runs,
# even when cargo-mutants itself exited non-zero, since a spuriously
# vacuous run (zero tests executed) also reports every mutant "missed"
# and so also exits non-zero, indistinguishable from a genuine miss at
# the exit-code layer alone.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

mode="diff"
base_ref="origin/main"
extra_args=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --diff)
      mode="diff"
      if [[ $# -ge 2 && "$2" != --* ]]; then
        base_ref="$2"
        shift
      fi
      ;;
    --full)
      mode="full"
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

if ! command -v cargo-mutants >/dev/null 2>&1; then
  echo "mutation.sh: cargo-mutants is not installed -- run: cargo install cargo-mutants" >&2
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
mutants_exit=0
if [[ "${mode}" == "diff" ]]; then
  patch_file=".mutation-gate/changes.patch"
  mkdir -p "$(dirname "${patch_file}")"
  git diff --end-of-options "${base_ref}..." >"${patch_file}"
  cargo mutants --in-diff "${patch_file}" "${extra_args[@]+"${extra_args[@]}"}" || mutants_exit=$?
else
  cargo mutants --workspace "${extra_args[@]+"${extra_args[@]}"}" || mutants_exit=$?
fi

outcomes_file="mutants.out/outcomes.json"
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
