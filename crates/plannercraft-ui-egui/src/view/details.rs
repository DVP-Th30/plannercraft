//! Volet de détails « Formulaire tâche » (seconde passe UX, ADR-0019 D6,
//! H-UI-112, H-UI-113), sous la vue (cadre du lot L4, contenu du lot L5).
//!
//! - Le volet suit la tâche de la cellule active (`FrameCtx::selection`) :
//!   brouillon ([`TaskFormDraft`], partagé avec « Informations sur la
//!   tâche ») relu seulement quand la tâche ou la clé du document change
//!   ([`sync`]) ; ligne vide ou rien ⇒ « Sélectionnez une tâche ».
//! - Haut : Nom, Durée, Début, Fin, Type de tâche, % achevé, Contrainte
//!   (type, date), Pilotée par l'effort, Planifiée manuellement, sur deux
//!   colonnes ; récapitulative : champs calculés grisés, raison en
//!   infobulle (H-UI-73).
//! - Bas : deux grilles selon la forme (`UiState.details.form`, clic droit
//!   dans le volet) : Ressources (Nom, Unités, Travail), Prédécesseurs ou
//!   Successeurs (N°, Nom lu, Type, Décalage) ; une ligne vide en bas de
//!   chaque grille sert à l'ajout, un N° vidé retire le lien.
//! - OK (ou Entrée dans un champ du haut) ⇒ `Action::Details(Ok)` : **un**
//!   lot libellé « Formulaire tâche » ([`TaskFormDraft::to_pane_batch`]),
//!   une entrée d'annulation ; rien de changé ⇒ aucun lot. Annuler (ou
//!   Échap dans un champ) ⇒ brouillon relu. Cellule active changée avec un
//!   brouillon modifié ⇒ OK implicite ; refus ⇒ sélection rendue à la
//!   tâche du brouillon, refus sous les champs et en notification.
//! - Document changé ailleurs (annuler, agent) : brouillon non modifié
//!   relu ; modifié gardé ([`TaskFormDraft::rebase`] : seuls les champs
//!   touchés restent) avec un avertissement.
//!
//! Aucune date calculée ici (règle 3) ; aucun `f32` non fini utilisé
//! (règle 10 : rectangle vérifié avant tout dessin).

use egui::{Id, Rect, Sense, UiBuilder};
use plannercraft_automation::Session;
use plannercraft_automation::text::Language as TextLanguage;
use plannercraft_core::{ResourceKind, Severity, TaskUid};

use crate::action::{Action, DetailsOp, PaneOp};
use crate::app::EditOrigin;
use crate::dialog::task_info::{combo, error_line};
use crate::i18n::{Arg, TextId, Texts};
use crate::state::DetailsForm;
use crate::task_form::{
    AssignRow, CONSTRAINTS, FieldError, LINK_TYPES, LinkRow, LinkSide, TASK_TYPES, TaskFormDraft,
    task_name_for_id,
};
use crate::view::FrameCtx;

/// État transitoire du volet : tâche suivie, clé du document lue,
/// brouillon et lecture d'origine, refus de la dernière tentative.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DetailsTransient {
    /// Tâche suivie (celle de la cellule active), s'il y en a une.
    pub task: Option<plannercraft_core::TaskUid>,
    /// Clé du document (ouverture, génération) du brouillon.
    pub doc_key: Option<(u64, u64)>,
    /// Le brouillon diffère de sa lecture d'origine.
    pub dirty: bool,
    /// Brouillon édité (`None` : aucune tâche suivie).
    pub draft: Option<TaskFormDraft>,
    /// Lecture d'origine du brouillon (comparée pour `dirty`).
    pub base: Option<TaskFormDraft>,
    /// Refus de la dernière tentative (champs, ou registre sans champ).
    pub errors: Vec<FieldError>,
    /// Brouillon modifié gardé sur un document changé ailleurs
    /// (avertissement affiché).
    pub kept: bool,
}

impl DetailsTransient {
    /// Relit le brouillon de `uid` sur `s` (aucun ⇒ invite), clé `key`.
    fn load(&mut self, uid: Option<TaskUid>, s: &Session, key: (u64, u64), lang: TextLanguage) {
        let draft = uid.and_then(|uid| TaskFormDraft::from_session(s, uid, key, lang));
        *self = Self {
            task: draft.as_ref().map(|d| d.uid),
            doc_key: Some(key),
            dirty: false,
            base: draft.clone(),
            draft,
            errors: Vec::new(),
            kept: false,
        };
    }

