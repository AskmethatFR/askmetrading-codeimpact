# ADR-0005: Package-by-Context, Package-by-Layer à l'intérieur

**Status:** Accepted  
**Date:** 2026-07-08  
**Relations:**  
  depends-on: ["architecture-overview"]  

## Context

Un seul bounded context (CodeImpact). Comment organiser les modules à l'intérieur?

## Decision

**Package-by-context au top-level** (workspace Rust). Chaque module est un crate du workspace: `hexagon`, `primaries`, `secondaries`, `tests`.

**À l'intérieur de chaque crate, package-by-layer**: `domain_model/`, `gateways-secondary_ports/`, `use_cases-application_services/`.

## Consequences

- Le dossier `codeimpact/` contient tout le bounded context, visible au top-level.
- À l'intérieur d'un crate, la séparation Clean Architecture est explicite.
- Un éventuel split futur (multi-contexts) se fait en déplaçant des dossiers.
