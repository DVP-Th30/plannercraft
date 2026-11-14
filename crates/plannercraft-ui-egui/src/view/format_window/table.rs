//! Onglet « Tableau » de la fenêtre « Mise en forme » (lot F1, H-UI-225,
//! H-UI-227, H-UI-228).
//!
//! - Colonnes : étiquettes des colonnes affichées, retirables d'un clic
//!   (sauf le N° et le nom, comme dans la table) ; « + Ajouter » ouvre le
//!   sélecteur de champs existant ; une étiquette choisie se déplace par
//!   deux flèches (ADR-0035 D7 : un glisser dans une liste qui passe à la
//!   ligne serait imprécis et invisible au clavier et aux lecteurs d'écran ;
//!   le glisser des titres de la table reste là) et ouvre « Paramètres de
//!   champ » (titre, largeur, alignement). Tout passe par les `ColumnOp`
//!   existants : les colonnes restent une préférence de l'application, pour
//!   tous les plannings (la phrase de l'onglet le dit ; le lot F2 les
//!   enregistrera dans le planning).
//! - Lignes : hauteur (compacte, normale, aérée) et fond (uni, zébré, par
//!   phase), écrits dans le style du document.
//! - Tâches sélectionnées : gras, italique, couleur du texte, surlignage,
//!   effacer (`TextStyleOp`, comme l'onglet Tâche du ruban).

use plannercraft_render::style::{Density, RowBanding};

use super::chart::{heading, segmented, setting_row};
use super::{FormatInput, FormatWindowState, SWATCHES, commands};
use crate::action::{Action, ColumnOp, TextStyleOp};
use crate::i18n::{Arg, TextId, Texts};

/// Dessine l'onglet Tableau.
pub(super) fn show(
    ui: &mut egui::Ui,
    texts: &Texts,
    state: &mut FormatWindowState,
    input: &FormatInput<'_>,
    actions: &mut Vec<Action>,
) {
    columns(ui, texts, state, input, actions);
    ui.separator();
    rows(ui, texts, input, actions);
    ui.separator();
    selected_tasks(ui, texts, input, actions);
}

/// Colonnes de la table (H-UI-225).
fn columns(
    ui: &mut egui::Ui,
    texts: &Texts,
    state: &mut FormatWindowState,
    input: &FormatInput<'_>,
    actions: &mut Vec<Action>,
) {
    heading(ui, texts.get(TextId::FwColumns));
    let columns = input.columns;
    if state.column.is_some_and(|c| c >= columns.len()) {
        state.column = None;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for (i, c) in columns.iter().enumerate() {
            let title = c.display_title(texts);
            let on = state.column == Some(i);
            let (pick, remove) = chip(ui, texts, &title, on, c.column.hideable());
            if pick {
                state.column = if on { None } else { Some(i) };
            }
            if remove {
                actions.push(Action::Column(ColumnOp::Hide(i)));
                state.column = None;
            }
        }
        if ui.button(texts.get(TextId::FwAddColumn)).clicked() {
            actions.push(Action::Column(ColumnOp::OpenPicker { at: columns.len() }));
        }
    });
    // Colonne choisie : déplacer, paramètres.
    if let Some(i) = state.column
        && let Some(c) = columns.get(i)
    {
        let title = c.display_title(texts);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("« {title} »")).strong());
            // Le N° reste en premier (H-UI-116) : il ne bouge pas, rien ne
            // passe devant lui.
            let can_left = i > 1;
            let next = i.saturating_add(1);
            let can_right = i >= 1 && next < columns.len();
            let left = ui
                .add_enabled(can_left, egui::Button::new("←"))
                .on_hover_text(texts.get(TextId::FwMoveLeft));
            let name = texts.get(TextId::FwMoveLeft);
            left.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can_left, name.clone()));
            if left.clicked() {
                actions.push(Action::Column(ColumnOp::Move {
                    from: i,
                    to: i.saturating_sub(1),
                }));
                state.column = Some(i.saturating_sub(1));
            }
            let right = ui
                .add_enabled(can_right, egui::Button::new("→"))
                .on_hover_text(texts.get(TextId::FwMoveRight));
            let name = texts.get(TextId::FwMoveRight);
            right.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can_right, name.clone()));
            if right.clicked() {
                actions.push(Action::Column(ColumnOp::Move { from: i, to: next }));
                state.column = Some(next);
            }
            if ui.button(texts.get(TextId::FwColumnSettings)).clicked() {
                actions.push(Action::Column(ColumnOp::OpenSettings(i)));
            }
        });
    } else {
        hint(ui, texts.get(TextId::FwColumnsHint));
    }
    hint(ui, texts.get(TextId::FwColumnsApp));
}

