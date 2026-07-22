use std::cell::Cell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::ControlFlow;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use codeimpact_hexagon::analysis::source_guard;
use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeParser;
use codeimpact_hexagon::analysis::DependencyContext;
use codeimpact_hexagon::analysis::Language;
use codeimpact_hexagon::analysis::LanguageCapabilities;
use codeimpact_hexagon::analysis::LoopCall;
use codeimpact_hexagon::analysis::MetricSupport;
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
use super::io_signatures::classifier::classify_csharp_call;
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

/// Per-function cap on how many `@loop`/`@branch.arm`/`@call` captures may
/// feed the O(n^2) containment helpers (`any_contained`, `max_nesting_depth`,
/// `max_switch_section_count`, the calls-in-loops scan) before the WHOLE
/// FILE is refused as `SourceTooComplex` (US16 T2 retry #1, Security HIGH).
/// `MAX_NESTING_DEPTH` only capped the reported VALUE, not the compute cost
/// — Security reproduced a 45.9s hang with 80,000 SIBLING (not nested)
/// `if` statements in one method: a flat structure keeps parse+query fast
/// (never trips `PARSE_QUERY_BUDGET`), then the O(n^2) post-processing
/// pass for that single function is the entire cost. 2,000 is generous
/// for any legitimate function (2,000^2 = 4M simple byte-range
/// comparisons, sub-millisecond) while closing the unbounded-input class
/// outright, independent of timing.
const MAX_QUADRATIC_CAPTURES_PER_FUNCTION: usize = 2_000;

/// `namespace -> declaring-files` (US16 T5) — named so `DepsIndex`'s field
/// stays readable.
type NamespaceIndex = HashMap<String, Vec<PathBuf>>;

/// The project-global pre-pass's full output (US16 T5, Security MEDIUM
/// retry #1): the `namespace -> declaring-files` index AND every file's
/// own `using` targets, captured in the SAME pass over `file_sources` —
/// `resolve_dependencies` looks its own file up in `file_usings` instead
/// of re-parsing `source` a second time (once here, once in the pre-pass,
/// for the SAME file, on every single call).
struct DepsIndex {
    namespace_declarers: NamespaceIndex,
    file_usings: HashMap<PathBuf, Vec<String>>,
}

/// The `deps_index_cache`'s memoized entry (#90 T5 retry #1): the exact
/// `file_sources` `Arc` the cached `DepsIndex` was built from, kept
/// alongside it so a later call can compare by pointer IDENTITY
/// (`Arc::ptr_eq`) rather than recomputing a content fingerprint — see
/// `TreeSitterCodeParser::deps_index`'s doc for the full rationale.
type DepsIndexCacheEntry = (Arc<Vec<(PathBuf, String)>>, Arc<DepsIndex>);

/// Parses C# via `tree-sitter` (US16 T2). `parse` runs a `.scm` query over
/// the file and assigns each capture to its innermost enclosing function by
/// byte range (`assign_captures_to_functions`). `resolve_dependencies`
/// (US16 T5) resolves C#'s `using` directives through a project-global
/// `DepsIndex`, built once per run from `DependencyContext::file_sources`
/// and memoized in `deps_index_cache` (keyed on the `file_sources` `Arc`'s
/// pointer IDENTITY, #90 T5 retry #1 — see `deps_index`'s doc for why) —
/// every file in a project scan shares the SAME `file_sources`/
/// `source_roots`, so the expensive tree-sitter pass over every project
/// file (including `current_file` itself) runs exactly once per run, not
/// once per file NOR twice for the same file (Security MEDIUM, retry #1).
pub struct TreeSitterCodeParser {
    language: Language,
    profile: LanguageProfile,
    deps_index_cache: Mutex<Option<DepsIndexCacheEntry>>,
}

impl TreeSitterCodeParser {
    /// `extra_prefixes` (US16 T4.3, ADR-0019's reserved `ioSignatures` key)
    /// are user-configured confident I/O prefixes, additive to the base
    /// `File.`/`Directory.` table — an empty `Vec` reproduces T4.1/T4.2's
    /// behavior byte-for-byte.
    pub fn csharp(extra_prefixes: Vec<String>) -> Self {
        let mut io_table: Vec<String> = io_signatures::csharp::IO_PREFIXES
            .iter()
            .map(|s| s.to_string())
            .collect();
        io_table.extend(extra_prefixes);
        Self {
            language: Language::CSharp,
            profile: LanguageProfile {
                grammar: tree_sitter_c_sharp::LANGUAGE.into(),
                scm: include_str!("queries/csharp.scm"),
                deps_scm: include_str!("queries/csharp_deps.scm"),
                io_table,
            },
            deps_index_cache: Mutex::new(None),
        }
    }

    /// The memoized `DepsIndex` for `ctx`'s project — rebuilt only when
    /// `ctx.file_sources` is a DIFFERENT `Arc` allocation than the one the
    /// cache was last built from (US16 T5, keying rule hardened #90 T5
    /// retry #1 — Dev-B changes-requested, Security MEDIUM CWE-400, QA
    /// convergent). `run_analysis` builds ONE `file_sources` `Arc` per scan
    /// and clones the SAME `Arc` into every file's `DependencyContext`
    /// (`Arc::clone(&file_sources)` in the project loop), so `Arc::ptr_eq`
    /// is a correct, O(1), never-rehashing cache key: `Vec<(PathBuf,
    /// String)>` has no interior mutability, so "same Arc" already implies
    /// "same content" — no hash needed to prove it. A prior content-hash
    /// fingerprint fixed a stale-reuse bug but reintroduced the cost it was
    /// meant to avoid: hashing every file's full text on EVERY call, which
    /// `resolve_dependencies` makes once PER PROJECT FILE — O(N_files x
    /// total_source_bytes) per scan, in production today, not just under a
    /// future LSP reuse. The trade Arc-identity makes: two distinct,
    /// byte-identical `Arc` allocations no longer share a cache entry and
    /// rebuild instead — rare (would need two independently-constructed
    /// `file_sources` vectors with identical content) and harmless (an
    /// extra rebuild, never a correctness issue).
    fn deps_index(&self, ctx: &DependencyContext) -> Arc<DepsIndex> {
        {
            // Poison hardening (#90 T5, Security LOW retry from #33 T5):
            // unreachable today (this parser is only ever driven
            // single-threaded), but a poisoned guard must never turn into a
            // hard panic once a future caller shares one instance across
            // threads (roadmapped LSP primary) — recover the guard instead.
            let cache = self
                .deps_index_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some((cached_sources, index)) = cache.as_ref() {
                if Arc::ptr_eq(cached_sources, &ctx.file_sources) {
                    return Arc::clone(index);
                }
            }
        }
        let index = Arc::new(build_deps_index(
            &self.profile,
            &ctx.file_sources,
            &ctx.source_roots,
        ));
        *self
            .deps_index_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some((Arc::clone(&ctx.file_sources), Arc::clone(&index)));
        index
    }
}

