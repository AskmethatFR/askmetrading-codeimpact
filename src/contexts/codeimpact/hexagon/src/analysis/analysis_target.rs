use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum TargetType {
    File,
    Project,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisTarget {
    path: PathBuf,
    target_type: TargetType,
}

impl AnalysisTarget {
    pub fn new(path: PathBuf, target_type: TargetType) -> Self {
        Self { path, target_type }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn target_type(&self) -> &TargetType {
        &self.target_type
    }
}