/// Étiquette d'une colonne (H-UI-225) : titre (clic : choisie) et, si la
/// colonne peut être masquée, une croix dans l'étiquette (clic : retirée).
/// Noms accessibles « Choisir la colonne … » et « Retirer la colonne … »
/// (le titre de la table porte déjà le nom de la colonne). Rend (choisie,
/// retirée) à cette image.
fn chip(ui: &mut egui::Ui, texts: &Texts, title: &str, on: bool, removable: bool) -> (bool, bool) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let visuals = ui.visuals().clone();
    let ink = if on {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    let galley = ui.painter().layout_no_wrap(title.to_owned(), font.clone(), ink);
    let cross = ui
        .painter()
        .layout_no_wrap("×".to_owned(), font, visuals.weak_text_color());
    let pad = 8.0;
    let cross_w = if removable { cross.size().x + 10.0 } else { 0.0 };
    let height = galley.size().y.max(cross.size().y) + 8.0;
    let size = egui::vec2(galley.size().x + 2.0 * pad + cross_w, height);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let title_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() - cross_w, rect.height()));
    let cross_rect = egui::Rect::from_min_max(egui::pos2(title_rect.right(), rect.top()), rect.max);
    let pick = ui.interact(title_rect, ui.auto_id_with(("chip", title)), egui::Sense::click());
    let name = texts.get_args(
        TextId::FwColumnChip,
        &[("name", Arg::Str(title.to_owned().into()))],
    );
    pick.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, name.clone()));
    let fill = if on {
        visuals.selection.bg_fill
    } else if pick.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        visuals.panel_fill
    };
    let stroke = if on {
        egui::Stroke::new(1.0, visuals.selection.stroke.color)
    } else {
        visuals.widgets.noninteractive.bg_stroke
    };
    ui.painter()
        .rect(rect, 7.0, fill, stroke, egui::StrokeKind::Inside);
    ui.painter().galley(
        egui::pos2(title_rect.left() + pad, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    let mut remove = false;
    if removable {
        let x = ui.interact(
            cross_rect,
            ui.auto_id_with(("chip-x", title)),
            egui::Sense::click(),
        );
        let name = texts.get_args(
            TextId::FwRemoveColumn,
            &[("name", Arg::Str(title.to_owned().into()))],
        );
        x.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name.clone()));
        let cross_ink = if x.hovered() {
            visuals.strong_text_color()
        } else {
            visuals.weak_text_color()
        };
        ui.painter().galley(
            egui::pos2(cross_rect.left() + 1.0, rect.center().y - cross.size().y / 2.0),
            cross,
            cross_ink,
        );
        remove = x.on_hover_text(name).clicked();
    }
    (pick.clicked(), remove)
}

