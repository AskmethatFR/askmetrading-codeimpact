use std::time::SystemTime;

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::ComplexityWarning;
use codeimpact_hexagon::analysis::EcologicalImpact;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::FunctionDetail;
use codeimpact_hexagon::analysis::LanguageCapabilities;
use codeimpact_hexagon::analysis::Measurement;
use codeimpact_hexagon::analysis::MetricSupport;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::StressTestRun;
use codeimpact_hexagon::analysis::ThresholdBreach;
use codeimpact_hexagon::analysis::ThresholdReport;
use codeimpact_hexagon::analysis::UnmeasurableFile;
use codeimpact_hexagon::analysis::WarningSeverity;

use super::humanize::render_threshold_warning;

// ── Serde DTOs (ADR-4.2: never on hexagon types) ──

#[derive(serde::Serialize)]
struct JsonOutput {
    tool: ToolInfo,
    timestamp: String,
    target: String,
    target_type: String,
    metrics: MetricsDto,
}

#[derive(serde::Serialize)]
struct ToolInfo {
    name: &'static str,
    version: &'static str,
}

#[derive(serde::Serialize)]
struct MetricsDto {
    cyclomatic_complexity: u32,
    transitive_complexity: u32,
    hidden_complexity: u32,
    max_call_depth: usize,
    complexity_level: String,
    functions_with_cycles: Vec<String>,
    function_details: Vec<FunctionDetailDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    economic_impact: Option<EconomicImpactDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ecological_impact: Option<EcologicalImpactDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<WarningDto>,
    /// `None` (amends ADR-0007, T3 #33) exactly when `metric_support.
    /// io_in_loops` is `"unsupported"` — an empty-but-measured array would
    /// read as "measured, nothing found" when nothing was measured at all.
    /// `Some(vec![])` still skips (an empty array is honestly omitted, same
    /// as pre-T3), so Rust's wire shape is unchanged.
    #[serde(skip_serializing_if = "is_empty_supported_io")]
    io_in_loops: Option<Vec<IoInLoopDto>>,
    /// Files that could not be measured — omitted when empty, consistent
    /// with `warnings`/`io_in_loops` above; always empty for a single-file
    /// report, which has no notion of other files (D3, #50).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unmeasurable_files: Vec<UnmeasurableFileDto>,
    /// The count is never skipped — `0` is an honest, meaningful answer
    /// ("no file failed"), unlike an omitted array which would leave the
    /// count implicit.
    unmeasurable_files_count: usize,
    /// Loop-nested calls whose receiver could not be classified at all
    /// (#56 T2, `IoClassification::Unknown`) — an aggregate signal only
    /// (ADR-0010/ADR-0014 §4). `None` (T3 #33) exactly when `io_in_loops`
    /// is `None` above — same "unsupported, not a measured 0" rule. There
    /// is deliberately no per-call array here — abstention must not become
    /// a pseudo-warning.
    unclassifiable_io_in_loops_count: Option<usize>,
    /// Threshold-breach outcome (US8, AD-3) — never skipped, same "0/false
    /// is honest" convention as `unclassifiable_io_in_loops_count`: an
    /// empty `breaches` array with `has_breach: false` is a meaningful
    /// answer ("evaluated, nothing exceeded"), not an omission.
    thresholds: ThresholdsDto,
    /// Per-metric honesty signal (T3 #33, amends ADR-0007) — what the
    /// parser that produced this file's metrics can actually claim. Never
    /// skipped: absent capabilities (Rust, or no calling use case attached
    /// one) reads as fully `"supported"`, the same shape every consumer
    /// already saw pre-T3.
    metric_support: MetricSupportDto,
}

#[derive(serde::Serialize)]
struct MetricSupportDto {
    cyclomatic_complexity: String,
    call_graph: String,
    economic_impact: String,
    ecological_impact: String,
    io_in_loops: String,
}

/// Skips `io_in_loops` only when it is `Some(vec![])` — an empty-but-
/// measured array, the pre-T3 shape. `None` (Unsupported) is NEVER skipped:
/// it must serialize as literal JSON `null`, not be silently omitted.
fn is_empty_supported_io(io_in_loops: &Option<Vec<IoInLoopDto>>) -> bool {
    matches!(io_in_loops, Some(v) if v.is_empty())
}

