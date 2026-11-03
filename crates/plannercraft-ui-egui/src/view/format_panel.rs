//! Volet de mise en forme directe des barres (J8, lot L15, H-UI-59 à
//! H-UI-61, H-UI-63, H-UI-65).
//!
//! Ouvert par un clic droit sur une barre (« Mettre en forme les barres de
//! ce type ») ou par Format → Mettre en forme les barres… : le type est
//! déduit de la barre (tâche, critique, récapitulative, jalon…), aucun
//! tableau de conditions. Réglages courants visibles d'emblée : couleur
//! (nuancier du modèle, palette Okabe-Ito, couleur libre), épaisseur,
//! extrémités, textes à gauche, à droite et dans la barre, case « durée
//! après le nom » (H-UI-63) ; détails repliés (texte dans la barre). Chaque
//! geste est **une** commande du registre (`style.set_bar_role`,
//! `style.reset_to_template`, `style.apply`), donc une entrée d'annulation,
//! et s'affiche à l'image suivante (le style du document change de
//! génération). Une couleur sous 3:1 sur le fond est signalée avec la
//! couleur conforme la plus proche (H-UI-61). « Revenir au modèle » et
//! « Enregistrer comme modèle… » toujours accessibles. Sur une
//! récapitulative : couleur de phase (barre, sous-barres en option, fond
//! de la ligne dans la table, H-UI-65).
//!
//! Le volet est une fenêtre non modale : la table et le Gantt restent
//! utilisables.

use std::collections::BTreeMap;

use plannercraft_automation::command::style::{
    BarEndsJson, BarFieldJson, BarRoleJson, StyleApply, StyleResetToTemplate, StyleSetBarRole, TemplateJson,
    VariantJson, nearest_compliant,
};
use plannercraft_automation::command::{Command, Patch};
use plannercraft_automation::text::Language;
use plannercraft_automation::{Batch, Session};
use plannercraft_core::TaskUid;
use plannercraft_render::style::{
    BarField, BarRole, BarTexts, DocStyle, Pattern, PhaseColor, PlanStyle, Rgb, TemplateId, Variant,
    contrast_ratio,
};

use crate::i18n::{Arg, TextId, Texts};

/// Rapport minimal d'une barre sur le fond (H-UI-45 : 3:1).
const MIN_BAR: u32 = 3_000;

/// Palette Okabe-Ito (jfly.uni-koeln.de/color) et couleurs des plannings
/// de chantier (gris, rouge, jaune).
const SWATCHES: [Rgb; 11] = [
    Rgb(0xE6, 0x9F, 0x00),
    Rgb(0x56, 0xB4, 0xE9),
    Rgb(0x00, 0x9E, 0x73),
    Rgb(0xF0, 0xE4, 0x42),
    Rgb(0x00, 0x72, 0xB2),
    Rgb(0xD5, 0x5E, 0x00),
    Rgb(0xCC, 0x79, 0xA7),
    Rgb(0x00, 0x00, 0x00),
    Rgb(0x7A, 0x7A, 0x7A),
    Rgb(0xC0, 0x00, 0x00),
    Rgb(0xFF, 0xC0, 0x00),
];

/// État du volet ouvert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatPanelState {
    /// Type de barre réglé.
    pub role: BarRole,
    /// Tâche de la barre cliquée (couleur de phase d'une récapitulative).
    pub task: Option<TaskUid>,
    /// Couleur libre en cours de choix (appliquée au relâcher).
    pending: Option<Rgb>,
    /// Épaisseur en cours de glisser (appliquée au relâcher).
    thickness: Option<u8>,
    /// Nom saisi pour « Enregistrer comme modèle… ».
    pub name: String,
}

impl FormatPanelState {
    /// Volet du type `role`, ouvert depuis la barre de `task`.
    #[must_use]
    pub const fn new(role: BarRole, task: Option<TaskUid>) -> Self {
        Self {
            role,
            task,
            pending: None,
            thickness: None,
            name: String::new(),
        }
    }
}

