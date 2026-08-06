# id: process-cpu-attribution
# context: CodeImpact
# origin: #75, #76
@feature:process-cpu-attribution
Feature: CPU attribution across the full process tree, with provenance

  @wip @scenario:S1
  Scenario: CPU share is attributed across the full process tree, not just the parent
    Given a stress-test run spawns child processes, such as cargo test spawning test binaries
    When CPU attribution is computed
    Then the reported CPU share includes the target process and its full descendant tree

  @wip @scenario:S2
  Scenario: Energy and CO2 figures carry explicit provenance end-to-end
    Given a completed stress-test run
    When the report is generated
    Then the ecological impact is shown with provenance measured, estimated, or unmeasurable
    And the provenance is shown on every surface: console, JSON, and HTML

  @wip @scenario:S3
  Scenario: No power sampler available yields an explicit unmeasurable outcome
    Given this slice ships only a stub power sampler with no real sampler wired in yet
    When power or energy is requested
    Then the figure is reported unmeasurable with a reason
    And no fabricated wattage is ever produced

  @wip @scenario:S4
  Scenario: Workspace aggregation invariants are preserved
    Given a workspace stress-test with multiple test binaries
    When CPU and memory are aggregated across runs
    Then CPU sums across runs
    And memory reports the max RSS observed, unchanged by the new attribution logic