    /// Recalcule `dirty` après une saisie.
    fn touch(&mut self) {
        self.dirty = self.draft != self.base;
    }
}

/// Synchronise le volet avec la cellule active `selection` et le document
/// (`s`, clé `key`) ; rend vrai si un OK implicite est demandé (cellule
/// active changée avec un brouillon modifié : le brouillon est gardé
/// jusqu'à ce que `App::details_op` l'écrive ou le refuse).
pub fn sync(
    state: &mut Option<DetailsTransient>,
    s: &Session,
    selection: Option<TaskUid>,
    key: (u64, u64),
    lang: TextLanguage,
) -> bool {
    let want = selection.filter(|uid| s.project().tasks.get(*uid).is_some_and(|t| !t.is_null));
    let st = state.get_or_insert_with(DetailsTransient::default);
    let current = st.draft.as_ref().map(|d| d.uid);
    let alive = current.is_some_and(|uid| s.project().tasks.contains(uid));
    if current != want || st.doc_key.is_none() {
        if st.dirty && alive {
            return true;
        }
        st.load(want, s, key, lang);
        return false;
    }
    if st.doc_key != Some(key) {
        let fresh = current.and_then(|uid| TaskFormDraft::from_session(s, uid, key, lang));
        match (fresh, st.draft.as_ref(), st.base.as_ref()) {
            (Some(fresh), Some(draft), Some(base)) if st.dirty => {
                st.draft = Some(draft.rebase(base, &fresh));
                st.base = Some(fresh);
                st.doc_key = Some(key);
                st.kept = true;
                st.touch();
            }
            _ => st.load(want, s, key, lang),
        }
    }
    false
}

/// Largeur des champs du haut.
const TOP_FIELD_W: f32 = 170.0;

/// Largeur des colonnes d'ID et de décalage des grilles.
const SMALL_W: f32 = 54.0;

/// Formes du volet et leur libellé (menu contextuel).
const FORMS: [(DetailsForm, TextId); 3] = [
    (
        DetailsForm::ResourcesPredecessors,
        TextId::DetailsFormResourcesPredecessors,
    ),
    (
        DetailsForm::ResourcesSuccessors,
        TextId::DetailsFormResourcesSuccessors,
    ),
    (
        DetailsForm::PredecessorsSuccessors,
        TextId::DetailsFormPredecessorsSuccessors,
    ),
];

/// Grille du bas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grid {
    Resources,
    Links(LinkSide),
}

/// Grilles de la forme `form`, de gauche à droite.
const fn grids(form: DetailsForm) -> [Grid; 2] {
    match form {
        DetailsForm::ResourcesPredecessors => [Grid::Resources, Grid::Links(LinkSide::Predecessors)],
        DetailsForm::ResourcesSuccessors => [Grid::Resources, Grid::Links(LinkSide::Successors)],
        DetailsForm::PredecessorsSuccessors => [
            Grid::Links(LinkSide::Predecessors),
            Grid::Links(LinkSide::Successors),
        ],
    }
}

/// Dessine le volet dans `rect` (sous l'en-tête du cadre) : invite sans
/// tâche suivie, sinon le formulaire de `details`.
pub fn show(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, details: &mut Option<DetailsTransient>, rect: Rect) {
    if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let Some(doc) = fc.doc else {
        return;
    };
    let lang = crate::edit::text_language(fc.texts.language());
    if sync(details, &doc.session, fc.selection, doc.cache_key(), lang) {
        fc.actions.push(Action::Details(DetailsOp::Ok));
    }
    // Fond : clic droit n'importe où ⇒ formes (les champs, posés après,
    // passent devant).
    let background = ui.interact(
        rect,
        Id::unique("plannercraft.details.background"),
        Sense::click(),
    );
    let title = fc.texts.get(TextId::DetailsTitle);
    background.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, true, title.clone()));
    background.context_menu(|ui| {
        for (form, text) in FORMS {
            if ui
                .radio(fc.state.details.form == form, fc.texts.get(text))
                .clicked()
            {
                fc.actions.push(Action::Pane(PaneOp::SetDetailsForm(form)));
                ui.close();
            }
        }
    });
    let Some(st) = details.as_mut().filter(|st| st.draft.is_some()) else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            fc.texts.get(TextId::DetailsSelectTask),
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().weak_text_color(),
        );
        return;
    };
    let inner = rect.shrink(6.0);
    if !inner.is_positive() {
        return;
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .id_salt("plannercraft.details.body"),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt("plannercraft.details.scroll")
        .auto_shrink([false, false])
        .show(&mut child, |ui| form(ui, fc, &doc.session, st));
}

