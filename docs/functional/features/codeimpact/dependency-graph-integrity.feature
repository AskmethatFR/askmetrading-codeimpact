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

  # Added in #132. T4.3 wrote a long, precise degradation chain under AD-6 (ADR-0030) on
  # the explicit ground that the operator leans on that text to decide whether to trust the
  # graph — then the chain reached no operator surface at all. A dense graph read without
  # its caveat is the ADR-0010 danger one notch up: not a fabricated metric, but an honest
  # metric whose honesty never leaves the code. The edge count and the caveat now travel
  # together, on the surface where the count is displayed. The caveat enumerates the blind
  # spots rather than summarizing them (AD-6) — a coverage count alone tells the operator
  # how much was measured, never what the graph cannot see.
  @scenario:S2
  Scenario: The dependency edge count is never displayed without saying what the graph cannot see
    Given a project whose language resolves only part of its inter-file dependencies
    When the project analysis report is produced
    Then the surface that displays the total dependency count also states what produces no edge
    And that statement names each blind spot rather than only counting the files measured
    And a project whose language resolves every dependency carries no such statement