/// Lignes : hauteur et fond (H-UI-227).
fn rows(ui: &mut egui::Ui, texts: &Texts, input: &FormatInput<'_>, actions: &mut Vec<Action>) {
    heading(ui, texts.get(TextId::FwRows));
    let ctx = &input.ctx;
    setting_row(ui, texts.get(TextId::FwRowHeight), |ui| {
        let options = [
            (Density::Compact, texts.get(TextId::FwDensityCompact)),
            (Density::Normal, texts.get(TextId::FwDensityNormal)),
            (Density::Airy, texts.get(TextId::FwDensityAiry)),
        ];
        if let Some(d) = segmented(ui, Some(ctx.style.density), &options)
            && let Some(b) = commands::density(ctx, d)
        {
            actions.push(Action::Exec(b));
        }
    });
    setting_row(ui, texts.get(TextId::FwRowFill), |ui| {
        let options = [
            (RowBanding::None, texts.get(TextId::FwBandNone)),
            (RowBanding::Zebra, texts.get(TextId::FwBandZebra)),
            (RowBanding::Phase, texts.get(TextId::FwBandPhase)),
        ];
        if let Some(rows) = segmented(ui, Some(ctx.style.grid.rows), &options)
            && let Some(b) = commands::banding(ctx, rows)
        {
            actions.push(Action::Exec(b));
        }
    });
}

/// Mise en forme des tâches sélectionnées (H-UI-228) : les opérations de
/// l'onglet Tâche du ruban, sur la sélection en cours.
fn selected_tasks(ui: &mut egui::Ui, texts: &Texts, input: &FormatInput<'_>, actions: &mut Vec<Action>) {
    heading(ui, texts.get(TextId::FwSelectedTasks));
    let any = input.selected > 0 && input.active.is_some();
    let look = input
        .active
        .and_then(|uid| input.ctx.style.task_text.get(&uid.get()).copied())
        .unwrap_or_default();
    ui.add_enabled_ui(any, |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(
                    look.bold,
                    egui::RichText::new(texts.get(TextId::RibbonBold)).strong(),
                )
                .clicked()
            {
                actions.push(Action::TextStyle(TextStyleOp::ToggleBold));
            }
            if ui
                .selectable_label(
                    look.italic,
                    egui::RichText::new(texts.get(TextId::RibbonItalic)).italics(),
                )
                .clicked()
            {
                actions.push(Action::TextStyle(TextStyleOp::ToggleItalic));
            }
            color_menu(ui, texts, false, look.color, actions);
            color_menu(ui, texts, true, look.fill, actions);
            if ui.button(texts.get(TextId::RibbonClearFormatting)).clicked() {
                actions.push(Action::TextStyle(TextStyleOp::Clear));
            }
        });
    });
    let line = if any {
        texts.get_args(
            TextId::FwSelectedCount,
            &[(
                "count",
                Arg::Count(i64::try_from(input.selected).unwrap_or(i64::MAX)),
            )],
        )
    } else {
        texts.get(TextId::FwSelectedNone)
    };
    hint(ui, line);
    hint(ui, texts.get(TextId::FwSelectedHint));
}

/// Menu d'une couleur du texte (`fill` faux) ou du surlignage.
fn color_menu(
    ui: &mut egui::Ui,
    texts: &Texts,
    fill: bool,
    current: Option<plannercraft_render::style::Rgb>,
    actions: &mut Vec<Action>,
) {
    let (label, none) = if fill {
        (TextId::FwHighlight, TextId::TextHighlightNone)
    } else {
        (TextId::FwTextColor, TextId::TextColorAuto)
    };
    let op = |c| {
        if fill {
            TextStyleOp::Fill(c)
        } else {
            TextStyleOp::Color(c)
        }
    };
    ui.menu_button(texts.get(label), |ui| {
        if ui.button(texts.get(none)).clicked() {
            actions.push(Action::TextStyle(op(None)));
            ui.close();
        }
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(6.0 * 26.0);
            for rgb in SWATCHES {
                if super::swatch(ui, texts, TextId::FpSwatch, rgb, current == Some(rgb)) {
                    actions.push(Action::TextStyle(op(Some(rgb))));
                    ui.close();
                }
            }
        });
    });
}

/// Phrase d'aide, petite et atténuée, repliée à la largeur.
fn hint(ui: &mut egui::Ui, text: String) {
    ui.add(egui::Label::new(egui::RichText::new(text).weak().small()).wrap());
}