/// Effet d'une image du volet, appliqué par `App`.
#[derive(Clone, Debug, PartialEq)]
pub enum FormatEffect {
    /// Lot de commandes de style (un geste, une entrée d'annulation).
    Exec(Batch),
    /// Fermer le volet.
    Close,
    /// Enregistrer le style du document comme modèle personnel.
    SaveTemplate(String),
}

/// Entrées du volet à une image.
pub struct FormatInput<'a> {
    /// Session du document.
    pub session: &'a Session,
    /// Style effectif résolu.
    pub style: &'a PlanStyle,
    /// Style choisi du document, s'il en a un.
    pub doc: Option<DocStyle>,
    /// Modèle des documents sans style (base d'un premier réglage).
    pub default: TemplateId,
    /// Variante affichée (thème).
    pub variant: Variant,
    /// Langue des commandes.
    pub lang: Language,
}

/// Libellé d'un type de barre.
#[must_use]
pub const fn role_label(role: BarRole) -> TextId {
    match role {
        BarRole::Task => TextId::FpRoleTask,
        BarRole::Critical => TextId::FpRoleCritical,
        BarRole::Summary => TextId::FpRoleSummary,
        BarRole::SummaryCritical => TextId::FpRoleSummaryCritical,
        BarRole::Milestone => TextId::FpRoleMilestone,
        BarRole::MilestoneCritical => TextId::FpRoleMilestoneCritical,
        BarRole::Progress => TextId::FpRoleProgress,
        BarRole::Baseline => TextId::FpRoleBaseline,
        BarRole::Inactive => TextId::FpRoleInactive,
        BarRole::Manual => TextId::FpRoleManual,
        BarRole::Slack => TextId::FpRoleSlack,
        BarRole::Deadline => TextId::FpRoleDeadline,
    }
}

/// Libellé d'un champ affiché.
const fn field_label(f: BarField) -> TextId {
    match f {
        BarField::Name => TextId::FieldName,
        BarField::NameDuration => TextId::FieldNameDuration,
        BarField::Resources => TextId::FieldResources,
        BarField::Start => TextId::FieldStart,
        BarField::Finish => TextId::FieldFinish,
        BarField::StartShort => TextId::FieldStartShort,
        BarField::FinishShort => TextId::FieldFinishShort,
        BarField::Duration => TextId::FieldDuration,
        BarField::DurationShort => TextId::FieldDurationShort,
        BarField::Percent => TextId::FieldPercent,
        BarField::TotalSlack => TextId::FieldTotalSlack,
        BarField::StartFinish => TextId::FieldStartFinish,
    }
}

