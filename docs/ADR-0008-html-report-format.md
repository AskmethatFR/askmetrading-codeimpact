# ADR-0008: HTML Report Format — Self-Contained Output & XSS Defense

**Status:** Applied  
**Date:** 2026-07-11  
**Decided in:** #7  
**Relations:**  
  supersedes: []  
  depends-on: ["architecture-overview", "ADR-0001", "ADR-0006", "ADR-0007"]  
  prerequisite: []

## Context

US7 requires a visual HTML report, shareable locally and in CI. It renders
code-derived data (file paths, function names, warning/suggestion messages) —
all of which are **untrusted input** from a security standpoint: an analyzed
repository can contain a file named `"><img src=x onerror=alert(1)>evil.rs`.
The hexagon is zero-dep (ADR-0001) and must stay serde-free. Three tensions:

1. **Where does generation live?** A runtime template engine (tera/handlebars)
   would add dependencies and diverge from the existing hand-written
   `console`/`json` writers.
2. **How is XSS prevented?** Per-value HTML escaping in the Rust builder is
   error-prone (one un-escaped interpolation = stored XSS).
3. **Self-containment.** The report must open from `file://` with zero network
   request (no CDN, no external asset) so it survives being emailed/zipped.

## Decision

**String-builder generation + data-island/`textContent` client rendering.**
This slice (T1) ships the **project view** (file list); the node-detail view is
T2 (see `html-report` node, staged breakdown).

| # | Decision | Rationale |
|---|---|---|
| ADR-8.1 | `write_html(&self, graph: &FileConsumptionGraph, target: &str) -> Result<String, AnalysisError>` on the `ReportWriter` port; returns the full HTML document | Mirrors `write_json` (ADR-7.1). Hexagon owns the contract, returns a `String`; the filesystem write lives in `primaries` (dependency rule, ADR-0001). |
| ADR-8.2 | `OutputFormat::Html` variant (pure enum, hexagon) + `handle_project_html` on `RunAnalysis` | Same shape as ADR-7.3/7.5. No serde on the enum. |
| ADR-8.3 | Hand-written string builder (`format!`), **no runtime template engine** | Consistency with `console`/`json` writers + ADR-0001 (no new dep). |
| ADR-8.4 | Presentation view-model DTOs (`ReportVm`/`ProjectVm`/`FileNodeVm`) in `secondaries`, `#[derive(Serialize)]` | ADR-7.2 pattern. Hexagon types never carry serde. |
| ADR-8.5 | **Data-island + `textContent` rendering is the primary XSS defense** | The writer emits a `<script id="ci-data" type="application/json">` island (serde-serialized VM) + an inline vanilla-JS renderer that writes every code-derived value through `textContent`/`createElement` — **never `innerHTML`**. DOM text nodes are never HTML-parsed, so injection from dynamic content is structurally impossible. |
| ADR-8.6 | `json_island_escape` closes the only residual vector (`</script>` breakout) | Browsers scan raw `<script>` bodies for a literal `</script` regardless of `type`. `<` `>` `&` → `<` `>` `&`; U+2028/U+2029 → ` `/` ` (defense in depth). |
| ADR-8.7 | Fully inline CSS + JS + JSON data island; **system font stack** (no embedded binary) | AC "openable in `file://`, zero external asset". A font binary adds weight + licensing surface to a zero-dep tool for no functional gain (Lean). Base64 `@font-face` remains a one-line CSS-variable swap if a brand font is later committed. |
| ADR-8.8 | Impact score displayed = `transitive_complexity()`, bars normalised to project max; level badge = `complexity_level()` | AD-4: a **display heuristic, not a new domain metric**. Inventing a scoring concept would leak business logic into a report adapter (ca-layering). |
| ADR-8.9 | File write reuses ADR-0006 discipline scaled to intent: canonicalize the parent dir, path-anonymised error messages | The `-o` path is user-chosen (local single-user CLI) → traversal risk low; still no path leak in errors. |

## Consequences

- **Positive**: XSS is structurally impossible for dynamic content (not merely "escaped"); confirmed by a live end-to-end test with a real malicious file name and an independent mutation check (bypassing `json_island_escape` fails exactly the 2 breakout integration tests).
- **Positive**: hexagon stays zero-dep; the HTML format is a pluggable adapter concern, same pattern as JSON (ADR-0007).
- **Positive**: T2 (node-detail view) is pure client-side JS — the T1 data island already carries the per-file model shape; no Rust change needed to add the detail view.
- **Cost**: the full document is built as one `String` (large repos allocate a big string). Mitigated identically to ADR-7 (stream later if needed, P1+).
- **Accepted risk (LOW, A01/CWE-22)**: `-o ../x.html` can write outside the cwd. No trust boundary is crossed (local CLI, user's own filesystem rights). Documented, not fixed, per the T1 threat model.

## Constraints

- **MUST**: `write_html` signature exactly `(&self, graph: &FileConsumptionGraph, target: &str) -> Result<String, AnalysisError>`.
- **MUST**: `OutputFormat` enum stays pure (no serde).
- **MUST**: every code-derived value reaches the DOM via `textContent`/`createElement`. **No `innerHTML` / `insertAdjacentHTML` / `document.write` / `eval` / `new Function` / `setAttribute('on*')` with data.**
- **MUST**: the JSON data island passes through `json_island_escape` before embedding.
- **MUST**: `--format html` on a single-file target errors (project view only in T1).
- **MUST NOT**: serde attributes on hexagon types; runtime template engine; external asset / network request.
- **MUST**: `ConsoleReportWriter` / `JsonReportWriter` implement `write_html` returning `Err` (only `HtmlReportWriter` renders HTML).
- **Out of scope (T1)**: node-detail view (T2), collapse/column-sort interactivity (T3), CI artifact (T4), filtering/search, base64 font embedding.

## References

- [[architecture-overview]] — Ports & Adapters: ReportWriter
- [[html-report]] — full technical node (view-model, escaping, staged T1–T4)
- [[ADR-0001]] — zero-dep hexagon (foundation)
- [[ADR-0006]] — file-write / path-leak discipline
- [[ADR-0007]] — JSON report format (sibling adapter, same pattern)
- Source: `secondaries/src/gateways/report_writers/html_report_writer.rs`
- Source: `hexagon/src/analysis/report_writer.rs`, `output_format.rs`, `run_analysis.rs`
- Source: `primaries/src/main.rs` (`write_html_report`, `--format html -o`)
