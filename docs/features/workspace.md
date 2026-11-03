# Espace de travail et vues JSON (`plannercraft_automation::{workspace, view, files, journal}`)

État au 2026-10-19 (lot L6 de J4, ADR-0012 D1, D6, D8). Couche commune à la CLI (`--json`), au serveur MCP et au canal de contrôle, au-dessus de `Session`. Aucune règle de calcul ici (règle 3) : des contrats de surface `C-MCP-01` à `C-MCP-03`.

## 1. `Workspace::call(id, params)`

Routeur unique : un id du registre (`REGISTRY`), des paramètres JSON, une valeur JSON ou une `ToolError { code, message, op_index, field, subject, diagnostics }`. Au plus une session ouverte ; « maintenant » et la langue sont injectés à la construction ; `with_root(dir)` confine tous les chemins (MCP HTTP) ; `resolve_path(p)` rend le chemin effectif qu'un outil de fichier visera (garde ADR-0002 des surfaces).

| Id | Paramètres | Résultat |
|---|---|---|
| `project.open`, `project.import_csv`, `project.new` | schéma du registre | `{summary, diagnostics, diagnostics_total}` ; la session ouverte est remplacée (même modifiée : l'appelant enregistre avant) |
| `project.save` | `path` facultatif (défaut : le fichier ouvert) | `{path, bytes, diagnostics, dirty}` ; la session suit le nouveau chemin, `dirty` repasse à faux |
| `project.export_mspdi` | `path` | idem, sans toucher à la session |
| `project.summary` | — | `SummaryView` |
| `task.list` | filtres, `cursor`, `limit` | `ListView` |
| `task.get` | `uid` | `TaskView` (`task.not_found`, champ `uid`) |
| `analysis.critical_path` | — | `CriticalPathView` : feuilles critiques dans l'ordre du calcul (R-035), au plus 500 |
| `analysis.explain_date` | `task`, `side`, `depth` | `ExplainView` (R-230 à R-233, module `explain`) |
| `analysis.what_if` | `ops`, `baseline`, `explain` (≤ 5), `detail` | `WhatIfView` (R-234, module `analysis`) : `Session::simulate`, session intacte, refus rendu comme donnée |
| `analysis.baseline_variance` | `baseline`, `only_varied`, `cursor`, `limit` (50, ≤ 500) | `VarianceView` (R-228, module `analysis`) |
| `render.gantt` | `from`, `to`, `zoom`, `critical_only`, `max_rows` (≤ 200, défaut 200) | `{svg, shown, hidden, truncated_days, truncated_bytes, bytes}` |
| `history.undo`, `history.redo` | — | `OutcomeView` |
| `history.list` | — | `HistoryView` |
| écritures (`task.add`…) | schéma de la commande + `dry_run`, `detail` | `OutcomeView` ; lot d'une commande |
| `batch.apply` | `ops`, `lang`, `label`, `format_version` (absent : 1 ; contrôlé avant `ops`, autre entier → `batch.unsupported_version`) + `dry_run`, `detail` | `OutcomeView` |

- Commande réservée à un jalon futur : `command.not_yet_available` (aucune depuis le lot L8 de J6) ; id inconnu ou paramètres mal formés : `param.invalid` ; sans session : `session.none` (message qui cite `open_project`, `new_project`, `import_csv`).
- Une écriture réussie hors `dry_run` crée une entrée d'historique (C-MCP-04, vérifié en L8) ; `dry_run` laisse la session intacte.
- `project.import_csv` : projet planifié depuis « maintenant » (hypothèse : MS Project prend la date du jour pour un projet neuf), document sans chemin (l'enregistrer demande `path`).
- `render.gantt` : un seul bord de plage donné ⇒ l'autre est le début ou la fin du projet ; `zoom: auto` ⇒ échelle ajustée à 1 200 px (H-R de `render`), thème clair, libellés dans la langue de l'espace.
- En wasm (pas de système de fichiers) : `project.open`, `project.import_csv` ⇒ `file.read` ; `project.save`, `project.export_mspdi` ⇒ `file.write`.

## 2. Vues

- Dates en ISO local sans fuseau (`"2026-11-02T08:00"`, R-075). Durées sous la forme unique `{"minutes","unit","text"}` (ADR-0012 D6). Une marge négative a des minutes négatives et un texte « -2j » (sortie seulement, jamais une entrée).
- `TaskView` : `uid`, `id`, `name`, `level`, `summary`, `milestone`, `duration` (saisie pour une feuille ; temps ouvré calculé, R-054, pour une récapitulative), `start`, `finish`, `total_slack`, `free_slack` (unité par défaut du projet), `critical`, `constraint` (absente pour « dès que possible »), `deadline`, `predecessors` (`{pred, kind, lag?}`, `lag` absent s'il est nul), `diagnostics` (codes), `progress { state, percent }` (J6, R-235 ; omis si non commencée). Une tâche non planifiée (inactive, récapitulative vide…) n'a ni dates ni marges. `task.get` ajoute `progress` détaillé (réels, restes, `stop`, `resume`, % travail, % physique) et `baselines` (`{number, start, finish, duration?, work?}`, absentes sans ligne).
- `SummaryView` : comptes hors lignes vides ; `critical` compte récapitulatives comprises (drapeau du moteur) ; `duration` = durée de la récapitulative du projet (R-068), à défaut le temps ouvré entre début et fin ; J6 (R-235) : `status_date` (date d'état effective, toujours présente), `percent_complete` (absent sans avancement), `baselines` (numéros présents, absent sans ligne).
- `OutcomeView` : `committed`, `labels`, `created_uids` (aussi en `dry_run`), `created` (vues des tâches créées, vide en `dry_run` : elles n'existent que dans la copie jetée, leurs dates sont dans `diff`), `diff`, `warnings`, `side_effects`, `history_len`.
- `DiffView` (R-163) : début et fin du projet avant/après, `finish_delta` = temps ouvré entre les deux fins sur le calendrier du projet (R-020, calendrier compilé du cœur ; en minutes écoulées si le calendrier ne se compile pas), `critical_entered`, `critical_left`, `changed_total`, `changed` (au plus 1 000), `truncated`.
- Clés JSON triées (objets `serde_json::Value`), donc sortie déterministe.

## 3. `task.list`

- Ordre des ID, lignes vides exclues. `cursor` : UID de la dernière tâche de la page précédente (exclusif) ; `next_cursor` absent à la fin ; `total` = tâches retenues, toutes pages comprises. Curseur inconnu : `task.not_found` (champ `cursor`).
- `limit` : 50 par défaut, 500 au plus (`limit.exceeded`), 0 refusé.
- Filtres cumulés : `critical` ; `late` (R-236 : fin planifiée après l'échéance ou après la fin de la ligne de base 0, non achevée à fin atteinte ou non commencée à début passé à la date d'état effective) ; `from`/`to` (jours inclus : fin le jour `from` ou après, début le jour `to` ou avant) ; `outline_max` ; `summary` (`true` récapitulatives, `false` feuilles) ; `resource` réservé (`command.not_yet_available`, J5). Une tâche non planifiée n'est retenue par aucun filtre de dates, ni par `critical`, ni par `late`.

## 4. Fichiers (`files`, hors wasm)

- `read_path(p, max)` : lecture arrêtée à `max + 1` octets (`file.too_large`), un dossier est refusé (`file.read`). Bornes : 256 Mio en MSPDI, 64 Mio en CSV.
- `write_atomic(p, bytes)` : temporaire `.<nom>.plannercraft-tmp-<pid>-<n>` du même dossier (création exclusive), écriture, `fsync`, renommage, `fsync` du dossier au mieux ; droits d'un fichier existant repris ; le temporaire est retiré en cas d'échec. Si `p` est un lien symbolique (CLI, MCP stdio : sans `--root`, le chemin n'est pas canonicalisé), c'est sa cible qui est remplacée, temporaire dans le dossier de la cible, et le lien reste un lien ; un lien sans cible → `file.write` (test `files_write_atomic_follows_symlink`).
- `confine(root, p)` : chemins canoniques (liens résolus) ; relatif lu depuis la racine ; fichier à créer accepté si son dossier existe dans la racine et que son nom est simple ; sinon `file.outside_root`.

## 5. Journal (`journal`)

Une ligne JSON compacte par entrée `{v, seq, tool, params, result?}` (`deny_unknown_fields`) ; `to_line` ne renvoie jamais de fin de ligne, `parse_line` refuse une forme inattendue. Écriture et `fsync` : lot L8 (`journal_file.rs`).

## 6. Contrats et tests (`crates/plannercraft-automation/tests/workspace.rs`)

- **C-MCP-01** (dates du moteur) : `view_task_dates_from_engine` compare chaque date et marge de `task.list`/`task.get` à `Schedule::scheduled`, et les dates du résumé et du chemin critique.
- **C-MCP-02** (forme figée) : `tests/golden/views/*.json` (`view_golden_summary_house`, `view_golden_outcome_add_task` ; J6 : `views_golden` de `views_j6.rs` pour `task_get_progress`, `task_list_progress`, `summary_tracking`, `baseline_variance`, `what_if`), régénérés par `PLANNERCRAFT_BLESS=1`.
- **C-MCP-03** (sortie bornée) : 500 tâches par page (`list_limit_over_500_refused`), 1 000 tâches de diff, 100 diagnostics par vue, 100 affectations par ressource et 100 ressources par tâche (J5, totaux `assignments_total`, `resources_total`), messages ≤ 2 000 caractères, 200 lignes de rendu.
- J6 (lot L10) : les vues de suivi (`task.get` avec `progress` et `baselines`, `project.summary` avec `status_date` et `percent_complete`, `analysis.baseline_variance`, `analysis.explain_date`, `analysis.what_if`) sont aussi figées de bout en bout par le rejeu `tracking_j6` (`docs/features/mcp.md` § 5), réouverture du MSPDI exporté comprise (mêmes écarts, mêmes `task.list`).
- Critère de fin du lot : `workspace_open_save_reopen_equal` (`house.xml` ouvert, modifié, enregistré, relu : projets égaux par `normalize`, R-133).
