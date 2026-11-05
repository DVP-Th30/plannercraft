//! Style du document : commandes `style.list`, `style.get`, `style.apply`,
//! `style.set_bar_role`, `style.reset_to_template`, `style.save_as_template`
//! (ADR-0016 D7, H-R12, H-UI-43, H-UI-52, H-UI-59 à H-UI-61).
//!
//! Le style est une donnée du document : entrée de `Project.extras`
//! d'origine `plannercraft` et de clé `style` (`plannercraft_mspdi::style_pi`), charge =
//! JSON canonique d'un [`DocStyle`] (modèle de base et surcharges). Les
//! écritures sont des commandes ordinaires du lot : une entrée
//! d'annulation, `dry_run`, document marqué modifié ; le fichier ne porte
//! l'instruction `<?plannercraft-style?>` que si le document a un style choisi.
//! Un document sans style prend le modèle par défaut de l'appelant
//! (préférence `default_template` de l'interface, H-UI-43 ; Classique pour
//! les surfaces sans préférence : CLI, MCP, [`HEADLESS_DEFAULT`]).
//!
//! Lecture à l'ouverture ([`check_opened`]) : une charge illisible, d'une
//! version inconnue ou hors bornes est retirée, diagnostic `style.ignored`,
//! jamais d'erreur d'ouverture.

use plannercraft_core::{Project, Subject};
use plannercraft_mspdi::style_pi::{self, StyleIgnoredReason};
use plannercraft_mspdi::{ImportCode, ImportDiagnostic};
use plannercraft_render::style::{
    BarField, BarPaint, BarRole, BarStyle, BarTexts, ColorTokensPatch, DocStyle, PlanStyle, Rgb,
    STYLE_VERSION, StyleError, StylePatch, TemplateId, Variant, VariantsPatch, contrast_ratio,
    parse_doc_style, template,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Patch;
use super::exec::Ctx;
use crate::error::{CommandError, CommandErrorKind, codes};

/// Modèle des documents sans style pour les surfaces sans préférence (CLI,
/// MCP, canal) : Classique, apparence de J7, jusqu'au lot visible (L15).
pub const HEADLESS_DEFAULT: TemplateId = TemplateId::Classique;

/// Longueur maximale du nom d'un modèle personnel, en caractères (comme
/// `UiState`).
pub const MAX_TEMPLATE_NAME_CHARS: usize = 64;

/// Contraste minimal d'une barre sur le fond (H-UI-45, WCAG 1.4.11), en
/// millièmes.
pub const MIN_BAR_CONTRAST: u32 = 3_000;

/// Contraste minimal du texte sur le fond (H-UI-45, WCAG 1.4.3), en
/// millièmes ; 7 000 pour Contraste élevé (WCAG 1.4.6).
pub const MIN_TEXT_CONTRAST: u32 = 4_500;

/// Modèle de mise en forme intégré, en JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateJson {
    /// Apparence de J7.
    Classique,
    /// Sobre.
    Sobre,
    /// Contraste élevé.
    ContrasteEleve,
    /// Présentation client.
    Presentation,
    /// Chantier.
    Chantier,
    /// Impression noir et blanc.
    ImpressionNb,
}

impl From<TemplateJson> for TemplateId {
    fn from(t: TemplateJson) -> Self {
        match t {
            TemplateJson::Classique => Self::Classique,
            TemplateJson::Sobre => Self::Sobre,
            TemplateJson::ContrasteEleve => Self::ContrasteEleve,
            TemplateJson::Presentation => Self::Presentation,
            TemplateJson::Chantier => Self::Chantier,
            TemplateJson::ImpressionNb => Self::ImpressionNb,
        }
    }
}

impl From<TemplateId> for TemplateJson {
    fn from(t: TemplateId) -> Self {
        match t {
            TemplateId::Classique => Self::Classique,
            TemplateId::Sobre => Self::Sobre,
            TemplateId::ContrasteEleve => Self::ContrasteEleve,
            TemplateId::Presentation => Self::Presentation,
            TemplateId::Chantier => Self::Chantier,
            TemplateId::ImpressionNb => Self::ImpressionNb,
        }
    }
}

