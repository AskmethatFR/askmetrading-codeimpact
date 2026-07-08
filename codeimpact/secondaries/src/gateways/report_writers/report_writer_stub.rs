use std::cell::RefCell;
use std::rc::Rc;

use codeimpact_hexagon::domain_model::code_metrics::CodeMetrics;
use codeimpact_hexagon::domain_model::errors::AnalysisError;
use codeimpact_hexagon::gateways_secondary_ports::report_writer_port::ReportWriterPort;

/// Stub — écriture du rapport en mémoire pour les tests.
#[derive(Default)]
pub struct ReportWriterStub {
    last_metrics: RefCell<Option<CodeMetrics>>,
}

impl ReportWriterStub {
    pub fn new() -> Self {
        Self {
            last_metrics: RefCell::new(None),
        }
    }

    pub fn last_metrics(&self) -> Option<CodeMetrics> {
        self.last_metrics.borrow().clone()
    }
}

impl ReportWriterPort for ReportWriterStub {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        self.last_metrics
            .replace(Some(CodeMetrics::new(metrics.cyclomatic_complexity())));
        Ok(())
    }
}

/// Wrapper partagé pour utiliser le stub avec le use case.
pub struct SharedReportWriterStub(pub Rc<RefCell<ReportWriterStub>>);

impl ReportWriterPort for SharedReportWriterStub {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        self.0.borrow().write_console(metrics)
    }
}

impl Clone for SharedReportWriterStub {
    fn clone(&self) -> Self {
        SharedReportWriterStub(self.0.clone())
    }
}
