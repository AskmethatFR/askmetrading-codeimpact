use std::path::{Path, PathBuf};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::AnalysisTarget;
use codeimpact_hexagon::analysis::CodeReader;
use codeimpact_hexagon::analysis::FileFilter;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
const MAX_WALK_DEPTH: usize = 128;
const ERR_FILE_NOT_FOUND: &str = "fichier introuvable";
const ERR_INVALID_GLOB: &str = "motif de filtrage invalide (syntaxe glob)";

/// Compiles `patterns` into a matchable `GlobSet` (D1: glob compilation is
/// an adapter concern — `FileFilter` itself carries only validated raw
/// patterns, never a compiled matcher, so the hexagon stays zero-dep).
/// A pattern that is syntactically invalid glob surfaces as an anonymized
/// `AnalysisError` (AC4/ADR-0006) rather than a panic.
fn build_glob_set(patterns: &[String]) -> Result<GlobSet, AnalysisError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern)
            .map_err(|_| AnalysisError::AnalysisFailed(ERR_INVALID_GLOB.to_string()))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|_| AnalysisError::AnalysisFailed(ERR_INVALID_GLOB.to_string()))
}

#[derive(Default)]
pub struct FileSystemCodeReader;

impl FileSystemCodeReader {
    pub fn new() -> Self {
        Self
    }
}

impl CodeReader for FileSystemCodeReader {
    fn read_source(&self, target: &AnalysisTarget) -> Result<String, AnalysisError> {
        let path = target.path();
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| AnalysisError::IoError(ERR_FILE_NOT_FOUND.to_string()))?;

        let metadata = std::fs::metadata(&canonical)
            .map_err(|_| AnalysisError::IoError(ERR_FILE_NOT_FOUND.to_string()))?;

        if metadata.len() > MAX_FILE_SIZE {
            return Err(AnalysisError::IoError(
                "fichier trop volumineux (max 10 Mo)".to_string(),
            ));
        }

        std::fs::read_to_string(&canonical).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AnalysisError::IoError(ERR_FILE_NOT_FOUND.to_string()),
            std::io::ErrorKind::PermissionDenied => {
                AnalysisError::IoError("permission refusée".to_string())
            }
            _ => AnalysisError::IoError("erreur de lecture".to_string()),
        })
    }

    fn list_source_files(
        &self,
        dir: &Path,
        extensions: &[&str],
        filter: &FileFilter,
    ) -> Result<Vec<PathBuf>, AnalysisError> {
        let canonical_root = std::fs::canonicalize(dir)
            .map_err(|_| AnalysisError::IoError("dossier introuvable".to_string()))?;

        let include_set = build_glob_set(filter.include())?;
        let exclude_set = build_glob_set(filter.exclude())?;
        let include_is_empty = filter.include().is_empty();
        let respect_gitignore = filter.respect_gitignore();

        let mut files = Vec::new();
        let walker = WalkBuilder::new(&canonical_root)
            .follow_links(false)
            .max_depth(Some(MAX_WALK_DEPTH))
            .hidden(true)
            // `ignore`'s WalkBuilder exposes FOUR independent ignore-source
            // toggles (git_ignore/.gitignore, git_exclude/.git/info/exclude,
            // git_global/the user's global gitignore, ignore/.ignore files)
            // — all default to `true`. Gating only `git_ignore` left the
            // other three ON unconditionally, silently dropping files even
            // under `FileFilter::unrestricted()` (QA finding, retry 1).
            // Every source must move together with `respect_gitignore` so
            // "unrestricted" is byte-identical to the pre-US31 `walkdir`
            // walk, which honored none of them.
            .git_ignore(respect_gitignore)
            .git_exclude(respect_gitignore)
            .git_global(respect_gitignore)
            .ignore(respect_gitignore)
            // The walk root itself is not guaranteed to be an actual git
            // working tree (e.g. an extracted archive, a CI checkout
            // shallow-cloned without `.git`) — honoring `.gitignore` at the
            // root must not silently depend on that.
            .require_git(false)
            // `parents(false)` (Security finding, retry 1): the walker must
            // NEVER consult ignore state from OUTSIDE the analyzed
            // directory. `parents(true)` would read .gitignore/.ignore from
            // every ancestor up to `/`, letting a party outside the
            // repository hide source files from a shared CI host's
            // ancestor directories and evade the --strict energy/CO2 gate
            // (ADR-0017).
            .parents(false)
            .build();

        for entry in walker {
            match entry {
                Ok(entry) => {
                    let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
                    if !is_file {
                        continue;
                    }
                    let path = entry.path();
                    if !path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| extensions.contains(&ext))
                    {
                        continue;
                    }
                    let relative = path.strip_prefix(&canonical_root).unwrap_or(path);
                    let keep = (include_is_empty || include_set.is_match(relative))
                        && !exclude_set.is_match(relative);
                    if !keep {
                        continue;
                    }
                    match std::fs::metadata(path) {
                        Ok(meta) if meta.len() <= MAX_FILE_SIZE => {
                            files.push(path.to_path_buf());
                        }
                        Ok(_) => {
                            eprintln!(
                                "Avertissement: fichier ignoré (trop volumineux): {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            );
                        }
                        Err(_) => {
                            eprintln!(
                                "Avertissement: fichier ignoré (illisible): {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Avertissement: erreur d'accès: {}", e);
                }
            }
        }

        Ok(files)
    }

    /// Real canonicalization (US16 T5, Security CRITICAL retry #1) —
    /// falls back to `dir` unchanged when it does not exist on disk,
    /// mirroring `html/view_model.rs`'s `build_tree` fallback (the same
    /// representation-mismatch class of bug, fixed the same way there).
    fn canonical_root(&self, dir: &Path) -> PathBuf {
        std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
    }
}
