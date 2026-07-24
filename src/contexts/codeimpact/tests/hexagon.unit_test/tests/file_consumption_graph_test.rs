use std::path::PathBuf;

use codeimpact_hexagon::analysis::{
    CodeLocation, CodeMetrics, EcologicalImpact, EconomicImpact, EfficiencyClass,
    FileConsumptionGraph, FileDependency, FunctionDetail, Language, LanguageCapabilities,
    MetricSupport, UnmeasurableFile, UnmeasurableReason,
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
//  22. hotspot_files_counts_only_critical — mixed critical/high/low files,
//      hotspot_files counts only the "critical" ones (#46/#49 follow-up)
//  23. median_differs_from_total_band — many tiny files + a couple huge
//      ones: total lands "critical", median lands "low" (#60 — the total is
//      off the per-file scale, the median stays on it)
//  24. median_even_count_averages_middle_two_round_half_up — 4 files,
//      middle two average to a .5 → rounds up
//  25. median_empty_project_reports_none — zero measured files → median is
//      0 and complexity_level() is "none", not the misleadingly clean "low"
//      complexity_level_for(0) would give
//  26. aggregated_metrics_metric_support_folds_mixed_capabilities_to_degraded
//      — Rust (no capabilities) + C# (io_in_loops Unsupported) → aggregate
//      io_in_loops is Degraded, not silently Supported/Unsupported (#89 S1)
//  27. aggregated_metrics_metric_support_all_unsupported_stays_unsupported
//      — every file Unsupported for io_in_loops (pure-C#, no Rust file in
//      the mix) → aggregate io_in_loops is Unsupported (#89 S1)
//
// total_dependencies / max_depth:
//  18. graph_with_chain — correct depth and count
//  19. graph_with_cycle — depth stops at cycle

// D3 (#50 slice S4): complexity_level() now reports "none" when
// function_details is empty, regardless of cc. These fixtures are about
// graph/aggregation math, not the D3 zero-function state, so they carry one
// measured function each — otherwise `aggregated_metrics_hotspot_files_
// counts_only_critical` below could never observe a "critical" file (every
// fixture would read "none").
fn make_metrics(cc: u32, tc: u32) -> CodeMetrics {
    CodeMetrics::with_call_graph(
        cc,
        tc,
        0,
        vec![],
        vec![FunctionDetail::new(
            "f".to_string(),
            CodeLocation::new("f.rs".into(), 1, 1),
            cc,
            0,
            0,
            false,
        )],
    )
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
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("b.rs"),
            to: path("c.rs"),
        },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    assert_eq!(graph.total_dependencies(), 2);
    assert_eq!(graph.max_depth(), 3);
    assert!(graph.files_with_cycles().is_empty());

    let chain = graph.consumption_chain(&path("a.rs"));
    let names: Vec<&str> = chain
        .iter()
        .map(|p| p.file_stem().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a", "b", "c"]);
}

#[test]
fn cycle_a_b_a() {
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(2, 2)),
    ];
    let deps = vec![
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("b.rs"),
            to: path("a.rs"),
        },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let cycles = graph.files_with_cycles();
    assert_eq!(cycles.len(), 2);
    let names: Vec<&str> = cycles
        .iter()
        .map(|p| p.file_stem().unwrap().to_str().unwrap())
        .collect();
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
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("b.rs"),
            to: path("c.rs"),
        },
        FileDependency {
            from: path("c.rs"),
            to: path("a.rs"),
        },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    assert_eq!(graph.files_with_cycles().len(), 3);
}

#[test]
fn missing_from_node_returns_error() {
    let files = vec![(path("a.rs"), make_metrics(1, 1))];
    let deps = vec![FileDependency {
        from: path("unknown.rs"),
        to: path("a.rs"),
    }];
    let result = FileConsumptionGraph::build(&files, deps);
    assert!(result.is_err());
}

#[test]
fn missing_to_node_returns_error() {
    let files = vec![(path("a.rs"), make_metrics(1, 1))];
    let deps = vec![FileDependency {
        from: path("a.rs"),
        to: path("unknown.rs"),
    }];
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
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("a.rs"),
            to: path("c.rs"),
        },
        FileDependency {
            from: path("b.rs"),
            to: path("d.rs"),
        },
        FileDependency {
            from: path("c.rs"),
            to: path("d.rs"),
        },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    assert_eq!(graph.total_dependencies(), 4);
    // max_depth = a→b→d (3) or a→c→d (3)
    assert_eq!(graph.max_depth(), 3);

    let chain = graph.consumption_chain(&path("a.rs"));
    // Chain should include a, its deps, and their transitive deps
    let names: Vec<&str> = chain
        .iter()
        .map(|p| p.file_stem().unwrap().to_str().unwrap())
        .collect();
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
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("x.rs"),
            to: path("y.rs"),
        },
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
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("b.rs"),
            to: path("c.rs"),
        },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let chain = graph.consumption_chain(&path("a.rs"));
    let names: Vec<&str> = chain
        .iter()
        .map(|p| p.file_stem().unwrap().to_str().unwrap())
        .collect();
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
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("b.rs"),
            to: path("a.rs"),
        },
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
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("b.rs"),
            to: path("a.rs"),
        },
    ];
    let graph = FileConsumptionGraph::build(&files, deps).unwrap();
    let cycles = graph.files_with_cycles();
    assert_eq!(cycles.len(), 2);
    // Should be sorted
    let names: Vec<&str> = cycles
        .iter()
        .map(|p| p.file_stem().unwrap().to_str().unwrap())
        .collect();
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

