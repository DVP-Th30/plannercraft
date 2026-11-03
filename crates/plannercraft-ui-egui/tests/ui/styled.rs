//! Écran stylé (J8, lot L11, ADR-0016 D5) : la table et le Gantt lisent le
//! style effectif du document (modèle par défaut des préférences sinon,
//! H-UI-43) et la géométrie partagée avec le SVG et le PDF (P55, test
//! unitaire de `view::gantt`). Classique reproduit J7 au pixel près
//! (`classique_screen_identical_to_j7`, et toutes les captures J7).

use egui::{Color32, Pos2, Rect, Shape};
use egui_kittest::Harness;
use plannercraft_automation::Batch;
use plannercraft_render::style::{BarRole, Rgb, TemplateId, Variant, template};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::rows::RowKind;
use plannercraft_ui_egui::state::TaskViewState;
use plannercraft_ui_egui::theme::Palette;
use plannercraft_ui_egui::{Action, App, EditOrigin, Env, LanguagePref, ThemePref, TimeScaleZoom, UiState};

use crate::common::{harness_with, snapshot};

/// Exemple ouvert, modèle par défaut `template`, thème clair, zoom
/// Semaine, Gantt au début de la plage.
fn demo(template: TemplateId) -> Harness<'static, App> {
    let state = UiState {
        language: LanguagePref::Fr,
        theme: ThemePref::Light,
        default_template: template,
        ..UiState::default()
    };
    let mut harness = harness_with(App::new(state, Env::fixed(crate::now())), ThemePref::Light);
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    harness
}

/// Zoom Semaine, Gantt au début de la plage.
fn week(harness: &mut Harness<'static, App>) {
    harness.state_mut().dispatch(Action::SetZoom(TimeScaleZoom::Week));
    harness.run();
    harness.state_mut().dispatch(Action::ScrollGanttX(-1.0e6));
    harness.run();
}

/// Applique un lot JSON comme une saisie.
fn exec(harness: &mut Harness<'static, App>, ops: &str) {
    let batch = Batch::from_json_str(&format!(r#"{{"format_version":1,"ops":[{ops}]}}"#)).unwrap();
    harness.state_mut().execute(&batch, EditOrigin::User).unwrap();
    harness.run();
}

/// Couleur egui d'une couleur du style.
const fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Formes de la dernière image.
fn shapes(harness: &Harness<'static, App>) -> Vec<Shape> {
    harness.output().shapes.iter().map(|c| c.shape.clone()).collect()
}

/// Rectangles pleins de couleur `color`.
fn filled(harness: &Harness<'static, App>, color: Color32) -> Vec<Rect> {
    shapes(harness)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(r) if r.fill == color => Some(r.rect),
            _ => None,
        })
        .collect()
}

/// Rectangles en contour seul (sans remplissage) de trait `color`.
fn outlined(harness: &Harness<'static, App>, color: Color32) -> Vec<Rect> {
    shapes(harness)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(r) if r.fill == Color32::TRANSPARENT && r.stroke.color == color => Some(r.rect),
            _ => None,
        })
        .collect()
}

/// Segments de couleur `color`.
fn segments(harness: &Harness<'static, App>, color: Color32) -> Vec<[Pos2; 2]> {
    shapes(harness)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::LineSegment { points, stroke } if stroke.color == color => Some(points),
            _ => None,
        })
        .collect()
}

/// Textes dessinés (galleys), avec leur position.
fn texts(harness: &Harness<'static, App>) -> Vec<(Pos2, String)> {
    shapes(harness)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Text(t) => Some((t.pos, t.galley.text().to_owned())),
            _ => None,
        })
        .collect()
}

/// UID de la première feuille de l'exemple.
fn first_leaf(app: &App) -> u32 {
    let rows = app.rows().unwrap();
    (0..rows.len())
        .filter_map(|i| rows.row(i))
        .find(|r| r.kind == RowKind::Leaf)
        .unwrap()
        .uid
        .get()
}

