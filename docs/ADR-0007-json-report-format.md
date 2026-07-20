# ADR-0007: JSON Report Format — Output Format & Schema

**Status:** Applied  
**Date:** 2026-07-11 (amended 2026-07-20 — #33/T3: `metric_support` + null-for-unsupported, ADR-7.9)  
**Decided in:** #4, #33  
**Relations:**  
  supersedes: []  
  depends-on: ["architecture-overview", "ADR-0001"]  
  related: ["ADR-0021"]  
  prerequisite: []

## Context

US4 requires JSON output for CI integration. The hexagon is zero-dep (ADR-0001). Two approaches exist:

1. **serde in hexagon** — add `serde`/`serde_json` to the hexagon crate. Breaks ADR-0001.
2. **DTOs in secondaries** — hexagon owns the contract (trait method returning `String`), serialization lives in adapters.

## Decision

**DTOs in secondaries, hexagon owns the contract.**

Eight sub-decisions, all taken together:

| # | Decision | Rationale |
|---|---|---|
| ADR-7.1 | `write_json` returns `String`, not writes to stdout | Hexagon owns contract, adapter owns output. Caller decides destination (stdout, file, pipe). |
| ADR-7.2 | DTOs with `#[derive(Serialize)]` in secondaries only | ADR-0001 (zero-dep hexagon). Hexagon types never carry serde attributes. |
| ADR-7.3 | `OutputFormat` enum in hexagon | Pure enum (`Console`, `Json`). No serde, no deps. Lives in `hexagon/src/analysis/output_format.rs`. |
| ADR-7.4 | `ConsoleReportWriter` implements `write_json` | Both adapters (console + JSON) produce JSON. Console writer uses same DTOs. Ensures format consistency. |
| ADR-7.5 | `handle_json`/`handle_project_json` on `RunAnalysis` | Cleaner than a format parameter on `handle`. Separate methods, separate signatures, no branching. |
| ADR-7.6 | `serde_json::to_string_pretty` for JSON output | Readability for CI logs and human inspection. |
| ADR-7.7 | ISO8601 via std lib, not chrono | `SystemTime::now()` + UNIX_EPOCH formatting. Avoid adding chrono dep. |
| ADR-7.8 | Version from `env!("CARGO_PKG_VERSION")` | Compile-time constant, zero runtime cost. |

### Amendment (#33 / US14-T3) — honest degradation in the JSON contract

Honest degradation ([[ADR-0021]]) reaches the JSON writer. See [[ADR-0021]] D3 for the cross-format decision; the JSON-specific clause:

| # | Decision | Rationale |
|---|---|---|
| ADR-7.9 | **A metric that is `Unsupported` serializes `null` — never `[]`, never `0`** — and the report gains a top-level `metric_support` object mapping each metric to its declared state | A `[]` reads "analyzed, no I/O found"; a `0` reads "free". Both lie when the truth is "not measured for this language". `null` + `metric_support` is the JSON transcription of [[ADR-0010]]'s "`Unmeasurable`, not `0`", now applied to a language's *capability*, not just a run's *measurement*. Concretely: `io_in_loops` and `unclassifiable_io_in_loops_count` serialize `null` when C#'s `io_in_loops` is `Unsupported`. |

**`metric_support` values are English**, `"supported"` / `"degraded: <reason>"` / `"unsupported"` — a stable machine contract, consistent with the rest of the JSON schema, so a CI consumer can distinguish "measured empty" from "not measured". (The Console writer renders the same states in **French** — human output; see [[console-report-enriched]].) Rust output is byte-unchanged: `capabilities: None` / all-`Supported` emits no `metric_support` annotation and no `null` where a value existed before ([[ADR-0021]] D1).

## Consequences

- **Positive**: hexagon remains zero-dep. JSON format is a pluggable concern of the adapter layer. Any new output format (YAML, TOML, HTML) follows the same pattern: new DTOs in secondaries, new `write_*` method on the port trait.
- **Positive**: `OutputFormat` enum in hexagon allows the CLI to select format without leaking serde into the core.
- **Positive**: `handle_json`/`handle_project_json` are independently testable from `handle`/`handle_project`.
- **Cost**: DTO duplication — secondaries must mirror hexagon domain types. Mitigated by thin DTOs with `From` impls.
- **Negative**: `write_json` returns `String` — large files may allocate significant strings. Mitigated by streaming serialization in a future P1 if needed.

## Constraints

- **MUST**: `write_json` signature: `(&self, metrics: &CodeMetrics, target: &str, target_type: &str) -> Result<String, AnalysisError>`.
- **MUST**: `OutputFormat` enum is pure — no `#[derive(Serialize)]`, no serde import.
- **MUST**: `JsonReportWriter.write_project_report` and `write_stress_test` return `Err(NotImplemented)` — project report and stress test JSON output are deferred.
- **MUST NOT**: serde attributes on hexagon types.
- **Out of scope**: JSON for project reports (handle_project_json exists but full JSON schema not defined). JSON for stress test output. HTML output.

## References

- [[architecture-overview]] — Ports & Adapters: ReportWriterPort
- [[json-report-schema]] — full JSON schema specification
- [[ADR-0001]] — zero-dep hexagon (foundation)
- [[ADR-0021]] — honest degradation: `metric_support` + null-for-unsupported (ADR-7.9)
- Source: `hexagon/src/analysis/report_writer.rs`
- Source: `hexagon/src/analysis/output_format.rs`
- Source: `hexagon/src/analysis/run_analysis.rs`
- Source: `secondaries/src/gateways/report_writers/json_report_writer.rs`
