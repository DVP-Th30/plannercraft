//! Scénario de bout en bout de l'usage quotidien (J7, lot L16, vérification
//! 5) : un seul test, `e2e_daily_use`, qui enchaîne sur l'exemple les gestes
//! livrés par les lots L1 à L15 — navigation et saisie au clavier,
//! « Informations sur la tâche », gestes dans le Gantt, Gantt de suivi,
//! feuille et utilisation des ressources, temps de travail, ligne de base —
//! puis annule tout (projet égal à l'ouverture), rétablit tout, enregistre
//! par un `FileService` en mémoire et relit les octets écrits.
//!
//! Les touches sont de vrais événements egui (comme l'utilisateur) ; les vues
//! et les fenêtres sont ouvertes par le réducteur (`Action`), comme le font
//! les menus. Aucune date n'est calculée ici : les positions du Gantt viennent
//! de `TimeAxis` (sens date → pixel) et les attendus du moteur.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::{
    BaselineNumber, Constraint, LinkType, Minutes, Moment, Project, Task, TaskUid, Units,
};
use plannercraft_ui_egui::action::DialogKind;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::document;
use plannercraft_ui_egui::grid;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::resource_rows::{COLUMNS, ResourceColumn};
use plannercraft_ui_egui::rows::RowKind;
use plannercraft_ui_egui::{Action, App, GanttStyle, TimeScaleZoom, ViewKind};

use crate::files::{SaveReply, sim};
use crate::{fr, now, open_demo, screen_axis};

/// Projet du document.
fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

fn task(app: &App, uid: TaskUid) -> &Task {
    project(app).tasks.get(uid).unwrap()
}

/// (entrées annulables, entrées rétablissables) de l'historique.
fn history(app: &App) -> (usize, usize) {
    let history = app.document.as_ref().unwrap().session.history();
    (history.entries.len(), history.redo.len())
}

/// Aucun message au bandeau.
fn quiet(app: &App) {
    assert!(app.messages().is_empty(), "{:?}", app.messages());
}

/// Indice de la ligne du curseur de la table (ligne vide : nombre de tâches).
fn cursor_row(app: &App) -> usize {
    let rows = app.rows().unwrap();
    match app.view().cursor.row {
        RowRef::Item(uid) => rows.index_of(uid).unwrap(),
        RowRef::Blank => rows.len(),
    }
}

fn uid_at(app: &App, i: usize) -> TaskUid {
    app.rows().unwrap().row(i).unwrap().uid
}

/// Libellé d'accessibilité de la ligne `i` de la table des tâches.
fn row_label(app: &App, i: usize) -> String {
    let rows = app.rows().unwrap();
    let row = rows.row(i).unwrap();
    Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[
            ("id", Arg::Count(i64::from(row.id))),
            ("name", Arg::Str(rows.text(i, Column::Name).to_owned().into())),
        ],
    )
}

/// Ordonnée écran du centre de la ligne `i`.
fn row_y(harness: &Harness<'static, App>, i: usize) -> f32 {
    let label = row_label(harness.state(), i);
    harness.get_by_label(&label).rect().center().y
}

/// Abscisses écran du début et de la fin de la barre de la ligne `i`.
fn bar_x(harness: &Harness<'static, App>, i: usize) -> (f32, f32) {
    let app = harness.state();
    let (start, finish) = app.rows().unwrap().row(i).unwrap().bar.unwrap();
    let axis = screen_axis(app);
    (axis.x(start), axis.x(finish))
}

/// Appuie sur `key` avec `modifiers`, puis laisse l'interface se stabiliser.
fn press(harness: &mut Harness<'static, App>, modifiers: Modifiers, key: Key) {
    harness.key_press_modifiers(modifiers, key);
    harness.run();
}

