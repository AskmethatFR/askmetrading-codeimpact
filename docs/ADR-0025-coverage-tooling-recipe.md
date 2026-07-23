# ADR-0025 — Recette de couverture : la sonde se construit *hors* de `llvm-cov-target`, le parallélisme est plafonné

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-23
> **Decided in:** Issue #87 / PR #102
> **Links:** [[architecture-overview]], [[ADR-0009]], [[ADR-0010]], [[ADR-0015]]

## Contexte

`cargo llvm-cov` (job `coverage`, [[ADR-0009]]) souffrait de deux défauts qui rendaient les chiffres de couverture indignes de confiance — au point que les relecteurs lisaient les tests à la main plutôt que de s'y fier (signalé par la revue QA de #33 T2) :

1. **Mauvaise attribution** : `run_analysis.rs`, `language.rs`, `parser_registry.rs`, `alert_thresholds.rs` — exercés uniquement via les crates de test externes (`hexagon.unit_test`, `secondaries.integration_test`), sans `#[test]` in-lib — affichaient ~0 % malgré des tests passants.
2. **`Unmeasurable(SourceTooComplex)` fantômes** : ~9 tests `SynCodeParser` échouaient sous llvm-cov.

**Racine unique des deux** : la recette précédente (PR #70, pour #63) construisait le binaire `codeimpact-parse-probe` **dans** `target/llvm-cov-target` (celui de `cargo-llvm-cov`) puis passait `--no-clean` pour l'y conserver — choix délibéré « pour une découverte déterministe de la sonde quelle que soit la disposition interne » ([[ADR-0015]] découverte de sonde).

- Le graphe de dépendances de la sonde inclut transitivement `codeimpact_hexagon`. Le construire dans l'arbre partagé y laissait une build **non instrumentée** du crate mesuré, là où la build de couverture instrumentée attendait ses propres artefacts → attribution cassée pour tout fichier sans `#[test]` in-lib.
- Les `SourceTooComplex` fantômes ne sont **pas** une interaction rlimit/instrumentation (la sonde n'est jamais instrumentée — `cargo build` simple). C'est de la **contention wall-clock** : une build de couverture à froid + parallélisme pleine-charge affament le sous-processus sonde au-delà de son délai de garde de 10 s (`PROBE_TIMEOUT`, [[ADR-0015]] §6). Reproduit sur des sources triviales d'une ligne à froid, disparu à chaud avec le **même** binaire.

## Décision

Une seule recette, source de vérité pour la CI **et** le local : `scripts/coverage.sh`.

1. **La sonde se construit dans `target/debug` ordinaire**, totalement découplé de l'arbre de `cargo-llvm-cov`. Supprime le partage de répertoire — la cause de la mauvaise attribution. `--no-clean` n'est plus nécessaire (on n'écrit plus rien dans `llvm-cov-target`).
2. **`cargo llvm-cov --workspace --lcov --output-path lcov.info -- --test-threads=4`** — plafonne le parallélisme des tests pour **fermer la classe de contention**, plutôt que d'élargir `PROBE_TIMEOUT`. Élargir le délai aurait été précisément l'anti-pattern « une marge, pas une différence de nature » que proscrit [[ADR-0010]], et aurait silencieusement affaibli la garantie anti-DoS de [[ADR-0015]]. Le délai de 10 s reste **intact**.
3. **Le job `coverage` de `.github/workflows/ci.yml` appelle `./scripts/coverage.sh`** — CI et local ne peuvent plus diverger.

**Aucun code Rust de production n'est touché** : `PROBE_TIMEOUT`, `discover_probe_path`, `RLIMIT_AS`, `verdict_from` sont inchangés. Ce n'est **pas** un amendement à [[ADR-0015]] — dont les garanties (§3 dominance de pile, §6 gardes runtime) restent telles quelles. C'est une décision de **recette d'outillage**, orthogonale au contrat de la sonde.

## Conséquences

- **(+)** Couverture digne de confiance : les 4 fichiers mal attribués passent à une couverture réelle non nulle, **stable sur deux builds à froid indépendants** (résultats byte-identiques — preuve contre le défaut intermittent). 0 `SourceTooComplex` fantôme. Les vrais tests `SourceTooComplex` (entrées réellement pathologiques) échouent toujours correctement.
- **(+)** Une seule recette : la CI ne peut plus mesurer autre chose que le développeur local.
- **(−)** `--test-threads=4` coûte du wall-clock (build à froid ~55–90 s) — acceptable pour un job de couverture qui ne garde pas le chemin de merge rapide.
- **(−)** `*.profraw` et `lcov.info` litièrent l'arbre source sous exécution locale (invisible sur le runner éphémère de CI) — ajoutés au `.gitignore` (même forme d'hygiène que `target/`).

## Renversement assumé de PR #70/#63

PR #70 construisait la sonde dans `llvm-cov-target` **pour** une découverte déterministe. La mesure de #87 établit que ce choix **était** la cause racine du défaut d'attribution : partager l'arbre avec une build de couverture `--no-clean` est incompatible avec une attribution correcte dès que le graphe de la sonde inclut transitivement le crate mesuré. Le renversement n'est pas une question de goût — c'est le choix antérieur prouvé fautif. La découverte de sonde reste correcte via `target/debug` (sibling/cousin de [[ADR-0015]]) ou l'override `CODEIMPACT_PARSE_PROBE`.
