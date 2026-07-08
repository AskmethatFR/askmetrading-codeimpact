# ADR-0003: Pas de Stryker — exécution directe + mesure

**Status:** Accepted  
**Date:** 2026-07-08

## Context

Stryker (mutation testing) est lourd, lent, et ne mesure pas l'impact économique. L'utilisateur veut un équivalent plus léger.

## Decision

Remplacer Stryker par exécution directe des tests existants avec instrumentation (`perf stat`, `time -v`). Le port `TestRunnerPort` lance `cargo test` et capture les métriques réelles.

## Consequences

- Pas de mutation, pas de génération de mutants
- Les tests existants ne sont pas modifiés
- Mesure réelle (CPU/mem/IO) vs estimation (mutation ne donne que du coverage)
- Plus simple à implémenter (wrapper `cargo test` + parsing)