/// Rôle d'une barre en JSON (type de barre de la mise en forme directe,
/// H-UI-59).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BarRoleJson {
    /// Tâche non critique.
    Task,
    /// Tâche critique.
    Critical,
    /// Récapitulative.
    Summary,
    /// Récapitulative critique.
    SummaryCritical,
    /// Jalon.
    Milestone,
    /// Jalon critique.
    MilestoneCritical,
    /// Bande d'avancement.
    Progress,
    /// Ligne de base.
    Baseline,
    /// Tâche inactive.
    Inactive,
    /// Tâche manuelle.
    Manual,
    /// Marge totale.
    Slack,
    /// Échéance.
    Deadline,
}

impl From<BarRoleJson> for BarRole {
    fn from(r: BarRoleJson) -> Self {
        match r {
            BarRoleJson::Task => Self::Task,
            BarRoleJson::Critical => Self::Critical,
            BarRoleJson::Summary => Self::Summary,
            BarRoleJson::SummaryCritical => Self::SummaryCritical,
            BarRoleJson::Milestone => Self::Milestone,
            BarRoleJson::MilestoneCritical => Self::MilestoneCritical,
            BarRoleJson::Progress => Self::Progress,
            BarRoleJson::Baseline => Self::Baseline,
            BarRoleJson::Inactive => Self::Inactive,
            BarRoleJson::Manual => Self::Manual,
            BarRoleJson::Slack => Self::Slack,
            BarRoleJson::Deadline => Self::Deadline,
        }
    }
}

/// Champ affiché autour d'une barre, en JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BarFieldJson {
    /// Nom.
    Name,
    /// Ressources affectées.
    Resources,
    /// Début.
    Start,
    /// Fin.
    Finish,
    /// Durée.
    Duration,
    /// % achevé.
    Percent,
    /// Marge totale.
    TotalSlack,
    /// Début – fin.
    StartFinish,
    /// Nom et durée courte, « Pose (4 sem.) » (H-UI-63).
    NameDuration,
    /// Durée courte (« 13 j », « 17,4 mois »).
    DurationShort,
    /// Début court (`JJ/MM/AA`).
    StartShort,
    /// Fin courte (`JJ/MM/AA`).
    FinishShort,
}

impl From<BarFieldJson> for BarField {
    fn from(f: BarFieldJson) -> Self {
        match f {
            BarFieldJson::Name => Self::Name,
            BarFieldJson::Resources => Self::Resources,
            BarFieldJson::Start => Self::Start,
            BarFieldJson::Finish => Self::Finish,
            BarFieldJson::Duration => Self::Duration,
            BarFieldJson::Percent => Self::Percent,
            BarFieldJson::TotalSlack => Self::TotalSlack,
            BarFieldJson::StartFinish => Self::StartFinish,
            BarFieldJson::NameDuration => Self::NameDuration,
            BarFieldJson::DurationShort => Self::DurationShort,
            BarFieldJson::StartShort => Self::StartShort,
            BarFieldJson::FinishShort => Self::FinishShort,
        }
    }
}

/// Variante d'un style en JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VariantJson {
    /// Écran, thème clair (défaut).
    #[default]
    Light,
    /// Écran, thème sombre.
    Dark,
    /// Impression et PDF.
    Print,
}

impl From<VariantJson> for Variant {
    fn from(v: VariantJson) -> Self {
        match v {
            VariantJson::Light => Self::Light,
            VariantJson::Dark => Self::Dark,
            VariantJson::Print => Self::Print,
        }
    }
}

/// Forme des extrémités d'une barre (H-UI-60).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BarEndsJson {
    /// Coins droits (rayon 0).
    Square,
    /// Coins arrondis (rayon [`ROUNDED_RADIUS16`]).
    Rounded,
}

/// Rayon des coins arrondis de [`BarEndsJson::Rounded`], en 1/16 px (3 px ;
/// hypothèse de design, borne du style : 128).
pub const ROUNDED_RADIUS16: u16 = 48;

/// `style.apply` : applique un modèle, une surcharge, ou revient au modèle
/// par défaut des préférences (`reset`). Une entrée d'annulation par appel.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StyleApply {
    /// Modèle de base. Seul : remplace le style du document (surcharges
    /// retirées) ; avec `patch` : modèle et surcharges neufs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<TemplateJson>,
    /// Surcharge (`StylePatch` de `plannercraft-render`, lue et bornée par
    /// `parse_doc_style` : au plus 64 Kio). Seule : fusionnée avec les
    /// surcharges du document (sections `colors`, `bars` et `texts` rôle
    /// par rôle, les autres remplacées en entier, dont `timeline` : UID des
    /// tâches de la chronologie, triés, au plus 200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<serde_json::Value>,
    /// Retire le style du document (modèle par défaut des préférences ;
    /// plus rien d'écrit dans le fichier). Exclusif des deux autres champs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reset: bool,
}

