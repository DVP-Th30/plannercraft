//! Ce que l'assistant modifie se voit dans le tableau (retours d'essai du
//! 2026-11-11, lot R1, H-UI-188, ADR-0027 D5).
//!
//! Quand un lot proposé par l'assistant ou par un agent MCP rattaché vient
//! d'être appliqué (`App::approve`), chaque champ de tâche **directement
//! écrit** par ses opérations (pas les dates recalculées par le moteur)
//! dont la colonne n'est pas affichée dans le tableau des tâches est
//! inséré à la fin du tableau, par le chemin d'« Insérer une colonne »
//! (`ColumnOp::Insert`, préférence d'affichage, hors de la pile
//! d'annulation) ; quatre au plus par lot, dans l'ordre des opérations. Un
//! lot qui ne touche que des ressources propose « Voir les ressources ».
//!
//! Correspondance opération → colonne dérivée des tables existantes : le
//! nom JSON d'un champ d'opération est le nom sérialisé de sa colonne
//! (`Column`, `serde(rename_all = "snake_case")`, celui de `UiState`) ;
//! seules les opérations qui désignent une colonne sans porter son nom
//! (liens, affectations, contrainte) et l'alias `type` sont écrites ici.
//! Un test parcourt le registre et le schéma de chaque opération.

use plannercraft_automation::{Batch, Command};
use plannercraft_core::model::custom::CustomFieldId;
use serde_json::Value;

use crate::column::Column;

/// Colonnes insérées au plus par lot (H-UI-188).
pub const MAX_REVEALED: usize = 4;

/// Résultat d'un lot appliqué, pour la carte de résultat (H-UI-188).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Colonnes affichées par le lot, dans l'ordre d'insertion.
    pub columns: Vec<Column>,
    /// Le lot crée ou modifie des ressources sans écrire de champ de
    /// tâche : « Voir les ressources ».
    pub resources: bool,
}

impl Applied {
    /// Rien à dire sur la carte.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.columns.is_empty() && !self.resources
    }
}

