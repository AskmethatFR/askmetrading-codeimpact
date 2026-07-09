use std::path::PathBuf;

use super::analysis_rule::AnalysisRule;
use super::analysis_target::{AnalysisTarget, TargetType};
use super::code_metrics::CodeMetrics;
use super::code_parser::CodeParser;
use super::code_reader::CodeReader;
use super::errors::AnalysisError;
use super::file_consumption_graph::{resolve_file_dependency, FileConsumptionGraph};
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

    pub fn handle(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
    ) -> Result<(), AnalysisError> {
        if *target.target_type() == TargetType::Project {
            return self.handle_project(target, rules);
        }
        let source = self.code_reader.read_source(target)?;
        let metrics = proactive_analyzer::analyze(&source, rules, self.parser.as_ref())?;
        self.reporter.write_console(&metrics)?;
        Ok(())
    }

    fn handle_project(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
    ) -> Result<(), AnalysisError> {
        let files = self.code_reader.list_rust_files(target.path())?;
        let mut per_file: Vec<(PathBuf, CodeMetrics)> = Vec::new();
        let mut all_deps: Vec<super::file_consumption_graph::FileDependency> = Vec::new();
        let crate_root = target.path().clone();

        for file in &files {
            let file_target = AnalysisTarget::new(file.clone(), TargetType::File);
            match self.code_reader.read_source(&file_target) {
                Ok(source) => {
                    match proactive_analyzer::analyze(
                        &source,
                        rules,
                        self.parser.as_ref(),
                    ) {
                        Ok(metrics) => {
                            per_file.push((file.clone(), metrics));
                        }
                        Err(e) => {
                            eprintln!(
                                "Avertissement: impossible d'analyser {}: {}",
                                file.file_name().unwrap_or_default().to_string_lossy(),
                                e
                            );
                        }
                    }

                    // Parse file dependencies
                    match self.parser.parse_file_dependencies(&source) {
                        Ok(raw_deps) => {
                            for raw in &raw_deps {
                                if let Some(to) = resolve_file_dependency(
                                    raw,
                                    file,
                                    &crate_root,
                                    &files,
                                ) {
                                    all_deps.push(
                                        super::file_consumption_graph::FileDependency {
                                            from: file.clone(),
                                            to,
                                        },
                                    );
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
                }
            }
        }

        let graph = FileConsumptionGraph::build(&per_file, all_deps)?;
        self.reporter.write_project_report(&graph)
    }
}
