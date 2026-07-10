use super::economic_impact::EconomicImpact;
use super::stress_test_run::StressTestRun;

const KB_TO_BYTES: u64 = 1024;
const MEMORY_TO_TOTAL_RATIO: f64 = 0.0001;
const STRESS_LEVEL_LOW_MAX: f64 = 1_000.0;
const STRESS_LEVEL_MODERATE_MAX: f64 = 10_000.0;
const STRESS_LEVEL_HIGH_MAX: f64 = 100_000.0;
const STRESS_LEVEL_LOW: &str = "low";
const STRESS_LEVEL_MODERATE: &str = "moderate";
const STRESS_LEVEL_HIGH: &str = "high";
const STRESS_LEVEL_CRITICAL: &str = "critical";

pub struct ReactiveAnalyzer;

impl ReactiveAnalyzer {
    /// Cost per CPU-second in microdollars.
    /// Based on ~$0.10/CPU-heure cloud pricing: $0.10 / 3600s = 27.78 μ$/s
    pub const MICRODOLLARS_PER_CPU_SECOND: f64 = 27.7778;

    pub fn analyze(run: &StressTestRun) -> EconomicImpact {
        let cpu_seconds = run.cpu_time_ms() as f64 / 1000.0;
        let cpu_cost = cpu_seconds * Self::MICRODOLLARS_PER_CPU_SECOND;
        let memory_bytes = run.memory_kb() * KB_TO_BYTES;
        let total = cpu_cost + memory_bytes as f64 * MEMORY_TO_TOTAL_RATIO;
        let level = Self::compute_stress_level(total);
        EconomicImpact::new(cpu_cost, memory_bytes, total, level)
    }

    fn compute_stress_level(total_cost_microdollars: f64) -> &'static str {
        if total_cost_microdollars <= STRESS_LEVEL_LOW_MAX {
            STRESS_LEVEL_LOW
        } else if total_cost_microdollars <= STRESS_LEVEL_MODERATE_MAX {
            STRESS_LEVEL_MODERATE
        } else if total_cost_microdollars <= STRESS_LEVEL_HIGH_MAX {
            STRESS_LEVEL_HIGH
        } else {
            STRESS_LEVEL_CRITICAL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::stress_test_run::StressTestRun;

    #[test]
    fn stress_level_low() {
        let run = StressTestRun::new(10, 0, 0, 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(impact.level(), "low");
    }

    #[test]
    fn stress_level_moderate() {
        // 50 MB = 51200 KB → 52428800 bytes → 52428800 * 0.0001 = 5242 μ$
        // total ≈ 5242 μ$ → moderate
        let run = StressTestRun::new(1000, 1000, 51_200, 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(impact.level(), "moderate");
    }

    #[test]
    fn stress_level_high() {
        // 500 MB = 512000 KB → 524288000 bytes → 52428 μ$
        let run = StressTestRun::new(1000, 1000, 512_000, 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(impact.level(), "high");
    }

    #[test]
    fn stress_level_critical() {
        // 1 GB = 1048576 KB → 1073741824 bytes → 107374 μ$
        let run = StressTestRun::new(1000, 1000, 1_048_576, 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(impact.level(), "critical");
    }
}