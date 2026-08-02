# ADR-0029 — Support TypeScript / JavaScript : le verdict de falsification d'ADR-0020, chiffré

> **Statut :** Appliqué
> **Décidé dans :** Issue #34 (US17 — T1, analyse TS/JS de bout en bout)
> **Liens :** [[ADR-0020]], [[ADR-0018]], [[ADR-0016]], [[ADR-0017]], [[ADR-0010]], [[ADR-0006]], [[ADR-0008]], [[architecture-overview]], [[glossary]]

## Contexte

[[ADR-0020]] a posé une promesse explicite : *« le Nième langage coûte une grammaire + un fichier de requêtes + une table I/O — de la donnée, pas du code. Pas de nouvel adaptateur, pas de nouveau port, pas de changement dans l'hexagone. »*

Le C# (US16) l'a validée pour un deuxième langage, mais l'abstraction avait été taillée **pour lui**. Le TS/JS est le premier langage à l'éprouver de l'extérieur. Le ticket #34 exigeait d'ailleurs un stop-et-escalade si l'hexagone devait changer.

Il a dû changer. Cet ADR enregistre **ce que ça a réellement coûté**, chiffré, plutôt que de déclarer la promesse tenue.

> **Correction de référence.** Le ticket #34 attribuait la promesse à ADR-0009. C'est une erreur héritée de la numérotation de l'étude #30 : ici [[ADR-0009]] est la CI/supply-chain. La promesse tree-sitter vit dans [[ADR-0020]] (+ [[ADR-0018]]).

## Verdict de falsification

**La promesse est substantiellement vraie, pas littéralement vraie.**

Ce qui est bien de la donnée, comme annoncé :

| Poste | Coût |
|---|---|
| Crates de grammaire | 2 (`tree-sitter-typescript`, `tree-sitter-javascript`) |
| Feature Cargo | 1 (`lang-typescript`) |
| Fichier de requêtes | 1, **partagé** par les deux grammaires (`ecmascript.scm`) |
| Table I/O | 1 (`io_signatures/typescript.rs`) |
| Constructeurs | 2, partageant un builder privé |
| Lignes de composition (`main.rs`) | 2 `.register(...)` |
| Données dans l'hexagone | 2 variantes d'enum + 3 bras de `match` |

Ce que la promesse n'avait **pas** prévu, et qui est du code partagé généralisé :

