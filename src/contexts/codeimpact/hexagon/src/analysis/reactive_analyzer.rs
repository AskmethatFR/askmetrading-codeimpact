use super::economic_impact::{compute_level, EconomicImpact};
use super::stress_test_run::StressTestRun;

pub struct ReactiveAnalyzer;

impl ReactiveAnalyzer {
    /// Cost per CPU-second in microdollars.
    /// Based on ~$0.10/CPU-heure cloud pricing: $0.10 / 3600s = 27.78 μ$/s
    pub const MICRODOLLARS_PER_CPU_SECOND: f64 = 27.7778;

    pub fn analyze(run: &StressTestRun) -> EconomicImpact {
        let cpu_seconds = run.cpu_time_ms() as f64 / 1000.0;
        let cpu_cost = cpu_seconds * Self::MICRODOLLARS_PER_CPU_SECOND;
        let memory_bytes = run.memory_kb() * 1024;
        let total = cpu_cost + memory_bytes as f64 * 0.0001;
        let level = compute_level(total);
        EconomicImpact::new(cpu_cost, memory_bytes, total, level)
    }
}