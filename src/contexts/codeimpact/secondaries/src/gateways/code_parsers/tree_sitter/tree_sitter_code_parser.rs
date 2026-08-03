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
use super::io_signatures::classifier::classify_call;
use super::language_profile::CapabilityDegradations;
use super::language_profile::DepsStrategy;
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
/// own referenced targets, captured in the SAME pass over `file_sources` —
/// `resolve_dependencies` looks its own file up in `file_references`
/// instead of re-parsing `source` a second time (once here, once in the
/// pre-pass, for the SAME file, on every single call). `namespace_declarers`
/// stays `NamespaceIndex`-specific (C#'s `using`/namespace resolution,
/// ADR-0023); `file_references` (renamed from `file_usings`, US17 T4.1 —
/// `usings` holding a relative path string like `"./x"` would be a name
/// that lies) is generalized across both `DepsStrategy` variants.
struct DepsIndex {
    namespace_declarers: NamespaceIndex,
    file_references: HashMap<PathBuf, Vec<String>>,
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
    #[cfg(feature = "lang-csharp")]
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
                suspicious_markers: io_signatures::csharp::SUSPICIOUS_RECEIVER_MARKERS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                degradations: CapabilityDegradations {
                    io_in_loops: MetricSupport::Degraded(
                        "syntactic only; instance/EF receivers abstained, not asserted"
                            .to_string(),
                    ),
                    call_graph: MetricSupport::Degraded(
                        "name-based resolution; unresolved-receiver calls may merge".to_string(),
                    ),
                    cross_file_dependencies: MetricSupport::Degraded(
                        "namespace-level resolution; a file links to every declarer of a used namespace"
                            .to_string(),
                    ),
                },
                deps: DepsStrategy::NamespaceIndex,
            },
            deps_index_cache: Mutex::new(None),
        }
    }

    /// US17 T1 — TypeScript, a second `LanguageProfile` sharing the entire
    /// pipeline C# already exercises (`parse_source`,
    /// `assign_captures_to_functions`, etc. are unchanged by this ticket).
    /// `deps_scm` is an EMPTY query (ruling A3): `resolve_dependencies`
    /// therefore returns empty for TypeScript in T1, the same honest
    /// staging ADR-0020 used for C# in T2 — real dependency resolution is
    /// T4.
    #[cfg(feature = "lang-typescript")]
    pub fn typescript(extra_prefixes: Vec<String>) -> Self {
        Self::ecmascript(
            Language::TypeScript,
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            extra_prefixes,
        )
    }

    /// US17 T1 — JavaScript, the third `LanguageProfile`, sharing
    /// `ecmascript.scm` with `typescript` (Q8: one query file for both
    /// grammars).
    #[cfg(feature = "lang-typescript")]
    pub fn javascript(extra_prefixes: Vec<String>) -> Self {
        Self::ecmascript(
            Language::JavaScript,
            tree_sitter_javascript::LANGUAGE.into(),
            extra_prefixes,
        )
    }

    /// Shared construction for both ECMAScript-family languages (US17 T1,
    /// cc-yagni: TypeScript and JavaScript differ only in their compiled
    /// grammar — everything else, including the I/O tables and
    /// degradations, is identical, so there is exactly one place that says
    /// so instead of two near-duplicate constructors).
    ///
    /// Feature-gated like `csharp()` above (retry — Dev-B F1, BLOCKING):
    /// `--no-default-features --features lang-csharp` must compile without
    /// `lang-typescript`, and vice versa — the per-language isolation the
    /// `lang-csharp` feature already had on `main` before this ticket.
    #[cfg(feature = "lang-typescript")]
    fn ecmascript(
        language: Language,
        grammar: tree_sitter::Language,
        extra_prefixes: Vec<String>,
    ) -> Self {
        let mut io_table: Vec<String> = io_signatures::typescript::IO_PREFIXES
            .iter()
            .map(|s| s.to_string())
            .collect();
        io_table.extend(extra_prefixes);
        Self {
            language,
            profile: LanguageProfile {
                grammar,
                scm: include_str!("queries/ecmascript.scm"),
                deps_scm: "",
                io_table,
                suspicious_markers: io_signatures::typescript::SUSPICIOUS_RECEIVER_MARKERS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                degradations: CapabilityDegradations {
                    io_in_loops: MetricSupport::Degraded(
                        "syntactic only; instance receivers and dynamic import() abstained"
                            .to_string(),
                    ),
                    call_graph: MetricSupport::Degraded(
                        "name-based resolution; every anonymous function is recorded under \
                         one placeholder name and merges into a single call-graph node — \
                         precise naming is deferred"
                            .to_string(),
                    ),
                    cross_file_dependencies: MetricSupport::Unsupported,
                },
                deps: DepsStrategy::RelativePath,
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
        // US17 T1: reads `self.profile.degradations` instead of hardcoding
        // per-language strings, so a new `LanguageProfile` (this ticket's
        // TypeScript/JavaScript ones) is the only thing that needs to
        // touch the capabilities question — C#'s three exact strings are
        // unchanged (moved into its own constructor, see `csharp()`).
        let degradations = &self.profile.degradations;
        LanguageCapabilities::all_supported(self.language)
            .with_io_in_loops(degradations.io_in_loops.clone())
            .with_call_graph(degradations.call_graph.clone())
            .with_cross_file_dependencies(degradations.cross_file_dependencies.clone())
    }

    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError> {
        source_guard::check_admissible(source).map_err(AnalysisError::Unmeasurable)?;
        parse_source(&self.profile, source)
    }

    /// Resolves `source`'s dependencies to actual project files, dispatched
    /// on `self.profile.deps` (US17 T4.1, AD-1) — a data field on the
    /// profile, never a `match self.language` branch here: a 5th language
    /// extends by writing a profile, not by editing this method (OCP is
    /// this ticket's whole point).
    ///
    /// `DepsStrategy::NamespaceIndex` is C#'s `using`/namespace semantics,
    /// entirely owned by this adapter (ADR-0018), moved here as-is from
    /// before T4.1: a `using` resolves to every project file that DECLARES
    /// the used namespace (namespace-granularity resolution, honestly
    /// reported as `Degraded` in `capabilities`) via the memoized
    /// `deps_index`; `current_file` is excluded from its own result (never
    /// a self-edge) and the result is deduped. A `using` with no project
    /// declarer (e.g. `using System;`) contributes no edge — same "absent,
    /// never an error" contract as `SynCodeParser`.
    ///
    /// `current_file`'s OWN referenced targets are looked up in
    /// `deps_index`'s `file_references` (Security MEDIUM, retry #1) — the
    /// pre-pass already parsed `source` once while building the index
    /// (`current_file` is itself one of `ctx.file_sources`), so a second
    /// `extract_deps_safe` call on the SAME text is redundant. Falls back
    /// to extracting `source` directly only when `current_file` is absent
    /// from `file_references` (not part of `ctx.file_sources` at all — e.g.
    /// a hand-built `DependencyContext` in a test, or a real caller that
    /// never populated it).
    ///
    /// `DepsStrategy::RelativePath` (TypeScript/JavaScript) returns no edge
    /// yet — T4.3 fills this arm in; T4.1 only opens the seam.
    fn resolve_dependencies(
        &self,
        source: &str,
        ctx: &DependencyContext,
    ) -> Result<Vec<PathBuf>, AnalysisError> {
        source_guard::check_admissible(source).map_err(AnalysisError::Unmeasurable)?;

        match self.profile.deps {
            DepsStrategy::NamespaceIndex => {
                let index = self.deps_index(ctx);
                let referenced = match index.file_references.get(&ctx.current_file) {
                    Some(referenced) => referenced.clone(),
                    None => {
                        extract_deps_safe(&self.profile, source)
                            .ok_or(AnalysisError::Unmeasurable(
                                UnmeasurableReason::SourceTooComplex,
                            ))?
                            .referenced
                    }
                };

                // `seen` dedupes in O(1) per candidate (MINOR, US16 T5
                // retry #2) — a linear `resolved.contains(..)` scan was
                // O(len(resolved)) per candidate; `resolved` itself stays a
                // plain `Vec` for its caller-visible insertion order.
                let mut resolved: Vec<PathBuf> = Vec::new();
                let mut seen: HashSet<PathBuf> = HashSet::new();
                for used_namespace in &referenced {
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
            DepsStrategy::RelativePath => Ok(Vec::new()),
        }
    }
}

/// Every dependency-relevant construct declared, and every one referenced,
/// by one file's source — the raw material both the namespace-index
/// builder and `resolve_dependencies` extract from a `deps_scm` query pass
/// (US16 T5). Renamed from `namespaces`/`usings` (US17 T4.1, AD-1): once
/// `RelativePath` populates `referenced` with strings like `"./x"`, calling
/// the field `usings` would be a name that lies — `declared`/`referenced`
/// are neutral across both `DepsStrategy` variants.
struct DepsExtraction {
    declared: Vec<String>,
    referenced: Vec<String>,
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
/// `file_references` is populated for EVERY successfully-extracted file,
/// unconditionally — `current_file` must be able to resolve its OWN
/// referenced targets regardless of whether current_file itself sits inside
/// or outside `source_roots` (identical to `resolve_dependencies`'s
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
    let mut file_references: HashMap<PathBuf, Vec<String>> = HashMap::new();

    for (path, source) in file_sources {
        let Some(extraction) = extract_deps_safe(profile, source) else {
            continue;
        };
        file_references.insert(path.clone(), extraction.referenced);

        if under_any_root(path, source_roots) {
            for namespace in extraction.declared {
                namespace_declarers
                    .entry(namespace)
                    .or_default()
                    .push(path.clone());
            }
        }
    }

    DepsIndex {
        namespace_declarers,
        file_references,
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
/// declared construct's name and every referenced target's text (US16 T5)
/// — `None` when parse/query is cancelled by `deadline` (mirrors
/// `run_pipeline`'s own budget contract). The query's own capture names
/// (`@namespace`/`@using`, C#-specific — `queries/csharp_deps.scm`) are
/// unchanged by US17 T4.1; only this function's Rust-side vocabulary is
/// generalized.
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
    let mut declared = Vec::new();
    let mut referenced = Vec::new();
    while let Some(query_match) = matches.next() {
        for capture in query_match.captures {
            match capture_names[capture.index as usize] {
                "namespace" => {
                    if let Some(text) = field_text_opt(&capture.node, "name", bytes) {
                        declared.push(text);
                    }
                }
                "using" => {
                    if let Some(text) = using_target_text(&capture.node, bytes) {
                        referenced.push(text);
                    }
                }
                _ => {}
            }
        }
    }
    if Instant::now() > deadline {
        return None;
    }

    Some(DepsExtraction {
        declared,
        referenced,
    })
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
    let suspicious_markers = profile.suspicious_markers.clone();

    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        run_pipeline(
            &grammar,
            query_source,
            &owned_source,
            &io_table,
            &suspicious_markers,
        )
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
    suspicious_io_markers: &[String],
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

    assign_captures_to_functions(
        bytes,
        captures,
        deadline,
        confident_io_prefixes,
        suspicious_io_markers,
    )
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
    suspicious_io_markers: &[String],
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
                // US17 T1 retry (Dev-B F6, BLOCKING 5, human ruling D1):
                // BOTH grammars give each label its OWN node — a cascade
                // (`case 1: case 2: doX(); break;`) parses as TWO separate
                // `switch_section` nodes in C# just as it parses as two
                // `switch_case` nodes in JS/TS (verified against the real
                // grammars: the retry's premise that C# already groups a
                // cascade into one node does not hold — same root cause on
                // both languages, same fix on both). An empty-bodied label
                // (nothing after its own `:` token — a fallthrough) is
                // therefore folded into the following label instead of
                // counted on its own, so a cascade counts as ONE decision
                // point/arm on EITHER language, restoring the ADR-0020 D4
                // cross-language comparability invariant this ticket's own
                // `?.`-omission comment already invokes.
                "switch_section" | "switch_case" | "switch_default"
                    if switch_label_has_body(&node) =>
                {
                    results[owner].decision_points += 1;
                    switch_sections_of[owner].push(node);
                    depth_nodes_of[owner].push(node);
                }
                "switch_section" | "switch_case" | "switch_default" => {
                    // Empty-bodied cascade label — folds into the next
                    // one, contributes nothing of its own.
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
            let name = call_callee_name(call_node, &function_nodes, source);
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
                    // hardcoded IoClassification::Unknown seam. US17 T1:
                    // `classify_call` is now language-agnostic, fed each
                    // language's own confident/suspicious tables.
                    io: classify_call(&name, confident_io_prefixes, suspicious_io_markers),
                });
            }
            results[i].calls.push(name);
        }
    }

    Some(results)
}

