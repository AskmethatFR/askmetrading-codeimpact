use super::analysis_rule::AnalysisRule;
use super::analysis_target::AnalysisTarget;
use super::code_reader::CodeReader;
use super::errors::AnalysisError;
use super::proactive_analyzer;
use super::report_writer::ReportWriter;

pub struct RunAnalysis {
    code_reader: Box<dyn CodeReader>,
    reporter: Box<dyn ReportWriter>,
}

impl RunAnalysis {
    pub fn new(code_reader: Box<dyn CodeReader>, reporter: Box<dyn ReportWriter>) -> Self {
        Self {
            code_reader,
            reporter,
        }
    }

    pub fn handle(
        &self,
        target: &AnalysisTarget,
        rules: &[AnalysisRule],
    ) -> Result<(), AnalysisError> {
        let source = self.code_reader.read_source(target)?;
        let metrics = proactive_analyzer::analyze(&source, rules)?;
        self.reporter.write_console(&metrics)?;
        Ok(())
    }
}
