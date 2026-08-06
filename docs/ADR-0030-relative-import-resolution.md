# ADR-0030 — Graphe de dépendances inter-fichiers TS/JS : la stratégie en donnée, la résolution lexicale, et l'ordre des candidats en toutes lettres

> **Statut :** Appliqué
> **Décidé dans :** Issue #34 (US17 — T4.1 couture `DepsStrategy`, T4.2 précondition du graphe, T4.3 résolution des specifiers relatifs)
> **Liens :** [[ADR-0029]], [[ADR-0020]], [[ADR-0021]], [[ADR-0010]], [[ADR-0016]], [[ADR-0022]], [[ADR-0023]], [[architecture-overview]], [[dependency-graph-integrity]], [[typescript-javascript-analysis]], [[glossary]]

## Contexte

[[ADR-0029]] a livré l'analyse TypeScript/JavaScript de bout en bout, mais avec un axe explicitement en jachère : `cross_file_dependencies` restait `Unsupported`. La requête `deps_scm` du profil TS/JS était **vide**, par décision de mise en scène honnête (ruling A3 de T1) — la même que le C# avait connue avant US16 T5.

T4 lève cette jachère. L'axe passe `Unsupported` → `Degraded`, et le graphe porte enfin de vraies arêtes.

Trois tranches, trois problèmes distincts :

- **T4.1** — le C# résolvait ses dépendances par index de namespaces. Le TS/JS résout par chemin relatif. Deux stratégies incompatibles derrière un seul `resolve_dependencies`.
- **T4.2** — un correctif de plantage découvert en route, sans rapport avec la résolution elle-même, mais qui devait la précéder.
- **T4.3** — la résolution réelle.

## Décision AD-1 — la stratégie est une donnée sur le profil, jamais un `match` sur `Language`

`LanguageProfile` gagne un champ :

```rust
pub enum DepsStrategy {
    NamespaceIndex,
    RelativePath,
}
```

`resolve_dependencies` dispatche sur `self.profile.deps`, pas sur `self.language`. C'est la ligne de conduite d'[[ADR-0020]] tenue là où elle était le plus tentante à rompre : un `match self.language { CSharp => …, TypeScript | JavaScript => … }` aurait marché, et aurait rouvert la fermeture à l'extension à chaque nouveau langage.

Le corollaire est un renommage : `namespaces`/`usings` → `declared`/`referenced`, `file_usings` → `file_references`. Le vocabulaire C# décrivait une structure devenue commune aux deux stratégies. Le comportement C# est resté **octet pour octet identique** après T4.1 — c'était le critère d'acceptation de la tranche.

## Décision AD-2 — une pré-passe, un index, construit une fois

`resolvable_targets` est construit dans la même pré-passe que `namespace_declarers` et `file_references`, mis en cache derrière `deps_index_cache`, jamais reconstruit par appel. La pré-passe parse déjà chaque *autre* fichier du projet ; y ajouter un troisième ensemble ne coûte rien de plus qu'un `HashSet` à remplir.

## Décision AD-3 — la whitelist, c'est l'appartenance à l'ensemble scanné

Il n'y a pas de liste blanche de chemins autorisés à maintenir. **Les cibles admissibles SONT les fichiers scannés.** Un specifier qui résout hors de `resolvable_targets` ne produit pas d'arête, et ne produit pas d'erreur non plus.

Cette formulation a une conséquence contre-intuitive que la revue a explicitée : un specifier `./t.d.ts` **explicite** résout si `t.d.ts` est lui-même un fichier scanné. Ce n'est pas une contradiction avec « `.d.ts` n'est jamais deviné » — deviner et honorer un chemin nommé par la source sont deux actes différents. La source a nommé ce chemin ; l'appartenance est le seul test.