fn metric_support_label(support: &MetricSupport) -> String {
    match support {
        MetricSupport::Supported => "supported".to_string(),
        MetricSupport::Degraded(reason) => format!("degraded: {}", reason),
        MetricSupport::Unsupported => "unsupported".to_string(),
    }
}

/// Builds the metric-support DTO from a (possibly absent) `LanguageCapabilities`
/// — `None` (no calling use case ever attached one, e.g. the project
/// aggregate, out of scope for T3 per Q2) reads as fully `"supported"`,
/// identical to the pre-T3 default every consumer already saw.
fn metric_support_dto(capabilities: Option<&LanguageCapabilities>) -> MetricSupportDto {
    match capabilities {
        Some(caps) => MetricSupportDto {
            cyclomatic_complexity: metric_support_label(caps.cyclomatic_complexity()),
            call_graph: metric_support_label(caps.call_graph()),
            economic_impact: metric_support_label(caps.economic_impact()),
            ecological_impact: metric_support_label(caps.ecological_impact()),
            io_in_loops: metric_support_label(caps.io_in_loops()),
        },
        None => MetricSupportDto {
            cyclomatic_complexity: "supported".to_string(),
            call_graph: "supported".to_string(),
            economic_impact: "supported".to_string(),
            ecological_impact: "supported".to_string(),
            io_in_loops: "supported".to_string(),
        },
    }
}

#[derive(serde::Serialize)]
struct ThresholdsDto {
    has_breach: bool,
    breaches: Vec<ThresholdBreachDto>,
    /// Human-readable "which threshold(s), by how much" — the ONE shared
    /// renderer (AD-3), empty when there is no breach.
    message: String,
}

#[derive(serde::Serialize)]
struct ThresholdBreachDto {
    metric: String,
    limit: f64,
    actual: f64,
    excess: f64,
}

#[derive(serde::Serialize)]
struct UnmeasurableFileDto {
    path: String,
    reason: String,
}

#[derive(serde::Serialize)]
struct FunctionDetailDto {
    name: String,
    location: LocationDto,
    direct: u32,
    transitive: u32,
    call_depth: usize,
    in_cycle: bool,
}

#[derive(serde::Serialize)]
struct EconomicImpactDto {
    cpu_cost_microdollars: f64,
    memory_bytes: u64,
    total_cost_microdollars: f64,
    level: String,
}

#[derive(serde::Serialize)]
struct EcologicalImpactDto {
    co2_grams: f64,
    energy_joules: f64,
    efficiency_class: String,
}

#[derive(serde::Serialize)]
struct WarningDto {
    pattern: String,
    severity: String,
    function: String,
    location: LocationDto,
    message: String,
    suggestion: String,
}

#[derive(serde::Serialize)]
struct IoInLoopDto {
    function: String,
    io_call: String,
    location: LocationDto,
}

#[derive(serde::Serialize)]
struct LocationDto {
    file: String,
    line: usize,
    col: usize,
}

// ── JsonReportWriter ──

#[derive(Default)]
pub struct JsonReportWriter;

impl JsonReportWriter {
    pub fn new() -> Self {
        Self
    }
}

impl ReportWriter for JsonReportWriter {
    fn write_console(&self, _metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        Err(AnalysisError::AnalysisFailed(
            "json writer does not support console output".into(),
        ))
    }

    fn write_json(
        &self,
        metrics: &CodeMetrics,
        target: &str,
        target_type: &str,
    ) -> Result<String, AnalysisError> {
        serialize_metrics(metrics, target, target_type)
    }

    fn write_project_report(&self, _graph: &FileConsumptionGraph) -> Result<(), AnalysisError> {
        // Print JSON to stdout for project-level reports
        Err(AnalysisError::AnalysisFailed(
            "json writer requires explicit format selection".into(),
        ))
    }

    fn write_project_json(
        &self,
        graph: &FileConsumptionGraph,
        target: &str,
    ) -> Result<String, AnalysisError> {
        serialize_project_metrics(graph, target)
    }

    fn write_stress_test(
        &self,
        _run: &StressTestRun,
        _impact: &Measurement<EconomicImpact>,
    ) -> Result<(), AnalysisError> {
        Err(AnalysisError::AnalysisFailed(
            "json writer does not support stress test output".into(),
        ))
    }

