use std::path::{Path, PathBuf};

use super::alert_thresholds::AlertThresholds;
use super::analysis_config::AnalysisConfig;
use super::analysis_rule::AnalysisRule;
use super::analysis_target::{AnalysisTarget, TargetType};
use super::code_location::CodeLocation;
use super::code_metrics::CodeMetrics;
use super::code_parser::{CodeParser, DependencyContext};
use super::code_reader::CodeReader;
use super::ecological_impact::EcologicalImpactEstimator;
use super::errors::AnalysisError;
use super::file_consumption_graph::{FileConsumptionGraph, UnmeasurableFile};
use super::gated_output::GatedOutput;
use super::io_in_loop_warning::IoInLoopWarning;
use super::measurement::UnmeasurableReason;
use super::parser_registry::ParserRegistry;
use super::proactive_analyzer;
use super::report_writer::ReportWriter;

pub struct RunAnalysis {
    code_reader: Box<dyn CodeReader>,
    reporter: Box<dyn ReportWriter>,
    registry: ParserRegistry,
}

impl RunAnalysis {
    pub fn new(
        code_reader: Box<dyn CodeReader>,
        reporter: Box<dyn ReportWriter>,
        registry: ParserRegistry,
    ) -> Self {
        Self {
            code_reader,
            reporter,
            registry,
        }
    }

    /// Resolves `path`'s extension to the `CodeParser` registered for its
    /// language (US16 T2) — `Err(Unmeasurable(UnsupportedLanguage))` for an
    /// extension no registered adapter claims, never a silent mis-dispatch
    /// to another language's parser and never a panic.
    fn dispatch_or_unsupported(&self, path: &Path) -> Result<&dyn CodeParser, AnalysisError> {
        self.registry
            .dispatch(path)
            .ok_or(AnalysisError::Unmeasurable(
                UnmeasurableReason::UnsupportedLanguage,
            ))
    }

    /// `config` (US8 thresholds + US31 file filter): thresholds are
    /// evaluated against the project's aggregate impact on the project
    /// path, and against the file's own impact on a single-file target (T3
    /// extended the gate to `CodeMetrics`). `AnalysisConfig::defaults()`
    /// reproduces the pre-US8/US31 behavior exactly (AC6/D4): `evaluate`
    /// against no configured threshold never breaches, and an unrestricted
    /// filter walks every file exactly as before.
    pub fn handle(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
        config: &AnalysisConfig,
    ) -> Result<GatedOutput<()>, AnalysisError> {
        if *target.target_type() == TargetType::Project {
            return self.handle_project(target, rules, config);
        }
        let parser = self.dispatch_or_unsupported(target.path())?;
        let source = self.code_reader.read_source(target)?;
        let metrics = proactive_analyzer::analyze(&source, rules, parser)?;
        let metrics = Self::set_file_paths(metrics, target.path());
        let metrics = Self::gate_metrics(metrics, config.thresholds());
        let report = metrics.threshold_report().cloned().unwrap_or_default();
        self.reporter.write_console(&metrics)?;
        Ok(GatedOutput::new((), report))
    }

