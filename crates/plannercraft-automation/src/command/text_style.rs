//! Commande `task.set_text_style` (finitions avant v1.0.0, lot P2,
//! ADR-0022 D3, H-UI-126) : mise en forme du texte des lignes de la table
//! (gras, italique, couleur du texte, surlignage).
//!
//! La mise en forme vit dans le style du document (`PlanStyle::task_text`,
//! UID → `TaskText`), jamais dans le MSPDI (le format XML ne porte pas la
//! mise en forme du texte par tâche) : le fichier ne porte l'instruction
//! `<?plannercraft-style?>` que si le document a un style choisi. Chaque
//! champ est un [`Patch`] : absent, inchangé ; valeur, posée ; `null`,
//! retirée. Une récapitulative est mise en forme elle-même (pas ses
//! sous-tâches). Une entrée qui ne porte plus rien est retirée. Aucun
//! calcul de date (règle 3) : le planning n'est pas touché.

use std::collections::BTreeMap;

use plannercraft_core::{Project, TaskUid};
use plannercraft_render::style::{
    DocStyle, MAX_TASK_TEXTS, Rgb, STYLE_VERSION, StylePatch, TaskText, TemplateId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::exec::Ctx;
use super::style::{HEADLESS_DEFAULT, TemplateJson, doc_style, merge_patch};
use super::{Patch, TaskRef};
use crate::error::{CommandError, CommandErrorKind, codes};

/// `task.set_text_style` (H-UI-126) : mise en forme du texte de `tasks`
/// dans la table. Au moins un des quatre réglages ; une entrée
/// d'annulation par lot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskSetTextStyle {
    /// Tâches (UID ou étiquette `$x`) ; une récapitulative est mise en
    /// forme elle-même, pas ses sous-tâches.
    pub tasks: Vec<TaskRef>,
    /// Gras : `true` pose, `false` ou `null` retire, absent : inchangé.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub bold: Patch<bool>,
    /// Italique : `true` pose, `false` ou `null` retire, absent : inchangé.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub italic: Patch<bool>,
    /// Couleur du texte `#RRGGBB` ; `null` : couleur de la ligne.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub color: Patch<String>,
    /// Couleur de surlignage `#RRGGBB` (fond des cellules de la ligne dans
    /// la table ; le Gantt n'est pas touché) ; `null` : aucun.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub fill: Patch<String>,
    /// Modèle de départ si le document n'a pas encore de style (défaut :
    /// Classique) ; ignoré sinon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<TemplateJson>,
}

impl TaskSetTextStyle {
    /// Lot qui retire toute mise en forme de `tasks` (« Effacer la mise en
    /// forme »).
    #[must_use]
    pub fn clear(tasks: Vec<TaskRef>) -> Self {
        Self {
            tasks,
            bold: Patch::Clear,
            italic: Patch::Clear,
            color: Patch::Clear,
            fill: Patch::Clear,
            base: None,
        }
    }

    /// Lot qui donne à `tasks` exactement la mise en forme `t` (pinceau
    /// « Reproduire la mise en forme »).
    #[must_use]
    pub fn exactly(tasks: Vec<TaskRef>, t: &TaskText) -> Self {
        let set_or_clear = |v: Option<Rgb>| v.map_or(Patch::Clear, |c| Patch::Set(c.hex()));
        Self {
            tasks,
            bold: Patch::Set(t.bold),
            italic: Patch::Set(t.italic),
            color: set_or_clear(t.color),
            fill: set_or_clear(t.fill),
            base: None,
        }
    }
}

/// `param.invalid` sur `field`.
fn invalid(field: &str, why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field(field)
}

/// Réglages lus : couleurs en `Rgb`.
struct Edit {
    bold: Patch<bool>,
    italic: Patch<bool>,
    color: Patch<Rgb>,
    fill: Patch<Rgb>,
}

/// Lit les réglages de `op`.
fn edit_of(op: &TaskSetTextStyle) -> Result<Edit, CommandError> {
    let color = |p: &Patch<String>, field: &str| -> Result<Patch<Rgb>, CommandError> {
        match p {
            Patch::Keep => Ok(Patch::Keep),
            Patch::Clear => Ok(Patch::Clear),
            Patch::Set(s) => Rgb::parse(s)
                .map(Patch::Set)
                .ok_or_else(|| invalid(field, "couleur #RRGGBB attendue")),
        }
    };
    let edit = Edit {
        bold: op.bold.clone(),
        italic: op.italic.clone(),
        color: color(&op.color, "color")?,
        fill: color(&op.fill, "fill")?,
    };
    if edit.bold.is_keep() && edit.italic.is_keep() && edit.color.is_keep() && edit.fill.is_keep() {
        return Err(invalid(
            "tasks",
            "au moins un réglage attendu (bold, italic, color, fill)",
        ));
    }
    Ok(edit)
}

/// Mise en forme de `current` après `edit`.
fn edited(current: TaskText, edit: &Edit) -> TaskText {
    let flag = |slot: &mut bool, p: &Patch<bool>| match p {
        Patch::Keep => {}
        Patch::Set(v) => *slot = *v,
        Patch::Clear => *slot = false,
    };
    let rgb = |slot: &mut Option<Rgb>, p: &Patch<Rgb>| match p {
        Patch::Keep => {}
        Patch::Set(v) => *slot = Some(*v),
        Patch::Clear => *slot = None,
    };
    let mut t = current;
    flag(&mut t.bold, &edit.bold);
    flag(&mut t.italic, &edit.italic);
    rgb(&mut t.color, &edit.color);
    rgb(&mut t.fill, &edit.fill);
    t
}

