use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

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
    pub nodes: Vec<NodeVm>,
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
}

#[derive(serde::Serialize)]
pub struct MetricVm {
    pub label: String,
    pub value: String,
    pub pct: u8,
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
    }
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
}

fn kind_str(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Project => "project",
        NodeKind::Folder => "folder",
        NodeKind::File => "file",
    }
}

fn level_rank(level: &str) -> u8 {
    match level {
        "low" => 0,
        "moderate" => 1,
        "high" => 2,
        _ => 3,
    }
}

fn level_name(rank: u8) -> &'static str {
    match rank {
        0 => "low",
        1 => "moderate",
        2 => "high",
        _ => "critical",
    }
}

/// A file's tree path relative to `target` (spec §3), `/`-joined regardless
/// of platform separator. Falls back to the file's own full path when it is
/// not actually rooted under `target` (test fixtures use bare relative
/// paths with an unrelated target name).
fn node_id(path: &Path, target: &str) -> String {
    let relative = path.strip_prefix(Path::new(target)).unwrap_or(path);
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
        return (n.direct, n.transitive, n.hidden, n.depth, n.score, n.level_rank);
    }

    let child_ids = raw[id].child_ids.clone();
    let mut direct = 0u32;
    let mut transitive = 0u32;
    let mut hidden = 0u32;
    let mut depth = 0usize;
    let mut score = 0u32;
    let mut level_rank = 0u8;
    for child_id in &child_ids {
        let (cd, ct, ch, cdepth, cscore, clevel) = aggregate(raw, child_id);
        direct = direct.saturating_add(cd);
        transitive = transitive.saturating_add(ct);
        hidden = hidden.saturating_add(ch);
        depth = depth.max(cdepth);
        score = score.max(cscore);
        level_rank = level_rank.max(clevel);
    }

    let node = raw.get_mut(id).expect("node id was just seeded");
    node.direct = direct;
    node.transitive = transitive;
    node.hidden = hidden;
    node.depth = depth;
    node.score = score;
    node.level_rank = level_rank;
    (direct, transitive, hidden, depth, score, level_rank)
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

    let mut raw: HashMap<String, RawNode> = HashMap::new();
    raw.insert(
        String::new(),
        RawNode {
            name: root_name(target),
            kind: NodeKind::Project,
            path: "project root".to_string(),
            child_ids: Vec::new(),
            direct: 0,
            transitive: 0,
            hidden: 0,
            depth: 0,
            score: 0,
            level_rank: 0,
        },
    );

    let mut file_entries: Vec<(String, &PathBuf, &codeimpact_hexagon::analysis::CodeMetrics)> =
        per_file
            .iter()
            .map(|(path, metrics)| (node_id(path, target), path, metrics))
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
                    RawNode {
                        name: segments[i].to_string(),
                        kind: NodeKind::Folder,
                        path: folder_id.clone(),
                        child_ids: Vec::new(),
                        direct: 0,
                        transitive: 0,
                        hidden: 0,
                        depth: 0,
                        score: 0,
                        level_rank: 0,
                    },
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
            },
        );
        raw.get_mut(&parent_id)
            .expect("parent folder/root was seeded above")
            .child_ids
            .push(id.clone());
    }

    aggregate(&mut raw, "");
    sort_children(&mut raw);

    let scale_files = file_entries.iter().fold((0u32, 0u32, 0u32, 0usize), |acc, (id, _, _)| {
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
            }
        })
        .collect()
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
