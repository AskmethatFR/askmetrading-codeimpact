use std::path::PathBuf;

/// Value Object — cible de l'analyse (fichier ou projet).
#[derive(Clone, Debug, PartialEq)]
pub enum TargetType {
    File,
    Project,
}

/// Value Object — cible de l'analyse avec son chemin et son type.
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