// #60: the project `complexity_level` was reading `complexity_level_for`
// (the PER-FILE scale, ceilings 10/20/40) against the PROJECT TOTAL — a
// number that is off that scale by construction, so it reads "critical" for
// almost any real project. The fix judges the scale against the MEDIAN
// per-file complexity instead, which stays on the scale it was calibrated
// against.
#[test]
fn median_differs_from_total_band() {
    // 9 tiny files (cc=2) + 2 huge files (cc=200).
    // total = 9*2 + 2*200 = 418 -> complexity_level_for(418) == "critical"
    // sorted per-file values (11 of them): [2,2,2,2,2,2,2,2,2,200,200]
    // median (6th of 11, odd count) = 2 -> complexity_level_for(2) == "low"
    let mut files: Vec<(PathBuf, CodeMetrics)> = (0..9)
        .map(|i| (path(&format!("tiny{i}.rs")), make_metrics(2, 2)))
        .collect();
    files.push((path("huge1.rs"), make_metrics(200, 200)));
    files.push((path("huge2.rs"), make_metrics(200, 200)));

    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let metrics = graph.aggregated_metrics();

    assert_eq!(metrics.total_cyclomatic_complexity, 418);
    assert_eq!(
        metrics.median_file_cyclomatic_complexity, 2,
        "median must reflect the typical file, not be dragged up by the two huge outliers"
    );
    assert_eq!(metrics.complexity_level(), "low");
}