    fn handle_project(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
        config: &AnalysisConfig,
    ) -> Result<GatedOutput<()>, AnalysisError> {
        let extensions = self.registry.extensions();
        let files =
            self.code_reader
                .list_source_files(target.path(), &extensions, config.file_filter())?;
        let project_root = target.path().clone();
        let source_roots = resolve_source_roots(&project_root, config.source_roots());

        let mut per_file: Vec<(PathBuf, CodeMetrics)> = Vec::new();
        let mut all_deps: Vec<super::file_consumption_graph::FileDependency> = Vec::new();
        let mut unmeasurable: Vec<UnmeasurableFile> = Vec::new();

        // Pass 1: read every file's source ONCE (US16 T5) — a
        // project-global dependency resolver (the C# namespace index)
        // needs every OTHER file's text before it can resolve even the
        // FIRST file's `using`s, not just the current one's, so every
        // source must be in hand before the per-file resolution pass
        // below runs.
        let file_sources = self.read_all_sources(&files, &mut unmeasurable);

        // Pass 2: analyze metrics and resolve dependencies per file,
        // sharing the SAME `file_sources`/`source_roots` context (adapter
        // owns the language's module/namespace semantics — US14 L1/L2,
        // US16 T5).
        for (file, source) in &file_sources {
            let parser = match self.registry.dispatch(file) {
                Some(parser) => parser,
                None => {
                    // Defensive: `list_source_files` already filtered by
                    // `extensions`, so this should be unreachable in
                    // practice — never fatal either way (US16 T2 AC: one
                    // undispatchable file must not kill the whole scan).
                    unmeasurable.push(UnmeasurableFile {
                        path: file.clone(),
                        reason: UnmeasurableReason::UnsupportedLanguage,
                    });
                    continue;
                }
            };

            match proactive_analyzer::analyze(source, rules, parser) {
                Ok(metrics) => {
                    let metrics = Self::set_file_paths(metrics, file);
                    per_file.push((file.clone(), metrics));
                }
                Err(e) => {
                    eprintln!(
                        "Avertissement: impossible d'analyser {}: {}",
                        file.file_name().unwrap_or_default().to_string_lossy(),
                        e
                    );
                    let reason = match e {
                        AnalysisError::Unmeasurable(reason) => reason,
                        _ => UnmeasurableReason::SourceUnparseable,
                    };
                    unmeasurable.push(UnmeasurableFile {
                        path: file.clone(),
                        reason,
                    });
                }
            }

            let ctx = DependencyContext::new(file.clone(), project_root.clone(), files.clone())
                .with_file_sources(file_sources.clone())
                .with_source_roots(source_roots.clone());
            match parser.resolve_dependencies(source, &ctx) {
                Ok(resolved) => {
                    for to in resolved {
                        all_deps.push(super::file_consumption_graph::FileDependency {
                            from: file.clone(),
                            to,
                        });
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Avertissement: impossible de parser les dépendances de {}: {}",
                        file.file_name().unwrap_or_default().to_string_lossy(),
                        e
                    );
                }
            }
        }

        let graph =
            FileConsumptionGraph::build(&per_file, all_deps)?.with_unmeasurable_files(unmeasurable);
        let graph = Self::gate_project(graph, config.thresholds());
        let report = graph.threshold_report().cloned().unwrap_or_default();
        self.reporter.write_project_report(&graph)?;
        Ok(GatedOutput::new((), report))
    }

    /// Reads every one of `files`' source text, appending an
    /// `UnmeasurableFile` to `unmeasurable` for each that could not be
    /// read (US16 T5) — pulled out because both project-level handlers
    /// (`handle_project`, `build_project_graph`) need the SAME full
    /// `file_sources` list before their own per-file pass runs.
    fn read_all_sources(
        &self,
        files: &[PathBuf],
        unmeasurable: &mut Vec<UnmeasurableFile>,
    ) -> Vec<(PathBuf, String)> {
        let mut file_sources = Vec::with_capacity(files.len());
        for file in files {
            let file_target = AnalysisTarget::new(file.clone(), TargetType::File);
            match self.code_reader.read_source(&file_target) {
                Ok(source) => file_sources.push((file.clone(), source)),
                Err(e) => {
                    eprintln!(
                        "Avertissement: impossible de lire {}: {}",
                        file.file_name().unwrap_or_default().to_string_lossy(),
                        e
                    );
                    unmeasurable.push(UnmeasurableFile {
                        path: file.clone(),
                        reason: UnmeasurableReason::SourceUnreadable,
                    });
                }
            }
        }
        file_sources
    }

    /// Evaluates the project's aggregate energy (kWh)/CO2 impact against
    /// `thresholds` and attaches the outcome to the graph (US8 AD-1/AD-3).
    /// Both metrics are derived from the SAME `Option<EcologicalImpact>`
    /// aggregate (change request on issue #8: energy replaces CPU cost as
    /// the first gated metric — `energy_joules() / KWH_TO_JOULES` recovers
    /// the kWh value `EcologicalImpactEstimator::estimate` originally
    /// derived it from). Pulled out of `handle_project` because slice 3
    /// (JSON/HTML) reuses the identical gate against
    /// `build_project_graph`'s output.
    fn gate_project(
        graph: FileConsumptionGraph,
        thresholds: &AlertThresholds,
    ) -> FileConsumptionGraph {
        let ecological = graph.aggregated_metrics().total_ecological_impact;
        let energy_kwh = ecological
            .as_ref()
            .map(|e| e.energy_joules() / EcologicalImpactEstimator::KWH_TO_JOULES);
        let co2 = ecological.as_ref().map(|e| e.co2_grams());
        let report = thresholds.evaluate(energy_kwh, co2);
        graph.with_threshold_report(report)
    }

