# ADR-0014 — Le parseur voit enfin les méthodes : nom qualifié, résolution intra-type, et trois états de mesure

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-14
> **Decided in:** Issue #50
> **Links:** [[architecture-overview]], [[ADR-0010]], [[ADR-0013]], [[ADR-0007]], [[ADR-0008]], [[glossary]], [[json-report-schema]], [[console-report-enriched]], [[html-report]]

## Contexte

`syn_code_parser.rs` ne visitait qu'une seule forme d'item :

```rust
for item in &syntax_tree.items {
    if let syn::Item::Fn(func) = item {      // uniquement les fn libres de premier niveau
```

Jamais `syn::Item::Impl` → `syn::ImplItem::Fn`. Jamais les méthodes par défaut de trait. Jamais les `mod` inline.

**Mesuré sur ce dépôt : 36 fichiers sur 69 parsaient zéro fonction** — dont `code_metrics.rs` et `call_graph.rs`, qui ne sont *que* des blocs `impl`. Pour CodeImpact, ils étaient vides.

En Rust idiomatique, la quasi-totalité du code applicatif vit dans un `impl`. L'outil ne voyait donc qu'une fraction du code, et **toutes les grandeurs publiées — complexité directe, transitive, cachée, hotspots, coût €, CO₂ — étaient des sous-comptages massifs**. Et un fichier non lu s'affichait comme un fichier *simple*.

C'est la quatrième forme, et la plus large, de la maladie que [[ADR-0010]] combat : **l'outil rapporte avec assurance des chiffres calculés sur ce qu'il n'a pas regardé.**

## Décision

### 1. Le nom qualifié : `Type::method`, et rien de plus exotique

`ParsedFunction.name` est désormais qualifié par la **portée englobante**. Le schéma est de la *syntaxe* → il vit dans l'adaptateur, jamais dans l'hexagone ([[ADR-0013]]). Le domaine continue de ne voir qu'une `String` opaque.

| Déclaration | `name` émis |
|---|---|
| `fn foo` racine | `foo` *(inchangé — non-régression)* |
| `impl Type { fn foo }` | `Type::foo` |
| `impl Trait for Type { fn foo }` | `Type::foo` |
| `impl<T> Foo<T> { fn bar }` | `Foo::bar` (génériques effacés) |
| `impl Trait for &Type` / `Vec<T>` | dernier segment du type déréférencé |
| `self_ty` non nommable (tuple, `[T; N]`) | repli sur le **nom du trait** : `Trait::foo` ; à défaut `foo` |
| `trait Tr { fn foo() {} }` (défaut) | `Tr::foo` |
| `trait Tr { fn foo(); }` (sans corps) | **rien** — une signature n'est pas une fonction |
| `mod m { … }` inline | préfixe `m::`, récursif (`m::T::foo`) |
| `fn` imbriquée dans un corps | pas d'entrée séparée — repliée dans le parent *(inchangé)* |

**Rejeté : `<Type as Trait>::method`.** Trois raisons, par poids décroissant :

1. **Ça casserait le graphe d'appel.** Un `self.foo()` ne peut pas être résolu vers `<Type as Trait>::foo` sans inférence de types — que `syn` ne fournit pas. Toutes les arêtes vers des méthodes de trait pendraient. On paierait de la désambiguïsation en perdant la fonctionnalité qui la motive.
2. **Le problème qu'il résout n'existe pas ici** : **0 collision mesurée sur 69 fichiers**.
3. C'est du bruit dans un rapport lu par un humain.

**L'unicité est garantie par le parseur.** Une collision reste possible (`Type::default` inhérent + `impl Default for Type`). Sur doublon, suffixe dans l'ordre source : `Type::foo`, `Type::foo#2`. Ce n'est pas cosmétique : `CallGraph::build` fait `edges.insert(f.name, …)` — **un doublon écraserait silencieusement une fonction entière**, sa complexité et ses arêtes disparaîtraient du rapport. Une fonction perdue est exactement le zéro invisible que ce ticket tue.

### 2. La résolution des appels : `self` oui, l'homonymie jamais

C'est ici que ce changement pouvait créer une **régression silencieuse**, et c'est le seul endroit qui la ferme. Qualifier les *déclarations* sans résoudre les *appels* aurait fait pendre toutes les arêtes intra-type — la complexité cachée et transitive se serait effondrée sans un mot.

| Site d'appel | Nom enregistré | Justification |
|---|---|---|
| `self.m(...)` dans `impl Type` | `Type::m` | **61/61 résolus (100 %)**. Sans cette règle, le callee `m` ne matcherait plus la déclaration `Type::m`. |
| `Self::m(...)` | `Type::m` (segment `Self` réécrit) | Exact, zéro heuristique. |
| `Type::m(...)` (UFCS) | `Type::m` | Matche déjà naturellement. |
| `x.m(...)`, `self.field.m(...)` | **`m` (identifiant nu)** | **Résoudre par homonymie est interdit.** |