#[test]
fn median_even_count_averages_middle_two_round_half_up() {
    // 4 files, cc = [1, 3, 4, 7]. Middle two are 3 and 4 -> average 3.5,
    // rounds half-up to 4.
    let files = vec![
        (path("a.rs"), make_metrics(1, 1)),
        (path("b.rs"), make_metrics(3, 3)),
        (path("c.rs"), make_metrics(4, 4)),
        (path("d.rs"), make_metrics(7, 7)),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let metrics = graph.aggregated_metrics();

    assert_eq!(metrics.median_file_cyclomatic_complexity, 4);
}

#[test]
fn median_empty_project_reports_none() {
    let graph = FileConsumptionGraph::build(&[], vec![]).unwrap();
    let metrics = graph.aggregated_metrics();

    assert_eq!(metrics.median_file_cyclomatic_complexity, 0);
    assert_eq!(metrics.complexity_level(), "none");
}

// #89 S1 (ADR-0021 T3b follow-up): the project stat tiles must read the
// SAME honest degradation the per-file detail already had — a mixed
// Rust+C# project must not silently report io_in_loops as fully
// Supported (nor flatly Unsupported: the Rust file DID measure it).
#[test]
fn aggregated_metrics_metric_support_folds_mixed_capabilities_to_degraded() {
    let rust = make_metrics(5, 5);
    let csharp = make_metrics(3, 3).with_capabilities(
        LanguageCapabilities::all_supported(Language::CSharp)
            .with_io_in_loops(MetricSupport::Unsupported),
    );
    let files = vec![(path("a.rs"), rust), (path("b.cs"), csharp)];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();

    let metrics = graph.aggregated_metrics();

    match metrics.metric_support.io_in_loops() {
        MetricSupport::Degraded(reason) => {
            assert_eq!(reason, "partial: 1/2 files measured this metric");
        }
        other => panic!(
            "a mixed Rust+C# project must fold io_in_loops to Degraded, got {:?}",
            other
        ),
    }
}

#[test]
fn aggregated_metrics_metric_support_all_unsupported_stays_unsupported() {
    let unsupported_csharp = || {
        make_metrics(1, 1).with_capabilities(
            LanguageCapabilities::all_supported(Language::CSharp)
                .with_io_in_loops(MetricSupport::Unsupported),
        )
    };
    let files = vec![
        (path("a.cs"), unsupported_csharp()),
        (path("b.cs"), unsupported_csharp()),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();

    let metrics = graph.aggregated_metrics();

    assert_eq!(
        *metrics.metric_support.io_in_loops(),
        MetricSupport::Unsupported
    );
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
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("b.rs"),
            to: path("c.rs"),
        },
        FileDependency {
            from: path("c.rs"),
            to: path("d.rs"),
        },
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
        FileDependency {
            from: path("a.rs"),
            to: path("b.rs"),
        },
        FileDependency {
            from: path("b.rs"),
            to: path("c.rs"),
        },
        FileDependency {
            from: path("c.rs"),
            to: path("a.rs"),
        },
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
    let economic = pm
        .total_economic_impact
        .expect("should have economic impact");
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
    let ecological = pm
        .total_ecological_impact
        .expect("should have ecological impact");
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
    let economic = pm
        .total_economic_impact
        .expect("should have economic impact");
    let ecological = pm
        .total_ecological_impact
        .expect("should have ecological impact");
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

// #46/#49 follow-up (QA gap): hotspot_files is a new branch introduced when
// aggregated_metrics() became the single source of truth for the "critical"
// count. Every other fixture in this file uses cc <= 10, so the "critical"
// branch (cc > 40) was never exercised. This fixture mixes critical, high,
// and low files so the count discriminates against both "critical" ->
// "high" and a deleted increment.
#[test]
fn aggregated_metrics_hotspot_files_counts_only_critical() {
    let files = vec![
        (path("a.rs"), make_metrics(45, 45)), // critical (> 40)
        (path("b.rs"), make_metrics(25, 25)), // high (21..=40)
        (path("c.rs"), make_metrics(35, 35)), // high (21..=40)
        (path("d.rs"), make_metrics(5, 5)),   // low (0..=10)
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let pm = graph.aggregated_metrics();

    assert_eq!(pm.hotspot_files, 1);
}

// #46/#49 (ADR-0012): total_hidden_complexity is additive across files/
// functions, never re-derived by subtracting the two file-level aggregates
// (ΣT - ΣC, which would give max(0, 9-8) = 1 on this fixture instead of 3).
#[test]
fn project_hidden_equals_sum_of_file_hidden() {
    let files = vec![
        (
            path("a.rs"),
            CodeMetrics::with_call_graph(
                6,
                8,
                1,
                vec![],
                vec![
                    FunctionDetail::new(
                        "f1".into(),
                        CodeLocation::new("a.rs".into(), 1, 1),
                        2,
                        3,
                        1,
                        false,
                    ),
                    FunctionDetail::new(
                        "f2".into(),
                        CodeLocation::new("a.rs".into(), 2, 1),
                        3,
                        0,
                        0,
                        false,
                    ),
                ],
            ),
        ),
        (
            path("b.rs"),
            CodeMetrics::with_call_graph(
                2,
                1,
                0,
                vec![],
                vec![FunctionDetail::new(
                    "g".into(),
                    CodeLocation::new("b.rs".into(), 1, 1),
                    1,
                    0,
                    0,
                    false,
                )],
            ),
        ),
    ];
    let graph = FileConsumptionGraph::build(&files, vec![]).unwrap();
    let pm = graph.aggregated_metrics();

    assert_eq!(pm.total_cyclomatic_complexity, 8);
    assert_eq!(pm.total_transitive_complexity, 9);
    assert_eq!(pm.total_hidden_complexity, 3);
}

// D3 (#50 slice S4), test case 19 — a file that failed to parse or read is
// a THIRD state, distinct from both "measured" and "nothing to measure"
// (complexity_level() == "none"): it never even reaches CodeMetrics, so it
// must be tracked separately and excluded from every sum, not silently
// dropped (the ADR-0010 bug one layer up: run_analysis.rs used to drop a
// failing file from the report entirely, undercounting total_files).
// Test List:
// 20. with_unmeasurable_files stores them; unmeasurable_files() returns
//     them with their path and reason
// 21. aggregated_metrics().unmeasurable_files counts them, total_files
//     keeps counting only MEASURED files, sums are untouched by them

#[test]
fn with_unmeasurable_files_stores_and_returns_them() {
    let files = vec![(path("a.rs"), make_metrics(5, 5))];
    let unmeasurable = vec![UnmeasurableFile {
        path: path("bad.rs"),
        reason: UnmeasurableReason::SourceUnparseable,
    }];
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_unmeasurable_files(unmeasurable);

    let stored = graph.unmeasurable_files();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].path, path("bad.rs"));
    assert_eq!(stored[0].reason, UnmeasurableReason::SourceUnparseable);
}

#[test]
fn aggregated_metrics_counts_unmeasurable_files_separately_from_total_files() {
    let files = vec![(path("a.rs"), make_metrics(5, 10))];
    let unmeasurable = vec![UnmeasurableFile {
        path: path("bad.rs"),
        reason: UnmeasurableReason::SourceUnparseable,
    }];
    let graph = FileConsumptionGraph::build(&files, vec![])
        .unwrap()
        .with_unmeasurable_files(unmeasurable);
    let pm = graph.aggregated_metrics();

    assert_eq!(
        pm.total_files, 1,
        "total_files keeps counting MEASURED files only"
    );
    assert_eq!(pm.unmeasurable_files, 1);
    // bad.rs's (nonexistent) numbers entered no sum: the sum is exactly
    // a.rs's own complexity, untouched by the unmeasurable entry.
    assert_eq!(pm.total_cyclomatic_complexity, 5);
    assert_eq!(pm.total_transitive_complexity, 10);
}
