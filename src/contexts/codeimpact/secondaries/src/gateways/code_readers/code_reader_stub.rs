use std::collections::HashMap;
use std::path::{Path, PathBuf};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;
use codeimpact_hexagon::analysis::FileFilter;
use codeimpact_hexagon::analysis::SourceFileListing;
use codeimpact_hexagon::analysis::UnmeasurableReason;

#[derive(Default)]
pub struct CodeReaderStub {
    sources: HashMap<PathBuf, String>,
    source_files: Vec<PathBuf>,
    dropped_files: Vec<(PathBuf, UnmeasurableReason)>,
    unexplored_subtree: bool,
}

impl CodeReaderStub {
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            source_files: Vec::new(),
            dropped_files: Vec::new(),
            unexplored_subtree: false,
        }
    }

    pub fn add_source(&mut self, path: PathBuf, source: String) {
        self.sources.insert(path, source);
    }

    pub fn add_source_file(&mut self, path: PathBuf) {
        self.source_files.push(path);
    }

    /// Simulates a file the driven adapter's WALK decided not to include in
    /// `source_files` at all (Security HIGH, #128 retry 1) — e.g. too large
    /// for the adapter's own walk-time size cap. Calling `add_source`
    /// without `add_source_file` has no equivalent in the real
    /// `FileSystemCodeReader` (a file that was never listed was also never
    /// read), so this method exists to let a use-case-level test pin the
    /// fold-in behavior without a real filesystem walk.
    pub fn add_dropped_file(&mut self, path: PathBuf, reason: UnmeasurableReason) {
        self.dropped_files.push((path, reason));
    }

    /// Simulates the walk leaving at least one directory subtree
    /// unexplored (#128 retry 2, Security HIGH) — the real
    /// `FileSystemCodeReader`'s equivalent is a `MAX_WALK_DEPTH`
    /// truncation or a directory-level access error, neither reproducible
    /// without a real filesystem walk. Mirrors `add_dropped_file`'s role:
    /// lets a use-case-level test pin the fold-in behavior directly.
    pub fn mark_subtree_unexplored(&mut self) {
        self.unexplored_subtree = true;
    }
}

impl CodeReader for CodeReaderStub {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError> {
        self.sources
            .get(target.path())
            .cloned()
            .ok_or_else(|| AnalysisError::IoError("fichier introuvable".to_string()))
    }

    fn list_source_files(
        &self,
        _dir: &Path,
        _extensions: &[&str],
        _filter: &FileFilter,
    ) -> Result<SourceFileListing, AnalysisError> {
        Ok(SourceFileListing {
            files: self.source_files.clone(),
            default_excluded_count: 0,
            dropped_files: self.dropped_files.clone(),
            unexplored_subtree: self.unexplored_subtree,
        })
    }
}
