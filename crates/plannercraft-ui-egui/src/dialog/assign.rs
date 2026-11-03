//! « Affecter des ressources » (J7, lot L13, S70) : fenêtre **non
//! modale** sur les tâches sélectionnées (plusieurs à la fois), relue à
//! chaque changement de la sélection ou du document.
//!
//! Filtre texte sur le nom, colonne Unités ; « Affecter », « Retirer » et
//! « Modifier les unités » produisent chacun **un seul lot**
//! (`assignment.add`, `assignment.remove`, `assignment.update`), donc une
//! entrée d'annulation, appliqué au clic comme dans MS Project (la fenêtre
//! reste ouverte). Unités laissées vides : le lot ne les donne pas et le
//! registre prend sa valeur par défaut (R-196 : capacité de la ressource,
//! au plus 100 % ; S70) ; la valeur rendue est relue sur le modèle et
//! affichée telle quelle. Aucune règle ni aucun nombre n'est calculé ici :
//! les unités saisies sont lues par le registre (`UnitsJson`), dont le
//! refus s'affiche dans la fenêtre. Aucun raccourci (S61, S70).
//!
//! Hypothèses d'interface (non numérotées) : une ressource se choisit par
//! un clic sur son nom (Ctrl ou Cmd + clic : plusieurs) ; les opérations
//! portent sur les ressources choisies **et visibles** (le filtre masque
//! aussi le choix) ; « Affecter » ignore une ressource déjà affectée à une
//! tâche (MS Project la garde telle quelle) ; une saisie d'unités non
//! appliquée survit au changement de sélection.

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_automation::command::{AssignmentAdd, AssignmentKey, AssignmentUpdate, ResourceRef};
use plannercraft_automation::text::{Language, TextContext};
use plannercraft_automation::{Batch, Command, Patch, Session, TaskRef, view};
use plannercraft_core::{Assignment, HasUid, ResourceKind, ResourceUid, TaskUid};

use super::task_info::{units_json, units_text};
use super::{batch_of, error_line, text_field};
use crate::i18n::{Arg, TextId, Texts};

/// Hauteur fixe de la liste des ressources (une zone défilante sans
/// hauteur minimale grandit sur plusieurs images, écart du lot L12).
const LIST_HEIGHT: f32 = 260.0;

/// Ligne d'une ressource dans la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignResource {
    /// Ressource.
    pub resource: ResourceUid,
    /// Nom affiché.
    pub name: String,
    /// Nom en minuscules (filtre).
    lower: String,
    /// Type de la ressource (unités, quantité ou rien).
    pub kind: ResourceKind,
    /// Tâches de la sélection auxquelles la ressource est affectée.
    pub on: BTreeSet<TaskUid>,
    /// Unités lues sur le modèle : celles des affectations de la sélection
    /// si elles sont toutes égales, vide sinon.
    pub read: String,
    /// Unités saisies (égales à `read` tant que rien n'est saisi).
    pub units: String,
}

impl AssignResource {
    /// Saisie en attente (texte différent de la valeur relue).
    fn dirty(&self) -> bool {
        self.units.trim() != self.read.trim()
    }
}

/// Bouton de la fenêtre qui produit un lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    /// « Affecter ».
    Assign,
    /// « Retirer ».
    Remove,
    /// « Modifier les unités ».
    UpdateUnits,
}

/// Réponse de la fenêtre pour une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOutcome {
    /// Rien.
    Open,
    /// « Fermer », la croix ou Échap.
    Close,
    /// Un bouton à lot.
    Apply(AssignOp),
}

/// État de la fenêtre (transitoire, jamais sérialisé).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignState {
    /// Clé du document relu (numéro d'ouverture, génération).
    pub key: (u64, u64),
    /// Sélection relue, telle que l'application la tient.
    selection: BTreeSet<TaskUid>,
    /// Tâches de la sélection, dans l'ordre du projet (lignes vides
    /// exclues).
    pub tasks: Vec<TaskUid>,
    /// Nom de la tâche quand une seule est sélectionnée.
    pub task_name: Option<String>,
    /// Filtre sur le nom (sans casse).
    pub filter: String,
    /// Ressources du projet, dans son ordre (ressource cachée d'UID 0
    /// exclue, comme dans la feuille des ressources).
    pub rows: Vec<AssignResource>,
    /// Ressources choisies.
    pub chosen: BTreeSet<ResourceUid>,
    /// Refus du dernier lot (registre), traduit.
    pub error: Option<String>,
}