/// Rôle en JSON du registre.
const fn role_json(role: BarRole) -> BarRoleJson {
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
const fn field_json(f: BarField) -> BarFieldJson {
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

/// Variante en JSON du registre.
const fn variant_json(v: Variant) -> VariantJson {
    match v {
        Variant::Light => VariantJson::Light,
        Variant::Dark => VariantJson::Dark,
        Variant::Print => VariantJson::Print,
    }
}

/// Commande `style.set_bar_role` vide (aucun réglage) pour `role`.
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

/// Lot d'une commande.
fn one(cmd: Command, lang: Language) -> Batch {
    crate::dialog::batch_of(vec![cmd], lang)
}

/// Couleur du rôle telle que dessinée (contour pour un motif « contour
/// seul »).
fn shown_color(style: &PlanStyle, role: BarRole, v: Variant) -> Option<Rgb> {
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

/// Lot qui pose la couleur `rgb` sur le rôle (variante affichée).
fn color_batch(role: BarRole, rgb: Rgb, input: &FormatInput<'_>) -> Batch {
    let mut cmd = set_role(role, base_of(input));
    cmd.variant = Some(variant_json(input.variant));
    cmd.color = Some(rgb.hex());
    one(Command::StyleSetBarRole(cmd), input.lang)
}

/// Modèle de départ d'un premier réglage : celui du document, sinon le
/// modèle par défaut (jamais Classique par mégarde).
fn base_of(input: &FormatInput<'_>) -> TemplateId {
    input.doc.as_ref().map_or(input.default, |d| d.base)
}

/// Lot des couleurs de phase `phases` (remplacées en entier) : surcharge
/// fusionnée si le document a un style, sinon modèle par défaut et
/// surcharge (H-UI-65).
fn phases_batch(phases: &BTreeMap<u32, PhaseColor>, input: &FormatInput<'_>) -> Batch {
    let patch = serde_json::json!({ "phases": phases });
    let template = input.doc.is_none().then(|| TemplateJson::from(input.default));
    let cmd = StyleApply {
        template,
        patch: Some(patch),
        reset: false,
    };
    one(Command::StyleApply(cmd), input.lang)
}

/// Identifiant de la fenêtre du volet.
#[must_use]
pub fn window_id() -> egui::Id {
    egui::Id::unique("plannercraft.format-panel")
}

/// Bouton d'une couleur du nuancier (nom accessible « Couleur #RRGGBB »).
fn swatch(ui: &mut egui::Ui, texts: &Texts, label: TextId, rgb: Rgb, selected: bool) -> bool {
    let size = egui::vec2(18.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let name = texts.get_args(label, &[("color", Arg::Str(rgb.hex().into()))]);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name.clone()));
    let stroke = if selected {
        egui::Stroke::new(2.0, ui.visuals().strong_text_color())
    } else {
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    ui.painter()
        .rect_filled(rect, 2.0, egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2));
    ui.painter()
        .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
    response.on_hover_text(rgb.hex()).clicked()
}

/// Liste de choix d'un champ affiché (« Aucun » compris) ; rend le choix
/// fait à cette image.
fn field_combo(
    ui: &mut egui::Ui,
    texts: &Texts,
    label: TextId,
    current: Option<BarField>,
) -> Option<Patch<BarFieldJson>> {
    let mut chosen = None;
    let shown = current.map_or_else(|| texts.get(TextId::FpNone), |f| texts.get(field_label(f)));
    egui::ComboBox::from_label(texts.get(label))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(current.is_none(), texts.get(TextId::FpNone))
                .clicked()
            {
                chosen = Some(Patch::Clear);
            }
            for f in BarField::ALL {
                if ui
                    .selectable_label(current == Some(f), texts.get(field_label(f)))
                    .clicked()
                {
                    chosen = Some(Patch::Set(field_json(f)));
                }
            }
        });
    chosen
}

/// Commande de la case « durée après le nom » (H-UI-63) : le champ Nom
/// devient « Nom (durée) » et inversement, à droite (ou à gauche s'il y
/// est) ; sans nom affiché, « Nom (durée) » à droite.
fn duration_after_name(texts: BarTexts, on: bool) -> (Patch<BarFieldJson>, Patch<BarFieldJson>) {
    let from = if on {
        BarField::Name
    } else {
        BarField::NameDuration
    };
    let to = if on {
        BarField::NameDuration
    } else {
        BarField::Name
    };
    if texts.right == Some(from) {
        (Patch::Keep, Patch::Set(field_json(to)))
    } else if texts.left == Some(from) {
        (Patch::Set(field_json(to)), Patch::Keep)
    } else if on {
        (Patch::Keep, Patch::Set(BarFieldJson::NameDuration))
    } else {
        (Patch::Keep, Patch::Keep)
    }
}

