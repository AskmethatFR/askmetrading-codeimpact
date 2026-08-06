# id: power-mode-auto-macos-powermetrics
# context: CodeImpact
# origin: #75, #78
@feature:power-mode-auto-macos-powermetrics
Feature: Auto power mode on macOS via powermetrics

  @wip @scenario:S1
  Scenario: An Apple Silicon Mac with the required privileges reports measured energy
    Given a Mac with Apple Silicon (M1 through M4) and the required privileges available
    When power measurement runs in auto mode
    Then energy is reported as measured
    And the measured value is consistent with the raw powermetrics reading

  @wip @scenario:S2
  Scenario: Missing privileges falls back explicitly, without ever invoking interactive sudo
    Given a Mac where the required privileges are not available
    When power measurement runs in auto mode
    Then the tool falls back to an explicitly labeled estimate with an actionable message
    And the tool never invokes an interactive sudo prompt
