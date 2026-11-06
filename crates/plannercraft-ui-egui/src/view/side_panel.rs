//! Volet latéral (ADR-0016 D10, J8 lot L12) : onglets « Opérations IA »
//! (H-UI-41 : écriture proposée par un agent en tête de file, table
//! avant/après, Accepter / Refuser, historique de session) et « Pourquoi
//! cette date ? » (H-UI-58 : `explain_date` de la tâche active, pilotes
//! cliquables).
//!
//! Règle 3 : le volet ne calcule rien. Les lignes avant/après viennent du
//! [`DiffView`] calculé **une fois** par le relais à l'arrivée de la demande
//! (`PendingApproval::diff`) ; les explications viennent du moteur
//! (`plannercraft_automation::explain::explain_date`), relues seulement quand la
//! tâche, la génération du document ou la langue changent
//! ([`SidePanelState::refresh_explain`]). Le volet ne prend jamais le focus
//! clavier : Accepter et Refuser suivent les règles du bandeau J7
//! ([`super::approval::decision_row`]).
//!
//! Onglet « Assistant » (ADR-0024, lot A1, H-UI-138 à H-UI-142) :
//! conversation avec l'assistant IA intégré, dessinée par
//! [`super::assistant`].
//!
//! Ouvert d'office à chaque demande d'un agent (onglet « Opérations IA »),
//! refermable (bouton du volet, Affichage → Opérations IA) ; fermé par
//! défaut, et jamais persisté : les captures J7 sont inchangées. Le bandeau
//! J7 reste affiché quand le volet est fermé ou sur l'autre onglet.

use std::collections::VecDeque;

use plannercraft_automation::command::TaskRef;
use plannercraft_automation::command::params::ExplainParams;
use plannercraft_automation::diff::{CriticalChange, DiffRow, DiffView, MAX_VIEW_ROWS, RowChange};
use plannercraft_automation::explain::{ExplainView, explain_date};
use plannercraft_automation::text::Language as ToolLanguage;
use plannercraft_core::{ConversionFactors, Minutes, Moment, TaskUid};

use super::FrameCtx;
use super::approval::{ApprovalBanner, decision_row};
use crate::action::{Action, UiOp};
use crate::app::OpenDocument;
use crate::format::{FormatLabels, format_date, format_slack};
use crate::i18n::{Arg, TextId};

/// Entrées au plus dans l'historique du panneau (règle 10 de J8).
pub const MAX_HISTORY: usize = 50;
/// Lignes au plus dans un diff affiché (règle 10 de J8) : celles du
/// [`DiffView`] du moteur.
pub const MAX_DIFF_ROWS: usize = MAX_VIEW_ROWS;

/// Largeur du volet à l'ouverture, en points logiques.
pub const DEFAULT_WIDTH: f32 = 460.0;

/// Onglet du volet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SideTab {
    /// Opérations IA.
    #[default]
    AiOperations,
    /// Pourquoi cette date ?
    WhyThisDate,
    /// Assistant IA (ADR-0024, lot A1, H-UI-138).
    Assistant,
}

/// Commande du volet (réducteur d'`App`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideOp {
    /// Ouvre ou referme le volet (Affichage → Opérations IA).
    Toggle,
    /// Ouvre le volet sur l'onglet donné.
    Open(SideTab),
    /// Referme le volet.
    Close,
    /// Ouvre le volet sur l'onglet donné, ou le referme s'il y est déjà
    /// (Affichage → Assistant IA, ADR-0024 lot A1).
    ToggleTab(SideTab),
}

/// Issue d'une proposition d'un agent (H-UI-41).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiOutcome {
    /// Acceptée et appliquée.
    Accepted,
    /// Refusée par l'utilisateur.
    Rejected,
    /// Délai de l'écouteur écoulé, ou agent parti, avant la réponse.
    Expired,
    /// Document fermé ou remplacé avant la réponse : rien d'appliqué.
    Stale,
    /// Acceptée mais refusée par le moteur à l'application (code stable).
    Failed(String),
}

/// Entrée de l'historique de session du panneau (H-UI-41).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiHistoryEntry {
    /// Outil demandé.
    pub tool: String,
    /// Nom du document visé.
    pub document: String,
    /// « Maintenant » injecté de la demande (règle 5).
    pub at: Moment,
    /// Issue.
    pub outcome: AiOutcome,
    /// Entrée d'annulation du lot accepté : (numéro d'ouverture, position
    /// de l'historique après le lot) ; `None` si rien n'a été appliqué.
    pub undo: Option<(u64, u64)>,
}

