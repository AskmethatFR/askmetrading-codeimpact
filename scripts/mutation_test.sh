#!/usr/bin/env bash
set -euo pipefail

# scripts/mutation_test.sh — smoke tests for scripts/mutation.sh's OWN
# contract (#114): the deterministic, scriptable behavior of the recipe
# itself. Covers AC6 (loud failure when cargo-mutants is absent) and the
# no-extra-args happy path (a stubbed cargo standing in for the real
# toolchain, so the test stays hermetic and fast).
#
# Does NOT invoke a REAL cargo-mutants campaign — whether the tool
# genuinely catches/misses mutants and whether a real run stays
# non-vacuous is empirical, one-shot evidence (see the ticket's AC1/AC2/AC3
# proof), not a repeatable hermetic assertion.
#
# SANDBOXING: every invocation below runs a COPY of mutation.sh placed in
# its own temp directory (with a symlinked .git so `git diff` still works
# for --diff-mode cases), never the real checkout. mutation.sh resolves its
# own working directory via `cd "$(dirname "${BASH_SOURCE[0]}")/.."`, so
# running the copy makes its `mutants.out`/`.mutation-gate` writes (and the
# `rm -rf` cleanup between cases below) land in the sandbox, never in the
# real repo root — a prior version of this file ran the real script in
# place and `rm -rf`'d a real multi-hour campaign's report this way.
#
# Run manually: scripts/mutation_test.sh

cd "$(dirname "${BASH_SOURCE[0]}")/.."
repo_root="$(pwd)"

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

assert_success_with_message() {
  local description="$1"
  local expected_message="$2"
  shift 2
  local output
  local status=0
  output=$("$@" 2>&1) || status=$?
  if [[ "$status" -ne 0 ]]; then
    echo "FAIL: ${description} -- expected exit 0, got ${status}: ${output}"
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

assert_exact_status_with_message() {
  local description="$1"
  local expected_status="$2"
  local expected_message="$3"
  shift 3
  local output
  local status=0
  output=$("$@" 2>&1) || status=$?
  if [[ "$status" -ne "$expected_status" ]]; then
    echo "FAIL: ${description} -- expected exit ${expected_status}, got ${status}: ${output}"
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

# --- sandbox setup -----------------------------------------------------
# A copy of mutation.sh in its own temp dir, `.git` symlinked back to the
# real repo so `git diff <ref>...` (a pure ref-to-ref comparison, no path
# given) resolves correctly without needing the sandbox's working tree
# files to match: three-dot diffs read only the object database and refs.
cleanup_paths=()
cleanup() {
  local path
  for path in "${cleanup_paths[@]}"; do
    rm -rf "${path}"
  done
}
trap cleanup EXIT

sandbox=$(mktemp -d)
cleanup_paths+=("${sandbox}")
mkdir -p "${sandbox}/scripts"
cp "${repo_root}/scripts/mutation.sh" "${sandbox}/scripts/mutation.sh"
chmod +x "${sandbox}/scripts/mutation.sh"
ln -s "${repo_root}/.git" "${sandbox}/.git"
sandboxed_script="${sandbox}/scripts/mutation.sh"

reset_sandbox_state() {
  rm -rf "${sandbox}/mutants.out" "${sandbox}/.mutation-gate"
}

# --- unified stub `cargo` dispatcher ------------------------------------
# Shadows the REAL `cargo` binary entirely (not just `cargo-mutants`) so
# both `cargo test --workspace --quiet` (the red-suite gate) and
# `cargo mutants ...` are hermetic and instant. Behavior is parameterized
# per-invocation via env vars:
#   CARGO_TEST_EXIT           exit code for the `test` subcommand (default 0)
#   CARGO_TEST_ARGV_LOG       when set, the `test` subcommand's own argv is
#                             appended to this file, one invocation per line
#   CARGO_MUTANTS_EXIT        exit code for the `mutants` subcommand (default 0)
#   CARGO_MUTANTS_JSON        outcomes.json body to write (default: 1 caught)
#   CARGO_MUTANTS_SKIP_WRITE  when set, `mutants` writes nothing (simulates a
#                             vacuous/no-op cargo-mutants run that never
#                             touches mutants.out/outcomes.json)
#   CARGO_MUTANTS_ARGV_LOG    when set, the `mutants` subcommand's argv
#                             (after `--version` short-circuits) is appended
#                             to this file, one invocation per line
stub_dir=$(mktemp -d)
cleanup_paths+=("${stub_dir}")
cat >"${stub_dir}/cargo" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
default_json='{"caught":1,"missed":0,"timeout":0,"unviable":0,"outcomes":[]}'
subcommand="${1:-}"
shift || true
case "${subcommand}" in
  test)
    if [[ -n "${CARGO_TEST_ARGV_LOG:-}" ]]; then
      printf '%s\n' "$*" >>"${CARGO_TEST_ARGV_LOG}"
    fi
    exit "${CARGO_TEST_EXIT:-0}"
    ;;
  mutants)
    if [[ "${1:-}" == "--version" ]]; then
      exit 0
    fi
    if [[ -n "${CARGO_MUTANTS_ARGV_LOG:-}" ]]; then
      printf '%s\n' "$*" >>"${CARGO_MUTANTS_ARGV_LOG}"
    fi
    if [[ -z "${CARGO_MUTANTS_SKIP_WRITE:-}" ]]; then
      mkdir -p mutants.out
      printf '%s' "${CARGO_MUTANTS_JSON:-${default_json}}" >mutants.out/outcomes.json
    fi
    exit "${CARGO_MUTANTS_EXIT:-0}"
    ;;
  *)
    echo "stub cargo: unsupported subcommand '${subcommand}'" >&2
    exit 127
    ;;