/// `style.set_bar_role` : mise en forme directe d'un type de barre
/// (H-UI-59, H-UI-60) ; une entrée d'annulation par appel (un geste).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StyleSetBarRole {
    /// Type de barre.
    pub role: BarRoleJson,
    /// Variante dont la couleur change (défaut : clair) ; la forme et les
    /// textes valent pour toutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<VariantJson>,
    /// Couleur de remplissage `#RRGGBB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = r"^#[0-9A-Fa-f]{6}$"))]
    pub color: Option<String>,
    /// Épaisseur, en % de la hauteur de ligne (10 à 100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 10, max = 100))]
    pub thickness: Option<u8>,
    /// Extrémités.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends: Option<BarEndsJson>,
    /// Texte à gauche (`null` : aucun).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub text_left: Patch<BarFieldJson>,
    /// Texte à droite (`null` : aucun).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub text_right: Patch<BarFieldJson>,
    /// Texte dans la barre (`null` : aucun).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub text_inside: Patch<BarFieldJson>,
    /// Modèle de départ si le document n'a pas encore de style (défaut :
    /// Classique) ; ignoré sinon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<TemplateJson>,
}

/// `style.reset_to_template` : « Revenir au modèle » (H-UI-61) ; retire
/// les surcharges du document, ou celles d'un seul type de barre. Le modèle
/// de base est gardé ; sans style, rien ne change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StyleResetToTemplate {
    /// Type de barre (défaut : toutes les surcharges).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<BarRoleJson>,
}

/// `style.save_as_template` : « Enregistrer comme modèle » (H-UI-61).
/// Lecture : rend le style du document nommé ; l'appelant le garde dans
/// ses modèles personnels (préférences de l'interface).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StyleSaveAsTemplate {
    /// Nom du modèle (1 à 64 caractères, espaces de bord retirés).
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
}

/// Style du document, s'il en a un lisible.
#[must_use]
pub fn doc_style(project: &Project) -> Option<DocStyle> {
    style_pi::entry(&project.extras).and_then(|e| parse_doc_style(&e.payload).ok())
}

/// Style effectif : celui du document, sinon le modèle `default`
/// (H-UI-43).
#[must_use]
pub fn effective_style(project: &Project, default: TemplateId) -> PlanStyle {
    doc_style(project).map_or_else(|| template(default), |d| d.resolve())
}

/// Contrôle du style relu à l'ouverture (H-R12) : charge valide ⇒ récrite
/// en JSON canonique ; illisible, version inconnue ou hors bornes ⇒
/// retirée, diagnostic `style.ignored` (raison `invalid`, détail de
/// l'erreur). Jamais d'erreur d'ouverture.
///
/// Chronologie (ADR-0017 D7, H-UI-79) : les UID absents du projet (tâches
/// supprimées avant l'enregistrement) sont retirés de la liste, faute de
/// quoi une tâche neuve qui réutilise leur UID (`UidAlloc` repart du plus
/// grand UID présent) apparaîtrait d'elle-même sur la frise. Pendant la
/// session, la liste les garde (l'annulation d'une suppression les
/// restitue).
pub fn check_opened(project: &mut Project) -> Option<ImportDiagnostic> {
    let payload = style_pi::entry(&project.extras)?.payload.clone();
    match parse_doc_style(&payload) {
        Ok(mut doc) => {
            if let Some(timeline) = doc.over.timeline.as_mut() {
                timeline
                    .tasks
                    .retain(|&uid| project.tasks.contains(plannercraft_core::TaskUid::new(uid)));
            }
            // Même règle pour la mise en forme du texte des tâches
            // (finitions P2, H-UI-126).
            if let Some(texts) = doc.over.task_text.as_mut() {
                texts.retain(|&uid, _| project.tasks.contains(plannercraft_core::TaskUid::new(uid)));
                if texts.is_empty() {
                    doc.over.task_text = None;
                }
            }
            style_pi::set(&mut project.extras, Some(doc.to_json()));
            None
        }
        Err(e) => {
            style_pi::set(&mut project.extras, None);
            Some(ImportDiagnostic::new(
                ImportCode::StyleIgnored {
                    reason: StyleIgnoredReason::Invalid,
                },
                None,
                None,
                "?plannercraft-style",
                Some(&e.to_string()),
            ))
        }
    }
}

