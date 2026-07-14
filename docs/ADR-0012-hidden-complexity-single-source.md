# ADR-0012 — `hidden_complexity` : mesurée à l'atome, jamais dérivée d'agrégats

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-14
> **Decided in:** Issues #46, #49
> **Links:** [[architecture-overview]], [[ADR-0010]], [[ADR-0007]], [[glossary]], [[json-report-schema]], [[console-report-enriched]]

## Contexte

Le même `AnalysisReport`, rendu par deux writers, annonçait deux nombres pour la même grandeur : **hidden = 63** en JSON, **hidden = 124** en HTML. `direct` et `transitive` étaient identiques. Seul « hidden » divergeait.

En parallèle (#49), la tuile de résumé HTML annonçait `Warnings: 7 (4 critical)` là où le rapport contenait 6 warnings (3 critiques) et 1 I/O-en-boucle.

Les deux tickets partagent une racine : **les agrégats de résumé étaient recalculés à côté du view-model qu'ils résument**, au lieu d'en être dérivés.

L'instruction du ticket #46 était : *« décider laquelle des deux sémantiques est la bonne »*. **La question était mal posée. Aucune des deux ne l'était.**

## Ce que l'enquête a trouvé

### Premier défaut — deux unités qui ne se parlent pas

- La complexité **directe d'un fichier** valait `1 + Σ decision_points` — un `+1` de base par **fichier**.
- La complexité **transitive d'un fichier** valait `Σ_f transitive(f)`, bâtie sur les `decision_points` **bruts** — **sans aucun `+1`**.

`T` n'héritait jamais du `+1` que `C` s'octroyait. Pour tout fichier dont les fonctions ne s'appellent pas entre elles, `T = C − 1`, donc **`C > T`**. Mesuré : **61 fichiers sur 67**, déficit de **exactement 1** à chaque fois. Et `61 = 124 − 63` : l'écart entier des deux formules.

`hidden_complexity()` soustrayait donc **un `C` en unité-fichier d'un `T` en unité-fonction**, et son `saturating_sub` **avalait silencieusement les 61 déficits négatifs**.

### Second défaut — la métrique elle-même était fausse

La première version de cette ADR affirmait que l'invariant `transitive ≥ direct` tenait « par construction » au niveau fonction. **L'audit de sécurité a prouvé que c'était faux.**

`compute_transitive` calculait une **somme sur les chemins** du graphe d'appel. La valeur mémoïsée d'un appelé était **ré-additionnée à chaque arête entrante**, donc un simple diamant suffisait :

```
f2 → {a, b}     a → f1     b → f1
⇒ transitive(f2) = direct(f2) + t(a) + t(b) = direct(f2) + 2·t(f1) + …
```

Empilé k fois : `t(f_k) ≈ 2^k`, **avec des appelés tous distincts**. Le débordement de `u32` survenait en une trentaine de fonctions — et sans `overflow-checks` en release, Rust **wrappe en silence**. L'invariant était donc réellement violable, et le `saturating_sub` transformait la violation en `0` plausible.

**Ce n'était pas une métrique de complexité. C'était un compteur de chemins** — un nombre déjà faux, sur de vrais dépôts, bien avant tout débordement.

### Troisième défaut — un garde-fou absent de l'artefact livré

L'invariant était protégé par un `debug_assert!`, **compilé hors du binaire en release**. L'audit l'a prouvé en exécutant le test d'invariant du projet en `--release` : il ne panique pas. **Un contrôle qui n'existe pas en release n'est pas un contrôle.**

## Décision

### 1. `transitive` mesure le coût de COMPRÉHENSION

La complexité du **sous-graphe atteignable**, chaque fonction distincte comptée **une seule fois** :

```
hidden(f)       = Σ_{g ∈ reachable(f) \ {f}}  direct(g)
transitive(f)   = direct(f) + hidden(f)
hidden(fichier) = Σ_{f ∈ fichier} hidden(f)
hidden(projet)  = Σ_{fichiers} hidden(fichier)
```

L'outil produit un rapport pour un humain qui va **lire** le code. Pour comprendre `f`, on lit l'**ensemble** des fonctions qu'elle atteint. Lire `g` deux fois n'est pas deux fois le travail : un appel dans une boucle, `g` appelée depuis deux branches, un diamant — le lecteur lit `g` **une fois**.

La lecture *coût d'exécution* (compter chaque site d'appel) serait défendable pour un **profileur**, pas pour une métrique statique — et elle était de toute façon fausse telle qu'implémentée : elle ignore les boucles, les branches non prises, la récursion. Elle ne mesurait ni l'exécution ni la compréhension.

La complexité cyclomatique (McCabe), dont `direct` dérive, est une propriété **du texte du code**, pas de ses traces d'exécution. **Étendre transitivement une métrique de texte doit rester une métrique de texte.**

**Corollaire décisif :** `transitive(f)` est borné par la somme des complexités directes du fichier. Le débordement de `u32` exigerait ~4·10⁹ points de décision **dans un seul fichier** — **structurellement inatteignable, pas simplement improbable.** Les cycles se terminent naturellement sur le `visited` set. La déduplication des appelés devient sans objet : l'ensemble atteignable est un `HashSet`.

### 2. La règle générale — corollaire d'ADR-0010

> **Une quantité se mesure à l'atome où ses termes sont commensurables, puis se somme. Elle ne se re-dérive JAMAIS en soustrayant deux totaux.**
>
> Un `saturating_sub` sur une différence d'agrégats est une **odeur de mensonge** : il ne protège pas d'un dépassement, il **convertit une violation d'invariant en zéro plausible**. [[ADR-0010]] interdisait d'afficher `0` pour « pas su mesurer » ; ADR-0012 interdit d'afficher `0` pour « mon invariant est cassé ».