/// Classique (modèle par défaut jusqu'au lot L15) reproduit J7 : palette,
/// densité, aucun texte autour des barres, et la capture `main_fr_light`
/// identique au pixel près (mode capture).
#[test]
fn classique_screen_identical_to_j7() {
    let classique = template(TemplateId::Classique);
    assert_eq!(Palette::from_style(&classique, Variant::Light), Palette::LIGHT);
    assert_eq!(Palette::from_style(&classique, Variant::Dark), Palette::DARK);
    let mut harness = demo(TemplateId::Classique);
    assert!((harness.state().row_h() - 22.0).abs() < f32::EPSILON);
    assert_ne!(filled(&harness, Palette::LIGHT.bar).len(), 0);
    let names: Vec<String> = {
        let rows = harness.state().rows().unwrap();
        (0..rows.len())
            .map(|i| rows.text(i, Column::Name).to_owned())
            .collect()
    };
    let table_right = f32::from(harness.state().state.table_width);
    // Les noms ne sont écrits que dans la table.
    for (pos, text) in texts(&harness) {
        if names.contains(&text) {
            assert!(pos.x < table_right, "« {text} » hors de la table : {pos:?}");
        }
    }
    // J8 lot L15 : « main_fr_light » est passée à Sobre (défaut) ; la
    // référence de Classique, identique à celle de J7, a son propre nom.
    snapshot(&mut harness, "classique_main_fr_light");
}

