// #28 (ADR-8.10 hardening) — the static structural gates
// (`html_renderer_gate_hardening_test.rs`) can prove a banned sink is
// absent from the emitted JS's SOURCE TEXT. They cannot prove that a
// payload landing in the DOM as `textContent` truly never becomes markup
// under a spec-conformant HTML parser, that the closed-whitelist `cls()`
// helper truly falls back on a hostile key instead of leaking
// `Object.prototype`, or that the two numeric `.style` sinks truly clamp
// hostile input. This test runs the REAL emitted JS
// (`codeimpact_secondaries::gateways::report_writers::html::assets::JS`,
// reached through the real `HtmlReportWriter::write_html` pipeline) inside
// jsdom — a genuine HTML parser + DOM implementation — against an
// adversarial data island, and asserts on the resulting DOM state.
//
// The adversarial data island is built by taking write_html()'s REAL
// output and patching the root node's `level`/`metrics` fields directly
// (bypassing the Rust domain, which cannot produce these values through its
// own closed enums) — a defense-in-depth simulation of "what if the data
// island contract were ever violated by a future bug", per the ADR-8.10
// rendering-discipline's own layered-defense intent.
//
// Test List (one scenario, multiple assertions — the AC itself specifies a
// single adversarial run with a compound assertion list; splitting it into
// N tests would only multiply Node process spawns for zero additional
// discriminating power, since every assertion targets the SAME execution):
// 1. no extra <script>/<img>/<iframe> tag was parsed from a script-breakout
//    or img-onerror payload in a file path
// 2. the payload reached the DOM as literal text (textContent), not markup
// 3. no code-execution canary fired (window.__pwned__ stays false)
// 4. the jsdom window's own Object.prototype carries no extra own property
//    (no prototype pollution), and a fresh plain object has no stray
//    enumerable (inherited) keys
// 5. a hostile whitelist key ("__proto__" for level, "constructor" for
//    support) falls back to the safe default class via cls()'s
//    hasOwnProperty guard, not a leaked prototype/constructor object
// 6. hostile numeric pct values (a non-numeric string with the ticket's own
//    example, "NaN", a large negative, a large positive) all clamp through
//    setPct's Number()/isFinite()/clamp, never producing NaN%/negative%
// 7. a tree nested past setIndent's 20-level cap clamps to exactly 300px,
//    never an unclamped larger value

use std::path::PathBuf;

use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_secondaries::gateways::report_writers::html_report_writer::json_island_escape;
use codeimpact_secondaries::gateways::report_writers::html_report_writer::HtmlReportWriter;
use codeimpact_secondaries_integration_test::js_runtime_check;

const SCRIPT_BREAKOUT_PATH: &str = "</script><script>window.__pwned__=true</script>evil-script.rs";
const IMG_ONERROR_PATH: &str = "\"><img src=x onerror=window.__pwned__=true>evil-img.rs";

fn make_metrics(cc: u32, tc: u32) -> CodeMetrics {
    CodeMetrics::with_call_graph(cc, tc, 0, vec![], vec![])
}

fn graph_with_files(files: Vec<(&str, u32, u32)>) -> FileConsumptionGraph {
    let entries: Vec<(PathBuf, CodeMetrics)> = files
        .into_iter()
        .map(|(path, cc, tc)| (PathBuf::from(path), make_metrics(cc, tc)))
        .collect();
    FileConsumptionGraph::build(&entries, vec![]).unwrap()
}

/// A single-child chain 25 folders deep, well past `setIndent`'s 20-level
/// clamp, so the deepest tree row exercises the clamp branch instead of the
/// pass-through branch.
fn deeply_nested_path() -> String {
    let folders: Vec<String> = (1..=25).map(|i| format!("lvl{i}")).collect();
    format!("{}/deep.rs", folders.join("/"))
}

/// Byte offsets of the JSON payload inside the `ci-data` script tag (start
/// inclusive, end exclusive) — mirrors `html_report_writer_test.rs`'s
/// `extract_data_island`, but returns offsets (not a parsed `Value`) so the
/// caller can splice a patched payload back into the document.
fn data_island_bounds(html: &str) -> (usize, usize) {
    let start_marker = r#"<script id="ci-data" type="application/json">"#;
    let start = html
        .find(start_marker)
        .expect("data island start marker should be present")
        + start_marker.len();
    let end = html[start..]
        .find("</script>")
        .expect("data island end marker should be present")
        + start;
    (start, end)
}

