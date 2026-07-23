#!/usr/bin/env bash
set -euo pipefail

# Single source of truth for running coverage locally AND in CI (#87) — the
# `coverage` job in .github/workflows/ci.yml calls this same script instead
# of re-deriving the recipe, so the two never drift apart.
#
# Two defects were traced to the SAME prior recipe (building
# codeimpact-parse-probe INTO cargo-llvm-cov's own target/llvm-cov-target,
# then passing --no-clean to keep it there across cargo-llvm-cov's rebuild):
#
# 1. Misattribution — codeimpact_hexagon source files exercised only via
#    the separate hexagon.unit_test crate (run_analysis.rs, language.rs,
#    parser_registry.rs, alert_thresholds.rs — none has an in-lib
#    `#[test]`) reported 0 lines hit despite their tests passing. Building
#    the probe's dependency graph (which transitively includes
#    codeimpact_hexagon) into the exact directory tree cargo-llvm-cov
#    reuses via --no-clean left a non-instrumented build of that shared
#    crate sitting where the instrumented coverage build expected its own
#    output, so the merged report couldn't attribute the executed
#    (instrumented) counters to it. Confirmed empirically: building the
#    probe into a completely separate tree (plain `target/debug`, below —
#    never touched by cargo-llvm-cov) makes all four files report real,
#    non-zero coverage.
#
# 2. Spurious `Unmeasurable(SourceTooComplex)` on trivial, healthy sources
#    — NOT a stack-rlimit/instrumentation interaction (the probe below is
#    a plain `cargo build`, never itself instrumented). It is wall-clock
#    contention: cargo-llvm-cov's coverage-instrumented build is
#    CPU-heavier than a plain build, and every test that calls
#    SynCodeParser::parse forks+execs the REAL probe subprocess. With the
#    libtest harness's default parallelism (one thread per core), a cold,
#    from-scratch coverage build saturates every core at once, so a
#    perfectly healthy probe child can be scheduled too late and cross the
#    canary's 10s kill-timeout (`PROBE_TIMEOUT`, ADR-0015 §6) for reasons
#    that have nothing to do with the source it was asked to parse.
#    Reproduced on a genuinely cold build; confirmed gone once test-thread
#    parallelism is capped via `--test-threads` below. This leaves
#    `PROBE_TIMEOUT` itself untouched — ADR-0015's production DoS
#    guarantee is not weakened; the fix closes the contention that crosses
#    the margin instead of widening the margin (the same "difference of
#    nature, not a margin" lesson ADR-0010 already draws).
cd "$(dirname "${BASH_SOURCE[0]}")/.."

cargo build -p codeimpact_secondaries --bin codeimpact-parse-probe
export CODEIMPACT_PARSE_PROBE="$PWD/target/debug/codeimpact-parse-probe"

cargo llvm-cov --workspace --lcov --output-path lcov.info -- --test-threads=4