/// Whether a switch label node (`switch_section` in C#, `switch_case`/
/// `switch_default` in JS/TS) has at least one statement of its own (US17
/// T1 retry, Dev-B F6, human ruling D1) — used to fold an empty-bodied
/// cascade label (`case 1:` immediately followed by another label, no
/// statements of its own) into the one label that actually owns the
/// shared statements, so a whole cascade counts as ONE decision point on
/// EITHER grammar. Grammar-agnostic by construction: every switch label in
/// both grammars is `('case' EXPR | 'default') ':' STATEMENT*` — the
/// literal `':'` token is always a direct child (verified against both
/// real grammars), so "any child AFTER the `:` token" is a body-presence
/// check that needs no per-language field-name assumption (C#'s
/// `repeat($.statement)` inside `switch_section` carries no field name at
/// all, unlike JS's `field('body', ...)` — a field-name-based check would
/// have silently never matched for C#).
fn switch_label_has_body(node: &Node) -> bool {
    let mut cursor = node.walk();
    let mut seen_colon = false;
    for child in node.children(&mut cursor) {
        if seen_colon {
            return true;
        }
        if child.kind() == ":" {
            seen_colon = true;
        }
    }
    false
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
/// Grammar precondition (US16 T2 retry #3, Security LOW; RESOLVED by US17
/// T1 — read this before reusing this helper for a future language's
/// `.scm`): the ownership check below used to be a single `open.last()`
/// (innermost) with no deeper-stack fallback — it silently dropped a
/// capture whose wrapping node shared the EXACT `start_byte` of the
/// `@function` it contains. That precondition held for `csharp.scm` (every
/// wrapping capture — `for(`/`while(`/`if(`/`case`/`?:`/a call — requires
/// at least one literal token before any nested content) but NOT for
/// `ecmascript.scm`: a JS/TS IIFE like `!function () { ... }()` puts the
/// wrapping `call_expression`'s `start_byte` at the exact same position as
/// the nested `function_expression` it invokes (no parenthesized wrapper
/// needed once a leading `!` already disambiguates the parse). The single
/// `open.last()` check attributed such a capture to the INNER function,
/// found its `end_byte` smaller than the call's own (the call includes the
/// trailing `()`), and dropped the capture entirely — proven by
/// `iife_call_sharing_start_byte_with_its_own_function_expression_is_
/// attributed_to_outer` before this fix. The down-stack scan below finds
/// the innermost function whose range still fully CONTAINS the capture —
/// for C# this is always the same element `open.last()` already returned
/// (the innermost open function always contains the capture there), so
/// the whole C# suite stays green, byte-for-byte, unmodified.
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

        if let Some(&owner) = open
            .iter()
            .rev()
            .find(|&&index| function_nodes[index].end_byte() >= node.end_byte())
        {
            owned.push((owner, name, node));
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

/// The name recorded for a call node's callee (US17 T1 retry, Dev-B F3
/// BLOCKING): ordinarily `field_text(call_node, "function", source)`, the
/// raw source text of the callee — but when the callee node IS ITSELF one
/// of the file's captured `@function` nodes (an IIFE: `!function(){...}
/// ()`, `(() => {})()`), that raw text is the callee's ENTIRE BODY, not a
/// name. Recording the blob is doubly wrong: it produces an absurd
/// `ParsedFunction.calls`/call-graph edge, AND `classify_call`'s
/// suspicious-marker check matches by `contains`, so a body merely
/// CONTAINING a marker substring (e.g. `prefetchAll` containing `fetch`)
/// false-classifies as `Unknown` I/O for a call that performs none.
///
/// Membership is checked by `Node::id()` against `function_nodes` — the
/// exact list this file already captured as `@function` — rather than a
/// hardcoded per-language node-kind list, so this stays correct for any
/// future grammar without another retry: "is this callee one of OUR
/// captured functions" is the precise question, independent of what that
/// grammar happens to name the node kind.
///
/// Retry 2, MINOR 6 — this function's `"<anonymous>"` and `field_text`'s
/// `"<unresolved>"` fallback (the name an anonymous `ParsedFunction` gets)
/// are DELIBERATELY two different sentinels, not the same one twice: an
/// IIFE's call edge can therefore never accidentally point at its own
/// enclosing `ParsedFunction` in the call graph (`"<anonymous>" !=
/// "<unresolved>"`) — a coincidental match there would be a WRONG
/// self-edge, not a correct one, so keeping them distinct is the safer
/// choice, even though it means the edge resolves to nothing rather than
/// to something meaningful.
fn call_callee_name(call_node: &Node, function_nodes: &[Node], source: &[u8]) -> String {
    let callee_is_function_shaped = call_node
        .child_by_field_name("function")
        .map(unwrap_transparent_wrapper)
        .is_some_and(|callee| function_nodes.iter().any(|f| f.id() == callee.id()));
    if callee_is_function_shaped {
        "<anonymous>".to_string()
    } else {
        field_text(call_node, "function", source)
    }
}

/// Descends through syntactically-transparent wrapper nodes to the real
/// expression they wrap (US17 T1 retry 2, Dev-B F3/Security convergent,
/// BLOCKING 3; extended, sweep, Dev-B MINOR C): the TEXTBOOK IIFE form,
/// `(function(){...})()` / `(() => {})()`, puts a `parenthesized_
/// expression` between the call and the function-shaped node it invokes
/// — the `!`/`void`-prefixed forms tested first put the function-shaped
/// node DIRECTLY in the callee position, which is the marginal shape, not
/// the common one.
///
/// **Correction (sweep)**: the previous doc here claimed "the wrapped
/// expression is always the first NAMED child" and used `named_child(0)`
/// uniformly. That claim was FALSE in two ways, both reproducible and
/// both fixed below:
/// - `comment` is a grammar "extra" (`extras: [$.comment, ...]`) — a NAMED
///   node that tree-sitter permits almost anywhere, including as the
///   FIRST child inside a `parenthesized_expression`
///   (`(/* c */ function(){})()`), silently displacing the real
///   expression from index 0.
/// - Not every wrapper puts the expression first. `type_assertion`
///   (`<Type>expr`) and `sequence_expression` (`(a, expr)`, the
///   comma-operator's "resulting value") put it LAST, per the real
///   grammar rules (`type_assertion: seq(type_arguments, expression)`,
///   `sequence_expression: seq(expression+)`).
///
/// So the descent is kind-dependent: `parenthesized_expression`,
/// `non_null_expression` (`expr!`), `as_expression` (`expr as Type`) and
/// `satisfies_expression` (`expr satisfies Type`) put the expression
/// FIRST (skipping any leading `comment`/`html_comment` extra);
/// `type_assertion` and `sequence_expression` put it LAST. Looped to also
/// handle doubly- or mixed-wrapped forms (`((() => {}))()`,
/// `(<any>expr as Fn)()`). Applied ONLY to the node used for the
/// `@function`-membership test — the returned NAME text, when the callee
/// is NOT function-shaped, still comes from `field_text` on the ORIGINAL
/// (un-unwrapped) callee, so a merely-parenthesized ordinary call
/// (`(Foo)()`) is unaffected.
fn unwrap_transparent_wrapper<'a>(mut node: Node<'a>) -> Node<'a> {
    loop {
        let inner = match node.kind() {
            "parenthesized_expression"
            | "non_null_expression"
            | "as_expression"
            | "satisfies_expression" => first_non_comment_named_child(node),
            "type_assertion" | "sequence_expression" => last_named_child(node),
            _ => None,
        };
        match inner {
            Some(next) => node = next,
            None => return node,
        }
    }
}