esac
STUB
chmod +x "${stub_dir}/cargo"

# --- AC6: absence of the real tool (no stub involved) -------------------
# Hide cargo-mutants from PATH without breaking the interpreter's own need
# for `bash`/`git`/`python3`/`env` — strip ONLY the directory that resolves
# `cargo-mutants` today, keep every other PATH entry intact. Runs with the
# REAL `cargo` (not our stub): `cargo mutants --version` must fail exactly
# as it would for a user who never installed the tool.
if ! real_cargo_mutants=$(command -v cargo-mutants); then
  echo "ERROR: cargo-mutants is not installed on this machine -- cannot exercise" \
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

reset_sandbox_state
assert_nonzero_with_message \
  "refuses to run when cargo-mutants is absent from PATH" \
  "cargo-mutants is not installed" \
  env PATH="${stripped_path}" "${sandboxed_script}" --full

# --- argument parsing (no stub needed, fails before reaching cargo) ------
reset_sandbox_state
assert_nonzero_with_message \
  "rejects an unknown argument" \
  "unknown argument" \
  env PATH="${PATH}" "${sandboxed_script}" --bogus-flag

# --- happy path: empty extra_args under bash 3.2 -------------------------
# `extra_args=()` stays genuinely empty on every call above (no `--` was
# given) -- under bash 3.2 (macOS's shipped /bin/bash), `set -u` +
# `"${extra_args[@]}"` on a zero-element array raises "unbound variable"
# rather than expanding to nothing. This exercises that exact path with
# the stubbed `cargo`, green workspace suite (default), one caught mutant.
reset_sandbox_state
assert_success_with_message \
  "runs the empty-extra-args path (--full, no -- args) without an unbound-variable crash" \
  "1 mutant(s) validly examined" \
  env PATH="${stub_dir}:${PATH}" "${sandboxed_script}" --full

