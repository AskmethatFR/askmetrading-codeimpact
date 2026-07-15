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
4. **Confinement défense-en-profondeur au point d'exécution** (ajouté #40) : le binaire de test compilé est confiné au `target/` du projet par `confine_to_target_dir`. Ce check est **rejoué à l'intérieur de `measure_cmd`** (la fonction qui construit et lance `Command::new`), pas seulement chez l'appelant `build_test_binaries`. `measure_cmd` retourne `Result<Command, _>` et exécute le **chemin canonique retourné** (validate-then-use-the-validated-value), pas l'argument brut. Erreur dure (pas `debug_assert!`) → l'invariant tient **par construction dans tout profil de build**, pas par convention de l'appelant.

## Consequences

- Path traversal impossible (canonicalize résout les `../../`).
- Fichier de 100 MB ne fait pas planter l'analyse.
- Les logs utilisateur ne contiennent pas de chemins sensibles.
- (#40) Un futur refactor ne peut plus contourner en silence le confinement du binaire exécuté : `measure_cmd` re-valide au point d'exécution. Résiduel accepté : la fenêtre TOCTOU canonicalize→exec (déjà documentée, non aggravée) et le confinement OS-level (sandbox/seccomp) restent hors scope.
