# ADR-0008: HTML Report Format — Self-Contained Output & XSS Defense

**Status:** Applied  
**Date:** 2026-07-11 (amended 2026-07-12 — #27: ADR-8.10, ADR-8.11, addenda 8.8a/8.8b; ADR-8.7's font clause superseded; amended 2026-07-20 — #33/T3: ADR-8.12 support/note + `SUP` whitelist)  
**Decided in:** #7, #27, #33  
**Relations:**  
  supersedes: []  
  depends-on: ["architecture-overview", "ADR-0001", "ADR-0006", "ADR-0007"]  
  related: ["ADR-0021"]  
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
| ~~ADR-8.7~~ | ~~Fully inline CSS + JS + JSON data island; **system font stack** (no embedded binary)~~ — **superseded by ADR-8.11 (#27)** | Original rationale: a font binary adds weight + licensing surface for no functional gain (Lean). That premise died when the operator adopted the "Industry" design system, whose identity *is* Barlow / Barlow Condensed — a system stack does not render the approved design. 8.7 itself anticipated this: *"Base64 `@font-face` remains a one-line CSS-variable swap if a brand font is later committed."* The **inline CSS + JS + data island** and **zero-external-asset** halves of 8.7 stand. |
| ADR-8.8 | Impact score displayed = `transitive_complexity()`, bars normalised to project max; level badge = `complexity_level()` | AD-4: a **display heuristic, not a new domain metric**. Inventing a scoring concept would leak business logic into a report adapter (ca-layering). |
| ADR-8.8a | **Addendum (#27) — folder/project `level` = max of descendant file `complexity_level()`** by ordinal `low < moderate < high < critical` | `complexity_level()` is defined on a *file* with thresholds on cyclomatic complexity. Applied to a folder's *summed* cyclomatic it would mark every folder `critical` — nonsense. A max roll-up invents no new rule; it presents a value the domain already computed. The design canvas's own reference script proposed a fresh `score >= 75/45/22` threshold ladder — **rejected**: that is a scoring rule invented in a report adapter, exactly what 8.8 forbids. |
| ADR-8.8b | **Addendum (#27) — folder/project `economic` / `ecological` = fold of the children's domain values** via `EconomicImpact::add` / `EcologicalImpact::add` | The reference script fabricated both from a coefficient on transitive complexity. We fold the domain's own types with the domain's own `Add`, which re-derives the level / efficiency class from the summed value (`EfficiencyClass::from_co2(Σ co2)`) rather than copying a child's. Pinned by `folder_economic_impact_is_the_sum_of_children_impacts` and `folder_ecological_class_is_recomputed_from_summed_co2`. |
| ADR-8.9 | File write reuses ADR-0006 discipline scaled to intent: canonicalize the parent dir, path-anonymised error messages | The `-o` path is user-chosen (local single-user CLI) → traversal risk low; still no path leak in errors. |
| ADR-8.10 | **The tree, aggregation, thresholds, bar percentages and number formatting are computed in Rust** (`html/view_model.rs`); the JS is a pure `VM → DOM` projection. Rendering discipline: one `el()` node factory (`textContent` only); colours as CSS classes resolved through closed whitelists (`hasOwnProperty` + fallback), never style strings built from data; **exactly two** clamped numeric `.style` sinks; total ban on `innerHTML`/`outerHTML`/`insertAdjacentHTML`/`document.write`/`setAttribute`/`eval`/`new Function`/`javascript:`/`srcdoc`/`cssText`; handlers only via `addEventListener` + closure | This is the **enforceable form of ADR-8.5/8.6 for a dynamic DOM**. T2's DOM is ~10× bigger with far more interpolation sites, so "remember to use `textContent`" stops being a defense. The smaller and dumber the JS, the more auditable the boundary — and pushing the arithmetic into Rust puts it under tests that bite. The reference design markup interpolates into `style="…"` everywhere: a sink T1 did not have, deliberately not ported. Pinned by two structural tests. |
| ADR-8.11 | **Embedded base64 `@font-face`, 2 latin-subset faces** — Barlow 400 (body) + Barlow Condensed 600 (headings), committed as `.woff2`, `include_bytes!`-ed and base64-encoded in-crate (`html/base64.rs`, hand-rolled RFC 4648, **no new dependency**), emitted as `data:font/woff2;base64,`. Supersedes ADR-8.7's font clause | ~40 KB of base64 per report. **The zero-network-request invariant is preserved and strengthened** — the fonts are *in* the document, so the report renders identically offline, emailed or zipped. Trimmed from the 5 faces offered to the 2 the design actually uses (Lean). System stack kept as the CSS fallback, so a font that fails to decode degrades instead of breaking. SIL OFL 1.1 licence committed beside the binaries. |

### Amendment (#33 / US14-T3) — honest degradation in the HTML view-model

Honest degradation ([[ADR-0021]]) reaches the HTML writer. See [[ADR-0021]] D3 for the cross-format decision; the HTML-specific clause:

| # | Decision | Rationale |
|---|---|---|
| ADR-8.12 | `MetricVm` gains a `support` field (`ok` / `degraded` / `na`) and a `note` field; the JS maps `support` through a **closed `SUP` whitelist** to one of three fixed CSS classes `sup-ok` / `sup-degraded` / `sup-na` | This is [[ADR-0008]] §8.10 (**"colours reach the DOM as class names resolved through a closed whitelist, never a style string built from data"**) applied to the *support* dimension. `SUP` is resolved with `hasOwnProperty` + fallback exactly like the existing colour whitelist; **no `innerHTML`, no data-driven `.style`** — the two-numeric-`.style`-sink budget of §8.10 is untouched. An `Unsupported` metric renders `n/a` with its note; a `Degraded` metric shows the value plus the reason note. The whole ADR-8.5/8.6/8.10 XSS boundary is preserved — the support state is an enum projected to a fixed class, never a string. |

Rust output is byte-unchanged: `capabilities: None` / all-`Supported` produces `support: ok` with no note, i.e. the pre-T3 markup ([[ADR-0021]] D1).

## Consequences

- **Positive**: XSS is structurally impossible for dynamic content (not merely "escaped"); confirmed by a live end-to-end test with a real malicious file name and an independent mutation check (bypassing `json_island_escape` fails exactly the 2 breakout integration tests).
- **Positive**: hexagon stays zero-dep; the HTML format is a pluggable adapter concern, same pattern as JSON (ADR-0007).
- **Positive**: T2 (node-detail view) is pure client-side JS — the T1 data island already carries the per-file model shape; no Rust change needed to add the detail view.
- **Cost**: the full document is built as one `String` (large repos allocate a big string). Mitigated identically to ADR-7 (stream later if needed, P1+).
- **Accepted risk (LOW, A01/CWE-22)**: `-o ../x.html` can write outside the cwd. No trust boundary is crossed (local CLI, user's own filesystem rights). Documented, not fixed, per the T1 threat model.

## Constraints

- **MUST**: `write_html` signature exactly `(&self, graph: &FileConsumptionGraph, target: &str) -> Result<String, AnalysisError>`.
- **MUST**: `OutputFormat` enum stays pure (no serde).
- **MUST**: every code-derived value reaches the DOM via `textContent`/`createElement`. **No `innerHTML` / `outerHTML` / `insertAdjacentHTML` / `document.write` / `setAttribute` / `eval` / `new Function` / `javascript:` / `srcdoc` / `cssText`** — anywhere in the emitted JS (ADR-8.10 widens 8.5's ban from `setAttribute('on*')` to `setAttribute` entirely: it removes the whole class).
- **MUST**: colours reach the DOM as CSS **class names resolved through a closed whitelist**, never as a style string built from data. Exactly two numeric `.style` sinks (`width`, `paddingLeft`), each `Number()`-coerced, `isFinite`-checked and clamped.
- **MUST**: the JSON data island passes through `json_island_escape` before embedding.
- **MUST**: `--format html` on a single-file target errors (project view only).
- **MUST NOT**: serde attributes on hexagon types; runtime template engine; external asset / network request; **a scoring or threshold rule invented in the report adapter** (ADR-8.8a/b — fold the domain's values, never a coefficient).
- **MUST**: `ConsoleReportWriter` / `JsonReportWriter` implement `write_html` returning `Err` (only `HtmlReportWriter` renders HTML).
- **Out of scope**: CI PASS/FAIL check band (US8 — needs thresholds), git metadata banner (no port), consumption chains, filtering/search, dark theme. Column sort is **dropped**, not deferred: it belonged to design option 1b, and the operator chose 1a.

## References

- [[architecture-overview]] — Ports & Adapters: ReportWriter
- [[html-report]] — full technical node (view-model, escaping, staged T1–T4)
- [[ADR-0001]] — zero-dep hexagon (foundation)
- [[ADR-0006]] — file-write / path-leak discipline
- [[ADR-0007]] — JSON report format (sibling adapter, same pattern)
- [[ADR-0021]] — honest degradation: `support`/`note` + `SUP` whitelist (ADR-8.12)
- Source: `secondaries/src/gateways/report_writers/html_report_writer.rs`
- Source: `hexagon/src/analysis/report_writer.rs`, `output_format.rs`, `run_analysis.rs`
- Source: `primaries/src/main.rs` (`write_html_report`, `--format html -o`)
