# id: power-measurement-capability
# context: CodeImpact
# origin: #75
@feature:power-measurement-capability
Feature: Real power measurement — capability, provenance, honest fallback

  @wip @scenario:S1
  Scenario: Same workload reports different, sourced power draw on different CPUs
    Given two different CPU models running the same workload
    When power measurement runs in auto mode on each
    Then each report shows a distinct wattage figure
    And each figure states its measurement provenance

  @wip @scenario:S2
  Scenario: Every energy/CO2 figure carries explicit provenance
    Given a stress-test run producing an energy or CO2 figure
    When the report is generated on any surface (console, JSON, HTML)
    Then the figure is labeled measured, estimated, or unmeasurable
    And the figure is never shown without a stated provenance

  @wip @scenario:S3
  Scenario: Power mode is selectable and auto falls back honestly when measurement is unavailable
    Given the operator selects the auto power mode on a host without an available power sampler
    When the analysis or stress-test runs
    Then the tool falls back to an explicitly labeled estimate, or reports unmeasurable
    And no silent zero is ever produced

  @wip @scenario:S4
  Scenario: The legacy money-to-energy heuristic no longer competes as a rival number
    Given no real power measurement is available
    When energy or CO2 is reported
    Then the existing microdollar-to-kWh heuristic is shown only as the explicitly labeled estimated fallback
    And it is never presented as measured