impl AssignState {
    /// Fenêtre relue sur la session pour la sélection `selection`.
    #[must_use]
    pub fn open(s: &Session, key: (u64, u64), lang: Language, selection: &BTreeSet<TaskUid>) -> Self {
        let mut state = Self {
            key,
            selection: BTreeSet::new(),
            tasks: Vec::new(),
            task_name: None,
            filter: String::new(),
            rows: Vec::new(),
            chosen: BTreeSet::new(),
            error: None,
        };
        state.reread(s, key, lang, selection);
        state
    }

    /// Vrai si la fenêtre montre le document `key` et la sélection
    /// `selection`.
    #[must_use]
    pub fn shows(&self, key: (u64, u64), selection: &BTreeSet<TaskUid>) -> bool {
        self.key == key && &self.selection == selection
    }

    /// Relit tâches et ressources. Garde le filtre, le choix des ressources
    /// encore présentes et les unités saisies non appliquées.
    pub fn reread(&mut self, s: &Session, key: (u64, u64), lang: Language, selection: &BTreeSet<TaskUid>) {
        let project = s.project();
        let ctx = TextContext::for_project(project, lang);
        let tasks: Vec<TaskUid> = project
            .tasks
            .iter()
            .filter(|t| !t.is_null && selection.contains(&t.uid()))
            .map(HasUid::uid)
            .collect();
        let task_name = match tasks.as_slice() {
            [one] => project.tasks.get(*one).map(|t| t.name.clone()),
            _ => None,
        };
        let task_set: BTreeSet<TaskUid> = tasks.iter().copied().collect();
        let hidden = ResourceUid::new(0);
        let mut rows: Vec<AssignResource> = project
            .resources
            .iter()
            .filter(|r| r.uid() != hidden)
            .map(|r| AssignResource {
                resource: r.uid(),
                name: r.name.clone(),
                lower: r.name.to_lowercase(),
                kind: r.kind.clone(),
                on: BTreeSet::new(),
                read: String::new(),
                units: String::new(),
            })
            .collect();
        // Unités des affectations de la sélection, par ressource : la
        // première vue, `None` dès qu'une autre diffère.
        let index: BTreeMap<ResourceUid, usize> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| (row.resource, i))
            .collect();
        let mut common: Vec<Option<Option<String>>> = vec![None; rows.len()];
        for a in &project.assignments {
            let Some(resource) = a.resource else {
                continue;
            };
            if !task_set.contains(&a.task) {
                continue;
            }
            let Some(&i) = index.get(&resource) else {
                continue;
            };
            if let (Some(row), Some(slot)) = (rows.get_mut(i), common.get_mut(i)) {
                row.on.insert(a.task);
                let text = assignment_text(&row.kind, a, &ctx);
                *slot = match slot.take() {
                    None => Some(Some(text)),
                    Some(Some(u)) if u == text => Some(Some(u)),
                    Some(_) => Some(None),
                };
            }
        }
        // Saisies non appliquées, gardées.
        let pending: BTreeMap<ResourceUid, &str> = self
            .rows
            .iter()
            .filter(|old| old.dirty())
            .map(|old| (old.resource, old.units.as_str()))
            .collect();
        for (row, slot) in rows.iter_mut().zip(&common) {
            if let Some(Some(text)) = slot {
                row.read.clone_from(text);
            }
            row.units = pending
                .get(&row.resource)
                .map_or_else(|| row.read.clone(), |text| (*text).to_owned());
        }
        self.chosen
            .retain(|uid| rows.iter().any(|row| row.resource == *uid));
        self.key = key;
        self.selection.clone_from(selection);
        self.tasks = tasks;
        self.task_name = task_name;
        self.rows = rows;
    }

    /// Ligne visible par le filtre.
    fn matches(&self, row: &AssignResource) -> bool {
        let filter = self.filter.trim().to_lowercase();
        filter.is_empty() || row.lower.contains(&filter)
    }

    /// Lignes visibles par le filtre, dans l'ordre du projet.
    pub fn visible(&self) -> impl Iterator<Item = &AssignResource> {
        self.rows.iter().filter(|row| self.matches(row))
    }

    /// Lignes choisies et visibles.
    fn targets(&self) -> impl Iterator<Item = &AssignResource> {
        self.visible().filter(|row| self.chosen.contains(&row.resource))
    }

    /// Choisit `resource` : seule, ou ajoutée / retirée (`toggle`).
    pub fn choose(&mut self, resource: ResourceUid, toggle: bool) {
        if toggle {
            if !self.chosen.remove(&resource) {
                self.chosen.insert(resource);
            }
        } else {
            self.chosen = BTreeSet::from([resource]);
        }
    }

    /// Vrai si le bouton `op` a quelque chose à faire.
    #[must_use]
    pub fn can(&self, op: AssignOp) -> bool {
        match op {
            AssignOp::Assign => !self.tasks.is_empty() && self.targets().next().is_some(),
            AssignOp::Remove | AssignOp::UpdateUnits => self.targets().any(|row| !row.on.is_empty()),
        }
    }

    /// Lot du bouton `op` (fonction pure) ; rien à faire ⇒ `None`. Unités
    /// saisies envoyées en texte (« 50 » vaut 50 % pour une ressource de
    /// travail), lues par le registre ; ressource coût : ni unités ni
    /// quantité.
    #[must_use]
    pub fn to_batch(&self, op: AssignOp, lang: Language) -> Option<Batch> {
        let mut ops = Vec::new();
        for row in self.targets() {
            let text = row.units.trim();
            let (units, quantity) = match (&row.kind, text.is_empty()) {
                (_, true) | (ResourceKind::Cost, _) => (None, None),
                (ResourceKind::Work, false) => (Some(units_json(text, true)), None),
                (ResourceKind::Material { .. }, false) => (None, Some(units_json(text, false))),
            };
            let key = |task: TaskUid| AssignmentKey {
                task: TaskRef::Uid(task),
                resource: ResourceRef::Uid(row.resource),
            };
            match op {
                AssignOp::Assign => {
                    for task in self.tasks.iter().filter(|t| !row.on.contains(t)) {
                        ops.push(Command::AssignmentAdd(AssignmentAdd {
                            task: TaskRef::Uid(*task),
                            resource: ResourceRef::Uid(row.resource),
                            units: units.clone(),
                            work: None,
                            quantity: quantity.clone(),
                            delay: None,
                        }));
                    }
                }
                AssignOp::Remove => {
                    ops.extend(row.on.iter().map(|task| Command::AssignmentRemove(key(*task))));
                }
                AssignOp::UpdateUnits => {
                    if !row.dirty() || (units.is_none() && quantity.is_none()) {
                        continue;
                    }
                    for task in self.tasks.iter().filter(|t| row.on.contains(t)) {
                        let AssignmentKey { task, resource } = key(*task);
                        ops.push(Command::AssignmentUpdate(AssignmentUpdate {
                            task,
                            resource,
                            units: units.clone().map_or(Patch::Keep, Patch::Set),
                            work: Patch::Keep,
                            quantity: quantity.clone().map_or(Patch::Keep, Patch::Set),
                            delay: Patch::Keep,
                        }));
                    }
                }
            }
        }
        (!ops.is_empty()).then(|| batch_of(ops, lang))
    }

    /// Lot appliqué : saisies des lignes choisies abandonnées (relues à
    /// l'image suivante), refus effacé.
    pub fn applied(&mut self) {
        let chosen = &self.chosen;
        for row in self.rows.iter_mut().filter(|row| chosen.contains(&row.resource)) {
            row.units.clone_from(&row.read);
        }
        self.error = None;
    }
}

