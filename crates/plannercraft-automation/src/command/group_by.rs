//! Commande `style.set_group_by` (finitions avant v1.0.0, lot P3,
//! ADR-0022 D4, H-UI-129) : regroupement des tâches de la vue et de
//! l'impression (champ personnalisé texte, ressource, critique, jalon).
//!
//! Le regroupement vit dans le style du document (`PlanStyle::group_by`),
//! jamais dans le MSPDI : le fichier ne porte l'instruction
//! `<?plannercraft-style?>` que si le document a un style choisi. `null`
//! (ou absent) retire le regroupement. Aucun calcul de date (règle 3) :
//! les groupes et leurs bornes sont lus du planning par
//! `plannercraft_render::group`.

use plannercraft_core::{CustomFieldId, CustomKind, Project};
use plannercraft_render::style::{DocStyle, GroupBy, STYLE_VERSION, StylePatch, TemplateId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::exec::Ctx;
use super::style::{HEADLESS_DEFAULT, TemplateJson, doc_style};
use crate::error::{CommandError, CommandErrorKind, codes};

/// Regroupement en JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupByJson {
    /// Champ personnalisé texte, `"text1"` à `"text30"` (`{"custom":"text1"}`).
    Custom(String),
    /// Ressources affectées : une tâche à plusieurs ressources est sous
    /// chacune ; sans ressource, « (sans valeur) ».
    Resource,
    /// Critique, puis non critique.
    Critical,
    /// Jalons, puis autres tâches.
    Milestone,
}

impl From<GroupBy> for GroupByJson {
    fn from(g: GroupBy) -> Self {
        match g {
            GroupBy::Custom(field) => Self::Custom(field.to_string()),
            GroupBy::Resource => Self::Resource,
            GroupBy::Critical => Self::Critical,
            GroupBy::Milestone => Self::Milestone,
        }
    }
}

/// `style.set_group_by` (H-UI-129) : regroupe les tâches de la vue et de
/// l'impression ; une entrée d'annulation par appel.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StyleSetGroupBy {
    /// Regroupement ; `null` ou absent : aucun groupe.
    #[serde(default)]
    pub group_by: Option<GroupByJson>,
    /// Modèle de départ si le document n'a pas encore de style (défaut :
    /// Classique) ; ignoré sinon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<TemplateJson>,
}

impl StyleSetGroupBy {
    /// Commande qui pose `by` (`None` : retire le regroupement).
    #[must_use]
    pub fn to(by: Option<GroupBy>) -> Self {
        Self {
            group_by: by.map(GroupByJson::from),
            base: None,
        }
    }
}

/// Regroupement lu de `op`.
///
/// # Errors
///
/// `param.invalid` (champ `group_by`) : champ inconnu ou d'un autre type
/// que texte.
pub fn group_of(op: &StyleSetGroupBy) -> Result<Option<GroupBy>, CommandError> {
    Ok(match &op.group_by {
        None => None,
        Some(GroupByJson::Custom(name)) => {
            let field: CustomFieldId = name.parse().map_err(|_| invalid("champ personnalisé inconnu"))?;
            if field.kind() != CustomKind::Text {
                return Err(invalid("champ personnalisé texte attendu (text1 à text30)"));
            }
            Some(GroupBy::Custom(field))
        }
        Some(GroupByJson::Resource) => Some(GroupBy::Resource),
        Some(GroupByJson::Critical) => Some(GroupBy::Critical),
        Some(GroupByJson::Milestone) => Some(GroupBy::Milestone),
    })
}

/// `param.invalid` sur `group_by`.
fn invalid(why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field("group_by")
}

/// Regroupement du document `project` (aucun sans style).
#[must_use]
pub fn group_by_of(project: &Project) -> Option<GroupBy> {
    doc_style(project).and_then(|d| d.resolve().group_by)
}

/// Vrai si `op` changerait le regroupement du document (geste sans effet :
/// aucune entrée d'annulation). Un lot illisible est réputé changer
/// quelque chose (le refus viendra de l'exécution).
#[must_use]
pub fn would_change(project: &Project, op: &StyleSetGroupBy) -> bool {
    group_of(op).map_or(true, |by| by != group_by_of(project))
}

/// `style.set_group_by`.
pub(super) fn set_group_by(op: &StyleSetGroupBy, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let by = group_of(op)?;
    let current = doc_style(ctx.project);
    // Aucun regroupement sur un document sans style : rien à écrire (le
    // fichier reste octet pour octet celui d'avant).
    if by.is_none() && current.is_none() {
        return Ok(());
    }
    let base: TemplateId = op.base.map_or(HEADLESS_DEFAULT, Into::into);
    let mut doc = current.unwrap_or(DocStyle {
        v: STYLE_VERSION,
        base,
        over: StylePatch::default(),
    });
    doc.over.group_by = by;
    super::style::store_doc(ctx, &doc, "group_by")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests : un échec doit paniquer")]

    use super::*;

    #[test]
    fn h_ui_129_group_of_reads_text_fields_only() {
        let op = |v: serde_json::Value| -> StyleSetGroupBy { serde_json::from_value(v).unwrap() };
        assert_eq!(
            group_of(&op(serde_json::json!({ "group_by": { "custom": "text3" } }))).unwrap(),
            Some(GroupBy::Custom("text3".parse().unwrap()))
        );
        assert_eq!(
            group_of(&op(serde_json::json!({ "group_by": "resource" }))).unwrap(),
            Some(GroupBy::Resource)
        );
        assert_eq!(
            group_of(&op(serde_json::json!({ "group_by": null }))).unwrap(),
            None
        );
        assert_eq!(group_of(&op(serde_json::json!({}))).unwrap(), None);
        for bad in ["number1", "text31", "lot"] {
            let e = group_of(&op(serde_json::json!({ "group_by": { "custom": bad } }))).unwrap_err();
            assert_eq!(e.code, codes::PARAM_INVALID, "{bad}");
        }
    }
}
