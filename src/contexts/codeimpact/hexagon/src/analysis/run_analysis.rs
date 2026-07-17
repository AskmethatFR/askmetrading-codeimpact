use std::path::{Path, PathBuf};

use super::alert_thresholds::AlertThresholds;
use super::analysis_rule::AnalysisRule;
use super::analysis_target::{AnalysisTarget, TargetType};
use super::code_location::CodeLocation;
use super::code_metrics::CodeMetrics;
use super::code_parser::CodeParser;
use super::code_reader::CodeReader;
use super::errors::AnalysisError;
use super::file_consumption_graph::{
    resolve_file_dependency, FileConsumptionGraph, UnmeasurableFile,
};
use super::gated_output::GatedOutput;
use super::io_in_loop_warning::IoInLoopWarning;
use super::measurement::UnmeasurableReason;
use super::proactive_analyzer;
use super::report_writer::ReportWriter;

pub struct RunAnalysis {
    code_reader: Box<dyn CodeReader>,
    reporter: Box<dyn ReportWriter>,
    parser: Box<dyn CodeParser>,
}

impl RunAnalysis {
    pub fn new(
        code_reader: Box<dyn CodeReader>,
        reporter: Box<dyn ReportWriter>,
        parser: Box<dyn CodeParser>,
    ) -> Self {
        Self {
            code_reader,
            reporter,
            parser,
        }
    }

    /// `thresholds` (US8): evaluated against the project's aggregate impact
    /// on the project path, and against the file's own impact on a
    /// single-file target (T3 extended the gate to `CodeMetrics`).
    /// `AlertThresholds::none()` reproduces the pre-US8 behavior exactly
    /// (AC6): `evaluate` against no configured threshold never breaches.
    pub fn handle(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
        thresholds: &AlertThresholds,
    ) -> Result<GatedOutput<()>, AnalysisError> {
        if *target.target_type() == TargetType::Project {
            return self.handle_project(target, rules, thresholds);
        }
        let source = self.code_reader.read_source(target)?;
        let metrics = proactive_analyzer::analyze(&source, rules, self.parser.as_ref())?;
        let metrics = Self::set_file_paths(metrics, target.path());
        let metrics = Self::gate_metrics(metrics, thresholds);
        let report = metrics.threshold_report().cloned().unwrap_or_default();
        self.reporter.write_console(&metrics)?;
        Ok(GatedOutput::new((), report))
    }