/// Unités d'une affectation telles que le registre les relit : pourcentage
/// (travail), quantité tirée du travail (matériel, `view::material_quantity`,
/// comme `assignment.list`), rien (coût).
fn assignment_text(kind: &ResourceKind, a: &Assignment, ctx: &TextContext) -> String {
    match kind {
        ResourceKind::Work => units_text(Some(kind), a.units.get(), ctx),
        ResourceKind::Material { .. } => view::quantity_out(view::material_quantity(a.work), ctx)
            .text
            .unwrap_or_default(),
        ResourceKind::Cost => String::new(),
    }
}

/// Identifiant de la fenêtre.
#[must_use]
pub fn window_id() -> egui::Id {
    egui::Id::unique("plannercraft.assign-resources")
}

/// Texte de la colonne « Affectée ».
fn assigned_text(texts: &Texts, row: &AssignResource, tasks: usize) -> String {
    match row.on.len() {
        0 => String::new(),
        n if n == tasks => texts.get(TextId::AsAssignedAll),
        n => texts.get_args(
            TextId::AsAssignedSome,
            &[
                ("count", Arg::Count(i64::try_from(n).unwrap_or(i64::MAX))),
                ("value", Arg::Str(tasks.to_string().into())),
            ],
        ),
    }
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut AssignState) -> AssignOutcome {
    let mut outcome = AssignOutcome::Open;
    let mut open = true;
    egui::Window::new(texts.get(TextId::AsTitle))
        .id(window_id())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(480.0)
        // À droite, sous la barre d'outils : la table (à gauche) reste
        // visible et cliquable.
        .pivot(egui::Align2::RIGHT_TOP)
        .default_pos({
            let screen = ctx.content_rect();
            egui::pos2(screen.right() - 16.0, screen.top() + 72.0)
        })
        .show(ctx, |ui| {
            let header = match (&state.task_name, state.tasks.len()) {
                (_, 0) => texts.get(TextId::AsNoTask),
                (Some(name), 1) => texts.get_args(TextId::AsTaskOne, &[("name", Arg::Str(name.clone().into()))]),
                (_, n) => texts.get_args(
                    TextId::AsTaskMany,
                    &[("count", Arg::Count(i64::try_from(n).unwrap_or(i64::MAX)))],
                ),
            };
            ui.label(header);
            let filter = texts.get(TextId::AsFilter);
            ui.horizontal(|ui| {
                ui.label(&filter);
                text_field(ui, &filter, &mut state.filter, true, 240.0);
            });
            ui.separator();
            rows(ui, texts, state);
            ui.small(texts.get(TextId::AsHint));
            error_line(ui, state.error.as_deref());
            ui.separator();
            ui.horizontal(|ui| {
                for (op, label) in [
                    (AssignOp::Assign, TextId::AsAssign),
                    (AssignOp::Remove, TextId::AsRemove),
                    (AssignOp::UpdateUnits, TextId::AsUpdateUnits),
                ] {
                    if ui.add_enabled(state.can(op), egui::Button::new(texts.get(label))).clicked() {
                        outcome = AssignOutcome::Apply(op);
                    }
                }
                if ui.button(texts.get(TextId::AsClose)).clicked() {
                    outcome = AssignOutcome::Close;
                }
            });
        });
    if !open || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        outcome = AssignOutcome::Close;
    }
    outcome
}