/// Ajoute `entry` à `history`, en retirant les plus anciennes au-delà de
/// [`MAX_HISTORY`].
pub fn push_history(history: &mut VecDeque<AiHistoryEntry>, entry: AiHistoryEntry) {
    history.push_back(entry);
    while history.len() > MAX_HISTORY {
        drop(history.pop_front());
    }
}

/// Clé d'une explication : document (ouverture, génération), tâche, langue.
type ExplainKey = (u64, u64, TaskUid, ToolLanguage);

/// Explication en cache (H-UI-58) : vue du moteur, ou message d'erreur.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExplainCache {
    key: ExplainKey,
    result: Result<ExplainView, String>,
}

/// État du volet : jamais sérialisé (fermé à chaque lancement).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidePanelState {
    /// Volet ouvert.
    pub open: bool,
    /// Onglet affiché.
    pub tab: SideTab,
    /// Explication de la tâche active (onglet « Pourquoi cette date ? »).
    explain: Option<ExplainCache>,
    /// Explications calculées depuis le lancement (tests : une par clé).
    explain_builds: u64,
}

impl SidePanelState {
    /// Applique une commande du volet.
    pub fn apply(&mut self, op: SideOp) {
        match op {
            SideOp::Toggle => self.open = !self.open,
            SideOp::Open(tab) => {
                self.open = true;
                self.tab = tab;
            }
            SideOp::Close => self.open = false,
            SideOp::ToggleTab(tab) => {
                if self.open && self.tab == tab {
                    self.open = false;
                } else {
                    self.open = true;
                    self.tab = tab;
                }
            }
        }
    }

    /// Vrai si l'onglet « Opérations IA » est visible (le bandeau J7 est
    /// alors remplacé par la tête du volet).
    #[must_use]
    pub fn shows_ai(&self) -> bool {
        self.open && self.tab == SideTab::AiOperations
    }

    /// Explications calculées depuis le lancement.
    #[must_use]
    pub const fn explain_builds(&self) -> u64 {
        self.explain_builds
    }

    /// Relit l'explication de la tâche `active` (H-UI-58) si l'onglet est
    /// visible et que la clé a changé ; rien sinon.
    pub fn refresh_explain(
        &mut self,
        doc: Option<&OpenDocument>,
        active: Option<TaskUid>,
        lang: ToolLanguage,
    ) {
        if !(self.open && self.tab == SideTab::WhyThisDate) {
            return;
        }
        let (Some(doc), Some(uid)) = (doc, active) else {
            self.explain = None;
            return;
        };
        let key = (doc.opening, doc.generation(), uid, lang);
        if self.explain.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        let params = ExplainParams {
            task: TaskRef::Uid(uid),
            side: None,
            depth: None,
        };
        let result = explain_date(&doc.session, &params).map_err(|e| e.message(lang));
        self.explain_builds = self.explain_builds.saturating_add(1);
        self.explain = Some(ExplainCache { key, result });
    }

    /// Explication en cache pour la tâche `active`, si elle est à jour de
    /// cette tâche.
    fn explanation(&self, active: Option<TaskUid>) -> Option<&Result<ExplainView, String>> {
        self.explain
            .as_ref()
            .filter(|c| Some(c.key.2) == active)
            .map(|c| &c.result)
    }
}

/// Données du relais lues par l'onglet « Opérations IA ».
#[derive(Clone, Copy, Debug)]
pub struct AiPanelInput<'a> {
    /// Proposition en tête de file, avec son résumé.
    pub banner: Option<ApprovalBanner<'a>>,
    /// Diff affiché de la proposition en tête de file.
    pub diff: Option<&'a DiffView>,
    /// Historique de session (le plus ancien d'abord).
    pub history: Option<&'a VecDeque<AiHistoryEntry>>,
}

impl AiPanelInput<'_> {
    /// Aucune donnée (web : pas de canal).
    pub const EMPTY: AiPanelInput<'static> = AiPanelInput {
        banner: None,
        diff: None,
        history: None,
    };
}

