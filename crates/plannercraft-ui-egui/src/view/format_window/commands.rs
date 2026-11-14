//! Commandes de la fenêtre « Mise en forme » (lot F1, ADR-0035) : un geste
//! = un lot du registre = une entrée d'annulation. Fonctions pures du style
//! du document, sans egui : testées une à une (H-UI-218, H-UI-221 à
//! H-UI-224, H-UI-227, H-UI-229).
//!
//! Sections remplacées en entier par la fusion (`merge_patch`) : `scale`,
//! `grid`, `density` sont renvoyées complètes, résolues depuis le style du
//! document avec le seul champ changé ; `bars` l'est rôle par rôle (forme
//! entière du rôle).

use std::collections::BTreeMap;

use plannercraft_automation::Batch;
use plannercraft_automation::command::style::{
    BarEndsJson, BarFieldJson, BarRoleJson, StyleApply, StyleResetToTemplate, StyleSetBarRole, TemplateJson,
};
use plannercraft_automation::command::{Command, Patch};
use plannercraft_automation::text::Language;
use plannercraft_render::style::{
    BarField, BarRole, Density, DocStyle, GridStyle, Pattern, PhaseColor, PlanStyle, Rgb, RowBanding,
    ScaleStyle, TemplateId, Variant,
};

/// Ce que lit une commande : style effectif du document, style choisi (s'il
/// en a un), modèle par défaut des documents sans style, langue du lot.
#[derive(Clone, Copy, Debug)]
pub struct StyleCtx<'a> {
    /// Style effectif résolu (celui du document, sinon le modèle par
    /// défaut) : jamais le style de l'écran adouci.
    pub style: &'a PlanStyle,
    /// Style choisi du document.
    pub doc: Option<&'a DocStyle>,
    /// Modèle des documents sans style (H-UI-43).
    pub default: TemplateId,
    /// Langue des commandes.
    pub lang: Language,
}

impl StyleCtx<'_> {
    /// Modèle de départ d'un premier réglage : celui du document, sinon le
    /// modèle par défaut (jamais Classique par mégarde).
    #[must_use]
    pub fn base(&self) -> TemplateId {
        self.doc.map_or(self.default, |d| d.base)
    }
}

/// Lot d'une suite de commandes (une entrée d'annulation).
fn batch(ops: Vec<Command>, lang: Language) -> Batch {
    crate::dialog::batch_of(ops, lang)
}

/// Rôle en JSON du registre.
#[must_use]
pub const fn role_json(role: BarRole) -> BarRoleJson {
    match role {
        BarRole::Task => BarRoleJson::Task,
        BarRole::Critical => BarRoleJson::Critical,
        BarRole::Summary => BarRoleJson::Summary,
        BarRole::SummaryCritical => BarRoleJson::SummaryCritical,
        BarRole::Milestone => BarRoleJson::Milestone,
        BarRole::MilestoneCritical => BarRoleJson::MilestoneCritical,
        BarRole::Progress => BarRoleJson::Progress,
        BarRole::Baseline => BarRoleJson::Baseline,
        BarRole::Inactive => BarRoleJson::Inactive,
        BarRole::Manual => BarRoleJson::Manual,
        BarRole::Slack => BarRoleJson::Slack,
        BarRole::Deadline => BarRoleJson::Deadline,
    }
}

/// Champ en JSON du registre.
#[must_use]
pub const fn field_json(f: BarField) -> BarFieldJson {
    match f {
        BarField::Name => BarFieldJson::Name,
        BarField::NameDuration => BarFieldJson::NameDuration,
        BarField::Resources => BarFieldJson::Resources,
        BarField::Start => BarFieldJson::Start,
        BarField::Finish => BarFieldJson::Finish,
        BarField::StartShort => BarFieldJson::StartShort,
        BarField::FinishShort => BarFieldJson::FinishShort,
        BarField::Duration => BarFieldJson::Duration,
        BarField::DurationShort => BarFieldJson::DurationShort,
        BarField::Percent => BarFieldJson::Percent,
        BarField::TotalSlack => BarFieldJson::TotalSlack,
        BarField::StartFinish => BarFieldJson::StartFinish,
    }
}