    fn write_html(
        &self,
        _graph: &FileConsumptionGraph,
        _target: &str,
    ) -> Result<String, AnalysisError> {
        Err(AnalysisError::AnalysisFailed(
            "json writer does not support html output".into(),
        ))
    }
}

/// Builds the thresholds DTO from a (possibly absent) `ThresholdReport` —
/// shared by the single-file and project serializers. `None` (no threshold
/// evaluation ever ran) reads identically to an evaluated-but-empty report:
/// both are honestly "nothing breached".
fn threshold_dto(report: Option<&ThresholdReport>) -> ThresholdsDto {
    let empty = ThresholdReport::default();
    let report = report.unwrap_or(&empty);
    ThresholdsDto {
        has_breach: report.has_breach(),
        breaches: report
            .breaches()
            .iter()
            .map(|b: &ThresholdBreach| ThresholdBreachDto {
                metric: b.metric().label().to_string(),
                limit: b.limit(),
                actual: b.actual(),
                excess: b.excess(),
            })
            .collect(),
        message: render_threshold_warning(report),
    }
}

// ── Shared serialization function (used by both JsonReportWriter and ConsoleReportWriter) ──

pub fn serialize_metrics(
    metrics: &CodeMetrics,
    target: &str,
    target_type: &str,
) -> Result<String, AnalysisError> {
    let timestamp = format_timestamp();

    let details: Vec<FunctionDetailDto> = metrics
        .function_details()
        .iter()
        .map(|d: &FunctionDetail| FunctionDetailDto {
            name: d.name().to_string(),
            location: LocationDto {
                file: d.location().file_path().to_string(),
                line: d.location().line(),
                col: d.location().col(),
            },
            direct: d.direct(),
            transitive: d.transitive(),
            call_depth: d.call_depth(),
            in_cycle: d.in_cycle(),
        })
        .collect();

    let warnings: Vec<WarningDto> = metrics
        .warnings()
        .iter()
        .map(|w: &ComplexityWarning| {
            let pattern = format!("{:?}", w.pattern);
            let severity = match w.severity {
                WarningSeverity::Warning => "Warning".to_string(),
                WarningSeverity::Critical => "Critical".to_string(),
            };
            WarningDto {
                pattern,
                severity,
                function: w.function.clone(),
                location: LocationDto {
                    file: w.location.file_path().to_string(),
                    line: w.location.line(),
                    col: w.location.col(),
                },
                message: w.message.clone(),
                suggestion: w.suggestion.clone(),
            }
        })
        .collect();

    // T3 (US16, #33, amends ADR-0007): an Unsupported io_in_loops capability
    // means nothing was measured at all — the array and its count must
    // serialize as `null`, never an empty-but-measured `[]`/`0`.
    let io_unsupported = metrics
        .capabilities()
        .map(|c| matches!(c.io_in_loops(), MetricSupport::Unsupported))
        .unwrap_or(false);

    let io_in_loops: Option<Vec<IoInLoopDto>> = if io_unsupported {
        None
    } else {
        Some(
            metrics
                .io_in_loops()
                .iter()
                .map(|w| IoInLoopDto {
                    function: w.function.clone(),
                    io_call: w.io_call.clone(),
                    location: LocationDto {
                        file: w.location.file_path().to_string(),
                        line: w.location.line(),
                        col: w.location.col(),
                    },
                })
                .collect(),
        )
    };
    let unclassifiable_io_in_loops_count: Option<usize> = if io_unsupported {
        None
    } else {
        Some(metrics.unclassifiable_io_in_loops_count())
    };

    let economic = metrics
        .economic_impact()
        .map(|e: &EconomicImpact| EconomicImpactDto {
            cpu_cost_microdollars: e.cpu_cost_microdollars(),
            memory_bytes: e.memory_bytes(),
            total_cost_microdollars: e.total_cost_microdollars(),
            level: e.level().to_string(),
        });

    let ecological = metrics
        .ecological_impact()
        .map(|e: &EcologicalImpact| EcologicalImpactDto {
            co2_grams: e.co2_grams(),
            energy_joules: e.energy_joules(),
            efficiency_class: e.efficiency_class().label().to_string(),
        });

    let output = JsonOutput {
        tool: ToolInfo {
            name: "codeimpact",
            version: env!("CARGO_PKG_VERSION"),
        },
        timestamp,
        target: target.to_string(),
        target_type: target_type.to_string(),
        metrics: MetricsDto {
            cyclomatic_complexity: metrics.cyclomatic_complexity(),
            transitive_complexity: metrics.transitive_complexity(),
            hidden_complexity: metrics.hidden_complexity(),
            max_call_depth: metrics.max_call_depth(),
            complexity_level: metrics.complexity_level().to_string(),
            functions_with_cycles: metrics.functions_with_cycles().to_vec(),
            function_details: details,
            economic_impact: economic,
            ecological_impact: ecological,
            warnings,
            io_in_loops,
            // A single file has no notion of other files failing to
            // measure — that is a project-level concept (D3, #50).
            unmeasurable_files: vec![],
            unmeasurable_files_count: 0,
            unclassifiable_io_in_loops_count,
            thresholds: threshold_dto(metrics.threshold_report()),
            metric_support: metric_support_dto(metrics.capabilities()),
        },
    };

    serde_json::to_string_pretty(&output)
        .map_err(|e| AnalysisError::AnalysisFailed(format!("JSON serialization error: {}", e)))
}

