//! Boîte « Dépendance » (J7, lot L8, H-UI-21, S69) : ouverte par un
//! double-clic sur le tracé d'un lien du Gantt. Montre le prédécesseur et
//! le successeur (lus), le type et le décalage (saisi au format de R-141,
//! « 2j », « -50% ») ; « OK » produit au plus un `link.update` (champs
//! changés seulement), « Supprimer » un `link.remove`, chacun un seul lot
//! (une entrée d'annulation). Le brouillon est figé à l'ouverture
//! (`key`) ; aucune date ni durée n'est calculée ici (règle 3) : le texte
//! du décalage est lu par `plannercraft_automation::text::parse_lag`.

use plannercraft_automation::command::{LagJson, LinkKey, LinkSpec, LinkTypeJson};
use plannercraft_automation::text::{Language, TextContext, format_lag, parse_lag};
use plannercraft_automation::{Batch, Command, Session, TaskRef};
use plannercraft_core::{Lag, LagValue, LinkType, Minutes, TaskUid};

use crate::edit::EditError;
use crate::i18n::{Arg, TextId, Texts};

/// Types de lien proposés, dans l'ordre de MS Project.
const LINK_TYPES: [(LinkType, TextId); 4] = [
    (LinkType::FinishToStart, TextId::TaskInfoLinkFs),
    (LinkType::StartToStart, TextId::TaskInfoLinkSs),
    (LinkType::FinishToFinish, TextId::TaskInfoLinkFf),
    (LinkType::StartToFinish, TextId::TaskInfoLinkSf),
];

/// Brouillon de la boîte d'un lien.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkState {
    /// Prédécesseur.
    pub predecessor: TaskUid,
    /// Successeur.
    pub successor: TaskUid,
    /// Clé du document à l'ouverture : (numéro d'ouverture, génération).
    pub key: (u64, u64),
    /// « ID Nom » du prédécesseur.
    pub from: String,
    /// « ID Nom » du successeur.
    pub to: String,
    /// Type saisi.
    pub kind: LinkType,
    /// Décalage saisi (vide : nul).
    pub lag: String,
    /// Type relu à l'ouverture.
    pub original_kind: LinkType,
    /// Décalage relu à l'ouverture.
    pub original_lag: String,
    /// Refus à afficher (lecture du décalage ou registre).
    pub error: Option<String>,
    /// Le document a changé : brouillon relu, avertissement affiché.
    pub stale: bool,
}

/// Réponse de la boîte pour une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkOutcome {
    /// Rien : la boîte reste ouverte.
    Open,
    /// « OK ».
    Ok,
    /// « Supprimer ».
    Delete,
    /// « Annuler » ou Échap.
    Cancel,
}

/// Décalage nul (temps ou pourcentage), quelle que soit son unité.
fn is_zero_lag(lag: &Lag) -> bool {
    match lag.value {
        LagValue::Time { minutes, .. } => minutes == Minutes::ZERO,
        LagValue::Percent { percent } => percent.get() == 0,
    }
}

/// « ID Nom » d'une tâche.
fn task_label(s: &Session, texts: &Texts, uid: TaskUid) -> String {
    let project = s.project();
    let id = project.task_id(uid).map_or(0, plannercraft_core::TaskId::get);
    let name = project.tasks.get(uid).map(|t| t.name.clone()).unwrap_or_default();
    texts.get_args(
        TextId::LinkDialogTask,
        &[("id", Arg::Count(i64::from(id))), ("name", Arg::Str(name.into()))],
    )
}

impl LinkState {
    /// Brouillon du lien `predecessor` → `successor`, textes dans la langue
    /// `language` ; `None` si le lien n'existe pas.
    #[must_use]
    pub fn open(
        s: &Session,
        texts: &Texts,
        predecessor: TaskUid,
        successor: TaskUid,
        key: (u64, u64),
        language: Language,
    ) -> Option<Self> {
        let project = s.project();
        let link = project
            .tasks
            .get(successor)?
            .predecessors
            .iter()
            .find(|l| l.predecessor == predecessor)?;
        let ctx = TextContext::for_project(project, language);
        let lag_text = if is_zero_lag(&link.lag) {
            String::new()
        } else {
            format_lag(&link.lag, &ctx)
        };
        Some(Self {
            predecessor,
            successor,
            key,
            from: task_label(s, texts, predecessor),
            to: task_label(s, texts, successor),
            kind: link.kind,
            lag: lag_text.clone(),
            original_kind: link.kind,
            original_lag: lag_text,
            error: None,
            stale: false,
        })
    }

