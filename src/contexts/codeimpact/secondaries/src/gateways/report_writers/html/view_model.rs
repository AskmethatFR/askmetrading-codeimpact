use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::WarningSeverity;

// ── Presentation view-model (secondaries only, per ca-models / ADR-8.4) ──
//
// Serde DTOs live beside the adapter, never on hexagon types. Covered
// transitively by html_report_writer_test.rs (adapter boundary), not
// unit-tested in isolation: see use-case-driven-design Test Surface Map.

#[derive(serde::Serialize)]
pub struct ReportVm {
    pub project: ProjectVm,
    pub stats: Vec<StatVm>,
    pub files: Vec<FileNodeVm>,
}

#[derive(serde::Serialize)]
pub struct ProjectVm {
    pub target: String,
    pub tool: String,
}

#[derive(serde::Serialize)]
pub struct StatVm {
    pub label: String,
    pub value: String,
    pub sub: String,
}

#[derive(serde::Serialize)]
pub struct FileNodeVm {
    pub path: String,
    pub score: u32,
    pub score_pct: u8,
    pub level_label: String,
}

/// Builds the project-view model: banner + 8 aggregated stat tiles (US7 T2
/// S1) plus the flat per-file list (T1 shape, kept as-is until S2 replaces
/// it with the tree + detail pane).
pub fn build_report_vm(graph: &FileConsumptionGraph, target: &str) -> ReportVm {
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
            tool: concat!("codeimpact v", env!("CARGO_PKG_VERSION")).to_string(),
        },
        stats: build_stats(graph),
        files,
    }
}

fn build_stats(graph: &FileConsumptionGraph) -> Vec<StatVm> {
    let per_file = graph.per_file_metrics();
    let aggregated = graph.aggregated_metrics();

    let total_hidden: u32 = per_file.values().map(|m| m.hidden_complexity()).sum();

    let total_warnings_and_io: usize = per_file
        .values()
        .map(|m| m.warnings().len() + m.io_in_loops().len())
        .sum();
    let critical_count: usize = per_file
        .values()
        .map(|m| {
            m.warnings()
                .iter()
                .filter(|w| w.severity == WarningSeverity::Critical)
                .count()
                + m.io_in_loops().len()
        })
        .sum();

    let hotspots = per_file
        .values()
        .filter(|m| m.complexity_level() == "critical")
        .count();

    let cost_value = match &aggregated.total_economic_impact {
        Some(economic) => format_dollars(economic.total_cost_microdollars()),
        None => "\u{2014}".to_string(),
    };

    let (eco_class_value, eco_sub) = match &aggregated.total_ecological_impact {
        Some(ecological) => (
            ecological.efficiency_class().label().to_string(),
            format_energy_short(ecological.energy_joules()),
        ),
        None => ("\u{2014}".to_string(), String::new()),
    };

    vec![
        StatVm {
            label: "Files".to_string(),
            value: per_file.len().to_string(),
            sub: "analysed".to_string(),
        },
        StatVm {
            label: "Direct \u{3a3}".to_string(),
            value: aggregated.total_cyclomatic_complexity.to_string(),
            sub: "cyclomatic".to_string(),
        },
        StatVm {
            label: "Transitive \u{3a3}".to_string(),
            value: aggregated.total_transitive_complexity.to_string(),
            sub: format!("{} hidden", total_hidden),
        },
        StatVm {
            label: "Warnings".to_string(),
            value: total_warnings_and_io.to_string(),
            sub: format!("{} critical", critical_count),
        },
        StatVm {
            label: "Max depth".to_string(),
            value: aggregated.max_call_depth.to_string(),
            sub: "call chain".to_string(),
        },
        StatVm {
            label: "Hotspots".to_string(),
            value: hotspots.to_string(),
            sub: "critical files".to_string(),
        },
        StatVm {
            label: "Est. cost".to_string(),
            value: cost_value,
            sub: "per run".to_string(),
        },
        StatVm {
            label: "Eco class".to_string(),
            value: eco_class_value,
            sub: eco_sub,
        },
    ]
}

/// Temporary local copy (US7 T2 S1) — merged into the shared `humanize`
/// module at slice R, once console_report_writer's duplicated MB/kJ
/// branches are extracted alongside it (spec §5).
fn format_dollars(microdollars: f64) -> String {
    let dollars = microdollars / 1_000_000.0;
    if dollars < 0.0001 {
        format!("${:.6}", dollars)
    } else if dollars < 1.0 {
        format!("${:.4}", dollars)
    } else {
        format!("${:.2}", dollars)
    }
}

/// Energy formatted for a stat-tile sub (no kWh parenthetical) — same
/// thresholds as console_report_writer's inline branch.
fn format_energy_short(joules: f64) -> String {
    if joules >= 1000.0 {
        format!("{:.1} kJ", joules / 1000.0)
    } else {
        format!("{:.1} J", joules)
    }
}
