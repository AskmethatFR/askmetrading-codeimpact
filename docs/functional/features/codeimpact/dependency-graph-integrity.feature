# id: dependency-graph-integrity
# context: CodeImpact
# origin: #34
@feature:dependency-graph-integrity
Feature: Inter-file dependency graph integrity

  # Added in #34 T4.2. The graph is right to demand that both endpoints of an edge be
  # nodes it knows — but the composition that feeds it can violate that precondition on
  # its own: dependency resolution is anchored on the files that were READ, while the
  # graph is built from the files that were ANALYZED SUCCESSFULLY. Any file that is read
  # and then found unmeasurable (oversized, unparseable) while some other file depends on
  # it makes those two sets diverge, and the whole scan exits with an error instead of
  # producing a report. Latent for C# since US16; T4's relative imports make it probable,
  # so it is closed before that slice lands. Dropping the edge is not a measurement
  # silence: the file is already named in the report's unmeasurable list (ADR-0010).
  @scenario:S1
  Scenario: A dependency on a file that could not be analyzed does not fail the whole scan
    Given a project where one file is read but cannot be analyzed
    And another file in that project depends on it
    When the project is analyzed
    Then a report is produced rather than an error
    And no edge is recorded for that dependency
    And the file that could not be analyzed is still named among the unmeasurable files
