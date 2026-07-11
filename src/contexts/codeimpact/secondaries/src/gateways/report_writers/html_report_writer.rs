use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::StressTestRun;

// ── Presentation view-model (secondaries only, per ca-models / AD-3) ──
//
// Serde DTOs live beside the adapter, never on hexagon types — same rule
// ADR-0007/json-report-schema already applies to JsonReportWriter's DTOs.
// Covered transitively by html_report_writer_test.rs (adapter boundary),
// not unit-tested in isolation: see use-case-driven-design Test Surface Map.

#[derive(serde::Serialize)]
struct ReportVm {
    project: ProjectVm,
    files: Vec<FileNodeVm>,
}

#[derive(serde::Serialize)]
struct ProjectVm {
    target: String,
    file_count: usize,
}

#[derive(serde::Serialize)]
struct FileNodeVm {
    path: String,
    kind_label: String,
    score: u32,
    score_pct: u8,
    level_label: String,
}

/// Builds the project-view model (T1 scope): one row per file, no
/// per-function / per-warning detail (that is T2's node-detail view).
///
/// Score = transitive_complexity() (AD-4: a display heuristic, not a new
/// domain metric); bars are normalised against the project's max score.
fn build_report_vm(graph: &FileConsumptionGraph, target: &str) -> ReportVm {
    let per_file = graph.per_file_metrics();
    let max_score = per_file
        .values()
        .map(|m| m.transitive_complexity())
        .max()
        .unwrap_or(0);

    let mut files: Vec<FileNodeVm> = per_file
        .iter()
        .map(|(path, metrics)| {
            let score = metrics.transitive_complexity();
            let score_pct = if max_score == 0 {
                0
            } else {
                ((score as f64 / max_score as f64) * 100.0).round() as u8
            };
            FileNodeVm {
                path: path.to_string_lossy().to_string(),
                kind_label: "FILE".to_string(),
                score,
                score_pct,
                level_label: metrics.complexity_level().to_string(),
            }
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));

    ReportVm {
        project: ProjectVm {
            target: target.to_string(),
            file_count: files.len(),
        },
        files,
    }
}

const CSS: &str = r#"
:root {
  --font-heading: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  --font-body: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  --color-accent: #5b6cff;
  --color-accent-600: #4a58e0;
  --color-accent-700: #3944b3;
  --color-accent-900: #232a70;
  --color-text: #1a1a1f;
  --color-divider: #e4e4e8;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: var(--font-body);
  color: var(--color-text);
  background: #fafafa;
  padding: 32px;
}
h1 {
  font-family: var(--font-heading);
  font-weight: 600;
  font-size: 22px;
  margin: 0 0 4px;
}
p.summary { margin: 0 0 20px; color: #55555c; font-size: 13px; }
.blueprint {
  border: 1px solid var(--color-divider);
  background: #fff;
  padding: 4px 16px 16px;
}
.tag {
  display: inline-block;
  font-family: var(--font-heading);
  font-size: 11px;
  letter-spacing: .06em;
  text-transform: uppercase;
  padding: 3px 10px;
  border-radius: 3px;
  background: color-mix(in srgb, var(--color-accent) 15%, transparent);
  color: var(--color-accent-700);
}
table { width: 100%; border-collapse: collapse; }
th, td { text-align: left; padding: 8px 10px; border-bottom: 1px solid var(--color-divider); font-size: 13px; }
th { font-family: var(--font-heading); font-size: 11px; letter-spacing: .08em; text-transform: uppercase; color: #77777f; }
td.path { font-family: ui-monospace, Menlo, monospace; word-break: break-all; }
.bar-track { display: inline-block; height: 6px; width: 100px; background: color-mix(in srgb, var(--color-text) 10%, transparent); vertical-align: middle; }
.bar-fill { height: 100%; background: var(--color-accent-600); }
.score-value { display: inline-block; margin-left: 8px; font-variant-numeric: tabular-nums; }
"#;

const JS: &str = r#"
(function () {
  "use strict";
  var raw = document.getElementById("ci-data").textContent;
  var data = JSON.parse(raw);
  var root = document.getElementById("ci-root");

  var heading = document.createElement("h1");
  heading.textContent = "CodeImpact — " + data.project.target;
  root.appendChild(heading);

  var summary = document.createElement("p");
  summary.className = "summary";
  summary.textContent = data.project.file_count + " file(s) analyzed";
  root.appendChild(summary);

  var card = document.createElement("div");
  card.className = "blueprint";

  var table = document.createElement("table");
  var thead = document.createElement("thead");
  var headRow = document.createElement("tr");
  ["File", "Kind", "Score", "Level"].forEach(function (label) {
    var th = document.createElement("th");
    th.textContent = label;
    headRow.appendChild(th);
  });
  thead.appendChild(headRow);
  table.appendChild(thead);

  var tbody = document.createElement("tbody");
  data.files.forEach(function (file) {
    var row = document.createElement("tr");

    var pathCell = document.createElement("td");
    pathCell.className = "path";
    pathCell.textContent = file.path;
    row.appendChild(pathCell);

    var kindCell = document.createElement("td");
    kindCell.textContent = file.kind_label;
    row.appendChild(kindCell);

    var scoreCell = document.createElement("td");
    var track = document.createElement("span");
    track.className = "bar-track";
    var fill = document.createElement("span");
    fill.className = "bar-fill";
    fill.style.display = "block";
    fill.style.width = file.score_pct + "%";
    track.appendChild(fill);
    scoreCell.appendChild(track);
    var scoreText = document.createElement("span");
    scoreText.className = "score-value";
    scoreText.textContent = String(file.score);
    scoreCell.appendChild(scoreText);
    row.appendChild(scoreCell);

    var levelCell = document.createElement("td");
    var tag = document.createElement("span");
    tag.className = "tag";
    tag.textContent = file.level_label;
    levelCell.appendChild(tag);
    row.appendChild(levelCell);

    tbody.appendChild(row);
  });
  table.appendChild(tbody);
  card.appendChild(table);
  root.appendChild(card);
})();
"#;

fn render_html(vm: &ReportVm) -> Result<String, AnalysisError> {
    let json = serde_json::to_string(vm).map_err(|e| {
        AnalysisError::AnalysisFailed(format!("HTML view-model serialization error: {}", e))
    })?;
    let data_island = json_island_escape(&json);

    Ok(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>CodeImpact Report</title>
<style>{css}</style>
</head>
<body>
<main id="ci-root"></main>
<script id="ci-data" type="application/json">{data}</script>
<script>{js}</script>
</body>
</html>
"#,
        css = CSS,
        data = data_island,
        js = JS,
    ))
}

/// Escapes a JSON string body for safe embedding inside a
/// `<script type="application/json">` data island (AD-2).
///
/// Browsers scan raw script content for a literal `</script` close tag
/// regardless of the `type` attribute, so `<` / `>` / `&` are escaped to
/// their `\uXXXX` form to make a breakout like `</script><script>...`
/// structurally impossible. U+2028/U+2029 (line/paragraph separator) are
/// valid inside a JSON string but are escaped too for defense in depth,
/// consistent with the port contract.
pub fn json_island_escape(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '<' => escaped.push_str("\\u003C"),
            '>' => escaped.push_str("\\u003E"),
            '&' => escaped.push_str("\\u0026"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            _ => escaped.push(c),
        }
    }
    escaped
}