/// `style.invalid` sur `field`.
fn invalid(field: &str, detail: String) -> CommandError {
    CommandError::new(codes::STYLE_INVALID, CommandErrorKind::InvalidParam(detail)).with_field(field)
}

/// `param.invalid` sur `field`.
fn param(field: &str, why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
    .with_field(field)
}

/// Explication d'une erreur de lecture, bornée.
fn short(e: &impl std::fmt::Display) -> String {
    e.to_string().chars().take(300).collect()
}

/// Écrit `doc` dans le clone du projet après validation complète (bornes de
/// `parse_doc_style`, charge écrivable dans l'instruction).
fn store(ctx: &mut Ctx<'_>, doc: &DocStyle, field: &str) -> Result<(), CommandError> {
    let json = doc.to_json();
    parse_doc_style(&json).map_err(|e: StyleError| invalid(field, short(&e)))?;
    if !style_pi::writable(&json) {
        return Err(invalid(field, "style non écrivable dans le fichier".to_owned()));
    }
    style_pi::set(&mut ctx.project.extras, Some(json));
    ctx.touched.insert(Subject::Project, ctx.op_index);
    Ok(())
}

/// [`store`] pour les commandes voisines (`task.set_text_style`).
pub(super) fn store_doc(ctx: &mut Ctx<'_>, doc: &DocStyle, field: &str) -> Result<(), CommandError> {
    store(ctx, doc, field)
}

/// Style courant du clone, sinon un style neuf sur `base`.
fn current_or(project: &Project, base: TemplateId) -> DocStyle {
    doc_style(project).unwrap_or(DocStyle {
        v: STYLE_VERSION,
        base,
        over: StylePatch::default(),
    })
}

/// `style.apply`.
pub(super) fn apply(op: &StyleApply, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    if op.reset {
        if op.template.is_some() || op.patch.is_some() {
            return Err(CommandError::new(
                codes::PARAM_INVALID,
                CommandErrorKind::InvalidParam("« reset » exclut « template » et « patch »".to_owned()),
            )
            .with_field("reset"));
        }
        style_pi::set(&mut ctx.project.extras, None);
        ctx.touched.insert(Subject::Project, ctx.op_index);
        return Ok(());
    }
    let patch = match &op.patch {
        None => None,
        Some(v) => {
            Some(serde_json::from_value::<StylePatch>(v.clone()).map_err(|e| invalid("patch", short(&e)))?)
        }
    };
    let doc = match (op.template, patch) {
        (None, None) => {
            return Err(param("template", "« template », « patch » ou « reset » attendu"));
        }
        (Some(t), patch) => {
            // Ce qui appartient au projet et non au modèle (tâches de la
            // chronologie, ADR-0017 D7 ; couleurs de phase, H-UI-65) est
            // gardé s'il n'est pas dans `patch`, comme dans la galerie.
            let mut over = patch.unwrap_or_default();
            if let Some(current) = doc_style(ctx.project) {
                if over.timeline.is_none() {
                    over.timeline = current.over.timeline;
                }
                if over.phases.is_none() {
                    over.phases = current.over.phases;
                }
                // Mise en forme du texte des tâches (finitions P2,
                // H-UI-126) : propre au projet aussi.
                if over.task_text.is_none() {
                    over.task_text = current.over.task_text;
                }
                // Regroupement (finitions P3, H-UI-129) : une vue du
                // document, gardée aussi.
                if over.group_by.is_none() {
                    over.group_by = current.over.group_by;
                }
            }
            DocStyle {
                v: STYLE_VERSION,
                base: t.into(),
                over,
            }
        }
        (None, Some(patch)) => {
            let mut doc = current_or(ctx.project, HEADLESS_DEFAULT);
            doc.over = merge_patch(&doc.over, &patch)
                .ok_or_else(|| invalid("patch", "fusion impossible".to_owned()))?;
            doc
        }
    };
    store(ctx, &doc, "patch")
}

