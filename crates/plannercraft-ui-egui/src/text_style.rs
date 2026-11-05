//! Mise en forme du texte des lignes (finitions avant v1.0.0, lot P2,
//! ADR-0022 D3, H-UI-126 à H-UI-128) : Tâche → Texte (Gras, Italique,
//! Couleur du texte ▾, Surlignage ▾, Reproduire la mise en forme, Effacer la
//! mise en forme) et leurs jumeaux du menu Format.
//!
//! Un geste = un lot `task.set_text_style` sur la sélection, une entrée
//! d'annulation ; un geste sans effet (gras déjà posé, rien à effacer)
//! n'écrit rien et ne dit rien (`would_change`, sans simulation : la
//! mise en forme ne touche pas le planning). Gras et italique inversent
//! l'état de la tâche active, appliqué à toute la sélection (comme les
//! bascules de tâche, H-UI-74). Le pinceau copie la mise en forme de la
//! tâche active ; la sélection suivante (relâcher de la souris, ou
//! déplacement au clavier) la reçoit exactement, puis il se désarme ; Échap
//! ou un second clic sur le bouton l'annulent.

use std::collections::BTreeSet;

use plannercraft_automation::command::style::{TemplateJson, doc_style};
use plannercraft_automation::command::text_style::{TaskSetTextStyle, task_text_of, would_change};
use plannercraft_automation::command::{Command, Patch, TaskRef};
use plannercraft_automation::{Batch, text::Language};
use plannercraft_core::TaskUid;
use plannercraft_render::style::{Rgb, TaskText};

use crate::action::TextStyleOp;
use crate::app::{App, EditOrigin};
use crate::i18n::TextId;
use crate::state::ViewKind;

/// Pinceau armé (H-UI-128) : mise en forme copiée et sélection d'où elle
/// vient (la sélection suivante, différente, la reçoit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PainterState {
    /// Mise en forme copiée.
    pub style: TaskText,
    /// Sélection au moment de la copie.
    pub from: BTreeSet<TaskUid>,
}

/// Couleur `#RRGGBB` posée, ou retirée.
fn color_patch(c: Option<Rgb>) -> Patch<String> {
    c.map_or(Patch::Clear, |c| Patch::Set(c.hex()))
}

/// Lot `task.set_text_style` d'un geste sur `uids` dont la tâche active a
/// la mise en forme `active` ; `None` pour le pinceau (pas de lot direct).
#[must_use]
pub fn command_for(op: TextStyleOp, uids: &[TaskUid], active: &TaskText) -> Option<TaskSetTextStyle> {
    let tasks: Vec<TaskRef> = uids.iter().map(|u| TaskRef::Uid(*u)).collect();
    let base = TaskSetTextStyle {
        tasks,
        ..TaskSetTextStyle::default()
    };
    Some(match op {
        TextStyleOp::ToggleBold => TaskSetTextStyle {
            bold: Patch::Set(!active.bold),
            ..base
        },
        TextStyleOp::ToggleItalic => TaskSetTextStyle {
            italic: Patch::Set(!active.italic),
            ..base
        },
        TextStyleOp::Color(c) => TaskSetTextStyle {
            color: color_patch(c),
            ..base
        },
        TextStyleOp::Fill(c) => TaskSetTextStyle {
            fill: color_patch(c),
            ..base
        },
        TextStyleOp::Clear => TaskSetTextStyle::clear(base.tasks),
        TextStyleOp::Painter => return None,
    })
}

impl App {
    /// Tâches sélectionnées (ordre des UID) et tâche active (elle-même si
    /// elle est sélectionnée, sinon la première).
    fn text_targets(&self) -> (Vec<TaskUid>, Option<TaskUid>) {
        let uids: Vec<TaskUid> = self.selected.uids().iter().copied().collect();
        let active = self
            .selection
            .filter(|u| uids.contains(u))
            .or_else(|| uids.first().copied());
        (uids, active)
    }