    /// Evaluates a single file's own energy (kWh)/CO2 impact against
    /// `thresholds` (US8 T3) — the single-file twin of `gate_project`,
    /// same shape, same gate (`AlertThresholds::evaluate`), same single
    /// `Option<EcologicalImpact>` source, different data-carrier
    /// (`CodeMetrics` rather than `FileConsumptionGraph`).
    fn gate_metrics(metrics: CodeMetrics, thresholds: &AlertThresholds) -> CodeMetrics {
        let ecological = metrics.ecological_impact();
        let energy_kwh =
            ecological.map(|e| e.energy_joules() / EcologicalImpactEstimator::KWH_TO_JOULES);
        let co2 = ecological.map(|e| e.co2_grams());
        let report = thresholds.evaluate(energy_kwh, co2);
        metrics.with_threshold_report(report)
    }

    fn set_file_paths(metrics: CodeMetrics, path: &Path) -> CodeMetrics {
        let file_path = path.to_string_lossy().to_string();

        let updated_warnings: Vec<super::complexity_detector::ComplexityWarning> = metrics
            .warnings()
            .iter()
            .map(|w| super::complexity_detector::ComplexityWarning {
                location: CodeLocation::new(file_path.clone(), w.location.line(), w.location.col()),
                ..w.clone()
            })
            .collect();

        let updated_details: Vec<super::code_metrics::FunctionDetail> = metrics
            .function_details()
            .iter()
            .map(|d| d.clone().with_location(file_path.clone()))
            .collect();

        let updated_io: Vec<IoInLoopWarning> = metrics
            .io_in_loops()
            .iter()
            .map(|w| IoInLoopWarning {
                location: CodeLocation::new(file_path.clone(), w.location.line(), w.location.col()),
                ..w.clone()
            })
            .collect();

        metrics
            .with_warnings(updated_warnings)
            .with_function_details(updated_details)
            .with_io_in_loops(updated_io)
    }

    pub fn handle_json(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
        config: &AnalysisConfig,
    ) -> Result<GatedOutput<String>, AnalysisError> {
        let parser = self.dispatch_or_unsupported(target.path())?;
        let source = self.code_reader.read_source(target)?;
        let metrics = proactive_analyzer::analyze(&source, rules, parser)?;
        let metrics = Self::set_file_paths(metrics, target.path());
        let metrics = Self::gate_metrics(metrics, config.thresholds());
        let report = metrics.threshold_report().cloned().unwrap_or_default();
        let target_str = target.path().to_string_lossy();
        let target_type = if *target.target_type() == TargetType::Project {
            "project"
        } else {
            "file"
        };
        let json = self
            .reporter
            .write_json(&metrics, &target_str, target_type)?;
        Ok(GatedOutput::new(json, report))
    }

    pub fn handle_project_json(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
        config: &AnalysisConfig,
    ) -> Result<GatedOutput<String>, AnalysisError> {
        let graph = self.build_project_graph_with_source_roots(
            target,
            rules,
            config.file_filter(),
            config.source_roots(),
        )?;
        let graph = Self::gate_project(graph, config.thresholds());
        let report = graph.threshold_report().cloned().unwrap_or_default();
        let target_str = target.path().to_string_lossy();
        let json = self.reporter.write_project_json(&graph, &target_str)?;
        Ok(GatedOutput::new(json, report))
    }

    pub fn handle_project_html(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
        config: &AnalysisConfig,
    ) -> Result<GatedOutput<String>, AnalysisError> {
        let graph = self.build_project_graph_with_source_roots(
            target,
            rules,
            config.file_filter(),
            config.source_roots(),
        )?;
        let graph = Self::gate_project(graph, config.thresholds());
        let report = graph.threshold_report().cloned().unwrap_or_default();
        let target_str = target.path().to_string_lossy();
        let html = self.reporter.write_html(&graph, &target_str)?;
        Ok(GatedOutput::new(html, report))
    }

