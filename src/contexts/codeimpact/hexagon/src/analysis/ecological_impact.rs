use super::economic_impact::EconomicImpact;

/// Energy efficiency class A–G based on CO2 emissions.
///
/// Thresholds (grams CO2):
/// A: < 1, B: < 5, C: < 10, D: < 25, E: < 50, F: < 100, G: >= 100
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EfficiencyClass {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
}

impl EfficiencyClass {
    /// Determines efficiency class from CO2 grams.
    pub fn from_co2(co2_grams: f64) -> Self {
        if co2_grams < 1.0 {
            Self::A
        } else if co2_grams < 5.0 {
            Self::B
        } else if co2_grams < 10.0 {
            Self::C
        } else if co2_grams < 25.0 {
            Self::D
        } else if co2_grams < 50.0 {
            Self::E
        } else if co2_grams < 100.0 {
            Self::F
        } else {
            Self::G
        }
    }

    /// Returns the label (single letter) for this class.
    pub fn label(&self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
            Self::G => "G",
        }
    }
}

/// Ecological impact of a code file: CO2, energy, and efficiency class.
///
/// Derived from economic impact using a CO2/kWh factor (default: EU average 400 gCO2/kWh).
#[derive(Clone, Debug, PartialEq)]
pub struct EcologicalImpact {
    co2_grams: f64,
    energy_joules: f64,
    efficiency_class: EfficiencyClass,
}

impl EcologicalImpact {
    pub fn new(co2_grams: f64, energy_joules: f64, efficiency_class: EfficiencyClass) -> Self {
        Self {
            co2_grams,
            energy_joules,
            efficiency_class,
        }
    }

    pub fn co2_grams(&self) -> f64 {
        self.co2_grams
    }

    pub fn energy_joules(&self) -> f64 {
        self.energy_joules
    }

    pub fn efficiency_class(&self) -> &EfficiencyClass {
        &self.efficiency_class
    }
}

/// Domain service that estimates ecological impact from economic impact.
///
/// Heuristics:
/// - kWh ≈ cpu_cost_microdollars × MICRODOLLARS_TO_KWH (1 μ$ ≈ 0.000001 kWh)
/// - CO2 = kWh × co2_g_per_kwh
/// - Energy = kWh × KWH_TO_JOULES (1 kWh = 3.6 MJ)
/// - Efficiency class from CO2 grams
pub struct EcologicalImpactEstimator;

impl EcologicalImpactEstimator {
    /// 1 μ$ ≈ 0.000001 kWh (rough data-center CPU cost heuristic).
    pub const MICRODOLLARS_TO_KWH: f64 = 0.000001;
    /// 1 kWh = 3.6 MJ = 3 600 000 J.
    pub const KWH_TO_JOULES: f64 = 3_600_000.0;
    /// Default CO2 factor: 400 gCO2/kWh (EU average).
    pub const DEFAULT_CO2_G_PER_KWH: f64 = 400.0;

    pub fn estimate(economic: &EconomicImpact, co2_g_per_kwh: f64) -> EcologicalImpact {
        let kwh = economic.cpu_cost_microdollars() * Self::MICRODOLLARS_TO_KWH;
        let co2_grams = kwh * co2_g_per_kwh;
        let energy_joules = kwh * Self::KWH_TO_JOULES;
        let efficiency_class = EfficiencyClass::from_co2(co2_grams);

        EcologicalImpact::new(co2_grams, energy_joules, efficiency_class)
    }
}
