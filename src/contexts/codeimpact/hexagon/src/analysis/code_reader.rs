use std::path::{Path, PathBuf};

use super::analysis_target::AnalysisTarget;
use super::errors::AnalysisError;
use super::file_filter::FileFilter;
use super::measurement::UnmeasurableReason;

/// The result of walking a directory for source files (#34 T2 MED-1,
/// ADR-0010): alongside the file list itself, surfaces how many walk
/// entries were dropped specifically because of a `DEFAULT_EXCLUDES`
/// pattern — a file skipped by a standing default (`node_modules/`,
/// `dist/`, `target/`, a minified file) is precisely the kind of
/// unmeasured thing ADR-0010 says the tool must say out loud, not pass
/// over in silence.
///
/// `default_excluded_count` is a COUNT OF PRUNED WALK ENTRIES, not a file
/// count: a walk-time-pruned directory (an entire `node_modules/`, say)
/// counts as ONE entry regardless of how many files live inside it,
/// because pruning means its contents are never enumerated to begin with
/// — the tool cannot honestly report a file count it never computed. A
/// file dropped by the post-walk fallback (e.g. `**/*.min.js`, which is
/// never walk-time-safe) IS counted individually, since that one file is
/// known precisely. Either way the count only includes entries that would
/// otherwise have been eligible (matching extension/include) — a file
/// dropped for an unrelated reason (wrong extension, excluded by the
/// user's OWN pattern) is not attributed to the standing defaults.
///
/// `Deref<Target = Vec<PathBuf>>` (deliberate): this struct replaces
/// `list_source_files`'s previous bare `Vec<PathBuf>` return value, and
/// every pre-existing call site across the codebase only ever read the
/// file list itself (`.iter()`, `.len()`, indexing) — Deref lets those
/// keep compiling and behaving identically, while the one new field is
/// reached explicitly by the handful of call sites that actually need it.
///
/// `dropped_files` (Security HIGH, #128 retry 1): a file the walk decided
/// NOT to include in `files` at all — too large for the adapter's own
/// walk-time size cap, unreadable, or dropped by a walker-level access
/// error — paired with WHY, so the use case that measures the project can
/// fold it into `unmeasurable_files` exactly as it already does for a file
/// that WAS read and then failed later (`RunAnalysis::read_all_sources`).
/// Before this field existed, a walk-time drop was reported only to
/// stderr: the file vanished from the gated sum with nothing telling the
/// gate it had ever existed — the exact `--strict` bypass Security
/// demonstrated by inflating one file past the walk-time cap.
///
/// `unexplored_subtree` (Security HIGH, #128 retry 2): unlike
/// `dropped_files` — which names an exact FILE the walk actually visited —
/// this names an absence the walker could never observe a file COUNT for:
/// a directory the walker truncated at `MAX_WALK_DEPTH` before descending
/// into it, or a directory whose listing failed outright (a
/// permission-denied subtree). Neither case can honestly populate
/// `dropped_files` (that would require enumerating files the walk never
/// visited), so this stays an UNQUANTIFIED boolean — "at least one subtree
/// was not explored," never a fabricated count. Security demonstrated the
/// consequence: a file nested past `MAX_WALK_DEPTH` vanished from BOTH
/// `files` and `dropped_files`, `GateCoverage` read `Complete`, and
/// `--strict` exited 0 on a project that genuinely breached.
/// `user_excluded_count` (#147, Volet B — MEDIUM): the twin of
/// `default_excluded_count` for patterns the ANALYZED REPOSITORY wrote in
/// `.codeimpact.json` (`FileFilter::exclude()`'s user subset, i.e. the
/// union minus the standing `DEFAULT_EXCLUDES`). Same entries-vs-files
/// semantics, same "reported, never gating" treatment as the default
/// count. Before this field existed, an `exclude` in `.codeimpact.json`
/// shrank the measured set with zero machine trace — no count, no JSON
/// field, no console line — defeating ADR-0006's documented remediation
/// (`--max-kwh`/`--max-co2` override thresholds, but nothing overrides
/// `exclude`): the count gives a CI a field to branch on.
///
/// `hidden_excluded_count` (#147, Volet A — HIGH): hidden-entry count,
/// never a fabricated file count — see ADR-0006 for the invariant.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceFileListing {
    pub files: Vec<PathBuf>,
    pub default_excluded_count: usize,
    pub user_excluded_count: usize,
    pub hidden_excluded_count: usize,
    pub dropped_files: Vec<(PathBuf, UnmeasurableReason)>,
    pub unexplored_subtree: bool,
}

impl std::ops::Deref for SourceFileListing {
    type Target = Vec<PathBuf>;

    fn deref(&self) -> &Vec<PathBuf> {
        &self.files
    }
}

pub trait CodeReader: Send + Sync {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError>;

    /// Lists every file under `dir` whose extension (no leading dot) is one
    /// of `extensions` — language-agnostic (US14 L3): the port no longer
    /// knows "Rust", it only filters on whatever extension set the caller
    /// passes. The composition root (`RunAnalysis`) supplies `&["rs"]` to
    /// preserve today's behavior exactly. `filter` (US31) additionally
    /// restricts the walk to files matching `include` (when non-empty) and
    /// not matching `exclude`, and optionally honors `.gitignore`.
    /// `FileFilter::unrestricted()` no longer reproduces the pre-US31 walk
    /// byte-for-byte (F2/F3, #34 T2 review sweep): it carries no *user*
    /// restriction, but it DOES carry the standing `DEFAULT_EXCLUDES`
    /// (#34 T2) — vendored/generated/build-artifact output is excluded
    /// even with no config file at all. The two filters compose: a file is
    /// kept iff its extension is in `extensions` AND `filter`'s
    /// include/exclude/gitignore predicate holds.
    fn list_source_files(
        &self,
        dir: &Path,
        extensions: &[&str],
        filter: &FileFilter,
    ) -> Result<SourceFileListing, AnalysisError>;

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