**Pourquoi l'homonymie est interdite.** `x` est le plus souvent un `Vec`/`String`/`HashMap` : **2275 appels de ce type, 8 seulement ont un homonyme déclaré dans le fichier**. Résoudre par nom court **fabriquerait 8 arêtes fausses** vers du code jamais appelé.

> **Fabriquer une arête est le pendant exact du `0` confiant d'[[ADR-0010]].** L'un invente une mesure, l'autre invente une relation. On ne fabrique pas.

**Ce que le graphe d'appel ne sait pas — écrit noir sur blanc.** Le graphe est **intra-fichier**, et pour les méthodes **intra-type**. Un appel `x.m()` sur un récepteur externe pend et contribue `0` à la complexité transitive. C'est *correct* : ce code n'est pas dans le fichier, on ne l'a pas mesuré. La résolution inter-fichiers n'a jamais existé et n'est pas créée ici.

### 3. `#[cfg(test)] mod tests` est exclu du parsing

Descendre dans les `mod` inline (§1) fait entrer le `mod` inline **massivement dominant en Rust** : `#[cfg(test)] mod tests`. Livré tel quel, cela faisait entrer **172 fonctions de test** dans les métriques de production — gonflant la liste des fonctions, le graphe d'appel, et surtout `hidden_complexity` (chaque test « atteint » le code de prod), donc **le coût € et le CO₂ d'un code qui ne tourne jamais en production**.

Ce n'est pas du scope creep : c'est **refuser de livrer un défaut neuf créé par ce changement même**.

Le coût est nul, mesuré : l'exclusion fait passer Σ complexité de **531 à 530** (Δ = 1) et ne déplace **aucun** fichier dans la distribution des niveaux. Les corps de test sont plats, sans branches. On retire 172 fausses fonctions sans toucher à la complexité.

`#[cfg(test)]` est de la syntaxe Rust → la règle vit dans `secondaries/` ([[ADR-0013]]). Les fichiers d'un répertoire `tests/` (tests d'intégration, sans l'attribut) restent analysés si l'utilisateur les cible.

### 4. Trois états de mesure, pas deux — et « zéro fonction » n'est PAS « non mesuré »

Le ticket exigeait : *« un fichier qui parse zéro fonction doit être remonté NON MESURÉ »*. **Appliqué à la lettre, ce critère produisait un second mensonge.**

Les **17 fichiers qui restent à zéro fonction après correction** sont, un par un : `code_parser.rs`, `code_reader.rs`, `report_writer.rs` (déclarations de traits **sans corps par défaut**), `io_in_loop_warning.rs`, `output_format.rs` (types de données purs), et 12 `mod.rs`/`lib.rs` de ré-export. **Aucun n'est un fichier que l'outil a échoué à lire.** Les étiqueter « NON MESURÉ » serait faux : on les a parfaitement lus — il n'y a simplement **rien à mesurer dedans**.

L'*intention* du critère — *« ne jamais afficher `trivial` pour ce qu'on n'a pas regardé »* — est juste. Elle est servie par **trois** états :

| État | Condition | Publié | Agrégats |
|---|---|---|---|
| **Mesuré** | ≥ 1 fonction parsée | `complexity_level` ∈ `low`/`moderate`/`high`/`critical` | comptés |
| **Rien à mesurer** | parse OK, 0 fonction | **`complexity_level: "none"`** — neutre en console, **gris** en HTML. Jamais `low`, jamais vert. | comptés (leur complexité *est* 1) |
| **Non mesuré** | `syn::parse_file` échoue **ou** la lecture échoue | le fichier **apparaît**, marqué `NON MESURÉ` + raison | **exclus de toute somme** |

**Le vrai bug [[ADR-0010]] que le ticket n'avait pas vu.** Avant ce changement, un fichier qui échouait au parse était **purement supprimé du rapport** (`run_analysis.rs` : `if let Ok(metrics) = … { per_file.push(…) }`, sinon rien). Il ne devenait pas `0` : **il devenait invisible**, et `total_files` le sous-comptait. Même mensonge, un étage plus haut. Fermé ici.

**Le signal remonte sur les trois surfaces, jamais sur une seule** — une honnêteté qui ne vaut que sur un canal n'est pas de l'honnêteté :
- **JSON fichier** : `complexity_level: "none"`.
- **JSON projet** : `unmeasurable_files: [{path, reason}]` + `unmeasurable_files_count`. **Additif** → [[ADR-0007]] respecté, aucun champ retiré ni renommé.
- **Console** : `niveau: none` par fichier ; section `=== Fichiers NON MESURÉS (n) ===`.
- **HTML** : niveau gris `lvl-none` + section dédiée.

