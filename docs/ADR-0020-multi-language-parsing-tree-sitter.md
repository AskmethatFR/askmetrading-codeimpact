# ADR-0020 — Parsing multi-langage via tree-sitter : un adaptateur générique, dispatch par extension, isolation in-process du parsing natif

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-20
> **Decided in:** Issue #33 (US16 — US14-T2, support C#/.NET)
> **Links:** [[architecture-overview]], [[ADR-0018]], [[ADR-0001]], [[ADR-0015]], [[ADR-0006]], [[glossary]]

## Contexte

[[ADR-0018]] (US14-T1) a rendu l'hexagone ~100 % agnostique du langage : la sémantique par-langage vit **entièrement** dans l'adaptateur pilote. C'était le **prérequis** — aucune capacité utilisateur nouvelle. Ce cycle (#33, US14-T2) est le **premier retour sur cet investissement** : `codeimpact analyze <csharp-dir>` produit désormais complexité cyclomatique, liste de fonctions et impact économique/écologique **pour du C#**, là où il ne produisait **rien** auparavant. Le Rust continue d'être analysé par `SynCodeParser` — **deux moteurs derrière un seul port**, la preuve que le port d'[[ADR-0018]] est correct.

L'étude d'architecture #30 avait recommandé la direction **tree-sitter** pour le multi-langage. Ce cycle la concrétise, et tranche quatre questions contextuelles (soumises à l'humain, approuvées) :

- **Q1** — La couche de capacité/dégradation : jusqu'où la modéliser dès T2 ?
- **Q2** — L'isolation du parsing natif tree-sitter : étendre la sonde canari d'[[ADR-0015]], ou garder in-process ? *(co-instruite avec Security, confirmée par un spike)*
- Le comptage cyclomatique C# : coller à `syn` ou diverger ?
- La forme de l'adaptateur : un adaptateur par langage, ou un adaptateur générique paramétré ?

## Décision

### D1 — Un **seul** adaptateur générique `TreeSitterCodeParser`, paramétré par un `LanguageProfile`

Plutôt qu'un adaptateur par langage, **un** adaptateur secondaire `TreeSitterCodeParser` est paramétré par un **`LanguageProfile`** qui porte les trois seules choses qui changent d'un langage à l'autre :

| Donnée du `LanguageProfile` | Rôle |
|---|---|
| La **grammaire** tree-sitter (ex. `tree-sitter-c-sharp`) | Produit l'AST |
| La **requête `.scm`** (ex. `csharp.scm`) | Une requête S-expression qui capture fonctions + points de décision |
| La **table de signatures d'I/O** | La sémantique d'I/O-en-boucle propre au langage ([[ADR-0016]]) |

