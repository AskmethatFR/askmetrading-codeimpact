# id: typescript-javascript-analysis
# context: CodeImpact
# origin: #34
@feature:typescript-javascript-analysis
Feature: TypeScript / JavaScript analysis support

  @scenario:S1
  Scenario: A TypeScript or JavaScript file is analyzed end-to-end like any other supported language
    Given a TypeScript or JavaScript source file
    When it is analyzed
    Then complexity, loops, branching, calls, in-loop I/O and impact are reported as for any other supported language
    And each metric is reported with its honest per-language support level

  # AMENDED (#34, Phase 5). The original wording claimed "no change is required in the
  # hexagon", and asserted properties no runtime test can observe ("no new adapter or
  # port"). Both were wrong to state here. The claim was factually refuted during this
  # cycle — `Language` is a closed enum living in the hexagon, so a new language
  # necessarily adds variants to it — and the operator accepted the change on the ground
  # that it is a change of DATA (the registry of supported languages, part of the
  # ubiquitous language) rather than of STRUCTURE. The architectural half of the original
  # claim is a review-time property and now lives in ADR-0029 with its measured
  # falsification verdict; what remains here is the part a test can actually observe.
  @scenario:S2
  Scenario: A new language is routed by the shared registry, not by a language-specific path
    Given the supported-language registry declares TypeScript and JavaScript with their extensions
    When a file of either language is dispatched for analysis
    Then it is routed by extension through the same registry as every other language
    And an extension no language claims is refused rather than guessed

  @wip @scenario:S3
  Scenario: An external import produces no file-dependency edge
    Given a file that imports an external package, such as `import React from 'react'`
    When the inter-file dependency graph is built
    Then no edge is produced for that external import

  @wip @scenario:S4
  Scenario: A local relative import resolves to a real file-dependency edge
    Given a file that imports another file via a relative path, such as `import './x'`
    When the inter-file dependency graph is built
    Then the import resolves to a real file-dependency edge
    And configured sourceRoots are honored when set

  @scenario:S5
  Scenario: Common non-source directories are excluded by default
    Given a TypeScript/JavaScript project containing node_modules/, dist/, and minified files
    When the project is analyzed with default settings
    Then those paths are excluded from analysis by default

  # Added in #34 T1. The tool ingests sources the operator does not control, and the
  # report IS the product: a name able to forge what the operator reads defeats the
  # tool's purpose. JavaScript is the first supported language whose method names can
  # be arbitrary string literals, so this scenario only became reachable with US17.
  @scenario:S6
  Scenario: A hostile symbol or file name cannot forge the console report
    Given a source file whose function name or file name carries terminal control or bidi-override characters
    When the analysis report is printed to the console
    Then no raw control character reaches the terminal in any report section
    And the neutralized name stays readable and unambiguously decodable
    And the machine-readable formats keep the real name
