# ADR-0023 — Résolution des dépendances inter-fichiers C# : index namespace→fichiers, arêtes N:M, dégradation honnête

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-20
> **Decided in:** Issue #33 (US14-T5 — graphe de dépendances C#)
> **Links:** [[architecture-overview]], [[ADR-0020]], [[ADR-0018]], [[ADR-0019]], [[ADR-0021]], [[ADR-0014]], [[ADR-0006]], [[ADR-0024]], [[glossary]]
> **Relations:**
>   depends-on: ["ADR-0020", "ADR-0018", "ADR-0019", "ADR-0021"]
>   related: ["ADR-0014", "ADR-0006", "ADR-0024", "architecture-overview", "glossary"]

## Contexte

[[ADR-0020]] (US14-T2) a rendu le C# analysable — complexité, fonctions, impact éco/écolo — mais avait laissé **une dette explicite et nommée** : *« `resolve_dependencies` rend vide pour le C# en T2 ; le graphe de dépendances namespace→fichier est explicitement T5 »*. Ce cycle (#33, US14-T5) **encaisse cette dette** : `TreeSitterCodeParser::resolve_dependencies` — un stub vide en T2 — rend désormais de **vraies arêtes** pour le C#. C'est le **premier graphe de dépendances inter-fichiers pour un langage non-Rust**, et la preuve que le port neutre d'[[ADR-0018]] (`resolve_dependencies(source, &DependencyContext) -> Vec<PathBuf>`) supporte une sémantique de modules radicalement différente de celle de Rust sans changer de forme.

**Le Rust résout des modules (`crate::`/`super::`/`mod.rs`, un fichier ↔ un chemin de module — voir [[ADR-0014]], [[ADR-0018]]) ; le C# résout des *namespaces*, et un namespace n'est pas un fichier.** C'est la difficulté centrale que cet ADR tranche.

> **Note de correction — référence périmée du ticket.** Le ticket de cadrage de ce cycle renvoyait à « ADR-0011 » pour le graphe d'appels. **C'est une référence périmée** : ADR-0011 est *« Stress test — portée workspace »*, sans rapport. La sémantique de résolution inter-fichiers et de graphe d'appels C# est régie par **le présent ADR-0023**, qui **supplante** cette référence erronée. Aucun contenu d'ADR-0011 n'est concerné.

## Décision

### D1 — Sémantique namespace N:M + pré-passe projet-globale mémoïsée et gardée

En C#, `using Foo.Bar;` ne désigne **pas** un fichier : il désigne un **namespace**, et un namespace est une relation **N:M** —

- un namespace est **déclaré par N fichiers** (`namespace Foo.Bar { … }` peut apparaître dans plusieurs fichiers) ;
- un fichier **déclare M namespaces** (plusieurs blocs `namespace` par fichier sont légaux).

La résolution se fait donc en **deux temps**, via un **index projet-global `namespace → fichiers déclarants`** construit en une **pré-passe** :

1. **Pré-passe (une fois par projet).** Une requête tree-sitter dédiée **`csharp_deps.scm`** extrait, pour chaque fichier, les namespaces qu'il **déclare**. On en construit l'index `namespace → Vec<fichier déclarant>` (la relation N).
2. **Résolution par fichier.** Pour chaque fichier, ses `using` sont **mappés à travers l'index** : `using Foo.Bar;` → **tous** les fichiers qui déclarent `Foo.Bar`. Un `using` vers un namespace externe (BCL, NuGet) n'a aucun déclarant dans le projet → **arête absente**, jamais fausse (honnêteté d'[[ADR-0010]], comme la dépendance non résolue d'[[ADR-0018]]).

> **Un fichier se lie à *tout* déclarant d'un namespace qu'il utilise.** C'est la source de dégradation revendiquée en D4 : au grain namespace, on ne sait pas *quel* déclarant précis est réellement consommé, donc on les lie tous. C'est une **sur-approximation honnête**, pas une invention.

