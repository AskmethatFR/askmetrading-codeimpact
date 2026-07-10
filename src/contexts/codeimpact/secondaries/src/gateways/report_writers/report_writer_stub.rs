use std::sync::{Arc, Mutex};

use codeimpact_hexagon::analysis::AnalysisError;
use codeimpact_hexagon::analysis::CodeMetrics;
use codeimpact_hexagon::analysis::EconomicImpact;
use codeimpact_hexagon::analysis::FileConsumptionGraph;
use codeimpact_hexagon::analysis::ReportWriter;
use codeimpact_hexagon::analysis::StressTestRun;

#[derive(Clone)]
pub struct SharedReportWriterStub {
    pub last_metrics: Arc<Mutex<Option<CodeMetrics>>>,
    pub last_graph: Arc<Mutex<Option<FileConsumptionGraph>>>,
    pub last_stress_run: Arc<Mutex<Option<StressTestRun>>>,
    pub last_stress_impact: Arc<Mutex<Option<EconomicImpact>>>,
}

impl SharedReportWriterStub {
    pub fn new() -> Self {
        Self {
            last_metrics: Arc::new(Mutex::new(None)),
            last_graph: Arc::new(Mutex::new(None)),
            last_stress_run: Arc::new(Mutex::new(None)),
            last_stress_impact: Arc::new(Mutex::new(None)),
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

    fn write_stress_test(
        &self,
        run: &StressTestRun,
        impact: &EconomicImpact,
    ) -> Result<(), AnalysisError> {
        *self.last_stress_run.lock().unwrap() = Some(run.clone());
        *self.last_stress_impact.lock().unwrap() = Some(impact.clone());
        Ok(())
    }
}
