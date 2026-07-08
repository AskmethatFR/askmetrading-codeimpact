use crate::domain_model::analysis_rule::AnalysisRule;
use crate::domain_model::analysis_target::AnalysisTarget;
use crate::domain_model::errors::AnalysisError;
use crate::domain_model::proactive_analyzer;
use crate::gateways_secondary_ports::code_reader_port::CodeReaderPort;
use crate::gateways_secondary_ports::report_writer_port::ReportWriterPort;

/// Cas d'utilisation — exécute une analyse de code.
pub struct RunAnalysis {
    code_reader: Box<dyn CodeReaderPort>,
    reporter: Box<dyn ReportWriterPort>,
}

impl RunAnalysis {
    pub fn new(code_reader: Box<dyn CodeReaderPort>, reporter: Box<dyn ReportWriterPort>) -> Self {
        Self {
            code_reader,
            reporter,
        }
    }

    pub fn execute(
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