    /// Geste de mise en forme du texte (H-UI-126 à H-UI-128) ; vue Gantt
    /// seulement (les vues des ressources n'affichent pas les tâches).
    pub(crate) fn text_style_op(&mut self, op: TextStyleOp) {
        if self.state.view != ViewKind::Gantt {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let (uids, active) = self.text_targets();
        let current = active.map_or_else(TaskText::default, |u| task_text_of(document.project(), u));
        if op == TextStyleOp::Painter {
            self.view.painter = match (&self.view.painter, active) {
                (Some(_), _) | (None, None) => None,
                (None, Some(_)) => Some(PainterState {
                    style: current,
                    from: uids.into_iter().collect(),
                }),
            };
            return;
        }
        if uids.is_empty() {
            return;
        }
        if let Some(cmd) = command_for(op, &uids, &current) {
            self.run_text_style(cmd);
        }
    }

    /// Exécute `cmd` s'il change quelque chose (un geste, une entrée
    /// d'annulation « Mise en forme du texte ») ; document sans style :
    /// modèle de départ = modèle par défaut des préférences (H-UI-43).
    fn run_text_style(&mut self, mut cmd: TaskSetTextStyle) {
        let Some(document) = &self.document else {
            return;
        };
        let project = document.project();
        if !would_change(project, &cmd) {
            return;
        }
        if doc_style(project).is_none() {
            cmd.base = Some(TemplateJson::from(self.state.default_template));
        }
        let lang: Language = self.text_language();
        let batch = Batch {
            format_version: Batch::FORMAT_VERSION,
            lang: Some(lang),
            ops: vec![Command::TaskSetTextStyle(cmd)],
            label: Some(self.texts.get(TextId::TextStyleUndo)),
        };
        drop(self.execute(&batch, EditOrigin::User));
    }

    /// Échap désarme le pinceau (touche consommée) ; rend vrai si c'est le
    /// cas.
    pub(crate) fn painter_escape(&mut self, ctx: &egui::Context) -> bool {
        if self.view.painter.is_none() {
            return false;
        }
        let pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if pressed {
            self.view.painter = None;
        }
        pressed
    }

    /// Pinceau armé (H-UI-128) : dès que la sélection a changé et que la
    /// souris est relâchée, la nouvelle sélection reçoit la mise en forme
    /// copiée (un lot, ou rien si elle l'a déjà) et le pinceau se désarme.
    /// Document fermé ou vue changée : désarmé.
    pub(crate) fn painter_tick(&mut self, ctx: &egui::Context) {
        let Some(painter) = &self.view.painter else {
            return;
        };
        if self.document.is_none() || self.state.view != ViewKind::Gantt {
            self.view.painter = None;
            return;
        }
        let now: BTreeSet<TaskUid> = self.selected.uids().clone();
        if now.is_empty() || now == painter.from || ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let style = painter.style;
        self.view.painter = None;
        let uids: Vec<TaskUid> = now.into_iter().collect();
        let tasks = uids.into_iter().map(TaskRef::Uid).collect();
        self.run_text_style(TaskSetTextStyle::exactly(tasks, &style));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests : un échec doit paniquer")]

    use super::*;

    #[test]
    fn h_ui_126_toggle_follows_active_task() {
        let uids = [TaskUid::new(2), TaskUid::new(3)];
        let bold = TaskText {
            bold: true,
            ..TaskText::default()
        };
        let cmd = command_for(TextStyleOp::ToggleBold, &uids, &bold).unwrap();
        assert_eq!(cmd.bold, Patch::Set(false));
        assert!(cmd.italic.is_keep() && cmd.color.is_keep() && cmd.fill.is_keep());
        let cmd = command_for(TextStyleOp::ToggleItalic, &uids, &bold).unwrap();
        assert_eq!(cmd.italic, Patch::Set(true));
        assert_eq!(cmd.tasks.len(), 2);
        let cmd = command_for(TextStyleOp::Color(None), &uids, &bold).unwrap();
        assert_eq!(cmd.color, Patch::Clear);
        let cmd = command_for(TextStyleOp::Fill(Some(Rgb(0xFF, 0xC0, 0))), &uids, &bold).unwrap();
        assert_eq!(cmd.fill, Patch::Set("#FFC000".to_owned()));
        assert!(command_for(TextStyleOp::Painter, &uids, &bold).is_none());
    }
}