/// Jumeau critique d'un type de barre, et inversement (H-UI-221) : tâche ↔
/// tâche critique, récapitulative ↔ récapitulative critique, jalon ↔ jalon
/// critique ; aucun pour les autres types.
#[must_use]
pub const fn twin(role: BarRole) -> Option<BarRole> {
    match role {
        BarRole::Task => Some(BarRole::Critical),
        BarRole::Critical => Some(BarRole::Task),
        BarRole::Summary => Some(BarRole::SummaryCritical),
        BarRole::SummaryCritical => Some(BarRole::Summary),
        BarRole::Milestone => Some(BarRole::MilestoneCritical),
        BarRole::MilestoneCritical => Some(BarRole::Milestone),
        BarRole::Progress
        | BarRole::Baseline
        | BarRole::Inactive
        | BarRole::Manual
        | BarRole::Slack
        | BarRole::Deadline => None,
    }
}

/// Type non critique d'une paire de jumeaux (celui qui nomme la case).
#[must_use]
pub const fn plain_of(role: BarRole) -> BarRole {
    match role {
        BarRole::Critical => BarRole::Task,
        BarRole::SummaryCritical => BarRole::Summary,
        BarRole::MilestoneCritical => BarRole::Milestone,
        other => other,
    }
}

/// Couleur d'un rôle telle que dessinée dans la variante `v` (contour pour
/// un motif « contour seul »).
#[must_use]
pub fn shown_color(style: &PlanStyle, role: BarRole, v: Variant) -> Option<Rgb> {
    let paint = style.tokens(v).bars.get(&role)?;
    let outline_only = style
        .bars
        .get(&role)
        .is_some_and(|s| s.pattern == Pattern::Outline);
    Some(match (outline_only, paint.outline) {
        (true, Some(o)) => o,
        _ => paint.fill,
    })
}

/// État de la case « Même couleur pour les tâches critiques » (H-UI-221) :
/// déduit du style seul, jamais d'une préférence ; vrai si `role` a un
/// jumeau et que les deux sont dessinés de la même couleur dans les trois
/// variantes (écran clair, sombre, impression).
#[must_use]
pub fn twins_same(style: &PlanStyle, role: BarRole) -> bool {
    twin(role).is_some_and(|other| {
        [Variant::Light, Variant::Dark, Variant::Print]
            .into_iter()
            .all(|v| shown_color(style, role, v) == shown_color(style, other, v))
    })
}

/// Commande `style.set_bar_role` vide pour `role`.
fn set_role(role: BarRole, base: TemplateId) -> StyleSetBarRole {
    StyleSetBarRole {
        role: role_json(role),
        variant: None,
        color: None,
        thickness: None,
        ends: None,
        text_left: Patch::Keep,
        text_right: Patch::Keep,
        text_inside: Patch::Keep,
        base: Some(TemplateJson::from(base)),
    }
}

/// Couleur `rgb` sur `role` pour les trois variantes (H-UI-196) ; case des
/// jumeaux cochée (`twins`) : aussi sur le jumeau, dans le même lot
/// (H-UI-221 : les deux restent synchronisés).
#[must_use]
pub fn color(role: BarRole, rgb: Rgb, twins: bool, ctx: &StyleCtx<'_>) -> Batch {
    let mut ops = Vec::new();
    for r in std::iter::once(role).chain(twin(role).filter(|_| twins)) {
        let mut cmd = set_role(r, ctx.base());
        cmd.color = Some(rgb.hex());
        ops.push(Command::StyleSetBarRole(cmd));
    }
    batch(ops, ctx.lang)
}

/// Case des jumeaux cochée (H-UI-221) : la couleur `rgb` (celle du type
/// choisi, telle que l'écran la dessine) est posée sur les deux types.
#[must_use]
pub fn twins_on(role: BarRole, rgb: Rgb, ctx: &StyleCtx<'_>) -> Batch {
    color(role, rgb, true, ctx)
}

