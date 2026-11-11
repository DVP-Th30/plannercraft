# Automatisation : session, lots de commandes, historique (`crates/plannercraft-automation`)

État au 2026-10-19 (fin de J4, lots L0 à L6, ADR-0012). Vue d'ensemble de la couche que partagent l'interface, la CLI, le serveur MCP et le canal de contrôle. Les règles de calcul sont dans `docs/scheduling-rules.md` (R-150 à R-165, statut `hypothèse`) ; les vues JSON et le routeur dans `docs/features/workspace.md` ; les surfaces dans `docs/features/{cli,mcp,control,ui}.md`.

## 1. Une seule API : `Session`

`Session` tient un projet ouvert, son planning, ses diagnostics, son historique et l'allocateur d'UID. Aucune entrée/sortie (compilable en wasm) : `open_bytes` lit des octets MSPDI, `save_bytes` rend des octets ; « maintenant » et la langue sont injectés.

- `new_project(NewProject { name, anchor: Start | Finish, lang }, now)` ; `open_bytes(bytes, origin, now, lang)` ; `from_project` (import CSV).
- `apply(&Batch, Commit | DryRun)` (et `apply_with` pour le diff détaillé) ; `undo`, `redo`, `history` ; `is_dirty`, `mark_saved` ; `generation` (+1 à chaque commit, undo ou redo : clé de cache de l'interface).
- `Workspace` (`docs/features/workspace.md`) est une couche fine : session facultative, chemins de fichiers, routeur `call(id, params)` vers le registre.

## 2. Lot de commandes (`Batch`)

Format versionné unique pour la CLI (`plannercraft apply`), le MCP (`apply_batch` et chaque outil d'écriture, lot d'une commande), le canal et le journal : `{"format_version":1,"lang":"fr","ops":[{"op":"task.add",…}],"label":…}`, `deny_unknown_fields`, schémas `schemars` publiés par le registre.

- Commandes (`Command`, `#[serde(tag = "op")]`) : `task.add`, `task.update`, `task.delete`, `task.move`, `task.indent`, `task.outdent`, `link.add`, `link.update`, `link.remove`, `task.set_constraint`, `task.set_deadline`, `project.set_info`, `project.set_anchor`, `project.set_status_date`, `calendar.set_exception`, `calendar.remove_exception` ; J5 : `resource.*`, `assignment.*` ; J6 : `progress.update`, `progress.reschedule`, `baseline.set`.
- Désignation (R-150) : UID (entier) ou étiquette `"$x"` posée par `"as":"$x"` sur un `task.add` plus tôt dans le même lot ; l'ID de ligne n'est jamais accepté (la CLI le convertit seule). UID alloués par `max + 1`, jamais réutilisés dans la session (R-151).
- Durées et décalages : `{"minutes","unit","text"}` en sortie ; en entrée `text` seul (« 5j », « +2j », « 50% », « +1ej ») ou `minutes` seul, les deux devant concorder s'ils sont donnés (ADR-0012 D6). Dates ISO locales sans fuseau (`2026-11-02T08:00`).

## 3. Pipeline d'un lot (ADR-0012 D2 à D4)

1. Le lot s'applique à un **clone** du projet, commande par commande (validation de chaque commande, `op_index` de la première refusée) ; au plus 10 000 opérations. Les commandes qui lisent le planning (`progress.*`, `baseline.set`) partagent le dernier recalcul du clone tant qu'il ne change pas ; le travail de recalcul d'un lot est borné (`SessionLimits::max_batch_schedule_work`, 1 000 000 tâches et affectations recalculées, `limit.exceeded` `schedule_recomputes`, R-216, ADR-0014 relecture finale (10)).
2. **Un recalcul** (`plannercraft_core::schedule`) en fin de lot.
3. **Refus sur nouvelle erreur** (R-160) : le lot est refusé (`batch.would_create_errors`, avec `op_index` et les diagnostics) si une erreur absente avant apparaît (comparée par code et sujet ; un `link.cycle` n'est nouveau que si sa composante n'était contenue dans aucune d'avant). Les avertissements sont rendus sans bloquer.
4. `Commit` : échange du clone et du projet, **delta par entité** (pré- et post-images des seules entités changées, R-164) poussé dans l'historique, `Extras` des entités modifiées invalidés (R-161), valeurs relues du fichier jetées au premier commit (R-162). `DryRun` : clone jeté, session intacte, aucune entrée d'historique.
5. Réponse (`ApplyOutcome`, vue `OutcomeView`) : diff du planning (R-163 : fin avant/après, tâches entrées ou sorties du chemin critique, nombre de tâches aux dates déplacées `changed_total` et de tâches touchées `touched_total`, 10 tâches les plus décalées, `detail` jusqu'à 1 000), UID créés, table des étiquettes, effets de bord, longueur de l'historique.

## 4. Historique (R-164, P30)

- Une entrée par lot commité, identifiants jamais réutilisés ; budget 64 Mio et 1 000 entrées, les plus anciennes abandonnées avec un compteur (`dropped`).
- `undo` applique les pré-images, `redo` les post-images : l'état obtenu est exactement l'un des deux projets comparés (P30, par construction). Un nouveau lot vide la pile de rétablissement. `history.empty` quand il n'y a rien à annuler ou rétablir ; `history.inconsistent` si un delta est inapplicable (annulation tentée sur une copie, session intacte).
- `is_dirty` suit l'identifiant de l'état enregistré, pas la profondeur de la pile.

## 5. Registre (`REGISTRY`, ADR-0012 D7)

Table triée par id (`task.add`), testée (unicité, ordre, schémas) : nom MCP (`add_task`), genre (lecture, analyse, écriture, historique, fichier), `dry_run`, disponibilité (maintenant ou jalon J5/J6), résumés FR/EN, schéma des paramètres. Le MCP génère ses outils depuis cette table ; l'interface vérifie ses raccourcis contre elle (`KEYMAP`). Liste des outils : `docs/features/mcp.md` § 2.

## 6. Preuve de bout en bout (lot L18)

Le rejeu `build_30` (`docs/features/mcp.md` § 5) traverse toute la couche : 30 tâches, 25 liens, contraintes, calendrier, `dry_run`, un lot refusé, undo/redo, enregistrement MSPDI, puis undo jusqu'au projet vide (P30 de bout en bout, `replay_build_30_undo_to_empty`). J5 : `resources_j5` (ressources, affectations). J6 (lot L10) : `tracking_j6` traverse les commandes de suivi (`progress.update`, `progress.reschedule`, `baseline.set`, `project.set_status_date`), les analyses (`analysis.explain_date`, `analysis.what_if` par `Session::simulate`, `analysis.baseline_variance`) et l'historique, jusqu'à l'aller-retour MSPDI ([`tracking.md`](tracking.md) § 3).

## 7. Tests

- Règles : `tests/rules/r150_*.rs` à `r165_*.rs` (`cargo test -p plannercraft-tests --test rules`).
- Propriétés : `session_properties.rs` (P30 : undo restaure exactement, undo + redo identique), `session_properties_l5.rs`, `session_properties_l8.rs` (P40 : `what_if` laisse la session identique, diff de `dry_run`), `task_commands_properties.rs` ; contrats et registre : `contract.rs`, `registry.rs`, `commands.rs`, `project_commands.rs`, `workspace.rs` (`crates/plannercraft-automation/tests/`).
- Surfaces : `crates/plannercraft-mcp/tests/{memory,guard,http,replay,replay_http}.rs`, `apps/plannercraft-cli/tests/{cli,mcp_replay}.rs`.

## 8. Limites

- Recalcul complet à chaque lot (pas d'incrémental) ; coût du clone mesuré en L2/L15, repli sur des pré-images instrumentées seulement si le budget de 50 ms saute.
- Ressources et affectations (J5), avancement et lignes de base (J6) : commandes disponibles ; `Session::simulate` (J6, R-234) partage avec `dry_run` l'étape `prepare` du pipeline (copie, commandes, recalcul, delta R-160, diff R-163).
- Journal write-ahead réduit au journal de rejeu `--journal` ; reprise automatique après plantage en J7.

## 9. Presse-papiers de lignes et aperçu CSV (J8, lot L3, ADR-0016 D3)

- `clip::RowClip::capture(session, uids)` : tâches sélectionnées et descendants des récapitulatives (ordre des ID), liens internes, affectations par nom de ressource, valeurs affichées pour le texte tabulé ; `to_tsv(colonnes, TextContext)` : une ligne par tâche, sans en-tête, `\n` entre les lignes, textes de R-141 à R-143 (durées « 5j », relisibles), garde de formule de R-146 sur les textes libres, tabulations et retours des cellules remplacés par une espace ; `fingerprint` (FNV-1a 64, fins de ligne normalisées) reconnaît au collage le texte émis.
- `clip::paste_batch` (R-244) et `clip::paste_text` (R-256, collage externe réduit) rendent **un lot ordinaire** (`task.add` étiquetés `$t1`…, `link.add`, `resource.add`, `assignment.add`) et un `PasteReport` (tâches, liens et affectations écartés, ressources créées) ; rien n'est écrit avant `Session::apply` (une entrée d'annulation, `dry_run` gratuit). Couper = copier puis `task.delete` en lot séparé (interface, L10). Codes : `ClipError::code()` → `paste.unrecognized`, `paste.too_large`, `task.not_found`.
- `csv::recognize(intitulé)` (public depuis L3) : sort d'un en-tête (R-144), partagé par l'import, l'aperçu et le collage. `csv::preview(octets, options)` : en-têtes et leur sort (lue, non prise en charge, inconnue, en double), 20 lignes de données au plus (cellules brutes), séparateur, langue et encodage retenus, diagnostics de l'import du **seul début du fichier** (mêmes codes et numéros de ligne que l'import ; le fichier entier n'est jamais importé pour un aperçu, le nombre de lignes n'y est donc pas contrôlé).
- Interface : `io::load_csv(octets, options, now)` ouvre un nouveau document par `document::load_project` (même chemin que l'ouverture MSPDI) et rend les diagnostics de l'import et les réglages retenus (`CsvLoaded`).
- Limites : priorité et calendrier d'une tâche copiés mais pas recollés (aucune commande ne les écrit en J8) ; collage cellule par cellule reporté ; chaque tâche manuelle collée recalcule le planning du clone (budget de recalcul du lot, comme R-253).
- Tests : `tests/rules/r244_paste_rows.rs`, `tests/rules/r256_external_paste.rs`, P52 `crates/plannercraft-automation/tests/clip_properties.rs` (1 024 cas), aperçu `csv::preview::tests` (`csv_preview_20_rows`, `csv_preview_detects_fr_semicolon`), interface `io::tests::load_csv_opens_a_new_document_with_diagnostics`.
