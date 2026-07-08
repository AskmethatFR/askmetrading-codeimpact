use std::sync::{Arc, Mutex};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::ReportWriter;

#[derive(Clone)]
pub struct SharedReportWriterStub {
    pub last_metrics: Arc<Mutex<Option<CodeMetrics>>>,
}

impl SharedReportWriterStub {
    pub fn new() -> Self {
        Self {
            last_metrics: Arc::new(Mutex::new(None)),
        }
    }
}

impl ReportWriter for SharedReportWriterStub {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        *self.last_metrics.lock().unwrap() = Some(metrics.clone());
        Ok(())
    }
}
