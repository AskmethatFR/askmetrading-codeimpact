use std::path::Path;

use codeimpact_hexagon::analysis::AlertThresholds;
use codeimpact_hexagon::analysis::AnalysisConfig;
use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::ConfigReaderPort;
use codeimpact_hexagon::analysis::FileFilter;

const CONFIG_FILE_NAME: &str = ".codeimpact.json";
/// A thresholds/filter config is a handful of bytes; 1 MiB is a generous cap
/// that still refuses anything resembling an attack payload (mirrors
/// `FileSystemCodeReader::MAX_FILE_SIZE`'s discipline at a scale fitting a
/// config file rather than a source file).
const MAX_CONFIG_SIZE: u64 = 1024 * 1024;
const ERR_NOT_FOUND: &str = "fichier de configuration introuvable";
const ERR_NOT_REGULAR: &str = "la configuration n'est pas un fichier régulier";
const ERR_TOO_LARGE: &str = "fichier de configuration trop volumineux (max 1 Mo)";
const ERR_UNREADABLE: &str = "impossible de lire la configuration";

fn default_respect_gitignore() -> bool {
    true
}

/// Full `.codeimpact.json` schema (US8 `thresholds` + US31
/// `include`/`exclude`/`respectGitignore`, plus reserved forward-compat
/// keys). `deny_unknown_fields` (US31, D-none — a change from US8's
/// tolerant schema): every reserved key is declared, even the ones this
/// reader does not yet wire up (`languages`, `sourceRoots`, `extensions`,
/// `parser`, `ioSignatures` — typed as loose `serde_json::Value`, parsed
/// then discarded) — a real future key therefore does not break the
/// schema, but a typo in ANY key now surfaces as an actionable error
/// instead of being silently swallowed.
#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CodeImpactConfig {
    #[serde(default, rename = "$schema")]
    _schema: Option<String>,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    /// D4: a config file that IS present but omits this key defaults to
    /// `true` — distinct from the "no config file at all" case, which
    /// falls back to `FileFilter::unrestricted()` (`respect_gitignore`
    /// `false`) one layer up, in `read_config`.
    #[serde(default = "default_respect_gitignore", rename = "respectGitignore")]
    respect_gitignore: bool,
    #[serde(default)]
    thresholds: Option<ThresholdsSection>,
    #[serde(default, rename = "languages")]
    _languages: Option<serde_json::Value>,
    #[serde(default, rename = "sourceRoots")]
    _source_roots: Option<serde_json::Value>,
    #[serde(default, rename = "extensions")]
    _extensions: Option<serde_json::Value>,
    #[serde(default, rename = "parser")]
    _parser: Option<serde_json::Value>,
    #[serde(default, rename = "ioSignatures")]
    _io_signatures: Option<serde_json::Value>,
}

#[derive(serde::Deserialize, Default)]
struct ThresholdsSection {
    #[serde(default)]
    max_energy_kwh: Option<f64>,
    #[serde(default)]
    max_co2_grams: Option<f64>,
}

#[derive(Default)]
pub struct FileSystemConfigReader;

impl FileSystemConfigReader {
    pub fn new() -> Self {
        Self
    }

    /// Reads, validates, and parses a config file already known to exist at
    /// `path` (ADR-0006 discipline, mirrors `write_report_file`/
    /// `FileSystemCodeReader::read_source`): canonicalize, refuse anything
    /// that isn't a regular file (symlink/FIFO/dir — `symlink_metadata`
    /// does not follow the final component, so a symlink is caught before
    /// `read_to_string` would follow it), enforce the size cap, then parse.
    fn read_and_validate(&self, path: &Path) -> Result<AnalysisConfig, AnalysisError> {
        // Canonicalize only the PARENT directory, then re-join the file
        // name — never `canonicalize(path)` directly, which follows a
        // symlink straight to its target and would make the
        // `symlink_metadata` check below inspect the TARGET's metadata
        // instead of the symlink's own (the exact bypass a naive
        // full-path canonicalize opens; mirrors `write_report_file`'s
        // discipline at main.rs).
        let parent = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|_| AnalysisError::IoError(ERR_NOT_FOUND.to_string()))?;
        let file_name = path
            .file_name()
            .ok_or_else(|| AnalysisError::IoError(ERR_NOT_FOUND.to_string()))?;
        let resolved = canonical_parent.join(file_name);

        match std::fs::symlink_metadata(&resolved) {
            Ok(meta) if !meta.file_type().is_file() => {
                return Err(AnalysisError::IoError(ERR_NOT_REGULAR.to_string()));
            }
            Err(_) => return Err(AnalysisError::IoError(ERR_NOT_FOUND.to_string())),
            _ => {}
        }

        let metadata = std::fs::metadata(&resolved)
            .map_err(|_| AnalysisError::IoError(ERR_NOT_FOUND.to_string()))?;
        if metadata.len() > MAX_CONFIG_SIZE {
            return Err(AnalysisError::IoError(ERR_TOO_LARGE.to_string()));
        }

        let content = std::fs::read_to_string(&resolved)
            .map_err(|_| AnalysisError::IoError(ERR_UNREADABLE.to_string()))?;

        // `e` carries serde_json's own line/column and (on deny_unknown_fields)
        // the offending key name — actionable, and path-free by construction
        // (it only ever describes a position within `content`).
        let config: CodeImpactConfig = serde_json::from_str(&content).map_err(|e| {
            AnalysisError::AnalysisFailed(format!("configuration JSON invalide: {}", e))
        })?;

        let section = config.thresholds.unwrap_or_default();
        let thresholds = AlertThresholds::new(section.max_energy_kwh, section.max_co2_grams)
            .map_err(|e| AnalysisError::AnalysisFailed(e.to_string()))?;
        let filter = FileFilter::new(config.include, config.exclude, config.respect_gitignore)
            .map_err(|e| AnalysisError::AnalysisFailed(e.to_string()))?;

        Ok(AnalysisConfig::new(thresholds, filter))
    }
}

impl ConfigReaderPort for FileSystemConfigReader {
    fn read_config(
        &self,
        explicit_path: Option<&Path>,
        search_dirs: &[&Path],
    ) -> Result<Option<AnalysisConfig>, AnalysisError> {
        if let Some(explicit) = explicit_path {
            return self.read_and_validate(explicit).map(Some);
        }

        for dir in search_dirs {
            let candidate = dir.join(CONFIG_FILE_NAME);
            if candidate.is_file() {
                return self.read_and_validate(&candidate).map(Some);
            }
        }

        Ok(None)
    }
}
