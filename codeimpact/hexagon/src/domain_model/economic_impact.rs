use crate::domain_model::CodeMetrics;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct MicroDollars(f64);

impl MicroDollars {
    pub fn new(value: f64) -> Result<Self, AnalysisError> {
        if value < 0.0 {
            return Err(AnalysisError::invalid_economic("cost must be >= 0"));
        }
        Ok(Self(value))
    }
    pub fn value(&self) -> f64 { self.0 }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ByteSize(u64);

impl ByteSize {
    pub fn new(bytes: u64) -> Self { Self(bytes) }
    pub fn bytes(&self) -> u64 { self.0 }
    pub fn megabytes(&self) -> f64 { self.0 as f64 / 1_048_576.0 }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EconomicImpact {
    cpu_cost: MicroDollars,
    memory_bytes: ByteSize,
    network_bytes: ByteSize,
    total_cost: MicroDollars,
}

impl EconomicImpact {
    pub fn new(
        cpu_cost: MicroDollars,
        memory_bytes: ByteSize,
        network_bytes: ByteSize,
    ) -> Self {
        let total = MicroDollars::new(
            cpu_cost.value() + (memory_bytes.bytes() as f64 * 0.0001) + (network_bytes.bytes() as f64 * 0.0005)
        ).unwrap_or(MicroDollars::new(0.0).unwrap());
        Self { cpu_cost, memory_bytes, network_bytes, total_cost: total }
    }

    pub fn cpu_cost(&self) -> &MicroDollars { &self.cpu_cost }
    pub fn memory_bytes(&self) -> &ByteSize { &self.memory_bytes }
    pub fn network_bytes(&self) -> &ByteSize { &self.network_bytes }
    pub fn total_cost(&self) -> &MicroDollars { &self.total_cost }

    pub fn from_metrics(metrics: &CodeMetrics) -> Self {
        let cpu = MicroDollars::new(metrics.cyclomatic_complexity() as f64 * 0.5).unwrap();
        let mem = ByteSize::new(metrics.allocation_hotspots().len() as u64 * 4096);
        let net = ByteSize::new(metrics.io_in_loops().len() as u64 * 8192);
        Self::new(cpu, mem, net)
    }
}

use crate::domain_model::errors::AnalysisError;