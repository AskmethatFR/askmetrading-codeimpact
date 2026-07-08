#[derive(Clone, Debug, PartialEq)]
pub enum AnalysisRule {
    CyclomaticComplexity,
}

impl AnalysisRule {
    pub fn all() -> Vec<Self> {
        vec![Self::CyclomaticComplexity]
    }
}