/// Réponse des champs d'une image : Entrée (champs du haut) ou Échap
/// (tout champ) relâchés.
#[derive(Clone, Copy, Debug, Default)]
struct Keys {
    enter: bool,
    escape: bool,
}

impl Keys {
    /// Relève Entrée et Échap sur un champ qui vient de perdre le focus.
    fn read(&mut self, ui: &egui::Ui, response: &egui::Response, enter_ok: bool) {
        if response.lost_focus() {
            let (enter, escape) =
                ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            self.enter |= enter && enter_ok;
            self.escape |= escape;
        }
    }
}

/// Champ de texte d'une ligne d'identifiant `id`, nommé `label`.
fn field(
    ui: &mut egui::Ui,
    id: Id,
    label: &str,
    value: &mut String,
    enabled: bool,
    width: f32,
) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::TextEdit::singleline(value).id(id).desired_width(width),
    );
    let name = label.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, enabled, name.clone()));
    response
}

/// Refus du champ `field` (ligne `row`).
fn error_for<'a>(errors: &'a [FieldError], field: &str, row: Option<usize>) -> Option<&'a str> {
    errors
        .iter()
        .find(|e| e.field == field && e.row == row)
        .map(|e| e.message.as_str())
}

/// Contenu du formulaire (brouillon présent).
fn form(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, s: &Session, st: &mut DetailsTransient) {
    let texts = fc.texts;
    let errors = st.errors.clone();
    let Some(draft) = st.draft.as_mut() else {
        return;
    };
    let mut keys = Keys::default();
    ui.horizontal(|ui| {
        if ui.button(texts.get(TextId::DetailsOk)).clicked() {
            fc.actions.push(Action::Details(DetailsOp::Ok));
        }
        if ui.button(texts.get(TextId::DetailsCancel)).clicked() {
            fc.actions.push(Action::Details(DetailsOp::Cancel));
        }
        if st.kept {
            ui.colored_label(ui.visuals().warn_fg_color, texts.get(TextId::DetailsDraftKept));
        }
    });
    for e in errors.iter().filter(|e| e.field.is_empty()) {
        ui.colored_label(ui.visuals().error_fg_color, e.message.as_str());
    }
    top_fields(ui, texts, draft, &errors, &mut keys);
    ui.separator();
    let [left, right] = grids(fc.state.details.form);
    ui.columns(2, |cols| {
        if let [a, b] = cols {
            grid(a, texts, s, draft, left, &errors, &mut keys);
            grid(b, texts, s, draft, right, &errors, &mut keys);
        }
    });
    st.touch();
    if keys.escape {
        fc.actions.push(Action::Details(DetailsOp::Cancel));
    } else if keys.enter {
        fc.actions.push(Action::Details(DetailsOp::Ok));
    }
}

