# ADR-0026 — Agrégation `MetricSupport` multi-langage : le projet est honnête, jamais un « 0 » fabriqué

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-24
> **Decided in:** Issue #89 / PR #107 (S1 HTML), PR #111 (S2 JSON)
> **Links:** [[architecture-overview]], [[ADR-0021]], [[ADR-0010]], [[ADR-0007]], [[ADR-0008]], [[json-report-schema]], [[html-report]]

## Contexte

[[ADR-0021]] (T3) a rendu honnêtes les métriques **par-fichier / par-nœud** (rendu `n/a` / `degraded` au lieu d'un `0` silencieux quand une capacité de langage manque), mais a **explicitement différé l'agrégat projet** — la tuile de stat en tête du rapport HTML et l'agrégat du JSON projet. Sa « dette connue » nommait ce suivi **T3b**. Sans règle d'agrégation, `build_stats` imprimait `total_io_in_loops = 0` et `serialize_project_metrics` passait `metric_support_dto(None)` + `Some(vec![])` : un projet purement C# lisait **« I/O in loops : 0 »** — exactement le zéro confiant que [[ADR-0010]] proscrit.

Il fallait une règle pour **combiner** le `MetricSupport` d'un ensemble de fichiers (potentiellement multi-langage) en un état projet unique, par axe.

## Décision

Un VO hexagone `AggregateMetricSupport` (zéro-dépendance, `hexagon/src/analysis/language_capabilities.rs`) + un **fold pur** réduisant le `MetricSupport` de chaque fichier (par axe métrique) en un état projet. L'agrégat est porté par `ProjectMetrics.metric_support` (peuplé dans `FileConsumptionGraph::aggregated_metrics()`) — le signal **voyage sur l'objet de données jusqu'aux writers, pas de nouveau port** (même forme qu'[[ADR-0021]] D1). Les deux writers **branchent sur l'agrégat, jamais sur la langue** ([[ADR-0021]] D2).

### La lattice (approuvée par l'humain — GATE 1.5)

Par axe, fold sur `(any_supported, any_degraded, any_unsupported)` où un fichier sans `LanguageCapabilities` (`None`) contribue `Supported` à chaque axe :

| États des fichiers (pour un axe) | Agrégat | Raison ([[ADR-0010]]/[[ADR-0021]]) |
|---|---|---|
| **tous** `Supported` (dont projet vide, fichiers `None`) | `Supported` | Le nombre couvre tout → fiable, rendu nominal. Rust-only reste **byte-identique**. |
| **tous** `Unsupported` | `Unsupported` → `n/a` | Rien n'a été mesuré. On tue le `0` confiant. |
| **tout mélange** : `Supported`+`Unsupported`, ou tout `Degraded`, ou `Degraded`+`Unsupported` | `Degraded(raison)` | Une valeur existe mais sur une **partie** du projet — c'est ce que « dégradé » veut dire. Le nombre est montré, la couverture partielle signalée. |

Formellement : `any_degraded → Degraded` ; sinon `any_supported && any_unsupported → Degraded` (couverture mixte) ; sinon `any_unsupported → Unsupported` ; sinon `Supported`. La raison `Degraded` est un compte de couverture — `"partial: M/N files measured this metric"` (M/N sont des `usize`, digits-only, aucune injection possible). **Quatre axes** seulement (`cyclomatic_complexity`, `io_in_loops`, `economic_impact`, `ecological_impact`) — ceux qu'un cas d'usage appelant consomme ; `call_graph`/`cross_file_dependencies` n'ont pas de tuile, donc non foldés (YAGNI).

### Rendu — deux slices observables

- **S1 (HTML, PR #107)** : `StatVm.support` alimente une tuile qui rend `n/a` (Unsupported) ou un badge `DEGRADED`, via la **whitelist `SUP` fermée** existante (`cls()` `hasOwnProperty`, `textContent`) — [[ADR-0008]] §8.10 préservé, **zéro nouvelle surface XSS** (délta `assets.rs` = 4 lignes, réutilise le chemin durci par #28).
- **S2 (JSON, PR #111)** : `serialize_project_metrics` construit le DTO `metric_support` depuis l'agrégat ; quand `io_in_loops` est `Unsupported`, `io_in_loops` **et** `unclassifiable_io_in_loops_count` sérialisent `null` (jamais `[]`, jamais `0`) — la règle null-jamais-vide d'[[ADR-0021]] D3, au niveau agrégat. **Strictement additif** ([[ADR-0007]]) : l'objet `metric_support` existait déjà (T3), on le rend seulement véridique ; Rust-only reste byte-identique.

## Conséquence honnête à ne pas masquer — l'état `Unsupported` n'est pas atteignable end-to-end aujourd'hui

**Aucun adaptateur expédié n'émet `MetricSupport::Unsupported`** : l'adaptateur C# déclare `io_in_loops` = `Degraded` (T4 a déjà retourné `Unsupported → Degraded`, [[ADR-0021]] D4). Donc un vrai projet purement C# lu par la CLI aujourd'hui affiche l'agrégat `io_in_loops` en **« degraded »** (avec une valeur partielle réelle), **pas « n/a »**. Le chemin `Unsupported → n/a`/`null` est **correct et forward-compatible** (il se déclenchera dès qu'un axe/adaptateur futur déclarera `Unsupported`) mais est aujourd'hui exercé uniquement via des fixtures `LanguageCapabilities` synthétiques dans les tests. La formulation initiale du ticket (« un projet pure-C# lit n/a ») décrivait donc un scénario non reproductible ; le docstring d'`AggregateMetricSupport` et cette ADR le disent explicitement pour qu'un futur mainteneur n'en hérite pas comme d'un fait acquis.

## Conséquences

- **(+)** L'agrégat projet ne ment plus : `n/a`/`degraded` honnêtes sur HTML **et** JSON, la dette T3b d'[[ADR-0021]] est **fermée**.
- **(+)** Projet Rust-only : sortie **byte-identique** au pré-#89 (fold tout-`Supported`, `metric_support` tout `"supported"`, aucun badge) — régression épinglée sur les deux writers.
- **(+)** Writers découplés de la langue (branchent sur `MetricSupport`), hexagone zéro-dépendance, additif JSON ([[ADR-0007]]), discipline §8.10 intacte.
- **(−)** Le badge `Degraded` porte une raison `"partial: M/N"` que le `StatVm` HTML ne surface pas encore (pas de champ `note` sur la tuile) — observation UX pour un futur slice, rien d'unsafe supprimé.
- **Dette / observation** : l'état `Unsupported` reste non atteignable via adaptateur tant qu'un axe ne le déclare pas ; les tuiles dérivées de `cyclomatic_complexity` (Warnings, Max depth, Hotspots) sont câblées mais inertes (C# et Rust supportent la complexité).
