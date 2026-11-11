//! Bandeau de validation d'une écriture d'un agent (H-UI-27, ADR-0015 D10,
//! lot L3) : « L'agent propose sur « plan.xml » : task.update — 3 tâches
//! modifiées · 3 dates déplacées, fin +2j, chemin critique modifié »,
//! Accepter / Refuser.
//!
//! Le bandeau ne calcule rien (règle 3) : les compteurs et l'écart de fin
//! (texte du moteur) viennent du `dry_run` de la commande ([`ApprovalSummary`],
//! construit par `crate::relay`). Un seul bandeau à la fois, les suivants
//! attendent (compteur « +N en attente »). Le bandeau ne prend **jamais**
//! le focus clavier (relecture finale J7, interface F1) : un Entrée ou un
//! Espace de navigation ou de saisie au moment où il paraît validerait
//! l'écriture sans qu'elle soit lue. Accepter au clavier : Ctrl/Cmd+Entrée
//! (geste délibéré, hors champ de texte) ; Échap refuse seulement si un
//! bouton du bandeau a le focus (atteint par Tab, hors table).

use egui::{Key, Modifiers};

use crate::action::Action;
use crate::approvals::ProposalOp;
use crate::i18n::{Arg, TextId};
use crate::view::FrameCtx;

/// Résumé d'une écriture proposée, tiré du diff de son `dry_run` (R-163) ;
/// l'interface ne calcule rien.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApprovalSummary {
    /// Tâches aux dates déplacées (`diff.changed_total`).
    pub changed_tasks: u32,
    /// Tâches touchées par l'écriture (`diff.touched_total`, R-163
    /// amendée), jamais moins que `changed_tasks`.
    pub touched_tasks: u32,
    /// Écart de fin du projet en minutes ouvrées (`diff.finish_delta`).
    pub finish_delta: Option<i64>,
    /// Même écart en texte du moteur (« 2j », « -1j »), signé à
    /// l'affichage.
    pub finish_delta_text: Option<String>,
    /// Le chemin critique change (tâches entrées ou sorties).
    pub critical_changed: bool,
    /// Avertissements nouveaux.
    pub warnings: u32,
}

/// Bandeau à dessiner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApprovalBanner<'a> {
    /// Appel relayé (identifiant de la requête de l'agent).
    pub id: u64,
    /// Outil demandé (`task.update`, `project.save`…).
    pub tool: &'a str,
    /// Libellé d'une proposition de l'assistant (lot A2).
    pub label: Option<&'a str>,
    /// Proposition de l'assistant intégré (origine « Assistant », et non
    /// un agent MCP) : Accepter et Refuser poussent [`Action::Proposal`].
    pub assistant: bool,
    /// Nom du document visé (relecture finale J7, relais F2 : l'utilisateur
    /// voit sur quel document l'écriture porte).
    pub document: &'a str,
    /// Résumé du `dry_run` ; `None` pour un appel sans aperçu
    /// (enregistrer, exporter, annuler, rétablir).
    pub summary: Option<&'a ApprovalSummary>,
    /// Chemin visé par un enregistrement ou un export, s'il est donné.
    pub path: Option<&'a str>,
    /// Appels en attente derrière celui-ci.
    pub queued: usize,
}

/// Phrase du bandeau : outil et effets du `dry_run`.
#[must_use]
pub fn banner_text(fc: &FrameCtx<'_>, banner: &ApprovalBanner<'_>) -> String {
    let texts = fc.texts;
    let mut parts: Vec<String> = Vec::new();
    match banner.summary {
        Some(summary) => {
            parts.push(texts.get_args(
                TextId::ApprovalChanged,
                &[
                    ("touched", Arg::Count(i64::from(summary.touched_tasks))),
                    ("moved", Arg::Count(i64::from(summary.changed_tasks))),
                ],
            ));
            match (summary.finish_delta, summary.finish_delta_text.as_deref()) {
                (Some(minutes), Some(text)) if minutes != 0 => {
                    let signed = if minutes > 0 {
                        format!("+{text}")
                    } else {
                        text.to_owned()
                    };
                    parts.push(texts.get_args(TextId::ApprovalFinish, &[("value", Arg::Str(signed.into()))]));
                }
                _ => parts.push(texts.get(TextId::ApprovalFinishSame)),
            }
            if summary.critical_changed {
                parts.push(texts.get(TextId::ApprovalCritical));
            }
            if summary.warnings > 0 {
                parts.push(texts.get_args(
                    TextId::ApprovalWarnings,
                    &[("count", Arg::Count(i64::from(summary.warnings)))],
                ));
            }
        }
        None => {
            if let Some(path) = banner.path {
                parts.push(path.to_owned());
            }
        }
    }
    let head = if banner.assistant {
        texts.get_args(
            TextId::ApprovalTitleAssistant,
            &[("name", Arg::Str(banner.label.unwrap_or(banner.tool).into()))],
        )
    } else {
        texts.get_args(
            TextId::ApprovalTitle,
            &[
                ("name", Arg::Str(banner.tool.into())),
                ("document", Arg::Str(banner.document.into())),
            ],
        )
    };
    if parts.is_empty() {
        head
    } else {
        format!("{head} \u{2014} {}", parts.join(", "))
    }
}