/// Champs du haut, sur deux colonnes (libellé, valeur, libellé, valeur).
fn top_fields(
    ui: &mut egui::Ui,
    texts: &Texts,
    draft: &mut TaskFormDraft,
    errors: &[FieldError],
    keys: &mut Keys,
) {
    let editable = !draft.summary;
    let reason = texts.get(TextId::DetailsSummaryReason);
    let uid = draft.uid;
    let id = |name: &str| Id::unique("plannercraft.details.top").with((name, uid.get()));
    egui::Grid::new("plannercraft.details.top")
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            let g = &mut draft.general;
            let pairs: [[(&'static str, TextId, &mut String, bool); 2]; 3] = [
                [
                    ("name", TextId::DetailsName, &mut g.name, true),
                    ("duration", TextId::DetailsDuration, &mut g.duration, editable),
                ],
                [
                    ("start", TextId::DetailsStart, &mut g.start, editable),
                    ("finish", TextId::DetailsFinish, &mut g.finish, editable),
                ],
                [
                    ("percent", TextId::DetailsPercent, &mut g.percent, editable),
                    (
                        "constraint_date",
                        TextId::DetailsConstraintDate,
                        &mut draft.advanced.constraint_date,
                        draft.advanced.constraint.is_dated(),
                    ),
                ],
            ];
            for pair in pairs {
                for (name, label, value, enabled) in pair {
                    let label = texts.get(label);
                    ui.label(&label);
                    ui.vertical(|ui| {
                        let r = field(ui, id(name), &label, value, enabled, TOP_FIELD_W);
                        let r = if draft.summary && !enabled && name != "constraint_date" {
                            r.on_disabled_hover_text(&reason)
                        } else {
                            r
                        };
                        keys.read(ui, &r, true);
                        error_line(ui, error_for(errors, name, None));
                    });
                }
                ui.end_row();
            }
            let a = &mut draft.advanced;
            let label = texts.get(TextId::DetailsConstraint);
            ui.label(&label);
            combo(
                ui,
                ("plannercraft.details.constraint", uid.get()),
                &label,
                texts,
                &mut a.constraint,
                &CONSTRAINTS,
                true,
            );
            let label = texts.get(TextId::DetailsTaskType);
            ui.label(&label);
            combo(
                ui,
                ("plannercraft.details.task-type", uid.get()),
                &label,
                texts,
                &mut a.task_type,
                &TASK_TYPES,
                editable,
            );
            ui.end_row();
            ui.allocate_space(egui::Vec2::ZERO);
            let r = ui.add_enabled(
                editable,
                egui::Checkbox::new(&mut a.effort_driven, texts.get(TextId::DetailsEffortDriven)),
            );
            if !editable {
                r.on_disabled_hover_text(&reason);
            }
            ui.allocate_space(egui::Vec2::ZERO);
            let r = ui.add_enabled(
                editable,
                egui::Checkbox::new(&mut draft.general.manual, texts.get(TextId::DetailsManual)),
            );
            if !editable {
                r.on_disabled_hover_text(&reason);
            }
            ui.end_row();
        });
}

/// Nom accessible d'une cellule : « {grille} {colonne}, ligne {n} ».
fn cell_name(texts: &Texts, grid: TextId, column: TextId, row: usize) -> String {
    let column = format!("{} {}", texts.get(grid), texts.get(column));
    texts.get_args(
        TextId::TaskInfoRowField,
        &[
            ("column", Arg::Str(column.into())),
            (
                "count",
                Arg::Count(i64::try_from(row.saturating_add(1)).unwrap_or(i64::MAX)),
            ),
        ],
    )
}

/// Une grille du bas.
fn grid(
    ui: &mut egui::Ui,
    texts: &Texts,
    s: &Session,
    draft: &mut TaskFormDraft,
    which: Grid,
    errors: &[FieldError],
    keys: &mut Keys,
) {
    match which {
        Grid::Resources => resources_grid(ui, texts, s, draft, errors, keys),
        Grid::Links(side) => links_grid(ui, texts, s, draft, side, errors, keys),
    }
}