/// Portée d'une opération d'écriture du registre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpScope {
    /// Champs de tâche nommés par les clés de l'opération (nom de colonne).
    Fields,
    /// Opération qui écrit ces colonnes sans les nommer.
    Columns(&'static [Column]),
    /// Opération sur les ressources (aucun champ de tâche).
    Resources,
    /// Aucun champ de tâche à montrer, et pourquoi.
    Excluded(&'static str),
}

/// Portée de l'opération `id` ; `None` pour une opération inconnue (un
/// test vérifie que chaque écriture du registre en a une).
#[must_use]
pub fn op_scope(id: &str) -> Option<OpScope> {
    Some(match id {
        "task.add" | "task.update" | "task.set_deadline" | "progress.update" => OpScope::Fields,
        "link.add" | "link.update" | "link.remove" => OpScope::Columns(&[Column::Predecessors]),
        "assignment.add" | "assignment.update" | "assignment.remove" => {
            OpScope::Columns(&[Column::ResourceNames])
        }
        "task.set_constraint" => OpScope::Columns(&[Column::ConstraintType, Column::ConstraintDate]),
        "resource.add" | "resource.update" | "resource.delete" => OpScope::Resources,
        "task.delete" => OpScope::Excluded("ligne retirée : aucune valeur à montrer"),
        "task.move" | "task.indent" | "task.outdent" => {
            OpScope::Excluded("structure : place et niveau, visibles dans l'ID et le Nom")
        }
        "task.shift" => OpScope::Excluded(
            "dates décalées par le moteur (Début et Fin) ; la contrainte posée est un effet",
        ),
        "progress.reschedule" => {
            OpScope::Excluded("travail restant replanifié : dates recalculées par le moteur")
        }
        "task.set_text_style" => OpScope::Excluded("mise en forme du texte, pas un champ"),
        "baseline.set" | "baseline.clear" => {
            OpScope::Excluded("instantané des dates : Gantt de suivi, pas une saisie")
        }
        "batch.apply" => OpScope::Excluded("conteneur : ses opérations sont lues une à une"),
        "project.set_info" | "project.set_anchor" | "project.set_status_date" => {
            OpScope::Excluded("projet, pas une tâche")
        }
        "calendar.add"
        | "calendar.update"
        | "calendar.delete"
        | "calendar.set_week"
        | "calendar.set_exception"
        | "calendar.remove_exception" => OpScope::Excluded("calendrier, pas une tâche"),
        "style.apply" | "style.set_bar_role" | "style.set_group_by" | "style.reset_to_template" => {
            OpScope::Excluded("style du document, pas une tâche")
        }
        "custom_field.rename" => OpScope::Excluded("alias d'un champ, pas une valeur de tâche"),
        _ => return None,
    })
}

/// Clés d'une opération `Fields` qui désignent la tâche ou sa place, pas
/// un champ.
pub const REFERENCE_KEYS: [&str; 5] = ["uid", "task", "at", "as", "label"];

/// Clés de champ sans colonne, et pourquoi (vérifié par test sur le
/// schéma).
pub const KEY_EXCLUSIONS: [(&str, &str); 2] = [
    (
        "physical_percent_complete",
        "% physique achevé : aucune colonne dans le catalogue",
    ),
    ("dry_run", "option d'appel, pas un champ"),
];

/// Colonne d'une clé de champ : nom sérialisé de la colonne, ou `type`
/// (`task.update`, colonne Type de tâche).
#[must_use]
pub fn column_of_key(key: &str) -> Option<Column> {
    if key == "type" {
        return Some(Column::TaskType);
    }
    serde_json::from_value::<Column>(Value::String(key.to_owned()))
        .ok()
        .filter(|c| !matches!(c, Column::Custom(_)))
}

/// Colonnes de tâche écrites par `op`, dans l'ordre de ses champs.
#[must_use]
pub fn op_columns(op: &Command) -> Vec<Column> {
    match op_scope(op.id()) {
        Some(OpScope::Columns(columns)) => columns.to_vec(),
        Some(OpScope::Fields) => {
            let Ok(Value::Object(map)) = serde_json::to_value(op) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for (key, value) in &map {
                if key == "op" || REFERENCE_KEYS.contains(&key.as_str()) || value.is_null() {
                    continue;
                }
                if key == "custom" {
                    if let Value::Object(custom) = value {
                        out.extend(
                            custom
                                .keys()
                                .filter_map(|k| k.parse::<CustomFieldId>().ok())
                                .map(Column::Custom),
                        );
                    }
                    continue;
                }
                out.extend(column_of_key(key));
            }
            out
        }
        Some(OpScope::Resources | OpScope::Excluded(_)) | None => Vec::new(),
    }
}

/// Colonnes de tâche écrites par le lot, dans l'ordre des opérations, sans
/// doublon ; et vrai s'il touche des ressources.
#[must_use]
pub fn written(batch: &Batch) -> (Vec<Column>, bool) {
    let mut columns: Vec<Column> = Vec::new();
    let mut resources = false;
    for op in &batch.ops {
        resources |= op_scope(op.id()) == Some(OpScope::Resources);
        for column in op_columns(op) {
            if column != Column::Id && !columns.contains(&column) {
                columns.push(column);
            }
        }
    }
    (columns, resources)
}

impl crate::App {
    /// Après un lot accepté (H-UI-188) : colonnes des champs écrits non
    /// affichées insérées à la fin du tableau des tâches (quatre au plus),
    /// par le chemin d'« Insérer une colonne » ; défilement horizontal
    /// jusqu'à la première. Rend le résultat pour la carte.
    pub(crate) fn reveal_written(&mut self, batch: &Batch) -> Applied {
        use crate::action::ColumnOp;

        let (columns, resources) = written(batch);
        let missing: Vec<Column> = columns
            .iter()
            .copied()
            .filter(|c| !self.state.shown(*c))
            .take(MAX_REVEALED)
            .collect();
        let mut shown = Vec::new();
        for column in missing {
            let at = self.state.columns.len();
            self.column_op(ColumnOp::Insert { at, column });
            if self.state.shown(column) {
                shown.push(column);
            }
        }
        if let Some(first) = shown.first() {
            self.reveal_column_x(*first);
        }
        Applied {
            columns: shown,
            resources: resources && columns.is_empty(),
        }
    }

    /// Défile la table pour montrer la colonne `column` (H-UI-188), comme
    /// le curseur la suit (H-UI-17) ; rien avant la première image.
    fn reveal_column_x(&mut self, column: Column) {
        let width = self.view.last_table_width;
        if !width.is_finite() || width <= 0.0 {
            return;
        }
        let widths: Vec<u16> = self.state.visible_columns().map(|c| c.width).collect();
        let Some(col) = self.state.visible_columns().position(|c| c.column == column) else {
            return;
        };
        self.view.table_scroll_x =
            crate::view::table::scroll_to_column(&widths, col, self.view.table_scroll_x, width);
    }
}

#[cfg(test)]
mod tests {
    use plannercraft_automation::Batch;
    use plannercraft_automation::registry::{Kind, REGISTRY};

    use super::{KEY_EXCLUSIONS, OpScope, REFERENCE_KEYS, column_of_key, op_scope, written};
    use crate::column::Column;

    /// H-UI-188 : chaque écriture du registre a sa portée (colonnes ou
    /// exclusion motivée) ; chaque champ du schéma d'une opération à champs
    /// a sa colonne, une clé de référence ou une exclusion motivée.
    #[test]
    fn h_ui_188_every_write_op_has_a_column_or_a_reason() {
        for spec in REGISTRY.iter().filter(|s| s.kind == Kind::Write) {
            let scope = op_scope(spec.id);
            assert!(scope.is_some(), "{} : aucune portée", spec.id);
            if let Some(OpScope::Excluded(reason)) = scope {
                assert!(!reason.is_empty(), "{}", spec.id);
            }
            if scope != Some(OpScope::Fields) {
                continue;
            }
            let schema = serde_json::to_value((spec.schema)()).unwrap_or_default();
            let props = schema
                .get("properties")
                .and_then(serde_json::Value::as_object)
                .unwrap_or_else(|| panic!("{} : schéma sans propriétés", spec.id));
            for key in props.keys() {
                let ok = REFERENCE_KEYS.contains(&key.as_str())
                    || key == "custom"
                    || column_of_key(key).is_some()
                    || KEY_EXCLUSIONS.iter().any(|(k, why)| k == key && !why.is_empty());
                assert!(ok, "{} : champ « {key} » sans colonne ni exclusion", spec.id);
            }
        }
    }

    /// H-UI-188 : colonnes d'un lot dans l'ordre des opérations, sans
    /// doublon ; ressources seules signalées.
    #[test]
    fn h_ui_188_written_columns_in_order() {
        let batch = Batch::from_json_str(
            r#"{"format_version":1,"ops":[
                {"op":"resource.add","name":"Maçon","as":"$m"},
                {"op":"assignment.add","task":2,"resource":"$m"},
                {"op":"task.update","uid":2,"notes":"Voir plan","type":"fixed_duration"},
                {"op":"task.set_constraint","uid":3,"kind":"snet","date":"2026-10-26T08:00"},
                {"op":"progress.update","task":3,"percent_complete":{"value":5000}},
                {"op":"task.update","uid":3,"notes":"bis","custom":{"text1":"Lot A"}}
            ]}"#,
        )
        .unwrap();
        let (columns, resources) = written(&batch);
        assert!(resources);
        assert_eq!(columns.first(), Some(&Column::ResourceNames));
        assert_eq!(
            &columns[1..],
            [
                Column::Notes,
                Column::TaskType,
                Column::ConstraintType,
                Column::ConstraintDate,
                Column::PercentComplete,
                Column::Custom("text1".parse().unwrap()),
            ]
        );
        let only =
            Batch::from_json_str(r#"{"format_version":1,"ops":[{"op":"resource.add","name":"Grue"}]}"#)
                .unwrap();
        assert_eq!(written(&only), (Vec::new(), true));
        let shift = Batch::from_json_str(
            r#"{"format_version":1,"ops":[{"op":"task.shift","tasks":[2],"by":{"text":"1d"}}]}"#,
        )
        .unwrap();
        assert_eq!(written(&shift), (Vec::new(), false));
    }
}