/// Case des jumeaux décochée (H-UI-221) : le jumeau **critique** revient à
/// la couleur du modèle (règle écrite dans ADR-0035 D3 : le style ne garde
/// aucune « dernière couleur propre », Ctrl/Cmd+Z la rend) ; sa forme et
/// ses textes propres sont gardés. Un lot : retrait des surcharges du
/// jumeau, puis forme et textes reposés s'il en avait. `None` sans style
/// choisi (rien à retirer).
#[must_use]
pub fn twins_off(role: BarRole, ctx: &StyleCtx<'_>) -> Option<Batch> {
    let critical = twin(plain_of(role))?;
    let doc = ctx.doc?;
    let mut ops = vec![Command::StyleResetToTemplate(StyleResetToTemplate {
        role: Some(role_json(critical)),
    })];
    let mut patch = serde_json::Map::new();
    if let Some(shape) = doc.over.bars.get(&critical) {
        patch.insert("bars".to_owned(), by_role(critical, shape));
    }
    if let Some(texts) = doc.over.texts.get(&critical) {
        patch.insert("texts".to_owned(), by_role(critical, texts));
    }
    if !patch.is_empty() {
        ops.push(Command::StyleApply(StyleApply {
            template: None,
            patch: Some(serde_json::Value::Object(patch)),
            reset: false,
        }));
    }
    Some(batch(ops, ctx.lang))
}

/// Objet JSON `{ "<rôle>": value }` (section `bars` ou `texts` d'une
/// surcharge, fusionnée rôle par rôle).
fn by_role(role: BarRole, value: &impl serde::Serialize) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        serde_key(role),
        serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
    );
    serde_json::Value::Object(map)
}