    /// Walks every file under `target` matching `filter` (US31), analyzes
    /// it, and resolves inter-file dependencies into a
    /// `FileConsumptionGraph`. Analysis or parsing failures on an
    /// individual file are silently skipped (best effort over a whole
    /// project) — shared by `handle_project_json` and `handle_project_html`,
    /// which differ only in what they do with the resulting graph.
    /// `raw_source_roots` is `.codeimpact.json`'s `sourceRoots` (US16 T5,
    /// Q2) — resolved to absolute roots via `resolve_source_roots` and
    /// threaded onto every file's `DependencyContext`.
    fn build_project_graph_with_source_roots(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
        filter: &super::file_filter::FileFilter,
        raw_source_roots: &[String],
    ) -> Result<FileConsumptionGraph, AnalysisError> {
        let extensions = self.registry.extensions();
        let files = self
            .code_reader
            .list_source_files(target.path(), &extensions, filter)?;
        let project_root = target.path().clone();
        let source_roots = resolve_source_roots(&project_root, raw_source_roots);

        let mut per_file: Vec<(PathBuf, CodeMetrics)> = Vec::new();
        let mut all_deps: Vec<super::file_consumption_graph::FileDependency> = Vec::new();
        let mut unmeasurable: Vec<UnmeasurableFile> = Vec::new();

        let file_sources = self.read_all_sources(&files, &mut unmeasurable);

        for (file, source) in &file_sources {
            let parser = match self.registry.dispatch(file) {
                Some(parser) => parser,
                None => {
                    unmeasurable.push(UnmeasurableFile {
                        path: file.clone(),
                        reason: UnmeasurableReason::UnsupportedLanguage,
                    });
                    continue;
                }
            };

            match proactive_analyzer::analyze(source, rules, parser) {
                Ok(metrics) => {
                    let metrics = Self::set_file_paths(metrics, file);
                    per_file.push((file.clone(), metrics));
                }
                Err(e) => {
                    let reason = match e {
                        AnalysisError::Unmeasurable(reason) => reason,
                        _ => UnmeasurableReason::SourceUnparseable,
                    };
                    unmeasurable.push(UnmeasurableFile {
                        path: file.clone(),
                        reason,
                    });
                }
            }

            let ctx = DependencyContext::new(file.clone(), project_root.clone(), files.clone())
                .with_file_sources(file_sources.clone())
                .with_source_roots(source_roots.clone());
            if let Ok(resolved) = parser.resolve_dependencies(source, &ctx) {
                for to in resolved {
                    all_deps.push(super::file_consumption_graph::FileDependency {
                        from: file.clone(),
                        to,
                    });
                }
            }
        }

        if per_file.is_empty() {
            return Err(AnalysisError::AnalysisFailed(
                "no files could be analyzed in the project".into(),
            ));
        }

        Ok(FileConsumptionGraph::build(&per_file, all_deps)?.with_unmeasurable_files(unmeasurable))
    }
}

/// Resolves `.codeimpact.json`'s raw `sourceRoots` (relative strings)
/// against `project_root` into absolute `PathBuf`s (US16 T5, Q2) — an
/// empty/absent config list resolves to an EMPTY `Vec`, not to
/// `vec![project_root]`: an adapter that cares about source roots treats
/// empty as "unrestricted" (see `TreeSitterCodeParser::under_any_root`),
/// which is exactly the "absent -> behaves like before T5" contract this
/// function exists to keep. Materializing `project_root` itself here would
/// risk comparing a raw CLI `--path` (frequently relative, e.g. `.`)
/// against `available_files`' CANONICALIZED absolute paths — the same
/// representation mismatch already documented in
/// `html_report_writer_test.rs`'s `tree_ids_are_relative_when_target_
/// resolves_to_the_files_common_root` — an empty list sidesteps that
/// mismatch entirely instead of reproducing it.
fn resolve_source_roots(project_root: &Path, raw_source_roots: &[String]) -> Vec<PathBuf> {
    raw_source_roots
        .iter()
        .map(|root| project_root.join(root))
        .collect()
}
