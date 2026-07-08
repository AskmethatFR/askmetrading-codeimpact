# ADR-0004: Economic Impact — Heuristics P0, Profiling Real P2

**Status:** Accepted  
**Date:** 2026-07-08  
**Decided in:** #US2  
**Relations:**  
  supersedes: []  
  depends-on: ["architecture-overview"]  
  prerequisite: ["economic-impact-estimator"]  

## Context

US2 requires estimating economic impact (CPU cost, memory consumption) of analysed code. Two approaches exist:

1. **Real profiling** — run the code, measure actual CPU cycles and memory allocation via OS tools (`perf stat`, `time -v`) or language-specific profilers (ClrMd, JVMTI, V8).
2. **Heuristic estimation** — derive cost from static complexity metrics (cyclomatic complexity, call depth, warnings, loops).

## Decision

**Heuristics for P0, real profiling deferred to P2.**

Rationale:
- **P0 MVP constraint**: no test execution infrastructure yet. US1 (complexity analysis) is static-only. Profiling requires a running process.
- **Zero-cost feedback**: heuristics evaluate in ~0.1ms per file vs 100ms+ for real profiling. Developer gets instant feedback on every `cargo check`.
- **Cross-language without runtime**: heuristics work on any language the parser supports. JVMTI profiler only works on JVM code.
- **ProfilerPort already designed**: the port exists in the hexagon. Heuristics are the first `ProfilerPort` adapter. Real profilers plug in later without changing the domain.

## Consequences

- **Positive**: fast, portable, no runtime dependency, good enough for P0 triage ("is this file worth investigating?").
- **Negative**: estimated costs are not actual costs. A file with low complexity but heavy I/O (e.g., a single `read()` of 1GB) is underestimated.
- **Mitigation**: US5 (I/O-in-loops detection) extends the heuristic with I/O patterns. Real profiling in P2 calibrates the coefficients.

## Recalibration trigger

Heuristic coefficients must be reviewed when the first real profiler adapter is built (P2). Compare heuristic vs profiled costs for 100+ files. If mean absolute error > 50%, recalibrate formulas.

## References

- [[architecture-overview]] — Ports & Adapters table shows ProfilerPort
- [[economic-impact-estimator]] — full formula rationale
- [[ADR-0003]] — no-Stryker decision, same pragmatism about real measurement