/// Serializes a project's `ProjectMetrics` (the single source of truth —
/// `FileConsumptionGraph::aggregated_metrics()`) to the same JSON shape as a
/// single file's metrics (ADR-0007: stable shape). Never fabricates a
/// `CodeMetrics` to reuse `serialize_metrics` (ADR-0012) — the project has no
/// `function_details`, no per-file location, and no single economic/
/// ecological impact worth pretending is one file's.
pub fn serialize_project_metrics(
    graph: &FileConsumptionGraph,
    target: &str,
) -> Result<String, AnalysisError> {
    let aggregated = graph.aggregated_metrics();
    let timestamp = format_timestamp();

    let unmeasurable_files: Vec<UnmeasurableFileDto> = graph
        .unmeasurable_files()
        .iter()
        .map(|f: &UnmeasurableFile| UnmeasurableFileDto {
            path: f.path.to_string_lossy().to_string(),
            reason: format!("{:?}", f.reason),
        })
        .collect();

    let output = JsonOutput {
        tool: ToolInfo {
            name: "codeimpact",
            version: env!("CARGO_PKG_VERSION"),
        },
        timestamp,
        target: target.to_string(),
        target_type: "project".to_string(),
        metrics: MetricsDto {
            cyclomatic_complexity: aggregated.total_cyclomatic_complexity,
            transitive_complexity: aggregated.total_transitive_complexity,
            hidden_complexity: aggregated.total_hidden_complexity,
            max_call_depth: aggregated.max_call_depth,
            complexity_level: aggregated.complexity_level().to_string(),
            functions_with_cycles: aggregated
                .files_with_cycles
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
            function_details: vec![],
            economic_impact: None,
            ecological_impact: None,
            warnings: vec![],
            // Project aggregate honesty (io_in_loops Unsupported for a
            // mixed-language project) is OUT OF SCOPE for T3 (human-
            // approved Q2, deferred to T3b) — always the pre-T3 shape.
            io_in_loops: Some(vec![]),
            unmeasurable_files_count: unmeasurable_files.len(),
            unmeasurable_files,
            unclassifiable_io_in_loops_count: Some(aggregated.total_unclassifiable_io_in_loops),
            thresholds: threshold_dto(graph.threshold_report()),
            metric_support: metric_support_dto(None),
        },
    };

    serde_json::to_string_pretty(&output)
        .map_err(|e| AnalysisError::AnalysisFailed(format!("JSON serialization error: {}", e)))
}

/// Format ISO8601 timestamp from SystemTime (ADR-4.7: avoid chrono dep).
fn format_timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Convert to ISO8601 (UTC)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Calculate year/month/day from Unix timestamp
    let (year, month, day) = days_to_date(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day) using civil calendar.
fn days_to_date(days: u64) -> (u64, u32, u32) {
    let mut y = 1970i64;
    let mut d = days as i64;

    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if d < days_in_year {
            break;
        }
        d -= days_in_year;
        y += 1;
    }

    let months_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0;
    for (i, &md) in months_days.iter().enumerate() {
        if d < md as i64 {
            m = i;
            break;
        }
        d -= md as i64;
    }

    (y as u64, (m + 1) as u32, (d + 1) as u32)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
