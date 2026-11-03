# Suivi : avancement, date d'état, lignes de base, analyse (J6)

État au 2026-10-20 (lots L0 à L10 de J6, ADR-0014, cahier `docs/plans/j6.md`). Une seule fiche pour les trois domaines du suivi (arbitrage du § 0 du cahier). Règles détaillées : `docs/scheduling-rules.md`, sections « Avancement (R-200 à R-219) », « Lignes de base (R-220 à R-228) », « Analyse (R-230 à R-236) » ; toutes au statut `hypothèse` sauf mention contraire dans leur fiche. Comme partout, aucune date n'est calculée hors du moteur (règle 3) : les vues, les résumés de `explain_date` et les textes de la CLI recopient des valeurs du `Schedule`.

## 1. Modèle

- **Vérité = réels.** Une tâche porte une durée D, une durée réelle, un début réel, une fin réelle, un % physique et, pour une feuille en cours, une reprise saisie (`Progress::resume`) ; une affectation porte son travail W et son travail réel. Les restes sont **dérivés** : reste = D − réel, W − réel (`progress::remaining_duration`, `progress::remaining_work`, ADR-0014 D4) ; saisir un reste change D ou W. `Stop` n'est jamais stocké : il découle du début réel et de la durée réelle (R-204).
- **États** (`progress::facts`, R-200) : non commencée, commencée (début réel saisi ou implicite, `progress.actual_start_missing`), achevée (fin réelle ; 100 %).
- **Options de la date d'état** (`ProjectOptions::progress`, R-218) : `SplitsInProgressTasks` (fractionnement d'une tâche en cours, défaut vrai) est calculée ; `TaskUpdatesResource` n'est suivie qu'à vrai, son défaut (à faux, le travail réel est quand même réparti, avec `progress.option_unsupported`, info) ; les quatre options `Move*` sont lues et réécrites, seule la valeur 0 est calculée (1 ⇒ `progress.option_unsupported`, info).
- **Date d'état** (R-034, R-219) : saisie, sinon « maintenant » injecté. Elle ne change **aucune** date du planning (P37) : elle sert de défaut à `reschedule_remaining`, de référence au filtre `late` et à l'affichage.
- **Lignes de base** : 11 lignes (0 à 10, `model/baseline.rs`), posées par `baseline.set` à partir des valeurs du moteur (début, fin, durée, travail des tâches, des affectations et des ressources), jamais à partir des champs saisis (ADR-0014 D6).

## 2. Règles par domaine

| Domaine | Règles | Ce que fait PlannerCraft |
|---|---|---|
| Saisie (édition pure) | R-200 à R-202, R-211 à R-213 | `progress::edit::apply` équilibre % ↔ durée réelle ↔ reste, début réel implicite, fin réelle à 100 %, travail réel réparti au prorata de W (saisi seul : il fixe la durée réelle), % physique sans effet ; refus `progress.summary_not_editable`, `progress.inactive`, `progress.invalid_order`, `param.conflict`, et `progress.duration_below_actual` pour un `task.update duration` sous la durée réelle ; jalon commencé ramené à 100 % (`progress.milestone_rounded`, avertissement) |
| Moteur | R-203 à R-210 | Feuille achevée = épingle réelle (dates brutes, TS = 0, non critique, ne plafonne pas l'amont) ; feuille en cours = une coupure : partie réelle [début réel, `Stop`], reste placé depuis `Resume` (bornes du réseau si fractionnement permis) ; marges d'une tâche en cours (R-206, hypothèse la plus fragile : `hypothesis: true` dans `explain_date`) ; agrégats d'une récapitulative et de l'UID 0 (R-210) |
| MSPDI | R-214, R-215, R-218, R-226 | Lecture de la reprise, des options et des restes (restes incohérents remplacés, `mspdi.progress_inconsistent`) ; écriture de l'avancement (réels et restes des récapitulatives et de l'UID 0 compris), de `Stop` et des réels d'affectation par le moteur, `Resume` = reprise saisie seulement ; écarts de tâche et de ressource en minutes × 1000 |
| Commandes | R-216, R-217, R-219, R-227, R-250 (J7) | `update_progress`, `reschedule_remaining` (reprise posée sur une tâche en cours, SNET sur une tâche non commencée qui commence avant `after`), `set_baseline` (`baseline.exists` sans `overwrite`), `clear_baseline` (J7, lot L5 : projet entier, ressources comprises, ou sélection de tâches et de leurs affectations ; `Extras` `Baseline#n/` jetés ; `baseline.nothing_to_clear` si rien à effacer) |
| Lignes de base | R-220 à R-225 | Valeurs copiées, portée (projet ou sélection), cumul `to_all_summaries` / `from_subtasks`, remplacement (coût recopié, `baseline.cost_not_computed`), écarts = valeur − ligne, en minutes ouvrées signées |
| Analyse | R-228, R-230 à R-236 | `baseline_variance` (paginé, trié par écart de fin), `explain_date` (pilote, recalage, avancement, reprise et sa cause, côté au plus tard, chaîne bornée), résumés par gabarit, `what_if` (simulation sans effet, P40), vues, filtre `late` |

