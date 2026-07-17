/// Value Object (US8, AD-1): user-provided (CLI or config file) alert
/// thresholds for a project's aggregate ecological energy (kWh) and CO2 (g)
/// impact. Self-validating — construction rejects a non-finite or negative
/// threshold, so no other code in the system can ever hold an
/// `AlertThresholds` capable of producing a nonsensical breach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlertThresholds {
    max_energy_kwh: Option<f64>,
    max_co2_grams: Option<f64>,
}

/// Rejected construction of an `AlertThresholds` — names the offending
/// metric and the value that failed validation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThresholdError {
    InvalidEnergyThreshold(f64),
    InvalidCo2Threshold(f64),
}

impl std::fmt::Display for ThresholdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEnergyThreshold(v) => {
                write!(
                    f,
                    "seuil d'énergie invalide: {} (doit être un nombre fini >= 0)",
                    v
                )
            }
            Self::InvalidCo2Threshold(v) => {
                write!(
                    f,
                    "seuil CO2 invalide: {} (doit être un nombre fini >= 0)",
                    v
                )
            }
        }
    }
}

impl std::error::Error for ThresholdError {}

impl AlertThresholds {
    /// No thresholds configured. `evaluate` on this instance always returns
    /// an empty report (AC6: no threshold configured -> current behavior
    /// unchanged).
    pub fn none() -> Self {
        Self {
            max_energy_kwh: None,
            max_co2_grams: None,
        }
    }

    /// Rejects a non-finite or negative threshold at construction (ddd-value-object).
    pub fn new(
        max_energy_kwh: Option<f64>,
        max_co2_grams: Option<f64>,
    ) -> Result<Self, ThresholdError> {
        if let Some(v) = max_energy_kwh {
            if !v.is_finite() || v < 0.0 {
                return Err(ThresholdError::InvalidEnergyThreshold(v));
            }
        }
        if let Some(v) = max_co2_grams {
            if !v.is_finite() || v < 0.0 {
                return Err(ThresholdError::InvalidCo2Threshold(v));
            }
        }
        Ok(Self {
            max_energy_kwh,
            max_co2_grams,
        })
    }

    pub fn max_energy_kwh(&self) -> Option<f64> {
        self.max_energy_kwh
    }

    pub fn max_co2_grams(&self) -> Option<f64> {
        self.max_co2_grams
    }

    /// Merges a config-file-read `AlertThresholds` with a CLI-parsed one
    /// (US8 AD-5, T4): the CLI value wins per metric when both are set,
    /// otherwise the file value carries through. Pure domain composition —
    /// no I/O, no validation (both inputs are already-validated
    /// `AlertThresholds`, so the merge cannot produce an invalid result).
    pub fn from_sources(file: AlertThresholds, cli: AlertThresholds) -> Self {
        Self {
            max_energy_kwh: cli.max_energy_kwh.or(file.max_energy_kwh),
            max_co2_grams: cli.max_co2_grams.or(file.max_co2_grams),
        }
    }

    /// The pure domain gate (AD-1): compares aggregate energy/CO2 against
    /// the configured thresholds. An absent metric (`None` — the value
    /// could not be measured) never breaches, however low the threshold:
    /// absence is not a confident zero (ADR-0010).
    pub fn evaluate(&self, energy_kwh: Option<f64>, co2: Option<f64>) -> ThresholdReport {
        let mut breaches = Vec::new();
        if let (Some(limit), Some(actual)) = (self.max_energy_kwh, energy_kwh) {
            if actual > limit {
                breaches.push(ThresholdBreach::new(BreachedMetric::Energy, limit, actual));
            }
        }
        if let (Some(limit), Some(actual)) = (self.max_co2_grams, co2) {
            if actual > limit {
                breaches.push(ThresholdBreach::new(BreachedMetric::Co2, limit, actual));
            }
        }
        ThresholdReport::new(breaches)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreachedMetric {
    Energy,
    Co2,
}

impl BreachedMetric {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Energy => "ÉNERGIE",
            Self::Co2 => "CO2",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThresholdBreach {
    metric: BreachedMetric,
    limit: f64,
    actual: f64,
}

impl ThresholdBreach {
    fn new(metric: BreachedMetric, limit: f64, actual: f64) -> Self {
        Self {
            metric,
            limit,
            actual,
        }
    }

    pub fn metric(&self) -> BreachedMetric {
        self.metric
    }

    pub fn limit(&self) -> f64 {
        self.limit
    }

    pub fn actual(&self) -> f64 {
        self.actual
    }

    /// By how much the actual value exceeds the limit — always > 0 by
    /// construction (only `AlertThresholds::evaluate` builds one, and only
    /// when `actual > limit`).
    pub fn excess(&self) -> f64 {
        self.actual - self.limit
    }
}

/// The outcome of evaluating a project's measured impact against its
/// configured thresholds — zero or more breaches.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ThresholdReport {
    breaches: Vec<ThresholdBreach>,
}

impl ThresholdReport {
    fn new(breaches: Vec<ThresholdBreach>) -> Self {
        Self { breaches }
    }

    pub fn has_breach(&self) -> bool {
        !self.breaches.is_empty()
    }

    pub fn breaches(&self) -> &[ThresholdBreach] {
        &self.breaches
    }
}