1. **Les genres de nœuds de branchement.** Le dispatch `"branch.arm"` connaissait `switch_section` (C#) ; il a fallu lui apprendre `switch_case` / `switch_default`.
2. **Le paramètre « marqueurs suspects » du classifieur I/O.** `SUSPICIOUS_RECEIVER_MARKERS` était une constante partagée peuplée de marqueurs C# (`_context.`, `DbSet`) sans aucun sens en JS. Le classifieur a dû devenir paramétrable et les marqueurs migrer dans le profil.
3. **La dégradation par métrique.** `capabilities()` renvoyait trois chaînes C# écrites en dur, sans aiguillage par langage.
4. **Une précondition de grammaire qu'ADR-0020 avait lui-même listée en dette** — voir D8, c'est la plus instructive.

**Trois généralisations de code partagé et un correctif de précondition** : voilà le delta entre la promesse et le réel. Ce n'est pas cher, et c'est exactement ce que le ticket voulait acheter — apprendre où l'abstraction fuit pendant que le coût de l'apprendre est encore faible.

## Décisions

### D1 — Ajouter des variantes à `Language` est un changement de donnée, pas de structure

`hexagon/src/analysis/language.rs` porte un enum **fermé** plus la table d'extensions. Une nouvelle langue impose d'y toucher : le critère « aucun changement dans l'hexagone » de #34 est **factuellement réfuté**.

L'opérateur a accepté le changement, et cet ADR enregistre pourquoi. Vérifié par grep, pas supposé : les trois `match` exhaustifs sur `Language` en code de production vivent **dans `language.rs` lui-même**. Aucun port ne bouge, aucune méthode de trait n'est ajoutée, aucun algorithme de l'hexagone n'est touché. `Language` est le **registre des langages supportés** — du vocabulaire ubiquitaire, au même titre qu'une énumération de statuts métier. Le faire vivre dans l'hexagone est correct ; le fait qu'il faille l'étendre pour ajouter une langue n'est pas une fuite d'abstraction, c'est la définition d'un registre.

[[ADR-0018]] affirmait « aucune extension figée dans `hexagon/` ». C'est faux depuis [[ADR-0020]], qui a introduit `language.rs`. **Les deux sont amendés par le présent ADR** : la formulation correcte est « aucune *sémantique* de langage dans l'hexagone ; le registre des langages y vit, la syntaxe reste dans l'adaptateur ».

### D2 — Une seule requête `.scm` partagée par les deux grammaires

La grammaire TypeScript est un sur-ensemble de la JavaScript : les genres capturés existent dans les deux. Deux fichiers jumeaux auraient garanti la duplication et la dérive pour un gain nul.

L'arbitre n'est pas une opinion : `Query::new(grammar, …).expect(…)` **panique** si la requête ne compile pas contre sa grammaire. Le test `ecmascript_query_compiles_against_both_grammars` est donc un garde-fou réel, pas du confort — il force la scission le jour où une divergence de genres apparaît, et la révèle au build plutôt que chez l'utilisateur.

### D3 — La dégradation par métrique devient de la donnée sur `LanguageProfile`

`capabilities()` lit `self.profile.degradations` au lieu de chaînes en dur. C'est l'esprit d'[[ADR-0020]] D1 appliqué à un endroit qui l'avait manqué. Le profil C# porte les trois chaînes existantes **mot pour mot** — comportement constant, prouvé par une suite C# verte sans modification d'assertion.

Valeurs pour TS/JS ([[ADR-0010]], [[ADR-0021]]) : complexité cyclomatique, impact économique et écologique `Supported` ; `io_in_loops` et `call_graph` `Degraded` avec leur raison ; `cross_file_dependencies` **`Unsupported`** — la résolution inter-fichiers n'existe pas encore (T4), et annoncer `Degraded` avant que le code existe serait un mensonge de mesure.

### D4 — `?.` n'est délibérément pas compté

Le ticket le demandait. Refusé en v1, et l'omission est écrite en commentaire dans `ecmascript.scm` pour qu'elle se lise comme une décision.

Raison : `csharp.scm` ne le compte pas non plus. Le compter en TS seulement briserait l'invariant de comparabilité inter-langages d'[[ADR-0020]] D4 — un même motif structurel doit donner le même nombre quel que soit le langage — et donc le sens des seuils d'[[ADR-0017]]. Le geste correct est de l'ajouter **dans les deux langages en même temps** : issue #117.

### D5 — Une cascade d'étiquettes `case` vides compte pour un point de décision, dans tous les langages

C'est la décision la plus large de ce cycle, et elle est née d'une revue **erronée**.

La revue croisée avait affirmé que `case 1: case 2: doX();` comptait 2 en C# (étiquettes consécutives partageant une `switch_section`) contre 3 en JS. L'orchestrateur a instruit « aligner JS sur C# ». Le développeur a **vérifié la prémisse au lieu de l'appliquer** : dump des `grammar.js` et des s-expressions réelles. En `tree-sitter-c-sharp 0.23.5`, la règle est `switch_section: seq(choice(seq('case', …), 'default'), ':', repeat($.statement))` — **une seule étiquette par section**. Les deux langages comptaient 3.

L'argument décisif est venu du parseur de référence du projet : `syn_code_parser.rs` compte `expr_match.arms.len()`, et en Rust `1 | 2 => body` est **un seul `syn::Arm`**. **Rust collapsait déjà.** L'invariant de comparabilité d'[[ADR-0020]] D4 était donc **déjà violé en silence depuis US16** : Rust disait 1, C# et JS disaient 2.

Correctif : un prédicat `switch_label_has_body` **agnostique du langage** — « quelque chose suit-il le `:` de l'étiquette ? » — appliqué aux deux grammaires. Le choix du token `:` plutôt qu'un nom de champ est délibéré : le `repeat($.statement)` du C# n'a pas de field name, un test par champ aurait échoué silencieusement.

**Conséquence assumée : la métrique C# change.** Sur une cascade de trois étiquettes, la complexité directe passe de 4 à 2. Les seuils d'[[ADR-0017]] étaient calibrés sur les anciens chiffres. Le changement a été accepté parce qu'il **rétablit** la comparabilité au lieu de la casser, et parce que le cas cascade n'était couvert par **aucun test C#** — c'était du comportement latent, jamais épinglé. Aucune autre assertion C# n'a bougé (vérifié ligne à ligne). Livré ici et non dans un ticket séparé : livrer deux moitiés du même bug dans deux PR laisserait la comparabilité cassée entre les deux.

### D6 — La sortie console est neutralisée ; la donnée ne l'est jamais

**Le rapport EST le produit.** L'outil ingère des sources que l'opérateur ne contrôle pas, et l'opérateur lit le rapport pour décider. Un nom capable de forger ce qu'il lit défait la raison d'être de l'outil.

JavaScript est le **premier langage supporté dont les noms de méthode peuvent être des littéraux de chaîne arbitraires** — un identifiant C# ou Rust ne peut pas contenir d'octet de contrôle. La classe de menace n'est donc pas héritée : elle est née avec US17.

- **Neutralisation dans le writer console uniquement.** Le JSON conserve le nom réel (`serde_json` échappe les Cc) — un outil tiers qui consomme le rapport doit voir la vraie valeur. Le HTML est protégé par `json_island_escape` + un renderer `textContent`-only, vérifié par tentative de breakout réelle (`</script><img src=x onerror=…>` : `<` échappé en `<`, aucun breakout).
- **Toute chaîne dérivée du parseur, à tout site console.** Le premier correctif n'a couvert que le nom de fonction et a laissé quatre sinks ouverts (`ComplexityWarning.function`/`.message`, `IoInLoopWarning.function`/`.io_call`) — dont `io_call`, vecteur **indépendant** atteignable avec un nom de fonction bénin via un membre calculé derrière un préfixe légitime (`fs.promises["<ESC>…"]`).
- **Les chaînes dérivées d'un chemin aussi** : sur Unix un nom de fichier peut porter un `0x1b`. Le modèle de menace est tranché : un chemin est une entrée hostile dès lors que l'opérateur analyse un dépôt qu'il n'a pas écrit — le cas d'usage annoncé. Complète [[ADR-0006]].
- **Échappement injectif.** `\` est échappé, et la forme est `\u{…}` délimitée. Sans ça, un nom contenant le *texte* `\x1b` et un nom contenant le *vrai octet* rendaient identiques : l'opérateur ne pouvait plus savoir ce que la source contenait vraiment.
- **Classe = Cc + Cf + U+2028/U+2029.** Pas seulement le sous-ensemble bidi : ZWSP, BOM, SHY font rendre deux fonctions distinctes sur une ligne visuellement identique. La classe ne ferme **pas** l'usurpation par homoglyphe (un `а` cyrillique) — aucune classe de caractères ne le peut. C'est de la défense en profondeur, la borne est assumée.

### D7 — `.tsx` est hors périmètre, et c'est là qu'est la vraie réfutation

Le port `CodeParser::parse(&self, source: &str)` **ne reçoit pas le chemin**. Or `tree-sitter-typescript` expose deux grammaires mutuellement incompatibles (`LANGUAGE_TYPESCRIPT` et `LANGUAGE_TSX` divergent sur `<T>() => …`, fonction fléchée générique contre JSX). Une instance de parser porte un profil, donc une grammaire : sans le chemin, elle ne peut pas choisir.

Supporter `.tsx` exigerait donc **d'élargir la signature d'un port**. Ça, ce serait la réfutation littérale d'[[ADR-0020]] — un changement de structure, pas de donnée. Isolé dans l'issue #118 précisément pour que la mesure reste propre.

En attendant, `from_extension("tsx")` rend `None` : les `.tsx` ne sont jamais listés en mode projet, et sont un `UnsupportedLanguage` fatal en mode fichier unique. Une absence honnête ([[ADR-0010]]).

### D8 — La précondition de grammaire d'ADR-0020 était réelle, et elle s'est déclenchée

[[ADR-0020]] avait consigné en dette une précondition avec pour instruction de la **vérifier pour toute nouvelle `.scm`** : `owning_function_indices` droppait silencieusement une capture dont le nœud englobant partage exactement le `start_byte` d'une `@function` contenue.

En C# c'est impossible — tout constructeur englobant démarre sur un mot-clé, un opérande gauche ou un callee, jamais sur une déclaration. **En JavaScript, une expression de fonction peut commencer une expression** : `!function(){ doIo(); }()` fait coïncider le `start_byte` du `call_expression` et celui de sa propre `function_expression`. L'appel disparaissait du rapport.

Correctif : un balayage descendant de la pile de fonctions ouvertes au lieu du seul sommet. L'équivalence C# a été **prouvée**, pas supposée, par deux revues indépendantes : par laminarité de l'AST, diverger exige `F.end < capture.end`, ce qui force `capture.start == F.start` — configuration impossible en C#. Le coût reste O(1) amorti (≤ 2 itérations) et a été mesuré adverse.

**C'est le retour sur investissement le plus net de ce cycle** : une dette écrite dans un ADR huit semaines plus tôt a évité un défaut silencieux dans le langage suivant. La documentation a fait son travail.

## Conséquences

- Un projet TS/JS s'analyse de bout en bout : complexité, boucles, branchements, appels, I/O en boucle, impact — console, JSON et HTML.
- **La métrique C# change sur les cascades de `case`** (D5). À annoncer si des seuils sont calibrés en production.
- Le graphe de dépendances inter-fichiers TS/JS **n'existe pas encore** (T4) ; `cross_file_dependencies` rend `Unsupported`, honnêtement.
- Les exclusions par défaut (`node_modules`, `dist`, `*.min.js`) **ne sont pas livrées** (T2) : un `analyze` sur un projet TS réel avale encore les dépendances vendorées.
- La feature `lang-typescript` entre dans `default`, donc les deux nouvelles crates sont couvertes par la porte `cargo-deny` de la CI, qui résout le graphe par défaut.
- Le sanitizer console s'applique aux trois langages ; la sortie Rust et C# pour des identifiants ordinaires est inchangée, octet pour octet.

## Dettes ouvertes

| # | Sujet |
|---|---|
| #117 | `?.` comme point de décision, en C# **et** TS/JS simultanément (D4) |
| #118 | `.tsx` — exige d'élargir le port `CodeParser::parse` (D7), la vraie réfutation |
| #119 | Calibrer la table I/O TS/JS sur corpus réel — elle n'est aujourd'hui adossée à aucune mesure, contrairement au C# |
| #120 | `await import(...)` — c'est un `import_expression`, pas un `call_expression` : capture dédiée requise |
| #121 | Le gate mutation rend un faux `verdict: "empty"` quand `cargo-nextest` est installé |
| #122 | La CI ne construit que le jeu de features par défaut — régressions de combinaison invisibles |
| #123 | `call_callee_name` fait un scan linéaire par appel — O(fonctions × appels), borné par le budget de 5 s mais réel |

## Note de méthode

Trois observations de ce cycle méritent d'être réutilisées, parce qu'aucune n'est spécifique à TypeScript.

**Le gate mutation a rendu `survived=0` sur un diff contenant deux défauts réels.** Ce n'est pas une faiblesse du gate : il note la discrimination du code **atteint**. Aucun test n'exerçait les sites d'impression des warnings ni la forme parenthésée de l'IIFE. Zéro survivant signifie « ce que tes tests touchent, ils le tuent » — jamais « tes tests touchent tout ».

**Deux fois, le même motif de scan linéaire dans une boucle par capture a passé la revue de l'auteur** et a été trouvé en **mesurant** plutôt qu'en lisant. Sur ce post-processeur, toute nouvelle recherche par capture doit être mesurée adverse avant d'être déclarée bornée.

**Vérifier la prémisse d'une consigne vaut mieux que l'appliquer.** D5 n'existe que parce qu'un développeur a refusé d'obéir à une instruction fondée sur un fait faux, et a dumpé la grammaire. L'instruction aurait figé un bug commun aux deux langages et laissé la divergence de Rust invisible.
