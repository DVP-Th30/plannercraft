//! Champs personnalisés (R-264, seconde passe UX, lot L8) : valeurs par
//! `task.update { custom }`, alias par `custom_field.rename`.
//!
//! Les contrôles précèdent toute modification (atomicité du lot). Un champ
//! défini par le fichier ouvert (formule, table de choix : champ
//! « étranger » de R-265) ne se modifie pas : `custom.read_only`. Une
//! valeur posée ou effacée retire les `ExtendedAttribute` du même champ
//! gardés en `Extras` (valeur illisible du fichier) : la valeur du modèle
//! les remplace. Aucune date calculée, aucun `Extras` de planning jeté
//! (R-161 : aucune valeur calculée n'en dépend).

use std::collections::BTreeMap;

use plannercraft_core::model::custom::{CUSTOM_NUMBER_SCALE, MAX_CUSTOM_ALIAS_CHARS, MAX_CUSTOM_TEXT_CHARS};
use plannercraft_core::{
    CustomFieldDef, CustomFieldId, CustomKind, CustomValue, Moment, Subject, Task, TaskUid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::exec::Ctx;
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::text::Language;
use crate::text::lex::{Cursor, bounded_chars};

/// `custom_field.rename` (R-264) : pose l'alias d'un champ personnalisé
/// (« Lot » pour Texte1) ou le retire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomFieldRename {
    /// Champ : `text1` … `text30`, `number1` … `number20`, `date1` …
    /// `date10`, `flag1` … `flag20`.
    pub field: String,
    /// Alias, 50 caractères au plus ; `null` ou vide : le champ reprend
    /// son nom (Texte1…).
    pub alias: Option<String>,
}

/// Modifications des champs personnalisés d'une tâche, contrôlées :
/// valeur nouvelle, ou `None` pour effacer.
pub(super) type CustomEdits = Vec<(CustomFieldId, Option<CustomValue>)>;

/// Champ désigné par `key` ; `custom.unknown_field` sur `path` sinon.
fn field_of(key: &str, path: &str) -> Result<CustomFieldId, CommandError> {
    key.parse().map_err(|_| {
        CommandError::new(
            codes::CUSTOM_UNKNOWN_FIELD,
            CommandErrorKind::CustomUnknownField(key.to_owned()),
        )
        .with_field(path)
    })
}

/// `custom.read_only` si le champ est défini par le fichier (R-265).
fn check_writable(ctx: &Ctx<'_>, id: CustomFieldId, path: &str) -> Result<(), CommandError> {
    if plannercraft_mspdi::custom::foreign_fields(ctx.project).contains(&id) {
        return Err(
            CommandError::new(codes::CUSTOM_READ_ONLY, CommandErrorKind::CustomReadOnly(id)).with_field(path),
        );
    }
    Ok(())
}

/// R-264 : nombre saisi en texte, dans la langue `language` (virgule ou
/// point, R-140 ; signe admis ; espaces de bord ignorés), en dix-millièmes
/// arrondis par R-001 ; `None` s'il est illisible ou au-delà de ±10^14.
/// Exemple : « 12,5 » ⇒ 125 000.
#[must_use]
pub fn parse_custom_number(s: &str, language: Language) -> Option<i64> {
    let chars = bounded_chars(s).ok()?;
    let mut cur = Cursor::new(&chars);
    cur.skip_spaces();
    let negative = cur.sign();
    let d = cur.decimal_in(language).ok()?;
    cur.skip_spaces();
    if !cur.at_end() {
        return None;
    }
    let v = d.scale_to_integer(CUSTOM_NUMBER_SCALE).ok()?;
    let v = if negative { v.checked_neg()? } else { v };
    CustomValue::Number(v).check().ok()?;
    Some(v)
}