/// Builds the REAL report document (real `write_html`, real `assets::JS`),
/// then patches the root node's `level`/`metrics` with values the Rust
/// domain's closed enums could never produce — simulating a hostile data
/// island for the JS's own defensive code (`cls()`'s `hasOwnProperty`
/// guard, `setPct`'s `Number()`/`isFinite()` clamp) to prove itself against.
fn build_adversarial_report_html() -> String {
    let writer = HtmlReportWriter::new();
    let graph = graph_with_files(vec![
        (SCRIPT_BREAKOUT_PATH, 1, 1),
        (IMG_ONERROR_PATH, 1, 1),
        (deeply_nested_path().as_str(), 1, 1),
    ]);
    let html = writer
        .write_html(&graph, "proj")
        .expect("write_html should succeed");

    let (start, end) = data_island_bounds(&html);
    let mut data: serde_json::Value =
        serde_json::from_str(&html[start..end]).expect("data island should be valid JSON");

    let root = data["nodes"]
        .as_array_mut()
        .expect("nodes array")
        .iter_mut()
        .find(|n| n["id"] == "")
        .expect("root node with id \"\"");

    // Hostile whitelist key: LVL has no OWN "__proto__" property, so a
    // correctly-guarded cls() must fall back to "lvl-low".
    root["level"] = serde_json::json!("__proto__");

    // Four hostile numeric pcts + one hostile support key, all on the ONE
    // node (root) that renderDetail() shows by default (no simulated click
    // needed) — replaces whatever write_html() computed, since the point is
    // to feed setPct/cls() values the real pipeline could never produce.
    root["metrics"] = serde_json::json!([
        {
            "label": "Hostile injection string",
            "value": "1",
            "pct": "100; background:url(evil)",
            "support": "constructor",
            "note": ""
        },
        {
            "label": "Hostile NaN string",
            "value": "1",
            "pct": "NaN",
            "support": "supported",
            "note": ""
        },
        {
            "label": "Hostile large negative",
            "value": "1",
            "pct": -999,
            "support": "supported",
            "note": ""
        },
        {
            "label": "Hostile large positive",
            "value": "1",
            "pct": 999999,
            "support": "supported",
            "note": ""
        }
    ]);

    let patched_json = serde_json::to_string(&data).expect("re-serialize data island");
    let patched_escaped = json_island_escape(&patched_json);
    format!("{}{}{}", &html[..start], patched_escaped, &html[end..])
}

#[test]
fn rendered_js_survives_adversarial_data_island_without_code_exec_markup_or_pollution() {
    js_runtime_check::require_node_or_fail_loudly(
        "rendered_js_survives_adversarial_data_island_without_code_exec_markup_or_pollution",
    );
    js_runtime_check::ensure_npm_install();

    let adversarial_html = build_adversarial_report_html();
    let dir = tempfile::tempdir().expect("create temp dir");
    let html_path = dir.path().join("adversarial-report.html");
    std::fs::write(&html_path, &adversarial_html).expect("write adversarial html fixture");

    // IMG_ONERROR_PATH, not SCRIPT_BREAKOUT_PATH: the script-breakout payload
    // contains literal "/" characters (inside "</script>"), which Rust's own
    // `Path` component splitting (`node_id`, view_model.rs) legitimately
    // treats as folder separators — fragmenting it across several tree
    // nodes, so the original concatenated string never appears intact
    // anywhere. IMG_ONERROR_PATH has no "/" and survives as one atomic path
    // segment, making it the right probe for "did this literal string reach
    // the DOM as inert text".
    let result = js_runtime_check::run_check(&html_path, Some(IMG_ONERROR_PATH));

    assert!(
        result["run_error"].is_null(),
        "check.mjs must run the document without throwing: {:?}",
        result["run_error"]
    );

    // 1 & 2 — no markup was parsed from the script-breakout / img-onerror
    // payloads; exactly the two legitimate <script> tags exist.
    assert_eq!(
        result["img_tag_count"].as_u64(),
        Some(0),
        "an img-onerror payload in a file path must never become a real <img>: {:?}",
        result
    );
    assert_eq!(
        result["script_tag_count"].as_u64(),
        Some(2),
        "a script-breakout payload in a file path must never add a third <script>: {:?}",
        result
    );
    assert_eq!(
        result["iframe_tag_count"].as_u64(),
        Some(0),
        "no payload here targets iframe, sanity-checking the parser saw none anyway: {:?}",
        result
    );
    assert_eq!(
        result["body_text_contains_payload_literally"].as_bool(),
        Some(true),
        "the script-breakout payload must still reach the DOM as literal, inert text: {:?}",
        result
    );

    // 3 — no code execution.
    assert_eq!(
        result["pwned"].as_bool(),
        Some(false),
        "the code-execution canary (window.__pwned__) must never fire: {:?}",
        result
    );

    // 4 — Object.prototype hygiene, in the jsdom window's own realm.
    assert_eq!(
        result["prototype_polluted"].as_bool(),
        Some(false),
        "the jsdom window's Object.prototype must carry no extra own property: {:?}",
        result
    );
    assert_eq!(
        result["plain_object_has_stray_keys"].as_bool(),
        Some(false),
        "a fresh plain object must have no stray enumerable (inherited) keys: {:?}",
        result
    );

    // 5 — hostile whitelist keys fall back to the safe default class.
    assert_eq!(
        result["first_swatch_class"].as_str(),
        Some("swatch lvl-low"),
        "a node with level \"__proto__\" must render the cls() fallback class, not a leaked \
         prototype object stringified into the className: {:?}",
        result
    );
    assert_eq!(
        result["metrics_grid_tag_count"].as_u64(),
        Some(1),
        "exactly the one crafted hostile-support metric should render a support tag: {:?}",
        result
    );
    assert_eq!(
        result["support_tag_class"].as_str(),
        Some("tag sup-ok"),
        "a metric with support \"constructor\" must render the cls() fallback class, not a \
         leaked Object constructor stringified into the className: {:?}",
        result
    );

    // 6 — hostile numerics clamp through setPct's Number()/isFinite()/clamp,
    // in the SAME order as the crafted metrics array above.
    assert_eq!(
        result["metric_fill_widths"],
        serde_json::json!(["0%", "0%", "0%", "100%"]),
        "hostile pct values must clamp (non-numeric string -> 0%, \"NaN\" -> 0%, large \
         negative -> 0%, large positive -> 100%), never propagate unclamped or as NaN%: {:?}",
        result
    );

    // 7 — setIndent's 20-level depth clamp.
    assert_eq!(
        result["max_indent_padding_left_px"].as_f64(),
        Some(300.0),
        "a tree nested past the 20-level cap must clamp paddingLeft at exactly 20*15=300px: {:?}",
        result
    );
}