## 3. Scénario chiffré `tracking_j6`

Rejoué en CI (`tests/mcp/scenarios/tracking_j6.jsonl`, 53 appels, `--now 2026-11-02T08:00`, `fr`) en mémoire, en stdio et en HTTP ; MSPDI attendu `tests/mcp/expected/tracking_j6.xml`. Calendrier Standard (lun-ven 08:00-12:00, 13:00-17:00, 1 j = 480 min), aucun jour chômé.

**Planning initial** (un `apply_batch`, 20 tâches) : « Extension de bureaux », début lun 02/11/2026 08:00, fin mer 16/12/2026 17:00.

| UID | Tâche | Durée | Liens entrants | Ligne de base 0 |
|---|---|---|---|---|
| 1 | **Gros œuvre** | | | 02/11 – 13/11 |
| 2 | Terrassement (T1) | 2 j | | 02/11 – 03/11 |
| 3 | Maçonnerie (T2), Maçon | 10 j | SS 2 | 02/11 – 13/11 |
| 4 | Réseaux enterrés (T3) | 3 j | FS 2 | 04/11 – 06/11 |
| 5 | Fondations du local technique (T4), Maçon | 3 j | FS 4 | 09/11 – 11/11 |
| 6 | Gros œuvre terminé (jalon) | 0 j | FS 3, FS 5 | 13/11 17:00 |
| 7 | **Clos couvert et second œuvre** | | | 16/11 – 04/12 |
| 8 | Charpente et couverture (T5) | 4 j | FS 3 | 16/11 – 19/11 |
| 9 | Menuiseries extérieures (T6) | 3 j | FS 8 | 20/11 – 24/11 |
| 10 | Cloisons et doublages, Plaquiste | 5 j | FS 9 | 25/11 – 01/12 |
| 11 | Électricité | 4 j | FS 9 | 25/11 – 30/11 |
| 12 | Plomberie | 3 j | FS 9 | 25/11 – 27/11 |
| 13 | Ventilation | 2 j | FS 11 | 01/12 – 02/12 |
| 14 | Bandes et enduits, Plaquiste | 3 j | FS 10 | 02/12 – 04/12 |
| 15 | **Finitions** | | | 07/12 – 16/12 |
| 16 | Peinture | 4 j | FS 14, 13, 12 | 07/12 – 10/12 |
| 17 | Revêtements de sol | 3 j | FS 16 | 11/12 – 15/12 |
| 18 | Raccordement au réseau public, SNET 14/12 08:00 | 1 j | FS 6 | 14/12 |
| 19 | Nettoyage de fin de chantier | 1 j | FS 17, 18 | 16/12 |
| 20 | Réception des travaux (jalon) | 0 j | FS 19 | 16/12 17:00 |

Ressources : Maçon (200 %), Plaquiste, Électricien (non affecté). Chemin critique : 2, 3, 8, 9, 10, 14, 16, 17, 19, 20 (et les phases).

**Ligne de base 0** : `set_baseline` puis `baseline_variance` : tous les écarts à 0 ; un second `set_baseline` sans `overwrite` est refusé (`baseline.exists`, 27 éléments : 20 tâches, 4 affectations, 3 ressources). Le fichier exporté à cette étape (`tests/mcp/expected/tracking_j6_baseline0.xml`) est le point de départ de l'essai réel n° 3 (`docs/features/mcp.md` § 6).

**Avancement au ven 13/11/2026 17:00** (`set_status_date`), écarts chiffrés à la main puis comparés au moteur (`replay_tracking_j6_memory`) :

