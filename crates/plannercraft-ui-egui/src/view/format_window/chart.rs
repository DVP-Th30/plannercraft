//! Onglet « Graphique » de la fenêtre « Mise en forme » (lot F1,
//! H-UI-220 à H-UI-224) : réglages que le modèle de style sait déjà faire,
//! et rien d'autre (ce que la maquette marque « nouveau » viendra avec les
//! lots F2 et F3).
//!
//! - « Que voulez-vous changer ? » : cartes des types de barres dessinées
//!   dans leur style courant, avec le compte de barres du planning (H-UI-197,
//!   grisées si le type est absent, toujours choisissables) ; les autres
//!   types réglables dans « Autres barres » (marge et échéance, que
//!   l'application ne dessine pas, ne sont pas proposées).
//! - Case « Même couleur pour les tâches critiques » (H-UI-221).
//! - Couleur (nuancier stable, couleur libre, contraste : H-UI-193,
//!   H-UI-194), épaisseur, extrémités, remplissage (H-UI-222), couleur de
//!   phase d'une récapitulative (H-UI-65).
//! - « Textes de la barre » (H-UI-223), « Échelle de temps » et « Fond et
//!   repères » (H-UI-224).

use egui::{Color32, Rect, Sense, Stroke, pos2, vec2};
use plannercraft_render::style::{BarField, BarRole, BarShape, Pattern, PhaseColor, PlanStyle, Rgb, Variant};

use plannercraft_automation::command::Patch;

use super::commands::{self, shown_color};
use super::{FormatInput, FormatWindowState, SWATCHES, role_label};
use crate::action::Action;
use crate::i18n::{Arg, TextId, Texts};

/// Cartes principales (2 × 2), dans l'ordre de la maquette.
const MAIN_ROLES: [BarRole; 4] = [
    BarRole::Task,
    BarRole::Critical,
    BarRole::Summary,
    BarRole::Milestone,
];

/// Autres types réglables (« Autres barres »).
const OTHER_ROLES: [BarRole; 6] = [
    BarRole::SummaryCritical,
    BarRole::MilestoneCritical,
    BarRole::Progress,
    BarRole::Baseline,
    BarRole::Manual,
    BarRole::Inactive,
];

/// Hauteur d'une carte principale, en points.
const CARD_H: f32 = 56.0;

/// Opacité d'une carte dont le type est absent du planning.
const ABSENT_OPACITY: f32 = 0.45;

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

/// Titre de groupe (petit, atténué, appuyé).
pub(super) fn heading(ui: &mut egui::Ui, text: String) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).size(11.5).strong().weak());
}

/// Réglage : libellé, puis le contrôle dessous (une colonne de 344 points
/// ne tient pas « libellé + quatre options » côte à côte, surtout traduit).
pub(super) fn setting_row(ui: &mut egui::Ui, label: String, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(2.0);
    ui.label(label);
    add(ui);
}

/// Contrôle segmenté : une option par libellé, l'option courante marquée ;
/// rend l'option cliquée si elle diffère de la courante.
pub(super) fn segmented<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    current: Option<T>,
    options: &[(T, String)],
) -> Option<T> {
    let mut chosen = None;
    // Options de gauche à droite, à la ligne si la largeur manque.
    egui::Frame::NONE
        .fill(ui.visuals().widgets.inactive.weak_bg_fill)
        .corner_radius(7.0)
        .inner_margin(2.0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            ui.horizontal_wrapped(|ui| {
                for (value, label) in options {
                    let on = current == Some(*value);
                    if ui.selectable_label(on, label.as_str()).clicked() && !on {
                        chosen = Some(*value);
                    }
                }
            });
        });
    chosen
}