### 3. L'état illégal est rendu inconstructible, pas surveillé

`FunctionDetail` ne stocke plus `transitive`. Il stocke `direct` et `hidden` (**champs privés**, construction par `FunctionDetail::new`), et **dérive** `transitive() = direct + hidden`.

`transitive ≥ direct` devient une **vérité arithmétique**, en debug comme en release. Il ne reste ni `debug_assert!`, ni `assert!`, ni `saturating_sub` : **il n'y a plus rien à garder**. Un futur adaptateur FFI (.NET / Node.js / Java) ne peut pas fournir un `transitive` incohérent — il ne peut pas fournir de `transitive` du tout.

C'est le mécanisme d'[[ADR-0010]] : **l'impossibilité structurelle plutôt que la discipline**. Et c'est *moins* de code qu'avant, pas plus.

### 4. Sur « un outil de reporting ne panique pas en prod »

Le principe est maintenu, mais il ne justifie plus rien ici : il servait à couvrir un `saturating_sub` silencieux, ce qui était indéfendable. Un outil de reporting ne panique pas — **et il ne fabrique pas non plus un nombre**.

Quand une quantité n'est pas mesurable, la sortie honnête est `Measurement::Unmeasurable` ([[ADR-0010]], lane réactive). Dans la lane **proactive**, après cette correction, **il n'existe aucune condition de non-mesurabilité** : le parseur a le texte, l'ensemble atteignable est fini, la somme est bornée. Étendre `Measurement<T>` à cette lane câblerait une branche morte à travers les trois writers et le schéma [[ADR-0007]] : de la cérémonie d'honnêteté, sans objet à protéger.

**On ne saturera pas, on ne paniquera pas, on ne déclarera pas « non mesurable » — on calcule une quantité qui ne peut pas déborder.**

### 5. Source unique de vérité : `ProjectMetrics`

`ProjectMetrics` (hexagone, zéro dépendance) porte **tous** les nombres de niveau-rapport, calculés **une seule fois** par `aggregated_metrics()` : complexités (directe / transitive / cachée), profondeur, cycles, **warnings**, **critiques**, **I/O-en-boucle**, **hotspots**, impacts économique et écologique.

**Les trois writers (console, JSON, HTML) ne font que RENDRE `ProjectMetrics`. Aucun n'a le droit de recalculer un agrégat.**

Conséquence directe : le faux `CodeMetrics` que `handle_project_json` fabriquait à partir de totaux est **supprimé**. C'était un objet mensonger par nature — les métriques d'aucun fichier, sans détail de fonctions et **sans aucun warning** : voilà pourquoi le JSON n'a jamais exposé le défaut #49.

### 6. Un warning et une I/O-en-boucle sont deux classes de domaine distinctes

La tuile `Warnings` comptait `warnings + io_in_loops`, et son décompte `critical` ajoutait **la totalité** des I/O sans filtre de sévérité. Or `IoInLoopWarning` **n'a pas de sévérité** : lui appliquer la catégorie « critique » est un non-sens de langage ubiquitaire, pas une erreur d'arithmétique.

- La tuile `Warnings` compte **les warnings** (et ses critiques).
- L'I/O-en-boucle obtient **sa propre tuile**.

### 7. Le test qui manquait

Les writers étaient testés **en isolation** — leur désaccord était donc invisible par construction. Un test **inter-writers** (`cross_writer_consistency_test.rs`) rend un **unique** `FileConsumptionGraph` vers les trois writers et exige des nombres identiques.

Sa fixture est choisie pour **discriminer les formules candidates** : une assertion de simple égalité `JSON == HTML` aurait pu être satisfaite en rendant les deux writers faux **de la même façon**. Le test épingle donc **la valeur ET l'égalité**.

## Conséquences

- **(+)** `hidden` a une définition unique, écrite, et les trois rendus l'affichent.
- **(+)** Le clamp qui masquait la violation d'invariant a disparu, et l'invariant est désormais porté par le **type**, pas par une assertion absente du binaire livré.
- **(+)** Le débordement d'entier n'est pas *géré*, il est **dissous** : la quantité calculée est bornée par construction.
- **(+)** Une tuile de résumé ne peut plus diverger du détail qu'elle résume : elle n'a plus le droit de le recalculer.
- **(−)** **Les chiffres publiés changent, et ils baissent.** `transitive` et `hidden` étaient **gonflés exponentiellement** par la somme sur les chemins. C'est une correction, pas une régression : les anciens nombres n'étaient pas justes, ils étaient faux.

## Dette connue, explicitement non traitée

- **Le parser est aveugle aux méthodes `impl`** — 37 fichiers sur 67 parsent zéro fonction. Toutes les complexités publiées sont des **sous-comptages massifs**. Orthogonal à cette ADR (le correctif ci-dessus est juste quel que soit le nombre de fonctions vues). → **Issue #50 (P0)**.
- **`[profile.release] overflow-checks` n'est pas activé.** Tout dépassement résiduel **ailleurs** (agrégation projet, lane réactive, impacts, futurs adaptateurs FFI) wrappe encore en silence. La lane proactive est désormais non débordable **par construction**, pas par un garde-fou. → **Issue #51**.
- **`compute_depth` explore le graphe sans mémoïsation** — exploration exponentielle (2^k) sur un diamant. Troisième membre de la famille, après `compute_transitive` (cette ADR) et `detect_cycles` (#47, remplacé par Tarjan). → **Issue #52**.
- `complexity_level()` applique à un **fichier** entier des seuils qui ont la forme de seuils **par fonction**. Erreur de catégorie préexistante, non aggravée ici.