/// Grille Prédécesseurs ou Successeurs : N°, Nom (lu), Type, Décalage ;
/// ligne vide d'ajout en bas.
fn links_grid(
    ui: &mut egui::Ui,
    texts: &Texts,
    s: &Session,
    draft: &mut TaskFormDraft,
    side: LinkSide,
    errors: &[FieldError],
    keys: &mut Keys,
) {
    let (title, rows, id_field, lag_field) = match side {
        LinkSide::Predecessors => (
            TextId::DetailsPredecessors,
            &mut draft.links,
            "predecessor",
            "lag",
        ),
        LinkSide::Successors => (
            TextId::DetailsSuccessors,
            &mut draft.successors,
            "successor",
            "successor_lag",
        ),
    };
    let uid = draft.uid.get();
    ui.strong(texts.get(title));
    let mut blank = LinkRow::blank();
    let count = rows.len();
    egui::Grid::new(("plannercraft.details.links", title))
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            for col in [
                TextId::DetailsColId,
                TextId::DetailsColName,
                TextId::DetailsColType,
                TextId::DetailsColLag,
            ] {
                ui.strong(texts.get(col));
            }
            ui.end_row();
            for (i, row) in rows.iter_mut().chain(std::iter::once(&mut blank)).enumerate() {
                let id = |col: &str| Id::unique("plannercraft.details.cell").with((title, col, uid, i));
                ui.vertical(|ui| {
                    let name = cell_name(texts, title, TextId::DetailsColId, i);
                    let r = field(ui, id("id"), &name, &mut row.id, true, SMALL_W);
                    keys.read(ui, &r, false);
                    error_line(ui, error_for(errors, id_field, Some(i)));
                });
                ui.label(task_name_for_id(&row.id, s).unwrap_or_default());
                combo(
                    ui,
                    ("plannercraft.details.link-type", title, uid, i),
                    &cell_name(texts, title, TextId::DetailsColType, i),
                    texts,
                    &mut row.kind,
                    &LINK_TYPES,
                    true,
                );
                ui.vertical(|ui| {
                    let name = cell_name(texts, title, TextId::DetailsColLag, i);
                    let r = field(ui, id("lag"), &name, &mut row.lag, true, SMALL_W);
                    keys.read(ui, &r, false);
                    error_line(ui, error_for(errors, lag_field, Some(i)));
                });
                ui.end_row();
            }
        });
    // Ligne vide remplie : ajoutée au brouillon (même rang, même
    // identifiant de champ : le focus reste).
    if blank != LinkRow::blank() && rows.len() == count {
        rows.push(blank);
    }
}

/// Grille Ressources : Nom (liste des ressources), Unités, Travail,
/// Retirer ; ligne vide d'ajout en bas.
fn resources_grid(
    ui: &mut egui::Ui,
    texts: &Texts,
    s: &Session,
    draft: &mut TaskFormDraft,
    errors: &[FieldError],
    keys: &mut Keys,
) {
    let project = s.project();
    let title = TextId::DetailsResources;
    let uid = draft.uid.get();
    ui.strong(texts.get(title));
    let mut remove = None;
    let mut added = None;
    let rows = &mut draft.assignments;
    let count = rows.len();
    egui::Grid::new("plannercraft.details.resources")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            for col in [
                TextId::DetailsColName,
                TextId::DetailsColUnits,
                TextId::DetailsColWork,
            ] {
                ui.strong(texts.get(col));
            }
            ui.allocate_space(egui::Vec2::ZERO);
            ui.end_row();
            for i in 0..=count {
                let row = rows.get_mut(i);
                let current = row.as_ref().map(|r| r.resource);
                let selected = current
                    .and_then(|r| project.resources.get(r))
                    .map(|r| r.name.clone())
                    .unwrap_or_default();
                let mut chosen = current;
                ui.vertical(|ui| {
                    let response = egui::ComboBox::from_id_salt(("plannercraft.details.resource", uid, i))
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for r in &project.resources {
                                let r_uid = plannercraft_core::HasUid::uid(r);
                                if ui.selectable_label(chosen == Some(r_uid), &r.name).clicked() {
                                    chosen = Some(r_uid);
                                }
                            }
                        })
                        .response;
                    let name = cell_name(texts, title, TextId::DetailsColName, i);
                    response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name.clone()));
                    error_line(ui, error_for(errors, "resource", Some(i)));
                });
                let Some(row) = row else {
                    // Ligne vide : une ressource choisie l'ajoute.
                    added = chosen;
                    ui.end_row();
                    continue;
                };
                if let Some(r) = chosen {
                    row.resource = r;
                }
                let kind = project.resources.get(row.resource).map(|r| &r.kind);
                let id = |col: &str| Id::unique("plannercraft.details.cell").with((title, col, uid, i));
                ui.vertical(|ui| {
                    let name = cell_name(texts, title, TextId::DetailsColUnits, i);
                    let enabled = !matches!(kind, Some(ResourceKind::Cost));
                    let r = field(ui, id("units"), &name, &mut row.units, enabled, SMALL_W);
                    keys.read(ui, &r, false);
                    error_line(ui, error_for(errors, "units", Some(i)));
                });
                ui.vertical(|ui| {
                    let name = cell_name(texts, title, TextId::DetailsColWork, i);
                    let enabled = matches!(kind, Some(ResourceKind::Work));
                    let r = field(ui, id("work"), &name, &mut row.work, enabled, SMALL_W);
                    keys.read(ui, &r, false);
                    error_line(ui, error_for(errors, "work", Some(i)));
                });
                if ui.button(texts.get(TextId::TaskInfoRemoveRow)).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove
        && i < rows.len()
    {
        rows.remove(i);
    }
    if let Some(resource) = added {
        rows.push(AssignRow {
            resource,
            units: String::new(),
            work: String::new(),
        });
    }
}