/// Dessine l'onglet Graphique.
#[allow(clippy::too_many_lines, reason = "suite de réglages, un bloc chacun")]
pub(super) fn show(
    ui: &mut egui::Ui,
    texts: &Texts,
    state: &mut FormatWindowState,
    input: &FormatInput<'_>,
    actions: &mut Vec<Action>,
) {
    let ctx = &input.ctx;
    let v = input.variant;
    heading(ui, texts.get(TextId::FwWhat));
    // Cartes principales.
    let gap = 6.0;
    let card_w = ((ui.available_width() - gap) / 2.0).max(60.0);
    for pair in MAIN_ROLES.chunks(2) {
        let h = card_height(ui, texts, pair, card_w, CARD_H);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for role in pair {
                if card(ui, texts, input, *role, state.role == *role, vec2(card_w, h)) {
                    state.set_role(*role);
                }
            }
        });
    }
    // Autres barres : ouvertes tant qu'un de leurs types est choisi.
    let other = OTHER_ROLES.contains(&state.role);
    egui::CollapsingHeader::new(texts.get(TextId::FwOtherBars))
        .id_salt("plannercraft.format-window.other-bars")
        .open(other.then_some(true))
        .show(ui, |ui| {
            for pair in OTHER_ROLES.chunks(2) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    let w = ((ui.available_width() - gap) / 2.0).max(60.0);
                    let h = card_height(ui, texts, pair, w, CARD_H - 6.0);
                    for role in pair {
                        if card(ui, texts, input, *role, state.role == *role, vec2(w, h)) {
                            state.set_role(*role);
                        }
                    }
                });
            }
        });
    let role = state.role;
    // Jumeaux critiques (H-UI-221).
    let twins = commands::twins_same(ctx.style, role);
    if commands::twin(role).is_some() {
        let label = match commands::plain_of(role) {
            BarRole::Summary => TextId::FwTwinsSummary,
            BarRole::Milestone => TextId::FwTwinsMilestone,
            _ => TextId::FwTwinsTask,
        };
        let mut same = twins;
        if ui.checkbox(&mut same, texts.get(label)).changed() {
            if same {
                if let Some(rgb) = shown_color(input.shown, role, v) {
                    actions.push(Action::Exec(commands::twins_on(role, rgb, ctx)));
                }
            } else if let Some(batch) = commands::twins_off(role, ctx) {
                actions.push(Action::Exec(batch));
            }
        }
    }
    ui.separator();
    // Couleur (lot R1c, H-UI-193) : nuancier stable en trois rangées,
    // couleur libre ; couleur courante telle que dessinée.
    heading(ui, texts.get(TextId::FpColor));
    let current = shown_color(input.shown, role, v);
    let model = crate::theme::screen_style(plannercraft_render::style::template(ctx.base())).0;
    for row in super::swatch_rows(&model, input.shown, v) {
        if row.is_empty() {
            continue;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
            for rgb in row {
                if super::big_swatch(ui, texts, TextId::FpSwatch, rgb, current == Some(rgb)) {
                    actions.push(Action::Exec(commands::color(role, rgb, twins, ctx)));
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
    let pointer_down = ui.ctx().input(|i| i.pointer.any_down());
    if !pointer_down
        && let Some(rgb) = state.pending.take()
        && Some(rgb) != current
    {
        actions.push(Action::Exec(commands::color(role, rgb, twins, ctx)));
    }
    // Contraste (H-UI-61, H-UI-194) : une couleur choisie vaut pour les
    // trois variantes, mesurée sur le fond de chacune ; une couleur du
    // modèle l'est sur le fond affiché seulement.
    let user = ctx
        .style
        .tokens(v)
        .bars
        .get(&role)
        .is_some_and(plannercraft_render::style::BarPaint::is_user);
    let all = [Variant::Light, Variant::Dark, Variant::Print].map(|v| (v, ctx.style.tokens(v).bg));
    let shown_bg = [(v, ctx.style.tokens(v).bg)];
    let bgs: &[(Variant, Rgb)] = if user { &all } else { &shown_bg };
    if let Some(rgb) = current
        && let Some(better) = super::contrast_warning(ui, texts, ctx.lang, rgb, bgs)
    {
        actions.push(Action::Exec(commands::color(role, better, twins, ctx)));
    }
    ui.separator();
    // Forme (H-UI-222).
    if let Some(shape) = ctx.style.bars.get(&role).copied() {
        setting_row(ui, texts.get(TextId::FpThickness), |ui| {
            let mut t = state.thickness.unwrap_or(shape.height_pct);
            let response = ui.add(egui::Slider::new(&mut t, 10..=100).suffix(" %").show_value(true));
            let name = texts.get(TextId::FpThickness);
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::labeled(egui::Role::Slider, true, name.clone());
                info.value = Some(f64::from(t));
                info
            });
            if response.changed() {
                state.thickness = Some(t);
            }
            if !response.dragged()
                && !response.has_focus()
                && let Some(t) = state.thickness.take()
                && t != shape.height_pct
            {
                actions.push(Action::Exec(commands::thickness(role, t, ctx)));
            }
        });
        if shape.shape == BarShape::Bar {
            setting_row(ui, texts.get(TextId::FpEnds), |ui| {
                let options = [
                    (true, texts.get(TextId::FpEndsRounded)),
                    (false, texts.get(TextId::FpEndsSquare)),
                ];
                if let Some(rounded) = segmented(ui, Some(shape.radius16 > 0), &options) {
                    actions.push(Action::Exec(commands::ends(role, rounded, ctx)));
                }
            });
        }
        if shape.shape != BarShape::Line {
            setting_row(ui, texts.get(TextId::FwFill), |ui| {
                let options = [
                    (Pattern::Solid, texts.get(TextId::FwFillSolid)),
                    (Pattern::Hatch, texts.get(TextId::FwFillHatch)),
                    (Pattern::Dots, texts.get(TextId::FwFillDots)),
                    (Pattern::Outline, texts.get(TextId::FwFillOutline)),
                ];
                if let Some(p) = segmented(ui, Some(shape.pattern), &options)
                    && let Some(batch) = commands::pattern(role, p, ctx)
                {
                    actions.push(Action::Exec(batch));
                }
            });
        }
    }
    if ui
        .button(texts.get(TextId::FwResetRole))
        .on_hover_text(texts.get(TextId::FwResetRoleTip))
        .clicked()
    {
        actions.push(Action::Exec(commands::reset_role(role, ctx.lang)));
    }
    phase_section(ui, texts, state, input, actions);
    ui.separator();
    // Textes de la barre (H-UI-223).
    egui::CollapsingHeader::new(texts.get(TextId::FwBarTexts))
        .id_salt("plannercraft.format-window.bar-texts")
        .default_open(true)
        .show(ui, |ui| {
            let now = ctx.style.texts.get(&role).copied().unwrap_or_default();
            let mut changes = [Patch::Keep, Patch::Keep, Patch::Keep];
            egui::Grid::new("plannercraft.format-window.texts-grid")
                .num_columns(2)
                .spacing(vec2(10.0, 6.0))
                .show(ui, |ui| {
                    for (slot, (id, value)) in [
                        (TextId::FwTextLeft, now.left),
                        (TextId::FwTextRight, now.right),
                        (TextId::FwTextInside, now.inside),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let name = texts.get(id);
                        ui.label(name.as_str());
                        let chosen = field_combo(ui, texts, ("bar-text", slot), &name, value);
                        if let Some(cell) = changes.get_mut(slot) {
                            *cell = chosen;
                        }
                        ui.end_row();
                    }
                });
            let [left, right, inside] = changes;
            if let Some(batch) = commands::texts(role, left, right, inside, ctx) {
                actions.push(Action::Exec(batch));
            }
        });
    // Échelle de temps (H-UI-224).
    egui::CollapsingHeader::new(texts.get(TextId::FwScale))
        .id_salt("plannercraft.format-window.scale")
        .show(ui, |ui| {
            let scale = ctx.style.scale;
            let mut relative = scale.relative_months;
            if ui
                .checkbox(&mut relative, texts.get(TextId::FwRelativeMonths))
                .changed()
                && let Some(b) = commands::scale(ctx, |s| s.relative_months = relative)
            {
                actions.push(Action::Exec(b));
            }
            let mut weeks = scale.week_numbers;
            if ui
                .checkbox(&mut weeks, texts.get(TextId::FwWeekNumbers))
                .changed()
                && let Some(b) = commands::scale(ctx, |s| s.week_numbers = weeks)
            {
                actions.push(Action::Exec(b));
            }
        });
    // Fond et repères (H-UI-224).
    egui::CollapsingHeader::new(texts.get(TextId::FwBackground))
        .id_salt("plannercraft.format-window.background")
        .show(ui, |ui| {
            let (scale, grid) = (ctx.style.scale, ctx.style.grid);
            let mut shade = scale.shade_non_working;
            if ui.checkbox(&mut shade, texts.get(TextId::FwNonWorking)).changed()
                && let Some(b) = commands::scale(ctx, |s| s.shade_non_working = shade)
            {
                actions.push(Action::Exec(b));
            }
            let mut major = grid.major_v;
            if ui.checkbox(&mut major, texts.get(TextId::FwMajorLines)).changed()
                && let Some(b) = commands::grid(ctx, |g| g.major_v = major)
            {
                actions.push(Action::Exec(b));
            }
            let mut minor = grid.minor_v;
            if ui.checkbox(&mut minor, texts.get(TextId::FwMinorLines)).changed()
                && let Some(b) = commands::grid(ctx, |g| g.minor_v = minor)
            {
                actions.push(Action::Exec(b));
            }
            let mut today = scale.today_mark;
            if ui.checkbox(&mut today, texts.get(TextId::FwToday)).changed()
                && let Some(b) = commands::scale(ctx, |s| s.today_mark = today)
            {
                actions.push(Action::Exec(b));
            }
            let mut status = scale.status_label;
            if ui
                .checkbox(&mut status, texts.get(TextId::FwStatusLabel))
                .changed()
                && let Some(b) = commands::scale(ctx, |s| s.status_label = status)
            {
                actions.push(Action::Exec(b));
            }
        });
}

/// Couleur de phase d'une récapitulative (H-UI-65) : tâche active, sinon la
/// barre cliquée, si c'est une récapitulative. Une couleur de barre : elle
/// est dans l'onglet Graphique, avec une phrase qui dit qu'elle teinte aussi
/// la ligne du tableau.
fn phase_section(
    ui: &mut egui::Ui,
    texts: &Texts,
    state: &FormatWindowState,
    input: &FormatInput<'_>,
    actions: &mut Vec<Action>,
) {
    let project = input.session.project();
    let is_summary = |uid: plannercraft_core::TaskUid| {
        project
            .tasks
            .position(uid)
            .is_some_and(|p| plannercraft_automation::command::is_summary_at(&project.tasks, p))
    };
    let Some(uid) = input
        .active
        .filter(|u| is_summary(*u))
        .or(state.task.filter(|u| is_summary(*u)))
    else {
        return;
    };
    let Some(task) = project.tasks.get(uid) else {
        return;
    };
    let ctx = &input.ctx;
    let style: &PlanStyle = ctx.style;
    ui.separator();
    heading(
        ui,
        texts.get_args(TextId::FpPhase, &[("name", Arg::Str(task.name.clone().into()))]),
    );
    ui.add(egui::Label::new(egui::RichText::new(texts.get(TextId::FwPhaseHint)).weak().small()).wrap());
    let key = uid.get();
    let own = style.phases.get(&key).copied();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
        for &rgb in &SWATCHES {
            if super::big_swatch(
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
                actions.push(Action::Exec(commands::phases(&phases, ctx)));
            }
        }
    });
    // Contraste de la couleur de phase (H-UI-61) : une couleur pour toutes
    // les variantes, mesurée sur le fond de chacune.
    if let Some(p) = own {
        let bgs = [Variant::Light, Variant::Dark, Variant::Print].map(|v| (v, style.tokens(v).bg));
        if let Some(better) = super::contrast_warning(ui, texts, ctx.lang, p.fill, &bgs) {
            let mut phases = style.phases.clone();
            phases.insert(key, PhaseColor { fill: better, ..p });
            actions.push(Action::Exec(commands::phases(&phases, ctx)));
        }
        let mut children = p.children;
        if ui
            .checkbox(&mut children, texts.get(TextId::FpPhaseChildren))
            .changed()
        {
            let mut phases = style.phases.clone();
            phases.insert(key, PhaseColor { children, ..p });
            actions.push(Action::Exec(commands::phases(&phases, ctx)));
        }
        if ui.button(texts.get(TextId::FpPhaseRemove)).clicked() {
            let mut phases = style.phases.clone();
            phases.remove(&key);
            actions.push(Action::Exec(commands::phases(&phases, ctx)));
        }
    }
}

