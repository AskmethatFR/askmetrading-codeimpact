use super::economic_impact::EconomicImpact;
use super::stress_test_run::{Measurement, StressTestRun};

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

    /// Derives an `EconomicImpact` from a stress-test run — unless either
    /// physical input could not be sampled, in which case the whole estimate
    /// is `Unmeasurable`: there is no honest cost to report from a missing
    /// reading (#36).
    pub fn analyze(run: &StressTestRun) -> Measurement<EconomicImpact> {
        let cpu_time_ms = match run.cpu_time_ms() {
            Measurement::Available(ms) => ms,
            Measurement::Unmeasurable(reason) => return Measurement::Unmeasurable(reason),
        };
        let memory_kb = match run.memory_kb() {
            Measurement::Available(kb) => kb,
            Measurement::Unmeasurable(reason) => return Measurement::Unmeasurable(reason),
        };

        let cpu_seconds = cpu_time_ms as f64 / 1000.0;
        let cpu_cost = cpu_seconds * Self::MICRODOLLARS_PER_CPU_SECOND;
        let memory_bytes = memory_kb * KB_TO_BYTES;
        let total = cpu_cost + memory_bytes as f64 * MEMORY_TO_TOTAL_RATIO;
        let level = Self::compute_stress_level(total);
        Measurement::Available(EconomicImpact::new(cpu_cost, memory_bytes, total, level))
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
    use crate::analysis::stress_test_run::UnmeasurableReason;

    fn available(value: u64) -> Measurement<u64> {
        Measurement::Available(value)
    }

    #[test]
    fn stress_level_low() {
        let run = StressTestRun::new(10, available(0), available(0), 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
        assert_eq!(impact.level(), "low");
    }

    #[test]
    fn stress_level_moderate() {
        // 50 MB = 51200 KB → 52428800 bytes → 52428800 * 0.0001 = 5242 μ$
        // total ≈ 5242 μ$ → moderate
        let run = StressTestRun::new(1000, available(1000), available(51_200), 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
        assert_eq!(impact.level(), "moderate");
    }

    #[test]
    fn stress_level_high() {
        // 500 MB = 512000 KB → 524288000 bytes → 52428 μ$
        let run = StressTestRun::new(1000, available(1000), available(512_000), 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
        assert_eq!(impact.level(), "high");
    }

    #[test]
    fn stress_level_critical() {
        // 1 GB = 1048576 KB → 1073741824 bytes → 107374 μ$
        let run = StressTestRun::new(1000, available(1000), available(1_048_576), 1, 1, None);
        let impact = ReactiveAnalyzer::analyze(&run).available().unwrap();
        assert_eq!(impact.level(), "critical");
    }

    // #36 — the central acceptance criterion: an unmeasurable physical input
    // must never be silently turned into a `0`-cost (= "free") EconomicImpact.
    #[test]
    fn unmeasurable_cpu_time_makes_the_whole_impact_unmeasurable() {
        let run = StressTestRun::new(
            10,
            Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
            available(0),
            1,
            1,
            None,
        );
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(
            impact,
            Measurement::Unmeasurable(UnmeasurableReason::NoSampler)
        );
    }

    #[test]
    fn unmeasurable_memory_makes_the_whole_impact_unmeasurable() {
        let run = StressTestRun::new(
            10,
            available(0),
            Measurement::Unmeasurable(UnmeasurableReason::NoSampler),
            1,
            1,
            None,
        );
        let impact = ReactiveAnalyzer::analyze(&run);
        assert_eq!(
            impact,
            Measurement::Unmeasurable(UnmeasurableReason::NoSampler)
        );
    }
}
