#!/usr/bin/env bash
set -euo pipefail

# scripts/mutation_test.sh — smoke tests for scripts/mutation.sh's OWN
# contract (#114): the deterministic, scriptable behavior of the recipe
# itself. Covers AC6 (loud failure when cargo-mutants is absent).
#
# Does NOT invoke a real cargo-mutants campaign — whether the tool
# genuinely catches/misses mutants and whether a real run stays
# non-vacuous is empirical, one-shot evidence (see the ticket's AC1/AC2/AC3
# proof), not a repeatable hermetic assertion. This script only checks
# scripts/mutation.sh's own guard logic, in isolation from the real tool.
#
# Run manually: scripts/mutation_test.sh

cd "$(dirname "${BASH_SOURCE[0]}")/.."

failures=0

assert_nonzero_with_message() {
  local description="$1"
  local expected_message="$2"
  shift 2
  local output
  local status=0
  output=$("$@" 2>&1) || status=$?
  if [[ "$status" -eq 0 ]]; then
    echo "FAIL: ${description} -- expected non-zero exit, got 0"
    failures=$((failures + 1))
    return
  fi
  if [[ "$output" != *"$expected_message"* ]]; then
    echo "FAIL: ${description} -- expected message containing '${expected_message}', got: ${output}"
    failures=$((failures + 1))
    return
  fi
  echo "PASS: ${description}"
}

# Hide cargo-mutants from PATH without breaking the interpreter's own need
# for `bash`/`git`/`python3`/`env` — strip ONLY the directory that resolves
# `cargo-mutants` today, keep every other PATH entry intact.
if ! real_cargo_mutants=$(command -v cargo-mutants); then
  echo "SKIP: cargo-mutants is not installed on this machine -- cannot exercise" \
    "the 'already absent' case meaningfully (it would trivially pass for the" \
    "wrong reason). Install it and re-run: cargo install cargo-mutants"
  exit 1
fi
real_bin_dir=$(dirname "${real_cargo_mutants}")

stripped_path=""
IFS=':' read -ra path_parts <<<"${PATH}"
for part in "${path_parts[@]}"; do
  if [[ "${part}" != "${real_bin_dir}" ]]; then
    stripped_path="${stripped_path:+${stripped_path}:}${part}"
  fi
done

assert_nonzero_with_message \
  "refuses to run when cargo-mutants is absent from PATH" \
  "cargo-mutants is not installed" \
  env PATH="${stripped_path}" ./scripts/mutation.sh --full

assert_nonzero_with_message \
  "rejects an unknown argument" \
  "unknown argument" \
  env PATH="${stripped_path}" ./scripts/mutation.sh --bogus-flag

if [[ "${failures}" -gt 0 ]]; then
  echo "${failures} smoke test(s) failed"
  exit 1
fi
echo "all smoke tests passed"
