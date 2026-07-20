use std::path::PathBuf;

use super::errors::AnalysisError;
use super::io_classification::IoClassification;
use super::language::Language;
use super::language_capabilities::LanguageCapabilities;

/// A call — method or free-function — recorded at `loop_depth > 0`.
///
/// `io` classifies the call; it does not filter it. The parser records
/// every nested call as a fact, and each detector decides which facts it
/// cares about. Three states, not a `bool` (#56 T2) — see `IoClassification`.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopCall {
    pub name: String,
    pub line: usize,
    pub col: usize,
    pub io: IoClassification,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedFunction {
    pub name: String,
    pub start_line: usize,
    pub calls: Vec<String>,
    pub has_loop: bool,
    pub has_nested_loop: bool,
    pub decision_points: u32,
    pub depth: u32,
    pub branch_arms: u32,
    pub calls_in_loops: Vec<LoopCall>,
}

/// The neutral (language-agnostic) context a `CodeParser` needs to resolve
/// `source`'s declared dependencies to actual files on disk (US14 L1) —
/// `current_file`'s location, the analyzed project's root, and every file
/// available to resolve against. The adapter owns the language-specific
/// module/namespace semantics (`crate::`, `super::`, `.rs`, `mod.rs` for
/// Rust); this hexagon type carries no opinion about any of that.
#[derive(Clone, Debug, PartialEq)]
pub struct DependencyContext {
    pub current_file: PathBuf,
    pub project_root: PathBuf,
    pub available_files: Vec<PathBuf>,
}

impl DependencyContext {
    pub fn new(
        current_file: PathBuf,
        project_root: PathBuf,
        available_files: Vec<PathBuf>,
    ) -> Self {
        Self {
            current_file,
            project_root,
            available_files,
        }
    }
}

pub trait CodeParser: Send + Sync {
    /// The language this adapter parses (US16 T2) — the key `ParserRegistry`
    /// dispatches on.
    fn language(&self) -> Language;

    /// What this adapter can measure for its language (US16 T2, human-
    /// approved Q1 seam) — a forward-compat hook: T2 constructs only
    /// `LanguageCapabilities::all_supported`, T3 is free to report a
    /// degraded/unsupported metric without reopening this trait.
    fn capabilities(&self) -> LanguageCapabilities;

    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError>;

    /// Resolves `source`'s declared dependencies (Rust's `mod`/`use`, or
    /// whatever the adapter's language calls the equivalent) to actual file
    /// paths within `ctx.available_files` (US14 L1/L2). A dependency that
    /// cannot be resolved to any available file is simply absent from the
    /// result — never an error. The domain never sees the language's own
    /// declaration syntax; it only ever sees resolved `PathBuf`s.
    fn resolve_dependencies(
        &self,
        source: &str,
        ctx: &DependencyContext,
    ) -> Result<Vec<PathBuf>, AnalysisError>;
}
