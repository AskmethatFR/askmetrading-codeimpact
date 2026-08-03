use codeimpact_hexagon::analysis::MetricSupport;

/// How this language's adapter resolves `resolve_dependencies` (US17 T4.1,
/// AD-1) — data on the profile, never a `match self.language` branch in
/// `resolve_dependencies` itself. A `match self.language` would have worked
/// for two strategies and decomposed at the third (ADR-0029 already counts
/// this generalization among US17's real costs); a 5th language extends by
/// writing a profile, never by touching `resolve_dependencies`
/// (`/Users/alexteixeira/.claude/knowledge-base/solid/ocp.md`; Strategy as
/// data over a trait object —
/// `/Users/alexteixeira/.claude/knowledge-base/design-patterns/catalog.md`,
/// there being no per-strategy behavior beyond which arm runs).
/// `pub` and reachable through `pub mod language_profile` regardless of
/// which `lang-*` feature is enabled, so neither variant trips `dead_code`
/// under a single-feature build.
pub enum DepsStrategy {
    /// C#'s `using`/namespace resolution (ADR-0023): a project-global
    /// `namespace -> declaring-files` index built once per scan.
    NamespaceIndex,
    /// TypeScript/JavaScript's relative-`import`/`require` resolution.
    /// Empty in T4.1 (`resolve_dependencies` returns no edge for either
    /// language yet) — T4.3 fills this strategy in.
    RelativePath,
}

/// Everything `TreeSitterCodeParser` needs to parse one grammar: the
/// compiled `tree-sitter` language, the `.scm` query that captures the
/// constructs the range-containment post-processor turns into
/// `ParsedFunction`s, and the confident I/O prefixes (US16 T4.1) fed to
/// `classify_call`. Owned (`Vec<String>`), not `&'static`, because T4.3
/// appends user-configured prefixes at construction time — a runtime list,
/// not a compile-time constant. One profile per language keeps the parser
/// itself grammar-agnostic — US17 confirms the prediction this doc
/// comment made in T2: TypeScript/JavaScript arrived as a second (and
/// third) `LanguageProfile`, not a second parser type — `parse_source`,
/// `assign_captures_to_functions` and every other pipeline function in
/// `tree_sitter_code_parser.rs` are unchanged by this ticket.
pub struct LanguageProfile {
    pub grammar: tree_sitter::Language,
    pub scm: &'static str,
    /// The dependency-extraction query (US16 T5) — captures the
    /// namespace/module declarations and import-style directives a
    /// project-global pre-pass turns into a `namespace → declaring-files`
    /// index (`@namespace`/`@using` for C#). Separate from `scm` (the
    /// metric-extraction query): the two run over the same file for
    /// different purposes, at different times (`deps_scm` is also run,
    /// once per file, over every OTHER project file during the pre-pass).
    /// US17 T1: TypeScript/JavaScript's `deps_scm` is an EMPTY query —
    /// `resolve_dependencies` therefore returns empty for TS/JS, the same
    /// honest staging ADR-0020 used for C# in T2 (ruling A3 — real
    /// dependency resolution is T4).
    pub deps_scm: &'static str,
    pub io_table: Vec<String>,
    /// The confident/suspicious split's suspicious half (US16 T4.2) — text
    /// markers whose presence in a call's raw text abstains (`Unknown`)
    /// rather than asserts (`Io`), fed to `classify_call` alongside
    /// `io_table`. Per-language because C# and TypeScript/JavaScript name
    /// their own idiomatic unproven receivers differently (US17 Q2).
    pub suspicious_markers: Vec<String>,
    /// What this language's adapter can honestly claim for the three
    /// metrics that vary by grammar (US16 T3/T4/T5, US17 Q4) — `capabilities()`
    /// reads this instead of hardcoding per-language strings, so a new
    /// `LanguageProfile` is the only thing a new language needs to touch
    /// to answer the capabilities question.
    pub degradations: CapabilityDegradations,
    /// Which strategy `resolve_dependencies` dispatches to for this
    /// language (US17 T4.1, AD-1) — see `DepsStrategy`.
    pub deps: DepsStrategy,
}

/// The three metrics whose fidelity `TreeSitterCodeParser::capabilities()`
/// reports per-language (US17 Q4) — `cyclomatic_complexity`,
/// `economic_impact` and `ecological_impact` are `Supported` for every
/// tree-sitter-backed language today, so they are not data here (YAGNI: no
/// adapter has ever needed to vary them).
pub struct CapabilityDegradations {
    pub io_in_loops: MetricSupport,
    pub call_graph: MetricSupport,
    pub cross_file_dependencies: MetricSupport,
}
