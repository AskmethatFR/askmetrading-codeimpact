# ADR-0032 — La chaîne `Degraded` atteint enfin l'opérateur — le compte **et** l'énumération, six axes, quatre sinks assainis par un helper unique

> **Statut :** Appliqué
> **Décidé dans :** Issue #132 (issue de #34 US17 T4.3, dette tracée dans [[ADR-0030]]) / PR #144
> **Liens :** [[ADR-0030]], [[ADR-0026]], [[ADR-0021]], [[ADR-0010]], [[ADR-0029]], [[ADR-0028]], [[ADR-0007]], [[ADR-0006]], [[ADR-0023]], [[architecture-overview]], [[console-report-enriched]], [[json-report-schema]], [[dependency-graph-integrity]], [[typescript-javascript-analysis]], [[glossary]]

## Contexte

#34 T4.3 a livré un graphe de dépendances produisant **1 395 arêtes sur 1 889 fichiers**, sous l'invariant AD-6 d'[[ADR-0030]] :

> *« la chaîne `Degraded` nomme chaque angle mort »* — écrit sur le motif explicite que **l'opérateur s'appuie sur ce texte pour décider s'il fait confiance au graphe**.

Cette chaîne n'atteignait **aucune surface opérateur**. Ni la console, ni le JSON. AD-6 était une convention interne, pinnée par un test golden, invisible de bout en bout — c'est la ligne `#132` de la table de dette d'[[ADR-0030]].

Pire, et c'est ce qui fait de #132 un défaut et non une omission : `json_report_writer.rs` **codait en dur** `call_graph: "supported"` sur l'agrégat projet. Sur tout projet TS/JS, dans le **cas nominal** — pas dans un cas dégradé exotique — le JSON affirmait **l'inverse de la vérité**. Un `0` confiant qu'[[ADR-0010]] proscrit, mais sur l'axe *capacité* plutôt que sur l'axe *mesure* : le contrat machine déclarait « graphe d'appels pleinement supporté » là où l'adaptateur déclarait explicitement une résolution par nom aux arêtes ambiguës abandonnées.

**La cause commune.** `AggregateMetricSupport` ne foldait que **quatre axes** (`cyclomatic_complexity`, `io_in_loops`, `economic_impact`, `ecological_impact`) — restriction posée par [[ADR-0026]] au titre du YAGNI de #89 S1 : *« pas de tuile de stat, donc pas de cas d'usage appelant »*. Sans fold, le writer JSON n'avait rien à lire pour `call_graph`, d'où le littéral fabriqué ; et `cross_file_dependencies` n'avait même pas de champ.

## L'option retenue, et les deux écartées

