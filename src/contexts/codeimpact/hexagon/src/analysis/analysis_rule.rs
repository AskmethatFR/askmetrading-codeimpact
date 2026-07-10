#[derive(Clone, Debug, PartialEq)]
pub enum AnalysisRule {
    CyclomaticComplexity,
    IoInLoops,
}

impl AnalysisRule {
    pub fn all() -> Vec<Self> {
        vec![Self::CyclomaticComplexity, Self::IoInLoops]
    }
}
