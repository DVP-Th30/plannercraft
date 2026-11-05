//! Regrouper par (finitions avant v1.0.0, lot P3, ADR-0022 D4, H-UI-129 à
//! H-UI-131) : Affichage → Données → Regrouper par ▾ et son jumeau du menu
//! Affichage.
//!
//! Choisir un regroupement est un geste d'écriture : un lot
//! `style.set_group_by` (une entrée d'annulation), gardé dans le style du
//! document et suivi par l'aperçu, le PDF et le SVG ; un choix sans effet
//! (le regroupement courant, « Aucun groupe » sans regroupement) n'écrit
//! rien. Replier un groupe est une vue (rien d'écrit). Pendant le
//! regroupement, les gestes de structure (insérer, indenter, déplacer,
//! couper et coller des lignes) sont refusés avec la raison « Retirez le
//! regroupement… » ([`group_locks`]) ; la saisie des cellules reste
//! permise.

use plannercraft_automation::Batch;
use plannercraft_automation::command::Command;
use plannercraft_automation::command::group_by::{StyleSetGroupBy, would_change};
use plannercraft_automation::command::style::{TemplateJson, doc_style};
use plannercraft_core::{CustomFieldId, CustomKind, CustomValue, Project};
use plannercraft_render::group::GroupKey;
use plannercraft_render::style::GroupBy;

use crate::app::{App, EditOrigin};
use crate::column::Column;
use crate::i18n::{TextId, Texts};
use crate::structure::StructureOp;

/// Geste du regroupement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupOp {
    /// Regroupe par `by` (`None` : « Aucun groupe »).
    Set(Option<GroupBy>),
    /// Replie ou déplie le groupe de clé donnée (vue, rien d'écrit).
    Toggle(GroupKey),
    /// Ligne affichée appuyée dans la table (tâche répétée sous plusieurs
    /// groupes : le curseur part de cette ligne).
    CursorRow(usize),
}

/// Opération de structure refusée pendant le regroupement (H-UI-130) :
/// insérer une tâche, un jalon ou une récapitulative, indenter,
/// désindenter, monter, descendre.
#[must_use]
pub const fn group_locks(op: StructureOp) -> bool {
    matches!(
        op,
        StructureOp::InsertBefore
            | StructureOp::InsertMilestone
            | StructureOp::InsertSummary
            | StructureOp::Indent
            | StructureOp::Outdent
            | StructureOp::MoveUp
            | StructureOp::MoveDown
    )
}

/// Choix de « Regrouper par ▾ » (H-UI-129), dans l'ordre du menu, avec leur
/// libellé : Aucun groupe ; champs personnalisés texte renommés ou portant
/// au moins une valeur (sous leur alias, sinon « Texte1 ») ; Ressources ;
/// Critique / non critique ; Jalons.
#[must_use]
pub fn choices(project: Option<&Project>, texts: &Texts) -> Vec<(Option<GroupBy>, String)> {
    let mut out = vec![(None, texts.get(TextId::GroupByNone))];
    if let Some(p) = project {
        for id in CustomFieldId::all().filter(|id| id.kind() == CustomKind::Text) {
            let alias = p
                .custom_fields
                .get(&id)
                .and_then(|d| d.alias.as_deref())
                .filter(|a| !a.is_empty());
            let used = p
                .tasks
                .iter()
                .any(|t| matches!(t.custom.get(&id), Some(CustomValue::Text(v)) if !v.is_empty()));
            if alias.is_some() || used {
                let name = alias.map_or_else(|| Column::Custom(id).label(texts), str::to_owned);
                out.push((Some(GroupBy::Custom(id)), name));
            }
        }
    }
    out.push((Some(GroupBy::Resource), texts.get(TextId::GroupByResource)));
    out.push((Some(GroupBy::Critical), texts.get(TextId::GroupByCritical)));
    out.push((Some(GroupBy::Milestone), texts.get(TextId::GroupByMilestone)));
    out
}

impl App {
    /// Regroupement de la vue (dernière construction de la vue des
    /// lignes).
    #[must_use]
    pub const fn group_by(&self) -> Option<GroupBy> {
        self.view.group_by
    }

    /// Applique un geste du regroupement.
    pub(crate) fn group_op(&mut self, op: GroupOp) {
        match op {
            GroupOp::Set(by) => self.set_group_by(by),
            GroupOp::CursorRow(i) => self.view.cursor_row = Some(crate::row_view::DisplayRow::at(i)),
            GroupOp::Toggle(key) => {
                if !self.view.collapsed_groups.remove(&key) {
                    self.view.collapsed_groups.insert(key);
                }
                self.view.collapse_version = self.view.collapse_version.wrapping_add(1);
                self.ensure_row_view();
                self.sync_selection();
            }
        }
    }

    /// Pose le regroupement `by` : un lot `style.set_group_by`, rien si le
    /// document l'a déjà (H-UI-129).
    fn set_group_by(&mut self, by: Option<GroupBy>) {
        let Some(document) = &self.document else {
            return;
        };
        let project = document.project();
        let mut cmd = StyleSetGroupBy::to(by);
        if !would_change(project, &cmd) {
            return;
        }
        if doc_style(project).is_none() {
            cmd.base = Some(TemplateJson::from(self.state.default_template));
        }
        let batch = Batch {
            format_version: Batch::FORMAT_VERSION,
            lang: Some(self.text_language()),
            ops: vec![Command::StyleSetGroupBy(cmd)],
            label: Some(self.texts.get(TextId::GroupByUndo)),
        };
        drop(self.execute(&batch, EditOrigin::User));
        self.ensure_row_view();
        self.sync_selection();
    }
}