# --- cargo-mutants' own non-zero exit (a real missed mutant) survives ----
# cargo-mutants itself exits non-zero (2) whenever a mutant is MISSED --
# not only on a genuine tool error. Under `set -e`, a naive
# `cargo mutants ...` call with no `||` guard aborts the script on that
# line, so the post-run vacuity assertion (and the "N mutant(s) validly
# examined" message) never runs precisely in the case that matters most:
# a spuriously-vacuous run also reports every mutant "missed" and so also
# exits 2, indistinguishable from this at cargo-mutants' exit-code layer.
# The exact status (not just non-zero) matters: a script that swallowed
# cargo-mutants' 2 and returned a bare 1 would still pass a nonzero-only
# check while breaking the documented "propagates cargo-mutants' own exit
# code" contract.
reset_sandbox_state
assert_exact_status_with_message \
  "still runs the vacuity check, reports examined mutants, and propagates cargo-mutants' exact exit code (2) when it exits non-zero (a real missed mutant)" \
  2 \
  "1 mutant(s) validly examined" \
  env PATH="${stub_dir}:${PATH}" CARGO_MUTANTS_JSON='{"caught":0,"missed":1,"timeout":0,"unviable":0,"outcomes":[]}' CARGO_MUTANTS_EXIT=2 \
  "${sandboxed_script}" --full

reset_sandbox_state

# --- B1: a stale outcomes.json must never launder a vacuous run as green -
# `cargo mutants --in-diff <patch-with-no-Rust>` exits 0 and does NOT
# touch, rotate, or delete mutants.out/ -- so a leftover report from a
# PREVIOUS, genuinely-successful run sits there unchanged. Pre-seed such a
# stale "false success" report (caught: 99), then run against a stub that
# behaves exactly like that vacuous invocation (exits 0, writes nothing).
# The script must NOT report success off the stale file.
mkdir -p "${sandbox}/mutants.out"
printf '%s' '{"caught":99,"missed":0,"timeout":0,"unviable":0,"outcomes":[]}' \
  >"${sandbox}/mutants.out/outcomes.json"
assert_nonzero_with_message \
  "does not launder a stale outcomes.json as success when this run examined nothing" \
  "did it run at all?" \
  env PATH="${stub_dir}:${PATH}" CARGO_MUTANTS_SKIP_WRITE=1 CARGO_MUTANTS_EXIT=0 \
  "${sandboxed_script}" --full

reset_sandbox_state

# --- B2: a red workspace suite must be refused BEFORE any mutation run --
# Measured against real cargo-mutants 27.1.0: `test_workspace = true`
# makes cargo-mutants run `cargo test --workspace` for every MUTANT, but
# its own baseline still runs `cargo test --package=<mutated>` -- so a
# workspace-wide red test (unrelated to any mutant) never fails the
# baseline, and every mutant is then reported "caught" against a suite
# that fails unconditionally. The script must run its own green-suite
# check first and refuse before ever invoking cargo-mutants.
argv_log=$(mktemp)
cleanup_paths+=("${argv_log}")
assert_nonzero_with_message \
  "refuses to mutate when the workspace test suite is red" \
  "workspace" \
  env PATH="${stub_dir}:${PATH}" CARGO_TEST_EXIT=1 CARGO_MUTANTS_ARGV_LOG="${argv_log}" \
  "${sandboxed_script}" --full
if [[ -s "${argv_log}" ]]; then
  echo "FAIL: refuses to mutate when the workspace test suite is red -- cargo-mutants was invoked anyway (argv log is non-empty): $(cat "${argv_log}")"
  failures=$((failures + 1))
else
  echo "PASS: refuses to mutate when the workspace test suite is red -- cargo-mutants was never invoked"
fi

reset_sandbox_state

# --- B4 (Dev-B) / QA: the vacuity refusal ITSELF has no coverage ---------
# The one behavior this ticket exists for (AC3: refuse to report success on
# a run that validly examined zero mutants) had no test -- every prior stub
# wrote caught >= 1. All-unviable is a real shape (e.g. every mutant in the
# diff sits in an already-`#[cfg(test)]`-excluded or otherwise unbuildable
# spot): caught+missed+timeout == 0 even though cargo-mutants itself exits 0.
assert_nonzero_with_message \
  "refuses to report success when caught+missed+timeout == 0 (all unviable)" \
  "refusing to report success on a vacuous run" \
  env PATH="${stub_dir}:${PATH}" CARGO_MUTANTS_JSON='{"caught":0,"missed":0,"timeout":0,"unviable":3,"outcomes":[]}' \
  "${sandboxed_script}" --full

