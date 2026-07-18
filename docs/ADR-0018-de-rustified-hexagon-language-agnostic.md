# ADR-0018 — L'hexagone dé-rustifié : la sémantique par-langage vit entièrement dans les adaptateurs

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-18
> **Decided in:** Issue #32 (US14-T1), dérivé de l'étude d'architecture #30 (US14 — support multi-langage)
> **Links:** [[architecture-overview]], [[ADR-0001]], [[ADR-0013]], [[ADR-0014]], [[glossary]]

## Contexte

L'étude d'architecture #30 (US14) a posé la question : *que faut-il pour que CodeImpact analyse du C# (#33) et du TypeScript (#34), et pour adopter la direction tree-sitter recommandée ?* La réponse a d'abord été un audit de ce qui, dans l'hexagone censé être agnostique du langage, était **secrètement rustifié**. Trois fuites — trois endroits où de la sémantique Rust s'était installée dans le domaine ou dans un contrat de port :

| # | Fuite | Où | Symptôme |
|---|---|---|---|
| **L1** | La résolution de modules Rust (`crate::` / `super::`, `.rs`, `mod.rs`) | **dans le domaine** — `hexagon/.../file_consumption_graph.rs` | Le domaine « connaissait » la syntaxe de modules d'**un** langage. Un adaptateur C# aurait dû y injecter ses namespaces. |
| **L2** | Le port `CodeParser::parse_file_dependencies(&str) -> Vec<String>` | **contrat de port** | Protocole *stringly-typed* `"mod:<name>"` / `"use:<path>"` : des jetons de syntaxe Rust traversaient la frontière, à charge du domaine de les ré-interpréter. |
| **L3** | `CodeReader::list_rust_files(dir)` + `ext == "rs"` codé en dur | **contrat de port + adaptateur FS** | Le port de lecture nommait « Rust » et l'adaptateur filtrait sur une extension figée. |

Chacune est le **même défaut qu'[[ADR-0013]]** avait diagnostiqué pour l'I/O-en-boucle — *le domaine nomme le concept, l'adaptateur nomme la syntaxe* — mais laissé ouvert ailleurs. Tant qu'elles subsistaient, écrire un deuxième adaptateur revenait à **éditer l'hexagone** à chaque nouveau langage : violation d'OCP, et fin de l'invariant zéro-dépendance d'[[ADR-0001]].

Ce cycle est un **refactor à comportement constant** (branche `refactor/32-de-rustify-hexagon`, commit `6702d0b`) : comportement prouvé **octet-équivalent**, Dev-B **et** QA APPROVED, `fmt`/`clippy`/`test` verts. Aucune nouvelle capacité utilisateur — c'est le **prérequis** qui débloque #33 / #34 / tree-sitter.

## Décision

### L1 — La résolution de modules quitte le domaine pour l'adaptateur

La logique `resolve_dependency` / `module_path_candidates` (sémantique `crate::` / `super::` / `.rs` / `mod.rs`) sort de `FileConsumptionGraph` et devient des **helpers privés de l'adaptateur** `secondaries/.../syn_code_parser.rs`. L'hexagone `FileConsumptionGraph` ne conserve plus que de l'**algèbre de graphe pure** (nœuds, arêtes, atteignabilité) — aucune opinion sur ce qu'est un « module ».

### L2 — Un port neutre : des `PathBuf` résolus, jamais la syntaxe du langage

Le port `parse_file_dependencies(&str) -> Vec<String>` (le protocole `"mod:"` / `"use:"`) est **supprimé**, remplacé par :

```rust
// hexagon/src/analysis/code_parser.rs
fn resolve_dependencies(
    &self,
    source: &str,
    ctx: &DependencyContext,
) -> Result<Vec<PathBuf>, AnalysisError>;
```

Un nouveau VO **neutre** de l'hexagone porte le contexte de résolution — et **rien de spécifique à un langage** :

```rust
pub struct DependencyContext {
    pub current_file: PathBuf,
    pub project_root: PathBuf,
    pub available_files: Vec<PathBuf>,
}
```

