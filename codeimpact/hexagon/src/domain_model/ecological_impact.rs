use crate::domain_model::{EconomicImpact, EfficiencyClass, MicroDollars};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Co2Grams(f64);

impl Co2Grams {
    pub fn new(value: f64) -> Result<Self, AnalysisError> {
        if value < 0.0 {
            return Err(AnalysisError::invalid_ecological("CO2 must be >= 0"));
        }
        Ok(Self(value))
    }
    pub fn value(&self) -> f64 { self.0 }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyJoules(f64);

impl EnergyJoules {
    pub fn new(value: f64) -> Result<Self, AnalysisError> {
        if value < 0.0 {
            return Err(AnalysisError::invalid_ecological("energy must be >= 0"));
        }
        Ok(Self(value))
    }
    pub fn value(&self) -> f64 { self.0 }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EcologicalImpact {
    co2_grams: Co2Grams,
    energy_joules: EnergyJoules,
    energy_class: EfficiencyClass,
}

impl EcologicalImpact {
    /// CO2 factor: grams per kWh. Default ~400 (EU avg).
    /// See https://www.electricitymaps.com/
    pub fn from_economic(economic: &EconomicImpact, co2_g_per_kwh: f64) -> Self {
        let cpu_usd = economic.cpu_cost().value();
        let kwh = cpu_usd * 0.01;
        let co2 = Co2Grams::new(kwh * co2_g_per_kwh).unwrap_or(Co2Grams::new(0.0).unwrap());
        let joules = EnergyJoules::new(kwh * 3_600_000.0).unwrap_or(EnergyJoules::new(0.0).unwrap());
        let klass = EfficiencyClass::from_co2(co2.value());
        Self { co2_grams: co2, energy_joules: joules, energy_class: klass }
    }

    pub fn co2_grams(&self) -> &Co2Grams { &self.co2_grams }
    pub fn energy_joules(&self) -> &EnergyJoules { &self.energy_joules }
    pub fn energy_class(&self) -> &EfficiencyClass { &self.energy_class }
}

use crate::domain_model::errors::AnalysisError;