/// Clé JSON d'un rôle (`critical`, `summary_critical`…).
fn serde_key(role: BarRole) -> String {
    serde_json::to_value(role)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Épaisseur (H-UI-222), en % de la ligne.
#[must_use]
pub fn thickness(role: BarRole, pct: u8, ctx: &StyleCtx<'_>) -> Batch {
    let mut cmd = set_role(role, ctx.base());
    cmd.thickness = Some(pct.clamp(10, 100));
    batch(vec![Command::StyleSetBarRole(cmd)], ctx.lang)
}

/// Extrémités arrondies ou droites (H-UI-222).
#[must_use]
pub fn ends(role: BarRole, rounded: bool, ctx: &StyleCtx<'_>) -> Batch {
    let mut cmd = set_role(role, ctx.base());
    cmd.ends = Some(if rounded {
        BarEndsJson::Rounded
    } else {
        BarEndsJson::Square
    });
    batch(vec![Command::StyleSetBarRole(cmd)], ctx.lang)
}

/// Textes autour de la barre (H-UI-223) : à gauche, à droite, dedans
/// (`Keep` : inchangé, `Clear` : aucun texte) ; `None` si rien ne change.
#[must_use]
pub fn texts(
    role: BarRole,
    left: Patch<BarField>,
    right: Patch<BarField>,
    inside: Patch<BarField>,
    ctx: &StyleCtx<'_>,
) -> Option<Batch> {
    let patch = |p: Patch<BarField>| match p {
        Patch::Keep => Patch::Keep,
        Patch::Clear => Patch::Clear,
        Patch::Set(f) => Patch::Set(field_json(f)),
    };
    let mut cmd = set_role(role, ctx.base());
    cmd.text_left = patch(left);
    cmd.text_right = patch(right);
    cmd.text_inside = patch(inside);
    if cmd.text_left.is_keep() && cmd.text_right.is_keep() && cmd.text_inside.is_keep() {
        return None;
    }
    Some(batch(vec![Command::StyleSetBarRole(cmd)], ctx.lang))
}

/// `style.apply { patch }` (avec le modèle de départ si le document n'a
/// pas encore de style).
fn apply(patch: serde_json::Value, ctx: &StyleCtx<'_>) -> Batch {
    let template = ctx.doc.is_none().then(|| TemplateJson::from(ctx.default));
    batch(
        vec![Command::StyleApply(StyleApply {
            template,
            patch: Some(patch),
            reset: false,
        })],
        ctx.lang,
    )
}

/// Remplissage (motif) d'un type de barre (H-UI-222) : forme du rôle
/// résolue, motif seul changé ; `None` si rien ne change.
#[must_use]
pub fn pattern(role: BarRole, pattern: Pattern, ctx: &StyleCtx<'_>) -> Option<Batch> {
    let mut shape = *ctx.style.bars.get(&role)?;
    if shape.pattern == pattern {
        return None;
    }
    shape.pattern = pattern;
    let patch = serde_json::json!({ "bars": by_role(role, &shape) });
    Some(apply(patch, ctx))
}

/// Échelle de temps et repères (H-UI-224) : section entière, champ changé
/// par `edit` ; `None` si rien ne change.
#[must_use]
pub fn scale(ctx: &StyleCtx<'_>, edit: impl FnOnce(&mut ScaleStyle)) -> Option<Batch> {
    let mut scale = ctx.style.scale;
    edit(&mut scale);
    if scale == ctx.style.scale {
        return None;
    }
    let patch = serde_json::json!({ "scale": scale });
    Some(apply(patch, ctx))
}

/// Grille (traits verticaux, fond des lignes ; H-UI-224, H-UI-227) :
/// section entière, champ changé par `edit` ; `None` si rien ne change.
#[must_use]
pub fn grid(ctx: &StyleCtx<'_>, edit: impl FnOnce(&mut GridStyle)) -> Option<Batch> {
    let mut grid = ctx.style.grid;
    edit(&mut grid);
    if grid == ctx.style.grid {
        return None;
    }
    let patch = serde_json::json!({ "grid": grid });
    Some(apply(patch, ctx))
}

/// Fond des lignes (H-UI-227).
#[must_use]
pub fn banding(ctx: &StyleCtx<'_>, rows: RowBanding) -> Option<Batch> {
    grid(ctx, |g| g.rows = rows)
}

/// Hauteur des lignes (H-UI-227) ; `None` si rien ne change.
#[must_use]
pub fn density(ctx: &StyleCtx<'_>, density: Density) -> Option<Batch> {
    if ctx.style.density == density {
        return None;
    }
    Some(apply(serde_json::json!({ "density": density }), ctx))
}

/// Couleurs de phase remplacées en entier (H-UI-65).
#[must_use]
pub fn phases(map: &BTreeMap<u32, PhaseColor>, ctx: &StyleCtx<'_>) -> Batch {
    apply(serde_json::json!({ "phases": map }), ctx)
}

/// « Rétablir ce type » : surcharges d'un type de barre retirées
/// (`style.reset_to_template { role }`, H-UI-61).
#[must_use]
pub fn reset_role(role: BarRole, lang: Language) -> Batch {
    batch(
        vec![Command::StyleResetToTemplate(StyleResetToTemplate {
            role: Some(role_json(role)),
        })],
        lang,
    )
}

/// « Revenir au modèle » : toutes les surcharges du modèle retirées, la
/// part du document gardée (`style.reset_to_template`, H-UI-226).
#[must_use]
pub fn reset_all(lang: Language) -> Batch {
    batch(
        vec![Command::StyleResetToTemplate(StyleResetToTemplate::default())],
        lang,
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests : un échec doit paniquer")]

    use super::*;

    #[test]
    fn json_mappings_round_trip() {
        for role in BarRole::ALL {
            assert_eq!(BarRole::from(role_json(role)), role);
            assert_ne!(serde_key(role), "");
        }
        for f in BarField::ALL {
            assert_eq!(BarField::from(field_json(f)), f);
        }
    }

    /// H-UI-221 : jumeaux symétriques, trois paires seulement.
    #[test]
    fn h_ui_221_twins_pairs() {
        for role in BarRole::ALL {
            if let Some(other) = twin(role) {
                assert_eq!(twin(other), Some(role));
                assert_eq!(plain_of(other), plain_of(role));
            }
        }
        let paired = BarRole::ALL.iter().filter(|r| twin(**r).is_some()).count();
        assert_eq!(paired, 6);
    }
}
