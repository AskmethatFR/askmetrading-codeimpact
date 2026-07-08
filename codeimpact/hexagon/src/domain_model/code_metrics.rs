use crate::domain_model::CodeLocation;

#[derive(Debug, Clone, PartialEq)]
pub struct CodeMetrics {
    cyclomatic_complexity: u32,
    io_in_loops: Vec<CodeLocation>,
    nested_depth: u32,
    allocation_hotspots: Vec<CodeLocation>,
}

impl CodeMetrics {
    pub fn new(
        cyclomatic_complexity: u32,
        io_in_loops: Vec<CodeLocation>,
        nested_depth: u32,
        allocation_hotspots: Vec<CodeLocation>,
    ) -> Self {
        Self { cyclomatic_complexity, io_in_loops, nested_depth, allocation_hotspots }
    }

    pub fn cyclomatic_complexity(&self) -> u32 { self.cyclomatic_complexity }
    pub fn io_in_loops(&self) -> &[CodeLocation] { &self.io_in_loops }
    pub fn nested_depth(&self) -> u32 { self.nested_depth }
    pub fn allocation_hotspots(&self) -> &[CodeLocation] { &self.allocation_hotspots }

    pub fn complexity_level(&self) -> &'static str {
        match self.cyclomatic_complexity {
            0..=10 => "low",
            11..=20 => "moderate",
            21..=40 => "high",
            _ => "critical",
        }
    }
}