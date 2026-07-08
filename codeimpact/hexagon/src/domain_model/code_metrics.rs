/// Value Object — résultats de l'analyse de code.
/// Contient la complexité cyclomatique et le niveau associé.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeMetrics {
    cyclomatic_complexity: u32,
}

impl CodeMetrics {
    pub fn new(cyclomatic_complexity: u32) -> Self {
        Self {
            cyclomatic_complexity,
        }
    }

    pub fn cyclomatic_complexity(&self) -> u32 {
        self.cyclomatic_complexity
    }

    /// Retourne le niveau de complexité:
    /// - 0-10  → "low"
    /// - 11-20 → "moderate"
    /// - 21-40 → "high"
    /// - 41+   → "critical"
    pub fn complexity_level(&self) -> &'static str {
        match self.cyclomatic_complexity {
            0..=10 => "low",
            11..=20 => "moderate",
            21..=40 => "high",
            _ => "critical",
        }
    }
}
