use codeimpact_hexagon::analysis::EconomicImpact;

// Test List:
// EfficiencyClass:
// 1. class_a_co2_below_1 → co2 < 1.0 → A
// 2. class_b_co2_1_to_5 → 1.0 <= co2 < 5.0 → B
// 3. class_c_co2_5_to_10 → 5.0 <= co2 < 10.0 → C
// 4. class_d_co2_10_to_25 → 10.0 <= co2 < 25.0 → D
// 5. class_e_co2_25_to_50 → 25.0 <= co2 < 50.0 → E
// 6. class_f_co2_50_to_100 → 50.0 <= co2 < 100.0 → F
// 7. class_g_co2_100_or_more → co2 >= 100.0 → G
// 8. class_boundary_zero → co2 = 0.0 → A
// 9. class_boundary_exact_1 → co2 = 1.0 → B
// 10. class_boundary_exact_100 → co2 = 100.0 → G
// 11. class_label_a → label for A is "A"
// 12. class_label_b → label for B is "B"
// 13. class_label_c → label for C is "C"
// 14. class_label_d → label for D is "D"
// 15. class_label_e → label for E is "E"
// 16. class_label_f → label for F is "F"
// 17. class_label_g → label for G is "G"
//
// EcologicalImpact:
// 18. vo_constructor_stores_values → new() stores co2, energy, class
// 19. vo_getters_return_values → getters return stored values
// 20. vo_clone_debug_partial_eq → supports Clone, Debug, PartialEq
//
// EcologicalImpactEstimator:
// 21. estimate_zero_cpu → zero cpu cost → zero co2, zero energy, class A
// 22. estimate_moderate_cpu → moderate cpu → positive co2, positive energy
// 23. estimate_custom_factor → custom co2 factor used
// 24. estimate_default_factor → DEFAULT_CO2_G_PER_KWH used
// 25. estimate_energy_joules_formula → energy = kwh * 3_600_000
// 26. estimate_co2_grams_formula → co2 = kwh * factor
// 27. estimate_kwh_formula → kwh = cpu_cost_microdollars * 0.000001
// 28. estimate_efficiency_class_b → co2 = 2.4g → class B
// 29. estimate_efficiency_class_e → co2 = 30.0g → class E
// 30. estimate_efficiency_class_g → co2 = 150.0g → class G

// === EfficiencyClass tests ===

#[test]
fn class_a_co2_below_1() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(0.5);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::A);
}

#[test]
fn class_b_co2_1_to_5() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(2.4);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::B);
}

#[test]
fn class_c_co2_5_to_10() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(7.5);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::C);
}

#[test]
fn class_d_co2_10_to_25() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(18.0);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::D);
}

#[test]
fn class_e_co2_25_to_50() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(30.0);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::E);
}

#[test]
fn class_f_co2_50_to_100() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(75.0);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::F);
}

#[test]
fn class_g_co2_100_or_more() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(150.0);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::G);
}

#[test]
fn class_boundary_zero() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(0.0);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::A);
}

#[test]
fn class_boundary_exact_1() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(1.0);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::B);
}

#[test]
fn class_boundary_exact_100() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::from_co2(100.0);
    assert_eq!(class, codeimpact_hexagon::analysis::EfficiencyClass::G);
}

#[test]
fn class_label_a() {
    assert_eq!(
        codeimpact_hexagon::analysis::EfficiencyClass::A.label(),
        "A"
    );
}

#[test]
fn class_label_b() {
    assert_eq!(
        codeimpact_hexagon::analysis::EfficiencyClass::B.label(),
        "B"
    );
}

#[test]
fn class_label_c() {
    assert_eq!(
        codeimpact_hexagon::analysis::EfficiencyClass::C.label(),
        "C"
    );
}

#[test]
fn class_label_d() {
    assert_eq!(
        codeimpact_hexagon::analysis::EfficiencyClass::D.label(),
        "D"
    );
}