> **Deux pièges désamorcés, sans quoi le remède était pire que le mal.** `view_model.rs` `level_rank` avait un fourre-tout `_ => 3` : introduire la chaîne `"none"` sans y toucher aurait classé **tout fichier sans fonction en CRITICAL rouge**. Et le map JS `LVL` de `assets.rs` repliait sur `"lvl-low"` : `none` se serait affiché **vert « propre »**. Les deux ont été fermés (rang 0 explicite, entrée `none → lvl-none` explicite).

**XSS.** Le chemin d'un fichier non mesuré atteint le HTML pour la première fois (avant, un fichier illisible n'y arrivait jamais — il disparaissait). Aucun échappement nouveau n'a été écrit : le payload traverse le `json_island_escape` existant ([[ADR-0008]] §8.6) et le JS ne fait que `textContent`, jamais `innerHTML` (§8.10). Audit adversarial mené sur le binaire réel avec un fichier nommé `"><img src=x onerror=alert(1)>evil.rs` et un payload `</script><script>alert(1)</script>` : **zéro balise injectée**. Les deux défenses tiennent indépendamment.

### 5. Les seuils ne sont PAS recalibrés — et c'est une décision, pas un oubli

Le ticket anticipait une recalibration. **Les données disent non.**

- La distribution *avant* était dégénérée (**96 % « low »**) — non parce que les seuils étaient faux, mais parce que **la moitié du code était invisible**. On ne recalibre pas un seuil pour compenser une entrée manquante : **on répare l'entrée.**
- La distribution *après* est saine et discriminante : `none` 17 / `low` 42 / `moderate` 3 / `high` 5 / `critical` 4. Longue traîne — exactement ce qu'un seuil doit produire.
- Les fichiers `critical` sont, à l'inspection, **réellement les plus complexes du dépôt** (`syn_code_parser.rs`, `console_report_writer.rs`, `html/view_model.rs`, `cargo_test_runner.rs`). Zéro fausse alerte. `hotspot_files` désigne enfin de vrais hotspots.

**Argument décisif de méthode :** changer les seuils *dans le même commit* qui change l'entrée rendrait le changement **infalsifiable** — plus personne ne saurait lequel des deux a bougé les chiffres. Toute recalibration se fera dans un ticket ultérieur, avec la distribution post-#50 comme ligne de base.

## Conséquences

- **(+)** Les fichiers à zéro fonction passent de **36/69 à 17/69**, et les 17 restants ont *vraiment* zéro fonction.
- **(+)** Les grandeurs publiées cessent d'être des sous-comptages : **Σ complexité 222 → 571**, fonctions parsées **382 → 636**.
- **(+)** Un fichier illisible cesse d'être invisible : il apparaît, marqué, exclu des sommes, **sur les trois surfaces**.
- **(+)** Le graphe d'appel voit enfin la récursion mutuelle intra-type et les vraies boucles quadratiques en forme de méthode.
- **(−) Les chiffres publiés augmentent d'un facteur ~2,5.** C'est **voulu** : c'était le bug. Un utilisateur qui voit ses chiffres doubler doit savoir pourquoi — d'où cet ADR.
- **(−)** Le suffixe `#2` peut apparaître dans le JSON. 0 occurrence mesurée sur ce dépôt.

## Dette connue, explicitement non traitée

- **`is_io` reste structurellement `false` pour un appel de méthode** ([[ADR-0013]], dette connue). #50 corrige *quelles déclarations on voit* ; #56 concerne *comment classifier le récepteur d'un appel* — orthogonal. Après #50, `self.m()` devient `Type::m` : ça ne commence toujours pas par `std::fs::`. Détecter `file.read_to_string(…)` exige le **type du récepteur**, que `syn` n'infère pas. La seule voie est une liste blanche de noms de méthodes — un arbitrage faux-positifs/faux-négatifs qui mérite son propre ADR. → **#56**, non bloquant, non dégradé par #50.
- **Amplification de ressources** : la construction du nom qualifié coûte `O(fonctions × profondeur de mod)`. Mesuré : un fichier hostile de ~1 Mo (500 `mod` imbriqués, 80 k fonctions) atteint **902 Mo de RSS**. Dégradation gracieuse (lent, pas faux), mais un garde-fou de taille/nombre de fonctions manque. → **Issue #62**.
- **Débordement de pile dans `syn::parse_file` lui-même** : ~1700 `mod` imbriqués (fichier de 12 Ko) → `SIGABRT` « stack overflow ». Vérifié en A/B contre `main` : **identique avant et après #50** — la pile déborde dans `syn`, avant que le code de ce ticket ne s'exécute. **Pas une régression**, mais une DoS réelle. Cousin de **#52** (les parcours récursifs du graphe d'appel). → **Issue #63**.
- ~~**`complexity_level_for` appliqué au total projet**~~ : **Résolu (#60)**, voir [[ADR-0010]] § Dette connue — le JSON projet lit désormais la médiane par fichier, pas le total.
- **Recalibration des seuils** : délibérément non faite ici (§5), à instruire sur la ligne de base post-#50. → **Issue #61**.