Le ticket offrait trois options. **Retenue (approuvée par l'humain, GATE 1.5) : l'hybride — option 1 pour la cause, option 2 pour la surface.**

| # | Option | Verdict |
|---|---|---|
| 1 | Folder les deux axes **et** les rendre partout, y compris deux nouvelles tuiles de stat HTML | Retenue **pour la cause** (folder les deux axes), écartée **pour la surface** : elle exigeait d'inventer deux tuiles de stat qui n'existent pas — un chantier UX complet, hors énoncé |
| 2 | Rendre uniquement là où le graphe est **affiché** | Retenue **pour la surface** : la mise en garde accompagne le nombre qu'elle qualifie, là où l'opérateur le lit |
| 3 | Statu quo documenté | **Écartée** : elle aurait fait d'AD-6 une convention interne sans **aucun** effet observable — exactement le défaut que #132 existe pour corriger, requalifié en fonctionnalité |

L'hybride sépare correctement les deux moitiés du problème : le fold est la **correction du modèle** (le VO doit connaître les six axes, sinon le writer fabrique), le rendu est une **question de surface** (où le nombre est-il montré ?). Traiter les deux avec la même option aurait soit sur-livré (option 1), soit rien livré (option 3).

## Décision AD-1 — la raison agrégée porte le compte **et** l'énumération, uniformément sur les six axes

```
partial: M/N files measured this metric; <raison A>; <raison B>
```

Séparateur `; ` (approuvé par l'humain, Q3). Ancre : `hexagon/src/analysis/language_capabilities.rs:255-276` (`AxisTally::resolve`).

**Pourquoi le compte seul ne suffisait pas.** Folder les axes tels quels aurait satisfait la **lettre** des critères d'acceptation — un texte de dégradation apparaît bien sur la surface opérateur — en violant leur **intention** : l'opérateur aurait appris *combien* de fichiers ont été mesurés, **jamais *quoi*** le graphe ne sait pas voir. Or c'est précisément l'énumération (« specifiers calculés, imports bare, alias tsconfig, `.tsx` non analysés… ») qui fonde la décision de faire confiance ou non aux 1 395 arêtes. Le compte répond « quelle couverture ? », l'énumération répond « quel angle mort ? » — l'opérateur a besoin des deux, et AD-6 d'[[ADR-0030]] exige la seconde.

**Amendement partiel d'[[ADR-0026]].** La règle d'origine disait la raison *« un compte de couverture […] jamais une concaténation des raisons individuelles par fichier »*. Elle est **partiellement révisée** : le compte reste (il n'a jamais été le problème), l'énumération s'y ajoute. La motivation d'origine — ne pas produire un mur de texte par fichier — est préservée par AD-2 (dédoublonnage) et validée par AD-3 (mesure de la borne réelle).

**Compatibilité de préfixe.** La chaîne commence toujours par `partial: M/N files measured this metric`, donc toute assertion existante de la forme `starts_with("degraded: partial:")` reste verte sans modification. Le bras « sans raison » (mélange `Supported`/`Unsupported` sans aucun `Degraded` par fichier) n'a rien à énumérer et reste **byte-identique** à la forme pré-#132.

## Décision AD-2 — les raisons sont dédoublonnées et triées lexicographiquement (`BTreeSet<String>`), jamais en ordre de rencontre

Ancre : `language_capabilities.rs:234-243` (champ `reasons: BTreeSet<String>` sur `AxisTally`).

Ce n'est pas une préférence esthétique, c'est une contrainte de reproductibilité :

- `FileConsumptionGraph::per_file_metrics` est un `HashMap` (`hexagon/src/analysis/file_consumption_graph.rs:40`) ;
- le fold itère `.values()` (`file_consumption_graph.rs:277`) ;
- Rust **randomise l'ordre d'itération d'un `HashMap` par processus** (SipHash, graine aléatoire).

Un `join` en ordre de rencontre produirait donc une chaîne **différente d'un run à l'autre sur des données identiques** : goldens instables, diffs JSON non reproductibles entre deux exécutions CI du même commit. Une sortie non reproductible n'est pas une mesure honnête ([[ADR-0010]]) — elle rend l'outil impossible à utiliser comme référence. Le `BTreeSet` règle le dédoublonnage et l'ordre d'un seul coup.

## Décision AD-3 — pas de cap, pas de troncature, et cette fois c'est **mesuré** et non argumenté

La question « faut-il borner la longueur de la chaîne composée ? » se tranche sur la borne réelle, pas sur l'intuition « ça grandit avec le corpus ».

**Le nombre de raisons distinctes par axe est borné par le nombre de blocs `CapabilityDegradations` expédiés (deux : C# et TS/JS), pas par la taille du corpus.** La lane Security l'a vérifié empiriquement sur le vrai binaire :

| Fichiers analysés | Longueur de la ligne composée | RSS crête |
|---|---|---|
| 4 | 638 caractères | — |
| 1 200 | 641 caractères | — |
| 3 600 | 641 caractères | 38 Mo |

Le delta de **3 caractères** est entièrement l'élargissement du compteur (`4/4` → `3600/3600`) : **l'énumération elle-même est constante en octets**.

**Cause racine confirmée** : `capabilities()` clone `self.profile.degradations`, construit **une fois par parseur** ; la seule donnée variant à l'exécution (`extra_prefixes`) alimente `io_table`, **jamais** `degradations`.

Tronquer aurait donc réintroduit exactement le résumé qu'AD-6 interdit — pour résoudre un problème qui n'existe pas.

## Décision AD-4 — la restriction à quatre axes est levée ; l'agrégat en folde six

Ancre : `language_capabilities.rs:120-135` (docstring), champs `call_graph` / `cross_file_dependencies` sur `AggregateMetricSupport`, accesseurs et bras `None` du fold (`:148`+).

La justification de #89 (« pas de tuile ⇒ pas de cas d'usage appelant ») **était juste à l'époque et ne l'est plus**. T4.3 livre 1 395 arêtes et un graphe d'appels fusionnant les nœuds anonymes, tous deux **affichés**. Les cas d'usage appelants existent désormais :

- la ligne console `Dépendances totales` (`secondaries/src/gateways/report_writers/console_report_writer.rs:445`) pour `cross_file_dependencies` ;
- la ligne console `Complexité cachée totale` (`console_report_writer.rs:473`) pour `call_graph` — le seul nombre agrégé **entièrement** dérivé du graphe d'appels, là où la complexité transitive inclut aussi la complexité directe, fiable quelle que soit la résolution du graphe (arbitrage humain Q2) ;
- les champs JSON `metric_support.call_graph` et `metric_support.cross_file_dependencies` (`json_report_writer.rs:164` et `:168`).

Le YAGNI de #89 n'est donc pas désavoué : il a été **correctement daté**. C'est la forme saine de la déférence — on ne folde pas un axe sans consommateur, on le folde le jour où le consommateur arrive.

## Décision AD-5 — un `"supported"` fabriqué est une violation d'[[ADR-0010]] dans le cas **nominal**, pas une omission

Le hardcode `call_graph: "supported"` n'était pas une mise en garde manquante : il **affirmait l'inverse de la vérité** sur tout projet TS/JS, en fonctionnement normal. C'est une catégorie de défaut plus grave qu'un silence.

**Le motif à proscrire, en toutes lettres :**

> Un adaptateur qui ne sait pas lire un signal **propage l'absence** ; il ne fabrique jamais une valeur plausible.

**Le risque de récidive est structurel, et il faut le dire.** La barrière de revue a trouvé un **second** littéral `"supported"` non épinglé — `json_report_writer.rs:147`, le bras `None` par-fichier — que le test de mutation était **structurellement incapable** d'attraper :

- `MetricSupportDto` ne dérive pas `Default`, donc son unique mutant possible est `unviable` ;
- `cargo-mutants` **ne mute pas les champs d'un littéral de structure**.

Il est désormais assuré par un test. C'est un **quatrième faux-vert** de la famille recensée par [[ADR-0028]], d'une nature différente des trois qui y sont consignés : ceux-là venaient de la **recette d'exécution** (rapport périmé, asymétrie de périmètre baseline/mutants, contention), celui-ci vient de la **forme du code muté** — aucun réglage d'outil ne le ferme. Mais la forme de ce DTO — un `struct` de six `String` construit par deux constructeurs séparés, dont l'un remplit des littéraux — reste un **angle mort permanent du gate de mutation** : tout champ ajouté à `MetricSupportDto` devra être épinglé à la main, le gate ne le signalera pas. C'est consigné ici comme un danger permanent de cette forme, pas comme une anecdote de cycle. Même famille de raisonnement qu'AD-11 d'[[ADR-0030]] : quand le gate ne peut rien prouver, on le dit et on fournit la preuve compensatoire.

## Décision AD-6 — la note dégradée est assainie à **chaque** sink console, et c'est maintenant structurel

Ancre : `console_report_writer.rs:29-37` (`fn degraded_note`).

Les notes `[dégradé: …]` **contournaient `sanitize_console_text`** — la défense D6 d'[[ADR-0029]], qui existe parce que l'outil ingère des arbres de sources que l'opérateur ne contrôle pas et que **le rapport EST le produit**.

**Security l'a démontré avec un harnais réel** : une raison hostile matérialise une ligne forgée `Dépendances totales: 999 [tout est mesuré]`, avec des ESC bruts et un RLO Trojan-Source atteignant le terminal.

Deux nuances d'honnêteté à ne pas gommer :

- **Non exploitable aujourd'hui** : toute raison expédiée est un littéral de compilation.
- **Préexistant** : le sink par-fichier, non touché par cette tranche, échouait à l'identique. Cette tranche n'a pas créé la faille — elle l'a **élargie de 2 à 4 sinks**.

Les quatre sinks passent désormais par l'unique helper `degraded_note`, qui assainit. La barrière est nécessaire parce que rien n'empêchait **structurellement** une raison future d'interpoler de l'entrée analysée : `MetricSupport::Degraded(String)` accepte n'importe quelle chaîne, et les constructeurs `with_*` sont `pub`. Le helper *est* cette barrière — le contrat de type ne la donne pas.

**Effet de bord instructif, à retenir pour la méthode.** Extraire le helper a fait passer `console_report_writer.rs` de **3 à 5 mutants** : du code inline dans une méthode `-> ()` est inatteignable pour `cargo-mutants`, une fonction extraite ne l'est pas. La propreté a **élargi le filet de sécurité** — argument concret à opposer au réflexe « extraire une fonction, c'est cosmétique ».

## Limites connues, explicitement non masquées

- **Le séparateur `; ` entre en collision avec les `;` déjà présents à l'intérieur des chaînes de raison.** La chaîne composée n'est donc **pas décomposable par machine**. Arbitré par l'humain (Q3) : `metric_support` est une chaîne **lisible par un humain**, jamais un contrat machine — un consommateur CI lit la présence du préfixe `degraded:`, pas la structure interne. **Échappatoire nommée d'avance** : si un consommateur machine apparaît un jour, passer le séparateur d'agrégation à ` | ` (absent de toutes les chaînes de raison expédiées) rend la décomposition non ambiguë sans toucher au modèle.
- **`SynCodeParser` sur-déclare.** Il rend `all_supported(Rust)` (`secondaries/src/gateways/code_parsers/syn_code_parser.rs:276`) alors que `resolve_dependency` (`syn_code_parser.rs:372-402`) abandonne silencieusement `#[path]`, les modules générés par macro, les chaînes de ré-export `pub use` et les références inter-crates : **le même angle mort que C# et TS/JS, eux, déclarent**. Trouvé par la lane Security, **routé vers #133**, hors périmètre ici. La référence croisée appartient au graphe parce que #132 est précisément la tranche qui rend cette sur-déclaration **visible par l'opérateur** — jusqu'ici elle était aussi invisible que la chaîne TS/JS.
- **`MetricSupport::Unsupported` reste non atteignable end-to-end** — dette d'[[ADR-0026]] inchangée : aucun adaptateur expédié ne l'émet, le chemin `Unsupported → n/a`/`null` n'est exercé que par fixtures.

## Conséquences

- **(+)** AD-6 d'[[ADR-0030]] tient enfin sa promesse : le texte que 1 395 arêtes rendaient nécessaire **atteint l'opérateur**, sur la console et dans le JSON. La ligne `#132` de la table de dette d'[[ADR-0030]] est **fermée**.
- **(+)** La violation nominale d'[[ADR-0010]] est corrigée : le JSON n'affirme plus `call_graph: "supported"` sur un projet TS/JS. Le contrat machine redevient véridique.
- **(+)** L'agrégat folde **six** axes ; [[ADR-0026]] n'a plus de restriction à quatre. Un futur axe se folde par ajout de donnée, pas de structure.
- **(+)** Les quatre sinks console de note dégradée sont assainis par un helper unique — la défense D6 d'[[ADR-0029]] cesse d'avoir un trou par construction, et l'extraction a élargi la couverture de mutation (3 → 5 mutants).
- **(+)** L'alternative AD-10 d'[[ADR-0030]] — exposer le scoping `sourceRoots` **une seule fois** au niveau du rapport plutôt que dupliqué par langage — était *« bloquée de fait par #132 »* faute de surface opérateur. **Ce blocage est levé** : la surface existe désormais. L'alternative redevient recevable pour une tranche future.
- **(=)** Projet Rust-only : sortie inchangée (fold tout-`Supported`, aucune note).
- **(−)** La chaîne composée n'est pas décomposable par machine (collision `; `) — assumé, échappatoire ` | ` documentée ci-dessus.
- **(−)** `MetricSupportDto` reste un angle mort permanent du gate de mutation (AD-5) : tout champ ajouté devra être épinglé à la main.