/// Mises en forme du document `project` (vide sans style).
fn current_map(project: &Project) -> BTreeMap<u32, TaskText> {
    doc_style(project)
        .map(|d| d.resolve().task_text)
        .unwrap_or_default()
}

/// Table après `edit` sur `uids` (entrées vides retirées).
fn apply_edit(mut map: BTreeMap<u32, TaskText>, uids: &[TaskUid], edit: &Edit) -> BTreeMap<u32, TaskText> {
    for uid in uids {
        let key = uid.get();
        let t = edited(map.get(&key).copied().unwrap_or_default(), edit);
        if t.is_empty() {
            map.remove(&key);
        } else {
            map.insert(key, t);
        }
    }
    map
}

/// Vrai si `op` changerait la mise en forme du document (geste sans effet :
/// aucune entrée d'annulation, H-UI-126). Un lot illisible ou qui désigne
/// une étiquette `$x` est réputé changer quelque chose (le refus viendra
/// de l'exécution).
#[must_use]
pub fn would_change(project: &Project, op: &TaskSetTextStyle) -> bool {
    let Ok(edit) = edit_of(op) else {
        return true;
    };
    let mut uids = Vec::with_capacity(op.tasks.len());
    for r in &op.tasks {
        match r {
            TaskRef::Uid(uid) if project.tasks.contains(*uid) => uids.push(*uid),
            _ => return true,
        }
    }
    let before = current_map(project);
    apply_edit(before.clone(), &uids, &edit) != before
}

/// Mise en forme de la tâche `uid` dans le style du document (vide sans
/// mise en forme).
#[must_use]
pub fn task_text_of(project: &Project, uid: TaskUid) -> TaskText {
    current_map(project).get(&uid.get()).copied().unwrap_or_default()
}

/// `task.set_text_style`.
pub(super) fn set_text_style(op: &TaskSetTextStyle, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    if op.tasks.is_empty() {
        return Err(invalid("tasks", "au moins une tâche attendue"));
    }
    let edit = edit_of(op)?;
    let mut uids = Vec::with_capacity(op.tasks.len());
    for r in &op.tasks {
        uids.push(ctx.task(r, "tasks")?);
    }
    let base: TemplateId = op.base.map_or(HEADLESS_DEFAULT, Into::into);
    let mut doc = doc_style(ctx.project).unwrap_or(DocStyle {
        v: STYLE_VERSION,
        base,
        over: StylePatch::default(),
    });
    let map = apply_edit(doc.resolve().task_text, &uids, &edit);
    if map.len() > MAX_TASK_TEXTS {
        return Err(CommandError::new(
            codes::LIMIT_EXCEEDED,
            CommandErrorKind::Limit {
                what: "task_text",
                max: u64::try_from(MAX_TASK_TEXTS).unwrap_or(u64::MAX),
            },
        )
        .with_field("tasks"));
    }
    let patch = StylePatch {
        task_text: Some(map),
        ..StylePatch::default()
    };
    doc.over = merge_patch(&doc.over, &patch).ok_or_else(|| invalid("tasks", "fusion impossible"))?;
    // Table vide : la section est retirée (style du document plus court).
    if doc.over.task_text.as_ref().is_some_and(BTreeMap::is_empty) {
        doc.over.task_text = None;
    }
    super::style::store_doc(ctx, &doc, "tasks")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests : un échec doit paniquer")]

    use super::*;

    #[test]
    fn h_ui_126_edit_patch_semantics() {
        let op = TaskSetTextStyle {
            tasks: vec![TaskRef::Uid(TaskUid::new(1))],
            bold: Patch::Set(true),
            color: Patch::Set("#C00000".to_owned()),
            ..TaskSetTextStyle::default()
        };
        let edit = edit_of(&op).unwrap();
        let t = edited(TaskText::default(), &edit);
        assert!(t.bold && !t.italic);
        assert_eq!(t.color, Some(Rgb(0xC0, 0, 0)));
        // `null` retire, absent garde.
        let clear = edit_of(&TaskSetTextStyle {
            bold: Patch::Clear,
            ..op.clone()
        })
        .unwrap();
        let t2 = edited(t, &clear);
        assert!(!t2.bold);
        assert_eq!(t2.color, Some(Rgb(0xC0, 0, 0)));
        // Tout absent : refus.
        assert!(
            edit_of(&TaskSetTextStyle {
                tasks: op.tasks,
                ..TaskSetTextStyle::default()
            })
            .is_err()
        );
    }

    #[test]
    fn h_ui_126_empty_entries_removed() {
        let edit = edit_of(&TaskSetTextStyle::clear(vec![TaskRef::Uid(TaskUid::new(3))])).unwrap();
        let mut map = BTreeMap::new();
        map.insert(
            3,
            TaskText {
                italic: true,
                ..TaskText::default()
            },
        );
        assert_eq!(apply_edit(map, &[TaskUid::new(3)], &edit), BTreeMap::new());
    }
}