#[test]
fn class_label_e() {
    assert_eq!(
        codeimpact_hexagon::analysis::EfficiencyClass::E.label(),
        "E"
    );
}

#[test]
fn class_label_f() {
    assert_eq!(
        codeimpact_hexagon::analysis::EfficiencyClass::F.label(),
        "F"
    );
}

#[test]
fn class_label_g() {
    assert_eq!(
        codeimpact_hexagon::analysis::EfficiencyClass::G.label(),
        "G"
    );
}

// === EcologicalImpact tests ===

#[test]
fn vo_constructor_stores_values() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::B;
    let impact = codeimpact_hexagon::analysis::EcologicalImpact::new(2.4, 8640.0, class);
    assert!((impact.co2_grams() - 2.4).abs() < 1e-9);
    assert!((impact.energy_joules() - 8640.0).abs() < 1e-9);
    assert_eq!(
        impact.efficiency_class(),
        &codeimpact_hexagon::analysis::EfficiencyClass::B
    );
}

#[test]
fn vo_getters_return_values() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::A;
    let impact = codeimpact_hexagon::analysis::EcologicalImpact::new(0.0, 0.0, class);
    assert!((impact.co2_grams() - 0.0).abs() < 1e-9);
    assert!((impact.energy_joules() - 0.0).abs() < 1e-9);
    assert_eq!(
        impact.efficiency_class(),
        &codeimpact_hexagon::analysis::EfficiencyClass::A
    );
}

#[test]
fn vo_clone_debug_partial_eq() {
    let class = codeimpact_hexagon::analysis::EfficiencyClass::C;
    let impact = codeimpact_hexagon::analysis::EcologicalImpact::new(5.0, 18000.0, class);
    let cloned = impact.clone();
    assert_eq!(impact, cloned);
    let debug = format!("{:?}", impact);
    assert!(debug.contains("EcologicalImpact"));
}

// === EcologicalImpactEstimator tests ===

#[test]
fn estimate_zero_cpu() {
    let economic = EconomicImpact::new(0.0, 0, 0.0, "low");
    let ecological =
        codeimpact_hexagon::analysis::EcologicalImpactEstimator::estimate(&economic, 400.0);
    assert!((ecological.co2_grams() - 0.0).abs() < 1e-9);
    assert!((ecological.energy_joules() - 0.0).abs() < 1e-9);
    assert_eq!(
        ecological.efficiency_class(),
        &codeimpact_hexagon::analysis::EfficiencyClass::A
    );
}

#[test]
fn estimate_moderate_cpu() {
    // cpu_cost = 6.0 μ$ → kwh = 6.0 * 0.000001 = 0.000006
    // co2 = 0.000006 * 400 = 0.0024 g
    // energy = 0.000006 * 3_600_000 = 21.6 J
    let economic = EconomicImpact::new(6.0, 1000, 6.1, "low");
    let ecological =
        codeimpact_hexagon::analysis::EcologicalImpactEstimator::estimate(&economic, 400.0);
    assert!(ecological.co2_grams() > 0.0);
    assert!(ecological.energy_joules() > 0.0);
}

#[test]
fn estimate_custom_factor() {
    // cpu_cost = 10.0 μ$ → kwh = 10.0 * 0.000001 = 0.00001
    // co2 = 0.00001 * 200 = 0.002 g
    // energy = 0.00001 * 3_600_000 = 36.0 J
    let economic = EconomicImpact::new(10.0, 0, 10.0, "low");
    let ecological =
        codeimpact_hexagon::analysis::EcologicalImpactEstimator::estimate(&economic, 200.0);
    let expected_co2 = 10.0 * 0.000001 * 200.0;
    assert!((ecological.co2_grams() - expected_co2).abs() < 1e-9);
}

#[test]
fn estimate_default_factor() {
    let default = codeimpact_hexagon::analysis::EcologicalImpactEstimator::DEFAULT_CO2_G_PER_KWH;
    assert!((default - 400.0).abs() < 1e-9);
}