`.tsx` en revanche n'est jamais atteignable sous aucune forme : il n'entre pas dans `file_sources` en amont (#118).

## Décision AD-4 — normalisation purement lexicale, zéro accès disque

`normalize_lexically` manipule des `Component`, rien d'autre. Aucun appel `std::fs` : pas de `exists`, pas de `canonicalize`, pas de `read_link`. **Les liens symboliques ne sont jamais suivis.**

C'est une décision de sécurité autant que de déterminisme. Le texte d'un `import` est une entrée non fiable ; le laisser piloter un accès disque ouvrirait une surface de traversée et un TOCTOU, là où l'appartenance à un ensemble déjà constitué n'en ouvre aucune.

Un `..` qui remonterait au-delà de la racine (rien à dépiler, ou dernier composant non `Normal`) **abandonne le candidat** (`None`) plutôt que de sortir des bornes du chemin joint.

## Décision AD-9 — l'ordre des candidats, reproduit ici en toutes lettres

> **Pourquoi cette section existe.** T1 avait déjà arbitré cette question une fois. Le node de l'époque a enregistré *qu'elle avait été tranchée*, pas *ce qui avait été tranché* — et le contenu a été perdu, obligeant l'opérateur à re-arbitrer deux tranches plus tard. La leçon est consignée ailleurs ; ici on l'applique : l'ordre est copié intégralement, pas résumé, pas référencé.

Les 7 suffixes proposés comme candidats **devinés**, dans cet ordre exact :

```rust
const CANDIDATE_EXTENSIONS_WITHOUT_EXTENSION: [&str; 7] =
    ["ts", "mts", "cts", "js", "jsx", "mjs", "cjs"];
```

L'arbitrage opérateur (Q-A) a retenu la **liste complète des 16 candidats**, pas l'ensemble minimal.

### Cas 1 — le specifier porte déjà une extension analysable : **au plus 2 candidats**

1. le chemin exact
2. son jumeau source TypeScript, pour la famille `.js` uniquement

```rust
"js"  => Some("ts")
"mjs" => Some("mts")
"cjs" => Some("cts")
_     => None
```

C'est l'idiome NodeNext : une source TypeScript s'importe sous son nom **émis**. `.ts`/`.mts`/`.cts` n'ont pas de jumeau (ils sont déjà la source) ; `.jsx` non plus (c'est une extension d'exécution réelle, il n'y a rien d'autre à essayer).

### Cas 2 — le specifier n'a pas d'extension analysable : **jusqu'à 16 candidats**

D'abord les 7 directs, dans l'ordre de la constante :

| # | Candidat |
|---|---|
| 1 | `<path>.ts` |
| 2 | `<path>.mts` |
| 3 | `<path>.cts` |
| 4 | `<path>.js` |
| 5 | `<path>.jsx` |
| 6 | `<path>.mjs` |
| 7 | `<path>.cjs` |

puis les 7 mêmes sous un répertoire `index` :

| # | Candidat |
|---|---|
| 8 | `<path>/index.ts` |
| 9 | `<path>/index.mts` |
| 10 | `<path>/index.cts` |
| 11 | `<path>/index.js` |
| 12 | `<path>/index.jsx` |
| 13 | `<path>/index.mjs` |
| 14 | `<path>/index.cjs` |

### Correction de comptage — il y en a 14, pas 16

L'option d'arbitrage présentée à l'opérateur s'intitulait « liste complète **16** candidats », et le commentaire de production a repris ce nombre. **L'implémentation en produit 14** : 7 directs + 7 sous `index/`, tirés du même tableau de 7 suffixes.

Le 16 vient probablement de l'addition des deux branches **mutuellement exclusives** (2 pour un specifier déjà extensionné + 14 pour un specifier sans extension). Aucun specifier ne reçoit jamais 16 candidats.

**Ce qui a été arbitré reste honoré** : l'opérateur a choisi la liste *complète* contre la liste *minimale*, et c'est bien la complète qui est implémentée. Seule l'étiquette était fausse. Aucun comportement ne dépend du nombre — il n'apparaît que dans de la documentation. Le commentaire de production a été corrigé dans la même PR.

C'est exactement le genre d'écart que la Phase 5 est censée attraper : un nombre qu'on se transmet de proposition en commentaire sans jamais le recompter contre le code.

**Le premier membre présent dans `resolvable_targets` gagne.** L'ordre encode donc une priorité : source TypeScript avant JavaScript émis, fichier direct avant `index/`.

### La construction, pas seulement l'ordre

Les 7 directs sont construits en **concaténant** `.ext` au nom de fichier complet, jamais par `Path::with_extension` :

```rust
fn append_extension(path: &Path, ext: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(ext);
    PathBuf::from(name)
}
```

`Path::with_extension` **remplace** tout ce qui suit le dernier `.`. Sur `./app.module`, il produirait `app.ts` — ratant le vrai `app.module.ts` (la convention de nommage dominante d'Angular et NestJS) et, plus grave, risquant une **arête fausse** vers un `app.ts` sans rapport qui existerait par ailleurs.

C'est un mensonge de mesure au sens d'[[ADR-0010]] : une arête vers un fichier réel du projet que le code source n'a jamais désigné. [[AD-3]] ne protège pas ici, la cible étant bien dans l'ensemble scanné. Le défaut a été trouvé en revue croisée, pas par le gate de mutation.

`normalized.join("index")` n'est pas concerné — `"index"` ne contient pas de `.`, donc `with_extension` y ajoute correctement.

### Le chemin normalisé vide

Un specifier qui désigne explicitement un répertoire en remontant jusqu'à la racine (`import '../..'`, `'./'`, `'./.'`) laisse un chemin sans `file_name`. Les 7 candidats directs ne sont alors **pas proposés du tout** : `append_extension` produirait un dotfile nu `.ts` qui **gagnerait** sur le `index.ts` correct.

Le cas était inatteignable en production, mais seulement parce que `FileSystemCodeReader` construit son walker avec `.hidden(true)` — une protection **accidentelle**, pas une intention. Ne rien proposer laisse la branche `index/` comme seuls candidats, ce qui correspond à la sémantique de Node pour un specifier de répertoire.

## Décision AD-7 — une seule requête partagée par deux grammaires

`queries/ecmascript_deps.scm` est partagé par les grammaires TypeScript et JavaScript. Trois sites de capture :

```scheme
(import_statement source: (string) @import)
(export_statement source: (string) @import)
(call_expression
  function: (identifier) @_callee
  arguments: (arguments . (string) @import)
  (#eq? @_callee "require"))
```

L'ancre `.` force le littéral à être le **premier** argument : `require(cfg, './x')` ne matche pas. Un commentaire précédant l'argument (`require(/* c */ './x')`) échoue aussi à l'ancre et s'abstient — le `comment` de cette grammaire est un nœud nommé « extra » qui compte comme premier enfant nommé réel.

La forme héritée `import x = require('./y')` est **délibérément non capturée** : sa chaîne cible vit sur `import_require_clause`, nœud qui n'existe **que** dans la grammaire TypeScript. Le référencer ferait paniquer `Query::new` au moment où la requête tourne contre le JavaScript nu.

L'inclusion de `require('./x')` (CommonJS) dans T4.3 est un arbitrage opérateur (Q-B), pris **contre** la recommandation de l'architecte qui proposait de le reporter.

## Décision AD-8 — s'abstenir, ne jamais deviner, ne jamais échouer

`string_literal_text` lit le littéral depuis son enfant `string_fragment` — jamais le texte brut du nœud `string`, qui porte encore les guillemets — et s'abstient sur tout ce qu'il ne peut pas lire sûrement :

- zéro ou plusieurs `string_fragment`
- un fragment unique qui ne couvre pas **toute** la zone entre guillemets
- un octet de contrôle dans le texte extrait

Cette discipline est héritée de `classify_call` ([[ADR-0016]], [[ADR-0022]]) : l'abstention est un troisième état de première classe, pas un échec dégradé.

### Le bras `escape_sequence`, supprimé après double preuve

Une première version portait un bras explicite `"escape_sequence" => return None`. Il a été supprimé après démonstration qu'il était **subsumé** par la vérification de couverture :

- **structurellement** — un `escape_sequence` est un *frère* du fragment, jamais imbriqué dedans ; il occupe donc ≥2 octets strictement entre les guillemets, et scinde ou raccourcit toujours le fragment. Il n'existe pas de cinquième position possible.
- **empiriquement** — 40 000 itérations de fuzz : 26 072 chaînes portant un `escape_sequence`, 42 094 littéraux réels, ~278 formes ciblées, plus une réimplémentation verbatim de la fonction d'avant-suppression comparée littéral par littéral. **Zéro** cas où l'ancienne rejetait et la nouvelle accepte.

> **Nuance consignée volontairement.** La prémisse structurelle telle qu'énoncée (« chaque guillemet fait exactement 1 octet ») est **fausse** : 194 guillemets fermants de largeur nulle existent, produits par la récupération d'erreur du parser. Mais la *direction* de la falsification a été mesurée — un guillemet fermant de largeur nulle rend la condition de couverture impossible à satisfaire, donc il **resserre** la garde. La conclusion tient ; le raisonnement qui y mène est plus fragile qu'annoncé. C'est écrit ici pour qu'un futur lecteur ne re-dérive pas la prémisse comme acquise.

### L'octet de contrôle n'est pas de la défense en profondeur

Un commentaire du code affirmait que le balayage d'octets était « defense in depth, not diagnosed ». **C'est mesurablement faux**, et corrigé : le balayage est la **seule** garde pour `0x01`, `0x09`, `0x0B`, `0x0C` et `0x7F`. Seul `0x00` produit un nœud `ERROR` frère détecté par la couverture. Un futur lecteur supprimant ce balayage comme « redondant » rouvrirait un trou réel.

## Décision AD-6 — la chaîne `Degraded` nomme chaque angle mort

> literal relative specifiers only (import, export-from, require); computed, dynamic and escaped specifiers, bare and tsconfig-aliased imports, and the legacy `import x = require()` form produce no edge; a shadowed `require` identifier is still followed (syntactic only); type-only imports produce a full edge like any other import; .tsx targets are not analyzed

L'invariant AD-6 est que **tout angle mort découvert atterrit dans cette chaîne**, y compris ceux découverts en revue. C'est la raison pour laquelle un import type-only y est nommé : le comportement actuel (arête pleine) est peut-être discutable — c'est #133 — mais il doit être **dit**, pas subi.

La chaîne est pinnée par une assertion golden-verbatim : une mise à jour du comportement sans mise à jour de la chaîne casse le test.

## T4.2 — la précondition du graphe que son appelant violait

Découvert hors énoncé, livré **avant** la résolution sur arbitrage opérateur (Q-C).

`per_file` (fichiers analysés avec succès) et `file_sources` (fichiers lus) divergent dès qu'un fichier est lu puis jugé non mesurable alors que quelque chose en dépend. `FileConsumptionGraph::build` retourne alors une erreur sur l'extrémité orpheline — et `codeimpact analyze` sortait en erreur au lieu de produire un rapport. Un seul `.js` généré de plus d'1 Mio suffisait.

**Le graphe n'a pas été touché** : sa précondition est correcte. C'est l'appelant qui la violait. `drop_dangling_edges` filtre en amont, aux deux sites projet, toute arête dont `from` **ou** `to` est absent de `per_file`.

Ce n'est pas un silence de mesure : l'extrémité tombée est déjà nommée dans `unmeasurable_files` ([[ADR-0010]]).

En retirant un `exit 1` qui ne protégeait que par accident, T4.2 a élargi la portée d'un défaut préexistant de `--strict` — consigné en #128 plutôt que corrigé en douce.

## Conséquences

**Acquis.** `cross_file_dependencies` est `Degraded` et non plus `Unsupported` pour TS/JS. Le graphe porte de vraies arêtes. La couture `DepsStrategy` accueillera un quatrième langage sans `match` supplémentaire. Aucun accès disque n'est piloté par du texte d'import.

**Dette assumée, tracée.**

| # | Sujet |
|---|---|
| #130 | Le gate de mutation rend un faux `pass` par asymétrie de périmètre baseline/mutants. **Le plus grave** : il neutralise le gate bloquant sur tout le dépôt |
| #132 | La chaîne `Degraded` n'est rendue sur **aucune** surface opérateur — 1 395 arêtes affichées, mise en garde invisible |
| #133 | Un import type-only doit-il produire une arête ? `cross_file_dependencies` mesure-t-il le couplage ou l'exécution ? |
| #131 | Ré-extraction redondante (×3 mesuré : 10,04 s → 30,16 s) et `Query::new` par fichier (~4,1 ms/fichier fixe) |
| #134 | Specifier de répertoire bare résolvant vers un fichier frère au lieu de son `index` ; `Cargo.lock` gitignoré alors qu'on livre un binaire |
| #128 | `--strict` sort en 0 alors qu'une partie du projet n'a pas été mesurée |

**T4.4 — la dernière tranche, livrée.** `resolvable_targets` est borné par les `sourceRoots` configurés, via le prédicat `under_any_root` existant — le miroir exact de ce que `namespace_declarers` fait déjà côté C#. Une ligne :

```rust
.filter(|path| under_any_root(path, source_roots))
```

L'asymétrie est délibérée et pinnée par un test : `file_references` reste peuplé **inconditionnellement**. `sourceRoots` borne qui peut être **cible** d'une arête, jamais qui peut **demander** une résolution — un fichier situé hors des roots résout toujours ses propres imports. Un import qui pointe hors des roots ne produit ni arête ni erreur (AD-8). `sourceRoots` vide signifie « non configuré » et laisse tout le projet résolvable, comportement inchangé.

Le `@wip` de S4 est tombé avec cette tranche, et US17 est close.

Le code final inverse `.filter()` et `.map()` — filtrer avant de cloner, plutôt que cloner tous les chemins pour en jeter une partie.

## Décision AD-10 — les deux chaînes `Degraded` nomment l'angle mort `sourceRoots`

> `targets outside the configured sourceRoots produce no edge;`

Ajouté à la chaîne TS/JS **et** à la chaîne C#.

**L'arbitrage, parce que la conclusion seule ne suffit pas.** Le développeur a d'abord jugé qu'aucune modification n'était nécessaire, avec un argument de cohérence réel : le gate `under_any_root` existe côté C# depuis US16 T5 et la chaîne C# ne l'a jamais mentionné ; traiter les deux différemment serait incohérent. Security a jugé l'inverse. L'orchestrateur a tranché pour Security, parce que sa synthèse **résout** l'objection de cohérence au lieu de l'ignorer : on amende les deux chaînes, donc aucune asymétrie documentaire n'est créée.

**La raison de fond**, celle qu'il faut retenir si la question se repose : le scoping `sourceRoots` est le seul angle mort de cette liste qu'un lecteur **ne peut pas déduire du langage**. Tous les autres — specifiers calculés, imports bare, alias tsconfig, `.tsx` — sont des propriétés syntaxiques de TS/JS, qu'un développeur connaissant l'écosystème peut anticiper. Celui-ci est une propriété de **la configuration de l'opérateur**, invisible depuis le code analysé.

**Alternative écartée** : exposer le scoping `sourceRoots` une seule fois au niveau du rapport plutôt que dupliqué par langage. Plus propre à long terme, mais hors périmètre T4.4 — et bloqué de fait par #132, puisque aucune de ces chaînes n'atteint aujourd'hui une surface opérateur.

## Décision AD-11 — le gate de mutation ne peut rien prouver sur cette tranche, et c'est structurel

**Verdict final de T4.4 : `empty`. 3 mutants générés, 3 `unviable`, 0 exécuté.** Dérogation accordée par l'orchestrateur sur preuves compensatoires. Ce n'est pas un contournement de confort : c'est une limite d'outil, démontrée.

### Pourquoi aucun mutant utile n'existe

`cargo-mutants` en mode `--in-diff` ne sait produire, pour cette forme de diff, que des remplacements de **fonction entière** par `Default::default()`. Le changement de comportement tient en une ligne — une closure d'itérateur appelant un prédicat réutilisé et **intouché** — et `cargo-mutants` ne mute pas les closures de cette forme. Les trois mutants générés visent `build_deps_index` et les deux constructeurs (entrés dans le diff parce que les chaînes `Degraded` y vivent) ; aucun de ces types n'implémente `Default` en production.

### La tentative de correction, et pourquoi elle était pire

Une première passe a ajouté `#[derive(Default)]` sur `DepsIndex` pour rendre le mutant viable, obtenant un `verdict: "pass"`. Deux lanes l'ont rejetée pour des raisons **indépendantes** :

- **Le `pass` était plus malhonnête que le `empty`.** Un mutant de fonction entière tué prouve qu'*un* test exerce `build_deps_index` — vrai depuis US16 T5 — et ne dit rien de la ligne `.filter(...)`. Pire, ce `pass` alimente la relaxation ADR-2026-14, qui le lit comme « quelque chose a réellement été tué » et retire la vérification manuelle. On aurait fabriqué un vert qui désarme le contrôle suivant.
- **Le derive nu retirait un garde-fou de compilation.** `Default` se propage à travers `Arc<DepsIndex>`, puis le tuple `DepsIndexCacheEntry`, puis `Option<DepsIndexCacheEntry>` — si bien qu'un futur `cache.take().unwrap_or_default()` **compile**, et rendrait un index silencieusement vide : zéro cible, zéro declarer, **zéro arête C# et TS/JS**, sans aucune erreur puisque AD-8 impose l'abstention. Avant ce commit, ce code ne compilait pas.

Le code retenu est `#[cfg_attr(test, derive(Default))]` — le garde-fou est restauré, et le gate retombe à `empty`.

### L'impossibilité, démontrée

`.cargo/mutants.toml` porte `test_workspace = true`, donc chaque mutant lance `cargo test --workspace`. Ce build compile `codeimpact_secondaries` en **deux unités distinctes** :

1. son binaire de tests unitaires, **avec** `--cfg test` — le `cfg_attr` s'y active ;
2. une bibliothèque ordinaire (`--crate-type lib`), **sans** `--cfg test`, que les autres crates de test du workspace lient comme dépendance.

Le mutant patche le **fichier source**, donc les deux unités voient le corps muté, et la seconde échoue en `E0277`.

**Aucune formulation de l'attribut sur `DepsIndex` ne peut satisfaire simultanément** « garde-fou actif dans tout build non-test » et « ce mutant compile dans l'unité plain-lib sous `test_workspace = true` ». Les deux exigences sont mutuellement exclusives. Le même `E0277` qui *prouve* que le garde-fou est réel est celui qui tue la viabilité du mutant.

### Les preuves compensatoires qui fondent la dérogation

1. Le commit RED épingle l'assertion S4 **avant** l'existence du filtre — 100 lignes de test, zéro ligne de production.
2. L'orchestrateur a retiré lui-même la ligne `.filter(...)` et observé le rouge sur l'assertion S4 précise, les deux autres tests restant verts à juste titre.
3. Dev-B a raisonné **six mutants à la main** : chacun des trois tests en tue au moins un qu'aucun autre ne tue. Le plus subtil — gater le *demandeur* au lieu de la *cible* — n'est tué que par le test d'asymétrie.
4. Security a produit un différentiel e2e sur **13 orthographes de `sourceRoots`** contre le vrai binaire, plus une preuve 18×17 que la tranche ne peut que rétrécir l'ensemble.

**Leçon à retenir** : un `empty` bloquant peut être un verdict *légitime*, pas un échec à contourner. Le contrat développeur ne le prévoit pas, ce qui pousse mécaniquement à modifier la production pour nourrir l'outil — c'est le trou de process, de la famille de #130.

## Ce que Security a mesuré sur le confinement, et qui reste ouvert

Le comportement de `Path::starts_with` est **composant par composant**, pas textuel : `srcfoo/x.ts` n'est pas admis par un root `src`. Les trois orthographes naturelles (`frontend`, `./frontend`, `frontend/`) fonctionnent en production, où `resolve_source_roots` canonicalise.

Mais `sourceRoots` n'a **aucune validation**, seule clé de configuration de chemins dans ce cas — `include`/`exclude` ont reçu un validateur complet en #34 T2, la tranche précédente. Mesuré sur le vrai binaire :

| Valeur | Effet |
|---|---|
| `["/"]`, `["."]`, `[""]` | gate **entièrement neutralisée** — et comme `under_any_root` fait un `any()`, une seule entrée `"/"` annule tous les autres roots |
| `[".."]`, `["/etc"]` | **blackout total silencieux**, 0 arête, aucun diagnostic |
| `["../proj/frontend"]` | 0 arête, alors que c'est le **même dossier** que `["frontend"]` qui en donne 1 — le `join` ne normalise pas les `..` |
| 95 323 roots (sous le plafond de 1 Mio) | scan de 500 fichiers : 4,21 s → 31,78 s |

Atteignable depuis une PR hostile en CI, puisque `.codeimpact.json` est auto-découvert dans le répertoire analysé. **Arbitrage produit en attente de l'opérateur** : appliquer le validateur déjà écrit à côté ferait d'une configuration acceptée aujourd'hui une erreur dure.

Aggravant : une mauvaise configuration du confinement ne produit **aucun signal**. Sur `--format json` et `--format html`, le rapport est byte-identique gate active ou non ; seul `--format console` affiche `Dépendances totales: 0`, sans cause.
