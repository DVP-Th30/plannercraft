# Moteur en dixièmes de minute : inventaire et conception (phase A)

Décision du propriétaire (2026-11-15) : option O1 de l'ADR-0037 D6. Le moteur calcule désormais en dixièmes de minute, comme MS Project. Ce document est la phase A : inventaire et conception, sans aucun changement de code du produit. La phase B (réalisation) attend sa relecture.

Statuts : **observé** (mesuré ou lu sur des fichiers ou dans le code, commande et résultat cités) ; **déduit** (tiré des observations) ; **inconnu**.

## 0. Ce que montrent les fichiers

- **Observé**, sur 4 fichiers réels publics (`advaith14…`, `AtharvaTK1997…`, `nehathota123…`, `lautarofaccini…`) : toutes les dates enregistrées ont des secondes multiples de 6, donc une précision au dixième de minute. Ce sont les 4 seuls fichiers réels sur 41 dont des dates portent des secondes, et aucune de ces secondes n'est hors des multiples de 6.
- **Observé** : sur les mêmes fichiers, 157 tâches commencent et finissent le même jour. Leur fin vaut exactement le début plus la durée, au dixième près, dont 7 à durée fractionnaire. Exemple, `advaith14…` UID 80 : 1 125 dixièmes (`PT1H52M30S`), du 09:27:12 au 11:19:42.
- **Observé** : les plages horaires des calendriers sont toujours en minutes entières. Sur 582 plages lues dans les fichiers réels et de test, aucun début ni aucune durée n'a de dixième non nul.
- **Observé** : le travail des affectations a une précision plus fine que le dixième. Il est stocké en `f64`, en millièmes de minute, dans le `.mpp`. Le XML du corrigé l'écrit à la seconde, par exemple `PT13H38M11S`.
- **Observé**, sur les fichiers réels de disposition 2013, valeurs brutes du `.mpp` : pour les tâches à unités fixes, la durée vaut le maximum de travail ÷ unités sur les affectations de travail. Résultats :
  - quotient entier en dixièmes : 1 657 égaux, 27 non égaux pour d'autres causes (nivellement, valeurs incohérentes) ;
  - quotient non entier : sur 22 cas, la troncature au dixième retombe sur la durée de MS Project 20 fois, l'arrondi au plus proche 18 fois.
  - Les deux formes de désaccord : `advaith14…` UID 12 (272,73 → 273, arrondi) et `AtharvaTK1997…` UID 27 (46 525,71 → 46 525, troncature).
  - Ailleurs, MS Project enregistre 191 h 59 min 54 s pour 38 h 24 de travail à 0,2 : une division flottante tronquée.
- **Déduit** : MS Project garde durées, dates et marges en dixièmes de minute, et le travail plus finement. Une durée tirée du travail semble tronquée au dixième, après un calcul flottant. **Inconnu** : la règle exacte (2 cas sur 22 restent inexpliqués).
- **Observé** : les dates sont affichées et comparées à la minute, tronquées (R-283, R-004). Le corrigé et MS Project montrent 09:27 pour 09:27:12.

## 1. Types et unité

### Ce qui existe

| Type | Aujourd'hui | Accès bruts |
|---|---|---|
| `Minutes(i64)` | minutes signées | `new(i64)`, `get() -> i64`, `checked_*` ; `#[serde(transparent)]`, entier JSON |
| `Moment(i64)` | minutes depuis 1984-01-01 | `from_minutes`, `minutes() -> i64`, `time_of_day() -> TimeOfDay`, `checked_add(Minutes)`, `minutes_until` ; JSON `"AAAA-MM-JJTHH:MM"` |
| `TimeOfDay(u16)` | minutes du jour, 0 à 1 440 | `new`, `hm`, `minutes()` |
| `Duration` | `Minutes` ≥ 0, unité, base, estimée | `minutes() -> Minutes` |
| `Work(Minutes)` | minutes ≥ 0 | `new(Minutes)`, `minutes()` ; JSON entier |
| `Lag`, marges, retards, durées réelles et restantes | `Minutes` | — |
| `ConversionFactors` | minutes par jour, par semaine (`u32`), jours par mois | `minutes_per_unit -> Minutes` |

### Options

