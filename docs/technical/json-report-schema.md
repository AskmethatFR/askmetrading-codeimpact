# JSON Report Schema — CodeImpact

**id:** json-report-schema  
**type:** technical  
**owner:** Architect  
**status:** Applied  
**decided_in:** #4  
**relations:**  
  depends-on: ["ADR-0007", "architecture-overview"]  
  applied-in: ["ADR-0007"]

## Overview

The JSON report is the machine-readable output of `CodeImpact` analysis. It is produced by `JsonReportWriter` (and `ConsoleReportWriter` for consistency) and consumed by CI pipelines, dashboards, and automated tooling.

## Schema

```json
{
  "tool": {
    "name": "codeimpact",
    "version": "0.1.0"
  },
  "timestamp": "2026-07-11T15:30:00Z",
  "target": "main.rs",
  "target_type": "file",
  "metrics": {
    "cyclomatic_complexity": 5,
    "transitive_complexity": 8,
    "hidden_complexity": 3,
    "max_call_depth": 2,
    "complexity_level": "low",
    "functions_with_cycles": [],
    "function_details": [
      {
        "name": "main",
        "direct": 5,
        "transitive": 8,
        "call_depth": 2,
        "in_cycle": false
      }
    ],
    "economic_impact": {
      "cpu_cost_microdollars": 12.5,
      "memory_bytes": 5000,
      "total_cost_microdollars": 13.0,
      "level": "moderate"
    },
    "ecological_impact": {
      "co2_grams": 2.4,
      "energy_joules": 21600.0,
      "efficiency_class": "B"
    },
    "warnings": [
      {
        "pattern": "DeepConditional",
        "severity": "Warning",
        "function": "main",
        "message": "Function 'main' has deeply nested conditionals (depth=5). Consider extracting inner branches.",
        "suggestion": "Extract inner branches into named functions."
      }
    ],
    "io_in_loops": [
      {
        "function": "read_file",
        "io_call": "std::fs::read",
        "location": {
          "file": "src/main.rs",
          "line": 5,
          "col": 9
        }
      }
    ]
  }
}
```

## Fields

### Top-level

| Field | Type | Required | Description |
|---|---|---|---|
| `tool` | object | yes | Tool identification (name, version) |
| `timestamp` | string | yes | ISO8601 UTC timestamp of analysis |
| `target` | string | yes | Analysed file path or project name |
| `target_type` | string | yes | `"file"` or `"project"` |
| `metrics` | object | yes | All analysis metrics |

### `tool`

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Always `"codeimpact"` |
| `version` | string | yes | Semver from `Cargo.toml` (compile-time constant) |

### `metrics`

| Field | Type | Required | Description |
|---|---|---|---|
| `cyclomatic_complexity` | integer | yes | Direct cyclomatic complexity |
| `transitive_complexity` | integer | yes | `direct + hidden` — cost of comprehension. Derived, never stored. See [[ADR-0012]] |
| `hidden_complexity` | integer | yes | Sum of `direct(g)` over the **reachable subgraph**, each distinct function counted **once**. Aggregates additively. **Never** `transitive - direct` on aggregates — see [[ADR-0012]] |
| `max_call_depth` | integer | yes | Maximum call chain depth |
| `complexity_level` | string | yes | `"low"`, `"moderate"`, `"high"`, `"critical"` |
| `functions_with_cycles` | string[] | yes | Function names in call cycles |
| `function_details` | array | yes | Per-function breakdown |
| `economic_impact` | object | no | Economic impact estimation (absent if N/A) |
| `ecological_impact` | object | no | Ecological impact estimation (absent if N/A) |
| `warnings` | array | yes | Complexity warnings (empty if none) |
| `io_in_loops` | array | yes | I/O calls inside loops (empty if none) |

### `function_details[]`

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Function name |
| `direct` | integer | yes | Direct cyclomatic complexity |
| `transitive` | integer | yes | Transitive complexity |
| `call_depth` | integer | yes | Call chain depth |
| `in_cycle` | boolean | yes | True if function is part of a call cycle |

### `economic_impact`

| Field | Type | Required | Description |
|---|---|---|---|
| `cpu_cost_microdollars` | number | yes | Estimated CPU cost in μ$ |
| `memory_bytes` | integer | yes | Estimated memory in bytes |
| `total_cost_microdollars` | number | yes | `cpu_cost + memory * 0.0001` |
| `level` | string | yes | `"low"`, `"moderate"`, `"high"`, `"critical"` |

### `ecological_impact`

| Field | Type | Required | Description |
|---|---|---|---|
| `co2_grams` | number | yes | Estimated CO₂ in grams |
| `energy_joules` | number | yes | Estimated energy in joules |
| `efficiency_class` | string | yes | `"A"` through `"G"` efficiency class |

### `warnings[]`

| Field | Type | Required | Description |
|---|---|---|---|
| `pattern` | string | yes | Warning pattern identifier (e.g. `DeepConditional`) |
| `severity` | string | yes | `"Warning"` or `"Error"` |
| `function` | string | yes | Affected function name |
| `message` | string | yes | Human-readable description |
| `suggestion` | string | yes | Remediation suggestion |

### `io_in_loops[]`

| Field | Type | Required | Description |
|---|---|---|---|
| `function` | string | yes | Function containing the I/O call |
| `io_call` | string | yes | I/O function name (e.g. `std::fs::read`) |
| `location` | object | yes | Source location (file, line, col) |

## DTO Implementation

The JSON schema is implemented as `#[derive(serde::Serialize)]` DTOs in `secondaries/src/gateways/report_writers/json_report_writer.rs`:

- `JsonOutput` — top-level wrapper
- `ToolInfo` — tool identification
- `MetricsDto` — all metrics
- `FunctionDetailDto` — per-function detail
- `EconomicImpactDto` — economic impact
- `EcologicalImpactDto` — ecological impact
- `WarningDto` — complexity warning
- `IoInLoopDto` — I/O in loop detection
- `CodeLocationDto` — source location

Conversion from hexagon domain types to DTOs is via `From` trait impls.

## Project-aggregate `metric_support` honesty (#89 S2, ADR-0026)

The project-level `metrics` object carries a `metric_support` object (one label per axis: `"supported"` / `"degraded: partial: M/N…"` / `"unsupported"`), folded from every file's `LanguageCapabilities` per the [[ADR-0026]] lattice. When an axis aggregates to `Unsupported`, its metric serializes **JSON `null`, never `[]` or `0`** — e.g. `io_in_loops: null` and `unclassifiable_io_in_loops_count: null` — the same null-not-empty rule [[ADR-0021]] D3 set at file level. `Degraded` keeps the real (partial) value; `Supported` is byte-identical to the pre-#89 shape (additive only, [[ADR-0007]]). Note (ADR-0026): no shipped adapter emits `Unsupported` today, so the `null` path is forward-compatible but exercised only via synthetic fixtures.

## References

- [[ADR-0007]] — decisions that led to this schema
- [[architecture-overview]] — economic impact formulas
- Source: `secondaries/src/gateways/report_writers/json_report_writer.rs`
- Tests: `tests/secondaries.integration_test/tests/json_report_writer_test.rs`
