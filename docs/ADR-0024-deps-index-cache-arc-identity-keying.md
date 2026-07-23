# ADR-0024 — Cache du `DepsIndex` mémoïsé : clé sur identité de pointeur `Arc`, pas sur empreinte de contenu

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-22
> **Decided in:** Issue #90 (T5 — durcissement cache-poisoning + poison mutex avant réutilisation LSP)
> **Links:** [[architecture-overview]], [[ADR-0023]], [[ADR-0020]], [[ADR-0006]], [[ADR-0015]], [[glossary]]
> **Relations:**
>   depends-on: ["ADR-0023", "ADR-0020"]
>   related: ["ADR-0006", "ADR-0015", "architecture-overview", "glossary"]

## Contexte

[[ADR-0023]] (US14-T5) a introduit un `DepsIndex` (index namespace→fichiers) **mémoïsé** dans `TreeSitterCodeParser`, pour ne pas reconstruire le graphe de dépendances inter-fichiers à chaque appel de `resolve_dependencies` (appelé une fois par fichier du projet). La revue Sécurité de #33 T5 a laissé **deux dettes LOW explicites**, à fermer avant qu'un adaptateur LSP ne réutilise une même instance de parser entre plusieurs scans (issue #90) :

1. **Cache-poisoning.** La clé de cache était une empreinte de `(chemin, source.len())` — la **longueur**, pas le contenu. Deux jeux de fichiers aux mêmes chemins et mêmes longueurs par-fichier mais au contenu différent **collisionnent** → le `DepsIndex` périmé est réutilisé silencieusement → résolution de dépendances vers le mauvais namespace. Latent aujourd'hui (parser construit à neuf par scan CLI one-shot, `file_sources` jamais muté en cours de scan), mais bug vivant dès qu'un contexte long-vécu (LSP) réutilise le parser.
2. **Poison mutex.** `deps_index_cache.lock().unwrap()` panique sur mutex empoisonné — inatteignable en mono-thread, mais fragile avant toute parallélisation.

## Décision

**Clé de cache = identité de pointeur `Arc` de `ctx.file_sources`, via `Arc::ptr_eq`. L'empreinte de contenu (`file_set_fingerprint`) est supprimée entièrement.**

```rust
type DepsIndexCacheEntry = (Arc<Vec<(PathBuf, String)>>, Arc<DepsIndex>);
deps_index_cache: Mutex<Option<DepsIndexCacheEntry>>,

fn deps_index(&self, ctx: &DependencyContext) -> Arc<DepsIndex> {
    {
        let cache = self.deps_index_cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((cached_sources, index)) = cache.as_ref() {
            if Arc::ptr_eq(cached_sources, &ctx.file_sources) {
                return Arc::clone(index);           // hit O(1), aucun octet touché
            }
        }
    }
    let index = Arc::new(build_deps_index(&self.profile, &ctx.file_sources, &ctx.source_roots));
    *self.deps_index_cache.lock().unwrap_or_else(|p| p.into_inner()) =
        Some((Arc::clone(&ctx.file_sources), Arc::clone(&index)));
    index
}
```

**Pourquoi l'identité de pointeur suffit — et est plus forte qu'un hash.** `Vec<(PathBuf, String)>` n'a **aucune mutabilité interne** (pas de `Cell`/`RefCell`). Une fois un `Arc<Vec<...>>` construit, les octets pointés sont immuables pour la durée de l'allocation. « Même pointeur » implique donc **prouvablement** « même contenu » — au sens des règles d'aliasing de Rust — là où une empreinte 64 bits ne donnait que « probablement même contenu ». Dans `run_analysis`, `file_sources` est construit **une seule fois par scan** puis `Arc::clone` dans chaque `DependencyContext` de la boucle fichiers : le chemin de hit ne dépend donc plus de la taille du projet.

### Alternative rejetée — empreinte de contenu (`source.hash()`)

Un correctif intermédiaire hashait le **contenu complet** (`source.hash()` au lieu de `source.len()`). Rejeté par la barrière de revue (Dev-B ∥ QA ∥ Security, unanime) :

- **Régression de perf / CWE-400.** Hashait tout le contenu de tous les fichiers à **chaque** appel `resolve_dependencies` → O(N_fichiers × octets_totaux) par scan, **dès aujourd'hui**, pas seulement sous LSP. Au plafond `MAX_PROJECT_SOURCE_BYTES = 100 MiB` ([[ADR-0015]] / `source_guard`) et milliers de fichiers, des minutes de hachage évitable — la classe de coût même que le commentaire pré-correctif existait pour éviter. Sous LSP (réutilisation par édition) c'est exactement le motif tueur de réactivité que #90 vise à anticiper.
- **Résidu de collision.** `DefaultHasher` est un SipHash-1-3 à **clé fixe** — collidable délibérément (~2^32). Blast radius faible (arête de dépendance erronée, pas RCE), mais un vecteur de cache-poisoning résiduel qu'`Arc::ptr_eq` élimine par construction (pas d'espace de hash à attaquer).

### Poison mutex

Les deux sites `lock()` récupèrent le guard sur empoisonnement : `unwrap_or_else(|poisoned| poisoned.into_inner())`. Sûr sans risque d'état déchiré : chaque section critique est une lecture/écriture unique non-panicante ; le chemin de lecture re-valide via `Arc::ptr_eq` avant de faire confiance à l'entrée → une mauvaise récupération dégrade en reconstruction, jamais en résultat silencieusement faux.

## Conséquences

- **Compromis assumé.** Deux `Arc<Vec<...>>` construits indépendamment mais octet-identiques ne partagent plus l'entrée de cache (une reconstruction en trop). Rare (production construit exactement un `Arc` `file_sources` par scan), sans impact de correction.
- **Simplification.** Suppression de `file_set_fingerprint`, de `DefaultHasher` et des imports `Hash`/`Hasher` associés ; chemin chaud réduit à une comparaison de pointeurs.
- **Les deux dettes LOW de #33 T5 sont fermées** ; la régression de perf MEDIUM introduite par l'alternative rejetée est fermée par la même décision.
- **Note pour l'évolution.** L'avaleur de poison inconditionnel aux deux sites suppose des sections critiques atomiques : si le bloc verrouillé grandit vers un invariant multi-étapes, réviser (aucun alarme automatique).

## Tests (TDD, red→green)

- `stale_deps_index_is_not_reused_when_file_content_changes_but_lengths_match` — AC1 (pas de réutilisation périmée).
- `deps_index_lookup_recovers_from_a_poisoned_cache_mutex_instead_of_panicking` — AC2 (récupération poison, couvre les deux sites).
- `deps_index_reuses_the_same_arc_but_rebuilds_for_a_different_arc_with_identical_content` — épingle le keying par identité (vérifié par mutation contre « always-hit » et « content-hash »).