/// `style.set_bar_role` : réglages courants d'un type de barre, partis du
/// style résolu (les autres valeurs du rôle sont gardées).
pub(super) fn set_bar_role(op: &StyleSetBarRole, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let role = BarRole::from(op.role);
    let base = op.base.map_or(HEADLESS_DEFAULT, Into::into);
    let mut doc = current_or(ctx.project, base);
    let resolved = doc.resolve();
    let mut patch = StylePatch::default();
    if let Some(color) = &op.color {
        let fill = Rgb::parse(color).ok_or_else(|| param("color", "couleur #RRGGBB attendue"))?;
        let variant = op.variant.unwrap_or_default();
        let outline = resolved
            .tokens(variant.into())
            .bars
            .get(&role)
            .and_then(|p| p.outline);
        let mut tokens = ColorTokensPatch::default();
        tokens.bars.insert(role, BarPaint { fill, outline });
        let mut colors = VariantsPatch::default();
        match variant {
            VariantJson::Light => colors.light = Some(tokens),
            VariantJson::Dark => colors.dark = Some(tokens),
            VariantJson::Print => colors.print = Some(tokens),
        }
        patch.colors = Some(colors);
    } else if op.variant.is_some() {
        return Err(param("variant", "« variant » sans « color »"));
    }
    if op.thickness.is_some() || op.ends.is_some() {
        let mut shape: BarStyle = *resolved
            .bars
            .get(&role)
            .ok_or_else(|| invalid("role", format!("rôle absent du style : {role:?}")))?;
        if let Some(t) = op.thickness {
            if !(10..=100).contains(&t) {
                return Err(param("thickness", "épaisseur de 10 à 100 % attendue"));
            }
            shape.height_pct = t;
        }
        match op.ends {
            Some(BarEndsJson::Square) => shape.radius16 = 0,
            Some(BarEndsJson::Rounded) => shape.radius16 = ROUNDED_RADIUS16,
            None => {}
        }
        patch.bars.insert(role, shape);
    }
    let texts_touched = [&op.text_left, &op.text_right, &op.text_inside]
        .iter()
        .any(|p| !p.is_keep());
    if texts_touched {
        let mut texts: BarTexts = resolved.texts.get(&role).copied().unwrap_or_default();
        let set = |slot: &mut Option<BarField>, p: &Patch<BarFieldJson>| match p {
            Patch::Keep => {}
            Patch::Set(f) => *slot = Some((*f).into()),
            Patch::Clear => *slot = None,
        };
        set(&mut texts.left, &op.text_left);
        set(&mut texts.right, &op.text_right);
        set(&mut texts.inside, &op.text_inside);
        patch.texts.insert(role, texts);
    }
    if patch.is_empty() {
        return Err(param(
            "role",
            "au moins un réglage attendu (color, thickness, ends, text_left, text_right, text_inside)",
        ));
    }
    doc.over =
        merge_patch(&doc.over, &patch).ok_or_else(|| invalid("role", "fusion impossible".to_owned()))?;
    store(ctx, &doc, "role")
}

/// `style.reset_to_template`.
pub(super) fn reset_to_template(op: &StyleResetToTemplate, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
    let Some(mut doc) = doc_style(ctx.project) else {
        return Ok(());
    };
    match op.role {
        // La mise en forme du texte des tâches (finitions P2, H-UI-126)
        // n'est pas une surcharge du modèle : gardée.
        None => {
            doc.over = StylePatch {
                task_text: doc.over.task_text.take(),
                // Regroupement (finitions P3) : pas une surcharge du modèle.
                group_by: doc.over.group_by.take(),
                ..StylePatch::default()
            };
        }
        Some(role) => {
            let role = BarRole::from(role);
            doc.over.bars.remove(&role);
            doc.over.texts.remove(&role);
            if let Some(colors) = &mut doc.over.colors {
                for slot in [&mut colors.light, &mut colors.dark, &mut colors.print] {
                    if let Some(tokens) = slot {
                        tokens.bars.remove(&role);
                        if *tokens == ColorTokensPatch::default() {
                            *slot = None;
                        }
                    }
                }
                if *colors == VariantsPatch::default() {
                    doc.over.colors = None;
                }
            }
        }
    }
    store(ctx, &doc, "role")
}