| Saisie | Effet (moteur) | Écarts à la ligne 0 |
|---|---|---|
| T1 100 % | achevée lun 02/11 08:00 – mar 03/11 17:00 ; sort du chemin critique (`critical_left` = [2], S20) | 0 |
| T2 début réel mar 03/11 08:00, 50 % | réel 5 j (2 400 min) : mar 03/11 – lun 09/11 17:00 (`Stop`) ; reste 5 j depuis mar 10/11 (`Resume`) : fin lun 16/11 17:00 | début + 1 j (480), fin + 1 j |
| T3 fin réelle jeu 05/11 17:00 | début réel implicite = début planifié mer 04/11 08:00, D = 2 j mesurés, achevée | durée − 1 j, fin − 1 j |
| T4 durée réelle 2 j et travail réel 16 h | commencée ven 06/11 (début réel implicite, tiré de la fin réelle de T3) ; `Stop` lun 09/11 17:00, reste 1 j / 8 h mar 10/11 | début − 1 j, fin − 1 j |

Suite : jalon 6 + 1 j (fin de T2), T5 à T20 décalés de + 1 j (fin de T2 lun 16/11 au lieu de ven 13/11 ; 18, à sa SNET, ne bouge pas), fin du projet jeu 17/12/2026 17:00 : **+ 1 j** (ligne du projet déduite des tâches de premier niveau, `derived: true`, lot L8). % achevé du projet : Σ réels / Σ D des feuilles = (960 + 2 400 + 960 + 960) / 24 000 = 5 280 / 24 000 = **22 %**.

`explain_date` sur T5 (UID 8) : début mar 17/11 08:00 = fin de la Maçonnerie (lun 16/11 17:00, lien FS) reportée au premier instant ouvré ; chaîne : Maçonnerie, pilotée par son début réel (`driver: actual`).

**Refus et avertissement** : avancement sur la phase 1 ⇒ `progress.summary_not_editable` ; fin réelle avant le début réel ⇒ `progress.invalid_order` ; jalon 6 à 50 % en `dry_run` ⇒ ramené à 100 %, `progress.milestone_rounded`.

**Hors séquence** : T6 (Menuiseries) début réel jeu 12/11 08:00, durée réelle 1 j, alors que T5 n'est pas commencée ; `explain_date` T6 : `actual` (12/11 08:00), puis `resume` : partie réelle jusqu'au jeu 12/11 17:00, reprise lun 23/11 08:00, cause = lien FS de la Charpente (UID 8) ; `hypothesis: true` (R-206). La fin du projet passe à mer 16/12 (le reste de T6, 2 j, finit un jour plus tôt).