/// Frappe `text`.
fn type_text(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// Clic simple au point `p`.
fn click_at(harness: &mut Harness<'static, App>, p: Pos2) {
    harness.event(Event::PointerMoved(p));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
}

/// Clique sur l'unique widget portant `label`.
fn click_label(harness: &mut Harness<'static, App>, label: &str) {
    crate::click(harness, label);
}

/// Remplace le texte du champ nommé `label` par `text`.
fn retype(harness: &mut Harness<'static, App>, label: &str, text: &str) {
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, label)
        .focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.key_press(Key::Backspace);
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// Libellé « champ, ligne n » d'une ligne de fenêtre.
fn row_field(field: TextId, row: usize) -> String {
    Texts::new(Language::Fr).get_args(
        TextId::TaskInfoRowField,
        &[
            ("column", Arg::Str(fr(field).into())),
            ("count", Arg::Count(i64::try_from(row).unwrap())),
        ],
    )
}

/// Presse en `from`, glisse en quatre pas jusqu'à `to`, lâche.
fn drag(harness: &mut Harness<'static, App>, from: Pos2, to: Pos2) {
    harness.hover_at(from);
    harness.step();
    harness.drag_at(from);
    harness.step();
    for k in 1..=4_u8 {
        let t = f32::from(k) / 4.0;
        harness.hover_at(from.lerp(to, t));
        harness.step();
    }
    harness.drop_at(to);
    harness.run();
}

/// Première feuille non commencée d'au moins 3 jours : (ligne, UID).
fn unstarted_leaf(app: &App) -> (usize, TaskUid) {
    let rows = app.rows().unwrap();
    (0..rows.len())
        .find_map(|i| {
            let row = rows.row(i)?;
            let (start, finish) = row.bar?;
            let long = start.minutes_until(finish) >= Minutes::new(3 * 1_440);
            let t = task(app, row.uid);
            let fresh = t.progress.percent_complete.get() == 0 && t.progress.actual_start.is_none();
            (row.kind == RowKind::Leaf && long && fresh).then_some((i, row.uid))
        })
        .expect("aucune feuille non commencée de 3 jours")
}

/// Zoom Jour, ligne `i` visible, début de sa barre à 150 points du bord
/// gauche du Gantt.
fn show_row(harness: &mut Harness<'static, App>, i: usize) {
    harness.state_mut().dispatch(Action::SetZoom(TimeScaleZoom::Day));
    harness.state_mut().dispatch(Action::ScrollToRow(i));
    harness.run();
    let app = harness.state();
    let (start, _) = app.rows().unwrap().row(i).unwrap().bar.unwrap();
    let dx = screen_axis(app).x(start) - app.view().last_gantt_left - 150.0;
    harness.state_mut().dispatch(Action::ScrollGanttX(dx));
    harness.run();
}

fn scheduled_start(app: &App, uid: TaskUid) -> Moment {
    app.document
        .as_ref()
        .unwrap()
        .schedule()
        .scheduled(uid)
        .unwrap()
        .start
}

/// Libellé d'accessibilité de la ligne `i` de la feuille des ressources
/// (ligne vide au-delà).
fn res_row_label(app: &App, i: usize) -> String {
    let rows = &app.view().resources.rows;
    match rows.row(i) {
        Some(row) => app.texts().get_args(
            TextId::ResRowLabel,
            &[
                ("id", Arg::Count(i64::try_from(row.id).unwrap())),
                (
                    "name",
                    Arg::Str(rows.text(i, ResourceColumn::Name).to_owned().into()),
                ),
            ],
        ),
        None => app.texts().get(TextId::ResBlankRowLabel),
    }
}

/// Clic sur la cellule (ligne `i`, `column`) de la feuille des ressources.
fn res_click_cell(harness: &mut Harness<'static, App>, i: usize, column: ResourceColumn) {
    let rect = harness.get_by_label(&res_row_label(harness.state(), i)).rect();
    let (left, width) = grid::column_span(&COLUMNS, rect.left(), column).unwrap();
    click_at(harness, pos2(left + width / 2.0, rect.center().y));
}

/// Frappe `text` dans la feuille des ressources (saisie ouverte), Entrée.
fn res_type_enter(harness: &mut Harness<'static, App>, text: &str) {
    type_text(harness, text);
    assert!(
        harness.state().view().resources.edit.editing.is_some(),
        "saisie de la feuille ouverte"
    );
    press(harness, Modifiers::NONE, Key::Enter);
}

/// Ligne de base `n` posée sur au moins une tâche.
fn has_baseline(app: &App, n: u8) -> bool {
    let number = BaselineNumber::new(n).unwrap();
    project(app)
        .tasks
        .iter()
        .any(|t| t.baselines.contains_key(&number))
}

/// Vérification 5 du lot L16 : usage quotidien de bout en bout.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "un seul scénario linéaire, lisible de haut en bas"
)]
fn e2e_daily_use() {
    let mut s = sim(SaveReply::Ok);
    let harness = &mut s.harness;
    open_demo(harness);
    let initial = project(harness.state()).clone();
    assert_eq!(cursor_row(harness.state()), 0);
    assert_eq!(harness.state().view().cursor.column, Column::Name);

    // 1. ↓ ×3, Origine, Tab ×2 (ID → Nom → Durée), F2, Ctrl+A, « 3j », Entrée.
    for _ in 0..3 {
        press(harness, Modifiers::NONE, Key::ArrowDown);
    }
    assert_eq!(cursor_row(harness.state()), 3);
    press(harness, Modifiers::NONE, Key::Home);
    press(harness, Modifiers::NONE, Key::Tab);
    press(harness, Modifiers::NONE, Key::Tab);
    assert_eq!(harness.state().view().cursor.column, Column::Duration);
    let edited = uid_at(harness.state(), 3);
    assert_eq!(
        harness.state().rows().unwrap().row(3).unwrap().kind,
        RowKind::Leaf
    );
    press(harness, Modifiers::NONE, Key::F2);
    assert!(
        harness.state().view().edit.editing.is_some(),
        "F2 ouvre la saisie"
    );
    // F2 garde le texte en place (« 1 jour ») : tout sélectionner avant de taper.
    press(harness, Modifiers::COMMAND, Key::A);
    type_text(harness, "3j");
    press(harness, Modifiers::NONE, Key::Enter);
    let app = harness.state();
    quiet(app);
    assert!(app.view().edit.editing.is_none(), "Entrée valide");
    assert_eq!(task(app, edited).duration.minutes().get(), 3 * 480);
    assert_eq!(history(app), (1, 0));

    // 2. Maj+Entrée : une ligne plus haut, aucune saisie (H-UI-13).
    let row_after_enter = cursor_row(harness.state());
    press(harness, Modifiers::SHIFT, Key::Enter);
    let app = harness.state();
    assert_eq!(cursor_row(app), row_after_enter - 1);
    assert!(app.view().edit.editing.is_none());

    // 3. Maj+F2, renommer, OK : un seul lot.
    let renamed = uid_at(harness.state(), cursor_row(harness.state()));
    press(harness, Modifiers::SHIFT, Key::F2);
    assert_eq!(
        harness.state().view().dialog.as_ref().map(|d| d.kind),
        Some(DialogKind::TaskInformation(renamed))
    );
    retype(harness, &fr(TextId::TaskInfoName), "Tâche revue");
    click_label(harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    quiet(app);
    assert!(app.view().dialog.is_none(), "fenêtre fermée");
    assert_eq!(task(app, renamed).name, "Tâche revue");
    assert_eq!(history(app), (2, 0));

    // 4. Glisser une barre de +2 jours : contrainte « début au plus tôt »
    // (R-240), un seul lot.
    let (i, moved) = unstarted_leaf(harness.state());
    show_row(harness, i);
    let day = harness.state().state.zoom.px_per_day();
    let start = scheduled_start(harness.state(), moved);
    let y = row_y(harness, i);
    let (x0, x1) = bar_x(harness, i);
    let from = pos2(f32::midpoint(x0, x1), y);
    drag(harness, from, from + egui::vec2(2.0 * day, 0.0));
    let app = harness.state();
    quiet(app);
    let expected = start.checked_add(Minutes::new(2 * 1_440)).unwrap();
    assert_eq!(
        task(app, moved).constraint,
        Constraint::StartNoEarlierThan { date: expected }
    );
    assert_eq!(history(app), (3, 0));

    // 5. Tracer un lien : de la barre déplacée vers la barre d'une ligne
    // suivante (R-243 : lâcher sur la barre), visible à l'écran : zoom Mois
    // pour que les barres voisines tiennent dans la largeur du Gantt.
    harness
        .state_mut()
        .dispatch(Action::SetZoom(plannercraft_ui_egui::TimeScaleZoom::Month));
    harness.run();
    let (left, width) = {
        let view = harness.state().view();
        (view.last_gantt_left, view.last_gantt_width)
    };
    let target = (i + 1..i + 12)
        .find(|&j| {
            let app = harness.state();
            app.rows().unwrap().row(j).is_some_and(|row| {
                row.kind == RowKind::Leaf
                    && !task(app, row.uid)
                        .predecessors
                        .iter()
                        .any(|l| l.predecessor == moved)
                    && {
                        let (tx0, tx1) = bar_x(harness, j);
                        tx1.min(left + width - 4.0) - tx0.max(left + 4.0) > 6.0
                    }
            })
        })
        .expect("aucune ligne cible");
    let target_uid = uid_at(harness.state(), target);
    let (x0, x1) = bar_x(harness, i);
    let from = pos2(f32::midpoint(x0, x1), row_y(harness, i));
    // Lâcher sur la partie visible de la barre de la cible (lâché hors
    // d'une barre, le geste reste un déplacement).
    let (tx0, tx1) = bar_x(harness, target);
    let x = f32::midpoint(tx0.max(left + 4.0), tx1.min(left + width - 4.0));
    let to = pos2(x, row_y(harness, target));
    drag(harness, from, to);
    let app = harness.state();
    quiet(app);
    assert!(
        task(app, target_uid)
            .predecessors
            .iter()
            .any(|l| l.predecessor == moved && l.kind == LinkType::FinishToStart),
        "{:?}",
        task(app, target_uid).predecessors
    );
    assert_eq!(history(app), (4, 0));

    // 6. Gantt de suivi.
    harness
        .state_mut()
        .dispatch(Action::SetGanttStyle(GanttStyle::Tracking));
    harness.run();
    assert_eq!(harness.state().state.gantt_style, GanttStyle::Tracking);
    quiet(harness.state());

    // 7. Feuille des ressources : « Grutier » sur la ligne vide, capacité 50 %.
    harness
        .state_mut()
        .dispatch(Action::SetView(ViewKind::ResourceSheet));
    harness.run();
    assert_eq!(harness.state().state.view, ViewKind::ResourceSheet);
    assert!(
        project(harness.state()).resources.is_empty(),
        "l'exemple n'a aucune ressource"
    );
    res_click_cell(harness, 0, ResourceColumn::Name);
    res_type_enter(harness, "Grutier");
    let app = harness.state();
    quiet(app);
    assert_eq!(project(app).resources.len(), 1);
    assert_eq!(app.view().resources.rows.text(0, ResourceColumn::Name), "Grutier");
    assert_eq!(history(app), (5, 0));
    res_click_cell(harness, 0, ResourceColumn::MaxUnits);
    res_type_enter(harness, "50");
    let app = harness.state();
    quiet(app);
    let crane = project(app).resources.iter().next().unwrap();
    assert_eq!(crane.name, "Grutier");
    assert_eq!(crane.max_units, Units::new(5_000).unwrap());
    assert_eq!(history(app), (6, 0));

    // 8. Utilisation des ressources : la vue s'affiche sans message.
    harness
        .state_mut()
        .dispatch(Action::SetView(ViewKind::ResourceUsage));
    harness.run();
    assert_eq!(harness.state().state.view, ViewKind::ResourceUsage);
    quiet(harness.state());

    // 9. Temps de travail : exception nommée « Pont » sur le calendrier du projet.
    harness.state_mut().dispatch(Action::SetView(ViewKind::Gantt));
    harness.run();
    let calendar = project(harness.state()).calendar;
    let exceptions_before = project(harness.state())
        .calendars
        .get(calendar)
        .unwrap()
        .exceptions()
        .len();
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::WorkingTime));
    harness.run();
    click_label(harness, &fr(TextId::WtExcAdd));
    let row = exceptions_before + 1;
    retype(harness, &row_field(TextId::WtExcName, row), "Pont");
    retype(harness, &row_field(TextId::WtExcFrom, row), "14/05/2027");
    retype(harness, &row_field(TextId::WtExcTo, row), "14/05/2027");
    click_label(harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    quiet(app);
    assert!(app.view().dialog.is_none(), "fenêtre fermée");
    let exceptions = project(app).calendars.get(calendar).unwrap().exceptions();
    assert_eq!(exceptions.len(), exceptions_before + 1);
    let bridge = exceptions
        .iter()
        .find(|e| e.name == "Pont")
        .expect("exception « Pont »");
    assert_eq!(bridge.period.first().to_string(), "2027-05-14");
    assert_eq!(history(app), (7, 0));

    // 10. Ligne de base 0 définie, puis effacée (projet entier).
    assert!(!has_baseline(harness.state(), 0));
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::Baseline));
    harness.run();
    click_label(harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    quiet(app);
    assert!(app.view().dialog.is_none());
    assert!(has_baseline(app, 0), "ligne de base 0 posée");
    assert_eq!(history(app), (8, 0));
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::Baseline));
    harness.run();
    click_label(harness, &fr(TextId::BlClear));
    click_label(harness, &fr(TextId::TaskInfoOk));
    let app = harness.state();
    quiet(app);
    assert!(app.view().dialog.is_none());
    assert!(!has_baseline(app, 0), "ligne de base 0 effacée");
    assert_eq!(history(app), (9, 0));
    let edited = project(app).clone();
    assert_ne!(edited, initial);

    // 11. Annuler tout ⇒ projet égal à l'ouverture, document propre.
    while history(harness.state()).0 > 0 {
        harness.state_mut().dispatch(Action::Undo);
        harness.run();
    }
    let app = harness.state();
    quiet(app);
    assert_eq!(project(app), &initial);
    assert_eq!(history(app), (0, 9));
    assert!(!app.document.as_ref().unwrap().is_dirty());

    // 12. Rétablir tout ⇒ projet égal à l'état édité.
    while history(harness.state()).1 > 0 {
        harness.state_mut().dispatch(Action::Redo);
        harness.run();
    }
    let app = harness.state();
    quiet(app);
    assert_eq!(project(app), &edited);
    assert_eq!(history(app), (9, 0));
    assert!(app.document.as_ref().unwrap().is_dirty());

    // 13. Enregistrer (`FileService` en mémoire) et relire : mêmes tâches,
    // même ressource.
    press(harness, Modifiers::COMMAND, Key::S);
    let requests = s.requests.borrow();
    assert_eq!(requests.len(), 1, "une demande d'enregistrement");
    let bytes = &requests[0].bytes;
    let app = s.harness.state();
    assert!(!app.document.as_ref().unwrap().is_dirty(), "enregistré");
    let reread = document::load(bytes, now()).unwrap();
    let summary = |p: &Project| -> Vec<(String, Minutes)> {
        p.tasks
            .iter()
            .map(|t| (t.name.clone(), t.duration.minutes()))
            .collect()
    };
    assert_eq!(reread.session.project().tasks.len(), edited.tasks.len());
    assert_eq!(summary(reread.session.project()), summary(&edited));
    let resources: Vec<&str> = reread
        .session
        .project()
        .resources
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(resources, ["Grutier"]);
    assert_eq!(
        reread.session.project().tasks.get(moved).unwrap().constraint,
        edited.tasks.get(moved).unwrap().constraint
    );
}