    /// Lot de « OK » : `link.update` des seuls champs changés (décalage
    /// effacé : zéro explicite) ; `Ok(None)` si rien n'a changé.
    ///
    /// # Errors
    ///
    /// Message traduit si le décalage est illisible.
    pub fn to_batch(&self, s: &Session, texts: &Texts, language: Language) -> Result<Option<Batch>, String> {
        let kind = (self.kind != self.original_kind).then(|| LinkTypeJson::from(self.kind));
        let lag_json = if self.lag.trim() == self.original_lag.trim() {
            None
        } else {
            let ctx = TextContext::for_project(s.project(), language);
            let value = if self.lag.trim().is_empty() {
                Lag::ZERO
            } else {
                parse_lag(&self.lag, &ctx).map_err(|e| EditError::from(e).message(texts))?
            };
            Some(LagJson {
                text: None,
                ..LagJson::from_lag(&value, &ctx)
            })
        };
        if kind.is_none() && lag_json.is_none() {
            return Ok(None);
        }
        Ok(Some(one(Command::LinkUpdate(LinkSpec {
            pred: TaskRef::Uid(self.predecessor),
            succ: TaskRef::Uid(self.successor),
            kind,
            lag: lag_json,
        }))))
    }

    /// Lot de « Supprimer » : `link.remove`.
    #[must_use]
    pub fn delete_batch(&self) -> Batch {
        one(Command::LinkRemove(LinkKey {
            pred: TaskRef::Uid(self.predecessor),
            succ: TaskRef::Uid(self.successor),
        }))
    }
}

/// Lot d'une opération.
fn one(op: Command) -> Batch {
    Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: None,
        ops: vec![op],
        label: None,
    }
}

/// Identifiant de la boîte (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.link-dialog")
}

/// Ligne « libellé : valeur » lue.
fn read_only(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.label(value);
    ui.end_row();
}

/// Dessine la boîte ; rend la réponse de l'image.
pub fn show(ctx: &egui::Context, texts: &Texts, state: &mut LinkState) -> LinkOutcome {
    let mut outcome = LinkOutcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.heading(texts.get(TextId::LinkDialogTitle));
        if state.stale {
            ui.colored_label(ui.visuals().warn_fg_color, texts.get(TextId::LinkDialogStale));
        }
        egui::Grid::new("plannercraft.link-dialog.grid")
            .num_columns(2)
            .show(ui, |ui| {
                read_only(ui, &texts.get(TextId::LinkDialogFrom), &state.from);
                read_only(ui, &texts.get(TextId::LinkDialogTo), &state.to);
                let type_label = texts.get(TextId::LinkDialogType);
                ui.label(&type_label);
                let selected = LINK_TYPES
                    .iter()
                    .find(|(k, _)| *k == state.kind)
                    .map(|(_, t)| texts.get(*t))
                    .unwrap_or_default();
                let combo = egui::ComboBox::from_id_salt("plannercraft.link-dialog.type")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (k, t) in LINK_TYPES {
                            ui.selectable_value(&mut state.kind, k, texts.get(t));
                        }
                    })
                    .response;
                combo.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::ComboBox, true, type_label.clone())
                });
                ui.end_row();
                let lag_label = texts.get(TextId::LinkDialogLag);
                ui.label(&lag_label);
                let field = ui.add(egui::TextEdit::singleline(&mut state.lag).desired_width(120.0));
                field.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::TextInput, true, lag_label.clone())
                });
                ui.end_row();
            });
        if let Some(error) = &state.error {
            ui.colored_label(ui.visuals().error_fg_color, error.as_str());
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(texts.get(TextId::TaskInfoOk)).clicked() {
                outcome = LinkOutcome::Ok;
            }
            if ui.button(texts.get(TextId::TaskInfoCancel)).clicked() {
                outcome = LinkOutcome::Cancel;
            }
            if ui.button(texts.get(TextId::LinkDialogDelete)).clicked() {
                outcome = LinkOutcome::Delete;
            }
        });
    });
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        outcome = LinkOutcome::Cancel;
    }
    if outcome == LinkOutcome::Open
        && super::enter_pressed(ctx, armed, response.is_top_modal, response.any_popup_open)
    {
        outcome = LinkOutcome::Ok;
    }
    outcome
}