**Replanification au ven 27/11/2026 17:00** : `reschedule_remaining` en `dry_run` (rien d'appliqué, même diff) puis appliqué : reprise posée sur les tâches en cours (T2 et T4 reprennent lun 30/11 08:00 ; le reste de T6 reste tiré par la Charpente), SNET 27/11 17:00 sur les tâches non commencées qui commençaient avant (UID 6, 8, 10, 11, 12) ; T2 finit ven 04/12, T5 lun 07/12 – jeu 10/12, reste de T6 ven 11/12 – lun 14/12, réception mar 05/01/2027 17:00 : **+ 14 j** sur la ligne 0. `explain_date` T2 : reprise lun 30/11 08:00, cause « reprise saisie ».

**`what_if`** (ligne de base 0, `explain [20]`), `project_summary` identique avant et après :
- Cloisons et doublages 5 j → 8 j : écart de fin du projet + 17 j (+ 3 j sur les + 14 j) ;
- second plaquiste (ressource « Plaquiste 2 », Cloisons pilotées par l'effort) : Cloisons 2,5 j, le chemin passe par Électricité puis Ventilation : écart + 12 j (− 2 j seulement, et non − 2,5 j).

**Ligne de base 1** sur la sélection {Cloisons, Bandes et enduits}, `to_all_summaries` : la phase 2 reçoit le cumul (15/12 – 24/12) ; `baseline_variance` contre 1 : écarts nuls sur les trois, autres tâches sans ligne (`baseline: null`). `list_tasks late` : Maçonnerie et ses successeurs, ni T1 (achevée), ni T3 (achevée en avance).

**Historique et fichiers** : 3 `undo`, 3 `redo` (historique 11 → 8 → 11) ; `save_project`, `export_mspdi`, `open_project` sur l'export : mêmes écarts (lignes 0 et 1), même `list_tasks`, même résumé hors chemin, état modifié et historique (`replay_tracking_j6_reopen_same_variances`).

## 4. Tests

- Règles : `tests/rules/r2NN_*.rs` (R-200 à R-236), `cargo xtask rules` (aucune règle sans test).
- Propriétés : P37 (avancement nul ⇒ rien ne change ; date d'état sans effet), P38 (achevée), P39 (édition), P40 (`what_if`), P41 (`explain`), P42 (lignes de base), P43 (aller-retour MSPDI), P44 (en cours).
- Corpus : A7 11/11 (UID 2 et 3 achevées, UID 0 agrégé), ressources A7 7/7 ; les cinq fichiers s'écrivent avec le moteur (aucun manque restant).
- Bout en bout : `replay_tracking_j6_memory`, `replay_tracking_j6_saved_mspdi_matches`, `replay_tracking_j6_reopen_same_variances` (`crates/plannercraft-mcp/tests/replay.rs`), `replay_tracking_j6_stdio` (`apps/plannercraft-cli/tests/mcp_replay.rs`), `replay_tracking_j6_http` (`crates/plannercraft-mcp/tests/replay_http.rs`) ; essai réel n° 3 : `replay_claude_run_3` (journal du 2026-10-20 versionné, faits clés vérifiés, `mcp.md` § 6).
- Performance (job `perf`, release) : `5000_progress` et `5000_assigned` sous 50 ms (médiane), `deep_hierarchy` et `deep_hierarchy_progress` sous 30 ms, `5000_assigned_leaves` sous 250 ms, `perf_baseline_variance_page` sous 50 ms ; mesuré à la relecture finale (serveurdvp, charge ≈ 1, médiane de 5 mesures) : 19,5 ms, 35,8 ms, 25,7 ms, 27,7 ms, 108,1 ms, 17,6 ms (`5000_assigned_progress` 34,8 ms ; lot de 200 `progress.update` sur 5 000 tâches 3,7 s).

## 5. Limites

- Aucune règle de suivi n'est vérifiée contre MS Project hors des cas du corpus (A7 : deux tâches achevées, une récapitulative manuelle achevée, l'UID 0) ; aucune tâche en cours au corpus : R-204 à R-207 reposent sur des pages d'aide et des hypothèses.
- `assignment.update work` sous le travail réel laisse un avancement stocké incohérent (ADR-0014, lot L6 point 10) : lisible (`progress::facts` borne le réel à D), non refusé. `task.update duration` sous la durée réelle est refusée (`progress.duration_below_actual`, R-213, arbitrage du 2026-10-20).
- Ligne de base du projet d'un projet créé par PlannerCraft : déduite des tâches de premier niveau dotées de la ligne (`derived`), sans créer de récapitulative du projet (arbitrage du propriétaire du 2026-10-20) ; sur une ligne posée sur une sélection, elle ne couvre que ces tâches (ligne 1 du scénario : début du projet − 31 j).
- **Reprise relue d'un fichier MS Project** (relecture finale J6, J6-F1) : `Resume` d'une feuille en cours est relu comme reprise saisie (R-214), donc comme plancher ; MS Project y écrit la reprise calculée. Dans un fichier MS Project, le reste d'une tâche hors séquence dont la reprise est tirée par un lien ne peut plus avancer si ce prédécesseur raccourcit, et `explain_date` cite « reprise saisie ». PlannerCraft n'écrit que la reprise saisie (R-215) : ses propres fichiers font l'aller-retour sans changer le comportement du plan (`r215_resume_round_trip_keeps_plan_behaviour`).
- `mspdi.write.rounded` à l'export du scénario : % achevés non entiers (66,67 %) arrondis à l'entier dans `PercentComplete` (le schéma n'admet que des entiers), durée réelle de l'UID 0 arrondie à la minute (A7 : 773 contre 772,68 min).
- Calendrier : aucun jour férié dans le scénario (lisibilité des écarts) ; les règles valent sur tout calendrier (R-113).

## 6. Hors J6

« Mettre à jour les tâches » (0-100 % à la date d'état) ; réel par période ; options `Move*` à 1 ; fractionnements multiples d'une tâche en cours ; avancement saisi sur une récapitulative ; plans intermédiaires (`Start1`-`Start10`)  ; bornes concurrentes dans `explain_date` ; coûts et valeur acquise ; ~~barres de ligne de base et colonne % dans l'interface (J7)~~ (faits en J7 : colonnes de suivi au lot L6, Gantt de suivi au lot L7, fenêtre « Ligne de base » au lot L12, scénario `e2e_daily_use` au lot L16 ; `docs/features/ui.md`) ; `plannercraft variance` en CLI.