**`FileConsumptionGraph` et la signature du port `CodeParser` restent INCHANGÉS.** L'algèbre de graphe pure de l'hexagone (nœuds, arêtes, atteignabilité — [[ADR-0018]] L1) **se généralise gratuitement** : elle ne sait rien des namespaces, elle consomme des `Vec<PathBuf>` déjà résolus. Toute la sémantique namespace vit **privée dans l'adaptateur** tree-sitter — exactement la discipline d'[[ADR-0018]] (« le domaine nomme le concept, l'adaptateur nomme la syntaxe »), transposée du module Rust au namespace C#.

**Gardes de la pré-passe, hérités d'[[ADR-0020]] § D5, rejoués intégralement :** `catch_unwind` **par fichier** (un fichier hostile ne tue pas la pré-passe), `source_guard` **1 Mo par fichier**, et le budget **`PARSE_QUERY_BUDGET`** couvrant parse + curseur de requête. **Chaque fichier est parsé exactement deux fois** — une passe métriques (`csharp.scm`) et une passe dépendances (`csharp_deps.scm`) — et le résultat est **mémoïsé** : jamais de re-parse au sein d'un scan.

### D2 — `DependencyContext` étendu **additivement** (non cassant)

Le VO neutre d'[[ADR-0018]] `DependencyContext { current_file, project_root, available_files }` gagne **deux champs**, sans retirer ni renommer les existants :

```rust
pub struct DependencyContext {
    pub current_file: PathBuf,
    pub project_root: PathBuf,
    pub available_files: Vec<PathBuf>,
    pub file_sources: Arc<Vec<(PathBuf, String)>>,   // NOUVEAU — sources de tout le projet
    pub source_roots: Vec<PathBuf>,                    // NOUVEAU — racines de source résolues (D3)
}
```

- **`file_sources: Arc<Vec<(PathBuf, String)>>`** porte le **texte source de tout le projet**, requis par la pré-passe D1 (l'index namespace se construit sur *toutes* les sources, pas seulement le fichier courant). Le `Arc` est un choix de sécurité, pas de confort (voir D5).
- **`SynCodeParser` (Rust) ignore les deux nouveaux champs** — sa résolution de modules reste fichier-par-fichier via `available_files`. L'extension est donc **non cassante par construction** : l'adaptateur Rust ne les lit jamais.

**Tout l'I/O disque continue de transiter par `run_analysis` → `ctx`.** L'adaptateur tree-sitter ne lit **jamais** de fichier lui-même : il reçoit les sources déjà lues dans `ctx.file_sources`. La discipline de lecture d'[[ADR-0006]] (canonicalize, plafonds de taille, refus symlink/FIFO, pas de fuite de path) **reste centralisée dans le `CodeReader`** — un seul point de lecture disque, un seul point de garde. L'adaptateur de parsing reste une fonction pure `sources → arêtes`.

### D3 — `sourceRoots` câblé : la clé réservée d'[[ADR-0019]] honorée, via un nouveau `canonical_root`

[[ADR-0019]] avait **déclaré mais laissé inerte** la clé `sourceRoots` (« parsée mais non câblée ; câblage laissé à #33/#34 »). Ce cycle la câble **sans rouvrir le contrat du fichier** — exactement la coordination inter-tickets qu'[[ADR-0019]] avait actée.

- **Forme** : `sourceRoots` est un **`Vec<String>` plat**, chaque entrée résolue **contre la racine projet canonicalisée**.
- **Nouvelle méthode de port `CodeReader::canonical_root`** : parce que la vérification d'appartenance d'un fichier à une racine de source compare **deux chemins**, et qu'un `..`/symlink non résolu d'un seul côté fausserait le test, la **canonicalisation doit être faite par l'adaptateur FS** (seul détenteur de l'accès disque, [[ADR-0006]]). Le port gagne donc :

  ```rust
  fn canonical_root(&self, root: &Path) -> Result<PathBuf, AnalysisError>;
  ```

  **Défaut identité** sur le trait (`root` rendu tel quel — les stubs et adaptateurs sans notion de FS ne sont pas cassés) ; **surcharge réelle** dans `FileSystemCodeReader` (via `fs::canonicalize`). Les deux côtés du test d'appartenance à la racine sont ainsi la **même représentation canonique**.
- **Absent → vecteur vide = non restreint.** Pas de `sourceRoots` (ou liste vide) ⇒ aucune restriction de périmètre de résolution ⇒ comportement inchangé. Même sémantique d'absence byte-neutre qu'[[ADR-0019]] pour le walk.

### D4 — Dégradation honnête via le canal `capabilities()` (couture d'[[ADR-0020]], rendue par [[ADR-0021]])

La couture `capabilities() -> LanguageCapabilities` (carte `métrique → MetricSupport { Supported | Degraded | Unsupported }`) a été posée en T2 ([[ADR-0020]] D3, *« tout `Supported` en T2 »*) et son **rendu** livré par T3 ([[ADR-0021]]). T5 est le **premier émetteur d'un `Degraded` réel** :

| Métrique | `MetricSupport` C# | Justification |
|---|---|---|
| `cross_file_dependencies` | `Degraded("namespace-level resolution; a file links to every declarer of a used namespace")` | Sur-approximation N:M de D1 : au grain namespace, on ne distingue pas le déclarant réellement consommé. |
| `call_graph` | `Degraded("name-based resolution; unresolved-receiver calls may merge")` | Résolution d'appels par nom : deux appels de récepteurs distincts mais de même nom peuvent fusionner. |

L'utilisateur **voit** donc que la dépendance C# est de grain namespace, pas fichier-exact — pas un chiffre présenté comme exact. C'est [[ADR-0010]] (honnêteté de la mesure) porté au graphe.

**Explicitement différé — T5.3 : le *drop* précis des arêtes d'appel ambiguës.** Distinguer et **laisser tomber** l'arête d'un appel à récepteur non résolu (plutôt que de la fusionner) demande le **classificateur de récepteur de T4** ([[ADR-0022]]) — non disponible dans ce grain. C'est différé en **T5.3**, qui le réutilisera. Jusque-là, le `Degraded("…may merge")` **déclare la limite** au lieu de la cacher. YAGNI/Lean : on annonce honnêtement la dégradation maintenant, on affine quand la brique T4 est là.

### D5 — Durcissement sécurité (deux tours de revue, findings Security fermés)

Le graphe C# a une propriété que le Rust n'avait pas : **au grain namespace, les arêtes sont denses** (un `using` très partagé lie beaucoup de fichiers). Cette densité a réveillé deux classes de coût dormantes. Quatre gardes, tous issus des deux retries :

1. **Plafond mémoire *agrégé* `MAX_PROJECT_SOURCE_BYTES` (100 Mo, fail-fast).** `read_all_sources` lit **tout le projet une fois** pour la pré-passe D1. [[ADR-0006]] ne bornait jusqu'ici que le **RSS d'un fichier unique** (`MAX_MEASURABLE_SOURCE_BYTES` 1 Mo, `MAX_FILE_SIZE` 10 Mo). La lecture *agrégée* est une **dimension de menace nouvelle** : mille fichiers de 1 Mo passent chaque garde par-fichier mais somment à 1 Go en RAM. `source_guard::check_project_admissible` refuse le projet **avant** de tout charger, en échec net. (Amendement porté dans [[ADR-0006]].)
2. **`Arc` sur `file_sources` (D2).** Le clone de `DependencyContext` par fichier ([[ADR-0018]] dette connue) devient **O(1)** au lieu de **O(N²)** : sans `Arc`, cloner le contexte pour chacun des N fichiers deep-clonerait les N sources → N² octets copiés. Le `Arc` rend le clone du contexte à coût constant. **C'est la résolution de la dette d'optimisation qu'[[ADR-0018]] avait laissée « à traiter uniquement si le profilage le signale »** — le profilage l'a signalée, au grain C#.
3. **Chaque fichier parsé exactement deux fois, mémoïsé** (métriques + dépendances) — voir D1. Pas de re-parse au sein d'un scan.
4. **`FileConsumptionGraph::compute_depth` mémoïsé.** La fonction était **exponentielle** (comptage de chemins non mémoïsé). **Dormante pour Rust** (faible fan-out des modules), elle devient **atteignable par du C# ordinaire** via les arêtes denses de grain namespace. Elle passe en **O(V+E)** grâce à un cache `HashMap` **calqué sur `detect_cycles`** (même patron de mémoïsation déjà éprouvé dans le graphe). (Amendement porté dans [[ADR-0006]].)

Les gardes de la pré-passe (`catch_unwind` par-fichier, `source_guard` par-fichier, `PARSE_QUERY_BUDGET`) restent **intacts** (D1).

## Conséquences

- **(+)** **Le C# a un graphe de dépendances inter-fichiers.** La dette T5 nommée par [[ADR-0020]] est fermée ; le port neutre d'[[ADR-0018]] est **prouvé** sur une sémantique de modules non-Rust.
- **(+)** **L'hexagone n'a pas bougé** : `FileConsumptionGraph` et la signature `CodeParser::resolve_dependencies` sont inchangés. L'algèbre de graphe se généralise gratuitement ; toute la sémantique namespace est privée à l'adaptateur (invariant [[ADR-0001]]/[[ADR-0018]] tenu).
- **(+)** **`sourceRoots` d'[[ADR-0019]] câblé sans rouvrir le contrat de config** — coordination inter-tickets tenue.
- **(+)** **Honnêteté préservée** : la dépendance C# est annoncée `Degraded` (grain namespace), pas présentée comme exacte ([[ADR-0010]], via le canal [[ADR-0021]]).
- **(+)** **Deux classes de coût fermées** (mémoire agrégée, profondeur exponentielle) — dont une (`compute_depth`) qui était une **bombe à retardement latente** pour tout langage à fan-out dense.
- **(=)** **`SynCodeParser` non affecté** : extension `DependencyContext` additive, il ignore les nouveaux champs.
- **(−)** **Résolution de grain namespace, pas fichier-exact** : sur-approximation N:M assumée et déclarée (`Degraded`). Le raffinement fichier-exact n'est pas au programme de ce grain.

## Dette connue, explicitement non traitée

- **T5.3 — drop précis des arêtes d'appel à récepteur ambigu.** Différé (D4) : requiert le classificateur de récepteur de T4 ([[ADR-0022]]). Jusque-là, `call_graph: Degraded("…may merge")` déclare la limite.
- **Grain namespace, pas symbole.** Un fichier se lie à **tout** déclarant d'un namespace utilisé (D1). Distinguer le type/symbole précis consommé (résolution sémantique complète) est hors scope de ce grain — sur-approximation honnête retenue.
- **`MAX_PROJECT_SOURCE_BYTES` = 100 Mo est une borne fixe.** Un très gros monorepo C# légitime au-delà de 100 Mo de sources serait refusé net (fail-fast honnête). Rendre la borne configurable est différé jusqu'à ce qu'un cas réel le demande (YAGNI).