impl CodeParser for TreeSitterCodeParser {
    fn language(&self) -> Language {
        self.language
    }

    fn capabilities(&self) -> LanguageCapabilities {
        // T3+T4+T5 (US16, #33, Q1 human-approved): C# honestly degrades
        // three metrics rather than claiming full support.
        // - io_in_loops is Degraded (T4): `classify_csharp_call` measures
        //   real (syntactic) I/O for static-qualified calls, but instance/EF
        //   receivers abstain (Unknown) rather than assert, so the metric is
        //   honestly partial, never fully Supported.
        // - call_graph is Degraded (T3): `assign_captures_to_functions`
        //   resolves calls by NAME only — an unresolved receiver can make
        //   two different calls merge into the same recorded name. Precise
        //   dropping of ambiguous edges is deferred (T5.3, human-approved).
        // - cross_file_dependencies is Degraded (T5): `resolve_dependencies`
        //   resolves at NAMESPACE granularity — a `using` links to every
        //   file that declares the used namespace, not necessarily the one
        //   that actually declares what this file needed.
        LanguageCapabilities::all_supported(self.language)
            .with_io_in_loops(MetricSupport::Degraded(
                "syntactic only; instance/EF receivers abstained, not asserted".to_string(),
            ))
            .with_call_graph(MetricSupport::Degraded(
                "name-based resolution; unresolved-receiver calls may merge".to_string(),
            ))
            .with_cross_file_dependencies(MetricSupport::Degraded(
                "namespace-level resolution; a file links to every declarer of a used namespace"
                    .to_string(),
            ))
    }

    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError> {
        source_guard::check_admissible(source).map_err(AnalysisError::Unmeasurable)?;
        parse_source(&self.profile, source)
    }

    /// Resolves `source`'s `using` directives to actual project files
    /// (US16 T5) — C#'s `using`/namespace semantics are entirely owned by
    /// this adapter (ADR-0018). A `using` resolves to every project file
    /// that DECLARES the used namespace (namespace-granularity resolution,
    /// honestly reported as `Degraded` in `capabilities`) via the memoized
    /// `deps_index`; `current_file` is excluded from its own result
    /// (never a self-edge) and the result is deduped. A `using` with no
    /// project declarer (e.g. `using System;`) contributes no edge — same
    /// "absent, never an error" contract as `SynCodeParser`.
    ///
    /// `current_file`'s OWN `using`s are looked up in `deps_index`'s
    /// `file_usings` (Security MEDIUM, retry #1) — the pre-pass already
    /// parsed `source` once while building the index (`current_file` is
    /// itself one of `ctx.file_sources`), so a second `extract_deps_safe`
    /// call on the SAME text is redundant. Falls back to extracting
    /// `source` directly only when `current_file` is absent from
    /// `file_usings` (not part of `ctx.file_sources` at all — e.g. a
    /// hand-built `DependencyContext` in a test, or a real caller that
    /// never populated it).
    fn resolve_dependencies(
        &self,
        source: &str,
        ctx: &DependencyContext,
    ) -> Result<Vec<PathBuf>, AnalysisError> {
        source_guard::check_admissible(source).map_err(AnalysisError::Unmeasurable)?;

        let index = self.deps_index(ctx);
        let usings = match index.file_usings.get(&ctx.current_file) {
            Some(usings) => usings.clone(),
            None => {
                extract_deps_safe(&self.profile, source)
                    .ok_or(AnalysisError::Unmeasurable(
                        UnmeasurableReason::SourceTooComplex,
                    ))?
                    .usings
            }
        };

        // `seen` dedupes in O(1) per candidate (MINOR, US16 T5 retry #2)
        // — a linear `resolved.contains(..)` scan was O(len(resolved)) per
        // candidate; `resolved` itself stays a plain `Vec` for its
        // caller-visible insertion order.
        let mut resolved: Vec<PathBuf> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        for used_namespace in &usings {
            let Some(declarers) = index.namespace_declarers.get(used_namespace) else {
                continue;
            };
            for declarer in declarers {
                if declarer != &ctx.current_file && seen.insert(declarer.clone()) {
                    resolved.push(declarer.clone());
                }
            }
        }
        Ok(resolved)
    }
}

/// Every namespace declared, and every namespace used (via `using`), by one
/// file's source — the raw material both the namespace-index builder and
/// `resolve_dependencies` extract from a `deps_scm` query pass (US16 T5).
struct DepsExtraction {
    namespaces: Vec<String>,
    usings: Vec<String>,
}

/// Whether `path` is in scope for the namespace index, given the
/// configured `roots` (US16 T5). Empty `roots` means "unset" — treated as
/// unrestricted (never as "nothing is in scope"), which is also what an
/// absent `sourceRoots` config resolves to (`run_analysis::
/// resolve_source_roots`): there is no materialized "project_root" PathBuf
/// here that could mismatch a canonicalized file path, only an honest
/// "no restriction configured."
fn under_any_root(path: &Path, roots: &[PathBuf]) -> bool {
    roots.is_empty() || roots.iter().any(|root| path.starts_with(root))
}

/// Builds the full `DepsIndex` from every project file's source in ONE
/// pass (US16 T5, Security MEDIUM retry #1 — this is also the ONLY place
/// a given file's text is ever parsed for dependency purposes, see
/// `resolve_dependencies`'s cache lookup). Each file is guarded
/// independently (`extract_deps_safe`) — a single hostile/oversized/
/// pathological file is simply excluded from the index, never fatal to
/// the whole project scan.
///
/// `file_usings` is populated for EVERY successfully-extracted file,
/// unconditionally — `current_file` must be able to resolve its OWN
/// `using`s regardless of whether current_file itself sits inside or
/// outside `source_roots` (identical to `resolve_dependencies`'s
/// pre-Security-MEDIUM-fix behavior, which always parsed `source`
/// directly with no `source_roots` gate at all). `namespace_declarers`,
/// by contrast, is scoped to `under_any_root` — `source_roots` bounds
/// which files may act as a namespace's DECLARER, not which files may
/// REQUEST resolution.
fn build_deps_index(
    profile: &LanguageProfile,
    file_sources: &[(PathBuf, String)],
    source_roots: &[PathBuf],
) -> DepsIndex {
    let mut namespace_declarers: NamespaceIndex = HashMap::new();
    let mut file_usings: HashMap<PathBuf, Vec<String>> = HashMap::new();

    for (path, source) in file_sources {
        let Some(extraction) = extract_deps_safe(profile, source) else {
            continue;
        };
        file_usings.insert(path.clone(), extraction.usings);

        if under_any_root(path, source_roots) {
            for namespace in extraction.namespaces {
                namespace_declarers
                    .entry(namespace)
                    .or_default()
                    .push(path.clone());
            }
        }
    }

    DepsIndex {
        namespace_declarers,
        file_usings,
    }
}