/// Fusion de surcharges : `top` l'emporte ; `colors` fusionné variante par
/// variante (champs remplacés, `bars` rôle par rôle), `bars` et `texts`
/// rôle par rôle, les autres sections remplacées en entier (elles le sont
/// aussi par `resolve`). `None` si un intermédiaire n'est pas
/// sérialisable (jamais attendu).
#[must_use]
pub fn merge_patch(base: &StylePatch, top: &StylePatch) -> Option<StylePatch> {
    let mut merged = serde_json::to_value(base).ok()?;
    let top = serde_json::to_value(top).ok()?;
    let (Value::Object(dst), Value::Object(src)) = (&mut merged, top) else {
        return None;
    };
    for (key, value) in src {
        match key.as_str() {
            "colors" => {
                let slot = dst
                    .entry(key)
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                merge_with(slot, value, &|_, variant_slot, variant| {
                    merge_with(variant_slot, variant, &|field, field_slot, field_value| {
                        if field == "bars" {
                            merge_with(field_slot, field_value, &replace);
                        } else {
                            *field_slot = field_value;
                        }
                    });
                });
            }
            "bars" | "texts" => {
                let slot = dst
                    .entry(key)
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                merge_with(slot, value, &replace);
            }
            _ => {
                dst.insert(key, value);
            }
        }
    }
    serde_json::from_value(merged).ok()
}

/// Remplace la valeur.
fn replace(_: &str, slot: &mut Value, value: Value) {
    *slot = value;
}

/// Fusion d'objets JSON, clé par clé, par `each(clé, ancienne, nouvelle)`
/// (ancienne : objet vide si absente) ; une valeur qui n'est pas un objet
/// remplace.
fn merge_with(slot: &mut Value, value: Value, each: &dyn Fn(&str, &mut Value, Value)) {
    match (slot, value) {
        (Value::Object(dst), Value::Object(src)) => {
            for (key, v) in src {
                let entry = dst
                    .entry(key.clone())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                each(&key, entry, v);
            }
        }
        (slot, value) => *slot = value,
    }
}

/// Provenance du style effectif.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleSource {
    /// Style choisi pour le document (écrit dans le fichier).
    Document,
    /// Aucun style choisi : modèle par défaut.
    Default,
}

/// Vue de `style.get`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StyleView {
    /// Provenance.
    pub source: StyleSource,
    /// Modèle de base.
    pub base: TemplateJson,
    /// Surcharges du document (objet vide sans surcharge).
    pub over: StylePatch,
    /// Style résolu (ce que dessinent l'écran, le SVG et le PDF).
    pub resolved: PlanStyle,
    /// Contrastes insuffisants du style résolu (H-UI-61).
    pub contrast_issues: Vec<ContrastIssue>,
}

/// Vue de `style.get` : style du document, sinon `default`.
#[must_use]
pub fn style_view(project: &Project, default: TemplateId) -> StyleView {
    let (source, doc) = match doc_style(project) {
        Some(doc) => (StyleSource::Document, doc),
        None => (
            StyleSource::Default,
            DocStyle {
                v: STYLE_VERSION,
                base: default,
                over: StylePatch::default(),
            },
        ),
    };
    let resolved = doc.resolve();
    StyleView {
        source,
        base: doc.base.into(),
        contrast_issues: contrast_issues(&resolved),
        over: doc.over,
        resolved,
    }
}

/// Modèle intégré listé par `style.list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TemplateEntry {
    /// Identifiant (`sobre`…).
    pub id: TemplateJson,
    /// Nom affiché.
    pub name: String,
}

/// Style du document listé par `style.list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DocStyleEntry {
    /// Modèle de base.
    pub base: TemplateJson,
    /// Le document a des surcharges.
    pub customized: bool,
}

/// Vue de `style.list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StyleListView {
    /// Modèles intégrés, dans l'ordre de la galerie.
    pub templates: Vec<TemplateEntry>,
    /// Style du document, s'il en a un.
    pub document: Option<DocStyleEntry>,
    /// Modèle des documents sans style.
    pub default: TemplateJson,
}

/// Vue de `style.list`.
#[must_use]
pub fn list_view(project: &Project, default: TemplateId) -> StyleListView {
    StyleListView {
        templates: TemplateId::ALL
            .into_iter()
            .map(|id| TemplateEntry {
                id: id.into(),
                name: template(id).name,
            })
            .collect(),
        document: doc_style(project).map(|d| DocStyleEntry {
            base: d.base.into(),
            customized: !d.over.is_empty(),
        }),
        default: default.into(),
    }
}

/// Vue de `style.save_as_template` : nom et style (forme de `NamedStyle`
/// de l'interface).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NamedStyleView {
    /// Nom.
    pub name: String,
    /// Style (modèle de base et surcharges).
    pub style: DocStyle,
}