> **Le comportement par-langage est de la *donnée*, pas du *code*.** Ajouter un langage = ajouter un `LanguageProfile` (grammaire + `.scm` + table I/O) et l'enregistrer, **sans écrire un nouvel adaptateur**. C'est exactement pourquoi T6 (TypeScript, #34) sera **bon marché** : la mécanique d'extraction est écrite une fois. C'est OCP d'[[ADR-0018]] tenu au niveau de l'adaptateur.

Le C# passe par `tree-sitter` + `tree-sitter-c-sharp` derrière la **feature Cargo `lang-csharp`** (la dépendance native reste optionnelle, l'invariant zéro-dép de l'**hexagone** d'[[ADR-0001]] est intact — tree-sitter vit en `secondaries/`). Les métriques sont extraites par la requête `.scm` puis pliées dans le `ParsedFunction` **neutre** de l'hexagone par un **post-processeur générique itératif** de conteneur-par-plage (range-containment), commun à tous les langages.

### D2 — `Language` (VO) + `ParserRegistry` (domain service) : dispatch par extension

Deux nouveaux types **zéro-dép** de l'hexagone ([[ADR-0001]] tenu) :

- **`Language`** — VO enum `Rust` / `CSharp`, avec `from_extension` / `extensions` / `display_name`. Le concept « quel langage est ce fichier » devient un type de première classe du domaine, pas une chaîne.
- **`ParserRegistry`** — domain service qui mappe `Language -> Box<dyn CodeParser>`, en dispatchant **par extension de fichier**. Peuplé à la **racine de composition** (`run_analysis` / `main`), qui y branche `SynCodeParser` pour `.rs` et `TreeSitterCodeParser<csharp>` pour `.cs`.

**Dispatch par extension ; fichier inconnu = sauté, non fatal.** Un fichier dont l'extension n'est enregistrée pour aucun langage est **ignoré silencieusement mais proprement** — jamais parsé-comme-du-Rust, jamais un panic. C'est la transposition de la règle « le type affirme, le nom s'abstient » ([[ADR-0016]]) au niveau du fichier : en l'absence de langage connu, on s'abstient.

### D3 — La couture capacité/dégradation, **minimale en T2** (Q1)

Le port `CodeParser` gagne deux méthodes : **`language()`** et **`capabilities()`**. `capabilities()` rend un VO **`LanguageCapabilities`** — une carte `métrique -> MetricSupport`, où **`MetricSupport`** est l'enum `Supported` / `Degraded` / `Unsupported`.

**Décision Q1 (humain) : en T2, tout est `Supported`.** La couture existe (le type, la méthode de port, le point d'appel) pour que T3 puisse **rendre** la dégradation par-métrique par-langage — mais **le rendu** d'un `Degraded` / `Unsupported` (griser une tuile, annoter « non supporté pour ce langage ») **est le travail de T3**, pas de ce cycle. On modélise la couture au plus juste : assez pour que T3 ne rouvre pas le port, pas plus. YAGNI/Lean sur le rendu, pas sur le contrat.

### D4 — Comptage cyclomatique : **parité `syn`**, plus le ternaire C# (décision humaine)

Le comptage C# est **épinglé sur celui de `syn`** : `cyclomatique = 1 + Σ points de décision`. Un même motif structurel doit donner le même nombre quel que soit le langage, sinon les seuils d'[[ADR-0017]] et les comparaisons de rapport perdent leur sens.

**Une extension délibérée, signée par l'humain : le ternaire C# (`?:`) compte +1 point de décision.** Rust n'a **pas** de ternaire (c'est un `if`/`else` expression, déjà compté), donc il n'y avait rien à aligner : ce n'est pas une divergence par rapport à `syn`, c'est une **couverture d'une construction que `syn` n'a jamais eu à classer**. Le principe reste : chaque branchement réel compte une fois, dans tous les langages.

### D5 — Isolation du parsing natif tree-sitter : **gardes in-process, PAS la sonde canari** (Q2 — l'amendement d'[[ADR-0015]])

Les grammaires tree-sitter sont du **C qui parse du C# non fiable** — une **nouvelle surface FFI native**. La question : faut-il étendre la **sonde sous-processus canari** d'[[ADR-0015]] à tree-sitter ? **Décision (humaine, co-instruite avec Security, CONFIRMÉE par un spike) : non — des gardes in-process.**

**Rationale, entièrement vérifié au spike :**

- Le parseur C de tree-sitter est **table-driven (pile sur le tas)** : il **n'abandonne pas le processus** (pas de `native abort`) **même à 500 000 niveaux d'imbrication** — contrairement à la descente récursive de `syn`, dont le débordement de pile *process-level* est **exactement la raison d'être** de la sonde canari d'[[ADR-0015]].
- Le `Drop` d'un `Tree` brut à **50 000** de profondeur **n'abandonne pas** non plus (vérifié).
- **La menace résiduelle est donc une DoS *wall-clock*, pas une atteinte à la sûreté-mémoire.** Le remède à une DoS temporelle est un **budget de temps**, pas une frontière de processus. Étendre la sonde canari ici serait payer le fork+exec par fichier ([[ADR-0015]]) contre une menace qui n'existe pas pour ce moteur.

**Les gardes in-process retenus :**

| Garde | Ce qu'il ferme |
|---|---|
| `catch_unwind` **par fichier** | Un panic d'un fichier ne tue pas le scan (fidèle à [[ADR-0010]] / [[ADR-0015]]) |
| **`PARSE_QUERY_BUDGET` — deadline partagée de 5 s** couvrant le parse **ET** le curseur de requête | La DoS wall-clock, sur **tout** le pipeline natif |
| Plafond **`source_guard` 1 Mo** | L'amplification par la taille d'entrée ([[ADR-0006]], garde-fou primaire) |
| Post-processeur **entièrement itératif (zéro récursion)** | Un débordement de pile *côté nous* sur AST profond |
| **`MAX_QUADRATIC_CAPTURES_PER_FUNCTION` (2000)** sur les helpers O(k²) | L'explosion quadratique par fonction à captures plates |
| **Balayage à pile d'intervalles O(n log n)** pour l'appartenance capture→fonction (remplace un scan linéaire O(fonctions × captures)) | La DoS quadratique par nombre de fonctions |

> **La leçon de sécurité de ce cycle : le budget doit couvrir le pipeline COMPLET (parse + requête + post-traitement), pas seulement le parsing.** Security a trouvé **deux** variantes de DoS distinctes au fil des tours de revue, toutes deux fermées : (1) **frères plats sous le plafond par-fonction** (l'explosion vit dans les helpers de post-traitement, pas dans le parse), et (2) **beaucoup de fonctions sous le plafond de taille** (le coût est O(fonctions × captures), invisible d'un budget qui ne chronométrerait que le parse). D'où le `PARSE_QUERY_BUDGET` **partagé** englobant parse + curseur, le plafond **par-fonction**, et le passage du scan d'appartenance à **O(n log n)**.

## Conséquences

- **(+)** **C# est analysable.** `analyze <csharp-dir>` rend complexité + fonctions + impact éco/écolo ; le port d'[[ADR-0018]] est **prouvé** par un deuxième moteur réel.
- **(+)** **T6/TypeScript sera bon marché.** Un langage = un `LanguageProfile` (grammaire + `.scm` + table I/O), pas un adaptateur. La mécanique d'extraction est écrite une fois (D1).
- **(+)** **Comparabilité inter-langages préservée** : même formule cyclomatique, seuils d'[[ADR-0017]] toujours valides d'un langage à l'autre (D4).
- **(+)** **Coût d'isolation ajusté à la menace** : pas de fork+exec par fichier pour tree-sitter (moteur table-driven), là où `syn` (descente récursive) le justifiait. **Deux moteurs, deux modèles de menace, deux remèdes** — voir l'amendement porté dans [[ADR-0015]].
- **(=)** **[[ADR-0001]] tenu** : les nouveaux types de l'hexagone (`Language`, `ParserRegistry`, `LanguageCapabilities`, `MetricSupport`) sont zéro-dép ; tree-sitter vit en `secondaries/` derrière la feature `lang-csharp`.
- **(−)** **Dépendance native optionnelle** (`tree-sitter` + grammaire C) compilée sous `lang-csharp` — surface FFI native, atténuée par D5.
- **(−)** **`resolve_dependencies` rend vide pour le C# en T2** : le graphe de dépendances namespace→fichier C# est explicitement **T5**, pas ce cycle. Comportement honnête ([[ADR-0010]]) — une dépendance non résolue est absente, jamais fausse.

## Dette connue, explicitement non traitée

- **Précondition de grammaire sur le balayage d'appartenance (D5).** Le balayage à pile d'intervalles s'appuie sur un test `open.last()` (la capture englobante la plus interne) qui **suppose qu'aucune capture englobante ne partage l'exact `start_byte` d'un `@function` niché**. C'est **vrai pour `csharp.scm`** ; **toute future `.scm` réutilisant ce helper doit le vérifier** avant de s'y fier — sinon l'appartenance capture→fonction peut être mal attribuée. Précondition documentée, non enforce par le type.
- **Graphe de dépendances C# (namespace→fichier)** : différé en **T5** (D4 des conséquences). `resolve_dependencies` rend vide pour le C# jusque-là.
- **Rendu de la dégradation par-métrique** (`Degraded` / `Unsupported`) : différé en **T3** (D3). La couture existe, le rendu non.