/// Dessine le volet (rien s'il est fermé) et pousse ses actions.
pub fn panel(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    state: &SidePanelState,
    ai: AiPanelInput<'_>,
    assistant: &mut crate::assistant::AssistantPanel,
) {
    if !state.open {
        return;
    }
    let texts = fc.texts;
    egui::Panel::right("plannercraft_side_panel")
        .resizable(true)
        .default_size(DEFAULT_WIDTH)
        .min_size(280.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (tab, id) in [
                    (SideTab::AiOperations, TextId::SideTabAi),
                    (SideTab::WhyThisDate, TextId::SideTabWhy),
                    (SideTab::Assistant, TextId::SideTabAssistant),
                ] {
                    if ui.selectable_label(state.tab == tab, texts.get(id)).clicked() && state.tab != tab {
                        fc.actions.push(Action::SidePanel(SideOp::Open(tab)));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(texts.get(TextId::SideClose)).clicked() {
                        fc.actions.push(Action::SidePanel(SideOp::Close));
                    }
                });
            });
            ui.separator();
            match state.tab {
                SideTab::AiOperations => ai_tab(ui, fc, ai),
                SideTab::WhyThisDate => why_tab(ui, fc, state),
                SideTab::Assistant => super::assistant::tab(ui, fc, assistant),
            }
        });
}

/// Facteurs de conversion du document (marges en jours ouvrés).
fn factors(fc: &FrameCtx<'_>) -> ConversionFactors {
    fc.doc.map(|d| d.project().options.factors).unwrap_or_default()
}

/// Onglet « Opérations IA » : tête de file, diff, historique.
fn ai_tab(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, ai: AiPanelInput<'_>) {
    let texts = fc.texts;
    if let Some(banner) = ai.banner {
        decision_row(ui, fc, banner);
        if let Some(diff) = ai.diff {
            diff_table(ui, fc, diff);
        } else {
            let detail = banner.path.unwrap_or(banner.tool);
            ui.label(texts.get_args(TextId::AiNoPreview, &[("detail", Arg::Str(detail.into()))]));
        }
    } else {
        ui.label(texts.get(TextId::AiNoPending));
    }
    ui.separator();
    ui.label(egui::RichText::new(texts.get(TextId::AiHistory)).strong());
    let history = ai.history.filter(|h| !h.is_empty());
    let Some(history) = history else {
        ui.label(texts.get(TextId::AiHistoryEmpty));
        return;
    };
    // Position courante de l'historique du document : seul le dernier lot
    // annulable garde son lien d'annulation.
    let top = fc.doc.map(|d| (d.opening, d.session.history_position()));
    let labels = texts.format_labels();
    egui::ScrollArea::vertical()
        .id_salt("plannercraft_ai_history")
        .max_height(220.0)
        .show(ui, |ui| {
            for (i, entry) in history.iter().enumerate().rev() {
                ui.push_id(("plannercraft_ai_history_entry", i), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(history_line(fc, labels, entry));
                        if entry.undo.is_some()
                            && entry.undo == top
                            && ui.small_button(texts.get(TextId::AiUndoEntry)).clicked()
                        {
                            fc.actions.push(Action::Undo);
                        }
                    });
                });
            }
        });
}

/// Ligne d'historique : heure, outil, document, issue.
fn history_line(fc: &FrameCtx<'_>, labels: &FormatLabels, entry: &AiHistoryEntry) -> String {
    let texts = fc.texts;
    let outcome = match &entry.outcome {
        AiOutcome::Accepted => texts.get(TextId::AiOutcomeAccepted),
        AiOutcome::Rejected => texts.get(TextId::AiOutcomeRejected),
        AiOutcome::Expired => texts.get(TextId::AiOutcomeExpired),
        AiOutcome::Stale => texts.get(TextId::AiOutcomeStale),
        AiOutcome::Failed(code) => texts.get_args(
            TextId::AiOutcomeFailed,
            &[("detail", Arg::Str(code.as_str().into()))],
        ),
    };
    let time = entry.at.to_naive().time().format("%H:%M").to_string();
    let at = format!("{} {time}", format_date(entry.at, labels));
    texts.get_args(
        TextId::AiHistoryLine,
        &[
            ("value", Arg::Str(at.into())),
            ("name", Arg::Str(entry.tool.as_str().into())),
            ("document", Arg::Str(entry.document.as_str().into())),
            ("detail", Arg::Str(outcome.into())),
        ],
    )
}

