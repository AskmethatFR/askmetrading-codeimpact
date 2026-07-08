# ADR-0002: Bounded context unique (YAGNI)

**Status:** Accepted  
**Date:** 2026-07-08

## Context

Le périmètre est cohérent: analyse d'impact du code. Pas de polysème, pas de sous-domaine distinct.

## Decision

Un seul bounded context **CodeImpact**. Split uniquement si un deuxième contexte émerge (ex: dashboard historique, recommandations).

## Consequences

- Structure plate: `codeimpact/hexagon/`, `codeimpact/primaries/`, `codeimpact/secondaries/`
- Package-by-context respecté (le context est au top-level du workspace)
- 0 coût de migration vers multi-contexts (extraire un dossier)