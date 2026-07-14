use super::errors::AnalysisError;

/// A call — method or free-function — recorded at `loop_depth > 0`.
///
/// `is_io` classifies the call; it does not filter it. The parser records
/// every nested call as a fact, and each detector decides which facts it
/// cares about.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopCall {
    pub name: String,
    pub line: usize,
    pub col: usize,
    pub is_io: bool,
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
    pub match_arms: u32,
    pub calls_in_loops: Vec<LoopCall>,
}

pub trait CodeParser: Send + Sync {
    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError>;

    /// Parse raw file dependencies (mod/use declarations) from source code.
    ///
    /// Returns strings in format:
    /// - `"mod:<name>"` for `mod foo;` declarations
    /// - `"use:<path>"` for `use foo::bar;` declarations
    ///
    /// External crates (`std::`, `core::`, `alloc::`) are filtered out.
    fn parse_file_dependencies(&self, source: &str) -> Result<Vec<String>, AnalysisError>;
}
