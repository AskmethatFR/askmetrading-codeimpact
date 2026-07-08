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
}

pub trait CodeParser: Send + Sync {
    fn parse(&self, source: &str) -> Result<Vec<ParsedFunction>, AnalysisError>;
}
