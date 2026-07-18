use std::cell::Cell;
use std::ops::ControlFlow;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use codeimpact_hexagon::analysis::source_guard;
use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::DependencyContext;
use codeimpact_hexagon::analysis::IoClassification;
use codeimpact_hexagon::analysis::Language;
use codeimpact_hexagon::analysis::LanguageCapabilities;
use codeimpact_hexagon::analysis::LoopCall;
use codeimpact_hexagon::analysis::ParsedFunction;
use codeimpact_hexagon::analysis::UnmeasurableReason;
use tree_sitter::Node;
use tree_sitter::ParseOptions;
use tree_sitter::Parser;
use tree_sitter::Point;
use tree_sitter::Query;
use tree_sitter::QueryCursor;
use tree_sitter::QueryCursorOptions;
use tree_sitter::StreamingIterator;

use super::io_signatures;
use super::language_profile::LanguageProfile;

/// Wall-clock budget for BOTH the parse and the query stage (US16 T2, Q2
/// spike). The spike proved tree-sitter's C parser/query machinery never
/// aborts the process even at extreme nesting (500k-deep, 64 KiB thread
/// stack — zero crashes) — the crash risk this slice actually guards
/// against is a NATIVE-recursive post-processor, which
/// `assign_captures_to_functions` below is not (iterative containment
/// checks only). What the spike DID show is that query matching can take
/// minutes on an adversarial-but-size-capped (1 MB, `source_guard`) input,
/// so this budget bounds wall-clock time, not stack depth — same spirit as
/// ADR-0015's canary timeout, tighter because this blocks the calling
/// thread directly instead of an isolated subprocess.
const PARSE_QUERY_BUDGET: Duration = Duration::from_secs(5);

/// Depth cap for the nesting-count helpers below — defense in depth, not a
/// load-bearing safety property (Q2): the containment counts are already
/// iterative (nested `for` loops, never a recursive call), so nothing here
/// can overflow the native stack regardless of this cap. It exists to keep
/// a pathological function's O(depth) inner counting loop bounded.
const MAX_NESTING_DEPTH: u32 = 2_000;

/// Parses C# via `tree-sitter` (US16 T2) — the second `CodeParser` adapter
/// ADR-0018 opened the hexagon up for. `resolve_dependencies` returns an
/// empty result in this slice (C# `using`/namespace resolution is out of
/// T2's scope, tracked as a T3+ follow-up); `parse` runs a `.scm` query
/// over the file and assigns each capture to its innermost enclosing
/// function by byte range (`assign_captures_to_functions`).
pub struct TreeSitterCodeParser {
    language: Language,
    profile: LanguageProfile,
}

impl TreeSitterCodeParser {
    pub fn csharp() -> Self {
        Self {
            language: Language::CSharp,
            profile: LanguageProfile {
                grammar: tree_sitter_c_sharp::LANGUAGE.into(),
                scm: include_str!("queries/csharp.scm"),
                io_table: io_signatures::csharp::IO_PREFIXES,
            },
        }
    }
}

impl CodeParser for TreeSitterCodeParser {
    fn language(&self) -> Language {
        self.language
    }

    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities::all_supported(self.language)
    }

    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError> {
        source_guard::check_admissible(source).map_err(AnalysisError::Unmeasurable)?;
        parse_source(&self.profile, source)
    }

    /// T2 scope note (tech spec): C# `using`/namespace resolution is out of
    /// scope for this slice. An empty result matches ADR-0018's own
    /// contract for `resolve_dependencies` — "a dependency that cannot be
    /// resolved... is simply absent from the result, never an error" — this
    /// adapter simply never looks for any yet.
    fn resolve_dependencies(
        &self,
        _source: &str,
        _ctx: &DependencyContext,
    ) -> Result<Vec<PathBuf>, AnalysisError> {
        Ok(vec![])
    }
}