    fn handle_project(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
        thresholds: &AlertThresholds,
    ) -> Result<GatedOutput<()>, AnalysisError> {
        let files = self.code_reader.list_rust_files(target.path())?;
        let mut per_file: Vec<(PathBuf, CodeMetrics)> = Vec::new();
        let mut all_deps: Vec<super::file_consumption_graph::FileDependency> = Vec::new();
        let mut unmeasurable: Vec<UnmeasurableFile> = Vec::new();
        let crate_root = target.path().clone();

        for file in &files {
            let file_target = AnalysisTarget::new(file.clone(), TargetType::File);
            match self.code_reader.read_source(&file_target) {
                Ok(source) => {
                    match proactive_analyzer::analyze(&source, rules, self.parser.as_ref()) {
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

                    // Parse file dependencies
                    match self.parser.parse_file_dependencies(&source) {
                        Ok(raw_deps) => {
                            for raw in &raw_deps {
                                if let Some(to) =
                                    resolve_file_dependency(raw, file, &crate_root, &files)
                                {
                                    all_deps.push(super::file_consumption_graph::FileDependency {
                                        from: file.clone(),
                                        to,
                                    });
                                }
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

        let graph =
            FileConsumptionGraph::build(&per_file, all_deps)?.with_unmeasurable_files(unmeasurable);
        let graph = Self::gate_project(graph, thresholds);
        let report = graph.threshold_report().cloned().unwrap_or_default();
        self.reporter.write_project_report(&graph)?;
        Ok(GatedOutput::new((), report))
    }

    /// Evaluates the project's aggregate CPU/CO2 impact against `thresholds`
    /// and attaches the outcome to the graph (US8 AD-1/AD-3). Pulled out of
    /// `handle_project` because slice 3 (JSON/HTML) reuses the identical
    /// gate against `build_project_graph`'s output.
    fn gate_project(
        graph: FileConsumptionGraph,
        thresholds: &AlertThresholds,
    ) -> FileConsumptionGraph {
        let aggregated = graph.aggregated_metrics();
        let cpu = aggregated
            .total_economic_impact
            .as_ref()
            .map(|e| e.cpu_cost_microdollars());
        let co2 = aggregated
            .total_ecological_impact
            .as_ref()
            .map(|e| e.co2_grams());
        let report = thresholds.evaluate(cpu, co2);
        graph.with_threshold_report(report)
    }

    /// Evaluates a single file's own economic/ecological impact against
    /// `thresholds` (US8 T3) — the single-file twin of `gate_project`,
    /// same shape, same gate (`AlertThresholds::evaluate`), different
    /// data-carrier (`CodeMetrics` rather than `FileConsumptionGraph`).
    fn gate_metrics(metrics: CodeMetrics, thresholds: &AlertThresholds) -> CodeMetrics {
        let cpu = metrics.economic_impact().map(|e| e.cpu_cost_microdollars());
        let co2 = metrics.ecological_impact().map(|e| e.co2_grams());
        let report = thresholds.evaluate(cpu, co2);
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
        thresholds: &AlertThresholds,
    ) -> Result<GatedOutput<String>, AnalysisError> {
        let source = self.code_reader.read_source(target)?;
        let metrics = proactive_analyzer::analyze(&source, rules, self.parser.as_ref())?;
        let metrics = Self::set_file_paths(metrics, target.path());
        let metrics = Self::gate_metrics(metrics, thresholds);
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
        thresholds: &AlertThresholds,
    ) -> Result<GatedOutput<String>, AnalysisError> {
        let graph = self.build_project_graph(target, rules)?;
        let graph = Self::gate_project(graph, thresholds);
        let report = graph.threshold_report().cloned().unwrap_or_default();
        let target_str = target.path().to_string_lossy();
        let json = self.reporter.write_project_json(&graph, &target_str)?;
        Ok(GatedOutput::new(json, report))
    }

    pub fn handle_project_html(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
        thresholds: &AlertThresholds,
    ) -> Result<GatedOutput<String>, AnalysisError> {
        let graph = self.build_project_graph(target, rules)?;
        let graph = Self::gate_project(graph, thresholds);
        let report = graph.threshold_report().cloned().unwrap_or_default();
        let target_str = target.path().to_string_lossy();
        let html = self.reporter.write_html(&graph, &target_str)?;
        Ok(GatedOutput::new(html, report))
    }

    /// Walks every Rust file under `target`, analyzes it, and resolves
    /// inter-file dependencies into a `FileConsumptionGraph`. Analysis or
    /// parsing failures on an individual file are silently skipped (best
    /// effort over a whole project) — shared by handle_project_json and
    /// handle_project_html, which differ only in what they do with the
    /// resulting graph.
    fn build_project_graph(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
    ) -> Result<FileConsumptionGraph, AnalysisError> {
        let files = self.code_reader.list_rust_files(target.path())?;
        let mut per_file: Vec<(PathBuf, CodeMetrics)> = Vec::new();
        let mut all_deps: Vec<super::file_consumption_graph::FileDependency> = Vec::new();
        let mut unmeasurable: Vec<UnmeasurableFile> = Vec::new();
        let crate_root = target.path().clone();

        for file in &files {
            let file_target = AnalysisTarget::new(file.clone(), TargetType::File);
            match self.code_reader.read_source(&file_target) {
                Ok(source) => {
                    match proactive_analyzer::analyze(&source, rules, self.parser.as_ref()) {
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
                    if let Ok(raw_deps) = self.parser.parse_file_dependencies(&source) {
                        for raw in &raw_deps {
                            if let Some(to) = super::file_consumption_graph::resolve_file_dependency(
                                raw,
                                file,
                                &crate_root,
                                &files,
                            ) {
                                all_deps.push(super::file_consumption_graph::FileDependency {
                                    from: file.clone(),
                                    to,
                                });
                            }
                        }
                    }
                }
                Err(_) => {
                    unmeasurable.push(UnmeasurableFile {
                        path: file.clone(),
                        reason: UnmeasurableReason::SourceUnreadable,
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
