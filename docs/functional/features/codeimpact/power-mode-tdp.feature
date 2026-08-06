# id: power-mode-tdp
# context: CodeImpact
# origin: #75, #79
@feature:power-mode-tdp
Feature: TDP power mode — CPU detection, TDP table, manual override

  @wip @scenario:S1
  Scenario: TDP mode estimates power from a detected CPU's TDP and attributed utilization
    Given a host whose CPU model is detected and resolves to a known TDP table entry
    When power measurement runs in tdp mode
    Then power is reported as estimated, computed from that TDP and the attributed CPU utilization
    And the TDP source is shown in the report

  @wip @scenario:S2
  Scenario: A manually declared TDP or CPU model takes precedence over auto-detection
    Given the operator declares a TDP value or an exact CPU model
    When power measurement runs in tdp mode
    Then the declared value is used instead of the detected one

  @wip @scenario:S3
  Scenario: An unknown CPU with no manual input yields an explicit unmeasurable outcome
    Given the CPU model cannot be detected or resolved to a TDP, and no manual value was declared
    When power measurement runs in tdp mode
    Then the tool reports unmeasurable with an explanatory message
    And the tool never falls back to a generic TDP silently