/// Dessine le volet (une image) ; rend ses effets.
#[allow(clippy::too_many_lines, reason = "suite de réglages, un bloc chacun")]
pub fn show(
    ctx: &egui::Context,
    texts: &Texts,
    state: &mut FormatPanelState,
    input: &FormatInput<'_>,
) -> Vec<FormatEffect> {
    let mut effects = Vec::new();
    let role = state.role;
    let style = input.style;
    let v = input.variant;
    let tokens = style.tokens(v);
    let title = texts.get_args(
        TextId::FpTitle,
        &[("role", Arg::Str(texts.get(role_label(role)).into()))],
    );
    let mut open = true;
    egui::Window::new(title)
        .id(window_id())
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 96.0))
        .resizable(false)
        .collapsible(false)
        .default_width(330.0)
        .show(ctx, |ui| {
            // Type de barre.
            let mut chosen_role = role;
            egui::ComboBox::from_label(texts.get(TextId::FpRole))
                .selected_text(texts.get(role_label(role)))
                .show_ui(ui, |ui| {
                    for r in BarRole::ALL {
                        ui.selectable_value(&mut chosen_role, r, texts.get(role_label(r)));
                    }
                });
            if chosen_role != role {
                state.role = chosen_role;
                state.pending = None;
                state.thickness = None;
            }
            ui.separator();
            // Couleur : nuancier du modèle, palette, couleur libre.
            let current = shown_color(style, role, v);
            ui.label(texts.get(TextId::FpColor));
            let mut model: Vec<Rgb> = tokens.bars.values().map(|p| p.fill).collect();
            model.sort_unstable();
            model.dedup();
            // Une couleur n'apparaît qu'une fois (nom accessible unique).
            let extra: Vec<Rgb> = SWATCHES.iter().copied().filter(|c| !model.contains(c)).collect();
            for row in [model.as_slice(), extra.as_slice()] {
                ui.horizontal_wrapped(|ui| {
                    for &rgb in row {
                        if swatch(ui, texts, TextId::FpSwatch, rgb, current == Some(rgb)) {
                            effects.push(FormatEffect::Exec(color_batch(role, rgb, input)));
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                let base = state.pending.or(current).unwrap_or(Rgb(0x80, 0x80, 0x80));
                let mut srgb = [base.0, base.1, base.2];
                let response = ui.color_edit_button_srgb(&mut srgb);
                let name = texts.get(TextId::FpCustomColor);
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name.clone()));
                if response.changed() {
                    state.pending = Some(Rgb(srgb[0], srgb[1], srgb[2]));
                }
                ui.label(texts.get(TextId::FpCustomColor));
            });
            // Couleur libre appliquée au relâcher : un geste, une entrée.
            let pointer_down = ctx.input(|i| i.pointer.any_down());
            if !pointer_down
                && let Some(rgb) = state.pending.take()
                && Some(rgb) != current
            {
                effects.push(FormatEffect::Exec(color_batch(role, rgb, input)));
            }
            // Contraste (H-UI-61).
            if let Some(rgb) = current {
                let ratio = contrast_ratio(rgb, tokens.bg);
                if ratio < MIN_BAR {
                    let shown = format!("{},{}", ratio / 1000, (ratio % 1000) / 100);
                    let shown = if matches!(input.lang, Language::En) {
                        shown.replace(',', ".")
                    } else {
                        shown
                    };
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        texts.get_args(TextId::FpContrastLow, &[("ratio", Arg::Str(shown.into()))]),
                    );
                    if let Some(better) = nearest_compliant(rgb, tokens.bg, MIN_BAR) {
                        let label = texts.get_args(
                            TextId::FpUseSuggestion,
                            &[("color", Arg::Str(better.hex().into()))],
                        );
                        if ui.button(label).clicked() {
                            effects.push(FormatEffect::Exec(color_batch(role, better, input)));
                        }
                    }
                }
            }
            ui.separator();
            // Épaisseur et extrémités.
            let shape = style.bars.get(&role).copied();
            if let Some(shape) = shape {
                let mut t = state.thickness.unwrap_or(shape.height_pct);
                let response =
                    ui.add(egui::Slider::new(&mut t, 10..=100).text(texts.get(TextId::FpThickness)));
                if response.changed() {
                    state.thickness = Some(t);
                }
                if !response.dragged()
                    && !response.has_focus()
                    && let Some(t) = state.thickness.take()
                    && t != shape.height_pct
                {
                    let mut cmd = set_role(role, base_of(input));
                    cmd.thickness = Some(t);
                    effects.push(FormatEffect::Exec(one(Command::StyleSetBarRole(cmd), input.lang)));
                }
                ui.horizontal(|ui| {
                    ui.label(texts.get(TextId::FpEnds));
                    let rounded = shape.radius16 > 0;
                    for (label, ends, on) in [
                        (TextId::FpEndsSquare, BarEndsJson::Square, !rounded),
                        (TextId::FpEndsRounded, BarEndsJson::Rounded, rounded),
                    ] {
                        if ui.radio(on, texts.get(label)).clicked() && !on {
                            let mut cmd = set_role(role, base_of(input));
                            cmd.ends = Some(ends);
                            effects.push(FormatEffect::Exec(one(Command::StyleSetBarRole(cmd), input.lang)));
                        }
                    }
                });
            }
            ui.separator();
            // Textes (H-UI-63).
            let bar_texts = style.texts.get(&role).copied().unwrap_or_default();
            let mut cmd = set_role(role, base_of(input));
            if let Some(p) = field_combo(ui, texts, TextId::FpTextLeft, bar_texts.left) {
                cmd.text_left = p;
            }
            if let Some(p) = field_combo(ui, texts, TextId::FpTextRight, bar_texts.right) {
                cmd.text_right = p;
            }
            let mut with_duration = bar_texts.left == Some(BarField::NameDuration)
                || bar_texts.right == Some(BarField::NameDuration);
            if ui
                .checkbox(&mut with_duration, texts.get(TextId::FpDurationAfterName))
                .changed()
            {
                let (left, right) = duration_after_name(bar_texts, with_duration);
                cmd.text_left = left;
                cmd.text_right = right;
            }
            egui::CollapsingHeader::new(texts.get(TextId::FpDetails))
                .id_salt("plannercraft.format-details")
                .show(ui, |ui| {
                    if let Some(p) = field_combo(ui, texts, TextId::FpTextInside, bar_texts.inside) {
                        cmd.text_inside = p;
                    }
                });
            if !(cmd.text_left.is_keep() && cmd.text_right.is_keep() && cmd.text_inside.is_keep()) {
                effects.push(FormatEffect::Exec(one(Command::StyleSetBarRole(cmd), input.lang)));
            }
            // Couleur de phase d'une récapitulative (H-UI-65).
            if let Some(uid) = state.task
                && let Some(task) = input.session.project().tasks.get(uid)
                && plannercraft_automation::command::is_summary_at(
                    &input.session.project().tasks,
                    input.session.project().tasks.position(uid).unwrap_or(usize::MAX),
                )
            {
                ui.separator();
                let name = task.name.clone();
                ui.label(texts.get_args(TextId::FpPhase, &[("name", Arg::Str(name.into()))]));
                let key = uid.get();
                let own = style.phases.get(&key).copied();
                ui.horizontal_wrapped(|ui| {
                    for &rgb in &SWATCHES {
                        if swatch(
                            ui,
                            texts,
                            TextId::FpPhaseSwatch,
                            rgb,
                            own.is_some_and(|p| p.fill == rgb),
                        ) {
                            let mut phases = style.phases.clone();
                            phases.insert(
                                key,
                                PhaseColor {
                                    fill: rgb,
                                    children: own.is_none_or(|p| p.children),
                                },
                            );
                            effects.push(FormatEffect::Exec(phases_batch(&phases, input)));
                        }
                    }
                });
                // Contraste de la couleur de phase (H-UI-61, relecture finale
                // de J8) : une couleur pour toutes les variantes, mesurée sur
                // le fond de chacune ; proposition sur le fond le plus
                // exigeant si elle convient aux trois.
                if let Some(p) = own {
                    let bgs = [Variant::Light, Variant::Dark, Variant::Print].map(|v| style.tokens(v).bg);
                    let (ratio, worst) = bgs
                        .iter()
                        .map(|bg| (contrast_ratio(p.fill, *bg), *bg))
                        .min_by_key(|(r, _)| *r)
                        .unwrap_or((u32::MAX, tokens.bg));
                    if ratio < MIN_BAR {
                        let shown = format!("{},{}", ratio / 1000, (ratio % 1000) / 100);
                        let shown = if matches!(input.lang, Language::En) {
                            shown.replace(',', ".")
                        } else {
                            shown
                        };
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            texts.get_args(TextId::FpContrastLow, &[("ratio", Arg::Str(shown.into()))]),
                        );
                        let better = nearest_compliant(p.fill, worst, MIN_BAR)
                            .filter(|b| bgs.iter().all(|bg| contrast_ratio(*b, *bg) >= MIN_BAR));
                        if let Some(better) = better {
                            let label = texts.get_args(
                                TextId::FpUseSuggestion,
                                &[("color", Arg::Str(better.hex().into()))],
                            );
                            if ui.button(label).clicked() {
                                let mut phases = style.phases.clone();
                                phases.insert(key, PhaseColor { fill: better, ..p });
                                effects.push(FormatEffect::Exec(phases_batch(&phases, input)));
                            }
                        }
                    }
                }
                if let Some(p) = own {
                    let mut children = p.children;
                    if ui
                        .checkbox(&mut children, texts.get(TextId::FpPhaseChildren))
                        .changed()
                    {
                        let mut phases = style.phases.clone();
                        phases.insert(key, PhaseColor { children, ..p });
                        effects.push(FormatEffect::Exec(phases_batch(&phases, input)));
                    }
                    if ui.button(texts.get(TextId::FpPhaseRemove)).clicked() {
                        let mut phases = style.phases.clone();
                        phases.remove(&key);
                        effects.push(FormatEffect::Exec(phases_batch(&phases, input)));
                    }
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(texts.get(TextId::FpRevert)).clicked() {
                    let cmd = StyleResetToTemplate {
                        role: Some(role_json(role)),
                    };
                    effects.push(FormatEffect::Exec(one(
                        Command::StyleResetToTemplate(cmd),
                        input.lang,
                    )));
                }
            });
            ui.horizontal(|ui| {
                crate::dialog::text_field(
                    ui,
                    &texts.get(TextId::GallerySaveName),
                    &mut state.name,
                    true,
                    160.0,
                );
                if ui.button(texts.get(TextId::FpSaveTemplate)).clicked() {
                    effects.push(FormatEffect::SaveTemplate(state.name.clone()));
                }
            });
            if ui.button(texts.get(TextId::FpClose)).clicked() {
                effects.push(FormatEffect::Close);
            }
        });
    if !open {
        effects.push(FormatEffect::Close);
    }
    effects
}

