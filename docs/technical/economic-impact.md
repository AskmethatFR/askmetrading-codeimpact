# Economic Impact Estimator — Technical Rationale

**id:** economic-impact-estimator  
**type:** technical  
**owner:** Architect  
**status:** Applied  
**decided_in:** #US2  
**relations:**  
  depends-on: ["architecture-overview"]  
  superseded-by: []  

## Overview

`EconomicImpactEstimator` is a domain service that derives CPU cost (μ$) and memory (bytes) from static complexity metrics. It is the P0 implementation of `ProfilerPort` — heuristic-based, zero runtime dependency.

## Heuristic Formulas

### CPU Cost (microdollars)

```
cpu_cost = direct × 0.5 + transitive × 0.3 + max_call_depth × 1.0 + warnings_count × 2.0
```

| Component | Symbol | Coefficient | Unit | Rationale |
|---|---|---|---|---|
| Direct complexity | `direct` | 0.5 | μ$ per decision point | Each `if`/`else`/`case` is ~1 conditional branch (~0.5 μ$ on modern CPU) |
| Transitive complexity | `transitive` | 0.3 | μ$ per transitive decision point | Callee decisions are ~60% of direct cost (call overhead + less local context) |
| Max call depth | `max_call_depth` | 1.0 | μ$ per depth level | Deep call chains incur stack setup/teardown, cache misses. Weighted higher than a single decision point |
| Warnings count | `warnings_count` | 2.0 | μ$ per warning | Each warning flags a known anti-pattern (deep nesting, complex condition). 2× a decision point cost |

**Why 0.5 μ$ per decision point?**  
A modern CPU executes ~10⁹ simple ops/sec at ~10⁻⁵ μ$ per op (cloud pricing: ~$0.10/CPU-hour). A decision point (branch + pipeline stall) is ~50 ops equivalent → 50 × 10⁻⁵ = 0.0005 μ$ = **0.5 μ$**. This is a calibrated order-of-magnitude, not a precise measurement.

**Why warnings at 2.0 μ$?**  
Warnings indicate non-trivial refactoring need. They are not just additional decision points — they represent maintainability debt that compounds over time. The 2.0 coefficient reflects this qualitative penalty.

### Memory Cost (bytes)

```
memory = direct × 100 + hidden_complexity × 200 + functions_with_loops × 1024
```

| Component | Coefficient | Unit | Rationale |
|---|---|---|---|
| Direct complexity | 100 | bytes per decision point | Each decision point contributes ~100 bytes of compiled code (branch tables, jump targets) |
| Hidden complexity (transitive − direct) | 200 | bytes per hidden point | Indirect call sites (vtables, function pointers) double the dispatch footprint |
| Functions with loops | 1024 | bytes per function | Each loop introduces stack-allocated iterator state, loop counter, and ~1KB of loop body provenance |

**Why 1024 bytes per loop function?**  
A loop introduces at minimum: loop counter (8 bytes), iterator/range state (32–64 bytes), stack frame growth (~128 bytes), and approximately 1KB of additional compiled code for the loop body and exit condition. 1024 = 1KB is a conservative estimate.

### Total Cost (microdollars)

```
total_cost = cpu_cost + memory_bytes × 0.0001
```

**Why 0.0001 scaling factor?**  
Memory is cheap relative to CPU in cloud pricing. At ~$0.01/GB-hour (cloud EBS/generic RAM pricing):
- 1 MB allocated for 1 hour = 1 × 10⁻⁶ × 0.01 = 10⁻⁸ μ$ — negligible
- But the relevant cost is **provisioning headroom**: each KB of working set pushes the memory ceiling. At ~$0.10/GB-month for provisioned memory:
  - 1 KB = 1 × 10⁻⁶ GB × $0.10 × 720h/month = 7.2 × 10⁻⁸ μ$ — still tiny

The 0.0001 factor scales bytes to a comparable μ$ order of magnitude:
- 1000 bytes × 0.0001 = 0.1 μ$ (a significant fraction of a 0.5 μ$ decision point)
- 1 MB × 0.0001 = 100 μ$ (dominant — correctly flags memory-heavy code)