#[derive(Default)]
pub struct HtmlReportWriter;

impl HtmlReportWriter {
    pub fn new() -> Self {
        Self
    }
}

impl ReportWriter for HtmlReportWriter {
    fn write_console(&self, _metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        Err(AnalysisError::AnalysisFailed(
            "html writer does not support console output".into(),
        ))
    }

    fn write_json(
        &self,
        _metrics: &CodeMetrics,
        _target: &str,
        _target_type: &str,
    ) -> Result<String, AnalysisError> {
        Err(AnalysisError::AnalysisFailed(
            "html writer does not support json output".into(),
        ))
    }

    fn write_project_report(&self, _graph: &FileConsumptionGraph) -> Result<(), AnalysisError> {
        Err(AnalysisError::AnalysisFailed(
            "html writer does not support console project output".into(),
        ))
    }

    fn write_stress_test(
        &self,
        _run: &StressTestRun,
        _impact: &EconomicImpact,
    ) -> Result<(), AnalysisError> {
        Err(AnalysisError::AnalysisFailed(
            "html writer does not support stress test output".into(),
        ))
    }

    fn write_html(&self, graph: &FileConsumptionGraph, target: &str) -> Result<String, AnalysisError> {
        let vm = build_report_vm(graph, target);
        render_html(&vm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test List (json_island_escape):
    // 1. plain string without special characters is unchanged
    // 2. `<` is escaped to <
    // 3. `>` is escaped to >
    // 4. `&` is escaped to &
    // 5. U+2028 (line separator) is escaped to
    // 6. U+2029 (paragraph separator) is escaped to
    // 7. `</script><script>alert(1)</script>` payload no longer contains a literal "</script" breakout
    // 8. `"><img onerror>` payload no longer contains a literal `<` or `>`
    // 9. empty string input returns empty string output

    #[test]
    fn plain_string_is_unchanged() {
        assert_eq!(json_island_escape("hello world"), "hello world");
    }

    #[test]
    fn escapes_less_than() {
        assert_eq!(json_island_escape("<"), "\\u003C");
    }

    #[test]
    fn escapes_greater_than() {
        assert_eq!(json_island_escape(">"), "\\u003E");
    }

    #[test]
    fn escapes_ampersand() {
        assert_eq!(json_island_escape("&"), "\\u0026");
    }

    #[test]
    fn escapes_line_separator() {
        assert_eq!(json_island_escape("\u{2028}"), "\\u2028");
    }

    #[test]
    fn escapes_paragraph_separator() {
        assert_eq!(json_island_escape("\u{2029}"), "\\u2029");
    }

    #[test]
    fn neutralizes_script_breakout_payload() {
        let payload = "</script><script>alert(1)</script>";
        let escaped = json_island_escape(payload);
        assert!(
            !escaped.contains("</script"),
            "escaped output must not contain a literal script close tag: {}",
            escaped
        );
    }

    #[test]
    fn neutralizes_img_onerror_payload() {
        let payload = "\"><img onerror>";
        let escaped = json_island_escape(payload);
        assert!(!escaped.contains('<'), "escaped output must not contain a literal '<': {}", escaped);
        assert!(!escaped.contains('>'), "escaped output must not contain a literal '>': {}", escaped);
    }

    #[test]
    fn empty_string_returns_empty_string() {
        assert_eq!(json_island_escape(""), "");
    }
}