impl crate::app::App {
    /// Applique une opération de mise en forme (J8 lot L15).
    pub(crate) fn format_op(&mut self, op: crate::action::FormatOp) {
        use crate::action::FormatOp;
        match op {
            FormatOp::OpenBars { role, task } => {
                if self.document.is_some() {
                    self.view.format_panel = Some(FormatPanelState::new(role, task));
                }
            }
            FormatOp::CloseBars => self.view.format_panel = None,
            FormatOp::SetRole(role) => {
                if let Some(panel) = &mut self.view.format_panel {
                    panel.role = role;
                }
            }
            FormatOp::SaveUserTemplate(name) => {
                let text = self.save_user_template_named(&name);
                self.push_message(plannercraft_core::Severity::Info, text);
            }
            FormatOp::ApplyUserTemplate(i) => {
                use plannercraft_automation::command::Command;
                use plannercraft_automation::command::style::{StyleApply, TemplateJson};
                let lang = self.text_language();
                if let Some(t) = self.state.user_templates.get(i) {
                    let patch = (!t.style.over.is_empty())
                        .then(|| serde_json::to_value(&t.style.over).ok())
                        .flatten();
                    let batch = crate::dialog::batch_of(
                        vec![Command::StyleApply(StyleApply {
                            template: Some(TemplateJson::from(t.style.base)),
                            patch,
                            reset: false,
                        })],
                        lang,
                    );
                    drop(self.execute(&batch, crate::app::EditOrigin::User));
                }
            }
            FormatOp::SetDefaultTemplate(id) => self.state.default_template = id,
            FormatOp::SetRelativeMonths(on) => self.set_relative_months(on),
        }
    }

