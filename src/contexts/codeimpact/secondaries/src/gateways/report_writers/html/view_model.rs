use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use codeimpact_hexagon::analysis::ComplexityWarning;
use codeimpact_hexagon::analysis::EcologicalImpact;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::IoInLoopWarning;
use codeimpact_hexagon::analysis::WarningPattern;
use codeimpact_hexagon::analysis::WarningSeverity;

use super::super::humanize::{format_dollars, format_energy, format_memory};

// ── Presentation view-model (secondaries only, per ca-models / ADR-8.4) ──
//
// Serde DTOs live beside the adapter, never on hexagon types. Covered
// transitively by html_report_writer_test.rs (adapter boundary), not
// unit-tested in isolation: see use-case-driven-design Test Surface Map.

#[derive(serde::Serialize)]
pub struct ReportVm {
    pub project: ProjectVm,
    pub stats: Vec<StatVm>,
    pub nodes: Vec<NodeVm>,
    pub unmeasurable_files: Vec<UnmeasurableFileVm>,
}

/// A file that could not be measured at all (D3, #50 slice S4) — distinct
/// from a node's `level: "none"` (parsed OK, zero functions). Surfaced at
/// the project level (spec's "same spirit as the console `=== Fichiers NON
/// MESURÉS ===` section"), not per-tree-node: an unmeasurable file has no
/// `CodeMetrics` to hang a `NodeVm` off of.
#[derive(serde::Serialize)]
pub struct UnmeasurableFileVm {
    pub path: String,
    pub reason: String,
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

/// One entry per project/folder/file node, flattened (spec §2: "the root is
/// the node with id == \"\""). Folders/project have no `CodeMetrics` of
/// their own — their fields are a postorder aggregation of their children
/// (spec §3); functions/warnings/io/economic/ecological detail is S3 scope.
#[derive(serde::Serialize)]
pub struct NodeVm {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub path: String,
    pub child_ids: Vec<String>,
    pub score: u32,
    pub level: String,
    pub metrics: Vec<MetricVm>,
    pub functions: Vec<FunctionVm>,
    pub warnings: Vec<WarningVm>,
    pub ios: Vec<IoVm>,
    pub economic: Option<EconomicVm>,
    pub ecological: Option<EcologicalVm>,
}

#[derive(serde::Serialize)]
pub struct MetricVm {
    pub label: String,
    pub value: String,
    pub pct: u8,
}

#[derive(Clone, serde::Serialize)]
pub struct FunctionVm {
    pub name: String,
    pub direct: u32,
    pub transitive: u32,
    pub depth: usize,
    pub loc: String,
    pub in_cycle: bool,
}

#[derive(Clone, serde::Serialize)]
pub struct WarningVm {
    pub pattern: String,
    pub severity: String,
    pub sev_label: String,
    pub function: String,
    pub loc: String,
    pub message: String,
    pub suggestion: String,
}

#[derive(Clone, serde::Serialize)]
pub struct IoVm {
    pub function: String,
    pub io_call: String,
    pub loc: String,
}

#[derive(Clone, serde::Serialize)]
pub struct EconomicVm {
    pub cpu: String,
    pub memory: String,
    pub total: String,
    pub level: String,
}

#[derive(Clone, serde::Serialize)]
pub struct EcologicalVm {
    pub co2: String,
    pub energy: String,
    pub class: String,
}

/// Exhaustive match (spec §4b field-map) — never `{:?}` — so a new
/// `WarningPattern` variant fails the build here instead of silently
/// leaking Rust's Debug spelling into the report.
fn warning_pattern_str(pattern: &WarningPattern) -> &'static str {
    match pattern {
        WarningPattern::QuadraticLoop => "QuadraticLoop",
        WarningPattern::NestedLoops => "NestedLoops",
        WarningPattern::DeepCallChain => "DeepCallChain",
        WarningPattern::HiddenComplexity => "HiddenComplexity",
        WarningPattern::Recursion => "Recursion",
        WarningPattern::LargeMatch => "LargeMatch",
        WarningPattern::DeepConditional => "DeepConditional",
    }
}

fn severity_key(severity: &WarningSeverity) -> &'static str {
    match severity {
        WarningSeverity::Warning => "warning",
        WarningSeverity::Critical => "critical",
    }
}

