# Alert Thresholds — Threshold Gate Design (US8)

> **Type:** technical
> **Status:** Applied
> **Updated:** 2026-08-17
> **Decided in:** Issue #8 (US8), PR #81 ; #128 (couverture du gate), PR #150
> **Links:** [[architecture-overview]], [[ADR-0017]], [[ADR-0033]], [[ADR-0010]], [[ADR-0006]], [[ADR-0019]], [[json-report-schema]], [[html-report]], [[glossary]]

État courant de la porte de seuils d'alerte. Rationale et alternatives : [[ADR-0017]] (conception initiale) et [[ADR-0033]] (couverture du gate, exit 4).

## Modèle de domaine (hexagone, zéro-dep)

| Élément | Type | Rôle |
|---|---|---|
| `AlertThresholds` | VO | Deux seuils optionnels (`max_energy_kwh`, `max_co2_grams`), auto-validant (`new` rejette non-fini/négatif → `ThresholdError`) |
| `AlertThresholds::evaluate(cpu, co2)` | fn pure | `(Option<f64>, Option<f64>) -> ThresholdReport`. Compare **seulement** sur `(Some, Some)` — `None` ne franchit jamais un seuil ([[ADR-0010]]) |
| `AlertThresholds::from_sources(file, cli)` | fn pure | Fusion par métrique : `cli.or(file)` — la CLI l'emporte |
| `AlertThresholds::none()` | ctor | Aucun seuil ; `evaluate` ne déclenche jamais |
| `ThresholdError` | erreur | Construction rejetée. Son `Display` est **surface utilisateur**, pas de la dette morte : rendu à trois sites de production — `primaries/src/main.rs:129` et `:312` (`eprintln!("erreur: {}", e)` puis exit 1) et `secondaries/.../file_system_config_reader.rs:143` (`e.to_string()` porté dans `AnalysisError`). L'impl est donc conservée et son rendu asservi par test (#115) ; sa suppression sous YAGNI casserait les trois sites |
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
{ "thresholds": { "max_energy_kwh": 0.00001, "max_co2_grams": 12 } }
```

- Seule la section `thresholds` est lue ; `#[serde(default)]` partout ; clés inconnues tolérées (pas de `deny_unknown_fields`) → US15 ajoutera `include`/`exclude` au même fichier sans collision.
- Sécurité (miroir `write_report_file`, [[ADR-0006]]) : canonicalize parent-seul, `symlink_metadata` refuse symlink/FIFO/dir avant lecture, plafond 1 MiB, pas de fuite de path, recursion-depth serde par défaut (128).

## CLI & codes de sortie

| Flag | Effet |
|---|---|
| `--max-kwh <N>` / `--max-co2 <N>` | Seuil CLI (surclasse le fichier par métrique) |
| `--config <path>` | Chemin explicite `.codeimpact.json` |
| `--strict` | Un dépassement → **exit 3** ; une couverture incomplète sans dépassement → **exit 4** ([[ADR-0033]]) |

| Exit code | Signification |
|---|---|
| 0 | Rien n'a dépassé **et** tout était couvert (ou dépassement sans `--strict`) |
| 1 | Erreur d'entrée / runtime (inclut seuil invalide `--max-kwh=-5`) |
| 2 | Réservé clap (arg-parse ; `--max-kwh -5` séparé par espace atterrit ici) |
| 3 | Dépassement en `--strict` — **l'emporte sur 4** |
| **4** | `--strict`, rien de **mesuré** n'a dépassé, mais le gate n'a **pas pu s'appliquer en totalité** (#128, [[ADR-0033]] AD-1) |

`4` se déclenche ssi : `strict` ∧ au moins un seuil configuré ∧ couverture ≠ `Complete` ∧ aucun dépassement. Hors `--strict`, l'exit reste 0. Un seul message est imprimé, celui du code gagnant.

`primaries/src/main.rs:351-378` (`gated_exit_code`, découverte auto de config).

## Couverture du gate — `GateCoverage` (#128, [[ADR-0033]])

`ThresholdReport` répond à « **qu'est-ce qui a dépassé ?** » ; `GateCoverage` répond à « **sur quoi ai-je pu décider ?** ». Deux faits, deux porteurs — `AlertThresholds::evaluate` reste **intouché** (diff nul sur toute la tranche) : il n'a aucune raison de connaître les échecs de lecture de fichier.

| Variant | Signification |
|---|---|
| `Complete` | Tout ce dont le gate avait besoin était disponible — ou aucun seuil n'est configuré (un projet non gaté n'a rien que le gate aurait pu manquer) |
| `Partial { unmeasurable_files: usize, unexplored_subtree: bool }` | Un **compte** pour les fichiers nommés-mais-non-mesurables, un fait **non quantifié** pour au moins un sous-arbre jamais parcouru. Faits **indépendants**, cohabitant dans le même variant, rendus en **deux clauses séparées** — les fusionner fabriquerait un compte ([[ADR-0010]]) |
| `Absent` | Il n'y avait aucun fichier sur lequel être partiel : la mesure unique du run n'a pas pu être prise (jumeau de [[ADR-0032]] AD-5) |

- Défini en `hexagon/src/analysis/gate_coverage.rs:14-47`. Dérivé dans le domaine, en un point unique par use case : `run_analysis.rs:306-321` et `run_stress_test.rs:35`.
- Porté par `GatedOutput<T>` (`hexagon/src/analysis/gated_output.rs`), dont `new` prend la couverture en **troisième argument positionnel obligatoire** — délibérément pas un builder : un builder rend le fait oubliable, et l'oubli retombe silencieusement sur `Complete`, c'est-à-dire sur le mensonge corrigé. 34 mutants en deviennent `unviable`.
- Rendu : `humanize::render_incomplete_coverage_warning` (`secondaries/.../humanize.rs:93-145`, stderr strict, jamais de chemin brut), plus `unexplored_subtree` sur JSON (`json_report_writer.rs:80`) et HTML (`html/view_model.rs:32-38`).

**Ce que la porte ne couvre pas** (énuméré, jamais masqué — [[ADR-0033]] § *Ce que `--strict` ne couvre pas*) : les chemins cachés (`hidden(true)`, **#147 HIGH**, un exit 0 strict ne prouve donc pas encore que tout a été vu), `exclude` sans trace machine (**#145**), l'assainissement console inatteignable depuis l'hexagone (**#146**).

## Câblage du gate

- `RunAnalysis::handle` (cible fichier → impact du fichier ; cible projet → `aggregated_metrics`) et `RunStressTest::handle` prennent `&AlertThresholds`, appellent `evaluate`, retournent `GatedOutput<()>`.
- Métriques gatées : **énergie (kWh) + CO2 (g)** au niveau **agrégat projet** uniquement — jamais par fonction. L'énergie provient de `EcologicalImpact::energy_joules()` convertie en kWh (`/ KWH_TO_JOULES`) à la frontière ; le gate compare des kWh purs.
- Message de dépassement : renderer unique `humanize::render_threshold_warning` (console/JSON/HTML/stderr strict).

`hexagon/src/analysis/run_analysis.rs`, `run_stress_test.rs`, `secondaries/src/gateways/report_writers/humanize.rs`.
