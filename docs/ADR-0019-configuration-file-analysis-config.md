# ADR-0019 — Fichier de configuration : agrégat `AnalysisConfig`, compilation des globs dans l'adaptateur, schéma forward-compat

> **Type:** technical (ADR)
> **Status:** Applied
> **Date:** 2026-07-18
> **Decided in:** Issue #31 (US15), sur `feat/31-config-file`
> **Links:** [[architecture-overview]], [[ADR-0017]], [[ADR-0006]], [[ADR-0001]], [[ADR-0018]], [[alert-thresholds]], [[glossary]]

## Contexte

US15 (#31) donne à l'utilisateur un `.codeimpact.json` capable de **restreindre le périmètre de scan** : `include`/`exclude` en globs, plus un `respectGitignore`. [[ADR-0017]] avait délibérément **réservé la place** de cette évolution dans le même fichier (section `thresholds` isolée, `#[serde(default)]`, tolérance aux clés inconnues). #31 encaisse cette réserve. Quatre forces cadrent la décision :

1. **L'hexagone reste zéro-dépendance** ([[ADR-0001]]). Ni `serde`, ni `globset`, ni `ignore` ne peuvent entrer dans le domaine. Un filtre de fichiers exprimé en globs doit donc séparer le **motif** (domaine) de sa **compilation** (adaptateur).
2. **L'honnêteté de la mesure** ([[ADR-0010]]) : un filtre qui laisse tomber des fichiers en silence — sous un `.gitignore` d'ancêtre, sous le gitignore global de la machine — peut faire **passer un gate `--strict`** ([[ADR-0017]]) en cachant du coût. Le périmètre du scan devient une **frontière de confiance**.
3. **Byte-for-byte compatibilité** : un dépôt sans `.codeimpact.json` (ou sans section filtre) doit se comporter **exactement** comme avant #31 — le walk `walkdir` d'origine n'honorait aucun gitignore.
4. **#33/#34 arrivent dans le même fichier.** Choisir un schéma « juste ce dont #31 a besoin » forcerait une réouverture cassante. #31 déclare donc la **totalité** du schéma cible, même les clés qu'il ne câble pas encore.

## Décision

### 1. Deux nouveaux Value Objects dans l'hexagone zéro-dep

`src/contexts/codeimpact/hexagon/src/analysis/file_filter.rs` — `FileFilter { include, exclude, respect_gitignore }` : porte les **motifs bruts validés**, jamais un matcher compilé. Auto-validant (`ddd-value-object`) — `FileFilter::new` rejette (`FileFilterError`) : motif vide, NUL, chemin absolu, traversée `..`, motif > 512 caractères (`MAX_PATTERN_LENGTH`), plus de 256 motifs (`MAX_PATTERN_COUNT`). Un état capable de produire une path-traversal ou un glob-DoS est **inconstructible**. `FileFilter::unrestricted()` = aucun filtrage = comportement pré-#31.

`src/contexts/codeimpact/hexagon/src/analysis/analysis_config.rs` — `AnalysisConfig { thresholds: AlertThresholds, filter: FileFilter }` : **composition pure de deux VO déjà validés** — aucune validation propre. `defaults()` = `AlertThresholds::none()` + `FileFilter::unrestricted()` (D4). C'est le point d'entrée unique de la config lue.

> **Précision DDD (cohérence avec [[architecture-overview]] « Pas d'Entity/Aggregate »).** `AnalysisConfig` *agrège* les deux réglages de config, mais c'est un **Value Object composite** — immuable, égalité par valeur, sans identité ni cycle de vie. Ce n'est **pas** un Aggregate DDD au sens racine-transactionnelle. Le MVP reste sans Entity/Aggregate.

### 2. `ConfigReaderPort` évolue : `read_thresholds` → `read_config`

Le port ([[ADR-0017]] livrait `read_thresholds -> Option<AlertThresholds>`) rend désormais l'objet de config complet :

```rust
fn read_config(&self, explicit_path: Option<&Path>, search_dirs: &[&Path])
    -> Result<Option<AnalysisConfig>, AnalysisError>;
```

`Ok(None)` = **aucun fichier trouvé** — jamais une erreur (le fichier est optionnel ; D4 : l'appelant retombe alors sur `AnalysisConfig::defaults()`). Un `explicit_path = Some(...)` invalide **est** une erreur (jamais un fall-through silencieux vers l'auto-découverte). L'hexagone ne voit que le VO validé — `serde` reste derrière l'adaptateur ([[ADR-0001]], DIP).

`hexagon/src/analysis/config_reader.rs`.

### 3. D1 — la compilation des globs vit dans l'adaptateur, l'hexagone ne détient que les motifs bruts

Le point le plus structurant. Un glob compilé est une dépendance (`globset`) ; l'hexagone ne peut pas la porter ([[ADR-0001]]). Donc :

- **Hexagone** : `FileFilter` détient les `Vec<String>` bruts, validés à la frontière du VO (voir §1).
- **Adaptateur** : `FileSystemCodeReader::build_glob_set` compile `include` et `exclude` en `GlobSet` (`globset::{Glob, GlobSetBuilder}`) au moment du walk.

C'est la stricte transposition de la discipline d'[[ADR-0018]] : la **syntaxe** (ici, la mécanique de matching glob) vit dans l'adaptateur pilote ; le domaine ne connaît qu'un concept neutre (« un motif de filtrage »).

`secondaries/src/gateways/code_readers/file_system_code_reader.rs`.

### 4. D3 — le walk migre de `walkdir` vers `ignore` + `globset`

`CodeReader::list_source_files` gagne un paramètre `filter: &FileFilter` :

```rust
fn list_source_files(&self, dir: &Path, extensions: &[&str], filter: &FileFilter)
    -> Result<Vec<PathBuf>, AnalysisError>;
```

Le moteur de walk passe de `walkdir` à la crate **`ignore`** (`WalkBuilder`) — deux **nouvelles dépendances `secondaries`** (`ignore`, `globset`), acceptées (D3) parce qu'elles vivent **entièrement** dans l'adaptateur : **l'hexagone reste zéro-dep, [[ADR-0001]] n'est pas seulement préservé mais renforcé** (la logique de scan est repoussée d'un cran hors du domaine). Décision de garde : un fichier est retenu ssi

```
extension ∈ extensions
  ET (include vide OU include_set.is_match(rel))
  ET NON exclude_set.is_match(rel)
```

— **`exclude` l'emporte sur `include`**, `include` vide = tout accepté, matching sur le chemin **relatif à la racine canonique** du walk.

### 5. Politique gitignore : les 4 sources gatent ensemble, `.parents(false)` borne le walk

`ignore::WalkBuilder` expose **quatre** toggles de source d'ignore indépendants — `git_ignore` (`.gitignore`), `git_exclude` (`.git/info/exclude`), `git_global` (gitignore global de la machine), `ignore` (`.ignore`) — tous `true` par défaut. Deux invariants durs (les deux issus de findings de review, retry 1) :

- **Les quatre gatent ensemble sur `respect_gitignore`.** Ne gater que `git_ignore` laissait les trois autres actives même sous `FileFilter::unrestricted()`, larguant des fichiers en silence (finding QA). « Unrestricted » doit être **byte-identique** au walk `walkdir` pré-#31, qui n'honorait aucune de ces sources.
- **`.parents(false)`** : le walker ne consulte **jamais** l'état d'ignore situé **hors du répertoire analysé**. `parents(true)` remonterait lire les `.gitignore`/`.ignore` de chaque ancêtre jusqu'à `/` (finding Security — voir la section frontière de confiance). Compléments : `require_git(false)` (la racine n'est pas garantie être un working tree git — archive extraite, checkout shallow sans `.git`), `hidden(true)`, `follow_links(false)`, `max_depth(MAX_WALK_DEPTH)`.

### 6. Schéma forward-compat + bascule vers `deny_unknown_fields` (changement assumé vs [[ADR-0017]])

Le DTO adaptateur `CodeImpactConfig` déclare le **schéma cible complet**, pas seulement ce que #31 câble :

| Clé JSON | Statut #31 | Type DTO |
|---|---|---|
| `thresholds` | **consommée** (US8) | `AlertThresholds` (via DTO) |
| `include` / `exclude` | **consommées** (US15) | `Vec<String>` |
| `respectGitignore` | **consommée** (défaut `true` si absente) | `bool` |
| `$schema` | acceptée (ergonomie éditeur) | ignorée |
| `languages`, `sourceRoots`, `extensions`, `parser`, `ioSignatures` | **parsées mais non câblées** | `Option<serde_json::Value>` |

Deux conséquences délibérées :

- **Bascule `#[serde(default)]` → `#[serde(deny_unknown_fields)]`.** [[ADR-0017]] était volontairement **tolérant aux clés inconnues** pour réserver la place de US15. Maintenant que le schéma cible est **entièrement déclaré** (y compris les clés #33/#34), la tolérance n'a plus de raison d'être : `deny_unknown_fields` **rejette une faute de frappe** (`respectGitignor`, `treshold`) au lieu de l'avaler en silence — un réglage muet est une perte de garde-fou. C'est un **changement assumé** de la décision d'[[ADR-0017]] §3, rendu sûr par le fait que toutes les clés futures connues sont déjà nommées.
- **`languages`/`sourceRoots`/`extensions`/`parser`/`ioSignatures` sont parsées-mais-inertes.** #33/#34 les câbleront sans **jamais rouvrir** le contrat du fichier ni risquer une migration cassante. La coordination inter-tickets est actée ici, pas découverte au moment de #33.

`FileSystemConfigReader` : `secondaries/src/gateways/config_readers/file_system_config_reader.rs`.

### 7. D4 — sémantique de l'absence, byte-for-byte

- **Fichier absent** ⇒ `read_config` rend `Ok(None)` ⇒ `AnalysisConfig::defaults()` ⇒ `FileFilter::unrestricted()` ⇒ walk **byte-identique** à aujourd'hui (aucune source gitignore honorée, aucun include/exclude).
- **Fichier présent sans clé `respectGitignore`** ⇒ défaut **`true`** (`default_respect_gitignore`). Choisir un `.codeimpact.json` est un acte explicite ; à ce moment, honorer `.gitignore` est l'attente naturelle.

### 8. D2 — le nom `.codeimpact.json` est conservé

Pas de renommage : [[ADR-0017]] a établi `.codeimpact.json` comme le fichier de config partagé, US15 y ajoute une section plutôt que d'introduire un second fichier. Un fichier, un schéma, la réserve d'[[ADR-0017]] honorée.

## Frontière de confiance — résiduel `git_global` (finding Security, LOW, accepté)

Le scan pouvant servir de **gate CI dur** ([[ADR-0017]] `--strict`, exit 3), tout ce qui **retire des fichiers du scan** est une surface de confiance — cacher du coût = faire passer un build qui devrait échouer. Deux vecteurs, traités différemment :

- **Ancêtres du répertoire analysé — fermé.** `.parents(false)` (§5) empêche le walker de lire l'état d'ignore d'un `.gitignore`/`.ignore` d'ancêtre. Sur un hôte CI partagé, une partie hors du dépôt aurait pu, sinon, cacher des fichiers via un répertoire ancêtre. **Fermé par construction.**
- **Gitignore global de la machine — résiduel accepté (LOW).** Sous `respectGitignore:true`, `git_global(true)` honore le gitignore **global de la machine** (`$XDG_CONFIG_HOME/git/ignore` / `core.excludesFile`) — une source **indépendante** de la borne walk-root. Sur un hôte CI mutualisé/multi-tenant, une partie disposant d'un accès en écriture à la **config d'identité git de la machine** pourrait masquer des fichiers du gate énergie/CO2 `--strict`.
  - **Barre de privilège plus haute** qu'un simple dépôt d'un `.gitignore` d'ancêtre : il faut l'écriture sur la config git globale de l'hôte, pas juste sur un répertoire parent.
  - **Sémantique git standard, pas un défaut.** `git status`, `ripgrep`, tout outil respectant gitignore honore la même source globale. La désactiver diverge du comportement attendu par l'utilisateur.
  - **Résiduel accepté, aucune contre-mesure code.** Remédiation opérationnelle, même schéma qu'[[ADR-0006]] §« frontière de confiance » (`.codeimpact.json` self-configurable) et que les résiduels TOCTOU / profondeur de walk : sur un hôte de confiance au niveau du contrôle d'accès, la config git globale l'est aussi. Un runner CI mutualisé non fiable est déjà hors du modèle de menace de l'outil.

## Conséquences

- **(+)** Hexagone toujours zéro-dep ([[ADR-0001]]) : `serde`, `ignore`, `globset` vivent **tous** dans `secondaries` ; le domaine ne connaît que `FileFilter`/`AnalysisConfig` déjà validés.
- **(+)** `unrestricted()` byte-identique au walk pré-#31 — aucune régression pour un dépôt sans config.
- **(+)** `deny_unknown_fields` fait échouer une faute de frappe au lieu de l'ignorer ; le schéma complet préempte la réouverture par #33/#34.
- **(+)** La fuite d'ignore par ancêtre est fermée (`.parents(false)`) ; le résiduel `git_global` est documenté, pas caché ([[ADR-0006]]).
- **(−)** Deux dépendances `secondaries` de plus (`ignore`, `globset`) — acceptées (D3), confinées à l'adaptateur.
- **(−)** Résiduel `git_global` sous hôte CI mutualisé non fiable (voir frontière de confiance) — non traité par du code, remédiation opérationnelle.

## Addendum #96 — pruning `exclude` au parcours, seulement pour les formes de glob sans ambiguïté de dialecte

Avant #96, `include`/`exclude` étaient appliqués **post-marche** (par fichier) : seul `.gitignore` élaguait la descente dans les répertoires. Exclure `target/` (20,6k fichiers) via `exclude=["target/**"]` sous `respectGitignore:false` était ~34x plus lent que l'exclusion par gitignore, à résultat identique.

#96 enregistre `exclude` comme surcharges `ignore::overrides::Override` (négatives) au **temps de marche** — mais **uniquement pour la forme `<littéral>/**`** (préfixe non-vide, sans métacaractère glob). Raison : `OverrideBuilder` utilise la **syntaxe gitignore-line**, qui diverge du `globset::Glob` ancré antérieur sur deux points (vérifiés empiriquement vs globset 0.4.19 / ignore 0.4.31) : (1) un motif sans `/` matche le **basename à toute profondeur** en gitignore vs. un **chemin littéral top-level** en globset ; (2) un `*` simple ne **traverse jamais `/`** en gitignore-line mais **le traverse** en globset (`literal_separator=false`). Seule la forme `<littéral>/**` est prouvée identique dans les deux dialectes. **Tout autre motif retombe sur le `GlobSet` post-marche pré-#96** — résultat byte-identique. La garantie « le filtrage produit exactement le même ensemble de fichiers » (§4) est ainsi préservée ; seul le **coût** change, et seulement là où c'est sûr (le cas motivant `target/**`).

## Dette connue, explicitement non traitée

- **`languages` / `sourceRoots` / `extensions` / `parser` / `ioSignatures`** — parsées mais inertes ; câblage laissé à #33/#34.
- **Résiduel `git_global`** — accepté, remédiation opérationnelle (hôte CI de confiance).
- **Filtres par-fonction / par-fichier de sortie** — hors scope ; `FileFilter` borne le **périmètre de scan**, pas la granularité du rapport.
