use std::path::{Path, PathBuf};

use super::analysis_target::AnalysisTarget;
use super::errors::AnalysisError;
use super::file_filter::FileFilter;

pub trait CodeReader: Send + Sync {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError>;

    /// Lists every file under `dir` whose extension (no leading dot) is one
    /// of `extensions` — language-agnostic (US14 L3): the port no longer
    /// knows "Rust", it only filters on whatever extension set the caller
    /// passes. The composition root (`RunAnalysis`) supplies `&["rs"]` to
    /// preserve today's behavior exactly. `filter` (US31) additionally
    /// restricts the walk to files matching `include` (when non-empty) and
    /// not matching `exclude`, and optionally honors `.gitignore`.
    /// `FileFilter::unrestricted()` reproduces the pre-US31 walk exactly
    /// (D4). The two filters compose: a file is kept iff its extension is
    /// in `extensions` AND `filter`'s include/exclude/gitignore predicate
    /// holds.
    fn list_source_files(
        &self,
        dir: &Path,
        extensions: &[&str],
        filter: &FileFilter,
    ) -> Result<Vec<PathBuf>, AnalysisError>;

    /// Resolves `dir` to the SAME canonical representation
    /// `list_source_files` returns its paths in (US16 T5, Security
    /// CRITICAL retry #1) — a caller that derives a path from `dir` (e.g.
    /// a configured `sourceRoots` entry joined onto the project root) and
    /// needs to compare it against `list_source_files`'s own results must
    /// canonicalize `dir` the SAME way first, or the comparison silently
    /// never matches (a raw CLI `--path` vs. `FileSystemCodeReader`'s
    /// canonicalized output). Default: identity — correct for a reader
    /// with no real filesystem of its own (`CodeReaderStub`: every
    /// fixture path already agrees on representation by construction, so
    /// canonicalizing would be a no-op at best, or corrupt the fixture at
    /// worst).
    fn canonical_root(&self, dir: &Path) -> PathBuf {
        dir.to_path_buf()
    }
}