reset_sandbox_state

# --- B4 (Dev-B) / QA: a genuinely missing outcomes.json (fresh, no stale
# report involved -- distinct from the B1 laundering case above, which
# pre-seeds a stale file) must be refused with the "did it run at all?"
# diagnostic.
assert_nonzero_with_message \
  "refuses to report success when cargo-mutants writes no outcomes.json at all" \
  "did it run at all?" \
  env PATH="${stub_dir}:${PATH}" CARGO_MUTANTS_SKIP_WRITE=1 \
  "${sandboxed_script}" --full

reset_sandbox_state

# --- QA / B4: --diff mode had ZERO automated coverage -- every existing
# case used --full or --bogus-flag. This is ordinary deterministic bash
# (build a `git diff`, forward it via `--in-diff`) that can be stubbed
# exactly like --full, per QA's own framing: the ticket's "empirical proof
# instead of tests" carve-out covers "does cargo-mutants discriminate", not
# "does the script build the right command line".

# --diff with no explicit ref: base_ref defaults to origin/main, the patch
# written must match a direct `git diff origin/main...`, and --timeout 300
# (parity with mutation_gate.py's own default) must reach cargo-mutants.
reset_sandbox_state
argv_log=$(mktemp)
cleanup_paths+=("${argv_log}")
expected_patch=$(git -C "${sandbox}" diff --end-of-options origin/main...)
description="--diff with no ref defaults to origin/main, writes the matching patch, forwards --timeout 300"
status=0
output=$(env PATH="${stub_dir}:${PATH}" CARGO_MUTANTS_ARGV_LOG="${argv_log}" "${sandboxed_script}" --diff 2>&1) || status=$?
patch_path="${sandbox}/.mutation-gate/mutation-sh-changes.patch"
if [[ "${status}" -ne 0 ]]; then
  echo "FAIL: ${description} -- expected exit 0, got ${status}: ${output}"
  failures=$((failures + 1))
elif [[ ! -f "${patch_path}" ]]; then
  echo "FAIL: ${description} -- expected patch at ${patch_path}, none written"
  failures=$((failures + 1))
elif [[ "$(cat "${patch_path}")" != "${expected_patch}" ]]; then
  echo "FAIL: ${description} -- patch content diverges from a direct 'git diff origin/main...'"
  failures=$((failures + 1))
elif [[ "$(cat "${argv_log}")" != *"--timeout 300"* ]]; then
  echo "FAIL: ${description} -- expected --timeout 300 in cargo-mutants argv, got: $(cat "${argv_log}")"
  failures=$((failures + 1))
else
  echo "PASS: ${description}"
fi

# --diff with an explicit ref: base_ref is consumed as the second token,
# and the patch reflects THAT ref, not the default.
reset_sandbox_state
argv_log=$(mktemp)
cleanup_paths+=("${argv_log}")
expected_patch=$(git -C "${sandbox}" diff --end-of-options HEAD~2...)
description="--diff HEAD~2 consumes the explicit ref and writes the matching patch"
status=0
output=$(env PATH="${stub_dir}:${PATH}" CARGO_MUTANTS_ARGV_LOG="${argv_log}" "${sandboxed_script}" --diff HEAD~2 2>&1) || status=$?
patch_path="${sandbox}/.mutation-gate/mutation-sh-changes.patch"
if [[ "${status}" -ne 0 ]]; then
  echo "FAIL: ${description} -- expected exit 0, got ${status}: ${output}"
  failures=$((failures + 1))
elif [[ "$(cat "${patch_path}")" != "${expected_patch}" ]]; then
  echo "FAIL: ${description} -- patch content diverges from a direct 'git diff HEAD~2...'"
  failures=$((failures + 1))
else
  echo "PASS: ${description}"