/// H-UI-43 à l'écran : un document sans style prend le modèle par défaut
/// des préférences (barres de Sobre, aucune barre de Classique).
#[test]
fn h_ui_43_default_template_on_screen() {
    let mut harness = demo(TemplateId::Sobre);
    week(&mut harness);
    let sobre = template(TemplateId::Sobre);
    let c = sobre.tokens(Variant::Light);
    let task = rgb(c.bars[&BarRole::Task].fill);
    assert_ne!(task, Palette::LIGHT.bar);
    assert_ne!(filled(&harness, task).len(), 0);
    assert_eq!(filled(&harness, Palette::LIGHT.bar).len(), 0);
    // Un style choisi dans le document l'emporte sur la préférence.
    exec(&mut harness, r#"{"op":"style.apply","template":"classique"}"#);
    assert_ne!(filled(&harness, Palette::LIGHT.bar).len(), 0);
    assert_eq!(filled(&harness, task).len(), 0);
}

/// H-UI-36 : colonne « Mode » (Manuelle / Automatique) et barre d'une
/// tâche manuelle en contour (rôle `Manual`).
#[test]
fn h_ui_36_mode_column() {
    let mut harness = demo(TemplateId::Classique);
    week(&mut harness);
    let uid = first_leaf(harness.state());
    let manual = rgb(
        template(TemplateId::Classique).tokens(Variant::Light).bars[&BarRole::Manual]
            .outline
            .unwrap(),
    );
    let before = outlined(&harness, manual).len();
    exec(
        &mut harness,
        &format!(r#"{{"op":"task.update","uid":{uid},"mode":"manual"}}"#),
    );
    // Colonne « Mode » affichée à la place des colonnes de dates.
    for column in [
        Column::Mode,
        Column::Duration,
        Column::Start,
        Column::Finish,
        Column::Predecessors,
    ] {
        assert!(harness.state_mut().state.toggle_column(column));
    }
    harness.run();
    let app = harness.state();
    let rows = app.rows().unwrap();
    let i = rows.index_of(plannercraft_core::TaskUid::new(uid)).unwrap();
    assert_eq!(rows.text(i, Column::Mode), "Manuelle");
    let other = (0..rows.len()).find(|&k| k != i && rows.row(k).is_some_and(|r| r.kind == RowKind::Leaf));
    assert_eq!(rows.text(other.unwrap(), Column::Mode), "Automatique");
    assert!(
        texts(&harness).iter().any(|(_, t)| t == "Manuelle"),
        "colonne non dessinée"
    );
    assert_eq!(
        outlined(&harness, manual).len(),
        before + 1,
        "barre manuelle en contour"
    );
}

/// H-UI-37 : tâche inactive grisée et barrée dans la table, barre du rôle
/// `Inactive` (contour atténué).
#[test]
fn h_ui_37_inactive_struck() {
    let mut harness = demo(TemplateId::Classique);
    week(&mut harness);
    let uid = first_leaf(harness.state());
    let weak = Palette::LIGHT.text_weak;
    let table_right = f32::from(harness.state().state.table_width);
    let struck = |h: &Harness<'static, App>| {
        segments(h, weak)
            .into_iter()
            .filter(|[a, b]| (a.y - b.y).abs() < 0.01 && b.x < table_right)
            .count()
    };
    assert_eq!(struck(&harness), 0);
    let bars_before = outlined(&harness, weak).len();
    exec(
        &mut harness,
        &format!(r#"{{"op":"task.update","uid":{uid},"active":false}}"#),
    );
    let app = harness.state();
    let i = app
        .rows()
        .unwrap()
        .index_of(plannercraft_core::TaskUid::new(uid))
        .unwrap();
    let row = app.rows().unwrap().row(i).unwrap();
    assert!(!row.active);
    assert_eq!(row.kind, RowKind::Excluded);
    assert!(struck(&harness) >= 1, "texte non barré");
    assert_eq!(outlined(&harness, weak).len(), bars_before + 1, "barre inactive");
}

/// H-UI-38 : sans surbrillance du chemin critique, aucune barre ni aucun
/// lien critique n'a la couleur critique.
#[test]
fn h_ui_38_highlight_off() {
    let mut harness = demo(TemplateId::Classique);
    week(&mut harness);
    let critical = Palette::LIGHT.critical;
    assert!(
        !filled(&harness, critical).is_empty(),
        "exemple sans critique visible"
    );
    let state = TaskViewState {
        highlight_critical: false,
        ..harness.state().task_view().clone()
    };
    harness.state_mut().set_task_view(state);
    harness.run();
    assert_eq!(filled(&harness, critical).len(), 0);
    let paths = shapes(&harness).into_iter().filter(|s| match s {
        Shape::Path(p) => p.stroke.color == egui::epaint::ColorMode::Solid(critical) || p.fill == critical,
        _ => false,
    });
    assert_eq!(paths.count(), 0, "lien critique encore rouge");
}

/// Densité : hauteur de ligne de la table et du Gantt (Aérée 28 px,
/// Compacte 18 px), sélection comprise (passe UX, lot L5 : une cellule
/// seule n'est pas teintée, son ID appuyé et sa ligne du Gantt prennent
/// `header_selected`, H-UI-103).
#[test]
fn styled_density_row_height() {
    for (id, expected) in [(TemplateId::Presentation, 28.0), (TemplateId::Chantier, 18.0)] {
        let mut harness = demo(id);
        assert!(
            (harness.state().row_h() - expected).abs() < f32::EPSILON,
            "{id:?}"
        );
        let uid = first_leaf(harness.state());
        harness
            .state_mut()
            .dispatch(Action::Select(Some(plannercraft_core::TaskUid::new(uid))));
        harness.run();
        let selection = Palette::from_style(&template(id), Variant::Light).header_selected;
        let rows = filled(&harness, selection);
        assert_ne!(rows.len(), 0, "{id:?}");
        for r in rows {
            assert!((r.height() - expected).abs() < 0.01, "{id:?} : {r:?}");
        }
    }
}

/// Hachures (Chantier : critique hachuré, H-UI-44) : segments obliques
/// dessinés dans les barres critiques seulement.
#[test]
fn styled_hatch_inside_bar() {
    let mut harness = demo(TemplateId::Chantier);
    week(&mut harness);
    let chantier = template(TemplateId::Chantier);
    let c = chantier.tokens(Variant::Light);
    let paint = c.bars[&BarRole::Critical];
    let fill = rgb(paint.fill);
    let ink = rgb(paint.outline.unwrap_or(c.bg));
    let mut bars = filled(&harness, fill);
    assert_ne!(bars.len(), 0);
    // Losanges des jalons critiques (polygones pleins).
    bars.extend(shapes(&harness).into_iter().filter_map(|shape| match shape {
        Shape::Path(p) if p.fill == fill => Some(Rect::from_points(&p.points)),
        _ => None,
    }));
    let hatch: Vec<_> = segments(&harness, ink)
        .into_iter()
        .filter(|[a, b]| (a.x - b.x).abs() > 0.01 && (a.y - b.y).abs() > 0.01)
        .collect();
    assert_ne!(hatch.len(), 0, "aucune hachure");
    for [a, b] in hatch {
        assert!(
            bars.iter()
                .any(|r| r.expand(0.01).contains(a) && r.expand(0.01).contains(b)),
            "hachure hors d'une barre critique : {a:?} {b:?}"
        );
    }
}

/// H-UI-53 : bandes de phase (Présentation) dans la table, de la couleur
/// de bande du style ; jamais dans le Gantt (H-UI-62, J8 lot L15 :
/// quadrillage vertical seul).
#[test]
fn h_ui_53_bands_on_screen() {
    let id = TemplateId::Presentation;
    let mut harness = demo(id);
    week(&mut harness);
    let band = rgb(template(id).tokens(Variant::Light).band[1]);
    let rects = filled(&harness, band);
    let table_right = f32::from(harness.state().state.table_width);
    assert!(rects.iter().any(|r| r.left() < table_right), "{id:?} : table");
    assert!(
        !rects.iter().any(|r| r.right() > table_right + 10.0),
        "{id:?} : bande dans le Gantt"
    );
}

/// H-UI-54 : étiquette « État JJ/MM » dans l'échelle et trait d'aujourd'hui
/// pointillé (Sobre) ; aucun des deux en Classique. Exemple : date d'état
/// ven 15/01/2027 17:00, « maintenant » le même jour à 08:00.
#[test]
fn h_ui_54_status_label_and_today() {
    // Hors de la chronologie (lot L5 : visible par défaut, elle porte sa
    // propre étiquette de date d'état, H-UI-78).
    let label = |h: &Harness<'static, App>| {
        // Galley posée jusqu'à un point au-dessus de la bande haute.
        let frise = h
            .state()
            .view()
            .timeline
            .drawn
            .as_ref()
            .map(|d| d.rect.expand(2.0));
        texts(h)
            .iter()
            .any(|(p, t)| t == "État 15/01" && !frise.is_some_and(|r| r.contains(*p)))
    };
    let today = |id: TemplateId| rgb(template(id).tokens(Variant::Light).today_line);
    let mut classique = demo(TemplateId::Classique);
    week(&mut classique);
    assert!(!label(&classique));
    let mut sobre = demo(TemplateId::Sobre);
    week(&mut sobre);
    // Gantt défilé jusqu'à la date d'état.
    let x = {
        let doc = sobre.state().document.as_ref().unwrap();
        let axis = plannercraft_ui_egui::timescale::TimeAxis::new(doc.range, TimeScaleZoom::Week, 0.0);
        axis.x(doc.project().effective_status_date()) - 200.0
    };
    sobre.state_mut().dispatch(Action::ScrollGanttX(x.max(0.0)));
    sobre.run();
    assert!(
        label(&sobre),
        "étiquette de la date d'état absente : {:?}",
        texts(&sobre)
    );
    let dashes = segments(&sobre, today(TemplateId::Sobre));
    assert_ne!(dashes.len(), 0, "trait d'aujourd'hui absent");
    assert!(
        dashes.iter().all(|[a, b]| (a.x - b.x).abs() < 0.01),
        "trait non vertical"
    );
}

/// H-UI-46 à l'écran : textes du moteur à droite des barres (Sobre : nom),
/// jamais dans la table.
#[test]
fn styled_bar_texts_from_engine() {
    let mut harness = demo(TemplateId::Sobre);
    week(&mut harness);
    let uid = first_leaf(harness.state());
    let app = harness.state();
    let rows = app.rows().unwrap();
    let name = rows
        .text(
            rows.index_of(plannercraft_core::TaskUid::new(uid)).unwrap(),
            Column::Name,
        )
        .to_owned();
    let table_right = f32::from(app.state.table_width);
    let drawn: Vec<_> = texts(&harness).into_iter().filter(|(_, t)| *t == name).collect();
    assert!(drawn.len() >= 2, "« {name} » : {drawn:?}");
    assert!(
        drawn.iter().any(|(p, _)| p.x > table_right),
        "texte de barre absent"
    );
}
