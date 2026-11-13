//! Bandeau de reprise après plantage (J7, lot L15) : « Récupérer
//! « plan.xml » (3 modifications) », Restaurer / Ignorer / Plus tard.
//!
//! Un orphelin à la fois (le premier de la liste de l'application) ; si un
//! point de reprise a eu lieu, le bandeau le dit : l'historique antérieur
//! n'est plus annulable après la restauration. Il dit aussi si le fichier a
//! changé sur le disque depuis (la version récupérée s'ouvre sans chemin) et
//! si le journal vient d'une autre version (pas de Restaurer). Desktop seulement (le web
//! rouvre son planning sans le proposer) ; le bandeau lui-même ne dépend d'aucune plateforme.
//!
//! Bandeau « Planning retrouvé » (version web, lot W1, H-UI-199) : même
//! panneau en haut, au fond neutre des panneaux (une information, pas une
//! décision), « Planning retrouvé dans ce navigateur : « nom » »,
//! Nouveau planning vide / Masquer.

use crate::action::{Action, RecoverChoice};
use crate::i18n::{Arg, TextId};
use crate::view::FrameCtx;

/// Orphelin proposé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryOffer<'a> {
    /// Identifiant du journal.
    pub id: &'a str,
    /// Nom du document.
    pub name: &'a str,
    /// Modifications journalisées depuis la base.
    pub entries: usize,
    /// Un point de reprise a eu lieu.
    pub checkpointed: bool,
    /// Le fichier du document a changé depuis la base : la version
    /// récupérée s'ouvrira sans chemin (relecture finale J7, F3).
    pub file_changed: bool,
    /// Journal d'une autre version ou trop grand : pas de Restaurer, Ignorer
    /// le supprime (relecture finale J7, relais F5).
    pub kept: bool,
}

/// Dessine le bandeau de reprise (rien sans orphelin).
pub fn recovery_banner(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, offer: Option<RecoveryOffer<'_>>) {
    let Some(offer) = offer else {
        return;
    };
    let texts = fc.texts;
    let fg = fc.palette.text_strong;
    let frame = egui::Frame::side_top_panel(ui.style()).fill(fc.palette.message_warning_bg);
    egui::Panel::top("plannercraft_recovery_banner")
        .frame(frame)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let count = i64::try_from(offer.entries).unwrap_or(i64::MAX);
                let title = texts.get_args(
                    TextId::RecoveryTitle,
                    &[
                        ("name", Arg::Str(offer.name.into())),
                        ("count", Arg::Count(count)),
                    ],
                );
                ui.label(egui::RichText::new(title).strong().color(fg));
                if offer.checkpointed {
                    ui.label(egui::RichText::new(texts.get(TextId::RecoveryCheckpointed)).color(fg));
                }
                if offer.file_changed {
                    ui.label(egui::RichText::new(texts.get(TextId::RecoveryFileChanged)).color(fg));
                }
                if offer.kept {
                    ui.label(egui::RichText::new(texts.get(TextId::RecoveryKept)).color(fg));
                }
                let id = offer.id;
                let restore =
                    (!offer.kept).then(|| (TextId::RecoveryRestore, RecoverChoice::Restore(id.to_owned())));
                let choices = restore.into_iter().chain([
                    (TextId::RecoveryDiscard, RecoverChoice::Discard(id.to_owned())),
                    (TextId::RecoveryLater, RecoverChoice::Later),
                ]);
                for (label, choice) in choices {
                    if ui.button(texts.get(label)).clicked() {
                        fc.actions.push(Action::Recover(choice));
                    }
                }
            });
        });
}

/// Dessine le bandeau « Planning retrouvé dans ce navigateur » (H-UI-199) :
/// rien sans planning rouvert. « Nouveau planning vide » pousse
/// [`Action::NewProject`] (bandeau de confirmation si le planning est
/// modifié) ; « Masquer » retire le bandeau.
pub fn restored_banner(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, name: Option<&str>) {
    let Some(name) = name else {
        return;
    };
    let texts = fc.texts;
    let frame = egui::Frame::side_top_panel(ui.style());
    egui::Panel::top("plannercraft_restored_banner")
        .frame(frame)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(texts.get_args(TextId::AutosaveRestored, &[("name", Arg::Str(name.into()))]));
                if ui.button(texts.get(TextId::AutosaveRestoredNew)).clicked() {
                    fc.actions.push(Action::NewProject);
                }
                if ui.button(texts.get(TextId::AutosaveRestoredDismiss)).clicked() {
                    fc.actions.push(Action::Recover(RecoverChoice::DismissRestored));
                }
            });
        });
}
