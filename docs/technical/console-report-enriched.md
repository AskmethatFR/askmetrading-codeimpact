# Console Report Format — CodeImpact (Enriched)

**id:** console-report-enriched  
**type:** technical  
**owner:** Architect  
**status:** Applied  
**decided_in:** #12  
**relations:**  
  depends-on: ["architecture-overview"]  
  contrasts-with: ["json-report-schema"]  

## Overview

The console report is the human-readable output of `CodeImpact` analysis. US12 enriched the format to surface **all complexity types** — direct, transitive, hidden — plus warnings and I/O-in-loops per file in the project report. Changes are confined to `console_report_writer.rs` (secondaries/adapter layer); no hexagon traits, no CLI, no JSON schema changed.

## Console Report (Single File / `write_console`)

```
=== Rapport d'analyse ===
Complexité directe: 5
Complexité transitive: 8 (dont 3 cachée dans les appels)
Profondeur d'appels max: 2
Fonctions avec cycle: 0
Niveau: low

=== Détails par fonction ===
  main — directe: 5, transitive: 8, profondeur: 2 (main.rs:1)

=== Impact économique estimé ===
Coût CPU: $0.000013
Mémoire: 4.9 KB
Coût total: $0.000013
Niveau: low

=== Avertissements ===
[CRITICAL][QuadraticLoop] process_data → boucle quadratique détectée (src/lib.rs:42)
========================

=== I/O dans boucles ===
[CRITICAL] read_file → I/O dans boucle: std::fs::read (src/reader.rs:10)
========================
```

### Sections

| Section | Condition | Content |
|---|---|---|
| Header | Always | Direct, transitive, hidden complexity + call depth + cycle count + level |
| Fonctions | If `function_details` not empty | Per-function: name, direct, transitive, depth, cycle flag, location |
| Impact éco | If `economic_impact` present | CPU cost, memory (KB/MB adaptive), total cost, level |
| Impact éco | If `ecological_impact` present | CO₂, energy (J/kJ adaptive), efficiency class |
| Avertissements | If warnings non-empty | `[SEVERITY][PatternName] function → message (location)` |
| I/O boucles | If io_in_loops non-empty | `[CRITICAL] function → I/O dans boucle: io_call (location)` |

### Warning line format

```
[SEVERITY][PatternName] function → message (location)
```

Where `PatternName` comes from `{:?}` Debug formatter on the `WarningPattern` enum (e.g. `QuadraticLoop`, `NestedLoops`, `DeepCallChain`). Debug chosen over `Display` to keep the hexagon zero-dep (no `Display` trait definition needed in the hexagon).

## Project Report (Multi-File / `write_project_report`)

```
=== Métriques par fichier ===
src/main.rs — complexité directe: 5, complexité transitive: 8, niveau: low
    main — directe: 5, transitive: 8, profondeur: 2 (main.rs:1)
    complexité cachée dans les appels: 3
    avertissements:
      [CRITICAL][QuadraticLoop] process_data → boucle quadratique (src/lib.rs:42)
    I/O dans boucles:
      [CRITICAL] read_file → I/O dans boucle: std::fs::read (src/reader.rs:10)
src/utils.rs — complexité directe: 3, complexité transitive: 5, niveau: low
    parse — directe: 3, transitive: 5, profondeur: 1 (utils.rs:15)

=== Chaînes de consommation ===
  src/main.rs → parse → process

=== Cycles ===
  (aucun cycle détecté)

=== Résumé du projet ===
Fichiers analysés: 2
Dépendances totales: 3
Complexité directe totale: 8
Complexité transitive totale: 13
Profondeur max de chaîne: 3
Fichiers en cycle: 0

=== Impact économique total ===
Coût CPU: $0.000025
Mémoire: 8.2 KB
Coût total: $0.000026
Niveau: low
```

### Per-file section enrichment (US12 additions)

| Field | Condition | Format |
|---|---|---|
| Hidden complexity | **Always** shown | `complexité cachée dans les appels: N` |
| Warnings | **Only if** per-file warnings non-empty | Indented `avertissements:` block with `[SEVERITY][PatternName]` lines |
| I/O in loops | **Only if** per-file io_in_loops non-empty | Indented `I/O dans boucles:` block with `[CRITICAL]` lines |

Hidden complexity is unconditional (always shown per file because it's always derivable as `transitive - direct`). Warnings and I/O sections are conditional to avoid clutter in clean files.

## Architecture Decisions

| ID | Decision | Rationale |
|---|---|---|
| **ADR-12.1** | `{:?}` Debug for pattern names | Zero-dep hexagon constraint — no `Display` trait import in domain. Debug is provided by `#[derive(Debug)]` on `WarningPattern` enum, which is already present and does not add dependencies. |
| **ADR-12.2** | `_to(&mut dyn Write)` methods added | Testability — captures output to `Vec<u8>` buffer instead of stdout. Trait unchanged (`ReportWriter` still delegates to internal methods). |
| **ADR-12.3** | Hidden complexity unconditional per file | Always derivable from `transitive - direct`. No cost to compute. Provides immediate signal even on clean code. |
| **ADR-12.4** | Warnings/I/O conditional per file | Reduces noise — clean files show no empty sections. Follows "don't print what isn't there" principle. |

## Testability

The `write_console_to(&mut dyn Write)` and `write_project_report_to(&mut dyn Write)` methods enable testing without polluting stdout:

```rust
let mut buf = Vec::new();
writer.write_console_to(&mut buf, &metrics);
let output = String::from_utf8(buf).unwrap();
assert!(output.contains("[CRITICAL][QuadraticLoop]"));
```

4 integration tests cover: pattern name display, per-file warnings, per-file I/O, conditional section suppression.

## References

- [[architecture-overview]] — module structure, ReportWriter port
- [[json-report-schema]] — JSON counterpart (contrast: machine-readable vs human-readable)
- Source: `secondaries/src/gateways/report_writers/console_report_writer.rs`
- Tests: `tests/secondaries.integration_test/tests/console_report_writer_test.rs`
