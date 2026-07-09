use std::path::PathBuf;

use codeimpact_hexagon::analysis::{
    CodeMetrics, EcologicalImpact, EconomicImpact, EfficiencyClass, FileConsumptionGraph,
    FileDependency,
};

// ── Test List ──────────────────────────────────────────────────────────
// FileConsumptionGraph::build:
//   1. empty — no files, no deps → graph with nothing
//   2. single_file_no_deps — one file, no deps → one node, no edges
//   3. chain — A→B→C → chain correctly computed, max_depth=3
//   4. cycle — A→B→A → cycle detected, files_with_cycles returns [A, B]
//   5. large_cycle — A→B→C→A → cycle with 3 nodes
//   6. missing_from_node — dependency with unknown 'from' → error
//   7. missing_to_node — dependency with unknown 'to' → error
//   8. diamond — A→B, A→C, B→D → chain from A goes through both paths
//   9. disconnected — two independent sub-graphs
//
// consumption_chain:
//  10. linear — chain from A: A→B→C → returns [A, B, C]
//  11. no_deps — file with no deps → returns [file]
//  12. cycle_chain — file in cycle → chain includes cycle nodes
//  13. unknown_file — file not in graph → returns empty vec
//
// files_with_cycles:
//  14. no_cycles → empty vec
//  15. one_cycle → returns cycle nodes sorted
//
// aggregated_metrics:
//  16. sum_of_totals — correct totals from all files
//  17. empty_project — zero metrics
//  18. aggregated_economic_impact — sums economic impacts from all files
//  19. aggregated_ecological_impact — sums ecological impacts from all files
//  20. aggregated_impacts_some_missing — skip files without impacts
//  21. aggregated_impacts_all_missing — None when no file has impacts
//
// total_dependencies / max_depth:
//  18. graph_with_chain — correct depth and count
//  19. graph_with_cycle — depth stops at cycle

fn make_metrics(cc: u32, tc: u32) -> CodeMetrics {
    CodeMetrics::with_call_graph(cc, tc, 0, vec![], vec![])
}

fn path(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn empty_files_no_deps() {
    let graph = FileConsumptionGraph::build(&[], vec![]).unwrap();
    assert_eq!(graph.total_dependencies(), 0);
    assert_eq!(graph.max_depth(), 0);
    assert!(graph.files_with_cycles().is_empty());
    assert!(graph.per_file_metrics().is_empty());
}

#[test]
fn single_file_no_deps() {
    let files = vec![(path("a.rs"), make_metrics(5, 5))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    assert_eq!(graph.total_dependencies(), 0);
    assert_eq!(graph.max_depth(), 1);
    assert!(graph.files_with_cycles().is_empty());
    assert_eq!(graph.per_file_metrics().len(), 1);
}

#[test]
fn chain_a_to_b_to_c() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(2, 2)),
        (path("c.rs"), make_metrics(3, 3)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("b.rs"), to: path("c.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    assert_eq!(graph.total_dependencies(), 2);
    assert_eq!(graph.max_depth(), 3);
    assert!(graph.files_with_cycles().is_empty());

    let chain = graph.consumption_chain(&path("a.rs"));
    let names: Vec<&str> = chain.iter().map(|p| p.file_stem().unwrap().to_str().unwrap()).collect();
    assert_eq!(names, vec!["a", "b", "c"]);
}

#[test]
fn cycle_a_b_a() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(2, 2)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("b.rs"), to: path("a.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let cycles = graph.files_with_cycles();
    assert_eq!(cycles.len(), 2);
    let names: Vec<&str> = cycles.iter().map(|p| p.file_stem().unwrap().to_str().unwrap()).collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
}

#[test]
fn large_cycle_three_nodes() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(1, 1)),
        (path("c.rs"), make_metrics(1, 1)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("b.rs"), to: path("c.rs") },
        FileDependency { from: path("c.rs"), to: path("a.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    assert_eq!(graph.files_with_cycles().len(), 3);
}

#[test]
fn missing_from_node_returns_error() {
    let files = vec![(path("a.rs"), make_metrics(1, 1))];
    let deps = vec![
        FileDependency { from: path("unknown.rs"), to: path("a.rs") },
    ];
    let result = FileConsumptionGraph::build(&files, deps);
    assert!(result.is_err());
}

#[test]
fn missing_to_node_returns_error() {
    let files = vec![(path("a.rs"), make_metrics(1, 1))];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("unknown.rs") },
    ];
    let result = FileConsumptionGraph::build(&files, deps);
    assert!(result.is_err());
}