- **A. Même type, nouvelle représentation.** `Minutes` et `Moment` portent des dixièmes en interne. Les constructeurs en minutes gardent leur sens (`Minutes::new(480)` = 480 minutes, stocké 4 800). Accesseurs explicites : `tenths()` (exact), `from_tenths()`. Toute sortie en minutes entières dit sa règle dans son nom : `whole_minutes_floor()`, `whole_minutes() -> Option<i64>` (`None` si fractionnaire). L'ancien `get()` et `Moment::minutes()` sont **retirés**, ce qui oblige à revoir chaque appel : la compilation les liste tous (§ 7).
- **B. Renommer** (`Minutes` → `Tenths`) : chaque `Minutes::new(480)` deviendrait `Tenths::new(4_800)`. Cela réécrit environ 700 constructions (dont 429 dans `tests/` et 98 dans les tests du cœur) et rend les tests de règles illisibles en minutes.
- **C. Nouveau type à côté** (`Minutes` garde les minutes, `Tenths` pour le moteur) : deux types pour une même grandeur, des conversions partout, et le risque de mélanger les deux.

**Retenu : A.** C'est le moins de réécriture (les 429 `Minutes::new` de `tests/` restent tels quels), les tests restent lisibles en minutes, et retirer `get()` / `minutes()` rend la troncature visible à chaque site au lieu de la laisser silencieuse.

| Grandeur | Unité retenue | Forme |
|---|---|---|
| Instant (`Moment`) | dixième de minute | A : `from_minutes` (×10), `from_tenths`, `tenths()`, `minutes_floor()` ; `time_of_day()` renommé `time_of_day_floor()` (tronque les secondes) |
| Durée, décalage, marge, retards, durée réelle et restante (`Minutes`) | dixième | A |
| Horaires de calendrier (`TimeOfDay`, plages) | **minute, inchangé** | 582 plages sur 582 en minutes entières, et MS Project ne saisit qu'à la minute ; converties en dixièmes à la compilation du calendrier (`CompiledCalendar`) |
| Facteurs (`ConversionFactors`) | minute, inchangé | `minutes_per_unit` rend des `Minutes` (donc des dixièmes en interne) |
| Travail (`Work`) | **millième de minute** | constructeur `Work::new(Minutes)` exact (×100) ; `from_milli`, `milli()` ; `minutes()` renommé `minutes_round()` ou retiré |
| Unités, pourcentages, coûts | inchangés | 1/10 000, centièmes |

**Pourquoi le travail en millièmes (déduit) :** avec le travail au dixième, `advaith14…` UID 226 (818,18 min à 0,6) donnerait 1 363,7 min, contre 1 363,6 chez MS Project. En millièmes, la troncature donne 1 363,6. Les millièmes sont aussi l'échelle du `.mpp` (`f64` ×1 000). Bornes : 99 999 999 960 min × 1 000 = 10^14, très loin de `i64` ; les produits travail × unités se font déjà en `i128`.

**Ce qui reste une hypothèse :** la règle de la durée tirée du travail (R-170 inverse). Deux options :
- troncature au dixième : 20 cas sur 22 ;
- arrondi R-001 : 18 cas sur 22.

Je propose la troncature (§ 2). Les artefacts flottants de MS Project (191 h 59 min 54 s au lieu de 192 h) ne seront **pas** reproduits : le moteur calcule exactement.

## 2. Arrondis

| Où le moteur arrondit aujourd'hui | Règle | Devient |
|---|---|---|
| `round_div` générique, moitié loin de zéro | R-001 | inchangé comme fonction ; sa cible devient le dixième pour les temps |
| Saisie décimale → minutes (`to_minutes`) | R-001, R-140 | → dixièmes (« 0,01 j » = 4,8 min, exact) |
| Minutes → valeur affichée à 2 décimales (`to_unit_value`) | R-009, R-141 | inchangé (entrée en dixièmes) |
| Décalage en % (`percent_of`) | R-010 | → dixièmes, R-001 (33 % de 480 = 158,4, exact) |
| `d_a = W × 10 000 / U` et inverse | R-170, R-181, R-183, R-209, R-211, R-194 | → dixièmes ; **troncature** pour la durée tirée du travail (hypothèse, § 0) ; travail tiré de la durée exact en millièmes |
| Débit d'utilisation | R-186, R-188, R-245 | division en dixièmes, R-001 |
| % achevé, % travail | R-055, R-200, R-210, R-252 | inchangé (1/10 000) ; la durée réelle `percent_of(D, %)` passe au dixième |
| ISO 8601 → minutes, secondes arrondies | R-070 | → dixièmes : multiple de 6 s exact, autre valeur arrondie au dixième (R-001) avec `mspdi.rounded` |
| `LinkLag` en dixièmes → minutes | R-072 | exact, plus de diagnostic |
| Secondes des dates tronquées | R-004, R-075, R-143, R-283 | tronquées **au dixième** (6 s) et non plus à la minute ; diagnostic seulement si la seconde n'est pas multiple de 6 |
| Marges relues en dixièmes, comparées au moteur × 10 | R-083 | comparées directement |
| Écriture des durées ISO et dates MSPDI | R-120, R-122, R-127, R-199, R-215 | écrites au dixième (`PT1H52M30S`, `09:27:12`) ; identiques pour des minutes entières |
| `.mpp` : horodatages et durées en dixièmes arrondis à la minute | R-283, R-285, R-286, R-289, R-295 | lus exactement, plus aucune conversion |
| Classes d'écart du rapport | R-298 | comparaison au dixième ; la cause `mpp.gap.sub_minute` disparaît (§ 9) |
| Glisser une barre ou l'avancement | R-240, R-242 | inchangé : le geste produit des minutes entières |

