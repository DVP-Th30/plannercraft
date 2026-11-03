# Traduire PlannerCraft

PlannerCraft est livré en français (langue de repli) et en anglais. Ce guide dit où vit chaque texte et ce qu'il faut faire pour ajouter une langue. Rien n'est traduit à la volée : chaque langue est écrite et relue dans le dépôt.

## Ce qui se traduit, et où

| Partie | Fichiers | Remarque |
|---|---|---|
| Interface (menus, fenêtres, bandeaux, barre d'état) | `crates/plannercraft-ui-egui/locales/<langue>.ftl` | Format [Fluent](https://projectfluent.org) ; une clé par variante de `TextId` (`crates/plannercraft-ui-egui/src/i18n.rs`, liste `text_ids!`) |
| Refus de commande | clés `error-<code>` des mêmes `.ftl` | Une clé par code stable du moteur (`task.not_found` → `error-task-not_found`) |
| Diagnostics à l'ouverture et erreurs de lecture MSPDI | clés `diag-<code>` des mêmes `.ftl` | `link.cycle` → `diag-link-cycle`, `mspdi.error.xml` → `diag-mspdi-error-xml` ; arguments nommés comme les champs du diagnostic (`$task`, `$calendar`, `$at`…) |
| Formats d'affichage (séparateur décimal, ordre des dates, pluriel, unités, mois, jours) | clés `fmt-*`, `unit-*`, `month-*`, `weekday-*`, `tier-*` des `.ftl` | Lues une fois par `Texts::format_labels` |
| Saisie des durées, dates et prédécesseurs | `crates/plannercraft-automation/src/text/` (`Language`, `duration.rs`, `date.rs`, `predecessors.rs`) | Abréviations d'unités (« j », « d »), types de liens (« FD », « FS ») : la lecture accepte toutes les langues livrées à la fois, l'écriture suit la langue |
| En-têtes CSV reconnus à l'import | `crates/plannercraft-automation/src/csv/header.rs` | Noms de colonnes de MS Project dans chaque langue |
| Textes du SVG et du PDF (échelle, légende, en-têtes et pieds de page) | `crates/plannercraft-render/src/request.rs` (`Lang`) et `crates/plannercraft-automation/src/render_text.rs` | Mois et jours de l'échelle, libellés de la légende |
| Messages du moteur hors interface (CLI, MCP, journaux) | `Display` des types d'erreur et de diagnostic | Restent en français : ce sont des messages pour un agent ou un journal ; l'interface les traduit par leur code |

## Ajouter une langue

1. **Fichier `.ftl`** : copier `locales/en.ftl` en `locales/<langue>.ftl` et traduire chaque valeur. Garder les clés, les variables (`{ $name }`) et les sélecteurs de pluriel (`{ $count -> [one] … *[other] … }`). Conventions : aucune espace fine insécable U+202F (absente des polices de l'interface), points de suspension « … » (U+2026), flèche écrite « -> ».
2. **Langue dans le code** : ajouter la variante à `Language` (`crates/plannercraft-ui-egui/src/i18n.rs` : `ALL`, `langid`, `from_tag`, `source`) et à `LanguagePref` (`state.rs`), puis l'entrée du menu Affichage → Langue (`lang-<code>` dans chaque `.ftl`).
3. **Formats** : renseigner dans le `.ftl` `fmt-decimal-sep`, `fmt-list-sep`, `fmt-date-order` (`dmy` ou `mdy`), `fmt-plural-rule` (`below-two` pour le français, `exactly-one` pour l'anglais ; une règle nouvelle s'ajoute à `PluralRule` dans `format.rs`).
4. **Saisie** : ajouter la langue à `plannercraft_automation::text::Language` ; ses abréviations d'unités et de types de liens dans `duration.rs` et `predecessors.rs`. **Les codes d'unités sont à sourcer** : prendre ceux que la version localisée de MS Project accepte (page d'aide Microsoft de la langue, relue), jamais ceux d'un autre logiciel ; une abréviation qui entre en conflit avec une autre langue (même lettre, autre unité) se signale et se tranche dans `docs/decisions.md`.
5. **CSV** : ajouter les en-têtes de colonnes de MS Project dans la langue (`csv/header.rs`), relus sur un export réel ou sur la documentation Microsoft.
6. **SVG et PDF** : ajouter la variante à `plannercraft_render::Lang` et ses textes (mois, jours, légende).
7. **Contrôles** : `cargo xtask text-ids` (chaque clé présente dans chaque `.ftl`, chaque code de diagnostic avec son message) et `cargo test -p plannercraft-ui-egui` (mêmes variables dans chaque langue, aucun message vide, pluriels, typographie).

## Vérifier qu'un texte tient à l'écran

La pseudo-langue de test (`Texts::pseudo`, `App::use_pseudo_texts`) remplace chaque message par une version accentuée, allongée de 40 % et encadrée de crochets (« [Fílé··] »). Les tests `pseudo_main_view_no_truncation` et `pseudo_dialogs_no_truncation` (`crates/plannercraft-ui-egui/tests/ui/pseudo.rs`) dessinent la vue principale, ses menus, le volet latéral et les fenêtres à 1280 × 720 et échouent si un libellé est tronqué ou rogné. Une langue plus longue que l'anglais de plus de 40 % (allemand, finnois) demande de les relancer avec un facteur plus grand.

Une fenêtre nouvelle s'ajoute à `pseudo_dialogs_no_truncation` dans le lot qui la crée.