impl crate::App {
    /// OK ou Annuler du volet (`Action::Details`, H-UI-113 ; OK implicite
    /// d'un changement de cellule active et des demandes globales,
    /// `quit.rs`). OK : brouillon modifié écrit en **un** lot ; rien de
    /// changé ⇒ aucun lot, aucun message ; refus ⇒ brouillon gardé, refus
    /// sous les champs et en notification, cellule active rendue à la
    /// tâche du brouillon. Annuler : brouillon relu. Volet masqué : rien.
    /// Rend `false` si l'OK a été refusé (le brouillon reste ouvert :
    /// Cmd+Q et les demandes globales s'arrêtent là, H-UI-108).
    pub(crate) fn details_op(&mut self, op: DetailsOp) -> bool {
        if !self.state.details.visible {
            return true;
        }
        let lang = self.text_language();
        let Some(doc) = self.document.as_ref() else {
            return true;
        };
        let key = doc.cache_key();
        let Some(st) = self.view.details.as_mut() else {
            return true;
        };
        let Some(uid) = st.draft.as_ref().map(|d| d.uid) else {
            return true;
        };
        if op == DetailsOp::Cancel || !st.dirty {
            if op == DetailsOp::Cancel || st.doc_key != Some(key) {
                st.load(Some(uid), &doc.session, key, lang);
            }
            return true;
        }
        // Document changé depuis la dernière image : brouillon repris
        // (même tâche demandée : jamais d'OK implicite ici).
        if st.doc_key != Some(key) {
            sync(&mut self.view.details, &doc.session, Some(uid), key, lang);
        }
        let Some(st) = self.view.details.as_mut() else {
            return true;
        };
        let Some(draft) = st.draft.clone() else {
            return true;
        };
        let batch = match draft.to_pane_batch(&doc.session, lang) {
            Ok(Some(batch)) => batch,
            Ok(None) => {
                st.load(Some(uid), &doc.session, key, lang);
                return true;
            }
            Err(errors) => {
                self.refuse_details(uid, errors, None);
                return false;
            }
        };
        match self.execute(&batch, EditOrigin::Dialog) {
            Ok(_) => {
                if let (Some(doc), Some(st)) = (self.document.as_ref(), self.view.details.as_mut()) {
                    st.load(Some(uid), &doc.session, doc.cache_key(), lang);
                }
                true
            }
            Err(error) => {
                // Refus du registre en langage d'utilisateur (H-UI-105) :
                // phrase sous les champs, détail technique repliable dans
                // la notification.
                let project = self.document.as_ref().map(crate::app::OpenDocument::project);
                let (message, detail) = crate::error_text::user_message(&error, project, &self.texts, lang);
                self.refuse_details(
                    uid,
                    vec![FieldError {
                        tab: crate::task_form::Tab::General,
                        field: "",
                        row: None,
                        message,
                    }],
                    Some(detail),
                );
                false
            }
        }
    }

    /// Refus du formulaire : refus posés sous les champs, premier refus en
    /// notification, cellule active rendue à la tâche `uid` du brouillon
    /// (H-UI-86).
    fn refuse_details(&mut self, uid: TaskUid, errors: Vec<FieldError>, detail: Option<String>) {
        let first = errors.first().map(|e| e.message.clone());
        if let Some(st) = self.view.details.as_mut() {
            st.errors = errors;
        }
        if self.selection != Some(uid) {
            self.selection = Some(uid);
            self.selected = crate::selection::Selection::single(Some(uid));
            self.sync_selection();
        }
        if let Some(message) = first {
            // Refus (H-UI-104) : gravité Erreur et titre des refus de
            // commande, comme `App::report_command_error`.
            let detail = detail.filter(|d| !d.is_empty());
            self.push_detailed(Severity::Error, Some(TextId::CommandErrorTitle), message, detail);
        }
    }
}