fi

# --diff immediately followed by `--`: the base-ref consumption guard
# (`"$2" != --*`) must NOT swallow "--" as a ref, base_ref stays the
# default, and the extra args after `--` are forwarded to cargo-mutants.
reset_sandbox_state
argv_log=$(mktemp)
cleanup_paths+=("${argv_log}")
expected_patch=$(git -C "${sandbox}" diff --end-of-options origin/main...)
description="--diff -- <extras> does not consume '--' as the ref and forwards the extra args"
status=0
output=$(env PATH="${stub_dir}:${PATH}" CARGO_MUTANTS_ARGV_LOG="${argv_log}" "${sandboxed_script}" --diff -- --alpha --beta 2>&1) || status=$?
patch_path="${sandbox}/.mutation-gate/mutation-sh-changes.patch"
if [[ "${status}" -ne 0 ]]; then
  echo "FAIL: ${description} -- expected exit 0, got ${status}: ${output}"
  failures=$((failures + 1))
elif [[ "$(cat "${patch_path}")" != "${expected_patch}" ]]; then
  echo "FAIL: ${description} -- base_ref was not left at its default (patch diverges from origin/main)"
  failures=$((failures + 1))
elif [[ "$(cat "${argv_log}")" != *"--alpha --beta"* ]]; then
  echo "FAIL: ${description} -- expected '--alpha --beta' forwarded, argv was: $(cat "${argv_log}")"
  failures=$((failures + 1))
else
  echo "PASS: ${description}"
fi

reset_sandbox_state

# --- ALSO FIX: --full and --diff together silently resolved to "last
# flag wins" -- reject the conflicting pair instead, in either order.
assert_nonzero_with_message \
  "rejects --full and --diff together (order: --full --diff)" \
  "mutually exclusive" \
  env PATH="${stub_dir}:${PATH}" "${sandboxed_script}" --full --diff

assert_nonzero_with_message \
  "rejects --full and --diff together (order: --diff --full)" \
  "mutually exclusive" \
  env PATH="${stub_dir}:${PATH}" "${sandboxed_script}" --diff --full

reset_sandbox_state

# --- N1 (#114 retry-2, Dev-B): the red-workspace-suite gate must run
# under a capped test-thread count -- ADR-0025 (docs/ADR-0025-coverage-tooling-recipe.md,
# lines 19, 26) establishes that phantom SourceTooComplex failures in
# syn_code_parser are wall-clock contention starving a probe subprocess
# past its PROBE_TIMEOUT, closed precisely by `--test-threads=4`.
# mutation.sh's own gate re-ran `cargo test --workspace` uncapped,
# reintroducing that exact contention class -- and because
# `test_workspace = true` makes cargo-mutants re-run the full suite for
# EVERY mutant too, a contention flake there reads as "caught": this
# ticket's own false-green class, through another door.
reset_sandbox_state
test_argv_log=$(mktemp)
cleanup_paths+=("${test_argv_log}")
env PATH="${stub_dir}:${PATH}" CARGO_TEST_ARGV_LOG="${test_argv_log}" "${sandboxed_script}" --full >/dev/null 2>&1 || true
description="the red-workspace-suite gate runs with --test-threads=4 (ADR-0025 contention cap)"
if [[ "$(cat "${test_argv_log}")" == *"--test-threads=4"* ]]; then
  echo "PASS: ${description}"
else
  echo "FAIL: ${description} -- expected '--test-threads=4' in 'cargo test' argv, got: $(cat "${test_argv_log}")"
  failures=$((failures + 1))
fi

# --- N2 (#114 retry-2, Dev-B): the header's documented `-- --timeout <n>`
# override contract is false against the real binary -- clap rejects a
# repeated `--timeout` ("cannot be used multiple times"). The script's own
# default must be present when the caller supplies no override, and absent
# (not duplicated) when the caller's own extra args already specify one.
reset_sandbox_state
argv_log=$(mktemp)
cleanup_paths+=("${argv_log}")
env PATH="${stub_dir}:${PATH}" CARGO_MUTANTS_ARGV_LOG="${argv_log}" "${sandboxed_script}" --full >/dev/null 2>&1 || true
description="--full with no override still passes the script's own default --timeout 300"
if [[ "$(cat "${argv_log}")" == *"--timeout 300"* ]]; then
  echo "PASS: ${description}"