This is a **deliberate over-weight** of memory in the total to surface memory problems early. Real profiling in P2 will calibrate the actual ratio.

### Level Thresholds

| Total cost (μ$) | Level | Meaning |
|---|---|---|
| 0–10 | low | Negligible impact. No action needed. |
| 10.01–20 | moderate | Some cost. Worth monitoring. |
| 20.01–40 | high | Significant cost. Investigate on next cycle. |
| 40.01+ | critical | Urgent. Immediate review required. |

**Why mirror complexity thresholds?**  
The same 0–10 / 11–20 / 21–40 / 41+ scheme was already validated in US1 for `CodeMetrics::complexity_level()`. Reusing the same boundaries means:
- Users apply the same mental model: a "high" complexity file is also "high" economic impact.
- The thresholds are conservative: a complexity-10 file (max of "low") yields ~6.5 μ$ total, well inside "low".
- Only files with complexity > 20 (already "high") reach the "high" economic tier.

## Known Limitations

1. **No I/O costing.** A file with a single `read(1GB)` is invisible to the heuristic. **Mitigation:** US5 adds I/O-in-loops detection.
2. **No allocation costing.** Heap allocations (e.g., `Box::new`, `vec![]`) are not counted. **Mitigation:** P2 real profiling captures allocation.
3. **Cloud-pricing assumptions.** 0.5 μ$ per decision point assumes ~$0.10/CPU-hour cloud pricing. Bare-metal or dedicated servers have different economics. **Mitigation:** coefficients are configurable when the ProfilerPort is wired.
4. **Single-threaded model.** No concurrency/parallelism weighting. A parallel sort has the same cost as a serial sort. **Mitigation:** P2 scope.
5. **Language-agnostic by design.** The heuristic ignores language-specific optimizations (JIT compilation, GC pressure). Results are comparable across languages but not precise within any one.
6. **No network cost.** Network I/O (HTTP calls, DB queries) is the most expensive operation in most applications but not detected. **Mitigation:** future feature.

## Recalibration Plan

The heuristic coefficients (0.5, 0.3, 1.0, 2.0, 100, 200, 1024, 0.0001) are **provisional** until real profiling data exists.

### Trigger conditions

Recalibrate when:
1. **First real profiler adapter ships** (P2). Compare heuristic vs profiled for ≥100 files across ≥3 languages.
2. **User reports consistent mis-ranking.** If a file feels "cheap" but the heuristic calls it "high", or vice versa, log the file and adjust.
3. **Cloud pricing shifts significantly.** If the dominant CPU-hour cost changes by >2×, rescale the μ$ base.

### Calibration process

1. Collect profiled data: `{actual_cpu_cycles, actual_memory_bytes}` for 100+ files.
2. Compute heuristic values for the same files.
3. Compute mean absolute percentage error (MAPE) for CPU and memory separately.
4. If MAPE > 50% for either dimension, run least-squares regression to fit new coefficients.
5. Validate the new coefficients on a held-out set of 20 files.
6. Update the formulas and bump the ADR.

### Derivation details for future recalibration

The current coefficients were derived from:
- **CPU base**: 1 CPU-hour at $0.10 (AWS c7g.large) = 3.6 × 10⁶ μ$ / 3.6 × 10¹² cycles ≈ 1 μ$ per 10⁷ cycles.
- **Decision point cost**: ~50 ops (branch + pipeline flush) ≈ 5 × 10⁻⁶ μ$ at 10⁹ ops/s. Scaled to 0.5 μ$ for readability (×10⁵ factor).
- **Memory base**: 1 GB-hour at $0.01 (AWS EBS gp3) = 1.0 × 10⁴ μ$ / 3.6 × 10¹² bytes·s ≈ 2.8 × 10⁻⁹ μ$ per byte-second. Not directly comparable to static allocation — the 0.0001 factor is a heuristic bridge.

## References

- [[ADR-0004]] — decision to use heuristics P0 → profiling P2
- [[architecture-overview]] — Ports & Adapters: ProfilerPort
- Source: `hexagon/src/analysis/economic_impact.rs`
- Tests: `hexagon.unit_test/tests/economic_impact_test.rs`
