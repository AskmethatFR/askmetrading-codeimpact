# ADR-0031 — Le cap quadratique par fonction ne borne pas la dimension « nombre de fonctions » — toute recherche par capture doit être mesurée adversarialement

> **Statut :** Appliqué
> **Décidé dans :** Issue #123 (issue de #34 US17 T1, finding F3 de la lane Security)
> **Liens :** [[ADR-0006]], [[ADR-0020]], [[ADR-0029]], [[ADR-0030]], [[architecture-overview]], [[typescript-javascript-analysis]]

## Contexte

Deux fois de suite sur `tree_sitter_code_parser.rs`, un scan linéaire à l'intérieur d'une boucle par capture a passé la revue de l'auteur et n'a été trouvé par la lane Security qu'**en mesurant, pas en lisant** :

1. `innermost_function_index` — finding HIGH à la première barrière de revue de #34 (US16 T2), corrigé en passant `owning_function_indices` à un balayage amorti.
2. `call_callee_name` — le même piège réintroduit **une fonction plus loin**, finding F3 à la seconde barrière de #34. C'est l'objet de #123.

Les deux fois, `MAX_QUADRATIC_CAPTURES_PER_FUNCTION` existait et donnait l'illusion d'une borne.

Le code fautif :

```rust
function_nodes.iter().any(|f| f.id() == callee.id())
```

Scan linéaire complet, appelé **par site d'appel**, lui-même dans une boucle sur toutes les fonctions du fichier. Coût O(fonctions × appels).

## Décision 31.1 — Ce que le cap borne, et ce qu'il ne borne pas

`MAX_QUADRATIC_CAPTURES_PER_FUNCTION = 2000` borne `loops_of[i]`, `depth_nodes_of[i]`, `switch_sections_of[i]` et `calls_of[i]` — c'est-à-dire des collections **par fonction**.

Il ne borne **pas** `function_nodes.len()`.

Toute recherche dont la collection parcourue est indexée par le nombre de fonctions est donc non bornée par construction, et doit être en O(1) ou O(log n). Le correctif de #123 est un `HashSet<usize>` des `Node::id()` des fonctions capturées, construit une fois par fichier — appartenance en O(1) au lieu d'un scan.

## Décision 31.2 — La mesure adverse est obligatoire sur ce post-processeur

Sur `tree_sitter_code_parser.rs`, toute nouvelle recherche par capture doit être **mesurée adversarialement** — fixture à N croissant, ratio par doublement — avant d'être déclarée bornée.

La revue par lecture est établie comme insuffisante sur ce fichier : deux échecs sur deux. C'est une règle non outillée, et son efficacité repose sur la lane Security ; un gate automatisé serait un chantier distinct.

## Décision 31.3 — Le cap reste après #123

Il n'est pas devenu vestigial. Quatre consommateurs réellement quadratiques subsistent, qu'aucun `HashSet` ne touche :

| Consommateur | Coût |
|---|---|
| `any_contained(&loops_of[i])` | O(loops²) |
| `max_nesting_depth(&depth_nodes_of[i])` | O(depth_nodes²) |
| `max_switch_section_count(&switch_sections_of[i])` | O(sections × switches distincts) |
| le test `in_loop` | O(calls × loops) **par fonction** |

La clause `calls_of[i]` en particulier — celle qu'on pourrait croire libérée par le correctif — reste porteuse : c'est elle qui borne le produit `calls × loops` du dernier. Le bloc de cap est **diff nul** dans #123, délibérément.

## Décision 31.4 — Le seuil du test de non-régression repose sur une mesure, pas sur une projection

Le test de non-régression exploite le fait que `PARSE_QUERY_BUDGET` (5 s) convertit la lenteur en un observable **discret** : `unmeasurable` / `SourceTooComplex` contre « mesuré ». Il n'assertit donc aucun temps — pas de `assert!(elapsed < X)`, non flaky par construction.

La taille de fixture a été arbitrée deux fois :

| | Fondement | Taille | GREEN mesuré | % du budget |
|---|---|---|---|---|
| Arbitrage initial (GATE 1.5) | **projection** de l'Architecte (~1,5 s attendu) | 45 000 fonctions | 3,84 s à chaud / 4,60 s à froid | 77 % / 92 % |
| Arbitrage final | **mesure** de la lane Security | 30 000 fonctions | 2,62 s | ~52 % |

La projection était fausse d'un facteur ~2,5. La décision finale retient 30 000 : le RED reste franc (6,24 s → `SourceTooComplex` avant le correctif) et l'exposition au flake CI est divisée par deux.

L'échange est asymétrique dans le bon sens — le test tournera en CI pendant des années dans la direction GREEN, jamais dans la direction RED.

> Enseignement de méthode : un chiffre qui fonde un arbitrage humain doit être **mesuré, pas extrapolé**. L'Architecte extrapolait ici depuis un proxy « 0 appel par fonction » ; seule la lane Security a exécuté le vrai chemin.

## Note — les deux occurrences de la même forme qui ne sont pas des défauts

Vérifié le 2026-08-11, par lecture (Architecte) puis par mesure adverse indépendante (Security) : deux occurrences de la *forme* « scan linéaire dans une boucle » subsistent dans le fichier, **bornées sur chacune de leurs deux dimensions**, donc non défectueuses :

- `max_switch_section_count` (`per_switch.iter_mut().find(...)`) — O(sections × switches), les deux ≤ 2000 par le cap.
- la descente de pile de `owning_function_indices` (`open.iter().rev().find(...)`) — bornée par la profondeur d'imbrication, pas par le nombre de fonctions.

Aucune troisième occurrence dangereuse n'existe. Nommées ici pour que le prochain qui scanne ce fichier ne refasse pas le travail.

## Conséquences

**(+)** La dimension non bornée est nommée, plus seulement le cap — c'est l'angle mort qui a produit deux régressions successives.

**(+)** Le correctif supprime le vecteur CWE-407 **structurellement** (un changement de type rend le scan linéaire inexprimable) plutôt qu'en ajoutant une borne de plus. Sur un fichier qui a déjà réintroduit le piège une fois, remplacer la forme vaut mieux que la borner.

**(+)** La ligne accept/reject se déplace vers l'extérieur — un fichier de 30 000 fonctions était **refusé**, il est désormais **mesuré** — et cela au **coût CPU pire-cas strictement inférieur** : l'ancien refus n'était pas une sortie précoce bon marché, c'était la deadline qui tombait *après* que le travail quadratique ait été largement fait. La garde de 1 Mo redevient la contrainte liante pour cette famille d'entrées, ce qui est sa place correcte : une borne en octets est déterministe, une borne en temps dépend de la machine.

**(−)** La 31.2 est une règle non outillée.

**(−)** Le gate mutation présente une asymétrie de scope (baseline `--package`, mutants `--workspace`) relevée deux fois — #115 puis #123. Sur #123 les quatre kills proviennent tous de crates que la baseline n'a jamais exécutées ; le verdict tient par recoupement entre runs de mutants, pas par construction de l'outil. Suivi en #138.