/// Liste de choix d'un champ affiché (« Aucun » compris), nommée `name`
/// pour l'accessibilité ; rend le choix fait à cette image (`Clear` :
/// aucun texte ; `Keep` : rien de choisi, ou le champ courant).
fn field_combo(
    ui: &mut egui::Ui,
    texts: &Texts,
    salt: (&'static str, usize),
    name: &str,
    current: Option<BarField>,
) -> Patch<BarField> {
    let mut chosen = Patch::Keep;
    let shown = current.map_or_else(|| texts.get(TextId::FpNone), |f| texts.get(field_label(f)));
    let response = egui::ComboBox::from_id_salt(salt)
        .selected_text(shown.as_str())
        .width(170.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(current.is_none(), texts.get(TextId::FpNone))
                .clicked()
                && current.is_some()
            {
                chosen = Patch::Clear;
            }
            for f in BarField::ALL {
                if ui
                    .selectable_label(current == Some(f), texts.get(field_label(f)))
                    .clicked()
                    && current != Some(f)
                {
                    chosen = Patch::Set(f);
                }
            }
        })
        .response;
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name);
        info.current_text_value = Some(shown.clone());
        info
    });
    chosen
}

/// Police du nom d'une carte.
fn card_font(ctx: &egui::Context) -> egui::FontId {
    crate::theme::strong_font(ctx, 12.5)
}