else
  echo "FAIL: ${description} -- expected '--timeout 300' in argv, got: $(cat "${argv_log}")"
  failures=$((failures + 1))
fi

reset_sandbox_state
argv_log=$(mktemp)
cleanup_paths+=("${argv_log}")
env PATH="${stub_dir}:${PATH}" CARGO_MUTANTS_ARGV_LOG="${argv_log}" "${sandboxed_script}" --full -- --timeout 60 >/dev/null 2>&1 || true
description="a caller-provided '-- --timeout 60' overrides the default instead of duplicating --timeout"
recorded_argv="$(cat "${argv_log}")"
if [[ "${recorded_argv}" == *"--timeout 60"* && "${recorded_argv}" != *"--timeout 300"* ]]; then
  echo "PASS: ${description}"
else
  echo "FAIL: ${description} -- expected only '--timeout 60' (no '--timeout 300'), got: ${recorded_argv}"
  failures=$((failures + 1))
fi

reset_sandbox_state

# --- N3 (#114 retry-2, Dev-B): the EXIT trap above (`trap cleanup EXIT`)
# is armed while `cleanup_paths` is still empty -- if anything between that
# line and the first `cleanup_paths+=(...)` below fails (e.g. `mktemp -d`
# itself), the trap fires against a zero-element array. Under bash 3.2
# (this host's /bin/bash, `set -u`), "${cleanup_paths[@]}" on an empty
# array raises "unbound variable" instead of expanding to nothing -- the
# exact bug already fixed once in mutation.sh:162 for its own
# extra_args[@]. `declare -f cleanup` extracts THIS file's own, currently
# live `cleanup` function definition (not a hand-written replica) and
# re-runs it in an isolated bash -c subshell with an empty cleanup_paths,
# so this test is tied to the real production function, not a copy of it.
n3_repro_output=$(bash -c "
$(declare -f cleanup)
set -u
cleanup_paths=()
cleanup
" 2>&1) || true
description="cleanup()'s EXIT trap tolerates an empty cleanup_paths under bash 3.2 (set -u)"
if [[ "${n3_repro_output}" == *"unbound variable"* ]]; then
  echo "FAIL: ${description} -- got: ${n3_repro_output}"
  failures=$((failures + 1))
else
  echo "PASS: ${description}"
fi

# --- nit (#114 retry-2, Dev-B): the stale-report rm -f (B1) now runs
# BEFORE the red-workspace-suite gate (B2), not after -- previously a red
# suite refused before ever reaching rm -f, leaving a stale outcomes.json
# on disk (harmless today since mutation_gate.py enforces mtime, but
# untidy). Pre-seed a stale report, force a red suite, and confirm the
# stale file is gone even though cargo-mutants was never invoked.
reset_sandbox_state
mkdir -p "${sandbox}/mutants.out"
printf '%s' '{"caught":99,"missed":0,"timeout":0,"unviable":0,"outcomes":[]}' \
  >"${sandbox}/mutants.out/outcomes.json"
env PATH="${stub_dir}:${PATH}" CARGO_TEST_EXIT=1 "${sandboxed_script}" --full >/dev/null 2>&1 || true
description="a stale outcomes.json is deleted even when the red-workspace-suite gate refuses"
if [[ -f "${sandbox}/mutants.out/outcomes.json" ]]; then
  echo "FAIL: ${description} -- stale report still present"
  failures=$((failures + 1))
else
  echo "PASS: ${description}"
fi

reset_sandbox_state

if [[ "${failures}" -gt 0 ]]; then
  echo "${failures} smoke test(s) failed"
  exit 1
fi
echo "all smoke tests passed"