    /// H-UI-67 : mois relatifs de l'échelle, en surcharge du style du
    /// document (`style.apply { patch: { scale } }`, échelle résolue avec
    /// le seul drapeau changé ; document sans style : modèle par défaut).
    fn set_relative_months(&mut self, on: bool) {
        use plannercraft_automation::command::Command;
        use plannercraft_automation::command::style::{StyleApply, TemplateJson, doc_style};
        let lang = self.text_language();
        let Some(document) = &self.document else {
            return;
        };
        let mut scale = self.state.effective_style(document.project()).scale;
        if scale.relative_months == on {
            return;
        }
        scale.relative_months = on;
        let template = doc_style(document.project())
            .is_none()
            .then(|| TemplateJson::from(self.state.default_template));
        let patch = serde_json::to_value(scale)
            .ok()
            .map(|scale| serde_json::json!({ "scale": scale }));
        let batch = crate::dialog::batch_of(
            vec![Command::StyleApply(StyleApply {
                template,
                patch,
                reset: false,
            })],
            lang,
        );
        drop(self.execute(&batch, crate::app::EditOrigin::User));
    }

    /// Dessine le volet de mise en forme s'il est ouvert ; ses lots passent
    /// par la file des actions (une entrée d'annulation chacun).
    pub(crate) fn show_format_panel(
        &mut self,
        ctx: &egui::Context,
        actions: &mut Vec<crate::action::Action>,
    ) {
        use crate::action::{Action, FormatOp};
        let lang = self.text_language();
        let (Some(document), Some(state)) = (&self.document, &mut self.view.format_panel) else {
            return;
        };
        let style = self.state.effective_style(document.project());
        let variant = match ctx.theme() {
            egui::Theme::Light => Variant::Light,
            egui::Theme::Dark => Variant::Dark,
        };
        let input = FormatInput {
            session: &document.session,
            style: &style,
            doc: plannercraft_automation::command::style::doc_style(document.project()),
            default: self.state.default_template,
            variant,
            lang,
        };
        for effect in show(ctx, &self.texts, state, &input) {
            match effect {
                FormatEffect::Exec(batch) => actions.push(Action::Exec(batch)),
                FormatEffect::Close => actions.push(Action::Format(FormatOp::CloseBars)),
                FormatEffect::SaveTemplate(name) => {
                    actions.push(Action::Format(FormatOp::SaveUserTemplate(name)));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests : un échec doit paniquer")]

    use super::*;

    /// H-UI-63 : la case « durée après le nom » bascule le champ Nom à
    /// droite (ou à gauche) entre « Nom » et « Nom (durée) ».
    #[test]
    fn h_ui_63_duration_after_name_patch() {
        let right = BarTexts {
            right: Some(BarField::Name),
            ..BarTexts::default()
        };
        assert_eq!(
            duration_after_name(right, true),
            (Patch::Keep, Patch::Set(BarFieldJson::NameDuration))
        );
        let back = BarTexts {
            right: Some(BarField::NameDuration),
            ..BarTexts::default()
        };
        assert_eq!(
            duration_after_name(back, false),
            (Patch::Keep, Patch::Set(BarFieldJson::Name))
        );
        let left = BarTexts {
            left: Some(BarField::Name),
            right: Some(BarField::DurationShort),
            ..BarTexts::default()
        };
        assert_eq!(
            duration_after_name(left, true),
            (Patch::Set(BarFieldJson::NameDuration), Patch::Keep)
        );
        assert_eq!(
            duration_after_name(BarTexts::default(), true),
            (Patch::Keep, Patch::Set(BarFieldJson::NameDuration))
        );
    }

    #[test]
    fn json_mappings_round_trip() {
        for role in BarRole::ALL {
            assert_eq!(BarRole::from(role_json(role)), role);
        }
        for f in BarField::ALL {
            assert_eq!(BarField::from(field_json(f)), f);
        }
    }
}