/// Hauteur commune des cartes `roles` d'une rangée de largeur `width` : au
/// moins `base`, plus les lignes de plus d'un nom replié (traductions
/// longues : jamais coupé).
fn card_height(ui: &egui::Ui, texts: &Texts, roles: &[BarRole], width: f32, base: f32) -> f32 {
    let one = ui
        .painter()
        .layout_no_wrap("Ag".to_owned(), card_font(ui.ctx()), Color32::WHITE)
        .size()
        .y;
    roles
        .iter()
        .map(|role| {
            let name = ui.painter().layout(
                texts.get(role_label(*role)),
                card_font(ui.ctx()),
                Color32::WHITE,
                (width - 20.0).max(20.0),
            );
            base + (name.size().y - one).max(0.0)
        })
        .fold(base, f32::max)
}

/// Carte d'un type de barre (H-UI-220) : la barre dessinée dans son style
/// courant, son nom, le nombre de barres du planning (types de ligne) ;
/// grisée si le type est absent du planning, toujours choisissable. Nom
/// accessible : le nom du type. Rend vrai au clic.
fn card(
    ui: &mut egui::Ui,
    texts: &Texts,
    input: &FormatInput<'_>,
    role: BarRole,
    selected: bool,
    size: egui::Vec2,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let name = texts.get(role_label(role));
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, name.clone()));
    let count = super::is_row_role(role)
        .then(|| {
            input
                .rows
                .as_ref()
                .map(|rows| super::role_count(rows, role, input.highlight_critical))
        })
        .flatten();
    let absent = count == Some(0);
    let visuals = ui.visuals();
    let (fill, stroke) = if selected {
        (
            visuals.selection.bg_fill,
            Stroke::new(1.5, visuals.selection.stroke.color),
        )
    } else if response.hovered() {
        (
            visuals.widgets.hovered.weak_bg_fill,
            visuals.widgets.noninteractive.bg_stroke,
        )
    } else {
        (visuals.panel_fill, visuals.widgets.noninteractive.bg_stroke)
    };
    let painter = ui.painter_at(rect.expand(1.0));
    painter.rect(rect, 8.0, fill, stroke, egui::StrokeKind::Inside);
    let alpha = if absent { ABSENT_OPACITY } else { 1.0 };
    let text = if selected {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    let shape_area = Rect::from_min_size(
        pos2(rect.left() + 10.0, rect.top() + 8.0),
        vec2(rect.width() - 20.0, 12.0),
    );
    mini_bar(
        &painter,
        input.shown,
        role,
        input.variant,
        shape_area,
        alpha,
        fill,
    );
    let galley = painter.layout(
        name,
        card_font(ui.ctx()),
        text.gamma_multiply(alpha),
        (rect.width() - 20.0).max(20.0),
    );
    painter.galley(
        pos2(rect.left() + 10.0, shape_area.bottom() + 4.0),
        galley,
        text.gamma_multiply(alpha),
    );
    if let Some(n) = count {
        let line = texts.get_args(
            TextId::FwKindCount,
            &[("count", Arg::Count(i64::try_from(n).unwrap_or(i64::MAX)))],
        );
        painter.text(
            pos2(rect.left() + 10.0, rect.bottom() - 6.0),
            egui::Align2::LEFT_BOTTOM,
            line,
            egui::FontId::proportional(11.5),
            visuals.weak_text_color(),
        );
    }
    response.clicked()
}