#[test]
fn diamond_graph() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(1, 1)),
        (path("c.rs"), make_metrics(1, 1)),
        (path("d.rs"), make_metrics(1, 1)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("a.rs"), to: path("c.rs") },
        FileDependency { from: path("b.rs"), to: path("d.rs") },
        FileDependency { from: path("c.rs"), to: path("d.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    assert_eq!(graph.total_dependencies(), 4);
    // max_depth = a→b→d (3) or a→c→d (3)
    assert_eq!(graph.max_depth(), 3);

    let chain = graph.consumption_chain(&path("a.rs"));
    // Chain should include a, its deps, and their transitive deps
    let names: Vec<&str> = chain.iter().map(|p| p.file_stem().unwrap().to_str().unwrap()).collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
    assert!(names.contains(&"c"));
    assert!(names.contains(&"d"));
}

#[test]
fn disconnected_subgraphs() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(1, 1)),
        (path("x.rs"), make_metrics(1, 1)),
        (path("y.rs"), make_metrics(1, 1)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("x.rs"), to: path("y.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    assert_eq!(graph.total_dependencies(), 2);

    let chain_a = graph.consumption_chain(&path("a.rs"));
    assert_eq!(chain_a.len(), 2);
    let chain_x = graph.consumption_chain(&path("x.rs"));
    assert_eq!(chain_x.len(), 2);
}

#[test]
fn consumption_chain_linear() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(1, 1)),
        (path("c.rs"), make_metrics(1, 1)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("b.rs"), to: path("c.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let chain = graph.consumption_chain(&path("a.rs"));
    let names: Vec<&str> = chain.iter().map(|p| p.file_stem().unwrap().to_str().unwrap()).collect();
    assert_eq!(names, vec!["a", "b", "c"]);
}

#[test]
fn consumption_chain_no_deps() {
    let files = vec![(path("a.rs"), make_metrics(1, 1))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let chain = graph.consumption_chain(&path("a.rs"));
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0], path("a.rs"));
}

#[test]
fn consumption_chain_cycle() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(1, 1)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("b.rs"), to: path("a.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let chain = graph.consumption_chain(&path("a.rs"));
    // Chain shows full cycle: A → B → A
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0], path("a.rs"));
    assert_eq!(chain[1], path("b.rs"));
    assert_eq!(chain[2], path("a.rs"));
}