/// `style.save_as_template` : le style du document (sinon `default` sans
/// surcharge) sous le nom donné.
///
/// # Errors
///
/// `param.invalid` (`name`) : nom vide ou de plus de
/// [`MAX_TEMPLATE_NAME_CHARS`] caractères.
pub fn save_as_template(
    project: &Project,
    p: &StyleSaveAsTemplate,
    default: TemplateId,
) -> Result<NamedStyleView, CommandError> {
    let name = p.name.trim();
    let n = name.chars().count();
    if n == 0 || n > MAX_TEMPLATE_NAME_CHARS {
        return Err(param("name", "nom de 1 à 64 caractères attendu"));
    }
    let mut style = current_or(project, default);
    // Un modèle ne porte pas la mise en forme du texte des tâches : ses UID
    // n'ont de sens que dans ce projet (finitions P2, H-UI-126).
    style.over.task_text = None;
    // Ni le regroupement (finitions P3, H-UI-129) : un champ personnalisé
    // n'a de sens que dans ce projet.
    style.over.group_by = None;
    Ok(NamedStyleView {
        name: name.to_owned(),
        style,
    })
}

/// Contraste insuffisant d'un élément du style (H-UI-61).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ContrastIssue {
    /// Variante.
    pub variant: VariantJson,
    /// Type de barre (`None` : texte courant, ou couleur de phase si
    /// `phase` est posé).
    pub role: Option<BarRoleJson>,
    /// UID de la récapitulative dont la couleur de phase (H-UI-65) est en
    /// cause, sur le fond (3:1 comme une barre ; relecture finale de J8).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<u32>,
    /// Couleur en cause.
    pub color: Rgb,
    /// Fond.
    pub background: Rgb,
    /// Rapport mesuré, en millièmes.
    pub ratio: u32,
    /// Rapport exigé, en millièmes.
    pub min: u32,
    /// Couleur conforme la plus proche (mélange le plus faible vers le noir
    /// ou le blanc), si elle existe.
    pub suggestion: Option<Rgb>,
}

/// Rôles de barre en JSON, dans l'ordre de [`BarRole::ALL`].
const ROLES: [BarRoleJson; 12] = [
    BarRoleJson::Task,
    BarRoleJson::Critical,
    BarRoleJson::Summary,
    BarRoleJson::SummaryCritical,
    BarRoleJson::Milestone,
    BarRoleJson::MilestoneCritical,
    BarRoleJson::Progress,
    BarRoleJson::Baseline,
    BarRoleJson::Inactive,
    BarRoleJson::Manual,
    BarRoleJson::Slack,
    BarRoleJson::Deadline,
];

/// Contrastes insuffisants du style (H-UI-45, H-UI-61) : texte sur le
/// fond (4,5:1, 7:1 pour Contraste élevé) et chaque barre sur le fond
/// (3:1 ; couleur du contour si la barre est en contour seul), chaque
/// couleur de phase sur le fond (3:1, `phase`), par
/// variante (impression seulement si elle a ses couleurs). La bande d'avancement, dessinée dans la barre, n'est pas
/// mesurée sur le fond.
#[must_use]
pub fn contrast_issues(style: &PlanStyle) -> Vec<ContrastIssue> {
    let text_min = if style.base == Some(TemplateId::ContrasteEleve) {
        7_000
    } else {
        MIN_TEXT_CONTRAST
    };
    let mut out = Vec::new();
    for variant in [VariantJson::Light, VariantJson::Dark, VariantJson::Print] {
        // Impression sans couleurs propres : celles du clair, déjà mesurées.
        if variant == VariantJson::Print && style.colors.print.is_none() {
            continue;
        }
        let tokens = style.tokens(variant.into());
        let bg = tokens.bg;
        let mut check = |role: Option<BarRoleJson>, color: Rgb, min: u32| {
            let ratio = contrast_ratio(color, bg);
            if ratio < min {
                out.push(ContrastIssue {
                    variant,
                    role,
                    phase: None,
                    color,
                    background: bg,
                    ratio,
                    min,
                    suggestion: nearest_compliant(color, bg, min),
                });
            }
        };
        check(None, tokens.text, text_min);
        for role_json in ROLES {
            let role = BarRole::from(role_json);
            if role == BarRole::Progress {
                continue;
            }
            let Some(paint) = tokens.bars.get(&role) else {
                continue;
            };
            let outline_only = style
                .bars
                .get(&role)
                .is_some_and(|s| s.pattern == plannercraft_render::style::Pattern::Outline);
            let color = match (outline_only, paint.outline) {
                (true, Some(outline)) => outline,
                _ => paint.fill,
            };
            check(Some(role_json), color, MIN_BAR_CONTRAST);
        }
        // Couleurs de phase : une seule couleur pour toutes les variantes,
        // mesurée sur le fond de chacune (relecture finale de J8).
        for (uid, phase) in &style.phases {
            let ratio = contrast_ratio(phase.fill, bg);
            if ratio < MIN_BAR_CONTRAST {
                out.push(ContrastIssue {
                    variant,
                    role: None,
                    phase: Some(*uid),
                    color: phase.fill,
                    background: bg,
                    ratio,
                    min: MIN_BAR_CONTRAST,
                    suggestion: nearest_compliant(phase.fill, bg, MIN_BAR_CONTRAST),
                });
            }
        }
    }
    out
}

