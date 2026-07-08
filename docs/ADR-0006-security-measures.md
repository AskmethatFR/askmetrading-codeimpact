# ADR-0006: Sécurité — Canonicalize, Limite Taille, Pas de Fuite de Path

**Status:** Applied  
**Date:** 2026-07-08  
**Applied in:** US1  
**Relations:**  
  depends-on: ["architecture-overview"]  

## Context

Le CLI lit des fichiers sur le filesystem. Risques: path traversal, fichier immense, fuite de chemin absolu dans les messages d'erreur.

## Decision

1. **Canonicalize** tous les chemins en entrée (`fs::canonicalize`) pour résoudre les symlinks et `..`.
2. **Limite de taille**: refuser les fichiers > 1 MB (const `MAX_FILE_SIZE`).
3. **Pas de fuite de path**: les messages d'erreur utilisent des identifiants anonymes. Le chemin absolu n'est jamais affiché à l'utilisateur.

## Consequences

- Path traversal impossible (canonicalize résout les `../../`).
- Fichier de 100 MB ne fait pas planter l'analyse.
- Les logs utilisateur ne contiennent pas de chemins sensibles.