**Règles dont la formulation change.** Au moins R-001 (exemples), R-004, R-009, R-010, R-070, R-072, R-075, R-083, R-140, R-141, R-143, R-170, R-186, R-188, R-200, R-209, R-211, R-283, R-285, R-286, R-295, R-298.

**À relire en phase B, formulation peut-être inchangée :** R-055, R-088, R-120, R-122, R-127, R-133, R-177, R-194, R-198, R-199, R-215, R-245.

Les 107 autres fiches qui citent « R-001 » ne le citent que comme source de l'arrondi des pourcentages : inchangées.

## 3. Affichage

Critère dur : pour un planning dont toutes les valeurs sont en minutes entières, aucun pixel ni aucun texte ne change.

- **Dates affichées : tronquées à la minute**, comme MS Project et le corrigé (`time_of_day_floor`, `minutes_floor`). Le rendu SVG calcule ses abscisses `floor(dt × longueur / étendue)` en entiers. Avec `dt` et l'étendue tous deux multipliés par 10, le résultat est identique pour des minutes entières (**déduit**, à vérifier sur les 4 golden SVG).
- **Interface egui** (`timescale.rs`, `x_minutes` : `delta as f32 × px_par_minute`). Multiplier un écart en dixièmes par `px_par_minute / 10` changerait l'arrondi flottant, donc des pixels. Retenu : si l'écart est un multiple de 10 dixièmes, on garde exactement l'ancien calcul (écart ÷ 10, entier) ; sinon, on ajoute la fraction. Les 111 captures egui doivent rester identiques.
- **Dates saisies** : à la minute (R-143). Une saisie ne crée jamais de fraction.
- **Durées affichées** : `to_unit_value` à 2 décimales (R-009, R-141), appliqué à des dixièmes. 112,5 min s'affiche « 112,5m » en minutes, « 1,88h » en heures (MS Project : « 1.88 hrs »).
  - Le texte écrit doit se relire à l'identique. R-141 passe déjà à 5 décimales si 2 ne suffisent pas ; la borne de son erreur est à refaire pour le dixième : 5·10⁻⁶ × 44 640 = 0,22 min > 0,05 min, donc 6 décimales seront nécessaires pour les mois (à établir en phase B).
  - Une durée entière s'affiche comme avant.
- **Durées saisies** (R-140) : 1 à 6 décimales acceptées comme aujourd'hui ; arrondi au dixième au lieu de la minute. « 112,5m » donne 112,5 min ; « 0,01j » donne 4,8 min (5 aujourd'hui) ; une saisie entière ne change pas.
- **Fenêtre des différences MS Project** (lot M3) : elle compare les dates à la minute affichée. Un écart en dessous de la minute s'affiche « 0 min » dans la classe « à deux minutes près » (§ 9).

## 4. Formats lus et écrits