/// Dessine le bandeau de validation (rien sans `banner`) et pousse
/// [`Action::Approve`] ou [`Action::Reject`].
pub fn approval_banner(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, banner: Option<ApprovalBanner<'_>>) {
    let Some(banner) = banner else {
        return;
    };
    let frame = egui::Frame::side_top_panel(ui.style()).fill(fc.palette.message_warning_bg);
    egui::Panel::top("plannercraft_approval_banner")
        .frame(frame)
        .show(ui, |ui| decision_row(ui, fc, banner));
}

/// Phrase, compteur de file, Accepter et Refuser de l'écriture en tête de
/// file : contenu du bandeau, repris en tête du volet « Opérations IA »
/// (J8 lot L12, H-UI-41) avec les mêmes règles de focus et de clavier.
pub fn decision_row(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, banner: ApprovalBanner<'_>) {
    let text = banner_text(fc, &banner);
    let texts = fc.texts;
    let fg = fc.palette.text_strong;
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(text).strong().color(fg));
        if banner.queued > 0 {
            let queued = i64::try_from(banner.queued).unwrap_or(i64::MAX);
            ui.label(texts.get_args(TextId::ApprovalQueued, &[("count", Arg::Count(queued))]));
        }
        // Libellés des boutons jamais coupés ; dans un volet étroit (lot
        // A5, largeur fixe), les deux boutons passent ensemble à la ligne
        // (chacun est dans son propre `push_id`, où egui ne sait plus
        // renvoyer à la ligne) au lieu de déborder hors du volet.
        let button = |text: String| egui::Button::new(text).wrap_mode(egui::TextWrapMode::Extend);
        let need = [TextId::ApprovalAccept, TextId::ApprovalReject]
            .into_iter()
            .map(|id| button_width(ui, &texts.get(id)))
            .sum::<f32>();
        if row_room(ui) < need {
            ui.end_row();
        }
        let accept = ui.push_id("plannercraft_approval_accept", |ui| {
            ui.add(button(texts.get(TextId::ApprovalAccept)))
        });
        let reject = ui.push_id("plannercraft_approval_reject", |ui| {
            ui.add(button(texts.get(TextId::ApprovalReject)))
        });
        let (accept, reject) = (accept.inner, reject.inner);
        // egui retire le focus sur Échap en début d'image : le bouton
        // qui vient de le perdre compte aussi.
        let focused = accept.has_focus() || reject.has_focus() || accept.lost_focus() || reject.lost_focus();
        let escape = focused && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
        // Ctrl/Cmd+Entrée : jamais pendant la saisie d'un champ (la
        // touche reste à la zone de saisie).
        let shortcut = !ui.ctx().text_edit_focused()
            && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter));
        let (on_accept, on_reject) = if banner.assistant {
            (
                Action::Proposal(ProposalOp::Accept(banner.id)),
                Action::Proposal(ProposalOp::Reject(banner.id)),
            )
        } else {
            (Action::Approve(banner.id), Action::Reject(banner.id))
        };
        if accept.clicked() || shortcut {
            fc.actions.push(on_accept);
        } else if reject.clicked() || escape {
            fc.actions.push(on_reject);
        }
    });
}

/// Largeur d'un bouton de libellé `text` (une ligne, marges et espacement
/// compris).
pub(crate) fn button_width(ui: &egui::Ui, text: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let text_w = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
        .size()
        .x;
    text_w + 2.0 * ui.spacing().button_padding.x + ui.spacing().item_spacing.x
}

/// Place restante sur la rangée en cours d'une mise en page à retour à la
/// ligne (`available_width` y rend la largeur entière).
pub(crate) fn row_room(ui: &egui::Ui) -> f32 {
    ui.max_rect().right() - ui.cursor().left()
}