#[test]
fn estimate_energy_joules_formula() {
    // cpu_cost = 25.0 μ$ → kwh = 25.0 * 0.000001 = 0.000025
    // energy = 0.000025 * 3_600_000 = 90.0 J
    let economic = EconomicImpact::new(25.0, 0, 25.0, "low");
    let ecological =
        codeimpact_hexagon::analysis::EcologicalImpactEstimator::estimate(&economic, 400.0);
    let expected_energy = 25.0 * 0.000001 * 3_600_000.0;
    assert!((ecological.energy_joules() - expected_energy).abs() < 1e-9);
}

#[test]
fn estimate_co2_grams_formula() {
    // cpu_cost = 25.0 μ$ → kwh = 25.0 * 0.000001 = 0.000025
    // co2 = 0.000025 * 400 = 0.01 g
    let economic = EconomicImpact::new(25.0, 0, 25.0, "low");
    let ecological =
        codeimpact_hexagon::analysis::EcologicalImpactEstimator::estimate(&economic, 400.0);
    let expected_co2 = 25.0 * 0.000001 * 400.0;
    assert!((ecological.co2_grams() - expected_co2).abs() < 1e-9);
}

#[test]
fn estimate_kwh_formula() {
    // cpu_cost = 100.0 μ$ → kwh = 100.0 * 0.000001 = 0.0001
    let economic = EconomicImpact::new(100.0, 0, 100.0, "low");
    let ecological =
        codeimpact_hexagon::analysis::EcologicalImpactEstimator::estimate(&economic, 400.0);
    // co2 = 0.0001 * 400 = 0.04 g
    // energy = 0.0001 * 3_600_000 = 360.0 J
    let expected_co2 = 100.0 * 0.000001 * 400.0;
    let expected_energy = 100.0 * 0.000001 * 3_600_000.0;
    assert!((ecological.co2_grams() - expected_co2).abs() < 1e-9);
    assert!((ecological.energy_joules() - expected_energy).abs() < 1e-9);
}

#[test]
fn estimate_efficiency_class_b() {
    // cpu_cost = 6.0 μ$ → kwh = 0.000006 → co2 = 0.000006 * 400 = 0.0024 g → class A
    // Actually let's use a value that gives co2 = 2.4g
    // co2 = kwh * 400, kwh = cpu * 0.000001
    // co2 = cpu * 0.000001 * 400 = cpu * 0.0004
    // cpu = co2 / 0.0004 = 2.4 / 0.0004 = 6000 μ$
    let economic = EconomicImpact::new(6000.0, 0, 6000.0, "low");
    let ecological =
        codeimpact_hexagon::analysis::EcologicalImpactEstimator::estimate(&economic, 400.0);
    assert_eq!(
        ecological.efficiency_class(),
        &codeimpact_hexagon::analysis::EfficiencyClass::B
    );
}

#[test]
fn estimate_efficiency_class_e() {
    // co2 = 30.0g → cpu = 30.0 / 0.0004 = 75000 μ$
    let economic = EconomicImpact::new(75000.0, 0, 75000.0, "high");
    let ecological =
        codeimpact_hexagon::analysis::EcologicalImpactEstimator::estimate(&economic, 400.0);
    assert_eq!(
        ecological.efficiency_class(),
        &codeimpact_hexagon::analysis::EfficiencyClass::E
    );
}

#[test]
fn estimate_efficiency_class_g() {
    // co2 = 150.0g → cpu = 150.0 / 0.0004 = 375000 μ$
    let economic = EconomicImpact::new(375000.0, 0, 375000.0, "critical");
    let ecological =
        codeimpact_hexagon::analysis::EcologicalImpactEstimator::estimate(&economic, 400.0);
    assert_eq!(
        ecological.efficiency_class(),
        &codeimpact_hexagon::analysis::EfficiencyClass::G
    );
}
