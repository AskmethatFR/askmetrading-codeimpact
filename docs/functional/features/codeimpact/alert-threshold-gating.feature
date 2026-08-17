# id: alert-threshold-gating
# context: CodeImpact
# origin: #128
@feature:alert-threshold-gating
Feature: Threshold gating never reports success on what it did not measure

  # Added in #128. The report has always been honest about what it could not measure —
  # unmeasurable_files is populated on all three surfaces with the real reason. But under
  # --strict the consumer is a CI, and a CI reads the exit code, not the report. The rule
  # "an absent metric never breaches: absence is not a confident zero" (ADR-0010) is right
  # in AlertThresholds::evaluate and must stay there untouched — fabricating a breach from
  # an absent measurement would be the very lie ADR-0010 forbids. The defect is downstream:
  # the exit code collapsed "nothing breached" and "nothing I managed to measure breached"
  # onto the same 0. Security measured the consequence: inflating one file past the size
  # guard drops it out of the gated sum and turns a real overrun into a success.
  # Note (retry 1): exit 4 only covers files the tool ATTEMPTED to measure and failed —
  # including, after this retry, a file dropped at directory-walk time (too large for the
  # adapter's own walk-time cap, unreadable, or an access error). A file whose language has
  # no registered parser is never attempted at all, so it is not counted and does not by
  # itself trigger exit 4.
  @scenario:S1
  Scenario: A partially measured project cannot report success under --strict
    Given a project where a threshold is configured and one file cannot be measured
    When the project is analyzed in strict mode and nothing measured exceeds the threshold
    Then the outcome is distinguishable from a project where everything was measured
    And it states how many files were not measured
    And a project where every file was measured still reports plain success
    And a project that genuinely exceeds its threshold still reports the breach

  # Added in #128. Same lie, different shape: a stress test whose measurement failed has no
  # unmeasured *files* at all — it has no measurement. Reporting "1 unmeasured file" here
  # would fabricate a count that does not exist, which is the fault ADR-0032 AD-5 names: an
  # adapter that cannot read a signal propagates the absence, it never manufactures a
  # plausible value. The absence is named as an absence.
  @scenario:S2
  Scenario: A stress test whose measurement is absent cannot report success under --strict
    Given a stress test run whose energy measurement could not be taken
    And a threshold configured in strict mode
    When the run is gated
    Then the outcome is distinguishable from a run that was measured and stayed under
    And it states that the measurement is absent rather than counting missing files
    And a run that was measured and stayed under its threshold still reports plain success