/// Vérification 6 du lot L16 : relais avec un vrai `plannercraft mcp`
/// (`relay_app_real_mcp`). L'application kittest lance l'écouteur du canal
/// et publie son rendez-vous dans un dossier temporaire
/// (`PLANNERCRAFT_CONTROL_DIR`) ; le binaire `plannercraft mcp` s'y rattache au
/// démarrage et un client JSON-RPC stdio (écrit à la main, un fil) rejoue
/// `tests/mcp/scenarios/relay_j7.jsonl` pendant que l'interface tourne
/// image par image : écriture acceptée, refusée, édition de l'utilisateur
/// pendant une attente (C-CTL-07), `ui_state`, `ui_action`, `screenshot`
/// (`control.screenshot_unavailable` sous kittest), `open_project` du
/// fichier tenu (rattachement gardé), `undo` relayé, application fermée
/// (`control.app_gone`, C-CTL-06) ; P47 : même JSON final qu'en local.
///
/// Binaire : `PLANNERCRAFT_MCP_BIN`, sinon `plannercraft` à côté du dossier `deps` de
/// cette cible (`cargo build -p plannercraft-cli` avant le test). Absent : le
/// test le dit et ne vérifie rien, sauf `PLANNERCRAFT_RELAY=required` (CI).
/// `PLANNERCRAFT_BLESS=1` réécrit le scénario depuis le journal produit.
/// Harnais de rejeu des scénarios MCP (chargement, `$TMP`, comparaison du
/// journal), partagé avec `plannercraft-mcp` et `plannercraft-cli` ; chemin relatif à
/// `tests/ui/`.
#[cfg(unix)]
#[path = "../../../plannercraft-mcp/tests/common/replay.rs"]
pub(crate) mod replay;

