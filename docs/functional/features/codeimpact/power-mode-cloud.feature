# id: power-mode-cloud
# context: CodeImpact
# origin: #75, #80
@feature:power-mode-cloud
Feature: Cloud power mode — provider coefficients, regional carbon intensity

  @wip @scenario:S1
  Scenario: A detected cloud instance is estimated using its provider's coefficients and real region
    Given the tool runs on a detected cloud instance of a known provider and region
    When power measurement runs in cloud mode
    Then the estimate uses that provider's per-instance-type power coefficients
    And the estimate uses that region's actual carbon intensity
    And the estimate is labeled estimated

  @wip @scenario:S2
  Scenario: Outside of a detected cloud instance, cloud mode refuses explicitly
    Given the tool is not running on a detected cloud instance
    When cloud mode is requested
    Then the tool explicitly refuses the mode
    And no estimate is produced

  @wip @scenario:S3
  Scenario: Cloud detection never blocks a local run
    Given the tool attempts to detect a cloud provider via the metadata endpoint
    When the endpoint is unreachable because the host is not in the cloud
    Then detection fails quickly within a short timeout
    And the run proceeds without blocking