fn severity_label(severity: &WarningSeverity) -> &'static str {
    match severity {
        WarningSeverity::Warning => "WARNING",
        WarningSeverity::Critical => "CRITICAL",
    }
}

fn to_warning_vm(warning: &ComplexityWarning) -> WarningVm {
    WarningVm {
        pattern: warning_pattern_str(&warning.pattern).to_string(),
        severity: severity_key(&warning.severity).to_string(),
        sev_label: severity_label(&warning.severity).to_string(),
        function: warning.function.clone(),
        loc: warning.location.to_string(),
        message: warning.message.clone(),
        suggestion: warning.suggestion.clone(),
    }
}

fn to_io_vm(io: &IoInLoopWarning) -> IoVm {
    IoVm {
        function: io.function.clone(),
        io_call: io.io_call.clone(),
        loc: io.location.to_string(),
    }
}

/// Builds the project-view model: banner, 8 aggregated stat tiles (S1), and
/// the project/folder/file tree with postorder-aggregated metrics (S2).
pub fn build_report_vm(graph: &FileConsumptionGraph, target: &str) -> ReportVm {
    ReportVm {
        project: ProjectVm {
            target: target.to_string(),
            tool: concat!("codeimpact v", env!("CARGO_PKG_VERSION")).to_string(),
        },
        stats: build_stats(graph),
        nodes: build_tree(graph, target),
        unmeasurable_files: build_unmeasurable_files(graph),
    }
}

/// Reuses `UnmeasurableReason`'s own `Display` (the same human-readable
/// French text the console writer already shows) — never a coefficient or a
/// re-derived string invented in this adapter (ADR-8.8/8.8a/8.8b's rule
/// generalises: present the domain's own value, don't recompute it).
fn build_unmeasurable_files(graph: &FileConsumptionGraph) -> Vec<UnmeasurableFileVm> {
    graph
        .unmeasurable_files()
        .iter()
        .map(|f| UnmeasurableFileVm {
            path: f.path.to_string_lossy().to_string(),
            reason: f.reason.to_string(),
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Project,
    Folder,
    File,
}

struct RawNode {
    name: String,
    kind: NodeKind,
    path: String,
    child_ids: Vec<String>,
    direct: u32,
    transitive: u32,
    hidden: u32,
    depth: usize,
    score: u32,
    level_rank: u8,
    functions: Vec<FunctionVm>,
    warnings: Vec<WarningVm>,
    ios: Vec<IoVm>,
    economic: Option<EconomicImpact>,
    ecological: Option<EcologicalImpact>,
}

impl RawNode {
    fn empty(name: String, kind: NodeKind, path: String) -> Self {
        Self {
            name,
            kind,
            path,
            child_ids: Vec::new(),
            direct: 0,
            transitive: 0,
            hidden: 0,
            depth: 0,
            score: 0,
            level_rank: 0,
            functions: Vec::new(),
            warnings: Vec::new(),
            ios: Vec::new(),
            economic: None,
            ecological: None,
        }
    }
}

fn kind_str(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Project => "project",
        NodeKind::Folder => "folder",
        NodeKind::File => "file",
    }
}

/// Ranks `CodeMetrics::complexity_level()`'s five states for the folder
/// "worst descendant wins" aggregation (`max` in `aggregate()`). `"none"`
/// ("nothing to measure" — zero functions) ranks LOWEST, not highest: a
/// function-less file (trait declaration, re-export `mod.rs`, pure data
/// type) carries no risk signal and must never outrank — let alone be
/// silently promoted above — an actually-measured `"critical"` file (D3,
/// #50 slice S4). The previous catch-all (`_ => 3`) mapped every unknown
/// string, including the newly-introduced `"none"`, straight into the
/// `"critical"` bucket — a function-less file would have rendered as the
/// reddest possible tag. Explicit arms for all five known states so that
/// trap cannot recur silently; `_` only guards a value `complexity_level()`
/// is not documented to produce.
fn level_rank(level: &str) -> u8 {
    match level {
        "none" => 0,
        "low" => 1,
        "moderate" => 2,
        "high" => 3,
        "critical" => 4,
        _ => 4,
    }
}

fn level_name(rank: u8) -> &'static str {
    match rank {
        0 => "none",
        1 => "low",
        2 => "moderate",
        3 => "high",
        _ => "critical",
    }
}

