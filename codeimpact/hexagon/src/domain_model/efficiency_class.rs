#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    pub fn from_co2(co2_grams: f64) -> Self {
        if co2_grams < 1.0 { Self::A }
        else if co2_grams < 5.0 { Self::B }
        else if co2_grams < 10.0 { Self::C }
        else if co2_grams < 25.0 { Self::D }
        else if co2_grams < 50.0 { Self::E }
        else if co2_grams < 100.0 { Self::F }
        else { Self::G }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::A => "A (very low impact)",
            Self::B => "B (low impact)",
            Self::C => "C (moderate impact)",
            Self::D => "D (significant impact)",
            Self::E => "E (high impact)",
            Self::F => "F (very high impact)",
            Self::G => "G (extreme impact)",
        }
    }
}