/// The first named child of `node` that is not a `comment`/`html_comment`
/// grammar "extra" — extras can appear almost anywhere tree-sitter allows
/// whitespace, including before the real expression a transparent wrapper
/// carries, so plain `named_child(0)` is not safe for that case.
fn first_non_comment_named_child<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|c| c.is_named() && !matches!(c.kind(), "comment" | "html_comment"));
    found
}

/// The last named child of `node` — the comma-operator's "resulting
/// value" for `sequence_expression`, or the asserted expression (which
/// follows the type in `<Type>expr`) for `type_assertion`.
fn last_named_child<'a>(node: Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).filter(|c| c.is_named()).last();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use codeimpact_hexagon::analysis::IoClassification;
    use codeimpact_hexagon::analysis::Language;

    // US17 T1 retry (same-shape sweep, Dev-B F1 BLOCKING): the entire C#
    // test section below calls `parser()` (-> `TreeSitterCodeParser::
    // csharp(..)`) or references `tree_sitter_c_sharp` directly, both now
    // gated behind `lang-csharp` (see the constructor's own `#[cfg]`) — so
    // this section must be excluded, not just left uncompiled by omission,
    // whenever `--no-default-features --features lang-typescript
    // --all-targets` is built (verified: this exact combination failed to
    // compile before this `mod` wrapper was added, the same regression
    // shape BLOCKING 1 fixed on the production side).
    #[cfg(feature = "lang-csharp")]
    mod csharp_tests {
        use super::*;

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
            let source =
            "class C { void M() { switch (x) { case 1: break; case 2: break; default: break; } } }";
            let functions = parser().parse(source).unwrap();
            assert_eq!(functions[0].branch_arms, 3);
            assert_eq!(functions[0].decision_points, 3);
        }

        // Retry 2 (BLOCKING 4) — `switch_case_cascade_counts_one_decision_
        // point_matching_csharp` (in `mod ecmascript_tests`, gated
        // `#[cfg(feature = "lang-typescript")]`) proves parity but lives
        // where `--no-default-features --features lang-csharp` (no
        // `lang-typescript`) never compiles it — the C# cascade fix would
        // be entirely UNTESTED in that build, the same gap class BLOCKING 1
        // (retry 1) closed on the production side. This standalone twin,
        // inside `csharp_tests` (gated `lang-csharp` only), proves the C#
        // behavior alone, independent of whether TS/JS is even built.
        #[test]
        fn switch_case_cascade_counts_one_decision_point() {
            let source = "class C { void M() { switch (x) { case 1: case 2: doX(); break; default: break; } } }";
            let functions = parser().parse(source).unwrap();
            assert_eq!(
                functions[0].decision_points, 2,
                "the case1+case2 cascade (empty-bodied case 1 folds into case 2) counts as \
                 ONE decision point, default counts as one more"
            );
            assert_eq!(functions[0].branch_arms, 2);
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
        // gone, replaced by `classify_call` (US17 T1: renamed from
        // `classify_csharp_call`, made language-agnostic). A call with no
        // confident-prefix match and (T4.1-only, no suspicion heuristic yet)
        // no receiver marker classifies NotIo. Two divergent call shapes,
        // same behavior.
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
            // Fold-in 8 (retry, Security LOW): the TS/JS analog of this same
            // guard lives OUTSIDE this `csharp_tests` module — see
            // `dropping_a_deeply_nested_tree_does_not_abort_the_process_on_
            // any_grammar` below, gated on both `lang-csharp` AND
            // `lang-typescript` since it exercises all three grammars in one
            // test.
        }

        // ── Security hardening (#90 T5, two LOW items deferred from #33 T5) ──
        // Both must be closed before an LSP primary reuses a single
        // TreeSitterCodeParser instance across scans:
        //   1. deps_index_cache must not stale-reuse a memoized DepsIndex for a
        //      different file set — keyed on Arc pointer identity so a changed
        //      file set (a fresh Arc) always rebuilds (retry #1 replaced the
        //      earlier content fingerprint; see `deps_index`).
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
        fn deps_index_reuses_the_same_arc_but_rebuilds_for_a_different_arc_with_identical_content()
        {
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
    } // mod csharp_tests

    // US17 T1 retry (Dev-B F1, BLOCKING): every TS/JS test below is nested
    // in its own `#[cfg(feature = "lang-typescript")]` submodule — the
    // constructors it exercises (`typescript()`/`javascript()`/
    // `ecmascript()`) are now feature-gated the same way `csharp()` always
    // was, so `--no-default-features --features lang-csharp` (no
    // `lang-typescript`) must not try to compile this section at all.
    #[cfg(feature = "lang-typescript")]
    mod ecmascript_tests {
        use super::*;

        // ── US17 T1 — TypeScript/JavaScript, a second/third `LanguageProfile`
        // sharing the entire pipeline above. Test List:
        //   1. language()/capabilities() for both constructors — the port
        //      delta (Q4's degradations) + resolve_dependencies' empty
        //      contract (A3: deps_scm is an empty query in T1).
        //   2. each @function kind (function_declaration, named function_
        //      expression, generator_function_declaration, named generator_
        //      function, arrow_function, method_definition) becomes its own
        //      ParsedFunction — one behavior, six divergent rows, one cycle.
        //   3. every loop kind (for/for-of/for-in/while/do) -> has_loop + +1
        //      decision point — one behavior, five divergent rows, one cycle.
        //   4. nested loop -> has_nested_loop; SIBLING loops must NOT set it.
        //   5. switch case/default arms -> branch_arms (max single switch) AND
        //      decision_points (sum of arms) — pins the new "switch_case" |
        //      "switch_default" node-kind dispatch (tech spec step 8).
        //   6. && / || / ?? -> +1 decision point each — one behavior, three
        //      divergent rows, one cycle.
        //   7. ternary -> +1 decision point.
        //   8. calls tracked in source order.
        //   9. a loop call classified Io (`fs.readFile`), Unknown (`fetch`),
        //      NotIo (`list.push`) — TS/JS ships with real classification from
        //      T1 (no C#-T2-style honest-abstention stage), one behavior,
        //      three divergent rows, one cycle.
        //   10. the shared ecmascript.scm query compiles against BOTH grammars
        //       (a non-compiling query panics inside Query::new, so parsing an
        //       empty source with each parser IS the guard).
        //   11. resolve_dependencies always returns empty for TS/JS (A3) even
        //       when the source contains import statements.
        //   12. the IIFE grammar-precondition test (tech spec step 8): a
        //       call_expression sharing its exact start_byte with the nested
        //       @function it wraps must still be attributed to the OUTER
        //       function, not silently dropped.

        fn ts_parser() -> TreeSitterCodeParser {
            TreeSitterCodeParser::typescript(Vec::new())
        }

        fn js_parser() -> TreeSitterCodeParser {
            TreeSitterCodeParser::javascript(Vec::new())
        }

        /// Both ECMAScript-family constructors, for tests that must prove a
        /// shared behavior on BOTH grammars (fold-in 7, retry — Dev-B F5/QA):
        /// the compile guard (`ecmascript_query_compiles_against_both_
        /// grammars`) only proves node-kind EXISTENCE, not structural parity
        /// (e.g. `max_switch_section_count`'s parent-walk was exercised
        /// against TS only before this retry).
        fn ecmascript_parsers() -> [TreeSitterCodeParser; 2] {
            [ts_parser(), js_parser()]
        }

        #[test]
        fn language_is_typescript_or_javascript() {
            assert_eq!(ts_parser().language(), Language::TypeScript);
            assert_eq!(js_parser().language(), Language::JavaScript);
        }

        #[test]
        fn capabilities_reports_typescript_and_javascript_degradation() {
            for capabilities in [ts_parser().capabilities(), js_parser().capabilities()] {
                assert_eq!(
                    *capabilities.cyclomatic_complexity(),
                    MetricSupport::Supported
                );
                assert_eq!(*capabilities.economic_impact(), MetricSupport::Supported);
                assert_eq!(*capabilities.ecological_impact(), MetricSupport::Supported);
                match capabilities.io_in_loops() {
                    MetricSupport::Degraded(reason) => {
                        assert!(
                            reason.contains("dynamic import"),
                            "expected the dynamic-import abstention reason, got: {}",
                            reason
                        );
                    }
                    other => panic!("expected io_in_loops to be Degraded, got {:?}", other),
                }
                match capabilities.call_graph() {
                    MetricSupport::Degraded(reason) => {
                        assert!(
                        reason.contains("merges into a single call-graph node"),
                        "expected the merge-not-mere-non-edge reason (retry, Dev-B F2), got: {}",
                        reason
                    );
                    }
                    other => panic!("expected call_graph to be Degraded, got {:?}", other),
                }
                // Q4 (human-approved ruling): cross_file_dependencies is
                // Unsupported in T1 — real dependency resolution is T4, and
                // reporting Degraded before the code exists would be a
                // measurement lie (ADR-0010).
                assert_eq!(
                    *capabilities.cross_file_dependencies(),
                    MetricSupport::Unsupported
                );
            }
        }

        #[test]
        fn resolve_dependencies_is_always_empty_for_typescript_and_javascript() {
            let source = "import { x } from './x';\nimport React from 'react';\nfunction f() {}";
            let ctx = DependencyContext::new(PathBuf::from("a.ts"), PathBuf::from("."), vec![]);
            let resolved = ts_parser().resolve_dependencies(source, &ctx).unwrap();
            assert!(
                resolved.is_empty(),
                "A3: deps_scm is an empty query in T1 — no edge is ever produced yet, got {:?}",
                resolved
            );
        }

        #[test]
        fn ecmascript_query_compiles_against_both_grammars() {
            // A non-compiling query panics inside `Query::new` — this test
            // IS the guard the tech spec's step 4/10 requires: parsing an
            // (empty) source with each constructed parser exercises
            // `parse_source` -> `run_pipeline` -> `Query::new(grammar, scm)`
            // for both grammars sharing the same `ecmascript.scm`.
            assert!(ts_parser().parse("").unwrap().is_empty());
            assert!(js_parser().parse("").unwrap().is_empty());
        }

        #[test]
        fn ecmascript_function_shaped_constructs_each_become_their_own_parsed_function() {
            // Fold-in 9 (retry, Dev-B F4) — `(source, expected_name)` pairs,
            // not a bare `len() == 1` count: the earlier count-only assertion
            // was too weak to catch a wrong grammar mapping (e.g. capturing
            // `(variable_declarator)` instead of `(arrow_function)` would
            // still yield `len() == 1`). Five of six constructs are NAMED —
            // only `arrow_function` has no `name` field in the grammar at all
            // (Q3, anonymous-function naming is T3) — asserting the real name
            // pins the capture AND the field-extraction together.
            let cases = [
                ("function foo() {}", "foo"),
                ("const bar = function foo() {};", "foo"),
                ("function* foo() {}", "foo"),
                ("const bar = function* foo() {};", "foo"),
                ("const bar = () => {};", "<unresolved>"),
                ("class C { foo() {} }", "foo"),
            ];
            for parser in ecmascript_parsers() {
                for (source, expected_name) in cases {
                    let functions = parser.parse(source).unwrap();
                    assert_eq!(
                        functions.len(),
                        1,
                        "source '{}': expected exactly one captured function, got {:?}",
                        source,
                        functions.iter().map(|f| &f.name).collect::<Vec<_>>()
                    );
                    assert_eq!(
                        functions[0].name, expected_name,
                        "source '{}': expected name '{}', got '{}'",
                        source, expected_name, functions[0].name
                    );
                }
            }
        }

        #[test]
        fn every_ecmascript_loop_kind_sets_has_loop_and_counts_one_decision_point() {
            let cases = [
                "function f() { for (let i = 0; i < 10; i++) {} }",
                "function f() { for (const x of xs) {} }",
                "function f() { for (const x in xs) {} }",
                "function f() { while (true) {} }",
                "function f() { do {} while (true); }",
            ];
            for parser in ecmascript_parsers() {
                for source in cases {
                    let functions = parser.parse(source).unwrap();
                    assert!(functions[0].has_loop, "source: {}", source);
                    assert_eq!(functions[0].decision_points, 1, "source: {}", source);
                }
            }
        }

        #[test]
        fn ecmascript_nested_loop_sets_has_nested_loop() {
            let source = "function f() { for (let i = 0; i < 10; i++) { while (true) {} } }";
            for parser in ecmascript_parsers() {
                let functions = parser.parse(source).unwrap();
                assert!(functions[0].has_nested_loop);
            }
        }

        #[test]
        fn ecmascript_sibling_loops_do_not_set_has_nested_loop() {
            let source = "function f() { for (let i = 0; i < 10; i++) {} while (true) {} }";
            for parser in ecmascript_parsers() {
                let functions = parser.parse(source).unwrap();
                assert!(!functions[0].has_nested_loop);
            }
        }

        // Pins the new "switch_case" | "switch_default" node-kind dispatch
        // (tech spec step 8) AND `max_switch_section_count`'s two-level
        // parent().parent() walk reaching `switch_statement` from a JS
        // `switch_case` (case -> switch_body -> switch_statement, same shape
        // as C#'s case -> switch_body(?) -> switch_statement — verified
        // against the real grammar, not assumed).
        #[test]
        fn ecmascript_switch_arms_count_branch_arms_and_decision_points() {
            let source =
                "function f() { switch (x) { case 1: break; case 2: break; default: break; } }";
            for parser in ecmascript_parsers() {
                let functions = parser.parse(source).unwrap();
                assert_eq!(functions[0].branch_arms, 3);
                assert_eq!(functions[0].decision_points, 3);
            }
        }

        // Dev-B F6 (retry, BLOCKING 5, human ruling D1) — a cascade of
        // empty-bodied `case` labels sharing one following body must count as
        // ONE decision point/branch arm, exactly like C#'s single
        // `switch_section` for the same construct — never one per label. The
        // parity assertion against the EQUIVALENT C# source is the point (a
        // JS-only count would not catch a language-specific divergence that
        // breaks ADR-0020 D4's cross-language comparability invariant).
        #[test]
        #[cfg(feature = "lang-csharp")]
        fn switch_case_cascade_counts_one_decision_point_matching_csharp() {
            let js_source =
                "function f() { switch (x) { case 1: case 2: doX(); break; default: break; } }";
            let cs_source = "class C { void M() { switch (x) { case 1: case 2: doX(); break; default: break; } } }";

            let js_functions = ts_parser().parse(js_source).unwrap();
            let cs_functions = TreeSitterCodeParser::csharp(Vec::new())
                .parse(cs_source)
                .unwrap();

            assert_eq!(
                js_functions[0].decision_points, cs_functions[0].decision_points,
                "JS decision_points={} must match C#'s decision_points={} for the \
             identical cascade construct (ADR-0020 D4 comparability)",
                js_functions[0].decision_points, cs_functions[0].decision_points
            );
            assert_eq!(
                js_functions[0].branch_arms, cs_functions[0].branch_arms,
                "JS branch_arms={} must match C#'s branch_arms={}",
                js_functions[0].branch_arms, cs_functions[0].branch_arms
            );
            // Pin the actual value too (2: the case1+case2 cascade counts as
            // ONE, default counts as one more) — not just "equal to whatever
            // C# happens to produce", in case BOTH were silently wrong.
            assert_eq!(js_functions[0].decision_points, 2);
            assert_eq!(js_functions[0].branch_arms, 2);
        }

        #[test]
        fn ecmascript_and_or_nullish_operators_each_count_one_decision_point() {
            let cases = ["let x = a && b;", "let x = a || b;", "let x = a ?? b;"];
            for parser in ecmascript_parsers() {
                for source in cases {
                    let source = format!("function f() {{ {} }}", source);
                    let functions = parser.parse(&source).unwrap();
                    assert_eq!(functions[0].decision_points, 1, "source: {}", source);
                }
            }
        }

        #[test]
        fn ecmascript_ternary_operator_counts_as_one_decision_point() {
            let source = "function f() { let y = x > 0 ? 1 : 2; }";
            for parser in ecmascript_parsers() {
                let functions = parser.parse(source).unwrap();
                assert_eq!(functions[0].decision_points, 1);
            }
        }

        #[test]
        fn ecmascript_calls_are_tracked_in_source_order() {
            let source = "function f() { foo(); bar.baz(); }";
            for parser in ecmascript_parsers() {
                let functions = parser.parse(source).unwrap();
                assert_eq!(
                    functions[0].calls,
                    vec!["foo".to_string(), "bar.baz".to_string()]
                );
            }
        }

        // Dev-B F2 (retry, BLOCKING 3) — pins the FACT the widened call_graph
        // degradation string now honestly describes: two anonymous functions
        // in the same file are BOTH recorded under the identical placeholder
        // name ("<unresolved>" — `arrow_function` has no `name` field), not
        // merely "each resolves no edge". This is what lets
        // `CallGraph::build`'s `edges.insert("<unresolved>", ...)` /
        // `direct_complexity.insert("<unresolved>", ...)` overwrite each
        // other downstream (hexagon logic, out of this adapter's scope to
        // fix — T3 owns precise naming) — this test only pins what THIS
        // adapter hands the hexagon: two same-named `ParsedFunction`s of
        // DIFFERENT complexity, proving the merge is a real collision, not a
        // same-value coincidence.
        #[test]
        fn two_anonymous_functions_in_one_file_share_the_same_unresolved_name() {
            let source = "function host() { xs.map(x => x > 0 ? 1 : 2); ys.filter(y => y); }";
            for parser in ecmascript_parsers() {
                let functions = parser.parse(source).unwrap();
                let anonymous: Vec<_> = functions
                    .iter()
                    .filter(|f| f.name == "<unresolved>")
                    .collect();
                assert_eq!(
                    anonymous.len(),
                    2,
                    "expected both arrow functions to be captured, got {:?}",
                    functions.iter().map(|f| &f.name).collect::<Vec<_>>()
                );
                assert_ne!(
                    anonymous[0].decision_points, anonymous[1].decision_points,
                    "the two arrow functions must have DIFFERENT complexity for the \
                 collision to be observable (not a same-value coincidence)"
                );
            }
        }

        #[test]
        fn ecmascript_loop_call_classifies_io_unknown_or_not_io() {
            let cases = [
                ("fs.readFile(p, cb);", IoClassification::Io),
                ("fetch(url);", IoClassification::Unknown),
                ("list.push(x);", IoClassification::NotIo),
            ];
            for (call, expected) in cases {
                let source = format!(
                    "function f() {{ for (let i = 0; i < 10; i++) {{ {} }} }}",
                    call
                );
                let functions = js_parser().parse(&source).unwrap();
                assert_eq!(functions[0].calls_in_loops.len(), 1, "case: {}", call);
                assert_eq!(
                    functions[0].calls_in_loops[0].io, expected,
                    "case: {}",
                    call
                );
            }
        }

        // Fold-in 6 (retry, Security MEDIUM #2) — the suspicious-marker table
        // was missing common network/process I/O markers entirely, so these
        // calls landed in `NotIo` — an ASSERTED negative, not an abstention —
        // and "Appels en boucle non classifiables: 0" reads to the operator as
        // "everything was classified" while real I/O was silently dropped.
        // Never added to the CONFIDENT table (ADR-0016 mandates abstention,
        // not a syntax-unproven assertion of `Io`).
        #[test]
        fn ecmascript_network_and_process_markers_classify_unknown_never_not_io() {
            let cases = [
                "http.get(url);",
                "https.request(opts);",
                "net.connect(port);",
                "dns.lookup(host);",
                "child_process.exec(cmd);",
                "cp.execSync(cmd);",
                "cp.spawn(cmd);",
            ];
            for call in cases {
                let source = format!(
                    "function f() {{ for (let i = 0; i < 10; i++) {{ {} }} }}",
                    call
                );
                let functions = js_parser().parse(&source).unwrap();
                assert_eq!(functions[0].calls_in_loops.len(), 1, "case: {}", call);
                assert_eq!(
                    functions[0].calls_in_loops[0].io,
                    IoClassification::Unknown,
                    "case: {} — must abstain (Unknown), never assert NotIo nor Io",
                    call
                );
            }
        }

        // Tech spec step 8's mandatory falsification test — verifies the
        // ADR-0020 grammar precondition `owning_function_indices` documents:
        // a wrapping capture sharing a nested @function's exact start_byte
        // was, before the down-stack-scan fix, silently DROPPED (matched
        // against the innermost function via `open.last()`, whose end_byte is
        // always smaller than the wrapping call's, so the containment check
        // failed and the capture was never assigned to ANY function — not
        // even the correct outer one). `outer`'s IIFE call must still be
        // recorded in its `calls`.
        #[test]
        fn iife_call_sharing_start_byte_with_its_own_function_expression_is_attributed_to_outer() {
            // Retry 2 (BLOCKING 3, Dev-B + Security convergent) — the
            // `!`/`void` forms tested in round 1 (`function_expression` sits
            // DIRECTLY in the call's "function" field) are the MARGINAL IIFE
            // shape. The PARENTHESIZED form — `(function(){})()`,
            // `(() => {})()`, `(async () => {})()` — is the textbook one:
            // the callee node is a `parenthesized_expression` WRAPPING the
            // function-shaped node, so the direct `Node::id()` membership
            // check used to miss it entirely (the wrapper itself is never
            // one of the file's captured `@function`s).
            // Sweep (Dev-B MINOR C) — two more transparent wrappers Dev-B's
            // 23-form AST probe found unhandled: a comma-operator
            // `sequence_expression` (the IIFE is the LAST operand, the
            // comma-operator's "resulting value") and a leading `comment`
            // (a grammar "extra" — a NAMED node that can appear before the
            // real expression inside ANY wrapper, which is exactly why the
            // old doc's "always the first named child" claim was wrong).
            let cases = [
                "function outer() { !function () { doIo(); }(); }",
                "function outer() { void function () { doIo(); }(); }",
                "function outer() { (function () { doIo(); })(); }",
                "function outer() { (() => { doIo(); })(); }",
                "function outer() { (async () => { doIo(); })(); }",
                "function outer() { (a, function () { doIo(); })(); }",
                "function outer() { (/* c */ function () { doIo(); })(); }",
            ];
            for parser in ecmascript_parsers() {
                for source in cases {
                    let functions = parser.parse(source).unwrap();
                    let outer = functions
                        .iter()
                        .find(|f| f.name == "outer")
                        .expect("outer function must be captured");
                    // The discriminating signal is that this call is present
                    // in `outer.calls` AT ALL: before the down-stack fix, the
                    // innermost-only ownership check attributed it to the
                    // (smaller-end_byte) inner function expression, failed
                    // the containment test there, and dropped it from EVERY
                    // function. When the callee node is itself function-
                    // shaped (possibly wrapped in a transparent
                    // `parenthesized_expression`), the recorded name is
                    // "<anonymous>" — never the callee's raw source text
                    // (the whole function body), which would falsely trip
                    // suspicious I/O markers by `contains` on an arbitrary
                    // substring of the body.
                    assert!(
                        outer.calls.iter().any(|c| c == "<anonymous>"),
                        "source '{}': the IIFE's own call must be attributed to `outer` as \
                         \"<anonymous>\", not silently dropped, and never as the raw callee \
                         body text — outer.calls = {:?}",
                        source,
                        outer.calls
                    );
                }
            }
        }

        // Dev-B F3 (retry, BLOCKING 4) — an IIFE nested in a loop must not
        // classify as `Unknown` I/O merely because its body text happens to
        // CONTAIN a suspicious marker substring (e.g. "fetch"). Before this
        // fix the recorded call name was the entire callee body
        // ("function(){ prefetchAll(); }"), and `classify_call`'s `contains`
        // check on suspicious markers would false-positive on ANY marker
        // substring appearing anywhere in that blob.
        #[test]
        fn iife_call_in_loop_never_false_classifies_from_its_own_body_text() {
            let source = "function f(n) { for (let i = 0; i < n; i++) { !function(){ prefetchAll(); }(); } }";
            for parser in ecmascript_parsers() {
                let functions = parser.parse(source).unwrap();
                let outer = functions
                    .iter()
                    .find(|f| f.name == "f")
                    .expect("f must be captured");
                let iife_call = outer
                    .calls_in_loops
                    .iter()
                    .find(|c| c.name == "<anonymous>")
                    .expect("the IIFE call must be tracked in calls_in_loops as \"<anonymous>\"");
                assert_eq!(
                    iife_call.io,
                    IoClassification::NotIo,
                    "an anonymous callee's own body text must never feed the suspicious-marker \
                 classifier — got {:?}",
                    iife_call.io
                );
            }
        }

        // Retry 2 (BLOCKING 3) — Security's exact reproduction: the
        // TEXTBOOK parenthesized IIFE form, `(function () { ... })()`,
        // false-classified as `Unknown` I/O before the fix because its
        // whole body text (containing the substring "fetch" inside
        // "prefetchAll") was recorded as the call name and matched by
        // `classify_call`'s `contains` check.
        #[test]
        fn parenthesized_iife_call_in_loop_never_false_classifies_from_its_own_body_text() {
            let source = "function outer() { for (const u of urls) { \
                           (function () { const prefetchAll = 1; return prefetchAll; })(); \
                           } }";
            for parser in ecmascript_parsers() {
                let functions = parser.parse(source).unwrap();
                let outer = functions
                    .iter()
                    .find(|f| f.name == "outer")
                    .expect("outer must be captured");
                let iife_call = outer
                    .calls_in_loops
                    .iter()
                    .find(|c| c.name == "<anonymous>")
                    .expect(
                        "the parenthesized IIFE call must be tracked in calls_in_loops as \
                         \"<anonymous>\"",
                    );
                assert_eq!(
                    iife_call.io,
                    IoClassification::NotIo,
                    "a parenthesized anonymous callee's own body text must never feed the \
                     suspicious-marker classifier — got {:?}",
                    iife_call.io
                );
            }
        }

        // Sweep (Dev-B MINOR C) — the two remaining forms from Dev-B's
        // 23-form probe are TypeScript-only syntax (invalid in plain JS,
        // so `ts_parser()` only): `type_assertion` (`<Type>expr`, the
        // type comes BEFORE the expression — the wrapped expression is
        // the LAST named child, not the first) and `satisfies_expression`
        // (`expr satisfies Type`, the expression comes first, same shape
        // as the already-handled `as_expression`).
        #[test]
        fn iife_wrapped_in_ts_only_transparent_forms_is_attributed_as_anonymous() {
            let cases = [
                "function outer() { (<any>function () { doIo(); })(); }",
                "function outer() { (function () { doIo(); } satisfies Function)(); }",
            ];
            for source in cases {
                let functions = ts_parser().parse(source).unwrap();
                let outer = functions
                    .iter()
                    .find(|f| f.name == "outer")
                    .expect("outer function must be captured");
                assert!(
                    outer.calls.iter().any(|c| c == "<anonymous>"),
                    "source '{}': the IIFE's own call must be attributed to `outer` as \
                     \"<anonymous>\" — outer.calls = {:?}",
                    source,
                    outer.calls
                );
            }
        }
    } // mod ecmascript_tests

    // Fold-in 8 (retry, Security LOW) — the C#-only Drop-safety guard
    // (`csharp_tests::dropping_a_deeply_nested_tree_does_not_abort_the_
    // process`) is extended to TS/JS. Its body below constructs ONLY the
    // TypeScript and JavaScript grammars (C# already has its own twin in
    // `csharp_tests`) — it lives at this outer scope simply because it is
    // neither purely a C# nor purely an ecmascript-family test, not
    // because it needs all three grammars in one test (retry 2, MINOR 5:
    // the previous `#[cfg(all(lang-csharp, lang-typescript))]` gate was
    // wrong — this test never touches `tree_sitter_c_sharp` at all, so
    // gating on `lang-csharp` too silently skipped it under
    // `--no-default-features --features lang-typescript`). No abort was
    // observed for TS/JS in the Security lane's own runs (100k nested
    // arrows, 20k nested IIFEs both exited cleanly as `SourceTooComplex`)
    // — this guards against a FUTURE regression, not a present defect.
    #[test]
    #[cfg(feature = "lang-typescript")]
    fn dropping_a_deeply_nested_tree_does_not_abort_the_process_on_typescript_and_javascript() {
        // TypeScript: 100k nested IIFE-shaped arrow-function calls.
        {
            let mut source = String::new();
            for _ in 0..100_000 {
                source.push_str("(() => ");
            }
            source.push('1');
            for _ in 0..100_000 {
                source.push_str(")()");
            }
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
                .expect("grammar must load");
            let tree = parser.parse(&source, None).expect("parse must succeed");
            drop(tree);
        }

        // JavaScript: 20k nested IIFEs.
        {
            let mut source = String::from("function outer() {\n");
            for _ in 0..20_000 {
                source.push_str("!function(){\n");
            }
            source.push_str("doIt();\n");
            for _ in 0..20_000 {
                source.push_str("}();\n");
            }
            source.push_str("}\n");
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter_javascript::LANGUAGE.into())
                .expect("grammar must load");
            let tree = parser.parse(&source, None).expect("parse must succeed");
            drop(tree);
        }

        // Reaching this line is the proof: the process survived every Drop.
    }
}