/// Barre miniature d'un type dans son style courant (forme, couleur,
/// coins, motif « contour » ou hachures).
fn mini_bar(
    painter: &egui::Painter,
    style: &PlanStyle,
    role: BarRole,
    variant: Variant,
    area: Rect,
    alpha: f32,
    back: Color32,
) {
    let Some(rgb) = shown_color(style, role, variant) else {
        return;
    };
    let ink = Color32::from_rgb(rgb.0, rgb.1, rgb.2).gamma_multiply(alpha);
    let shape = style.bars.get(&role).copied();
    let pattern = shape.map_or(Pattern::Solid, |s| s.pattern);
    let mid = area.center().y;
    match shape.map_or(BarShape::Bar, |s| s.shape) {
        BarShape::Bar | BarShape::Line => {
            let height = 10.0;
            let bar = Rect::from_min_size(
                pos2(area.left(), mid - height / 2.0),
                vec2(56.0_f32.min(area.width()), height),
            );
            let radius = (f32::from(shape.map_or(0, |s| s.radius16)) / 16.0).min(height / 2.0);
            if pattern == Pattern::Outline {
                painter.rect_stroke(bar, radius, Stroke::new(1.5, ink), egui::StrokeKind::Inside);
            } else {
                painter.rect_filled(bar, radius, ink);
                if matches!(pattern, Pattern::Hatch | Pattern::Dots) {
                    let line = Stroke::new(1.0, back.gamma_multiply(alpha));
                    let mut x = bar.left() + 3.0;
                    while x < bar.right() - 1.0 {
                        if pattern == Pattern::Dots {
                            painter.circle_filled(pos2(x, mid), 1.0, line.color);
                        } else {
                            painter.line_segment(
                                [
                                    pos2(x, bar.bottom()),
                                    pos2((x + height).min(bar.right()), bar.top()),
                                ],
                                line,
                            );
                        }
                        x += 5.0;
                    }
                }
            }
        }
        BarShape::Bracket => {
            let w = 64.0_f32.min(area.width());
            let bar = Rect::from_min_size(pos2(area.left(), mid - 3.0), vec2(w, 4.0));
            painter.rect_filled(bar, 2.0, ink);
            for (x, dir) in [(bar.left(), 1.0), (bar.right(), -1.0)] {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        pos2(x, bar.bottom()),
                        pos2(x + dir * 4.0, bar.bottom()),
                        pos2(x, bar.bottom() + 4.0),
                    ],
                    ink,
                    Stroke::NONE,
                ));
            }
        }
        BarShape::Diamond => {
            let (cx, half) = (area.left() + 6.0, 6.0);
            let points = vec![
                pos2(cx, mid - half),
                pos2(cx + half, mid),
                pos2(cx, mid + half),
                pos2(cx - half, mid),
            ];
            if pattern == Pattern::Outline {
                painter.add(egui::Shape::closed_line(points, Stroke::new(1.5, ink)));
            } else {
                painter.add(egui::Shape::convex_polygon(points, ink, Stroke::NONE));
            }
        }
    }
}
