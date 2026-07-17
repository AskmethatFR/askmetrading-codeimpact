# Alert Thresholds — Threshold Gate Design (US8)

> **Type:** technical
> **Status:** Applied
> **Updated:** 2026-07-17
> **Decided in:** Issue #8 (US8), PR #81
> **Links:** [[architecture-overview]], [[ADR-0017]], [[ADR-0010]], [[ADR-0006]], [[json-report-schema]], [[html-report]], [[glossary]]

État courant de la porte de seuils d'alerte. Rationale et alternatives : [[ADR-0017]].

## Modèle de domaine (hexagone, zéro-dep)

| Élément | Type | Rôle |
|---|---|---|
| `AlertThresholds` | VO | Deux seuils optionnels (`max_cpu_microdollars`, `max_co2_grams`), auto-validant (`new` rejette non-fini/négatif → `ThresholdError`) |
| `AlertThresholds::evaluate(cpu, co2)` | fn pure | `(Option<f64>, Option<f64>) -> ThresholdReport`. Compare **seulement** sur `(Some, Some)` — `None` ne franchit jamais un seuil ([[ADR-0010]]) |
| `AlertThresholds::from_sources(file, cli)` | fn pure | Fusion par métrique : `cli.or(file)` — la CLI l'emporte |
| `AlertThresholds::none()` | ctor | Aucun seuil ; `evaluate` ne déclenche jamais |
| `ThresholdReport` / `ThresholdBreach` / `BreachedMetric` | VO | Résultat du gate ; `has_breach()` porte la décision d'exit |
| `GatedOutput<T>` | wrapper | Payload du use case + `ThresholdReport` ; décision dans le domaine, mapping dans `main.rs` |

`hexagon/src/analysis/alert_thresholds.rs`, `gated_output.rs`.

## Port & adaptateur (DIP — hexagone zéro-dep)

| Port (hexagone) | Adaptateur (secondaries) | Techno |
|---|---|---|
| `ConfigReaderPort::read_thresholds(explicit_path, search_dirs) -> Result<Option<AlertThresholds>>` | `FileSystemConfigReader` | serde_json |

- `explicit_path: Some` → honoré exactement (manquant/invalide = erreur, pas de fall-through).
- `explicit_path: None` → `search_dirs` essayés dans l'ordre (dir de la cible, puis cwd) ; le premier `.codeimpact.json` gagne.
- `Ok(None)` = aucun fichier trouvé (optionnel, AC6), pas une erreur.

`hexagon/src/analysis/config_reader.rs`, `secondaries/src/gateways/config_readers/file_system_config_reader.rs`.

## Schéma de config `.codeimpact.json` (partagé, réservé pour US15 #31)

```json
{ "thresholds": { "max_cpu_microdollars": 50, "max_co2_grams": 12 } }
```

- Seule la section `thresholds` est lue ; `#[serde(default)]` partout ; clés inconnues tolérées (pas de `deny_unknown_fields`) → US15 ajoutera `include`/`exclude` au même fichier sans collision.
- Sécurité (miroir `write_report_file`, [[ADR-0006]]) : canonicalize parent-seul, `symlink_metadata` refuse symlink/FIFO/dir avant lecture, plafond 1 MiB, pas de fuite de path, recursion-depth serde par défaut (128).

## CLI & codes de sortie

| Flag | Effet |
|---|---|
| `--max-cpu <N>` / `--max-co2 <N>` | Seuil CLI (surclasse le fichier par métrique) |
| `--config <path>` | Chemin explicite `.codeimpact.json` |
| `--strict` | Un dépassement → **exit 3** |

| Exit code | Signification |
|---|---|
| 0 | OK (ou dépassement sans `--strict`) |
| 1 | Erreur d'entrée / runtime (inclut seuil invalide `--max-cpu=-5`) |
| 2 | Réservé clap (arg-parse ; `--max-cpu -5` séparé par espace atterrit ici) |
| 3 | Dépassement en `--strict` |

`primaries/src/main.rs` (`gated_exit_code`, découverte auto de config).

## Câblage du gate

- `RunAnalysis::handle` (cible fichier → impact du fichier ; cible projet → `aggregated_metrics`) et `RunStressTest::handle` prennent `&AlertThresholds`, appellent `evaluate`, retournent `GatedOutput<()>`.
- Métriques gatées : **CPU (µ$) + CO2 (g)** au niveau **agrégat projet** uniquement — jamais par fonction.
- Message de dépassement : renderer unique `humanize::render_threshold_warning` (console/JSON/HTML/stderr strict).

`hexagon/src/analysis/run_analysis.rs`, `run_stress_test.rs`, `secondaries/src/gateways/report_writers/humanize.rs`.