#[test]
fn consumption_chain_unknown_file() {
    let files = vec![(path("a.rs"), make_metrics(1, 1))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let chain = graph.consumption_chain(&path("unknown.rs"));
    assert!(chain.is_empty());
}

#[test]
fn files_with_cycles_no_cycles() {
    let files = vec![(path("a.rs"), make_metrics(1, 1))];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    assert!(graph.files_with_cycles().is_empty());
}

#[test]
fn files_with_cycles_one_cycle() {
    let files = vec![
        (path("b.rs"), make_metrics(1, 1)),
        (path("a.rs"), make_metrics(1, 1)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("b.rs"), to: path("a.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let cycles = graph.files_with_cycles();
    assert_eq!(cycles.len(), 2);
    // Should be sorted
    let names: Vec<&str> = cycles.iter().map(|p| p.file_stem().unwrap().to_str().unwrap()).collect();
    assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn aggregated_metrics_sums_totals() {
    let files = vec![
        (path("a.rs"), make_metrics(5, 10)),
        (path("b.rs"), make_metrics(3, 7)),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let metrics = graph.aggregated_metrics();
    assert_eq!(metrics.total_files, 2);
    assert_eq!(metrics.total_cyclomatic_complexity, 8);
    assert_eq!(metrics.total_transitive_complexity, 17);
}

#[test]
fn aggregated_metrics_empty_project() {
    let graph = FileConsumptionGraph::build(&[], vec![]).unwrap();
    let metrics = graph.aggregated_metrics();
    assert_eq!(metrics.total_files, 0);
    assert_eq!(metrics.total_cyclomatic_complexity, 0);
    assert_eq!(metrics.total_transitive_complexity, 0);
    assert_eq!(metrics.max_call_depth, 0);
    assert!(metrics.files_with_cycles.is_empty());
}

#[test]
fn max_depth_chain() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(1, 1)),
        (path("c.rs"), make_metrics(1, 1)),
        (path("d.rs"), make_metrics(1, 1)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("b.rs"), to: path("c.rs") },
        FileDependency { from: path("c.rs"), to: path("d.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    assert_eq!(graph.max_depth(), 4);
}

#[test]
fn max_depth_with_cycle() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(1, 1)),
        (path("c.rs"), make_metrics(1, 1)),
    ];
    let deps = vec![
        FileDependency { from: path("a.rs"), to: path("b.rs") },
        FileDependency { from: path("b.rs"), to: path("c.rs") },
        FileDependency { from: path("c.rs"), to: path("a.rs") },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    // Depth stops at cycle nodes
    assert!(graph.max_depth() >= 1);
}

#[test]
fn per_file_metrics_returns_map() {
    let files = vec![
        (path("a.rs"), make_metrics(5, 5)),
        (path("b.rs"), make_metrics(3, 3)),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let map = graph.per_file_metrics();
    assert_eq!(map.len(), 2);
    assert!(map.contains_key(&path("a.rs")));
    assert!(map.contains_key(&path("b.rs")));
}

#[test]
fn aggregated_economic_impact_sums_across_files() {
    let files = vec![
        (
            path("a.rs"),
            make_metrics(5, 10).with_economic_impact(EconomicImpact::new(10.0, 100, 10.5, "low")),
        ),
        (
            path("b.rs"),
            make_metrics(3, 7).with_economic_impact(EconomicImpact::new(20.0, 200, 21.0, "high")),
        ),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let pm = graph.aggregated_metrics();
    let economic = pm.total_economic_impact.expect("should have economic impact");
    assert!((economic.total_cost_microdollars() - 31.5).abs() < 1e-9);
    assert!((economic.cpu_cost_microdollars() - 30.0).abs() < 1e-9);
    assert_eq!(economic.memory_bytes(), 300);
}

#[test]
fn aggregated_ecological_impact_sums_across_files() {
    let files = vec![
        (
            path("a.rs"),
            make_metrics(5, 10)
                .with_economic_impact(EconomicImpact::new(10.0, 100, 10.5, "low"))
                .with_ecological_impact(EcologicalImpact::new(1.0, 9000.0, EfficiencyClass::B)),
        ),
        (
            path("b.rs"),
            make_metrics(3, 7)
                .with_economic_impact(EconomicImpact::new(20.0, 200, 21.0, "high"))
                .with_ecological_impact(EcologicalImpact::new(2.0, 18000.0, EfficiencyClass::D)),
        ),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let pm = graph.aggregated_metrics();
    let ecological = pm.total_ecological_impact.expect("should have ecological impact");
    assert!((ecological.co2_grams() - 3.0).abs() < 1e-9);
    assert!((ecological.energy_joules() - 27000.0).abs() < 1e-9);
}

#[test]
fn aggregated_impacts_some_missing_skips_none() {
    let files = vec![
        (
            path("a.rs"),
            make_metrics(5, 10)
                .with_economic_impact(EconomicImpact::new(10.0, 100, 10.5, "low"))
                .with_ecological_impact(EcologicalImpact::new(1.0, 9000.0, EfficiencyClass::B)),
        ),
        (path("b.rs"), make_metrics(3, 7)), // no impacts
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let pm = graph.aggregated_metrics();
    let economic = pm.total_economic_impact.expect("should have economic impact");
    let ecological = pm.total_ecological_impact.expect("should have ecological impact");
    assert!((economic.total_cost_microdollars() - 10.5).abs() < 1e-9);
    assert!((ecological.co2_grams() - 1.0).abs() < 1e-9);
}

#[test]
fn aggregated_impacts_all_missing_returns_none() {
    let files = vec![
        (path("a.rs"), make_metrics(5, 10)),
        (path("b.rs"), make_metrics(3, 7)),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let pm = graph.aggregated_metrics();
    assert!(pm.total_economic_impact.is_none());
    assert!(pm.total_ecological_impact.is_none());
}