/// Table avant/après (H-UI-41) : ID, nom, début, fin, marge, critique.
/// Chaque cellule montre la valeur d'après ; si elle change, la valeur
/// d'avant suit, barrée et estompée.
fn diff_table(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, diff: &DiffView) {
    let texts = fc.texts;
    let labels = texts.format_labels();
    let factors = factors(fc);
    let muted = fc.palette.text_weak;
    egui::ScrollArea::both()
        .id_salt("plannercraft_ai_diff")
        .max_height(ui.available_height() * 0.6)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("plannercraft_ai_diff_grid")
                .striped(true)
                .num_columns(6)
                .show(ui, |ui| {
                    for id in [
                        TextId::ColId,
                        TextId::ColName,
                        TextId::ColStart,
                        TextId::ColFinish,
                        TextId::ColTotalSlack,
                        TextId::ColCritical,
                    ] {
                        ui.label(egui::RichText::new(texts.get(id)).strong());
                    }
                    ui.end_row();
                    for row in diff.rows.iter().take(MAX_DIFF_ROWS) {
                        diff_row(ui, fc, row, labels, factors, muted);
                        ui.end_row();
                    }
                });
        });
    if diff.more > 0 {
        let more = i64::try_from(diff.more).unwrap_or(i64::MAX);
        ui.label(texts.get_args(TextId::AiMore, &[("count", Arg::Count(more))]));
    }
}

/// Une ligne de la table avant/après.
fn diff_row(
    ui: &mut egui::Ui,
    fc: &FrameCtx<'_>,
    row: &DiffRow,
    labels: &FormatLabels,
    factors: ConversionFactors,
    muted: egui::Color32,
) {
    let texts = fc.texts;
    ui.label(row.id.map(|id| id.get().to_string()).unwrap_or_default());
    let name = match row.change {
        RowChange::Added => texts.get_args(TextId::AiAdded, &[("name", Arg::Str(row.name.as_str().into()))]),
        RowChange::Removed => {
            texts.get_args(TextId::AiRemoved, &[("name", Arg::Str(row.name.as_str().into()))])
        }
        RowChange::Changed | RowChange::CriticalOnly => row.name.clone(),
    };
    ui.label(name);
    let date = |m: Option<Moment>| m.map(|m| format_date(m, labels)).unwrap_or_default();
    let slack = |m: Option<Minutes>| m.map(|m| format_slack(m, &factors, labels)).unwrap_or_default();
    pair_cell(ui, &date(row.start.0), &date(row.start.1), muted);
    pair_cell(ui, &date(row.finish.0), &date(row.finish.1), muted);
    pair_cell(ui, &slack(row.total_slack.0), &slack(row.total_slack.1), muted);
    let critical = match row.critical {
        CriticalChange::Same => String::new(),
        CriticalChange::Entered => texts.get(TextId::AiCriticalEntered),
        CriticalChange::Left => texts.get(TextId::AiCriticalLeft),
    };
    ui.label(critical);
}

/// Cellule avant/après : la valeur d'après, puis celle d'avant barrée si
/// elle diffère.
fn pair_cell(ui: &mut egui::Ui, before: &str, after: &str, muted: egui::Color32) {
    ui.vertical(|ui| {
        if after.is_empty() {
            ui.label(egui::RichText::new(before).strikethrough().color(muted));
            return;
        }
        ui.label(after);
        if before != after && !before.is_empty() {
            ui.label(egui::RichText::new(before).strikethrough().small().color(muted));
        }
    });
}

/// Onglet « Pourquoi cette date ? » (H-UI-58) : résumé du moteur
/// (gabarits R-233), puis les pilotes, cliquables (sélection de la tâche).
fn why_tab(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, state: &SidePanelState) {
    let texts = fc.texts;
    let Some(result) = state.explanation(fc.selection) else {
        ui.label(texts.get(TextId::WhyNoTask));
        return;
    };
    let view = match result {
        Ok(view) => view,
        Err(message) => {
            ui.label(texts.get_args(
                TextId::WhyUnavailable,
                &[("detail", Arg::Str(message.as_str().into()))],
            ));
            return;
        }
    };
    let id = view.task.id.map(|id| id.to_string()).unwrap_or_default();
    ui.label(egui::RichText::new(format!("{id} {}", view.task.name)).strong());
    let summary = match texts.language() {
        crate::Language::Fr => &view.summary_fr,
        crate::Language::En => &view.summary_en,
    };
    ui.label(summary.as_str());
    if view.chain.is_empty() {
        return;
    }
    ui.separator();
    ui.label(texts.get(TextId::WhyDrivers));
    for (i, link) in view.chain.iter().enumerate() {
        let id = link.id.map(|id| id.to_string()).unwrap_or_default();
        let clicked = ui
            .push_id(("plannercraft_why_driver", i), |ui| {
                ui.link(format!("{id} {}", link.name))
            })
            .inner
            .clicked();
        if clicked {
            fc.actions.push(Action::Ui(UiOp::Select(Some(link.uid))));
            fc.actions.push(Action::Ui(UiOp::ScrollTo(link.uid)));
        }
    }
}