/// A file's tree path relative to `target` (spec §3), `/`-joined regardless
/// of platform separator. Falls back to the file's own full path when it is
/// not actually rooted under `target` (test fixtures use bare relative
/// paths with an unrelated target name).
///
/// `target_root` should be the CANONICALIZED form of the `--path` CLI
/// argument, not the raw string: `FileSystemCodeReader::list_rust_files`
/// canonicalizes every file path it returns (resolving symlinks, e.g.
/// macOS's `/tmp` -> `/private/tmp`), while `target` reaches `write_html`
/// un-canonicalized (`run_analysis.rs`'s `target.path().to_string_lossy()`).
/// Stripping the raw target against canonicalized files fails even when
/// they name the SAME directory, degenerating the tree into a single-child
/// folder chain mirroring the whole filesystem path (found dogfooding the
/// real CLI — the tech spec did not anticipate this mismatch).
fn node_id(path: &Path, target_root: &Path) -> String {
    let relative = path.strip_prefix(target_root).unwrap_or(path);
    relative
        .iter()
        .map(|c| c.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn root_name(target: &str) -> String {
    Path::new(target)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| target.to_string())
}

/// Postorder aggregation (spec §3): direct/transitive/hidden SUM, depth and
/// score MAX, level the worst (max-ordinal) descendant level. Returns the
/// computed tuple for the caller's own aggregation, and also writes it into
/// `raw[id]` as the aggregation's single source of truth.
fn aggregate(raw: &mut HashMap<String, RawNode>, id: &str) -> (u32, u32, u32, usize, u32, u8) {
    if raw[id].kind == NodeKind::File {
        let n = &raw[id];
        return (
            n.direct,
            n.transitive,
            n.hidden,
            n.depth,
            n.score,
            n.level_rank,
        );
    }

    // Sorted already (spec §3: sort_children runs before aggregate), so the
    // concat below follows the same deterministic, folders-first order the
    // tree sidebar displays — not an arbitrary insertion order.
    let child_ids = raw[id].child_ids.clone();
    let mut direct = 0u32;
    let mut transitive = 0u32;
    let mut hidden = 0u32;
    let mut depth = 0usize;
    let mut score = 0u32;
    let mut level_rank = 0u8;
    let mut warnings: Vec<WarningVm> = Vec::new();
    let mut ios: Vec<IoVm> = Vec::new();
    let mut economic: Option<EconomicImpact> = None;
    let mut ecological: Option<EcologicalImpact> = None;

    for child_id in &child_ids {
        let (cd, ct, ch, cdepth, cscore, clevel) = aggregate(raw, child_id);
        direct = direct.saturating_add(cd);
        transitive = transitive.saturating_add(ct);
        hidden = hidden.saturating_add(ch);
        depth = depth.max(cdepth);
        score = score.max(cscore);
        level_rank = level_rank.max(clevel);

        let child = &raw[child_id];
        warnings.extend(child.warnings.iter().cloned());
        ios.extend(child.ios.iter().cloned());
        economic = fold_economic(economic, child.economic.clone());
        ecological = fold_ecological(ecological, child.ecological.clone());
    }

    let node = raw.get_mut(id).expect("node id was just seeded");
    node.direct = direct;
    node.transitive = transitive;
    node.hidden = hidden;
    node.depth = depth;
    node.score = score;
    node.level_rank = level_rank;
    node.warnings = warnings;
    node.ios = ios;
    node.economic = economic;
    node.ecological = ecological;
    (direct, transitive, hidden, depth, score, level_rank)
}

/// Folds via the domain's own `EconomicImpact::Add` (spec §0 finding 2) —
/// never a coefficient invented from transitive complexity.
fn fold_economic(
    acc: Option<EconomicImpact>,
    next: Option<EconomicImpact>,
) -> Option<EconomicImpact> {
    match (acc, next) {
        (Some(a), Some(b)) => Some(a + b),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Folds via `EcologicalImpact::Add`, which re-derives the efficiency class
/// from the summed CO2 — never copied from a single child's class.
fn fold_ecological(
    acc: Option<EcologicalImpact>,
    next: Option<EcologicalImpact>,
) -> Option<EcologicalImpact> {
    match (acc, next) {
        (Some(a), Some(b)) => Some(a + b),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// Child sort order (spec §3): folders before files; folders by name asc;
/// files by score desc, tie-broken by name asc.
fn sort_children(raw: &mut HashMap<String, RawNode>) {
    let ids: Vec<String> = raw.keys().cloned().collect();
    for id in ids {
        let mut children = raw[&id].child_ids.clone();
        children.sort_by(|a, b| {
            let na = &raw[a];
            let nb = &raw[b];
            let a_is_file = na.kind == NodeKind::File;
            let b_is_file = nb.kind == NodeKind::File;
            match (a_is_file, b_is_file) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                (false, false) => na.name.cmp(&nb.name),
                (true, true) => nb.score.cmp(&na.score).then_with(|| na.name.cmp(&nb.name)),
            }
        });
        raw.get_mut(&id).expect("id came from raw.keys()").child_ids = children;
    }
}

fn pct(value: u64, scale: u64) -> u8 {
    if value == 0 || scale == 0 {
        return 0;
    }
    let raw_pct = ((value as f64 / scale as f64) * 100.0).round() as i64;
    raw_pct.clamp(5, 100) as u8
}

fn build_tree(graph: &FileConsumptionGraph, target: &str) -> Vec<NodeVm> {
    let per_file = graph.per_file_metrics();

    // Canonicalize once: real file paths (FileSystemCodeReader) are already
    // canonical, so resolving `target` the same way is what makes
    // `strip_prefix` actually match in the real CLI. Falls back to the raw
    // string when `target` does not exist on disk (fixture-based tests).
    let target_root = std::fs::canonicalize(target).unwrap_or_else(|_| PathBuf::from(target));

    let mut raw: HashMap<String, RawNode> = HashMap::new();
    raw.insert(
        String::new(),
        RawNode::empty(
            root_name(target),
            NodeKind::Project,
            "project root".to_string(),
        ),
    );

    let mut file_entries: Vec<(String, &PathBuf, &codeimpact_hexagon::analysis::CodeMetrics)> =
        per_file
            .iter()
            .map(|(path, metrics)| (node_id(path, &target_root), path, metrics))
            .collect();
    file_entries.sort_by(|a, b| a.0.cmp(&b.0));

    for (id, full_path, metrics) in &file_entries {
        let segments: Vec<&str> = id.split('/').collect();
        let mut parent_id = String::new();
        for i in 0..segments.len() - 1 {
            let folder_id = segments[..=i].join("/");
            if !raw.contains_key(&folder_id) {
                raw.insert(
                    folder_id.clone(),
                    RawNode::empty(segments[i].to_string(), NodeKind::Folder, folder_id.clone()),
                );
                raw.get_mut(&parent_id)
                    .expect("parent folder/root was seeded on a prior iteration")
                    .child_ids
                    .push(folder_id.clone());
            }
            parent_id = folder_id;
        }

        let name = segments[segments.len() - 1].to_string();
        let level = metrics.complexity_level();
        raw.insert(
            id.clone(),
            RawNode {
                name,
                kind: NodeKind::File,
                path: full_path.to_string_lossy().to_string(),
                child_ids: Vec::new(),
                direct: metrics.cyclomatic_complexity(),
                transitive: metrics.transitive_complexity(),
                hidden: metrics.hidden_complexity(),
                depth: metrics.max_call_depth(),
                score: metrics.transitive_complexity(),
                level_rank: level_rank(level),
                functions: metrics
                    .function_details()
                    .iter()
                    .map(|f| FunctionVm {
                        name: f.name().to_string(),
                        direct: f.direct(),
                        transitive: f.transitive(),
                        depth: f.call_depth(),
                        loc: f.location().to_string(),
                        in_cycle: f.in_cycle(),
                    })
                    .collect(),
                warnings: metrics.warnings().iter().map(to_warning_vm).collect(),
                ios: metrics.io_in_loops().iter().map(to_io_vm).collect(),
                economic: metrics.economic_impact().cloned(),
                ecological: metrics.ecological_impact().cloned(),
            },
        );
        raw.get_mut(&parent_id)
            .expect("parent folder/root was seeded above")
            .child_ids
            .push(id.clone());
    }

    // Sort BEFORE aggregating: folder aggregation concatenates warnings/ios
    // in child_ids order (spec §3 "depth-first, deterministic order"), and
    // that order must be the same one the tree sidebar displays.
    sort_children(&mut raw);
    aggregate(&mut raw, "");

    let scale_files = file_entries
        .iter()
        .fold((0u32, 0u32, 0u32, 0usize), |acc, (id, _, _)| {
            let n = &raw[id];
            (
                acc.0.max(n.direct),
                acc.1.max(n.transitive),
                acc.2.max(n.hidden),
                acc.3.max(n.depth),
            )
        });
    let root = &raw[""];
    let scale_folder = (root.direct, root.transitive, root.hidden);

    let mut ids: Vec<String> = raw.keys().cloned().collect();
    ids.sort();
    ids.into_iter()
        .map(|id| {
            let n = &raw[&id];
            let is_file = n.kind == NodeKind::File;
            let (scale_direct, scale_transitive, scale_hidden) = if is_file {
                (scale_files.0, scale_files.1, scale_files.2)
            } else {
                (scale_folder.0, scale_folder.1, scale_folder.2)
            };
            let scale_depth = scale_files.3;

            NodeVm {
                id: id.clone(),
                name: n.name.clone(),
                kind: kind_str(n.kind).to_string(),
                path: n.path.clone(),
                child_ids: n.child_ids.clone(),
                score: n.score,
                level: level_name(n.level_rank).to_string(),
                metrics: vec![
                    MetricVm {
                        label: "Direct complexity".to_string(),
                        value: n.direct.to_string(),
                        pct: pct(n.direct as u64, scale_direct as u64),
                    },
                    MetricVm {
                        label: "Transitive complexity".to_string(),
                        value: n.transitive.to_string(),
                        pct: pct(n.transitive as u64, scale_transitive as u64),
                    },
                    MetricVm {
                        label: "Hidden complexity".to_string(),
                        value: n.hidden.to_string(),
                        pct: pct(n.hidden as u64, scale_hidden as u64),
                    },
                    MetricVm {
                        label: "Max call depth".to_string(),
                        value: n.depth.to_string(),
                        pct: pct(n.depth as u64, scale_depth as u64),
                    },
                ],
                functions: n.functions.clone(),
                warnings: n.warnings.clone(),
                ios: n.ios.clone(),
                economic: n.economic.as_ref().map(|e| EconomicVm {
                    cpu: format_dollars(e.cpu_cost_microdollars()),
                    memory: format_memory(e.memory_bytes()),
                    total: format_dollars(e.total_cost_microdollars()),
                    level: e.level().to_string(),
                }),
                ecological: n.ecological.as_ref().map(|e| EcologicalVm {
                    co2: format!("{:.3} g", e.co2_grams()),
                    energy: format_energy(e.energy_joules()),
                    class: e.efficiency_class().label().to_string(),
                }),
            }
        })
        .collect()
}

/// Renders the 9 project stat tiles. Every value comes straight from
/// `ProjectMetrics` (`FileConsumptionGraph::aggregated_metrics()`, the
/// single source of truth) — no `.len()`/`.sum()`/`.count()` here (#46/#49
/// tech spec §11): a tile that recomputes its own aggregate is exactly how
/// the JSON and HTML writers diverged in the first place (ADR-0012).
fn build_stats(graph: &FileConsumptionGraph) -> Vec<StatVm> {
    let aggregated = graph.aggregated_metrics();

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
            value: aggregated.total_files.to_string(),
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
            sub: format!("{} hidden", aggregated.total_hidden_complexity),
        },
        StatVm {
            label: "Warnings".to_string(),
            value: aggregated.total_warnings.to_string(),
            sub: format!("{} critical", aggregated.critical_warnings),
        },
        StatVm {
            label: "I/O in loops".to_string(),
            value: aggregated.total_io_in_loops.to_string(),
            sub: "in loops".to_string(),
        },
        StatVm {
            label: "Max depth".to_string(),
            value: aggregated.max_call_depth.to_string(),
            sub: "call chain".to_string(),
        },
        StatVm {
            label: "Hotspots".to_string(),
            value: aggregated.hotspot_files.to_string(),
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

/// Energy formatted for a stat-tile sub (no kWh parenthetical, unlike the
/// detail-pane's full `humanize::format_energy`) — mirrors the reference
/// `fmtEnergy(...).split(' (')[0]` shape.
fn format_energy_short(joules: f64) -> String {
    let full = super::super::humanize::format_energy(joules);
    full.split(" (").next().unwrap_or(&full).to_string()
}
