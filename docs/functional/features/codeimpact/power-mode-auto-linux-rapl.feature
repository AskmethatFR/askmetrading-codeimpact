# id: power-mode-auto-linux-rapl
# context: CodeImpact
# origin: #75, #77
@feature:power-mode-auto-linux-rapl
Feature: Auto power mode on Linux via RAPL

  @wip @scenario:S1
  Scenario: A RAPL-accessible host reports measured energy that differs by CPU
    Given a Linux host with RAPL energy counters accessible, on Intel or AMD Zen
    When power measurement runs in auto mode
    Then energy is reported as measured
    And the measured energy differs between two different CPU models under equal load

  @wip @scenario:S2
  Scenario: Missing RAPL privileges falls back honestly with an actionable message
    Given a Linux host where the RAPL energy counter is not accessible due to insufficient privileges
    When power measurement runs in auto mode
    Then the tool falls back to an explicitly labeled estimate, or reports unmeasurable if no fallback applies
    And the report includes a message explaining how to obtain the measurement
    And the tool never crashes and never reports a silent zero

  @wip @scenario:S3
  Scenario: A wrapped energy counter is still measured correctly
    Given a RAPL energy counter that wraps around during the sampled window
    When energy is computed from the counter readings
    Then the wraparound is accounted for
    And the measured energy remains correct

  @wip @scenario:S4
  Scenario: A runner that cannot measure reports the check as skipped, never as passed
    Given a CI runner where the RAPL energy counter is unavailable
    When the RAPL integration test runs
    Then the check is reported as skipped
    And the run never reports it as passed