| Format | Lecture | Écriture | Aller-retour |
|---|---|---|---|
| MSPDI | durées ISO au dixième (6 s) ; autre seconde : arrondie au dixième, `mspdi.rounded` ; dates `:SS` : multiple de 6 exact, sinon tronquée au dixième avec le même diagnostic ; `LinkLag` et marges en dixièmes : exacts ; travail ISO à la seconde : arrondi au millième de minute (erreur ≤ 0,000 5 min) | `PT{h}H{m}M{s}S` avec `s` multiple de 6 ; dates avec `:SS` quand non nulles ; identique pour des minutes entières (tests d'aller-retour et corpus MSPDI inchangés) | un fichier à fractions relu puis réécrit garde ses dixièmes ; le travail garde sa seconde quand elle tombe sur un millième, sinon il est arrondi au millième (**déduit**, limite consignée) |
| `.mpp` | horodatages et durées déjà en dixièmes : plus de conversion ; travail `f64` décodé exactement en millièmes (décodage par les bits déjà écrit au lot M2) | — | — |
| CSV | durées en texte (R-140) au dixième ; dates à la minute | durées par R-141 (relecture exacte) ; dates tronquées à la minute, comme l'affichage | une date à secondes passée par un CSV perd ses secondes : accepté, le CSV est un format d'échange à la minute |
| Import PDF | dates et durées lues à la minute ou au jour | — | inchangé |

## 5. Contrats JSON, MCP, journal et sauvegarde

**Ce qui sérialise une durée ou un instant (observé) :**
- le modèle `Project` (sérialisation serde des `Duration`, `Lag`, `Work`, `Moment`, marges) : sauvegarde automatique du navigateur (ADR-0031), fichier compagnon, journal de commandes (`journal.rs`, `journal_file.rs`), reprise (`recovery.rs`), presse-papiers (`clip.rs`) ;
- `Schedule` et les vues JSON (`view.rs`, `explain.rs`, `analysis.rs`, `diff.rs`) ;
- les paramètres de commande (`DurationJson`, `LagJson` : `minutes: Option<i64>`) et les 63 schémas golden (`crates/plannercraft-automation/tests/golden/schemas`, dont 10 avec un champ `minutes` entier, `format: int64`) ;
- le schéma `Moment` (motif `T..:..(:00)?`), les rapports JSON du CLI (`import-mpp`, `import-pdf`, `tasks --json`), le style et les préférences (aucune durée observée hors `TimeOfDay`).

**Retenu (j'accepte ta préférence, avec deux précisions) :**
- **Unité des contrats : la minute**, inchangée. Une valeur entière s'écrit sans décimale (octets identiques pour un planning en minutes entières). Une valeur fractionnaire s'écrit avec une décimale (`112.5`), jamais plus : les dixièmes sont exacts.
- **En entrée**, un entier garde son sens. Un nombre à décimale est accepté s'il tombe sur un dixième (tolérance 10⁻⁶ min), sinon il est refusé (`invalid`).
  - Travail : deux champs. `minutes` reste au dixième, et le millième n'est exposé que si nécessaire. Proposition : le travail s'écrit en minutes avec jusqu'à 3 décimales, puisqu'il est en millièmes.
- **Instants** : `"2018-09-24T09:27:12"` quand les secondes sont non nulles, sinon `"…T09:27"` comme aujourd'hui. À la lecture, `:SS` est toléré ; multiple de 6 exact, sinon tronqué au dixième. Le motif du schéma devient `(:[0-9]{2})?`.
- **Où se fait la conversion, et pourquoi elle est déterministe.** Dans `Serialize` / `Deserialize` de `Minutes`, `Work` et `Moment`, en cœur, sans calcul flottant :
  - lecture : `serde_json` lit le nombre en `f64` (lecture correctement arrondie, la même sur toutes les plateformes), puis le cœur décode le `f64` exactement à partir de ses bits (mantisse, exposant) et arrondit au dixième avec des entiers, comme `le_f64_round` du lot M2 ;
  - écriture : l'entier est formaté en texte (`"1125"` → `"112.5"`) puis analysé en `f64` (pas d'arithmétique flottante, lint `float_arithmetic` respecté) ; `serde_json` écrit le `f64` le plus court, `112.5`.
  - Écartée : la fonctionnalité `arbitrary_precision` de `serde_json`, qui changerait le comportement de tout le workspace par unification des fonctionnalités.
- **Schémas MCP** : `minutes` passe de `integer` à `number` (une entrée entière reste valide), la description dit « au dixième ». Les clients MCP qui ne lisent que des entiers en sortie ne voient un nombre à décimale que pour un planning fractionnaire.
- **Compatibilité ascendante (déduit)** : un journal, une sauvegarde ou un presse-papiers écrits par la version actuelle (entiers, dates à la minute) se relisent avec le même sens. L'inverse n'est pas garanti (une ancienne version refuse `112.5`), ce qui est acceptable pour une mise à jour.

## 6. Règles du dépôt

**Règle 5 de `CLAUDE.md`, nouvelle rédaction proposée :**

> 5. **Déterminisme.** Mêmes entrées, mêmes sorties, sur toutes les plateformes. Temps en entiers `i64` : instants, durées, décalages et marges en dixièmes de minute (comme MS Project), travail en millièmes de minute, horaires de calendrier en minutes ; les constructeurs prennent des minutes, toute sortie arrondie le dit dans son nom ; unités, pourcentages et coûts en point fixe entier (politique dans `docs/decisions.md`) ; jamais de calcul en `f32`/`f64` dans le moteur (un `f64` lu d'un fichier ou d'un JSON est décodé exactement depuis ses bits). Pas d'itération sur `HashMap`/`HashSet` dans le calcul (`BTreeMap`, `Vec` indexés). Aucune horloge système dans `plannercraft-core` : « maintenant » est injecté.

**À amender :**
- `docs/decisions.md` : ADR-0005 (« Point fixe : durées et travail en minutes `i64` », « MS Project arrondit à la minute, hypothèse à vérifier ») et ADR-0008 (« Temps : `Moment(i64)` en minutes… Résolution à la minute ») ; nouvelle ADR-0039 pour ce lot ; ADR-0037 D6 (option retenue).
- `docs/scheduling-rules.md` : les fiches du § 2.
- `docs/features/automation.md` (forme `{"minutes","unit","text"}`), `mcp.md`, `cli.md`, `render.md`, `ui.md` (affichage des durées) ; `docs/formats/mpp.md` (§ 6, plus de conversion) ; `docs/plans/mpp-import.md` (§ 10, classes) ; `docs/scorecard.md`.
- Guide utilisateur : `docs/user/ms-project-export.md` (précision au dixième) et `docs/user/panes-columns-fields.md` (colonne Durée).

## 7. Inventaire chiffré

**Mesure (observé)** : essai jetable hors produit, dans une copie détachée du dépôt (supprimée après la mesure). Cinq accès ont été marqués `#[deprecated]` : `Minutes::get`, `Moment::minutes`, `Moment::from_minutes`, `Moment::time_of_day`, `Work::minutes`. Puis `cargo check --workspace --all-targets` : **581 sites**, dédoublonnés.

| Crate | Zone | `Minutes::get` | `Moment::minutes` | `from_minutes` | `time_of_day` | `Work::minutes` | Fichiers |
|---|---|---|---|---|---|---|---|
| core | src | 84 | 10 | 12 | 3 | 27 | 21 |
| core | tests | 47 | 46 | 10 | 0 | 12 | 10 |
| automation | src | 37 | 1 | 1 | 0 | 8 | 15 |
| automation | tests | 9 | 1 | 1 | 0 | 4 | 4 |
| mspdi | src | 32 | 3 | 1 | 1 | 14 | 12 |
| mspdi | tests | 0 | 3 | 3 | 0 | 0 | 1 |
| mpp | src | 4 | 6 | 1 | 0 | 0 | 4 |
| mpp | tests | 2 | 6 | 0 | 0 | 1 | 3 |
| render | src / tests | 0 / 0 | 5 / 5 | 2 / 6 | 0 | 0 | 2 / 2 |
| ui-egui | src / tests | 6 / 28 | 1 / 5 | 1 / 3 | 2 / 0 | 6 / 5 | 9 / 18 |
| pdfimport | src / tests | 3 / 1 | 0 | 1 / 0 | 3 / 0 | 0 | 4 / 1 |
| mcp | tests | 1 | 0 | 0 | 0 | 0 | 1 |
| `tests/` (rules, corpus, generate…) | tests | 63 | 11 | 4 | 0 | 29 | 38 |

S'y ajoutent, hors de la mesure :
- les champs privés de `duration.rs` et `time.rs` (`Self(m)`, `self.0`) ;
- `CompiledCalendar` (`compiled.rs`, 18 sites mesurés, plus ses entiers internes : W(t), sommes de préfixes, plages) ;
- environ 36 constructions `Minutes::new` dans les lecteurs, à partir de dixièmes arrondis (`tenths_to_minutes`, `minutes_from_tenths`, `LinkLag`) ;
- 91 littéraux `1 440` / `10 080` / `MAX_MINUTES` dans les sources.

**Mécanique ou cas par cas (déduit) :**

| Nature | Où | Traitement |
|---|---|---|
| Différences et sommes d'instants, bornes, W(t) | core `schedule/*`, `calendar/*`, `progress/*`, `baseline/*`, render | **mécanique** : `tenths()` partout, constantes ×10 |
| Constructions de tests en minutes | 429 `Minutes::new` dans `tests/`, 98 dans les tests du cœur | **aucun changement** |
| Lectures de valeur dans les tests (`.get()` comparé à un entier) | 63 dans `tests/`, 47 core, 28 ui | mécanique (`whole_minutes()` ou comparaison de `Minutes`) |
| Lecteurs MSPDI, `.mpp`, CSV | mspdi, mpp, automation `csv`, `text` | **cas par cas** : exact, arrondi ou troncature, avec diagnostic (§ 4) |
| Sérialisation et schémas | core serde, automation `json.rs`, `view.rs`, golden schemas | **cas par cas** (§ 5) |
| Affichage | automation `text/display.rs`, `render_text.rs`, ui `format.rs`, `timescale.rs`, `rows.rs` | **cas par cas** (§ 3), avec la garde des pixels |
| Divisions qui arrondissaient à la minute | core `resource/edit.rs`, `placement.rs`, `progress/edit.rs`, `facts.rs`, `usage.rs`, `duration.rs` | **décision par règle** (§ 2) |

Estimation : environ 450 sites mécaniques, environ 130 cas par cas, une vingtaine de décisions de règle.

## 8. Plan de réalisation

Chaque étape compile et passe ses tests.

| Étape | Contenu | Comportement | Tests locaux | Ce que seule la CI verra |
|---|---|---|---|---|
| **B1 — représentation** | `Minutes`, `Moment`, `Work` en dixièmes et millièmes en interne ; nouveaux accesseurs ; `get()` / `minutes()` retirés ; tous les sites convertis ; **chaque division qui arrondissait à la minute garde un arrondi explicite à la minute** ; sérialisation inchangée (entiers, minutes) | **aucun** : tous les attendus inchangés | `cargo test -p plannercraft-core`, `-p plannercraft-tests --test rules --test corpus`, `-p plannercraft-mspdi`, `-p plannercraft-mpp`, `-p plannercraft-automation`, `-p plannercraft-render`, `-p plannercraft-mcp`, clippy par crate, wasm, xtask | captures egui, suite complète `plannercraft-ui-egui`, Windows et macOS, budget de performance |
| **B2 — contrats** | serde de `Minutes`, `Work`, `Moment` (décimale et secondes quand non entières), schémas `number`, motif des instants ; lecture tolérante | aucun pour des valeurs entières (octets identiques) ; tests de sérialisation ajoutés | automation (golden schemas régénérés : 10), mcp (rejeu `build_30`), core serde | rejeu MCP |
| **B3 — affichage et saisie** | dates tronquées à l'affichage ; durées au dixième (R-140, R-141) ; garde des pixels de l'interface ; fenêtre des différences | aucun pour des valeurs entières | automation `text`, render (golden SVG), ui-egui filtré sur `format`, `timescale` | les 111 captures egui |
| **B4 — précision** | retrait des arrondis explicites à la minute ; lecteurs exacts (MSPDI, `.mpp`) ; R-170 inverse par troncature ; marges du corpus comparées directement ; R-298 sans `sub_minute` | **changé** : attendus listés au § 9 | toutes celles de B1, plus la mesure `.mpp` (`measure_against_oracle` sur les fichiers réels et le corpus de test) | corpus MSPDI, `.mpp`, budget |
| **B5 — documents** | règle 5, ADR-0039, ADR-0005, ADR-0008, fiches de règles, `docs/features`, guide, scorecard | — | `cargo xtask rules` | — |

**PR séparées :**
- B1 seul : gros, mécanique, sans changement de comportement, donc facile à relire par les tests verts ;
- B2 + B3 ensemble : les contrats et l'affichage doivent savoir porter une fraction avant que le moteur n'en produise ;
- B4 + B5 ensemble : le changement de comportement et ses fiches dans le même commit, règle du dépôt.

Soit trois PR, cinq étapes, environ huit à douze commits.

## 9. Critères de réussite

- **Corpus MSPDI** : au moins 156 / 159. Les marges y sont relues en dixièmes : elles pourraient gagner, sans certitude.
- **Tests de règles** : tous verts sans changer un attendu en minutes entières (vérifié dès B1). Attendus qui changent en B4, avec leur raison :
  - r001 (exemples 4,8 → 5, 158,4 → 158) ;
  - r004 et r075 (`08:00:30` gardé au dixième) ;
  - r010 (158,4) ;
  - r070 (`PT0H0M30S` → 0,5 min, plus d'arrondi) ;
  - r072 (25 dixièmes → 2,5 min, plus de diagnostic) ;
  - r083 (comparaison directe) ;
  - r140 et r141 (saisies fractionnaires) ;
  - r143 (secondes) ;
  - r170 (deux fichiers : 7 min à 50 % = 3,5, ex-4 ; troncature de l'inverse) ;
  - r200, r209, r211 (`percent_of` de durée) si un exemple tombe entre deux minutes.

  La liste exacte sort du passage de B4 et chaque changement est justifié dans le commit. Environ 34 fichiers de tests de règles citent un arrondi : à relire, la plupart sans changement.
- **Captures** : les 111 captures egui et les 4 golden SVG identiques.
- **Fichiers réels `.mpp`** (aujourd'hui : 2 423 identiques, 165 à deux minutes près, 378 en écart réel dont 238 « fractions de minute ») :
  - les 403 tâches des deux premières classes sont dans 4 fichiers ;
  - attendu (déduit, à confirmer en B4) : **au moins 330** passent à « identiques », soit au moins 2 750 sur 2 966 ;
  - aucun fichier ne perd une tâche ;
  - ce qui peut rester : les artefacts flottants de MS Project (au moins 4 tâches d'`AtharvaTK1997…`, 0,1 min chacune) et les 2 cas sur 22 où la troncature ne retombe pas.
- **Classes du rapport** : la comparaison se fait au dixième.
  - « à deux minutes près » reste comme classe de sécurité ; attendu : quelques tâches seulement (artefacts flottants).
  - `mpp.gap.sub_minute` est retiré : la fraction de minute n'est plus une cause. La fenêtre des différences garde ses trois classes.
- **Corpus de test MPP14** : au moins 2 188 / 2 221 (aucune fraction n'y a été vue).
- **`.wasm`** : aujourd'hui 13 268 289 octets (release du 2026-11-15, `599d499`). Écart toléré : +1 %.
- **Temps** (CI `599d499`, médianes) : 5 000 tâches 6,9 ms ; contraintes 8,5 ; rebours 8,5 ; affectées 14,9 ; avancement 7,0 ; hiérarchie 12,6 ; lecture MSPDI 91 ms ; écriture 19 ms. Critère : aucune médiane au-delà de +10 %, budgets inchangés (50 ms).

## 10. Risques

| Risque | Tenue |
|---|---|
| Dépassement d'entier | Bornes ×10 : instants ≤ 873 086 400, durées ≤ 873 086 400, travail ≤ 10^14 (millièmes) ; produits en `i128` comme aujourd'hui ; les tests d'entrée hostile (`schedule_properties`, robustesse `.mpp`) tournent aux bornes |
| Troncature silencieuse | `get()` et `minutes()` retirés : la compilation liste tous les sites ; toute sortie entière porte sa règle dans son nom (`whole_minutes_floor`, `whole_minutes() -> Option`) |
| Déterminisme | aucun flottant dans le calcul ; `f64` lus décodés depuis leurs bits ; écriture par texte ; propriétés P13 à P23 inchangées, à relancer |
| Pixels et textes qui bougent | garde dans l'interface (chemin entier inchangé) ; golden SVG et captures en CI |
| Documents existants (journaux, sauvegardes, presse-papiers) | lecture des entiers et des dates à la minute avec le même sens ; tests de relecture d'un journal et d'une sauvegarde actuels ajoutés en B2 |
| Clients MCP | sortie entière inchangée tant que le planning est entier ; `number` au lieu de `integer` dans les schémas, signalé dans `docs/features/mcp.md` |
| Règle R-170 inverse incertaine | troncature fondée sur 20 cas sur 22 : `hypothèse` dans sa fiche ; mesure avant et après en B4 |
| Calendriers à plages fractionnaires (jamais vus) | `TimeOfDay` reste à la minute ; une plage `.mpp` non multiple de 10 dixièmes est arrondie avec un diagnostic, comme aujourd'hui |
| Taille de la relecture B1 (~580 sites) | étape sans changement de comportement, vérifiée par l'ensemble des tests existants inchangés |

## 11. Points ouverts pour le propriétaire

Réponses du propriétaire le 2026-11-15 : § 12.

1. **Durée tirée du travail : troncature (20/22) ou arrondi R-001 (18/22).** Je propose la troncature, comme hypothèse.
2. **Travail en millièmes de minute** (au lieu du dixième) : nécessaire pour `advaith14…` UID 226. Cela ajoute une seconde échelle, propre au travail.
3. **Travail dans les contrats JSON** : jusqu'à 3 décimales de minute, ou bien arrondi au dixième en sortie seulement (le moteur gardant les millièmes).

## 12. Réponses du propriétaire (2026-11-15) et état

Conception validée, décision reportée dans ADR-0039. Les réponses priment sur les propositions des § 1 à § 11 quand elles diffèrent.

1. **Durée tirée de travail ÷ unités : troncature au dixième**, au statut `hypothèse`. La fiche (R-170 inverse, en B4) donne les 22 cas observés : 20 expliqués par la troncature, 18 par l'arrondi R-001, 2 par aucune des deux ; elle dit que les artefacts flottants de MS Project ne sont pas reproduits.
2. **Travail en millièmes de minute : oui.** La seconde échelle se voit dans les noms (`Work::from_milli`, `Work::milli`) et aucune conversion travail ↔ durée ne se fait sans dire sa règle (`duration_exact`, `duration_round`, `whole_minutes`, `whole_minutes_round` ; `Work::new(Minutes)` exact).
3. **Travail dans le JSON, sans perte** : minutes avec jusqu'à 3 décimales, écrites seulement quand la valeur n'est pas entière. Une valeur entière s'écrit comme un entier JSON (jamais `480.0`, jamais `112.0`), pour les durées comme pour le travail. En entrée, un entier garde son sens ; un décimal est accepté s'il tombe sur un millième (travail) ou un dixième (durée), sinon refusé. Un contrat ne perd jamais ce que le moteur garde. Dès B2 : test d'octets identiques sur un planning en minutes entières (modèle, journal, sauvegarde, vues), entre `main` et la branche.
4. **Trois PR** : B1 seul ; B2 + B3 ; B4 + B5.

Précisions :
- le nom `Minutes` pour une valeur en dixièmes est une dette assumée, dite dans la documentation du type et dans ADR-0039 ; aucun accès ne rend des minutes sans dire sa règle ;
- interface : aucun changement d'apparence pour un planning en minutes entières ; toute capture qui change est un défaut à corriger, pas une capture à régénérer ;
- la règle 5 de `CLAUDE.md` n'est réécrite qu'en B4.

**État de B1 (branche `engine-tenths`).** Représentation livrée, sans changement de résultat ; accès retenus : `Minutes` (`new`, `from_tenths`, `tenths`, `whole_minutes`, `whole_minutes_floor`, `whole_minutes_round`, `round_to_minute`), `Moment` (`from_minutes`, `from_tenths`, `tenths`, `whole_minutes`, `minutes_floor`, `time_of_day_floor`), `Work` (`new`, `from_whole_minutes`, `from_milli`, `milli`, `duration_exact`, `duration_round`, `whole_minutes`, `whole_minutes_round`). Mesures et limites : ADR-0039.

## 13. État des lots B2-B3 (2026-11-15)

Détail et décisions : ADR-0039, « État des lots B2-B3 ». Pour chaque entrée, qui ouvre la fraction de minute :

| Entrée | B2-B3 | B4 |
|---|---|---|
| `minutes` d'une commande (durée, décalage, travail, décalage de tâches) | lu sans perte, refusé s'il n'est pas entier (`param.invalid`) | accepté au dixième (au millième pour un travail) |
| Date d'une commande (début, fin, contrainte, échéance, date d'état, date pilote, début et fin réels, replanification, `project.new`) | lue sans perte, refusée si ses secondes ne sont pas nulles | acceptée au dixième |
| Texte de durée ou de décalage (cellules, `text` des commandes, CSV, import PDF) | R-140 arrondit une seule fois à la minute (`ENTRY_RESOLUTION`), comme avant | au dixième |
| Texte de date (R-143) | secondes tronquées, à la minute | à la minute (MS Project ne saisit pas les secondes) |
| Gestes (glisser, redimensionner, décaler) | minutes entières (R-240) | minutes entières |
| Lecteurs MSPDI, `.mpp`, CSV, PDF | à la minute (diagnostics d'arrondi inchangés) | exacts (R-070, R-072, R-283…) |
| Fichier du modèle (sauvegarde automatique, journal de reprise) | relu sans perte ; une fraction écrite à la main subit les arrondis de B1 | sans arrondi |

Sorties sans perte dès B2 : modèle, journal, sauvegarde, vues, presse-papiers, diff. Restent à la minute jusqu'à B4 : écrivain MSPDI, CSV (format d'échange à la minute pour les dates), message du diagnostic `ConversionOutOfDomain`.

## 14. État final (lots B4-B5, 2026-11-15)

B1 à B5 réalisés (ADR-0039, « État des lots B4-B5 » puis « Révision de B4 après mesure »). Critères du § 9 :
- fichiers réels `.mpp` : 2 756 identiques sur 2 966 (objectif ≥ 2 750 atteint), aucun fichier ne perd de tâche, après D4 révisée (arrondi R-001 de la durée tirée du travail) et R-300 (reprise enregistrée à la minute) ;
- classe proche : 44 tâches, seuil ramené à une minute ;
- textes R-141 de quelques minutes entières changés (relecture au dixième) ;
- tests de règles : attendus changés listés, chacun avec sa règle, dans les commentaires des tests et le compte rendu du lot ;
- corpus MSPDI, corpus `.mpp` de test, captures, temps et `.wasm` : dans les critères.
