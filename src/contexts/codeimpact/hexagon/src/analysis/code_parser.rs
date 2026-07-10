use super::errors::AnalysisError;

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
    /// Tuples of (call_name, line, col) — I/O calls detected inside loops.
    /// CodeLocation is not used here because the call name is not a file path.
    pub calls_in_loops: Vec<(String, usize, usize)>,
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
    fn parse_file_dependencies(
        &self,
        source: &str,
    ) -> Result<Vec<String>, AnalysisError>;
}