/// Runs `guard_admissible`-style checks then `extract_deps` inside
/// `catch_unwind` (US16 T5) — the pre-pass parses every OTHER project
/// file, an untrusted-input surface identical in kind to `parse()`'s own
/// (Q2/#33 T2 precedent), so it gets the same defense: an oversized
/// source is refused before tree-sitter ever sees it, and an ordinary Rust
/// panic in extraction never takes down the whole project scan.
fn extract_deps_safe(profile: &LanguageProfile, source: &str) -> Option<DepsExtraction> {
    source_guard::check_admissible(source).ok()?;

    let deadline = Instant::now() + PARSE_QUERY_BUDGET;
    let owned = source.to_string();
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| extract_deps(profile, &owned, deadline)));
    outcome.ok().flatten()
}

/// Parses `source` and runs `deps_scm`'s query over it, returning every
/// declared namespace's name and every `using`'s target namespace text
/// (US16 T5) — `None` when parse/query is cancelled by `deadline`
/// (mirrors `run_pipeline`'s own budget contract).
fn extract_deps(
    profile: &LanguageProfile,
    source: &str,
    deadline: Instant,
) -> Option<DepsExtraction> {
    let grammar = &profile.grammar;
    let bytes = source.as_bytes();

    let mut parser = Parser::new();
    parser
        .set_language(grammar)
        .expect("grammar must load — a hardcoded, known-good constant");

    let mut read =
        |byte_offset: usize, _point: Point| -> &[u8] { bytes.get(byte_offset..).unwrap_or(&[]) };
    let mut parse_progress = |_state: &tree_sitter::ParseState| -> ControlFlow<()> {
        if Instant::now() > deadline {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let parse_options = ParseOptions::new().progress_callback(&mut parse_progress);
    let tree = parser.parse_with_options(&mut read, None, Some(parse_options))?;
    if Instant::now() > deadline {
        return None;
    }

    let query = Query::new(grammar, profile.deps_scm).expect("the deps .scm query must compile");
    let mut query_progress = |_state: &tree_sitter::QueryCursorState| -> ControlFlow<()> {
        if Instant::now() > deadline {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let query_options = QueryCursorOptions::new().progress_callback(&mut query_progress);

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches_with_options(&query, tree.root_node(), bytes, query_options);

    let capture_names = query.capture_names();
    let mut namespaces = Vec::new();
    let mut usings = Vec::new();
    while let Some(query_match) = matches.next() {
        for capture in query_match.captures {
            match capture_names[capture.index as usize] {
                "namespace" => {
                    if let Some(text) = field_text_opt(&capture.node, "name", bytes) {
                        namespaces.push(text);
                    }
                }
                "using" => {
                    if let Some(text) = using_target_text(&capture.node, bytes) {
                        usings.push(text);
                    }
                }
                _ => {}
            }
        }
    }
    if Instant::now() > deadline {
        return None;
    }

    Some(DepsExtraction { namespaces, usings })
}

/// The namespace text a `using_directive` node targets (US16 T5) — the
/// grammar gives this child NO field name for a plain `using Foo.Bar;`
/// (only an alias target, `using Alias = Foo.Bar;`, has a field, and it
/// names the ALIAS `Alias`, not the target). The target is therefore the
/// first namespace-shaped child (`qualified_name`/`identifier`/
/// `alias_qualified_name`/`generic_name`) that is NOT the `"name"`-field
/// alias identifier — this same rule handles both the plain and the
/// aliased/`using static`/`global using` shapes without special-casing
/// any of them (verified against the real grammar, tree-sitter-c-sharp
/// 0.23).
fn using_target_text(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for (index, child) in node.children(&mut cursor).enumerate() {
        let is_namespace_shaped = matches!(
            child.kind(),
            "qualified_name" | "identifier" | "alias_qualified_name" | "generic_name"
        );
        let is_alias_name_field = node.field_name_for_child(index as u32) == Some("name");
        if is_namespace_shaped && !is_alias_name_field {
            return child.utf8_text(source).ok().map(|s| s.to_string());
        }
    }
    None
}

/// `field_text`'s `Option`-returning twin (US16 T5) — `field_text` falls
/// back to the sentinel string `"<unresolved>"`, which is the right
/// contract for a `ParsedFunction`'s displayed name but would silently
/// poison the namespace index with a bogus `"<unresolved>"` entry here.
fn field_text_opt(node: &Node, field: &str, source: &[u8]) -> Option<String> {
    node.child_by_field_name(field)
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string())
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
    let io_table = profile.io_table.clone();

    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        run_pipeline(&grammar, query_source, &owned_source, &io_table)
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
    confident_io_prefixes: &[String],
) -> Option<Vec<ParsedFunction>> {
    let deadline = Instant::now() + PARSE_QUERY_BUDGET;
    let cancelled = Cell::new(false);

    let mut parser = Parser::new();
    parser
        .set_language(grammar)
        .expect("grammar must load — a hardcoded, known-good constant");

    let bytes = source.as_bytes();
    let mut read =
        |byte_offset: usize, _point: Point| -> &[u8] { bytes.get(byte_offset..).unwrap_or(&[]) };
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
    let mut matches = cursor.matches_with_options(&query, tree.root_node(), bytes, query_options);

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

    assign_captures_to_functions(bytes, captures, deadline, confident_io_prefixes)
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
///
/// `deadline` (US16 T2 retry #1, Security HIGH) bounds THIS pass too, not
/// just parse/query: checked once per function, defense in depth for many
/// moderately-sized functions cumulatively exceeding the budget. The
/// per-function `MAX_QUADRATIC_CAPTURES_PER_FUNCTION` cap is the load-
/// bearing fix for the single-function case (a deadline check between
/// functions never runs if there is only ONE pathological function —
/// the O(n^2) work for it must never start in the first place). `None`
/// means the file could not be safely measured within budget — the
/// caller must never publish a partial/undercounted result as if it were
/// complete (ADR-0010).
fn assign_captures_to_functions(
    source: &[u8],
    captures: Vec<(&str, Node)>,
    deadline: Instant,
    confident_io_prefixes: &[String],
) -> Option<Vec<ParsedFunction>> {
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

    for (owner, name, node) in owning_function_indices(&function_nodes, captures) {
        match name {
            "loop" => {
                results[owner].has_loop = true;
                results[owner].decision_points += 1;
                loops_of[owner].push(node);
                depth_nodes_of[owner].push(node);
            }
            "branch.arm" => match node.kind() {
                "switch_section" => {
                    results[owner].decision_points += 1;
                    switch_sections_of[owner].push(node);
                    depth_nodes_of[owner].push(node);
                }
                "if_statement" => {
                    results[owner].decision_points += 1;
                    depth_nodes_of[owner].push(node);
                }
                _ => {}
            },
            "conditional" => {
                results[owner].decision_points += 1;
            }
            "call" => {
                calls_of[owner].push(node);
            }
            _ => {}
        }
    }

    for i in 0..function_nodes.len() {
        // Defense in depth (Security HIGH, retry #1): many moderately-sized
        // functions could cumulatively exceed the budget even when no
        // SINGLE function trips the per-function cap below.
        if Instant::now() > deadline {
            return None;
        }

        // The load-bearing fix (Security HIGH, retry #1): the O(n^2)
        // containment work below must never START for an unbounded input —
        // a deadline check alone does not help when the entire cost lives
        // in ONE function's computation (80,000 sibling `if` statements in
        // a single method reproduced a 45.9s hang with parse+query both
        // finishing well inside budget).
        if loops_of[i].len() > MAX_QUADRATIC_CAPTURES_PER_FUNCTION
            || depth_nodes_of[i].len() > MAX_QUADRATIC_CAPTURES_PER_FUNCTION
            || switch_sections_of[i].len() > MAX_QUADRATIC_CAPTURES_PER_FUNCTION
            || calls_of[i].len() > MAX_QUADRATIC_CAPTURES_PER_FUNCTION
        {
            return None;
        }

        results[i].has_nested_loop = any_contained(&loops_of[i]);
        results[i].depth = max_nesting_depth(&depth_nodes_of[i]);
        results[i].branch_arms = max_switch_section_count(&switch_sections_of[i]);

        let mut call_nodes = calls_of[i].clone();
        call_nodes.sort_by_key(Node::start_byte);
        for call_node in &call_nodes {
            let name = field_text(call_node, "function", source);
            let in_loop = loops_of[i]
                .iter()
                .any(|loop_node| contains(loop_node, call_node));
            if in_loop {
                let point = call_node.start_position();
                results[i].calls_in_loops.push(LoopCall {
                    name: name.clone(),
                    line: point.row + 1,
                    col: point.column,
                    // US16 T4.1: real classification, replacing T2's
                    // hardcoded IoClassification::Unknown seam.
                    io: classify_csharp_call(&name, confident_io_prefixes),
                });
            }
            results[i].calls.push(name);
        }
    }

    Some(results)
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
/// the tech spec's `.scm` capture list). A capture outside every function
/// (e.g. a field initializer at class scope) is simply absent from the
/// result.
///
/// O(n log n): one sort (`captures`; `function_nodes` is already sorted by
/// `start_byte`) plus a single left-to-right sweep maintaining a stack of
/// currently-open functions — replaces a former O(functions x captures)
/// linear-scan-per-capture (`innermost_function_index`, US16 T2 retry #2,
/// Security HIGH). AST function nodes never partially overlap (a proper,
/// laminar nesting family: two functions are either disjoint or one fully
/// contains the other), so a bracket-matching stack is exactly correct —
/// not a heuristic: whenever a function's `end_byte` is at or before the
/// next position of interest, it MUST have already closed and is popped;
/// the stack's top, if any, is always the innermost function still open at
/// that position. Security reproduced a file of 58,000 individually-tiny
/// functions (each far under `MAX_QUADRATIC_CAPTURES_PER_FUNCTION`, so
/// retry #1's per-function cap never triggered) taking 16-33s in THIS
/// function alone, with parse+query both finishing fast — many legitimate
/// functions, not one pathological one, is not something a per-function
/// cap can ever catch; only removing the O(functions) scan itself does.
///
/// Grammar precondition (US16 T2 retry #3, Security LOW — read this
/// before reusing this helper for a future language's `.scm`): the
/// ownership check below is a single `open.last()` (innermost) with no
/// deeper-stack fallback — it silently drops a capture whose wrapping
/// node shares the EXACT `start_byte` of the `@function` it contains.
/// This never fires for `csharp.scm` today: every wrapping capture
/// (`for(`/`while(`/`if(`/`case`/`?:`/a call) requires at least one
/// literal token before any nested content, so a wrapping capture can
/// never start at the same byte as a `@function` it contains. A future
/// grammar that allows a zero-byte-gap tie (e.g. a construct whose own
/// span starts exactly where a nested function begins) would need a
/// down-stack fallback here instead of the single `top` check — do not
/// assume this precondition holds for a new `.scm` without verifying it.
fn owning_function_indices<'a>(
    function_nodes: &[Node<'a>],
    captures: Vec<(&'a str, Node<'a>)>,
) -> Vec<(usize, &'a str, Node<'a>)> {
    let mut non_function_captures: Vec<(&str, Node)> = captures
        .into_iter()
        .filter(|(name, _)| *name != "function")
        .collect();
    // Correctness, not just tidiness (retry #3, QA minor): the sweep below
    // assumes non-decreasing start_byte order. tree-sitter's own query
    // iteration is roughly a tree-position walk in practice, so this sort
    // is currently unexercised by any fixture (QA's mutation: removing it
    // survives every current test) — but that iteration order is not a
    // documented API guarantee this code should silently depend on.
    // Constructing a REAL parsed source that forces the query engine's
    // OWN iteration out of position order (rather than a hand-built,
    // impossible-to-fabricate `Node`) was judged not worth it for a
    // currently-unreachable case; kept as defensive, load-bearing-by-
    // contract code instead of being removed.
    non_function_captures.sort_by_key(|(_, node)| node.start_byte());

    let mut owned = Vec::with_capacity(non_function_captures.len());
    let mut open: Vec<usize> = Vec::new();
    let mut next_function = 0usize;

    for (name, node) in non_function_captures {
        let start = node.start_byte();

        // Open every function that starts at or before this capture,
        // popping any sibling that has ALREADY closed first — a function
        // whose range ends before the next function even starts cannot
        // still be open (laminar nesting).
        while next_function < function_nodes.len()
            && function_nodes[next_function].start_byte() <= start
        {
            while let Some(&top) = open.last() {
                if function_nodes[top].end_byte() <= function_nodes[next_function].start_byte() {
                    open.pop();
                } else {
                    break;
                }
            }
            open.push(next_function);
            next_function += 1;
        }

        // Close any still-open function that ended before this capture
        // starts (no NEW function's start crossed that boundary above to
        // trigger the pop, e.g. a gap between two top-level functions).
        while let Some(&top) = open.last() {
            if function_nodes[top].end_byte() <= start {
                open.pop();
            } else {
                break;
            }
        }

        if let Some(&top) = open.last() {
            if function_nodes[top].end_byte() >= node.end_byte() {
                owned.push((top, name, node));
            }
        }
    }

    owned
}

/// Whether any node in `nodes` is contained by another — used for
/// `has_nested_loop`: two SIBLING loops (sequential, not nested) must not
/// set it, only an actual loop-inside-loop does.
fn any_contained(nodes: &[Node]) -> bool {
    nodes.iter().enumerate().any(|(i, a)| {
        nodes
            .iter()
            .enumerate()
            .any(|(j, b)| i != j && contains(b, a))
    })
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
    per_switch
        .into_iter()
        .map(|(_, count)| count)
        .max()
        .unwrap_or(0)
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
    use codeimpact_hexagon::analysis::IoClassification;
    use codeimpact_hexagon::analysis::Language;

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
    //
    // ── Test List (US16 T4.1 — real C# I/O classification, replaces item 9
    //    above's hardcoded Unknown seam) ──────────────────────────────────
    //   10. a call whose name starts with a confident (static-class) prefix
    //       (e.g. "File.") in a loop -> classified Io.
    //   11. that same confident-prefix call OUTSIDE any loop -> not tracked
    //       in calls_in_loops at all (membership, not classification).
    //   12. a call with no confident-prefix match — free-function-shaped
    //       ("DoWork()") or method-shaped with a receiver ("list.Add(x)") —
    //       classifies NotIo. One behavior (no match -> NotIo), two
    //       divergent call shapes, one cycle.
    //   13. a call whose text merely CONTAINS a confident prefix without
    //       starting with it ("Fil.ReadAllText()", "MyFile.ReadAllText()")
    //       must NOT match (mutation-bite: `starts_with`, never `contains`).
    //
    // ── Test List (US16 T4.2 — EF/instance receiver-name abstention) ─────
    //   14. an EF-shaped call ("_context.Users.Where(...)") in a loop ->
    //       classified Unknown, never Io (no type proof) and never NotIo
    //       (human-approved Q1: EF receiver-name I/O is a counted
    //       abstention, not a warned assertion — ADR-0016 §3 split).
    //   15. capabilities() reports io_in_loops as Degraded (not Unsupported
    //       — T4 measures SOMETHING now, syntactically) with a reason
    //       naming the instance/EF abstention; the other metrics are
    //       unchanged from T3.
    //   15b. (retry #1, Dev-B BLOCKING + QA HIGH) the four demoted instance
    //        receivers — idiomatic underscore-camelCase field names
    //        (`_httpClient.`, `_sqlCommand.`, `_stream.`, `_dbContext.`) —
    //        in a loop classify Unknown, never NotIo (the silent false
    //        negative Dev-B reproduced: PascalCase markers never match real
    //        C# field-name receivers) and never Io (re-promoting any of
    //        them into IO_PREFIXES/confident_prefixes must fail this test —
    //        QA's mutation: re-adding "DbContext." to IO_PREFIXES survived
    //        the whole suite before this test existed).
    //
    // ── Test List (US16 T4.3 — user-configured confident prefixes) ───────
    //   16. csharp(extra_prefixes) with a user prefix ("MyIoWrapper.") ->
    //       a call starting with it, in a loop, classifies Io (additive to
    //       the base File./Directory. table).
    //   17. csharp(Vec::new()) — an absent/empty config — is byte-identical
    //       to T4.1/T4.2 (already proven by every test above still using
    //       `parser()`, which now passes Vec::new()).

    fn parser() -> TreeSitterCodeParser {
        TreeSitterCodeParser::csharp(Vec::new())
    }

    #[test]
    fn language_is_csharp() {
        assert_eq!(parser().language(), Language::CSharp);
    }

    // T3 (US16, #33, Q1 human-approved): C# honestly degrades two metrics —
    // io_in_loops is Unsupported (nothing measured until T4's real I/O
    // detection), call_graph is Degraded (name-based resolution, ambiguous
    // edges dropped) — the other three stay Supported, unchanged since T2.
    #[test]
    fn capabilities_reports_csharp_degradation() {
        let capabilities = parser().capabilities();
        assert_eq!(
            *capabilities.cyclomatic_complexity(),
            MetricSupport::Supported
        );
        assert_eq!(*capabilities.economic_impact(), MetricSupport::Supported);
        assert_eq!(*capabilities.ecological_impact(), MetricSupport::Supported);
        // T4.2 (US16, #33): io_in_loops flips from T3's Unsupported to
        // Degraded — real (syntactic) classification now happens, but
        // instance/EF receivers still abstain rather than assert.
        match capabilities.io_in_loops() {
            MetricSupport::Degraded(reason) => {
                assert!(
                    reason.contains("instance/EF receivers abstained"),
                    "expected the instance/EF abstention reason, got: {}",
                    reason
                );
            }
            other => panic!("expected io_in_loops to be Degraded, got {:?}", other),
        }
        match capabilities.call_graph() {
            MetricSupport::Degraded(reason) => {
                assert!(
                    reason.contains("unresolved-receiver"),
                    "expected the corrected (T5) name-based-resolution reason, got: {}",
                    reason
                );
            }
            other => panic!("expected call_graph to be Degraded, got {:?}", other),
        }
        match capabilities.cross_file_dependencies() {
            MetricSupport::Degraded(reason) => {
                assert!(
                    reason.contains("namespace-level"),
                    "expected the namespace-level-resolution reason, got: {}",
                    reason
                );
            }
            other => panic!(
                "expected cross_file_dependencies to be Degraded, got {:?}",
                other
            ),
        }
    }

    // Discriminating test (T5.2 tech spec): a C# call/dep capability
    // reported as Supported must FAIL — proves the two Degraded builders
    // above are actually wired, not merely present as dead code.
    #[test]
    fn call_graph_and_cross_file_dependencies_are_never_reported_supported() {
        let capabilities = parser().capabilities();
        assert_ne!(*capabilities.call_graph(), MetricSupport::Supported);
        assert_ne!(
            *capabilities.cross_file_dependencies(),
            MetricSupport::Supported
        );
    }

    #[test]
    fn resolve_dependencies_returns_empty_when_no_using_directives() {
        let ctx = DependencyContext::new(PathBuf::from("a.cs"), PathBuf::from("."), vec![]);
        let resolved = parser().resolve_dependencies("class C {}", &ctx).unwrap();
        assert!(resolved.is_empty());
    }

    // ── resolve_dependencies tests (US16 T5 — the C# namespace-index
    // resolver: extraction (namespace_declaration/file_scoped_namespace_
    // declaration/using_directive) + project-global index + lookup,
    // wired together through the real tree-sitter grammar) ──
    //
    // Test List (tech spec T5.1):
    //   1. edge file2 -> file1 via `namespace A` (file1) / `using A;` (file2)
    //   2. N:M multi-declarer -> every declaring file gets an edge
    //   3. `using System;` (no project declarer) -> no edge
    //   4. no self-edges (a file declaring AND using its own namespace)
    //   5. a namespace declared only OUTSIDE the configured source_roots
    //      does not resolve (source_roots scopes the index)

    fn deps_ctx(
        current_file: &str,
        file_sources: &[(&str, &str)],
        source_roots: &[&str],
    ) -> DependencyContext {
        let available_files: Vec<PathBuf> =
            file_sources.iter().map(|(p, _)| PathBuf::from(p)).collect();
        DependencyContext::new(
            PathBuf::from(current_file),
            PathBuf::from("."),
            available_files,
        )
        .with_file_sources(Arc::new(
            file_sources
                .iter()
                .map(|(p, s)| (PathBuf::from(*p), s.to_string()))
                .collect(),
        ))
        .with_source_roots(source_roots.iter().map(PathBuf::from).collect())
    }

    #[test]
    fn using_a_declared_namespace_resolves_to_its_declaring_file() {
        let file1 = "namespace A { class Foo {} }";
        let file2 = "using A;\nclass Bar {}";
        let ctx = deps_ctx("file2.cs", &[("file1.cs", file1), ("file2.cs", file2)], &[]);

        let resolved = parser().resolve_dependencies(file2, &ctx).unwrap();

        assert_eq!(resolved, vec![PathBuf::from("file1.cs")]);
    }

    #[test]
    fn using_a_namespace_declared_by_multiple_files_resolves_to_every_declarer() {
        let file1 = "namespace A { class Foo {} }";
        let file3 = "namespace A { class Baz {} }";
        let file2 = "using A;\nclass Bar {}";
        let ctx = deps_ctx(
            "file2.cs",
            &[
                ("file1.cs", file1),
                ("file2.cs", file2),
                ("file3.cs", file3),
            ],
            &[],
        );

        let mut resolved = parser().resolve_dependencies(file2, &ctx).unwrap();
        resolved.sort();

        assert_eq!(
            resolved,
            vec![PathBuf::from("file1.cs"), PathBuf::from("file3.cs")]
        );
    }

    #[test]
    fn using_a_namespace_with_no_project_declarer_produces_no_edge() {
        let file1 = "using System;\nclass Bar {}";
        let ctx = deps_ctx("file1.cs", &[("file1.cs", file1)], &[]);

        let resolved = parser().resolve_dependencies(file1, &ctx).unwrap();

        assert!(resolved.is_empty());
    }

    #[test]
    fn a_file_using_its_own_declared_namespace_does_not_link_to_itself() {
        let file1 = "using A;\nnamespace A { class Foo {} }";
        let ctx = deps_ctx("file1.cs", &[("file1.cs", file1)], &[]);

        let resolved = parser().resolve_dependencies(file1, &ctx).unwrap();

        assert!(resolved.is_empty());
    }

    #[test]
    fn a_namespace_declared_outside_configured_source_roots_does_not_resolve() {
        let outside = "namespace A { class Foo {} }";
        let inside = "using A;\nclass Bar {}";
        // file1.cs lives outside "src/" (the only configured source root) —
        // its declaration of namespace A must not enter the index.
        let ctx = deps_ctx(
            "src/file2.cs",
            &[("file1.cs", outside), ("src/file2.cs", inside)],
            &["src"],
        );

        let resolved = parser().resolve_dependencies(inside, &ctx).unwrap();

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

    // Retry #3 (QA HIGH) — the ONLY shape that distinguishes innermost
    // (correct) from outermost ownership in owning_function_indices's
    // stack: a capture INSIDE Local's body, while M is still open on the
    // stack underneath it. The test above never exercises this — its only
    // non-function capture (the `Local()` call) sits AFTER Local has
    // already closed, so the stack has already collapsed back to depth 1
    // by the time ownership is evaluated; `open.first()` and
    // `open.last()` are indistinguishable there. QA proved by mutation
    // that swapping to `open.first()` (outermost) survives the entire
    // suite without this test.
    #[test]
    fn nested_local_function_if_is_attributed_to_local_not_outer() {
        let source =
            "class C { void M() { if (a) { } int Local() { if (b) { } return 1; } Local(); } }";
        let functions = parser().parse(source).unwrap();
        let outer = functions.iter().find(|f| f.name == "M").unwrap();
        let local = functions.iter().find(|f| f.name == "Local").unwrap();
        assert_eq!(outer.decision_points, 1, "M's own if only");
        assert_eq!(local.decision_points, 1, "Local's own if only");
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
        let source = "class C { void M() { for (int i = 0; i < 10; i++) { while (true) { } } } }";
        let functions = parser().parse(source).unwrap();
        assert!(functions[0].has_nested_loop);
    }

    #[test]
    fn sibling_loops_do_not_set_has_nested_loop() {
        let source = "class C { void M() { for (int i = 0; i < 10; i++) { } while (true) { } } }";
        let functions = parser().parse(source).unwrap();
        assert!(!functions[0].has_nested_loop);
    }

    #[test]
    fn switch_arms_count_branch_arms_and_decision_points() {
        let source =
            "class C { void M() { switch (x) { case 1: break; case 2: break; default: break; } } }";
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
    fn ternary_operator_counts_as_one_decision_point() {
        // csharp.scm's `(conditional_expression) @conditional` — a
        // deliberate extension beyond SynCodeParser's exact node-kind
        // list, since Rust has no ternary to mirror (retry #1, Dev-B/QA).
        let source = "class C { void M() { int y = x > 0 ? 1 : 2; } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].decision_points, 1);
    }

    #[test]
    fn nested_if_for_if_tracks_depth_three() {
        // Mirrors SynCodeParser's own nesting_depth_tracked test (retry #1,
        // Dev-B/QA: the C# path had NO depth test, despite depth feeding
        // the user-visible DeepConditional warning).
        let source =
            "class C { void M() { if (a) { for (int i = 0; i < 10; i++) { if (b) { } } } } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].depth, 3);
    }

    #[test]
    fn sibling_ifs_do_not_inflate_depth() {
        // The negative case ruling out the false-positive class: three
        // SIBLING (not nested) ifs must report depth 1, not 3.
        let source = "class C { void M() { if (a) { } if (b) { } if (c) { } } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].depth, 1);
    }

    #[test]
    fn calls_are_tracked() {
        let source = "class C { void M() { Foo(); this.Bar(); } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].calls.len(), 2);
        assert_eq!(
            functions[0].calls,
            vec!["Foo".to_string(), "this.Bar".to_string()]
        );
    }

    // T4.1: supersedes T2's `call_in_loop_is_recorded_with_unknown_io_
    // classification` — the hardcoded `IoClassification::Unknown` seam is
    // gone, replaced by `classify_csharp_call`. A call with no confident-
    // prefix match and (T4.1-only, no suspicion heuristic yet) no receiver
    // marker classifies NotIo. Two divergent call shapes, same behavior.
    #[test]
    fn call_with_no_confident_prefix_match_classifies_not_io() {
        for call in ["DoWork();", "list.Add(x);"] {
            let source = format!(
                "class C {{ void M() {{ for (int i = 0; i < 10; i++) {{ {} }} }} }}",
                call
            );
            let functions = parser().parse(&source).unwrap();
            assert_eq!(functions[0].calls_in_loops.len(), 1, "case: {}", call);
            assert_eq!(
                functions[0].calls_in_loops[0].io,
                IoClassification::NotIo,
                "case: {}",
                call
            );
        }
    }

    #[test]
    fn confident_static_prefix_call_in_loop_classifies_io() {
        let source =
            "class C { void M() { for (int i = 0; i < 10; i++) { File.ReadAllText(p); } } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].calls_in_loops.len(), 1);
        assert_eq!(functions[0].calls_in_loops[0].name, "File.ReadAllText");
        assert_eq!(functions[0].calls_in_loops[0].io, IoClassification::Io);
    }

    #[test]
    fn confident_static_prefix_call_outside_any_loop_is_not_tracked_in_calls_in_loops() {
        let source = "class C { void M() { File.ReadAllText(p); } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].calls, vec!["File.ReadAllText".to_string()]);
        assert!(functions[0].calls_in_loops.is_empty());
    }

    #[test]
    fn user_configured_prefix_call_in_loop_classifies_io() {
        let source =
            "class C { void M() { for (int i = 0; i < 10; i++) { MyIoWrapper.DoSomething(); } } }";
        let functions = TreeSitterCodeParser::csharp(vec!["MyIoWrapper.".to_string()])
            .parse(source)
            .unwrap();
        assert_eq!(functions[0].calls_in_loops.len(), 1);
        assert_eq!(functions[0].calls_in_loops[0].io, IoClassification::Io);
    }

    #[test]
    fn ef_receiver_marker_call_in_loop_classifies_unknown() {
        let source = "class C { void M() { foreach (var x in xs) { _context.Users.Where(u => u.Id == x); } } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].calls_in_loops.len(), 1);
        assert_eq!(functions[0].calls_in_loops[0].io, IoClassification::Unknown);
    }

    // Retry #1 (Dev-B BLOCKING, QA HIGH): the four demoted instance
    // receivers must abstain on their REAL idiomatic C# shape — an
    // underscore-camelCase field (`_httpClient`, `_sqlCommand`, `_stream`,
    // `_dbContext`), never the PascalCase type name itself. Also pins the
    // static-vs-instance demotion QA's mutation found untested: none of
    // these may ever classify Io.
    #[test]
    fn idiomatic_instance_receiver_call_in_loop_classifies_unknown_never_io() {
        for call in [
            "_httpClient.GetAsync(url);",
            "_sqlCommand.ExecuteNonQuery();",
            "_stream.Read(buffer, 0, len);",
            "_dbContext.SaveChanges();",
        ] {
            let source = format!(
                "class C {{ void M() {{ for (int i = 0; i < 10; i++) {{ {} }} }} }}",
                call
            );
            let functions = parser().parse(&source).unwrap();
            assert_eq!(functions[0].calls_in_loops.len(), 1, "case: {}", call);
            assert_eq!(
                functions[0].calls_in_loops[0].io,
                IoClassification::Unknown,
                "case: {}",
                call
            );
        }
    }

    #[test]
    fn call_merely_containing_a_confident_prefix_does_not_match() {
        for call in ["Fil.ReadAllText(p);", "MyFile.ReadAllText(p);"] {
            let source = format!(
                "class C {{ void M() {{ for (int i = 0; i < 10; i++) {{ {} }} }} }}",
                call
            );
            let functions = parser().parse(&source).unwrap();
            assert_eq!(
                functions[0].calls_in_loops[0].io,
                IoClassification::NotIo,
                "case: {}",
                call
            );
        }
    }

    #[test]
    fn call_outside_any_loop_is_tracked_but_not_in_calls_in_loops() {
        let source = "class C { void M() { DoWork(); } }";
        let functions = parser().parse(source).unwrap();
        assert_eq!(functions[0].calls, vec!["DoWork".to_string()]);
        assert!(functions[0].calls_in_loops.is_empty());
    }

    // ── Security MEDIUM (retry #1) — Drop-of-deep-tree safety ──────────
    // The Q2 spike proved PARSING a deeply-nested tree never aborts the
    // process, but never verified DROPPING one — a distinct code path
    // (recursive free of a deep AST is exactly the native-abort class
    // that justified ADR-0015's subprocess canary for `syn`). Bypasses
    // TreeSitterCodeParser's own budget/cap machinery entirely to isolate
    // tree-sitter's OWN Drop implementation: this test PASSES by simply
    // completing — if `Tree::drop` recursed natively over 50,000 levels,
    // the whole process would abort right there (uncatchable by
    // catch_unwind, same as the naive-walk spike finding), and no
    // assertion after it would ever run.
    #[test]
    fn dropping_a_deeply_nested_tree_does_not_abort_the_process() {
        let mut source = String::from("class C { void M() {\n");
        for _ in 0..50_000 {
            source.push_str("if(x){\n");
        }
        source.push_str("int z = 1;\n");
        for _ in 0..50_000 {
            source.push_str("}\n");
        }
        source.push_str("} }\n");

        let mut ts_parser = tree_sitter::Parser::new();
        ts_parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .expect("grammar must load");
        let tree = ts_parser.parse(&source, None).expect("parse must succeed");
        drop(tree);

        // Reaching this line is the proof: the process survived the Drop.
    }

    // ── Security hardening (#90 T5, two LOW items deferred from #33 T5) ──
    // Both must be closed before an LSP primary reuses a single
    // TreeSitterCodeParser instance across scans:
    //   1. file_set_fingerprint must hash CONTENT, not just per-file
    //      lengths — a length-only fingerprint lets two file sets with the
    //      same paths/lengths but different content collide, silently
    //      stale-reusing the memoized DepsIndex.
    //   2. deps_index_cache's two lock sites must recover from mutex
    //      poisoning instead of propagating the panic.

    #[test]
    fn stale_deps_index_is_not_reused_when_file_content_changes_but_lengths_match() {
        let file1 = "namespace AAAA { class Foo {} }";
        let file2_v1 = "using AAAA;\nclass Bar {}";
        // Same length as file2_v1, but no `using` directive at all — built
        // by padding, not hand-counted, so the length-equality precondition
        // can never silently drift out of sync with file2_v1 above.
        let padding = " ".repeat(file2_v1.len() - "class Bar {}".len());
        let file2_v2 = format!("{padding}class Bar {{}}");
        assert_eq!(
            file2_v1.len(),
            file2_v2.len(),
            "precondition: same length, different content"
        );

        let parser = parser();

        let ctx1 = deps_ctx(
            "file2.cs",
            &[("file1.cs", file1), ("file2.cs", file2_v1)],
            &[],
        );
        let resolved1 = parser.resolve_dependencies(file2_v1, &ctx1).unwrap();
        assert_eq!(
            resolved1,
            vec![PathBuf::from("file1.cs")],
            "sanity: the first call resolves through the real `using AAAA;`"
        );

        let ctx2 = deps_ctx(
            "file2.cs",
            &[("file1.cs", file1), ("file2.cs", file2_v2.as_str())],
            &[],
        );
        let resolved2 = parser
            .resolve_dependencies(file2_v2.as_str(), &ctx2)
            .unwrap();

        assert!(
            resolved2.is_empty(),
            "the second file set has the SAME paths and SAME per-file \
             lengths as the first, but file2.cs no longer contains a \
             `using` directive — a length-only fingerprint collides with \
             the first file set and stale-reuses its cached DepsIndex, \
             wrongly resolving to {:?}",
            resolved2
        );
    }

    // Retry #1 (#90 T5 — Dev-B changes-requested, Security MEDIUM CWE-400,
    // QA convergent): the content-hash fingerprint above closed the stale-
    // reuse bug but introduced a NEW cost — hashing every file's full
    // content on EVERY `resolve_dependencies` call, in production TODAY
    // (`run_analysis` calls it once per project file, all sharing the SAME
    // `file_sources` `Arc`). Keying the cache on `Arc` pointer identity
    // instead is O(1) per call and never rehashes; the trade is that two
    // distinct, byte-identical `Arc` allocations no longer share a cache
    // entry (rare, harmless — just an extra rebuild, not a correctness
    // issue, and `Vec<(PathBuf,String)>` has no interior mutability so
    // "same Arc" already guarantees "same content").
    #[test]
    fn deps_index_reuses_the_same_arc_but_rebuilds_for_a_different_arc_with_identical_content() {
        let file_sources = Arc::new(vec![(
            PathBuf::from("file1.cs"),
            "namespace A { class Foo {} }".to_string(),
        )]);
        let ctx1 = DependencyContext::new(
            PathBuf::from("file1.cs"),
            PathBuf::from("."),
            vec![PathBuf::from("file1.cs")],
        )
        .with_file_sources(Arc::clone(&file_sources));

        let parser = parser();
        let index1 = parser.deps_index(&ctx1);
        let index2 = parser.deps_index(&ctx1);
        assert!(
            Arc::ptr_eq(&index1, &index2),
            "sanity: the SAME file_sources Arc across two calls must reuse the memoized \
             DepsIndex (cache hit, O(1)) — a per-call rebuild would defeat the whole point \
             of memoization"
        );

        // A second, DISTINCT Arc allocation with byte-identical content.
        let identical_content_sources = Arc::new((*file_sources).clone());
        let ctx2 = DependencyContext::new(
            PathBuf::from("file1.cs"),
            PathBuf::from("."),
            vec![PathBuf::from("file1.cs")],
        )
        .with_file_sources(Arc::clone(&identical_content_sources));

        let index3 = parser.deps_index(&ctx2);

        assert!(
            !Arc::ptr_eq(&index1, &index3),
            "a DIFFERENT file_sources Arc — even with byte-identical content — must NOT be \
             treated as a cache hit against the first Arc's memoized index: a fingerprint \
             keyed by content (rather than Arc identity) would incorrectly reuse it here, \
             and computing that content fingerprint on every call is exactly the \
             O(total project bytes) per-call cost this fix removes"
        );
    }

    #[test]
    fn deps_index_lookup_recovers_from_a_poisoned_cache_mutex_instead_of_panicking() {
        let parser = parser();

        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _guard = parser.deps_index_cache.lock().unwrap();
                    panic!("deliberately poisoning the cache mutex");
                })
                .join()
                .expect_err("the spawned thread must panic to poison the mutex");
        });
        assert!(parser.deps_index_cache.is_poisoned());

        let source = "namespace A { class Foo {} }";
        let ctx = deps_ctx("file1.cs", &[("file1.cs", source)], &[]);

        let resolved = parser.resolve_dependencies(source, &ctx);

        assert!(
            resolved.is_ok(),
            "resolve_dependencies must recover from a poisoned \
             deps_index_cache mutex instead of panicking on \
             .lock().unwrap(), got {:?}",
            resolved
        );
    }
}