/// Couleur conforme la plus proche (H-UI-61), définie par le rendu
/// (partagée avec le texte des tâches, finitions P2).
pub use plannercraft_render::style::nearest_compliant;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests : un échec doit paniquer")]

    use super::*;

    #[test]
    fn template_json_matches_render_keys() {
        for id in TemplateId::ALL {
            let json = TemplateJson::from(id);
            assert_eq!(TemplateId::from(json), id);
            assert_eq!(
                serde_json::to_value(json).ok(),
                Some(serde_json::Value::String(id.key().to_owned()))
            );
        }
    }

    #[test]
    fn json_enums_match_render_serde() {
        for (json, role) in ROLES.into_iter().zip(BarRole::ALL) {
            assert_eq!(BarRole::from(json), role);
            assert_eq!(serde_json::to_value(json).ok(), serde_json::to_value(role).ok());
        }
        for field in [
            BarFieldJson::Name,
            BarFieldJson::Resources,
            BarFieldJson::Start,
            BarFieldJson::Finish,
            BarFieldJson::Duration,
            BarFieldJson::Percent,
            BarFieldJson::TotalSlack,
            BarFieldJson::StartFinish,
            BarFieldJson::NameDuration,
            BarFieldJson::DurationShort,
            BarFieldJson::StartShort,
            BarFieldJson::FinishShort,
        ] {
            assert_eq!(
                serde_json::to_value(field).ok(),
                serde_json::to_value(BarField::from(field)).ok()
            );
        }
        assert_eq!(BarField::ALL.len(), 12);
    }

    #[test]
    fn merge_patch_keeps_other_roles_and_variants() {
        let base: StylePatch = serde_json::from_value(serde_json::json!({
            "density": "airy",
            "colors": { "light": { "text": "#111111", "bars": { "task": { "fill": "#000001" } } },
                        "dark": { "bg": "#000000" } },
            "texts": { "task": { "right": "name" } }
        }))
        .unwrap();
        let top: StylePatch = serde_json::from_value(serde_json::json!({
            "colors": { "light": { "bars": { "critical": { "fill": "#000002" } } } },
            "texts": { "critical": { "left": "duration" } },
            "density": "compact"
        }))
        .unwrap();
        let m = serde_json::to_value(merge_patch(&base, &top).unwrap()).unwrap();
        assert_eq!(
            m,
            serde_json::json!({
                "density": "compact",
                "colors": {
                    "light": { "text": "#111111", "bars": {
                        "task": { "fill": "#000001" }, "critical": { "fill": "#000002" } } },
                    "dark": { "bg": "#000000" }
                },
                "texts": { "task": { "right": "name" }, "critical": { "left": "duration" } }
            })
        );
    }

    #[test]
    fn nearest_compliant_darkens_light_yellow_on_white() {
        let white = Rgb(0xFF, 0xFF, 0xFF);
        let yellow = Rgb(0xFF, 0xF5, 0x9D);
        let s = nearest_compliant(yellow, white, MIN_BAR_CONTRAST).unwrap();
        assert!(contrast_ratio(s, white) >= MIN_BAR_CONTRAST);
        assert!(s.0 < yellow.0 && s.1 < yellow.1);
        // Déjà conforme : inchangée.
        let blue = Rgb(0x00, 0x72, 0xB2);
        assert_eq!(nearest_compliant(blue, white, MIN_BAR_CONTRAST), Some(blue));
    }
}
