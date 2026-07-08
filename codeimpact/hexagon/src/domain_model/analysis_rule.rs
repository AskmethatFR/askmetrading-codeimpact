#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnalysisRule {
    CyclomaticComplexity,
    IoInLoops,
    NestedDepth,
    AllocationHotspots,
}

impl AnalysisRule {
    pub fn all() -> Vec<Self> {
        vec![
            Self::CyclomaticComplexity,
            Self::IoInLoops,
            Self::NestedDepth,
            Self::AllocationHotspots,
        ]
    }
}