#[cfg(unix)]
#[allow(
    clippy::disallowed_methods,
    clippy::arithmetic_side_effects,
    reason = "test natif : processus fils, fils et attentes, comme `plannercraft mcp` en vrai ; compteurs d'identifiants et de générations bornés par le scénario"
)]
pub(crate) mod relay_app {
    use std::io::{BufRead, BufReader, Write};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
    use std::thread;
    use std::time::Duration;

    use egui_kittest::Harness;
    use plannercraft_automation::control::{
        ListenerHandle, ListenerOptions, RendezvousGuard, Transport, canonical_project, new_token,
        publish_in, spawn_listener_on,
    };
    use plannercraft_automation::text::Language as ToolLanguage;
    use plannercraft_automation::{Batch, EditSource, Workspace};
    use plannercraft_core::Moment;
    use plannercraft_ui_egui::{Action, App, DocOrigin, EditOrigin, Env, LanguagePref, ThemePref, UiState};
    use serde_json::{Value, json};

    use crate::harness_for;

    use super::replay::{self, Scenario, TempDir, logged, normalized, repo_root, substitute};

    /// Nom du scénario.
    const SCENARIO: &str = "relay_j7";

    /// Attente de la validation côté serveur (`--approval-timeout`).
    pub(crate) const APPROVAL_SECS: u64 = 30;