/// Valeur JSON `v` du champ `id` (R-264) ; `None` : effacer (`null`, texte
/// vide, `false`).
fn value_of(
    id: CustomFieldId,
    v: &Value,
    language: Language,
    path: &str,
) -> Result<Option<CustomValue>, CommandError> {
    let wrong = || {
        CommandError::new(
            codes::CUSTOM_VALUE_TYPE,
            CommandErrorKind::CustomValueType {
                field: id,
                expected: id.kind(),
            },
        )
        .with_field(path)
    };
    Ok(match (id.kind(), v) {
        (_, Value::Null) => None,
        (CustomKind::Text, Value::String(s)) => {
            if s.chars().nth(MAX_CUSTOM_TEXT_CHARS).is_some() {
                return Err(CommandError::new(
                    codes::CUSTOM_TOO_LONG,
                    CommandErrorKind::CustomTooLong {
                        max: MAX_CUSTOM_TEXT_CHARS,
                    },
                )
                .with_field(path));
            }
            (!s.is_empty()).then(|| CustomValue::Text(s.clone()))
        }
        (CustomKind::Number, Value::String(s)) => Some(CustomValue::Number(
            parse_custom_number(s, language).ok_or_else(wrong)?,
        )),
        // Nombre JSON : relu par son écriture décimale (jamais en flottant,
        // règle 5) ; un exposant est refusé.
        (CustomKind::Number, Value::Number(n)) => Some(CustomValue::Number(
            parse_custom_number(&n.to_string(), Language::En).ok_or_else(wrong)?,
        )),
        (CustomKind::Date, Value::String(s)) => Some(CustomValue::Date(
            s.trim().parse::<Moment>().map_err(|_| wrong())?,
        )),
        (CustomKind::Flag, Value::Bool(b)) => b.then_some(CustomValue::Flag(true)),
        _ => return Err(wrong()),
    })
}

/// R-264 : contrôle les champs personnalisés d'un `task.update` sur la
/// tâche `uid`, sans rien modifier.
///
/// # Errors
///
/// `custom.unknown_field`, `custom.read_only`, `custom.value_type`,
/// `custom.too_long` (champ `custom.<clé>`, sujet : la tâche).
pub(super) fn plan_values(
    custom: &BTreeMap<String, Value>,
    ctx: &Ctx<'_>,
    uid: TaskUid,
) -> Result<CustomEdits, CommandError> {
    let mut out = Vec::with_capacity(custom.len());
    if custom.is_empty() {
        return Ok(out);
    }
    let foreign = plannercraft_mspdi::custom::foreign_fields(ctx.project);
    for (key, v) in custom {
        let path = format!("custom.{key}");
        let at_task = |e: CommandError| e.with_subject(Subject::Task(uid));
        let id = field_of(key, &path).map_err(at_task)?;
        if foreign.contains(&id) {
            return Err(at_task(
                CommandError::new(codes::CUSTOM_READ_ONLY, CommandErrorKind::CustomReadOnly(id))
                    .with_field(&path),
            ));
        }
        out.push((id, value_of(id, v, ctx.text.language, &path).map_err(at_task)?));
    }
    Ok(out)
}

/// Applique les modifications contrôlées par [`plan_values`] ; vrai si
/// une valeur a changé.
pub(super) fn apply_values(t: &mut Task, edits: &CustomEdits) -> bool {
    let mut changed = false;
    for (id, value) in edits {
        plannercraft_mspdi::custom::remove_task_values(&mut t.extras, *id);
        let before = t.custom.get(id);
        if before == value.as_ref() {
            continue;
        }
        changed = true;
        match value {
            Some(v) => {
                t.custom.insert(*id, v.clone());
            }
            None => {
                t.custom.remove(id);
            }
        }
    }
    changed
}

/// R-264 : `custom_field.rename`.
///
/// # Errors
///
/// `custom.unknown_field` (champ `field`), `custom.read_only`,
/// `custom.too_long` (champ `alias`, plus de 50 caractères).
pub(super) fn rename(op: &CustomFieldRename, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let id = field_of(&op.field, "field")?;
    check_writable(ctx, id, "field")?;
    let alias = op.alias.as_deref().filter(|a| !a.is_empty());
    if alias.is_some_and(|a| a.chars().nth(MAX_CUSTOM_ALIAS_CHARS).is_some()) {
        return Err(CommandError::new(
            codes::CUSTOM_TOO_LONG,
            CommandErrorKind::CustomTooLong {
                max: MAX_CUSTOM_ALIAS_CHARS,
            },
        )
        .with_field("alias"));
    }
    match alias {
        Some(a) => {
            ctx.project.custom_fields.insert(
                id,
                CustomFieldDef {
                    alias: Some(a.to_owned()),
                },
            );
        }
        None => {
            ctx.project.custom_fields.remove(&id);
        }
    }
    ctx.touched.insert(Subject::Project, ctx.op_index);
    Ok(())
}