/// Liste des ressources : Affectée, nom (clic : choix), unités.
fn rows(ui: &mut egui::Ui, texts: &Texts, state: &mut AssignState) {
    let tasks = state.tasks.len();
    let mut clicked = None;
    let toggle = ui.input(|i| i.modifiers.command || i.modifiers.ctrl || i.modifiers.mac_cmd);
    egui::ScrollArea::vertical()
        .min_scrolled_height(LIST_HEIGHT)
        .max_height(LIST_HEIGHT)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut any = false;
            egui::Grid::new("plannercraft.assign-resources.rows")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong(texts.get(TextId::AsAssigned));
                    ui.strong(texts.get(TextId::AsResource));
                    ui.strong(texts.get(TextId::AsUnits));
                    ui.end_row();
                    let filter = state.filter.trim().to_lowercase();
                    let chosen = &state.chosen;
                    for row in state
                        .rows
                        .iter_mut()
                        .filter(|row| filter.is_empty() || row.lower.contains(&filter))
                    {
                        any = true;
                        ui.label(assigned_text(texts, row, tasks));
                        let selected = chosen.contains(&row.resource);
                        if ui.selectable_label(selected, &row.name).clicked() {
                            clicked = Some(row.resource);
                        }
                        let label =
                            texts.get_args(TextId::AsUnitsOf, &[("name", Arg::Str(row.name.clone().into()))]);
                        let enabled = !matches!(row.kind, ResourceKind::Cost);
                        text_field(ui, &label, &mut row.units, enabled, 90.0);
                        ui.end_row();
                    }
                });
            if !any {
                ui.weak(texts.get(TextId::AsNoResource));
            }
        });
    if let Some(resource) = clicked {
        state.choose(resource, toggle);
    }
}