    /// Images d'interface au plus par appel (5 ms chacune : ~60 s).
    pub(crate) const MAX_FRAMES: usize = 12_000;

    /// Ce que l'utilisateur fait pendant un appel relayé.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Directive {
        /// Rien : lecture, outil d'interface, refus immédiat.
        None,
        /// Accepter le bandeau.
        Approve,
        /// Refuser le bandeau.
        Reject,
        /// Modifier le projet pendant l'attente, Accepter (nouvel aperçu,
        /// rien d'appliqué), Accepter encore (C-CTL-07).
        EditThenApprove,
        /// Fermer l'application (écouteur et rendez-vous retirés) avant
        /// l'appel.
        CloseApp,
    }

    /// Directive de l'appel `seq` (1-based) du scénario.
    fn directive(seq: usize) -> Directive {
        match seq {
            2 | 6 | 9 => Directive::Approve,
            3 => Directive::Reject,
            4 => Directive::EditThenApprove,
            10 => Directive::CloseApp,
            _ => Directive::None,
        }
    }

    /// Édition de l'utilisateur pendant l'attente de l'appel 4 (C-CTL-07),
    /// rejouée en local pour P47.
    fn user_edit() -> Value {
        json!({ "uid": 3, "name": "Plans revus pendant l'attente" })
    }

    /// Binaire `plannercraft` : `PLANNERCRAFT_MCP_BIN`, sinon le premier
    /// dossier au-dessus de l'exécutable de test qui le contient (`deps/`
    /// jusqu'à Cargo 1.99, `build/<paquet>/<empreinte>/out/` depuis 1.100).
    pub(crate) fn mcp_binary() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("PLANNERCRAFT_MCP_BIN") {
            return Some(PathBuf::from(path));
        }
        let exe = std::env::current_exe().ok()?;
        exe.ancestors()
            .skip(1)
            .take(6)
            .map(|dir| dir.join("plannercraft"))
            .find(|path| path.is_file())
    }

    /// Client JSON-RPC stdio minimal (`initialize`, `tools/call`) dans un
    /// fil : les appels arrivent par `calls`, chaque résultat repart par
    /// `results` sous la forme du journal (`{"ok": …}` / `{"error": …}`).
    pub(crate) fn client_thread(
        child: &mut Child,
        calls: Receiver<Option<(String, Value)>>,
        results: SyncSender<Value>,
    ) {
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut next_id = 1_u64;
            let mut send = |method: &str, params: Value, id: Option<u64>| {
                let mut message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
                if let Some(id) = id {
                    message["id"] = json!(id);
                }
                let line = format!("{message}\n");
                stdin.write_all(line.as_bytes()).unwrap();
                stdin.flush().unwrap();
            };
            let mut receive = |id: u64| -> Value {
                let mut line = String::new();
                loop {
                    line.clear();
                    let n = reader.read_line(&mut line).unwrap();
                    assert!(n > 0, "plannercraft mcp a fermé sa sortie");
                    let message: Value = match serde_json::from_str(line.trim_end()) {
                        Ok(message) => message,
                        Err(_) => continue,
                    };
                    if message["id"] == json!(id) {
                        assert!(message.get("error").is_none(), "JSON-RPC : {message}");
                        return message["result"].clone();
                    }
                }
            };
            send(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "plannercraft-relay-test", "version": "0" }
                }),
                Some(next_id),
            );
            receive(next_id);
            next_id += 1;
            send("notifications/initialized", json!({}), None);
            while let Ok(Some((tool, params))) = calls.recv() {
                send(
                    "tools/call",
                    json!({ "name": tool, "arguments": params }),
                    Some(next_id),
                );
                let result = receive(next_id);
                next_id += 1;
                let is_error = result["isError"].as_bool();
                let structured = result.get("structuredContent").cloned();
                results.send(logged(is_error, structured)).unwrap();
            }
            // Entrée fermée : le serveur rend la main.
            drop(stdin);
        });
    }

    /// Application de test branchée sur un écouteur réel, fichier
    /// `maison.xml` (l'exemple) tenu dans `tmp`, rendez-vous publié dans
    /// `tmp/control` ; écouteur sur `transport` (J8 lot L8 : socket Unix
    /// ou TCP en boucle locale).
    pub(crate) fn wired(tmp: &TempDir, now: Moment, transport: Transport) -> Wired {
        wired_with(tmp, now, transport, APPROVAL_SECS)
    }

    /// Application, écouteur, rendez-vous publié, dossier des rendez-vous
    /// et dossier de la socket (à garder jusqu'à la fin du test).
    pub(crate) type Wired = (
        Harness<'static, App>,
        ListenerHandle,
        RendezvousGuard,
        PathBuf,
        SocketDir,
    );

    /// Dossier court de la socket Unix, supprimé à la fin du test. Pas dans
    /// `tmp` : sous macOS, le dossier temporaire canonique
    /// (`/private/var/folders/…/T/plannercraft-replay-<tag>-<pid>-<n>`) laissait
    /// au chemin de la socket moins que les 104 octets de `sun_path`
    /// (« path must be shorter than `SUN_LEN` », CI macOS de la PR #7).
    /// `temp_dir()` non canonique et un nom court gardent ~80 octets.
    pub(crate) struct SocketDir(PathBuf);

    impl SocketDir {
        fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("prs-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }
    }

    impl Drop for SocketDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Comme [`wired`], délai de validation `approval_secs` (J8 lot L16 :
    /// `relay_j8` laisse expirer une demande).
    pub(crate) fn wired_with(tmp: &TempDir, now: Moment, transport: Transport, approval_secs: u64) -> Wired {
        let plan = tmp.path().join("maison.xml");
        std::fs::write(&plan, plannercraft_ui_egui::DEMO_MSPDI).unwrap();
        let state = UiState {
            language: LanguagePref::Fr,
            theme: ThemePref::Light,
            ..UiState::default()
        };
        let mut harness = harness_for(App::new(state, Env::fixed(now)));
        let (tx, rx) = sync_channel(64);
        harness.state_mut().control_inbox(rx);
        harness.run();
        let origin = DocOrigin {
            name: "maison.xml".to_owned(),
            path: Some(plan.to_str().unwrap().to_owned()),
            demo: false,
        };
        harness
            .state_mut()
            .open_bytes(plannercraft_ui_egui::DEMO_MSPDI, origin);
        harness.run();
        assert!(
            harness.state().document.is_some(),
            "{:?}",
            harness.state().messages()
        );
        let options = ListenerOptions {
            approval_timeout: Duration::from_secs(approval_secs),
            reply_grace: Duration::from_secs(2),
        };
        let sockets = SocketDir::new();
        let handle = spawn_listener_on(
            &sockets.0,
            new_token().unwrap(),
            tx,
            Arc::new(|| {}),
            options,
            transport,
        )
        .unwrap();
        let control = tmp.path().join("control");
        let guard = publish_in(&control, &handle.rendezvous(canonical_project(&plan))).unwrap();
        (harness, handle, guard, control, sockets)
    }

    fn generation(app: &App) -> u64 {
        app.document.as_ref().unwrap().generation()
    }

    fn pending(app: &App) -> Option<(u64, u64)> {
        app.approvals().pending().front().map(|p| (p.id, p.generation))
    }

    /// Fait tourner l'interface jusqu'au résultat de l'appel en cours, en
    /// appliquant `directive` au bandeau.
    fn pump(harness: &mut Harness<'static, App>, results: &Receiver<Value>, directive: Directive) -> Value {
        let mut approvals = 0_u8;
        for _ in 0..MAX_FRAMES {
            if let Ok(result) = results.try_recv() {
                return result;
            }
            if let Some((id, pending_generation)) = pending(harness.state()) {
                match directive {
                    Directive::Approve if approvals == 0 => {
                        harness.state_mut().dispatch(Action::Approve(id));
                        approvals = 1;
                    }
                    Directive::Reject if approvals == 0 => {
                        harness.state_mut().dispatch(Action::Reject(id));
                        approvals = 1;
                    }
                    Directive::EditThenApprove if approvals == 0 => {
                        let batch: Batch = serde_json::from_value(json!({
                            "format_version": 1,
                            "ops": [{ "op": "task.update", "uid": 3, "name": "Plans revus pendant l'attente" }]
                        }))
                        .unwrap();
                        let before = generation(harness.state());
                        harness.state_mut().execute(&batch, EditOrigin::User).unwrap();
                        harness.run();
                        assert_eq!(generation(harness.state()), before + 1);
                        harness.state_mut().dispatch(Action::Approve(id));
                        harness.run();
                        // Aperçu périmé : rien d'appliqué, nouvel aperçu en attente.
                        assert_eq!(generation(harness.state()), before + 1, "aperçu périmé appliqué");
                        approvals = 1;
                    }
                    Directive::EditThenApprove
                        if approvals == 1 && pending_generation == generation(harness.state()) =>
                    {
                        harness.state_mut().dispatch(Action::Approve(id));
                        approvals = 2;
                    }
                    _ => {}
                }
            }
            harness.step();
            thread::sleep(Duration::from_millis(5));
        }
        panic!("aucun résultat de l'appel relayé en {MAX_FRAMES} images ({directive:?})");
    }

    #[test]
    fn relay_app_real_mcp() {
        relay_scenario(Transport::Unix, "relay");
    }

    /// Même scénario, même journal, relayé en TCP sur la boucle locale (J8
    /// lot L8, transport de Windows rejoué sous Linux ; P47).
    #[test]
    fn relay_app_real_mcp_tcp() {
        relay_scenario(Transport::Tcp, "relaytcp");
    }

    /// Détail du système dans `control.app_gone` : l'application fermée
    /// laisse une socket absente (`ENOENT`) en Unix, un port fermé
    /// (`ECONNREFUSED`) en TCP. Seul ce détail change d'un transport à
    /// l'autre ; il est ramené à la forme Unix du scénario.
    fn as_unix_detail(text: &str) -> String {
        const UNIX: &str = "No such file or directory (os error 2)";
        // Linux (111) et macOS (61).
        [
            "Connection refused (os error 111)",
            "Connection refused (os error 61)",
        ]
        .into_iter()
        .fold(text.to_owned(), |acc, tcp| acc.replace(tcp, UNIX))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "scénario de bout en bout lu d'un bloc, appel par appel"
    )]
    fn relay_scenario(transport: Transport, tag: &str) {
        let Some(binary) = mcp_binary().filter(|p| p.is_file()) else {
            let required = std::env::var("PLANNERCRAFT_RELAY").is_ok_and(|v| v == "required");
            assert!(
                !required,
                "PLANNERCRAFT_RELAY=required : binaire plannercraft introuvable (PLANNERCRAFT_MCP_BIN)"
            );
            eprintln!(
                "relay_app_real_mcp : binaire plannercraft absent, test non exécuté (cargo build -p plannercraft-cli)"
            );
            return;
        };
        let root = repo_root(env!("CARGO_MANIFEST_DIR"));
        let scenario = Scenario::load(&root, SCENARIO);
        let now: Moment = replay::NOW.parse().unwrap();
        let tmp = TempDir::new(tag);
        let (mut harness, handle, guard, control, _sockets) = wired(&tmp, now, transport);
        assert_eq!(
            handle.endpoint().starts_with("tcp:127.0.0.1:"),
            transport == Transport::Tcp,
            "{}",
            handle.endpoint()
        );
        let initial_session = harness.state().document.as_ref().unwrap().session.clone();
        let mut app_side: (Option<ListenerHandle>, Option<RendezvousGuard>) = (Some(handle), Some(guard));

        let journal = tmp.path().join("run.jsonl");
        let mut child = Command::new(&binary)
            .args(["--now", replay::NOW, "--lang", "fr", "mcp", "--journal"])
            .arg(&journal)
            .args(["--approval-timeout", &APPROVAL_SECS.to_string()])
            .env("PLANNERCRAFT_CONTROL_DIR", &control)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap_or_else(|e| panic!("{} : {e}", binary.display()));
        let (calls_tx, calls_rx) = sync_channel::<Option<(String, Value)>>(1);
        let (results_tx, results_rx) = sync_channel::<Value>(1);
        client_thread(&mut child, calls_rx, results_tx);

        let mut got = Vec::with_capacity(scenario.calls.len());
        let mut local = Workspace::new(now, ToolLanguage::Fr);
        local.set_session(initial_session);
        for (i, call) in scenario.calls.iter().enumerate() {
            let seq = i + 1;
            let directive = directive(seq);
            if directive == Directive::CloseApp {
                // Application fermée : écouteur arrêté, rendez-vous retiré.
                app_side = (None, None);
                harness.run();
            }
            let params = substitute(&call.params, tmp.path());
            calls_tx.send(Some((call.tool.clone(), params.clone()))).unwrap();
            let result = pump(&mut harness, &results_rx, directive);
            // P47 : les écritures acceptées (et l'édition de l'utilisateur)
            // rejouées en local, dans le même ordre.
            match (call.tool.as_str(), directive) {
                ("add_task", Directive::EditThenApprove) => {
                    local.call("task.update", user_edit()).unwrap();
                    local.call("task.add", params).unwrap();
                }
                ("add_task", Directive::Approve) => {
                    local.call("task.add", params).unwrap();
                }
                ("undo", Directive::Approve) => {
                    local.call("history.undo", params).unwrap();
                }
                _ => {}
            }
            let result = normalized(&result, tmp.path());
            got.push(match transport {
                Transport::Unix => result,
                Transport::Tcp => serde_json::from_str(&as_unix_detail(&result.to_string())).unwrap(),
            });
        }
        calls_tx.send(None).unwrap();
        drop(calls_tx);
        // 3 000 pas de 10 ms (~30 s) au plus, sans horloge (règle 5).
        let mut status = None;
        for _ in 0..3_000 {
            if let Some(s) = child.try_wait().unwrap() {
                status = Some(s);
                break;
            }
            harness.step();
            thread::sleep(Duration::from_millis(10));
        }
        let status = status.expect("plannercraft mcp ne s'arrête pas");
        assert!(status.success(), "{status}");
        drop(app_side);

        // Journal produit : scénario octet pour octet (ou régénéré).
        let mut produced = std::fs::read_to_string(&journal).unwrap();
        if transport == Transport::Tcp {
            produced = as_unix_detail(&produced);
        }
        if replay::blessing() {
            replay::bless(&root, &scenario, &produced, tmp.path(), None);
            return;
        }
        replay::assert_results(&scenario, &got);
        replay::assert_journal(&scenario, &produced, tmp.path());

        // Faits clés : la seule écriture refusée n'a rien changé, les
        // entrées d'historique de l'agent portent sa source, et le projet
        // final égale le rejeu local (P47).
        let app = harness.state();
        let session = &app.document.as_ref().unwrap().session;
        let local_session = local.session().unwrap();
        assert_eq!(
            serde_json::to_value(session.project()).unwrap(),
            serde_json::to_value(local_session.project()).unwrap(),
            "P47 : projet final"
        );
        assert_eq!(
            session.history().entries.len(),
            local_session.history().entries.len()
        );
        assert_eq!(session.history().redo.len(), local_session.history().redo.len());
        let sources: Vec<EditSource> = session.history().entries.iter().map(|e| e.source).collect();
        // Appel 2 (agent), édition de l'utilisateur, appel 4 (agent) annulé par l'appel 9.
        assert_eq!(sources, [EditSource::Agent, EditSource::User], "{sources:?}");
        assert_eq!(session.history().redo.len(), 1);
        assert!(app.approvals().pending().is_empty());
    }
}