`resolve_dependencies` est **consommé par le use case `RunAnalysis` dans la même tranche** (le VO n'arrive pas seul : son appelant est là). L'adaptateur reçoit `source` + `ctx`, applique **sa** sémantique de modules en privé, et rend des `PathBuf` **déjà résolus** contre `ctx.available_files`. Une dépendance non résolue est **simplement absente** du résultat — jamais une erreur. **Les jetons `"mod:"` / `"use:"` ont entièrement disparu : plus rien hors de l'adaptateur Rust ne les connaît.**

### L3 — Le port de lecture filtre sur un ensemble d'extensions passé

`list_rust_files(dir)` devient :

```rust
fn list_source_files(
    &self,
    dir: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>, AnalysisError>;
```

`FileSystemCodeReader` filtre sur l'ensemble reçu au lieu d'un `ext == "rs"` codé en dur. La **racine de composition** (`run_analysis.rs`) fournit `&["rs"]` — **comportement d'aujourd'hui préservé à l'identique**.

### Renommages de langage ubiquitaire — `match` est le mot de Rust, `branch` est celui du domaine

`match` est un mot-clé Rust ; le domaine, lui, parle de **branchement**. `crate` est Rust ; le domaine parle de **projet**. Alignement du vocabulaire (voir [[glossary]], [[ADR-0013]]) :

| Avant (rustifié) | Après (domaine) |
|---|---|
| `ParsedFunction.match_arms` | `ParsedFunction.branch_arms` |
| `WarningPattern::LargeMatch` | `WarningPattern::LargeBranching` |
| `DetectionConfig.max_match_arms` | `DetectionConfig.max_branch_arms` |
| `crate_root` | `project_root` |
| `CodeReaderStub.rust_files` / `add_rust_file` | `source_files` / `add_source_file` |

### Le principe capturé

> **L'hexagone est désormais ~100 % agnostique du langage. La sémantique par-langage — résolution de modules/namespaces, extensions de fichiers, signatures d'I/O — vit *entièrement* dans l'adaptateur pilote.**

C'est l'extension à la résolution de dépendances et aux extensions de fichiers du précédent qu'[[ADR-0013]] avait établi pour l'I/O, et qu'[[ADR-0014]] avait tenu pour les noms qualifiés : *le schéma est de la syntaxe, il vit dans l'adaptateur*. L'invariant d'[[ADR-0001]] est **maintenu** : `hexagon/Cargo.toml` reste sans dépendance ; aucune logique `syn` / `mod:` / `use:` / `crate::` / `list_rust_files` ne subsiste dans `hexagon/`.

## Conséquences

- **(+)** **Débloque le multi-langage.** Un adaptateur C# (#33) ou TypeScript (#34) implémente `CodeParser` + `CodeReader` sans toucher l'hexagone — c'est le prérequis direct de ces tickets et de la direction tree-sitter de l'étude #30.
- **(+)** **OCP restauré.** Ajouter un langage = ajouter un adaptateur, jamais éditer le domaine.
- **(+)** **[[ADR-0001]] renforcé, pas seulement préservé** : l'hexagone est maintenant agnostique *par construction*, pas par discipline.
- **(=)** **Zéro changement de comportement** : sortie octet-équivalente, `&["rs"]` fourni par la racine de composition.
- **(−)** Une frontière de conversion de plus (`joules→…` non concerné ; ici `Vec<String>`→`Vec<PathBuf>`) : le coût est un `DependencyContext` construit par fichier (voir dette).

**Instruction pour l'auteur d'un futur adaptateur (.NET / TypeScript / tree-sitter) :** vous implémentez `CodeParser::resolve_dependencies` et `CodeReader::list_source_files`. Vous parcourez **votre** AST, appliquez **votre** sémantique de modules/namespaces en privé, et rendez des `PathBuf` résolus contre `ctx.available_files` + un filtrage sur l'ensemble d'extensions **de votre langage** (`&["cs"]`, `&["ts", "tsx"]`). **Vous ne touchez jamais `hexagon/`.** Si vous vous surprenez à vouloir y ajouter une règle `crate::`-comme, un jeton `"mod:"`, ou une extension figée, vous avez mal lu cet ADR.

## Dette connue, explicitement non traitée

Suivis relevés par les revues (Dev-B / QA), **non bloquants** :

- **La généralité multi-extension de `list_source_files` n'est exercée qu'avec `&["rs"]`.** Aucun test ne couvre un autre ensemble d'extensions tant qu'un deuxième adaptateur n'existe pas — le contrat est généralisé mais sa généralité restera non éprouvée jusqu'à #33/#34.
- **`resolve_dependencies` clone `DependencyContext` par fichier.** Coût mémoire proportionnel à `available_files` × nombre de fichiers. À traiter **uniquement si** le profilage le signale — pas d'optimisation prématurée.
