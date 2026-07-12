use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::StressTestRun;

use super::html::assets::{self, JS};
use super::html::view_model::{build_report_vm, ReportVm};

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
        css = assets::css(),
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

    fn write_html(
        &self,
        graph: &FileConsumptionGraph,
        target: &str,
    ) -> Result<String, AnalysisError> {
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
        assert!(
            !escaped.contains('<'),
            "escaped output must not contain a literal '<': {}",
            escaped
        );
        assert!(
            !escaped.contains('>'),
            "escaped output must not contain a literal '>': {}",
            escaped
        );
    }

    #[test]
    fn empty_string_returns_empty_string() {
        assert_eq!(json_island_escape(""), "");
    }
}