/// Runs the parse+query+assign pipeline inside `catch_unwind` (Q2: defense
/// against an ordinary Rust panic in our own extraction code or a grammar
/// edge case — NOT a native stack-overflow guard, the spike showed that
/// risk does not apply to tree-sitter's own machinery here). A cancelled
/// budget (`run_pipeline` returning `None`) and a caught panic both map to
/// the SAME `SourceTooComplex` reason: either way, this file could not be
/// safely measured within budget, and ADR-0010 forbids reporting a
/// partial/misleading result as if it were complete.
fn parse_source(
    profile: &LanguageProfile,
    source: &str,
) -> Result<Vec<ParsedFunction>, AnalysisError> {
    let grammar = profile.grammar.clone();
    let query_source = profile.scm;
    let owned_source = source.to_string();

    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        run_pipeline(&grammar, query_source, &owned_source)
    }));

    match outcome {
        Ok(Some(functions)) => Ok(functions),
        Ok(None) | Err(_) => Err(AnalysisError::Unmeasurable(
            UnmeasurableReason::SourceTooComplex,
        )),
    }
}

/// Parses `source`, runs the metric-extraction query, and assigns every
/// capture to its innermost enclosing function — `None` when either stage
/// is cancelled by `PARSE_QUERY_BUDGET`.
fn run_pipeline(
    grammar: &tree_sitter::Language,
    query_source: &str,
    source: &str,
) -> Option<Vec<ParsedFunction>> {
    let deadline = Instant::now() + PARSE_QUERY_BUDGET;
    let cancelled = Cell::new(false);

    let mut parser = Parser::new();
    parser
        .set_language(grammar)
        .expect("grammar must load — a hardcoded, known-good constant");

    let bytes = source.as_bytes();
    let mut read = |byte_offset: usize, _point: Point| -> &[u8] { bytes.get(byte_offset..).unwrap_or(&[]) };
    let mut parse_progress = |_state: &tree_sitter::ParseState| -> ControlFlow<()> {
        if Instant::now() > deadline {
            cancelled.set(true);
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let parse_options = ParseOptions::new().progress_callback(&mut parse_progress);
    let tree = parser.parse_with_options(&mut read, None, Some(parse_options))?;
    if cancelled.get() {
        return None;
    }

    let query = Query::new(grammar, query_source).expect("the .scm query must compile");
    let mut query_progress = |_state: &tree_sitter::QueryCursorState| -> ControlFlow<()> {
        if Instant::now() > deadline {
            cancelled.set(true);
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let query_options = QueryCursorOptions::new().progress_callback(&mut query_progress);

    let mut cursor = QueryCursor::new();
    let mut matches =
        cursor.matches_with_options(&query, tree.root_node(), bytes, query_options);

    let capture_names = query.capture_names();
    let mut captures: Vec<(&str, Node)> = Vec::new();
    while let Some(query_match) = matches.next() {
        for capture in query_match.captures {
            captures.push((capture_names[capture.index as usize], capture.node));
        }
    }
    if cancelled.get() {
        return None;
    }

    Some(assign_captures_to_functions(bytes, captures))
}

/// The generic range-containment post-processor (US16 T2): assigns every
/// non-`@function` capture to its innermost enclosing `@function` capture
/// by byte range, then folds the assigned captures into that function's
/// `ParsedFunction` fields. Iterative throughout (nested `for`, never a
/// recursive call) — the Q2 safety property this slice actually depends
/// on. Written generically over `(capture_name, Node)` pairs so a future
/// language's adapter (a different `.scm`, a different grammar) can reuse
/// it unchanged; only the `.scm`'s capture names and the node-kind
/// dispatch below are C#-shaped today because C# is the only second
/// adapter that exists yet (cc-yagni — no abstraction was built for a
/// second caller that isn't here).
fn assign_captures_to_functions(source: &[u8], captures: Vec<(&str, Node)>) -> Vec<ParsedFunction> {
    let mut function_nodes: Vec<Node> = captures
        .iter()
        .filter(|(name, _)| *name == "function")
        .map(|(_, node)| *node)
        .collect();
    function_nodes.sort_by_key(Node::start_byte);

    let mut results: Vec<ParsedFunction> = function_nodes
        .iter()
        .map(|node| ParsedFunction {
            name: field_text(node, "name", source),
            start_line: node.start_position().row + 1,
            calls: Vec::new(),
            has_loop: false,
            has_nested_loop: false,
            decision_points: 0,
            depth: 0,
            branch_arms: 0,
            calls_in_loops: Vec::new(),
        })
        .collect();

    let mut loops_of: Vec<Vec<Node>> = vec![Vec::new(); function_nodes.len()];
    let mut depth_nodes_of: Vec<Vec<Node>> = vec![Vec::new(); function_nodes.len()];
    let mut switch_sections_of: Vec<Vec<Node>> = vec![Vec::new(); function_nodes.len()];
    let mut calls_of: Vec<Vec<Node>> = vec![Vec::new(); function_nodes.len()];

    for (name, node) in &captures {
        if *name == "function" {
            continue;
        }
        let Some(owner) = innermost_function_index(&function_nodes, node) else {
            continue; // A top-level construct outside any function — ignored.
        };

        match *name {
            "loop" => {
                results[owner].has_loop = true;
                results[owner].decision_points += 1;
                loops_of[owner].push(*node);
                depth_nodes_of[owner].push(*node);
            }
            "branch.arm" => match node.kind() {
                "switch_section" => {
                    results[owner].decision_points += 1;
                    switch_sections_of[owner].push(*node);
                    depth_nodes_of[owner].push(*node);
                }
                "if_statement" => {
                    results[owner].decision_points += 1;
                    depth_nodes_of[owner].push(*node);
                }
                _ => {}
            },
            "conditional" => {
                results[owner].decision_points += 1;
            }
            "call" => {
                calls_of[owner].push(*node);
            }
            _ => {}
        }
    }

    for i in 0..function_nodes.len() {
        results[i].has_nested_loop = any_contained(&loops_of[i]);
        results[i].depth = max_nesting_depth(&depth_nodes_of[i]);
        results[i].branch_arms = max_switch_section_count(&switch_sections_of[i]);

        let mut call_nodes = calls_of[i].clone();
        call_nodes.sort_by_key(Node::start_byte);
        for call_node in &call_nodes {
            let name = field_text(call_node, "function", source);
            let in_loop = loops_of[i].iter().any(|loop_node| contains(loop_node, call_node));
            if in_loop {
                let point = call_node.start_position();
                results[i].calls_in_loops.push(LoopCall {
                    name: name.clone(),
                    line: point.row + 1,
                    col: point.column,
                    // T2 scope note (io_signatures/csharp.rs doc comment):
                    // real I/O classification for C# is T4 — an honest
                    // abstention here, never a fabricated NotIo (ADR-0010).
                    io: IoClassification::Unknown,
                });
            }
            results[i].calls.push(name);
        }
    }

    results
}

fn contains(outer: &Node, inner: &Node) -> bool {
    outer.start_byte() <= inner.start_byte() && inner.end_byte() <= outer.end_byte()
}

/// The function capture whose range most tightly contains `target` — the
/// smallest (by byte length) of every function span that contains it, so a
/// local function nested inside a method claims its own body's captures
/// instead of leaking them into the enclosing method (US16 T2: local
/// functions are captured as their own `@function`, deliberately unlike
/// `SynCodeParser`'s fold-into-outer treatment of a nested Rust `fn` — see
/// the tech spec's `.scm` capture list). `None` when `target` sits outside
/// every captured function (e.g. field initializers at class scope).
fn innermost_function_index(function_nodes: &[Node], target: &Node) -> Option<usize> {
    function_nodes
        .iter()
        .enumerate()
        .filter(|(_, f)| contains(f, target))
        .min_by_key(|(_, f)| f.end_byte() - f.start_byte())
        .map(|(i, _)| i)
}

/// Whether any node in `nodes` is contained by another — used for
/// `has_nested_loop`: two SIBLING loops (sequential, not nested) must not
/// set it, only an actual loop-inside-loop does.
fn any_contained(nodes: &[Node]) -> bool {
    nodes
        .iter()
        .enumerate()
        .any(|(i, a)| nodes.iter().enumerate().any(|(j, b)| i != j && contains(b, a)))
}

/// 1 + the number of OTHER `nodes` entries that contain a given entry,
/// maximized over every entry — an iterative nesting-depth count (Q2: no
/// recursion), capped at `MAX_NESTING_DEPTH` as a bound on the inner loop's
/// own work, not a correctness requirement.
fn max_nesting_depth(nodes: &[Node]) -> u32 {
    nodes
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let ancestors = nodes
                .iter()
                .enumerate()
                .filter(|(j, b)| *j != i && contains(b, a))
                .count() as u32;
            (1 + ancestors).min(MAX_NESTING_DEPTH)
        })
        .max()
        .unwrap_or(0)
}

/// Groups `switch_section` captures by their parent `switch_statement`
/// (walking up two levels: section -> `switch_body` -> `switch_statement`)
/// and returns the largest single switch's section count — the C# analog
/// of `syn`'s `branch_arms = max(branch_arms, match_arm_count)`.
fn max_switch_section_count(switch_sections: &[Node]) -> u32 {
    let mut per_switch: Vec<(usize, u32)> = Vec::new();
    for section in switch_sections {
        let Some(switch_stmt) = section.parent().and_then(|body| body.parent()) else {
            continue;
        };
        let switch_id = switch_stmt.id();
        match per_switch.iter_mut().find(|(id, _)| *id == switch_id) {
            Some(entry) => entry.1 += 1,
            None => per_switch.push((switch_id, 1)),
        }
    }
    per_switch.into_iter().map(|(_, count)| count).max().unwrap_or(0)
}

fn field_text(node: &Node, field: &str, source: &[u8]) -> String {
    node.child_by_field_name(field)
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<unresolved>")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use codeimpact_hexagon::analysis::Language;
    use codeimpact_hexagon::analysis::MetricSupport;

    // ── Test List (US16 T2, step D + E's TreeSitterCodeParser half) ──────
    //   1. language()/capabilities()/resolve_dependencies() — the port
    //      delta + T2's empty-dependency contract.
    //   2. function-shaped constructs (method/constructor/local function)
    //      each become their own ParsedFunction — one behavior, three
    //      divergent rows, one cycle; local-function-is-SEPARATE-from-its-
    //      enclosing-method is a DIFFERENT behavior, its own test.
    //   3. if -> +1 decision point; else-if chain -> +1 PER if, plain
    //      trailing else -> +0 (mirrors SynCodeParser's own semantics).
    //   4. every loop kind (for/foreach/while/do) -> has_loop + +1 decision
    //      point — one behavior, four divergent rows, one cycle.
    //   5. nested loop -> has_nested_loop; SIBLING loops -> must NOT set it
    //      (the discriminating negative case).
    //   6. switch arms -> branch_arms (max single switch) AND decision_points
    //      (sum of arms).
    //   7. && / || -> +1 decision point each.
    //   8. calls tracked in source order.
    //   9. call-in-loop -> calls_in_loops, IoClassification::Unknown (T2:
    //      honest abstention, real I/O detection is T4).

    fn parser() -> TreeSitterCodeParser {
        TreeSitterCodeParser::csharp()
    }

    #[test]
    fn language_is_csharp() {
        assert_eq!(parser().language(), Language::CSharp);
    }

    #[test]
    fn capabilities_reports_every_metric_supported() {
        let capabilities = parser().capabilities();
        assert_eq!(*capabilities.cyclomatic_complexity(), MetricSupport::Supported);
        assert_eq!(*capabilities.io_in_loops(), MetricSupport::Supported);
        assert_eq!(*capabilities.economic_impact(), MetricSupport::Supported);
        assert_eq!(*capabilities.ecological_impact(), MetricSupport::Supported);
    }

    #[test]
    fn resolve_dependencies_is_always_empty_in_t2() {
        let ctx = DependencyContext::new(PathBuf::from("a.cs"), PathBuf::from("."), vec![]);
        let resolved = parser().resolve_dependencies("class C {}", &ctx).unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn function_shaped_constructs_each_become_their_own_parsed_function() {
        let cases = [
            ("class C { void M() { } }", "M"),
            ("class C { public C() { } }", "C"),
            (
                "class C { void M() { int Local() { return 1; } Local(); } }",
                "Local",
            ),
        ];
        for (source, expected_name) in cases {
            let functions = parser().parse(source).unwrap();
            assert!(
                functions.iter().any(|f| f.name == expected_name),
                "source '{}': expected a function named '{}', got {:?}",
                source,
                expected_name,
                functions.iter().map(|f| &f.name).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn local_function_is_counted_separately_from_its_enclosing_method() {
        let source = "class C { void M() { int Local() { return 1; } Local(); } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions.len(), 2);
        let outer = functions.iter().find(|f| f.name == "M").unwrap();
        // M's own body is just the local declaration + one call — no
        // decision points of its own, whatever Local's body contains.
        assert_eq!(outer.decision_points, 0);
    }

    #[test]
    fn if_statement_counts_one_decision_point() {
        let source = "class C { void M() { if (true) { } } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 1);
    }

    #[test]
    fn else_if_chain_counts_one_decision_point_per_if_plain_else_counts_zero() {
        let source = "class C { void M() { if (a) { } else if (b) { } else { } } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 2);
    }

    #[test]
    fn every_loop_kind_sets_has_loop_and_counts_one_decision_point() {
        let cases = [
            "class C { void M() { for (int i = 0; i < 10; i++) { } } }",
            "class C { void M() { foreach (var x in xs) { } } }",
            "class C { void M() { while (true) { } } }",
            "class C { void M() { do { } while (true); } }",
        ];
        for source in cases {
            let functions = parser().parse(source).unwrap();
            assert!(functions[0].has_loop, "source: {}", source);
            assert_eq!(functions[0].decision_points, 1, "source: {}", source);
        }
    }

    #[test]
    fn nested_loop_sets_has_nested_loop() {
        let source =
            "class C { void M() { for (int i = 0; i < 10; i++) { while (true) { } } } }";
        let functions = parser().parse(source).unwrap();
        assert!(functions[0].has_nested_loop);
    }

    #[test]
    fn sibling_loops_do_not_set_has_nested_loop() {
        let source =
            "class C { void M() { for (int i = 0; i < 10; i++) { } while (true) { } } }";
        let functions = parser().parse(source).unwrap();
        assert!(!functions[0].has_nested_loop);
    }

    #[test]
    fn switch_arms_count_branch_arms_and_decision_points() {
        let source = "class C { void M() { switch (x) { case 1: break; case 2: break; default: break; } } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].branch_arms, 3);
        assert_eq!(functions[0].decision_points, 3);
    }

    #[test]
    fn and_or_operators_count_as_decision_points() {
        let source = "class C { void M() { if (a && b || c) { } } }";
        let functions = parser().parse(source).unwrap();
        // 1 (if) + 1 (&&) + 1 (||)
        assert_eq!(functions[0].decision_points, 3);
    }

    #[test]
    fn calls_are_tracked() {
        let source = "class C { void M() { Foo(); this.Bar(); } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].calls.len(), 2);
    }

    #[test]
    fn call_in_loop_is_recorded_with_unknown_io_classification() {
        let source = "class C { void M() { for (int i = 0; i < 10; i++) { DoWork(); } } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].calls_in_loops.len(), 1);
        assert_eq!(functions[0].calls_in_loops[0].name, "DoWork");
        assert_eq!(
            functions[0].calls_in_loops[0].io,
            IoClassification::Unknown
        );
    }

    #[test]
    fn call_outside_any_loop_is_tracked_but_not_in_calls_in_loops() {
        let source = "class C { void M() { DoWork(); } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].calls, vec!["DoWork".to_string()]);
        assert!(functions[0].calls_in_loops.is_empty());
    }
}
