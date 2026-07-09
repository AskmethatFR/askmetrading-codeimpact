use std::sync::{Arc, Mutex};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::ReportWriter;

#[derive(Clone)]
pub struct SharedReportWriterStub {
    pub last_metrics: Arc<Mutex<Option<CodeMetrics>>>,
    pub last_graph: Arc<Mutex<Option<FileConsumptionGraph>>>,
}

impl SharedReportWriterStub {
    pub fn new() -> Self {
        Self {
            last_metrics: Arc::new(Mutex::new(None)),
            last_graph: Arc::new(Mutex::new(None)),
        }
    }
}

impl ReportWriter for SharedReportWriterStub {
    fn write_console(&self, metrics: &CodeMetrics) -> Result<(), AnalysisError> {
        *self.last_metrics.lock().unwrap() = Some(metrics.clone());
        Ok(())
    }

    fn write_project_report(
        &self,
        graph: &FileConsumptionGraph,
    ) -> Result<(), AnalysisError> {
        *self.last_graph.lock().unwrap() = Some(graph.clone());
        Ok(())
    }
}
