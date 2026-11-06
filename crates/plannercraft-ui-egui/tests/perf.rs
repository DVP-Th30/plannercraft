//! Fluidité de la vue principale (ADR-0011, décision 12) : coût CPU d'une
//! image (construction egui par [`plannercraft_ui_egui::ui`] puis tessellation) sur
//! 5 000 tâches et 10 000 liens, fenêtre 1920×1080, 1 px par point, avec un
//! `egui::Context` nu (sans `Harness` : AccessKit n'est actif sur le desktop
//! qu'en présence d'un lecteur d'écran).
//!
//! - `frame_budget_5000_tasks` (ignoré, à lancer en release) : 3 zooms × 3
//!   positions de défilement (début, milieu, fin, par `Action::ScrollToRow`)
//!   × 30 images ; chaque image mesurée reçoit un cran de molette de 37 points
//!   au-dessus de la table, pour forcer la mise en page de nouvelles lignes
//!   (le cache de texte d'egui ne joue pas en faveur de la mesure) ; le Gantt
//!   est défilé horizontalement jusqu'aux barres des lignes visibles, pour
//!   que barres et liens soient dessinés à chaque position. Échec si
//!   une médiane atteint 16 ms ou un 95e centile 32 ms.
//! - `shapes_do_not_grow_with_task_count` (toujours exécuté, debug) : garde
//!   déterministe de la virtualisation, indépendante de la vitesse de la
//!   machine ; `frame_counts_equal_at_1000_and_5000_tasks` (J7 L1,
//!   toujours exécuté) : formes, sommets et galleys égaux à ± 5 % à 1 000
//!   et 5 000 tâches, curseur dessiné et déplacé au clavier.
//! - J7 L1 (ignorés, release) : `nav_down_held` (↓ sur 300 images, une
//!   touche par image), `nav_page_sweep` (Page bas jusqu'à la fin puis
//!   Ctrl+Origine), mêmes seuils que ci-dessus ; `edit_latency` (20
//!   validations de durée par Entrée sur des tâches réparties : image de la
//!   validation, qui recalcule et reconstruit le `RowModel`, plus l'image
//!   suivante), mesure publiée avec sa décomposition (recalcul,
//!   `RowModel::build`, reste) ; **porte** depuis le lot L16 : médiane
//!   < 100 ms, p95 < 150 ms. `edit_latency_baseline` (relecture finale J7) :
//!   même mesure, ligne de base 0 posée sur tout le réseau (écarts du Gantt
//!   de suivi recalculés à chaque reconstruction des lignes), même porte.
//! - J7 L3 (ignoré, release) : `approval_banner` (bandeau de validation
//!   d'une écriture d'un agent affiché, défilement à chaque image), mêmes
//!   seuils.
//! - J7 L8 (ignoré, release) : `bar_drag` (120 images de glisser d'une
//!   barre, un point par image, aperçu `gesture::preview` recalculé quand
//!   l'écart en jours change, aucun recalcul du planning), puis lâcher (un
//!   seul lot) ; mêmes seuils.
//! - J7 L11 : `usage_scroll` (ignoré, release) : utilisation des
//!   ressources, 1 200 ressources, 5 000 tâches affectées, défilement
//!   vertical (molette) puis horizontal (blocs neufs du moteur), mêmes
//!   seuils ; pire image (un bloc neuf) < 32 ms. Garde toujours exécutée :
//!   `usage_counts_equal_at_1000_and_5000_tasks` (formes, sommets et galleys
//!   de la vue à ± 5 %).
//!
//! - J8 L5 (ignorés, release) : `filter_sort_5000` (filtre « Critiques »,
//!   tri Début décroissant avec structure, retour à l'ordre des ID, 60
//!   images) et `outline_5000` (niveau 1, Tous, 2, puis repli de 50
//!   récapitulatives) : `RowView::build` médiane < 8 ms, images mêmes
//!   seuils.
//! - Finitions P3 (ignoré, release) : `group_by_5000` (40 lots, vue
//!   regroupée, molette, ↓, replis) : image médiane < 16 ms, p95 < 32 ms ;
//!   changement de regroupement < 150 ms.
//! - J8 L9 (ignoré, release) : `search_5000` (barre de recherche ouverte,
//!   10 caractères tapés, un par image, puis 20 « suivant ») : image
//!   médiane < 16 ms, p95 < 32 ms.
//!
//! - J8 L10 (ignoré, release) : `paste_1000` (1 000 lignes copiées par
//!   `Event::Copy`, collées par `Event::Paste` au milieu du réseau, cinq
//!   fois, annulées entre deux) : image du collage (lot, recalcul,
//!   `RowModel`) plus l'image suivante, décomposée ; budget médiane
//!   < 500 ms publié (non tenu, non bloquant), garde bloquante < 1 s.
//!
//! - J8 L11 : `static_styled` (ignoré, release) : vue principale stylée
//!   (Classique depuis L16, Sobre, Présentation en densité Aérée, Chantier
//!   en densité Compacte, clair et sombre), 5 000 tâches, molette au milieu du réseau, zoom
//!   Semaine, mêmes seuils. Garde toujours exécutée :
//!   `styled_counts_equal_at_1000_and_5000_tasks` (formes, sommets et
//!   galleys à ± 5 % pour chaque modèle).
//! - J8 L12 (ignoré, release) : `ai_panel` (volet d'opérations IA ouvert
//!   sur une proposition de 250 tâches décalées, diff affiché de 200
//!   lignes, défilement de la table à chaque image) : mêmes seuils ; le
//!   diff n'est calculé qu'une fois.
//!
//! - J8 L16 : modèle de la vue principale de **tous** les scénarios
//!   (sauf `static_styled`, qui fixe les siens) lu dans
//!   `PLANNERCRAFT_PERF_TEMPLATE` (`sobre` par défaut, défaut de l'application
//!   depuis L15 ; `classique` : apparence de J7). Le job CI `perf` lance la
//!   série deux fois, en Sobre puis en Classique, rapports dans deux
//!   dossiers. `static_styled` mesure aussi Classique.
//!
//! - Ruban (ADR-0017, lot L2, cahier § 2.5) : `ribbon_tabs` (ignoré,
//!   release) : ruban à onglets dessiné (seul habillage depuis L5), changement
//!   d'onglet à chaque image (5 onglets × 12) et pointeur déplacé sur les
//!   éléments ; mêmes seuils.
//! - Chronologie (ADR-0017, lot L4, cahier § 2.5) : **tous** les scénarios
//!   ouverts par `app_with` tournent ruban déplié et frise affichée, 50
//!   tâches du réseau sur la frise (lot L5 : l'ancienne barre d'outils et
//!   `PLANNERCRAFT_PERF_CHROME` sont retirées). `timeline_5000` (ignoré,
//!   release) : 200 tâches sur la frise ; `layout` seule sur 60 largeurs
//!   (médiane < 2 ms), puis 60 images à largeur de fenêtre changée à
//!   chaque image (une disposition par image) et 60 images de défilement
//!   horizontal du Gantt (rectangle de la partie visible suivi) ; images
//!   mêmes seuils.
//! - Assistant (ADR-0024, lot A1b) : `assistant_stream_5000` (ignoré,
//!   release) : réseau de 5 000 tâches ouvert, volet Assistant, réponse
//!   Markdown longue (≈ 9 Kio, gras, listes) reçue en flux, quatre morceaux
//!   par image, puis 60 images sur la réponse complète ; mêmes seuils (rendu
//!   Markdown analysé à chaque image, sans cache par message).
//!
//! Le rendu GPU n'est pas chronométré (lavapipe en CI n'est pas
//! représentatif). Le projet est construit directement
//! (`plannercraft_tests::generate::ui_network`) et ouvert sans MSPDI
//! (`App::open_project`, N8, J7 lot L14).
//!
//! Rapport (J7 lot L16) : chaque scénario mesuré écrit un fichier JSON
//! `<scénario>.json` (médiane, p95, max, nombre d'images, formes, sommets,
//! galleys, décomposition) dans `PLANNERCRAFT_PERF_REPORT_DIR`, sinon
//! `<CARGO_TARGET_DIR ou target>/tmp/ui-perf/` ; le job CI `perf` publie
//! ce dossier en artefact `ui-perf-report`. Une erreur d'écriture est
//! signalée sur stderr et ne fait pas échouer la mesure.
//!
//! `cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture`

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "mesure de temps dans un test : l'horloge est interdite dans la crate, pas dans ses tests"
)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::epaint::{ClippedPrimitive, Primitive, Shape};
use egui::{
    Event, Key, Modifiers, MouseWheelSource, MouseWheelUnit, Pos2, RawInput, Rect, TouchPhase, pos2, vec2,
};
use plannercraft_automation::filter::{FilterCtx, FilterPreset};
use plannercraft_core::Moment;
use plannercraft_render::style::TemplateId;
use plannercraft_tests::generate::{ui_network, ui_network_with_resources};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::edit::{EditState, commit_edit};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::row_view::RowView;
use plannercraft_ui_egui::rows::{RowKind, RowModel};
use plannercraft_ui_egui::state::{FilterChoice, SortKey, SortSpec, TaskViewState};
use plannercraft_ui_egui::timescale::TimeAxis;
use plannercraft_ui_egui::view::{OVERSCAN, ROW_H};
use plannercraft_ui_egui::{
    Action, App, DocOrigin, Env, LanguagePref, ThemePref, TimeScaleZoom, UiState, ViewKind,
};

/// Taille de la fenêtre simulée.
const SCREEN: (f32, f32) = (1920.0, 1080.0);
/// Images mesurées par série (zoom × position).
const FRAMES: usize = 30;
/// Images de chauffe avant chaque série.
const WARMUP: usize = 5;
/// Seuil d'échec de la médiane.
const BUDGET: Duration = Duration::from_millis(16);
/// Seuil d'échec du 95e centile.
const SPIKE: Duration = Duration::from_millis(32);
/// Cran de molette par image mesurée, en points.
const WHEEL_STEP: f32 = 37.0;

/// Récapitulatives du grand réseau.
const SUMMARIES: usize = 250;
/// Feuilles par récapitulative : 250 × (1 + 19) = 5 000 tâches.
const LEAVES_PER_SUMMARY: usize = 19;
/// Liens du grand réseau.
const LINKS: usize = 10_000;

/// « Maintenant » fixe (début du projet généré).
fn now() -> Moment {
    "2026-01-05T08:00".parse().unwrap()
}

/// Modèle de la mesure (`PLANNERCRAFT_PERF_TEMPLATE`, clé d'un modèle intégré :
/// `sobre` par défaut, `classique`…) ; une valeur inconnue fait échouer la
/// mesure plutôt que de mesurer autre chose.
fn perf_template() -> TemplateId {
    match std::env::var("PLANNERCRAFT_PERF_TEMPLATE") {
        Err(std::env::VarError::NotPresent) => TemplateId::Sobre,
        Ok(key) => TemplateId::ALL
            .into_iter()
            .find(|id| id.key() == key)
            .unwrap_or_else(|| panic!("PLANNERCRAFT_PERF_TEMPLATE inconnu : {key:?}")),
        Err(e) => panic!("PLANNERCRAFT_PERF_TEMPLATE illisible : {e}"),
    }
}

/// UID de `n` feuilles réparties sur tout le réseau (premières, dernières
/// et intermédiaires à pas constant).
fn spread_leaves(app: &App, n: usize) -> Vec<u32> {
    use plannercraft_core::TaskRole;
    let doc = app.document.as_ref().unwrap();
    let leaves: Vec<u32> = doc
        .project()
        .tasks
        .iter()
        .map(plannercraft_core::Task::uid)
        .filter(|uid| {
            doc.schedule()
                .scheduled(*uid)
                .is_some_and(|s| s.role == TaskRole::Leaf)
        })
        .map(plannercraft_core::TaskUid::get)
        .collect();
    let step = (leaves.len() / n.max(1)).max(1);
    let mut picked: Vec<u32> = leaves.iter().step_by(step).take(n).copied().collect();
    picked.sort_unstable();
    picked
}

/// Pose `n` tâches réparties sur la frise (un lot `style.apply`, modèle
/// par défaut de l'application gardé) ; appliqué à l'image suivante.
fn put_on_timeline(app: &mut App, n: usize) {
    let tasks = spread_leaves(app, n);
    let json = serde_json::json!({
        "format_version": 1,
        "ops": [{
            "op": "style.apply",
            "template": app.state.default_template.key(),
            "patch": { "timeline": { "tasks": tasks } }
        }]
    });
    let batch = plannercraft_automation::Batch::from_json_str(&json.to_string()).unwrap();
    app.dispatch(Action::Exec(batch));
}

/// Habillage de la mesure (ADR-0017, cahier § 2.5) : ruban déplié (seul
/// habillage) et 50 tâches sur la frise.
fn decorate(app: &mut App) {
    put_on_timeline(app, 50);
}

/// Gardes de comptage (formes, sommets, galleys égaux à ± 5 % à 1 000 et
/// 5 000 tâches) : ruban gardé, frise masquée. Son contenu est borné (200
/// tâches au plus) mais dépend de la durée du projet (graduations, noms
/// tronqués ou passés à droite), qui diffère entre les deux réseaux ; la
/// frise a sa propre garde (`timeline_cache_reused`) et sa mesure
/// (`timeline_5000`).
fn for_counts(mut app: App) -> App {
    app.state.timeline_visible = false;
    app
}

/// Application ouverte sur un réseau généré : français, thème clair, modèle
/// de [`perf_template`], habillage de [`decorate`].
fn app_with(summaries: usize, leaves: usize, max_links: usize) -> App {
    let mut app = bare_app_with(summaries, leaves, max_links);
    decorate(&mut app);
    app
}

/// Comme [`app_with`], sans habillage.
fn bare_app_with(summaries: usize, leaves: usize, max_links: usize) -> App {
    let state = UiState {
        language: LanguagePref::Fr,
        theme: ThemePref::Light,
        default_template: perf_template(),
        ..UiState::default()
    };
    let mut app = App::new(state, Env::fixed(now()));
    let origin = DocOrigin {
        name: "mesure.xml".to_owned(),
        path: None,
        demo: false,
    };
    app.open_project(ui_network(summaries, leaves, max_links), origin);
    let document = app.document.as_ref().unwrap_or_else(|| {
        panic!("réseau généré refusé : {:?}", app.messages());
    });
    assert_eq!(document.project().tasks.len(), summaries * (leaves + 1));
    assert!(
        document.schedule().diagnostics.is_empty(),
        "{:?}",
        document.schedule().diagnostics.first()
    );
    app
}

/// Relevé d'une image indépendant de la machine : formes de la vue
/// principale, sommets tessellés, galleys (textes) émis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counts {
    shapes: usize,
    vertices: usize,
    galleys: usize,
}

/// Textes d'une forme (récursif dans les vecteurs de formes).
fn galleys_in(shape: &Shape) -> usize {
    match shape {
        Shape::Text(_) => 1,
        Shape::Vec(shapes) => shapes.iter().map(galleys_in).sum(),
        _ => 0,
    }
}

/// Sommets des maillages tessellés.
fn vertices_in(primitives: &[ClippedPrimitive]) -> usize {
    primitives
        .iter()
        .map(|p| match &p.primitive {
            Primitive::Mesh(mesh) => mesh.vertices.len(),
            Primitive::Callback(_) => 0,
        })
        .sum()
}

/// Contexte egui nu et horloge simulée des images.
struct Bench {
    ctx: egui::Context,
    time: f64,
    /// Largeur de la fenêtre simulée.
    width: f32,
    /// Relevé de la dernière image.
    last: Counts,
}

impl Bench {
    fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            time: 0.0,
            width: SCREEN.0,
            last: Counts::default(),
        }
    }

    /// Une image ; rend la durée de `run_ui` + `tessellate`.
    fn frame(&mut self, app: &mut App, events: Vec<Event>) -> Duration {
        self.time += 1.0 / 60.0;
        // Modificateurs de l'image : ceux de la touche, annoncés d'abord.
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut events = events;
        events.insert(0, Event::ModifiersChanged(modifiers));
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(self.width, SCREEN.1))),
            time: Some(self.time),
            events,
            ..RawInput::default()
        };
        let t0 = Instant::now();
        let mut output = self.ctx.run_ui(raw, |ui| plannercraft_ui_egui::ui(ui, app));
        let shapes = std::mem::take(&mut output.shapes);
        let galleys = shapes.iter().map(|c| galleys_in(&c.shape)).sum();
        let primitives = self.ctx.tessellate(shapes, output.pixels_per_point);
        let elapsed = t0.elapsed();
        self.last = Counts {
            shapes: app.last_frame.shapes,
            vertices: vertices_in(&primitives),
            galleys,
        };
        black_box(primitives);
        // Aucun moteur de rendu : les textures sont ignorées.
        output.textures_delta.clear();
        elapsed
    }

    /// Images sans entrée jusqu'à stabilité de la plage de lignes peintes.
    fn settle(&mut self, app: &mut App) {
        for _ in 0..12 {
            self.frame(app, Vec::new());
        }
    }
}

/// Défile le Gantt pour que la barre de la ligne `row` apparaisse près du
/// bord gauche (abscisse donnée par l'axe de l'interface : temps → pixels).
fn scroll_gantt_to_row(app: &mut App, row: usize) {
    let doc = app.document.as_ref().unwrap();
    let start = app.rows().unwrap().row(row).unwrap().bar.unwrap().0;
    let axis = TimeAxis::new(doc.range, app.state.zoom, 0.0);
    let target = (axis.x(start) - 200.0).max(0.0);
    let delta = target - app.view().gantt_scroll_x;
    app.dispatch(Action::ScrollGanttX(delta));
}

/// Cran de molette au-dessus de la table (pointeur au milieu du corps).
fn wheel_events(dy: f32) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos2(300.0, 540.0)),
        Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, dy),
            modifiers: Modifiers::NONE,
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
        },
    ]
}

/// Centile `p` (0 à 100) d'une série triée.
fn percentile(sorted: &[Duration], p: usize) -> Duration {
    sorted[(sorted.len() - 1) * p / 100]
}

/// Millisecondes, pour l'affichage.
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

/// Dossier du rapport JSON : `PLANNERCRAFT_PERF_REPORT_DIR`, sinon
/// `<CARGO_TARGET_DIR ou target>/tmp/ui-perf`.
fn report_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("PLANNERCRAFT_PERF_REPORT_DIR") {
        return PathBuf::from(dir);
    }
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"),
        PathBuf::from,
    );
    target.join("tmp").join("ui-perf")
}

/// Écrit le rapport JSON du scénario `label` (J7 lot L16) : `samples`
/// triés, relevé de formes, `extra` (décomposition, seuils).
fn report(label: &str, sorted: &[Duration], counts: Counts, extra: serde_json::Value) {
    let mut value = serde_json::json!({
        "scenario": label,
        "frames": sorted.len(),
        "median_ms": ms(percentile(sorted, 50)),
        "p95_ms": ms(percentile(sorted, 95)),
        "max_ms": sorted.last().map_or(0.0, |d| ms(*d)),
        "shapes": counts.shapes,
        "vertices": counts.vertices,
        "galleys": counts.galleys,
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
    });
    if let (Some(into), serde_json::Value::Object(extra)) = (value.as_object_mut(), extra) {
        into.extend(extra);
    }
    let dir = report_dir();
    let file = dir.join(format!("{}.json", label.replace(' ', "_")));
    let written = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, format!("{value:#}\n")));
    if let Err(error) = written {
        eprintln!("perf UI : rapport {} non écrit : {error}", file.display());
    }
}

#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn frame_budget_5000_tasks() {
    let open = Instant::now();
    let mut app = app_with(SUMMARIES, LEAVES_PER_SUMMARY, LINKS);
    let links: usize = app
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .iter()
        .map(|t| t.predecessors.len())
        .sum();
    assert_eq!(links, LINKS);
    eprintln!(
        "perf UI : ouverture (lecture MSPDI, calcul, RowModel) {:.1} ms",
        ms(open.elapsed())
    );
    let len = app.rows().unwrap().len();
    let mut bench = Bench::new();
    bench.settle(&mut app);

    let mut worst_median = Duration::ZERO;
    let mut worst_p95 = Duration::ZERO;
    for zoom in [TimeScaleZoom::Day, TimeScaleZoom::Week, TimeScaleZoom::Quarter] {
        app.dispatch(Action::SetZoom(zoom));
        bench.settle(&mut app);
        assert_eq!(app.state.zoom, zoom);
        let mut all = Vec::with_capacity(3 * FRAMES);
        for (label, row, dy) in [
            ("début", 0, -WHEEL_STEP),
            ("milieu", len / 2, -WHEEL_STEP),
            ("fin", len - 1, WHEEL_STEP),
        ] {
            app.dispatch(Action::ScrollToRow(row));
            bench.settle(&mut app);
            scroll_gantt_to_row(&mut app, row);
            bench.settle(&mut app);
            let painted = app.last_frame.table_rows.clone();
            assert!(
                painted.contains(&row.min(len - 1)) || (row == len - 1 && painted.end == len),
                "{label} : ligne {row} non peinte ({painted:?})"
            );
            for _ in 0..WARMUP {
                bench.frame(&mut app, wheel_events(dy));
            }
            let before = app.last_frame.table_rows.start;
            let mut samples: Vec<Duration> = (0..FRAMES)
                .map(|_| bench.frame(&mut app, wheel_events(dy)))
                .collect();
            let after = app.last_frame.table_rows.start;
            assert_ne!(before, after, "{label} : la molette n'a pas fait défiler");
            all.extend_from_slice(&samples);
            samples.sort();
            let (median, p95) = (percentile(&samples, 50), percentile(&samples, 95));
            eprintln!(
                "perf UI {zoom:?} {label:<6} : médiane {:6.2} ms, p95 {:6.2} ms, max {:6.2} ms, \
                 formes {}, liens {}, lignes {:?}",
                ms(median),
                ms(p95),
                ms(*samples.last().unwrap()),
                app.last_frame.shapes,
                app.last_frame.links_painted,
                app.last_frame.table_rows,
            );
            worst_median = worst_median.max(median);
            worst_p95 = worst_p95.max(p95);
        }
        all.sort();
        eprintln!(
            "perf UI {zoom:?} (3 positions × {FRAMES} images) : médiane {:.2} ms, p95 {:.2} ms",
            ms(percentile(&all, 50)),
            ms(percentile(&all, 95)),
        );
        report(
            &format!("frame_budget_{}", format!("{zoom:?}").to_lowercase()),
            &all,
            bench.last,
            serde_json::json!({ "budget_median_ms": BUDGET.as_millis(), "budget_p95_ms": SPIKE.as_millis() }),
        );
    }
    eprintln!(
        "perf UI : pire médiane {:.2} ms (seuil {} ms), pire p95 {:.2} ms (seuil {} ms)",
        ms(worst_median),
        BUDGET.as_millis(),
        ms(worst_p95),
        SPIKE.as_millis()
    );
    assert!(
        worst_median < BUDGET,
        "pire médiane {worst_median:?} ≥ {BUDGET:?}"
    );
    assert!(worst_p95 < SPIKE, "pire p95 {worst_p95:?} ≥ {SPIKE:?}");
}

/// Formes et lignes peintes de la vue principale après stabilisation, au
/// zoom Semaine, défilée à la ligne `row`.
fn painted(app: &mut App, row: usize) -> (usize, std::ops::Range<usize>) {
    let mut bench = Bench::new();
    bench.settle(app);
    app.dispatch(Action::SetZoom(TimeScaleZoom::Week));
    app.dispatch(Action::ScrollToRow(row));
    bench.settle(app);
    assert_eq!(app.state.zoom, TimeScaleZoom::Week);
    (app.last_frame.shapes, app.last_frame.table_rows.clone())
}

#[test]
fn shapes_do_not_grow_with_task_count() {
    // 5 × 20 = 100 tâches, 2 liens par tâche au plus ; 5 000 tâches, 10 000 liens.
    let mut small = app_with(5, LEAVES_PER_SUMMARY, 200);
    let mut large = app_with(SUMMARIES, LEAVES_PER_SUMMARY, LINKS);
    let (small_shapes, small_rows) = painted(&mut small, 0);
    let len = large.rows().unwrap().len();
    // Plafond de lignes peintes : fenêtre visible + débordement des deux côtés.
    let max_rows = (SCREEN.1 / ROW_H).ceil() as usize + 2 * OVERSCAN + 1;
    assert!(small_shapes > 0, "aucune forme à 100 tâches");
    for row in [0, len / 2, len - 1] {
        let (shapes, rows) = painted(&mut large, row);
        eprintln!(
            "formes : 100 tâches {small_shapes} ({small_rows:?}), 5 000 tâches ligne {row} : {shapes} ({rows:?})"
        );
        assert!(
            shapes <= 2 * small_shapes,
            "ligne {row} : {shapes} formes à 5 000 tâches > 2 × {small_shapes} à 100 tâches"
        );
        assert!(
            rows.len() <= max_rows,
            "ligne {row} : {} lignes peintes > {max_rows}",
            rows.len()
        );
    }
}

/// Appui (et relâchement) d'une touche.
fn key_events(key: Key, modifiers: Modifiers) -> Vec<Event> {
    [true, false]
        .into_iter()
        .map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .collect()
}

/// Indice de la ligne du curseur (ligne vide : nombre de tâches).
fn cursor_row(app: &App) -> usize {
    let rows = app.rows().unwrap();
    match app.view().cursor.row {
        RowRef::Item(uid) => rows.index_of(uid).unwrap(),
        RowRef::Blank => rows.len(),
    }
}

/// Médiane et 95e centile d'une série (triée ici).
fn stats(mut samples: Vec<Duration>) -> (Duration, Duration, Duration) {
    samples.sort();
    (
        percentile(&samples, 50),
        percentile(&samples, 95),
        *samples.last().unwrap(),
    )
}

/// Seuils de fluidité de la navigation (ADR-0015 D12) ; rapport JSON écrit
/// avant la vérification (un dépassement est publié aussi).
fn assert_budget(label: &str, mut samples: Vec<Duration>, counts: Counts) {
    let n = samples.len();
    samples.sort();
    let (median, p95, max) = (
        percentile(&samples, 50),
        percentile(&samples, 95),
        *samples.last().unwrap(),
    );
    eprintln!(
        "perf UI {label} ({n} images) : médiane {:.2} ms, p95 {:.2} ms, max {:.2} ms, {counts:?}",
        ms(median),
        ms(p95),
        ms(max),
    );
    report(
        label,
        &samples,
        counts,
        serde_json::json!({ "budget_median_ms": BUDGET.as_millis(), "budget_p95_ms": SPIKE.as_millis() }),
    );
    assert!(median < BUDGET, "{label} : médiane {median:?} ≥ {BUDGET:?}");
    assert!(p95 < SPIKE, "{label} : p95 {p95:?} ≥ {SPIKE:?}");
}

/// Réseau de 5 000 tâches ouvert, zoom Semaine, focus à la table (donné à
/// l'ouverture), images stabilisées.
fn nav_bench() -> (App, Bench) {
    let mut app = app_with(SUMMARIES, LEAVES_PER_SUMMARY, LINKS);
    let mut bench = Bench::new();
    bench.settle(&mut app);
    app.dispatch(Action::SetZoom(TimeScaleZoom::Week));
    bench.settle(&mut app);
    assert_eq!(cursor_row(&app), 0);
    (app, bench)
}

/// J7 L1 : ↓ maintenue, une touche par image sur 300 images.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn nav_down_held() {
    let (mut app, mut bench) = nav_bench();
    for _ in 0..WARMUP {
        bench.frame(&mut app, key_events(Key::ArrowDown, Modifiers::NONE));
    }
    let start = cursor_row(&app);
    let samples: Vec<Duration> = (0..300)
        .map(|_| bench.frame(&mut app, key_events(Key::ArrowDown, Modifiers::NONE)))
        .collect();
    assert_eq!(cursor_row(&app), start + 300, "une ligne par image");
    assert!(app.last_frame.table_rows.contains(&(start + 300)));
    assert_budget("nav_down_held", samples, bench.last);
}

/// J7 L1 : Page bas jusqu'à la fin, puis Ctrl+Origine.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn nav_page_sweep() {
    let (mut app, mut bench) = nav_bench();
    let blank = app.rows().unwrap().len();
    let mut samples = Vec::new();
    while cursor_row(&app) < blank {
        assert!(samples.len() < 1_000, "Page bas n'avance pas");
        samples.push(bench.frame(&mut app, key_events(Key::PageDown, Modifiers::NONE)));
    }
    samples.push(bench.frame(&mut app, key_events(Key::Home, Modifiers::COMMAND)));
    for _ in 0..3 {
        samples.push(bench.frame(&mut app, Vec::new()));
    }
    assert_eq!(cursor_row(&app), 0);
    assert_eq!(app.view().cursor.column, Column::Id);
    assert_eq!(app.last_frame.table_rows.start, 0);
    assert_budget("nav_page_sweep", samples, bench.last);
}

/// Budget de la latence d'édition (ADR-0015 D4, porte depuis le lot L16).
const EDIT_BUDGET: Duration = Duration::from_millis(100);
/// 95e centile de la latence d'édition.
const EDIT_SPIKE: Duration = Duration::from_millis(150);

/// J7 L1 : latence d'édition (ADR-0015 D4) : validation d'une durée par
/// Entrée → recalcul → `RowModel` → image suivante, sur 20 tâches
/// réparties. Décomposition par une copie de la session (même lot
/// appliqué, `RowModel::build` seul). **Porte** depuis le lot L16 :
/// médiane < 100 ms, p95 < 150 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn edit_latency() {
    measure_edit_latency("edit_latency", false);
}

/// Relecture finale J7 (performances, F5) : même latence d'édition avec une
/// ligne de base 0 posée sur tout le réseau (écarts du Gantt de suivi
/// recalculés par `RowModel::build` à chaque édition) ; même porte.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn edit_latency_baseline() {
    measure_edit_latency("edit_latency_baseline", true);
}

/// Pose la ligne de base 0 sur tout le projet (`baseline.set`).
fn set_baseline(app: &mut App, bench: &mut Bench) {
    let batch = plannercraft_automation::Batch::from_json(serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "baseline.set" }]
    }))
    .unwrap();
    app.execute(&batch, plannercraft_ui_egui::app::EditOrigin::User)
        .unwrap();
    bench.settle(app);
    let doc = app.document.as_ref().unwrap();
    assert!(doc.project().tasks.iter().all(|t| {
        t.baselines
            .contains_key(&plannercraft_core::BaselineNumber::PRIMARY)
    }));
}

/// Latence d'édition (voir [`edit_latency`]), ligne de base posée d'abord
/// si `baseline`.
fn measure_edit_latency(name: &str, baseline: bool) {
    let (mut app, mut bench) = nav_bench();
    if baseline {
        set_baseline(&mut app, &mut bench);
    }
    let labels = plannercraft_ui_egui::i18n::Texts::new(plannercraft_ui_egui::Language::Fr)
        .format_labels()
        .clone();
    // Colonne Durée : autant de crans à droite depuis l'ID que de colonnes
    // affichées avant elle (Indicateurs comprise depuis les finitions P1).
    let steps = app
        .state
        .visible_columns()
        .position(|c| c.column == Column::Duration)
        .unwrap();
    app.dispatch(Action::Nav {
        mv: plannercraft_ui_egui::nav::NavMove::RowStart,
        extend: false,
    });
    for _ in 0..steps {
        app.dispatch(Action::Nav {
            mv: plannercraft_ui_egui::nav::NavMove::Right,
            extend: false,
        });
    }
    bench.settle(&mut app);
    assert_eq!(app.view().cursor.column, Column::Duration);
    let len = app.rows().unwrap().len();
    let (mut total, mut apply, mut build) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..20 {
        let index = (7 + k * len / 20).min(len - 1);
        let row = app.rows().unwrap().row(index).unwrap();
        assert!(matches!(row.kind, RowKind::Leaf | RowKind::Milestone), "{index}");
        let uid = row.uid;
        app.dispatch(Action::ScrollToRow(index));
        select_rows(&mut app, uid, uid);
        bench.settle(&mut app);
        // Décomposition, hors image, sur une copie du document.
        let doc = app.document.as_ref().unwrap();
        let state = EditState::typed(Some(uid), Column::Duration, "3j");
        let batch = commit_edit(&state, &doc.session, plannercraft_automation::text::Language::Fr).unwrap();
        let mut copy = doc.clone();
        let t0 = Instant::now();
        copy.session
            .apply(&batch, plannercraft_automation::ApplyMode::Commit)
            .unwrap();
        apply.push(t0.elapsed());
        let t0 = Instant::now();
        black_box(RowModel::build(&copy, &labels));
        build.push(t0.elapsed());
        // Mesure : saisie ouverte par la frappe, puis Entrée et l'image
        // suivante.
        let generation = app.document.as_ref().unwrap().generation();
        bench.frame(&mut app, vec![Event::Text("3j".to_owned())]);
        assert!(app.view().edit.editing.is_some(), "saisie non ouverte ({index})");
        let enter = bench.frame(&mut app, key_events(Key::Enter, Modifiers::NONE));
        let next = bench.frame(&mut app, Vec::new());
        assert_eq!(app.document.as_ref().unwrap().generation(), generation + 1);
        assert!(app.messages().is_empty(), "{:?}", app.messages());
        total.push(enter + next);
    }
    total.sort();
    let (median, p95, max) = (
        percentile(&total, 50),
        percentile(&total, 95),
        *total.last().unwrap(),
    );
    let (apply_median, _, _) = stats(apply);
    let (build_median, _, _) = stats(build);
    let rest = (ms(median) - ms(apply_median) - ms(build_median)).max(0.0);
    eprintln!(
        "perf UI {name} (20 validations) : médiane {:.1} ms, p95 {:.1} ms, max {:.1} ms ; \
         décomposition (médianes) : recalcul (Session::apply) {:.1} ms, RowModel::build {:.1} ms, \
         reste (image, tessellation) {:.1} ms ; porte : médiane < {} ms, p95 < {} ms",
        ms(median),
        ms(p95),
        ms(max),
        ms(apply_median),
        ms(build_median),
        rest,
        EDIT_BUDGET.as_millis(),
        EDIT_SPIKE.as_millis(),
    );
    report(
        name,
        &total,
        bench.last,
        serde_json::json!({
            "apply_median_ms": ms(apply_median),
            "row_model_build_median_ms": ms(build_median),
            "rest_median_ms": rest,
            "budget_median_ms": EDIT_BUDGET.as_millis(),
            "budget_p95_ms": EDIT_SPIKE.as_millis(),
        }),
    );
    assert!(
        median < EDIT_BUDGET,
        "{name} : médiane {median:?} ≥ {EDIT_BUDGET:?}"
    );
    assert!(p95 < EDIT_SPIKE, "{name} : p95 {p95:?} ≥ {EDIT_SPIKE:?}");
}

/// Relevé stabilisé après `events` (une image par lot d'événements).
fn counts_after(app: &mut App, bench: &mut Bench, events: &[Vec<Event>]) -> Counts {
    for e in events {
        bench.frame(app, e.clone());
    }
    bench.settle(app);
    bench.last
}

/// J7 L1 (garde indépendante de la machine) : à 1 000 et 5 000 tâches,
/// défilées en haut au zoom Semaine, formes, sommets et galleys égaux à
/// ± 5 %, au départ puis après des déplacements du curseur au clavier, puis
/// toutes les colonnes affichées et la table défilée jusqu'à la dernière
/// (J7 L6).
#[test]
fn frame_counts_equal_at_1000_and_5000_tasks() {
    let scenarios: Vec<Vec<Vec<Event>>> = vec![
        Vec::new(),
        vec![
            key_events(Key::ArrowDown, Modifiers::NONE),
            key_events(Key::PageDown, Modifiers::NONE),
            key_events(Key::Tab, Modifiers::NONE),
            key_events(Key::PageDown, Modifiers::NONE),
            key_events(Key::ArrowDown, Modifiers::SHIFT),
        ],
    ];
    let mut measured = Vec::new();
    for summaries in [50, SUMMARIES] {
        let mut app = for_counts(app_with(
            summaries,
            LEAVES_PER_SUMMARY,
            2 * summaries * LEAVES_PER_SUMMARY,
        ));
        let mut bench = Bench::new();
        bench.settle(&mut app);
        app.dispatch(Action::SetZoom(TimeScaleZoom::Week));
        bench.settle(&mut app);
        let x = app.view().gantt_scroll_x;
        app.dispatch(Action::ScrollGanttX(-x));
        bench.settle(&mut app);
        let mut counts: Vec<Counts> = scenarios
            .iter()
            .map(|events| counts_after(&mut app, &mut bench, events))
            .collect();
        // J7 L6 : toutes les colonnes affichées (textes des colonnes de
        // suivi), table défilée horizontalement par Fin.
        for column in Column::ALL {
            if !app.state.shown(column) {
                app.dispatch(Action::ToggleColumn(column));
            }
        }
        bench.settle(&mut app);
        counts.push(counts_after(
            &mut app,
            &mut bench,
            &[key_events(Key::End, Modifiers::NONE)],
        ));
        assert!(app.view().table_scroll_x > 0.0, "table non défilée");
        eprintln!("{} tâches : {counts:?}", app.rows().unwrap().len());
        measured.push(counts);
    }
    let close = |a: usize, b: usize| a.abs_diff(b) * 20 <= a.max(b);
    for (small, large) in measured[0].iter().zip(&measured[1]) {
        assert!(
            small.shapes > 0 && small.galleys > 0 && small.vertices > 0,
            "{small:?}"
        );
        assert!(close(small.shapes, large.shapes), "formes {small:?} / {large:?}");
        assert!(
            close(small.vertices, large.vertices),
            "sommets {small:?} / {large:?}"
        );
        assert!(
            close(small.galleys, large.galleys),
            "galleys {small:?} / {large:?}"
        );
    }
}

/// J7 L3 : bandeau de validation d'une écriture d'un agent affiché
/// (H-UI-27), 5 000 tâches, défilement de la table à chaque image ; même
/// budget que la vue principale (médiane < 16 ms).
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn approval_banner() {
    use std::sync::Arc;
    use std::sync::mpsc::sync_channel;

    use plannercraft_automation::control::{CallState, RelayCall, RelayKind, Reply};

    let (mut app, mut bench) = nav_bench();
    let (tx, rx) = sync_channel(4);
    app.control_inbox(rx);
    let (reply, replies) = sync_channel(4);
    tx.send(RelayCall {
        id: 1,
        kind: RelayKind::Execute {
            tool: "task.update".to_owned(),
            params: serde_json::json!({ "uid": 3, "name": "Proposé par l'agent" }),
        },
        state: Arc::new(CallState::new()),
        reply,
    })
    .unwrap();
    bench.settle(&mut app);
    assert_eq!(replies.try_recv().unwrap(), Reply::PendingApproval { id: 1 });
    assert_eq!(app.relay().unwrap().pending().len(), 1);
    for _ in 0..WARMUP {
        bench.frame(&mut app, wheel_events(-WHEEL_STEP));
    }
    let samples: Vec<Duration> = (0..120)
        .map(|i| {
            let dy = if i % 2 == 0 { -WHEEL_STEP } else { WHEEL_STEP };
            bench.frame(&mut app, wheel_events(dy))
        })
        .collect();
    assert_eq!(
        app.relay().unwrap().pending().len(),
        1,
        "bandeau toujours affiché"
    );
    assert_budget("approval_banner", samples, bench.last);
}

/// Images de clic (appui puis relâchement) au point `p`.
fn click_frames(bench: &mut Bench, app: &mut App, p: Pos2) {
    bench.frame(app, vec![Event::PointerMoved(p)]);
    for pressed in [true, false] {
        bench.frame(
            app,
            vec![Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
        );
    }
}

/// Ligne sous l'ordonnée `y` du Gantt d'après la sélection posée par un
/// clic (abscisse `x`, sans barre ni lien supposés : seul l'indice compte).
fn row_clicked(bench: &mut Bench, app: &mut App, x: f32, y: f32) -> Option<usize> {
    click_frames(bench, app, pos2(x, y));
    let uid = app.selection?;
    app.rows().unwrap().index_of(uid)
}

/// J7 L8 : glisser d'une barre sur 120 images (ADR-0015 D12), réseau de
/// 5 000 tâches au zoom Semaine. Le haut de la ligne visée est retrouvé par
/// des clics (la sélection donne la ligne sous le pointeur), puis la barre
/// est saisie en son milieu (abscisse par `TimeAxis::x`).
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn bar_drag() {
    let (mut app, mut bench) = nav_bench();
    // Une colonne du Gantt loin des barres du début : x = bord gauche + 5.
    let probe_x = app.view().last_gantt_left + 5.0;
    let probe_row = row_clicked(&mut bench, &mut app, probe_x, 500.0).expect("aucune ligne à y = 500");
    // Haut de la ligne probe_row, au point près : plus petit y qui la donne encore.
    let (mut lo, mut hi) = (500.0 - ROW_H, 500.0_f32);
    while hi - lo > 0.5 {
        let mid = f32::midpoint(lo, hi);
        if row_clicked(&mut bench, &mut app, probe_x, mid) == Some(probe_row) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let top_k = hi;
    // Feuille non commencée près de probe_row, barre amenée à 200 points du bord.
    let row_index = (probe_row..probe_row + 10)
        .find(|&row_index| {
            let row = app.rows().unwrap().row(row_index).unwrap();
            row.kind == RowKind::Leaf && row.bar.is_some() && row.percent.get() == 0
        })
        .expect("aucune feuille près de la ligne visée");
    let uid = app.rows().unwrap().row(row_index).unwrap().uid;
    scroll_gantt_to_row(&mut app, row_index);
    bench.settle(&mut app);
    let y = top_k + (row_index - probe_row) as f32 * ROW_H + ROW_H / 2.0;
    let doc = app.document.as_ref().unwrap();
    let (start, finish) = app.rows().unwrap().row(row_index).unwrap().bar.unwrap();
    let axis = TimeAxis::new(
        doc.range,
        app.state.zoom,
        app.view().last_gantt_left - app.view().gantt_scroll_x,
    );
    let x = f32::midpoint(axis.x(start), axis.x(finish));
    let generation = doc.generation();
    bench.frame(&mut app, vec![Event::PointerMoved(pos2(x, y))]);
    bench.frame(
        &mut app,
        vec![Event::PointerButton {
            pos: pos2(x, y),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
    );
    // Au-delà du seuil de glisser d'egui (6 points).
    for i in 1..=WARMUP {
        bench.frame(&mut app, vec![Event::PointerMoved(pos2(x + 2.0 * i as f32, y))]);
    }
    let drag = app.view().drag.unwrap_or_else(|| {
        panic!(
            "glisser non commencé : ligne {row_index}, y {y}, x {x}, sélection {:?}",
            app.selection
        )
    });
    assert_eq!(drag.uid, uid);
    let samples: Vec<Duration> = (0..120)
        .map(|i| {
            let p = pos2(x + (2 * WARMUP + i) as f32, y);
            bench.frame(&mut app, vec![Event::PointerMoved(p)])
        })
        .collect();
    let doc = app.document.as_ref().unwrap();
    assert_eq!(doc.generation(), generation, "recalcul pendant le glisser");
    let days = app.view().drag.unwrap().days;
    assert!(days > 0, "écart nul après 125 points");
    let end = pos2(x + (2 * WARMUP + 120) as f32, y);
    bench.frame(
        &mut app,
        vec![Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    bench.settle(&mut app);
    assert!(app.view().drag.is_none());
    assert_eq!(
        app.document.as_ref().unwrap().generation(),
        generation + 1,
        "{:?}",
        app.messages()
    );
    assert_budget("bar_drag", samples, bench.last);
}

/// Ressources du réseau mesuré par `usage_scroll` (ADR-0015 § 0).
const USAGE_RESOURCES: usize = 1_200;

/// Application ouverte sur un réseau affecté, vue Utilisation des
/// ressources, zoom `zoom`, défilée au début.
fn usage_app(summaries: usize, resources: usize, zoom: TimeScaleZoom) -> (App, Bench) {
    let state = UiState {
        language: LanguagePref::Fr,
        theme: ThemePref::Light,
        ..UiState::default()
    };
    let mut app = App::new(state, Env::fixed(now()));
    let origin = DocOrigin {
        name: "utilisation.xml".to_owned(),
        path: None,
        demo: false,
    };
    let leaves = LEAVES_PER_SUMMARY;
    let project = ui_network_with_resources(summaries, leaves, 2 * summaries * leaves, resources);
    app.open_project(project, origin);
    let document = app
        .document
        .as_ref()
        .unwrap_or_else(|| panic!("réseau affecté refusé : {:?}", app.messages()));
    assert_eq!(document.project().resources.len(), resources);
    assert!(!document.schedule().assignments.is_empty());
    let mut bench = Bench::new();
    bench.settle(&mut app);
    app.dispatch(Action::SetView(ViewKind::ResourceUsage));
    app.dispatch(Action::SetZoom(zoom));
    bench.settle(&mut app);
    let x = app.view().gantt_scroll_x;
    app.dispatch(Action::ScrollGanttX(-x));
    bench.settle(&mut app);
    assert_eq!(app.state.view, ViewKind::ResourceUsage);
    (app, bench)
}

/// J7 L11 (garde indépendante de la machine) : utilisation des ressources
/// à 1 000 et 5 000 tâches (60 ressources, mêmes premières tâches), en haut
/// puis après des déplacements au clavier : formes, sommets et galleys
/// égaux à ± 5 % (virtualisation des lignes et des périodes).
#[test]
fn usage_counts_equal_at_1000_and_5000_tasks() {
    let moves = vec![
        key_events(Key::ArrowDown, Modifiers::NONE),
        key_events(Key::PageDown, Modifiers::NONE),
        key_events(Key::Tab, Modifiers::NONE),
    ];
    let mut measured = Vec::new();
    for summaries in [50, SUMMARIES] {
        let (mut app, mut bench) = usage_app(summaries, 60, TimeScaleZoom::Week);
        let start = bench.last;
        let after = counts_after(&mut app, &mut bench, &moves);
        let cells = app.view().usage.last_cells.len();
        eprintln!("utilisation, {summaries} récapitulatives : {start:?} {after:?}, {cells} cellules");
        assert!(cells > 0, "aucune cellule");
        measured.push([start, after]);
    }
    let close = |a: usize, b: usize| a.abs_diff(b) * 20 <= a.max(b);
    for (small, large) in measured[0].iter().zip(&measured[1]) {
        assert!(small.shapes > 0 && small.galleys > 0, "{small:?}");
        assert!(close(small.shapes, large.shapes), "formes {small:?} / {large:?}");
        assert!(
            close(small.vertices, large.vertices),
            "sommets {small:?} / {large:?}"
        );
        assert!(
            close(small.galleys, large.galleys),
            "galleys {small:?} / {large:?}"
        );
    }
}

/// J7 L11 : utilisation des ressources, 1 200 ressources, 5 000 tâches
/// affectées ; molette verticale (120 images) puis défilement horizontal au
/// zoom Jour (120 images, un bloc neuf toutes les ~8 images) ; médiane
/// < 16 ms, p95 < 32 ms, pire image (bloc neuf) < 32 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn usage_scroll() {
    let (mut app, mut bench) = usage_app(SUMMARIES, USAGE_RESOURCES, TimeScaleZoom::Week);
    for _ in 0..WARMUP {
        bench.frame(&mut app, wheel_events(-WHEEL_STEP));
    }
    let first = app.view().usage.last_scroll_y;
    let queries = app.view().usage.cache.queries();
    let vertical: Vec<Duration> = (0..120)
        .map(|_| bench.frame(&mut app, wheel_events(-WHEEL_STEP)))
        .collect();
    assert!(
        app.view().usage.last_scroll_y > first,
        "la molette n'a pas fait défiler"
    );
    let fetched_v = app.view().usage.cache.queries() - queries;
    assert_budget("usage_scroll vertical", vertical.clone(), bench.last);

    app.dispatch(Action::SetZoom(TimeScaleZoom::Day));
    bench.settle(&mut app);
    let x = app.view().gantt_scroll_x;
    app.dispatch(Action::ScrollGanttX(-x));
    bench.settle(&mut app);
    let queries = app.view().usage.cache.queries();
    let horizontal: Vec<Duration> = (0..120)
        .map(|_| {
            app.dispatch(Action::ScrollGanttX(300.0));
            bench.frame(&mut app, Vec::new())
        })
        .collect();
    let fetched_h = app.view().usage.cache.queries() - queries;
    assert!(fetched_h > 2, "aucun bloc neuf ({fetched_h})");
    let worst = *horizontal.iter().chain(&vertical).max().unwrap();
    eprintln!(
        "perf UI usage_scroll : blocs neufs {fetched_v} (vertical), {fetched_h} (horizontal), pire image {:.2} ms",
        ms(worst)
    );
    assert_budget("usage_scroll horizontal", horizontal, bench.last);
    assert!(worst < SPIKE, "bloc neuf : {worst:?} ≥ {SPIKE:?}");
}

/// Budget de `RowView::build` (J8, lot L5) : médiane.
const VIEW_BUILD_BUDGET: Duration = Duration::from_millis(8);

/// Durées de `RowView::build` pour `state` (repli courant de `app`), `n`
/// constructions.
fn view_builds(app: &App, state: &TaskViewState, n: usize) -> Vec<Duration> {
    let doc = app.document.as_ref().unwrap();
    let project = doc.project();
    let ctx = FilterCtx {
        project,
        schedule: doc.schedule(),
        status: project.effective_status_date(),
    };
    let model = app.rows().unwrap();
    (0..n)
        .map(|_| {
            let t0 = Instant::now();
            black_box(RowView::build(model, &ctx, state, app.collapsed()));
            t0.elapsed()
        })
        .collect()
}

/// Médiane des constructions de la vue, vérifiée contre
/// [`VIEW_BUILD_BUDGET`] (rapport JSON écrit avant).
fn assert_build_budget(label: &str, mut builds: Vec<Duration>) {
    builds.sort();
    let median = percentile(&builds, 50);
    eprintln!(
        "perf UI {label} : RowView::build médiane {:.2} ms, max {:.2} ms ({} constructions)",
        ms(median),
        builds.last().map_or(0.0, |d| ms(*d)),
        builds.len()
    );
    report(
        label,
        &builds,
        Counts::default(),
        serde_json::json!({ "budget_median_ms": VIEW_BUILD_BUDGET.as_millis() }),
    );
    assert!(
        median < VIEW_BUILD_BUDGET,
        "{label} : médiane {median:?} ≥ {VIEW_BUILD_BUDGET:?}"
    );
}

/// J8 L5 : filtre « Critiques », puis tri Début décroissant avec structure,
/// puis retour à l'ordre des ID, 20 images par étape (molette à chaque
/// image ; l'image du changement compte la reconstruction de la vue).
/// `RowView::build` médiane < 8 ms ; image médiane < 16 ms, p95 < 32 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn filter_sort_5000() {
    let (mut app, mut bench) = nav_bench();
    let critical = TaskViewState {
        filter: FilterChoice::Preset(FilterPreset::Critical),
        ..TaskViewState::default()
    };
    let sorted = TaskViewState {
        sort: Some(SortSpec {
            key: SortKey::Start,
            descending: true,
            keep_outline: true,
        }),
        ..critical.clone()
    };
    let by_id = TaskViewState {
        sort: Some(SortSpec {
            key: SortKey::Id,
            descending: false,
            keep_outline: true,
        }),
        ..critical.clone()
    };
    let mut builds = Vec::new();
    let mut samples = Vec::new();
    for state in [critical, sorted, by_id] {
        builds.extend(view_builds(&app, &state, 20));
        let t0 = Instant::now();
        app.set_task_view(state);
        let change = t0.elapsed();
        samples.push(change + bench.frame(&mut app, wheel_events(-WHEEL_STEP)));
        for _ in 1..20 {
            samples.push(bench.frame(&mut app, wheel_events(-WHEEL_STEP)));
        }
        assert!(!app.row_view().is_empty());
        assert!(app.row_view().len() < app.rows().unwrap().len());
    }
    assert_build_budget("filter_sort_5000 build", builds);
    assert_budget("filter_sort_5000", samples, bench.last);
}

/// J8 L5 : niveau 1, Tous, niveau 2 (20 images chacun), puis repli de 50
/// récapitulatives une par image ; mêmes seuils que `filter_sort_5000`.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn outline_5000() {
    let (mut app, mut bench) = nav_bench();
    let mut builds = Vec::new();
    let mut samples = Vec::new();
    for level in [Some(1), None, Some(2)] {
        let state = TaskViewState {
            outline_level: level,
            ..TaskViewState::default()
        };
        builds.extend(view_builds(&app, &state, 20));
        let t0 = Instant::now();
        app.set_task_view(state);
        let change = t0.elapsed();
        samples.push(change + bench.frame(&mut app, wheel_events(-WHEEL_STEP)));
        for _ in 1..20 {
            samples.push(bench.frame(&mut app, wheel_events(-WHEEL_STEP)));
        }
    }
    assert_eq!(app.row_view().len(), app.rows().unwrap().len());
    app.set_task_view(TaskViewState::default());
    let summaries: Vec<_> = (0..app.rows().unwrap().len())
        .filter_map(|i| app.rows().unwrap().row(i))
        .filter(|row| row.kind == RowKind::Summary)
        .map(|row| row.uid)
        .take(50)
        .collect();
    assert_eq!(summaries.len(), 50);
    for uid in summaries {
        let t0 = Instant::now();
        app.toggle_collapsed(uid);
        let change = t0.elapsed();
        samples.push(change + bench.frame(&mut app, Vec::new()));
    }
    builds.extend(view_builds(&app, &TaskViewState::default(), 20));
    assert_eq!(
        app.row_view().len(),
        app.rows().unwrap().len() - 50 * LEAVES_PER_SUMMARY
    );
    assert_build_budget("outline_5000 build", builds);
    assert_budget("outline_5000", samples, bench.last);
}

/// Finitions P3 (H-UI-129) : Texte1 posé sur toutes les feuilles (40
/// lots), regroupement par ce champ (lot `style.set_group_by`, recalcul et
/// vue regroupée : image du changement publiée, garde < 150 ms), puis 20
/// crans de molette, 100 ↓ (en-têtes sautés) et 10 groupes repliés un par
/// image ; image médiane < 16 ms, p95 < 32 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn group_by_5000() {
    use plannercraft_render::group::GroupKey;
    use plannercraft_render::style::GroupBy;
    use plannercraft_ui_egui::group_view::GroupOp;
    let (mut app, mut bench) = nav_bench();
    let leaves = spread_leaves(&app, usize::MAX);
    assert!(leaves.len() > 4_000, "{}", leaves.len());
    let ops: Vec<serde_json::Value> = leaves
        .iter()
        .map(|u| serde_json::json!({ "op": "task.update", "uid": u, "custom": { "text1": format!("Lot {}", u % 40) } }))
        .collect();
    // Lots de 500 opérations (borne d'un lot).
    for chunk in ops.chunks(500) {
        let batch = plannercraft_automation::Batch::from_json(
            serde_json::json!({ "format_version": 1, "ops": chunk }),
        )
        .unwrap();
        app.dispatch(Action::Exec(batch));
    }
    bench.settle(&mut app);
    let field = GroupBy::Custom(plannercraft_core::CustomFieldId::TEXT1);
    let t0 = Instant::now();
    app.dispatch(Action::Group(GroupOp::Set(Some(field))));
    let change = t0.elapsed() + bench.frame(&mut app, Vec::new());
    eprintln!("perf UI group_by_5000 changement : {:.2} ms", ms(change));
    assert!(change < EDIT_SPIKE, "changement {change:?} ≥ {EDIT_SPIKE:?}");
    assert!(app.row_view().is_grouped());
    // Toutes les feuilles regroupées, une ligne d'en-tête par lot présent.
    let lots: std::collections::BTreeSet<u32> = leaves.iter().map(|u| u % 40).collect();
    assert_eq!(app.row_view().members().len(), leaves.len());
    assert_eq!(app.row_view().len(), leaves.len() + lots.len());
    bench.settle(&mut app);
    let mut samples = Vec::new();
    for _ in 0..20 {
        samples.push(bench.frame(&mut app, wheel_events(-WHEEL_STEP)));
    }
    for _ in 0..100 {
        samples.push(bench.frame(&mut app, key_events(Key::ArrowDown, Modifiers::NONE)));
    }
    assert!(app.selection.is_some());
    for k in 0..10 {
        app.dispatch(Action::Group(GroupOp::Toggle(GroupKey::Text(format!("Lot {k}")))));
        samples.push(bench.frame(&mut app, wheel_events(-WHEEL_STEP)));
    }
    assert!(app.row_view().len() < leaves.len());
    assert_budget("group_by_5000", samples, bench.last);
}

/// J8 L9 : barre de recherche ouverte, « préparatio » tapé un caractère par
/// image (correspondances recalculées à chaque frappe), puis 20 « suivant »
/// (sélection, défilement) ; image médiane < 16 ms, p95 < 32 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn search_5000() {
    use plannercraft_ui_egui::view::task_view::TaskViewOp;
    let (mut app, mut bench) = nav_bench();
    // Génération de départ : la frise du ruban est posée par un lot.
    let generation = app.document.as_ref().unwrap().generation();
    app.dispatch(Action::TaskView(TaskViewOp::OpenSearch));
    bench.settle(&mut app);
    let query = "préparatio";
    let mut samples = Vec::new();
    for (end, _) in query.char_indices().skip(1).chain([(query.len(), ' ')]) {
        let prefix = query.get(..end).unwrap_or(query).to_owned();
        app.dispatch(Action::TaskView(TaskViewOp::SetQuery(prefix)));
        samples.push(bench.frame(&mut app, Vec::new()));
    }
    let search = app.search().unwrap();
    assert_eq!(search.query, query);
    assert_ne!(search.hits.len(), 0);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..20 {
        app.dispatch(Action::TaskView(TaskViewOp::SearchNext));
        samples.push(bench.frame(&mut app, Vec::new()));
        seen.extend(app.selection);
    }
    assert!(seen.len() > 1, "« suivant » doit avancer");
    assert_eq!(app.document.as_ref().unwrap().generation(), generation);
    assert_budget("search_5000", samples, bench.last);
}

/// Budget du collage de 1 000 lignes (J8, § 2.8) : collage → recalcul →
/// `RowModel` → image. **Non tenu au lot L10** : médiane mesurée de 522 à
/// 896 ms sur serveurdvp (machine partagée), dont 80 à 90 % dans
/// `Session::apply`, et là dans les 1 000 `task.add` (environ 0,7 ms chacun
/// sur 6 000 tâches ; les 1 985 `link.add` et le recalcul unique pèsent
/// peu) : coût du moteur, hors de l'interface. Publié dans le rapport, non
/// bloquant, en attendant l'arbitrage de la session principale (ADR-0016,
/// écarts du lot L10). Relecture finale de J8 (perf F4) : 429 et 435 ms
/// en release sur serveurdvp calme (2026-10-27), budget tenu mais sur une
/// machine calme seulement ; l'arbitrage reste dû au propriétaire
/// (ADR-0016, « Relecture finale de J8 »), la garde bloquante reste 1 s.
const PASTE_BUDGET: Duration = Duration::from_millis(500);

/// Garde bloquante du collage de 1 000 lignes (régression grossière) :
/// deux fois le budget.
const PASTE_GUARD: Duration = Duration::from_millis(1_000);

/// Décomposition d'un collage du tampon `slot` avant `uid`, sur une copie
/// du document : construction du lot, `Session::apply`, `RowModel::build`.
fn paste_parts(
    doc: &plannercraft_ui_egui::app::OpenDocument,
    slot: &plannercraft_ui_egui::clipboard::ClipboardSlot,
    uid: plannercraft_core::TaskUid,
    labels: &plannercraft_ui_egui::format::FormatLabels,
) -> (Duration, Duration, Duration) {
    let at = plannercraft_automation::Placement::Before(plannercraft_automation::TaskRef::Uid(uid));
    let t0 = Instant::now();
    let (batch, _) = plannercraft_automation::clip::paste_batch(
        &doc.session,
        &slot.clip,
        &at,
        &plannercraft_automation::clip::PasteOptions::default(),
    )
    .unwrap();
    let batch_time = t0.elapsed();
    let mut copy = doc.clone();
    let t0 = Instant::now();
    copy.session
        .apply(&batch, plannercraft_automation::ApplyMode::Commit)
        .unwrap();
    let apply = t0.elapsed();
    let t0 = Instant::now();
    black_box(RowModel::build(&copy, labels));
    (batch_time, apply, t0.elapsed())
}

/// J8 L10 : coller 1 000 lignes copiées (tampon interne reconnu par son
/// empreinte) au milieu du réseau de 5 000 tâches.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn paste_1000() {
    let (mut app, mut bench) = nav_bench();
    let first = app.rows().unwrap().row(0).unwrap().uid;
    let last = app.rows().unwrap().row(999).unwrap().uid;
    // Lignes entières (H-UI-95 : le tampon de lignes suit les lignes
    // entières depuis la passe UX).
    select_rows(&mut app, first, last);
    bench.settle(&mut app);
    bench.frame(&mut app, vec![Event::Copy]);
    let slot = app.clipboard().expect("rien copié").clone();
    assert_eq!(slot.clip.tasks.len(), 1_000);
    // Le texte émis (relu ici par les mêmes fonctions) : même empreinte.
    let doc = app.document.as_ref().unwrap();
    let ctx = plannercraft_automation::text::TextContext::for_project(
        doc.project(),
        plannercraft_automation::text::Language::Fr,
    );
    let text = slot
        .clip
        .to_tsv(&plannercraft_ui_egui::clipboard::tsv_columns(&app.state), &ctx);
    assert_eq!(
        plannercraft_automation::clip::fingerprint(&text),
        slot.fingerprint
    );
    let len = app.rows().unwrap().len();
    let labels = plannercraft_ui_egui::i18n::Texts::new(plannercraft_ui_egui::Language::Fr)
        .format_labels()
        .clone();
    let mut samples = Vec::new();
    let (mut batches, mut applies, mut builds) = (Vec::new(), Vec::new(), Vec::new());
    for k in 0..5 {
        let index = 2_500 + k * 100;
        let uid = app.rows().unwrap().row(index).unwrap().uid;
        app.dispatch(Action::ScrollToRow(index));
        select_rows(&mut app, uid, uid);
        bench.settle(&mut app);
        // Décomposition, hors image, sur une copie du document.
        let (batch, apply, build) = paste_parts(app.document.as_ref().unwrap(), &slot, uid, &labels);
        batches.push(batch);
        applies.push(apply);
        builds.push(build);
        let generation = app.document.as_ref().unwrap().generation();
        let paste = bench.frame(&mut app, vec![Event::Paste(text.clone())]);
        let next = bench.frame(&mut app, Vec::new());
        let doc = app.document.as_ref().unwrap();
        assert_eq!(doc.generation(), generation + 1, "collage non appliqué ({k})");
        assert_eq!(doc.project().tasks.len(), len + 1_000);
        samples.push(paste + next);
        app.dispatch(Action::Undo);
        bench.settle(&mut app);
        assert_eq!(app.document.as_ref().unwrap().project().tasks.len(), len);
    }
    samples.sort();
    let (median, max) = (percentile(&samples, 50), *samples.last().unwrap());
    let (batch_median, _, _) = stats(batches);
    let (apply_median, _, _) = stats(applies);
    let (build_median, _, _) = stats(builds);
    let rest = (ms(median) - ms(batch_median) - ms(apply_median) - ms(build_median)).max(0.0);
    eprintln!(
        "perf UI paste_1000 (5 collages de 1 000 lignes dans 5 000 tâches) : médiane {:.1} ms, max {:.1} ms ; \
         décomposition (médianes) : lot (paste_batch) {:.1} ms, Session::apply {:.1} ms, RowModel::build {:.1} ms, \
         reste {:.1} ms ; budget : médiane < {} ms",
        ms(median),
        ms(max),
        ms(batch_median),
        ms(apply_median),
        ms(build_median),
        rest,
        PASTE_BUDGET.as_millis(),
    );
    report(
        "paste_1000",
        &samples,
        bench.last,
        serde_json::json!({
            "guard_median_ms": PASTE_GUARD.as_millis(),
            "paste_batch_median_ms": ms(batch_median),
            "apply_median_ms": ms(apply_median),
            "row_model_build_median_ms": ms(build_median),
            "rest_median_ms": rest,
            "budget_median_ms": PASTE_BUDGET.as_millis(),
        }),
    );
    if median >= PASTE_BUDGET {
        eprintln!("perf UI paste_1000 : HORS BUDGET (médiane {median:?} ≥ {PASTE_BUDGET:?}), non bloquant");
    }
    assert!(
        median < PASTE_GUARD,
        "paste_1000 : médiane {median:?} ≥ {PASTE_GUARD:?}"
    );
}

/// J8 L11 : modèles mesurés par `static_styled` et la garde des formes
/// (Classique depuis L16, Sobre, Présentation en densité Aérée, Chantier
/// en densité Compacte).
const STYLED: [(TemplateId, &str); 4] = [
    (TemplateId::Classique, "classique"),
    (TemplateId::Sobre, "sobre"),
    (TemplateId::Presentation, "presentation"),
    (TemplateId::Chantier, "chantier"),
];

/// Application ouverte sur un réseau généré, modèle par défaut `id`,
/// thème `theme` (J8, lot L11).
fn styled_app(summaries: usize, links: usize, id: TemplateId, theme: ThemePref) -> App {
    let mut app = bare_app_with(summaries, LEAVES_PER_SUMMARY, links);
    app.state.default_template = id;
    app.state.theme = theme;
    decorate(&mut app);
    app
}

/// Zoom Semaine, table et Gantt défilés au milieu du réseau, images
/// stabilisées.
fn styled_middle(app: &mut App, bench: &mut Bench) {
    bench.settle(app);
    app.dispatch(Action::SetZoom(TimeScaleZoom::Week));
    bench.settle(app);
    let row = app.rows().unwrap().len() / 2;
    app.dispatch(Action::ScrollToRow(row));
    bench.settle(app);
    scroll_gantt_to_row(app, row);
    bench.settle(app);
}

/// J8 L11 (`static_styled`) : vue principale stylée sur 5 000 tâches et
/// 10 000 liens, Classique, Sobre, Présentation (Aérée), Chantier
/// (Compacte), clair et sombre ; molette à chaque image au milieu du réseau, zoom Semaine
/// (barres, liens, hachures et textes du moteur dessinés) ; mêmes seuils
/// que la vue principale.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn static_styled() {
    for (id, name) in STYLED {
        for (theme, variant) in [(ThemePref::Light, "clair"), (ThemePref::Dark, "sombre")] {
            let mut app = styled_app(SUMMARIES, LINKS, id, theme);
            let mut bench = Bench::new();
            styled_middle(&mut app, &mut bench);
            for k in 0..WARMUP {
                let dy = if k % 2 == 0 { -WHEEL_STEP } else { WHEEL_STEP };
                bench.frame(&mut app, wheel_events(dy));
            }
            let samples: Vec<Duration> = (0..2 * FRAMES)
                .map(|k| {
                    let dy = if k < FRAMES { -WHEEL_STEP } else { WHEEL_STEP };
                    bench.frame(&mut app, wheel_events(dy))
                })
                .collect();
            assert_budget(&format!("static_styled_{name}_{variant}"), samples, bench.last);
        }
    }
}

/// J8 L11 (garde indépendante de la machine) : pour chaque modèle stylé,
/// formes, sommets et galleys de la vue principale égaux à ± 5 % à 1 000 et
/// 5 000 tâches (aucun travail proportionnel au nombre de tâches pendant
/// une image : textes du moteur, bandes, hachures seulement sur les lignes
/// visibles).
#[test]
fn styled_counts_equal_at_1000_and_5000_tasks() {
    let close = |a: usize, b: usize| a.abs_diff(b) * 20 <= a.max(b);
    for (id, name) in STYLED {
        let mut measured = Vec::new();
        for summaries in [50, SUMMARIES] {
            // Ruban (seul habillage depuis L5), sans frise.
            let mut app = bare_app_with(summaries, LEAVES_PER_SUMMARY, 2 * summaries * LEAVES_PER_SUMMARY);
            app.state.default_template = id;
            app.state.theme = ThemePref::Light;
            let mut bench = Bench::new();
            styled_middle(&mut app, &mut bench);
            measured.push(bench.last);
        }
        let (small, large) = (measured[0], measured[1]);
        eprintln!("{name} : 1 000 tâches {small:?}, 5 000 tâches {large:?}");
        assert!(small.shapes > 0 && small.galleys > 0, "{name} : {small:?}");
        // Formes : borne d'un seul côté (lot L5). Sous le ruban, la vue
        // est plus basse et Chantier peint au milieu du petit réseau plus de
        // formes qu'au milieu du grand (1 090 / 1 030 : hachures des lignes
        // visibles, qui dépendent des barres montrées, pas du nombre de
        // tâches) ; un travail proportionnel au réseau ferait croître le
        // grand, ce que la borne refuse toujours. Sommets et galleys
        // restent à ± 5 %.
        assert!(
            large.shapes * 20 <= small.shapes * 21,
            "{name} : formes {small:?} / {large:?}"
        );
        assert!(
            close(small.vertices, large.vertices),
            "{name} : sommets {small:?} / {large:?}"
        );
        assert!(
            close(small.galleys, large.galleys),
            "{name} : galleys {small:?} / {large:?}"
        );
    }
}

/// J8 L12 : volet d'opérations IA ouvert sur une proposition de 250 tâches
/// décalées (diff affiché de 200 lignes, H-UI-41), 5 000 tâches,
/// défilement de la table à chaque image ; budget : médiane < 16 ms.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn ai_panel() {
    use std::sync::Arc;
    use std::sync::mpsc::sync_channel;

    use plannercraft_automation::control::{CallState, RelayCall, RelayKind, Reply};
    use plannercraft_core::{Task, TaskRole};
    use plannercraft_ui_egui::view::side_panel::MAX_DIFF_ROWS;

    let (mut app, mut bench) = nav_bench();
    let doc = app.document.as_ref().unwrap();
    let ops: Vec<serde_json::Value> = doc
        .project()
        .tasks
        .iter()
        .map(Task::uid)
        .filter(|uid| {
            doc.schedule()
                .scheduled(*uid)
                .is_some_and(|s| s.role == TaskRole::Leaf)
        })
        .take(250)
        .map(|uid| serde_json::json!({ "op": "task.update", "uid": uid.get(), "duration": { "text": "9j" } }))
        .collect();
    let (tx, rx) = sync_channel(4);
    app.control_inbox(rx);
    let (reply, replies) = sync_channel(4);
    tx.send(RelayCall {
        id: 1,
        kind: RelayKind::Execute {
            tool: "batch.apply".to_owned(),
            params: serde_json::json!({ "ops": ops }),
        },
        state: Arc::new(CallState::new()),
        reply,
    })
    .unwrap();
    bench.settle(&mut app);
    assert_eq!(replies.try_recv().unwrap(), Reply::PendingApproval { id: 1 });
    let relay = app.relay().unwrap();
    assert_eq!(relay.diff_builds(), 1);
    assert_eq!(
        relay.pending()[0].diff.as_ref().map(|d| d.rows.len()),
        Some(MAX_DIFF_ROWS)
    );
    assert!(app.side_panel().shows_ai(), "volet ouvert");
    for _ in 0..WARMUP {
        bench.frame(&mut app, wheel_events(-WHEEL_STEP));
    }
    let samples: Vec<Duration> = (0..120)
        .map(|i| {
            let dy = if i % 2 == 0 { -WHEEL_STEP } else { WHEEL_STEP };
            bench.frame(&mut app, wheel_events(dy))
        })
        .collect();
    assert_eq!(app.relay().unwrap().diff_builds(), 1, "diff calculé une fois");
    assert_budget("ai_panel", samples, bench.last);
}

/// ADR-0017 (lot L2, § 2.5) : ruban dessiné sur le réseau de 5 000 tâches,
/// un onglet différent à chaque image (5 onglets × 12) et le pointeur
/// survolant le corps du ruban ; image médiane < 16 ms, p95 < 32 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn ribbon_tabs() {
    use plannercraft_ui_egui::ribbon::{RibbonOp, RibbonTab};

    let (mut app, mut bench) = nav_bench();
    bench.settle(&mut app);
    for _ in 0..WARMUP {
        bench.frame(&mut app, Vec::new());
    }
    let mut samples = Vec::with_capacity(60);
    for round in 0..12_usize {
        for (i, tab) in RibbonTab::ALL.into_iter().enumerate() {
            app.dispatch(Action::Ribbon(RibbonOp::Select(tab)));
            // Pointeur sur le corps du ruban (sous la barre de menus et la
            // bande d'onglets), décalé à chaque image.
            let x = 40.0 + ((round * 5 + i) as f32 * 97.0) % (SCREEN.0 - 80.0);
            samples.push(bench.frame(&mut app, vec![Event::PointerMoved(pos2(x, 100.0))]));
        }
    }
    assert_eq!(app.state.ribbon.tab, RibbonTab::Format);
    assert_budget("ribbon_tabs", samples, bench.last);
}

/// ADR-0017 (lot L4, § 2.5) : chronologie de 200 tâches sur le réseau de
/// 5 000 tâches. `layout` seule (données du moteur lues une fois) sur 60
/// largeurs : médiane < 2 ms. Puis 60 images à largeur de fenêtre changée
/// à chaque image (une disposition par image), 60 images de défilement
/// horizontal du Gantt (disposition en cache, partie visible suivie) :
/// image médiane < 16 ms, p95 < 32 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn timeline_5000() {
    let (mut app, mut bench) = nav_bench();
    put_on_timeline(&mut app, 200);
    bench.settle(&mut app);
    let doc = app.document.as_ref().unwrap();
    let spec = app.state.effective_style(doc.project()).timeline;
    assert_eq!(spec.tasks.len(), 200);
    let input = plannercraft_automation::timeline::input(
        &doc.session,
        &spec,
        plannercraft_automation::text::Language::Fr,
    );
    assert_eq!(input.items.len(), 200);
    let mut layouts: Vec<Duration> = (0..60_i64)
        .map(|i| {
            let width16 = (1_200 + (i * 37) % 700) * 16;
            let t0 = Instant::now();
            black_box(plannercraft_render::timeline::layout(black_box(&input), width16));
            t0.elapsed()
        })
        .collect();
    layouts.sort();
    let layout_median = percentile(&layouts, 50);
    eprintln!(
        "perf UI timeline_5000 layout : médiane {:.3} ms, max {:.3} ms",
        ms(layout_median),
        ms(*layouts.last().unwrap())
    );
    let before = app.view().timeline.layouts_computed;
    for _ in 0..WARMUP {
        bench.frame(&mut app, Vec::new());
    }
    let mut samples = Vec::with_capacity(120);
    for i in 0..60_usize {
        bench.width = 1_200.0 + ((i * 37) % 700) as f32;
        samples.push(bench.frame(&mut app, Vec::new()));
    }
    let computed = app.view().timeline.layouts_computed - before;
    assert!(computed >= 55, "{computed} dispositions pour 60 largeurs");
    bench.width = SCREEN.0;
    bench.settle(&mut app);
    for i in 0..60_usize {
        let dx = if i % 20 < 10 { 40.0 } else { -40.0 };
        app.dispatch(Action::ScrollGanttX(dx));
        samples.push(bench.frame(&mut app, Vec::new()));
    }
    let drawn = app.view().timeline.drawn.clone().unwrap();
    assert!(drawn.window.is_some(), "partie visible du Gantt sur la frise");
    report(
        "timeline_5000_layout",
        &layouts,
        bench.last,
        serde_json::json!({ "budget_median_ms": 2 }),
    );
    assert!(
        layout_median < Duration::from_millis(2),
        "timeline_5000 : layout médiane {layout_median:?} ≥ 2 ms"
    );
    assert_budget("timeline_5000", samples, bench.last);
}

// --- Passe UX (ADR-0018, contrat L0) : sections posées vides, remplies par
// leur lot (budgets du cahier `docs/plans/ux.md` § 2.5).
// --- UX L1 --- (`select_drag_5000`, image de Ctrl+A de `select_all_copy_5000`)

/// Appui (bouton principal) en `p`, pointeur amené d'abord.
fn press_frames(bench: &mut Bench, app: &mut App, p: Pos2, pressed: bool) {
    bench.frame(app, vec![Event::PointerMoved(p)]);
    bench.frame(
        app,
        vec![Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }],
    );
}

/// UX L1 (ADR-0018, H-UI-88, H-UI-91) : appui sur une cellule du Nom vers
/// la ligne 10, pointeur tenu sous le bas du corps 120 images (défilement
/// automatique vertical, bloc étendu à chaque image), puis 60 images au-delà
/// du bord droit d'une table de 400 points (défilement automatique
/// horizontal). Image
/// médiane < 16 ms, p95 < 32 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn select_drag_5000() {
    let (mut app, mut bench) = nav_bench();
    // Table étroite : ses colonnes débordent, le défilement horizontal joue.
    app.dispatch(Action::SetTableWidth(400));
    bench.settle(&mut app);
    let x = 150.0;
    let start = (300_u16..900)
        .step_by(4)
        .map(f32::from)
        .find(|&y| {
            // Une seconde entre deux clics : jamais de double-clic (saisie).
            bench.time += 1.0;
            press_frames(&mut bench, &mut app, pos2(x, y), true);
            press_frames(&mut bench, &mut app, pos2(x, y), false);
            app.selection
                .and_then(|uid| app.rows().unwrap().index_of(uid))
                .is_some_and(|i| i >= 10)
        })
        .expect("aucune ligne 10 sous le pointeur");
    bench.time += 1.0;
    press_frames(&mut bench, &mut app, pos2(x, start), true);
    assert!(app.view().edit.editing.is_none() && app.view().select_drag.is_some());
    let first = cursor_row(&app);
    let below = pos2(x, SCREEN.1 + 40.0);
    for _ in 0..WARMUP {
        bench.frame(&mut app, vec![Event::PointerMoved(below)]);
    }
    let mut samples: Vec<Duration> = (0..120).map(|_| bench.frame(&mut app, Vec::new())).collect();
    let scrolled = app.view().last_scroll_y;
    assert!(scrolled > 0.0, "aucun défilement automatique");
    assert!(
        app.selected().len() > 60,
        "bloc de {} lignes depuis la ligne {first}",
        app.selected().len()
    );
    let right = pos2(app.view().last_table_width + 120.0, start);
    samples.extend((0..60).map(|i| {
        let events = if i == 0 {
            vec![Event::PointerMoved(right)]
        } else {
            Vec::new()
        };
        bench.frame(&mut app, events)
    }));
    let table_x = app.view().table_scroll_x;
    assert!(table_x > 0.0, "aucun défilement horizontal de la table");
    press_frames(&mut bench, &mut app, right, false);
    bench.settle(&mut app);
    assert!(app.view().select_drag.is_none());
    eprintln!("select_drag_5000 : défilement {scrolled} points, table {table_x} points");
    assert_budget("select_drag_5000", samples, bench.last);
}

/// UX L1 (ADR-0018, H-UI-92) : image de Ctrl+A sur 5 000 tâches (toutes les
/// lignes en lignes entières), 30 fois : médiane < 16 ms, p95 < 32 ms. La
/// copie du bloc 5 000 × 3 est mesurée par le lot L2.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn select_all_5000() {
    let (mut app, mut bench) = nav_bench();
    for _ in 0..WARMUP {
        bench.frame(&mut app, key_events(Key::A, Modifiers::COMMAND));
    }
    let samples: Vec<Duration> = (0..FRAMES)
        .map(|_| bench.frame(&mut app, key_events(Key::A, Modifiers::COMMAND)))
        .collect();
    assert_eq!(app.selected().len(), app.rows().unwrap().len());
    assert_budget("select_all_5000", samples, bench.last);
}
// --- UX L2 --- (`fill_down_1000`, `paste_cells_1000`, copie de `select_all_copy_5000`)

/// Lignes entières `first` à `last` (appui sur l'ID puis Maj+appui).
fn select_rows(app: &mut App, first: plannercraft_core::TaskUid, last: plannercraft_core::TaskUid) {
    use plannercraft_ui_egui::action::{PressMode, PressTarget, SelectOp};
    for (uid, mode) in [(first, PressMode::Replace), (last, PressMode::Extend)] {
        app.dispatch(Action::SelectOp(SelectOp::Press {
            row: RowRef::Item(uid),
            column: Column::Name,
            target: PressTarget::RowHeader,
            mode,
        }));
    }
}

/// Bloc de la colonne `column`, lignes affichées `first` à `last`.
fn select_cells(app: &mut App, first: usize, last: usize, column: Column) {
    use plannercraft_ui_egui::action::{PressMode, PressTarget, SelectOp};
    for (i, mode) in [(first, PressMode::Replace), (last, PressMode::Extend)] {
        let uid = app.rows().unwrap().row(i).unwrap().uid;
        app.dispatch(Action::SelectOp(SelectOp::Press {
            row: RowRef::Item(uid),
            column,
            target: PressTarget::Cell,
            mode,
        }));
    }
}

/// Mesure d'un geste d'édition sur sélection (5 répétitions annulées) :
/// image du geste et image suivante (lot appliqué) ; budget de
/// `paste_1000` (objectif 500 ms, garde bloquante 1 s).
fn measure_cell_gesture(label: &str, app: &mut App, bench: &mut Bench, events: &dyn Fn() -> Vec<Event>) {
    let mut samples = Vec::new();
    for k in 0..5 {
        let generation = app.document.as_ref().unwrap().generation();
        let gesture = bench.frame(app, events());
        let next = bench.frame(app, Vec::new());
        assert_eq!(
            app.document.as_ref().unwrap().generation(),
            generation + 1,
            "{label} non appliqué ({k})"
        );
        samples.push(gesture + next);
        app.dispatch(Action::Undo);
        bench.settle(app);
    }
    samples.sort();
    let median = percentile(&samples, 50);
    eprintln!(
        "perf UI {label} : médiane {:.1} ms, max {:.1} ms ; objectif {} ms, garde {} ms",
        ms(median),
        ms(*samples.last().unwrap()),
        PASTE_BUDGET.as_millis(),
        PASTE_GUARD.as_millis()
    );
    report(
        label,
        &samples,
        bench.last,
        serde_json::json!({ "budget_median_ms": PASTE_BUDGET.as_millis(), "guard_median_ms": PASTE_GUARD.as_millis() }),
    );
    if median >= PASTE_BUDGET {
        eprintln!("perf UI {label} : HORS OBJECTIF (médiane {median:?}), non bloquant");
    }
    assert!(
        median < PASTE_GUARD,
        "{label} : médiane {median:?} ≥ {PASTE_GUARD:?}"
    );
}

/// UX L2 : Ctrl+D sur la Durée de 1 000 lignes (R-258), dans 5 000 tâches.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn fill_down_1000() {
    let (mut app, mut bench) = nav_bench();
    select_cells(&mut app, 1_000, 1_999, Column::Duration);
    bench.settle(&mut app);
    measure_cell_gesture("fill_down_1000", &mut app, &mut bench, &|| {
        key_events(Key::D, Modifiers::COMMAND)
    });
}

/// UX L2 : collage d'un texte 1 000 × 2 (Nom, Durée) à partir d'une
/// cellule (R-257), dans 5 000 tâches.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn paste_cells_1000() {
    let (mut app, mut bench) = nav_bench();
    select_cells(&mut app, 2_000, 2_000, Column::Name);
    bench.settle(&mut app);
    let mut text = String::new();
    for i in 0..1_000 {
        std::fmt::Write::write_fmt(&mut text, format_args!("Tâche collée {i}\t{}j\n", 1 + i % 5)).unwrap();
    }
    measure_cell_gesture("paste_cells_1000", &mut app, &mut bench, &|| {
        vec![Event::Paste(text.clone())]
    });
}

/// UX L2 : copie d'un bloc 5 000 × 3 (Nom, Durée, Début) après Ctrl+A
/// (durée de la copie < 200 ms ; l'image de Ctrl+A est mesurée par L1).
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn select_all_copy_5000() {
    let (app, _bench) = nav_bench();
    let doc = app.document.as_ref().unwrap();
    let rows = app.visible_rows().unwrap();
    let block = plannercraft_ui_egui::block::Block {
        kind: plannercraft_ui_egui::selection::SelectionKind::Cells,
        rows: (0..rows.len())
            .map(plannercraft_ui_egui::row_view::DisplayRow::at)
            .collect(),
        columns: vec![Column::Name, Column::Duration, Column::Start],
        active: (RowRef::Blank, Column::Name),
    };
    let mut samples = Vec::new();
    for _ in 0..5 {
        let t0 = Instant::now();
        let text = plannercraft_ui_egui::cells::copy_text(
            &block,
            doc,
            rows,
            plannercraft_automation::text::Language::Fr,
        );
        samples.push(t0.elapsed());
        assert_eq!(black_box(text).lines().count(), rows.len());
    }
    samples.sort();
    let median = percentile(&samples, 50);
    eprintln!(
        "perf UI select_all_copy_5000 (copie) : médiane {:.1} ms ; budget 200 ms",
        ms(median)
    );
    report(
        "select_all_copy_5000_copy",
        &samples,
        Counts::default(),
        serde_json::json!({ "budget_median_ms": 200 }),
    );
    assert!(
        median < Duration::from_millis(200),
        "copie 5 000 × 3 : {median:?}"
    );
}
// --- UX L3 --- (`row_drag_5000`, `best_fit_5000`)

/// Abscisse de la colonne ID et ordonnée de la ligne affichée `target` :
/// cherchées par des clics sur l'ID (une seconde entre deux clics : jamais
/// de double-clic), comme `select_drag_5000`.
fn id_point(bench: &mut Bench, app: &mut App, target: usize) -> Pos2 {
    let x = 24.0;
    let y = (200_u16..1000)
        .step_by(2)
        .map(f32::from)
        .find(|&y| {
            bench.time += 1.0;
            press_frames(bench, app, pos2(x, y), true);
            press_frames(bench, app, pos2(x, y), false);
            app.selection
                .and_then(|uid| app.rows().unwrap().index_of(uid))
                .is_some_and(|i| i == target)
        })
        .expect("ligne introuvable sous le pointeur");
    pos2(x, y)
}

/// UX L3 (ADR-0018, H-UI-94) : deux lignes sœurs entières vers la ligne
/// 10, appui sur leur ID puis 60 images de glisser (4 points par image,
/// trait d'insertion et refus recalculés quand l'intervalle change), puis
/// lâcher (un lot). Image médiane < 16 ms, p95 < 32 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn row_drag_5000() {
    let (mut app, mut bench) = nav_bench();
    // Deux feuilles sœurs voisines à partir de la ligne 10.
    let first = {
        let rows = app.rows().unwrap();
        (10..rows.len() - 1)
            .find(|&i| {
                let (a, b) = (rows.row(i).unwrap(), rows.row(i + 1).unwrap());
                a.kind == RowKind::Leaf && b.kind == RowKind::Leaf && a.level == b.level
            })
            .unwrap()
    };
    let p = id_point(&mut bench, &mut app, first);
    let (a, b) = {
        let rows = app.rows().unwrap();
        (rows.row(first).unwrap().uid, rows.row(first + 1).unwrap().uid)
    };
    select_rows(&mut app, a, b);
    bench.settle(&mut app);
    let generation = app.document.as_ref().unwrap().generation();
    bench.time += 1.0;
    press_frames(&mut bench, &mut app, p, true);
    for k in 1..=WARMUP {
        bench.frame(&mut app, vec![Event::PointerMoved(p + vec2(0.0, 4.0 * k as f32))]);
    }
    assert!(app.view().row_drag.is_some(), "déplacement non ouvert");
    let samples: Vec<Duration> = (0..60)
        .map(|k| {
            let y = p.y + 4.0 * (WARMUP + k) as f32;
            bench.frame(&mut app, vec![Event::PointerMoved(pos2(p.x, y))])
        })
        .collect();
    let drop = pos2(p.x, p.y + 4.0 * (WARMUP + 60) as f32);
    press_frames(&mut bench, &mut app, drop, false);
    bench.settle(&mut app);
    assert!(app.view().row_drag.is_none());
    assert_eq!(
        app.document.as_ref().unwrap().generation(),
        generation + 1,
        "lignes non déplacées : {:?}",
        app.messages().last()
    );
    assert_budget("row_drag_5000", samples, bench.last);
}

/// UX L3 (ADR-0018, H-UI-99) : ajustement de la colonne Nom (galley de
/// chacun des 5 000 noms affichés) ; dix fois, la largeur remise à 220
/// entre deux. Mesure : image qui sert `FitColumn` (ajustement compris),
/// médiane < 50 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn best_fit_5000() {
    let (mut app, mut bench) = nav_bench();
    let mut samples = Vec::new();
    for _ in 0..10 {
        app.dispatch(Action::SetColumnWidth {
            column: Column::Name,
            width: 220,
        });
        bench.settle(&mut app);
        app.dispatch(Action::FitColumn(Column::Name));
        samples.push(bench.frame(&mut app, Vec::new()));
        let w = app.state.column(Column::Name).unwrap().width;
        assert_ne!(w, 220, "largeur non ajustée");
    }
    samples.sort();
    let median = percentile(&samples, 50);
    eprintln!(
        "perf UI best_fit_5000 : médiane {:.1} ms, max {:.1} ms ; budget 50 ms",
        ms(median),
        ms(*samples.last().unwrap())
    );
    report(
        "best_fit_5000",
        &samples,
        bench.last,
        serde_json::json!({ "budget_median_ms": 50 }),
    );
    assert!(median < Duration::from_millis(50), "ajustement : {median:?}");
}

// --- Seconde passe UX (ADR-0019, contrat L0) : sections posées vides,
// remplies par leur lot (budgets du cahier `docs/plans/ux2.md` § 2.5).
// --- UX2 L2 --- (`paste_cells_1000` et `paste_1000` tenus)
// --- UX2 L4 --- (`splitter_drag_5000`)

/// Glisse un séparateur de volets de `rect` (zone de saisie) sur 60
/// images, d'un point par image dans le sens `dir`, puis lâche ; rend les
/// durées des 60 images du glisser.
fn drag_splitter(bench: &mut Bench, app: &mut App, grip: Rect, dir: f32) -> Vec<Duration> {
    let start = grip.center();
    press_frames(bench, app, start, true);
    let mut samples = Vec::with_capacity(60);
    for i in 1..=60_u16 {
        let p = pos2(start.x, start.y + dir * f32::from(i));
        samples.push(bench.frame(app, vec![Event::PointerMoved(p)]));
    }
    let end = pos2(start.x, start.y + dir * 60.0);
    bench.frame(
        app,
        vec![Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    samples
}

/// UX2 L4 (H-UI-111, § 2.5) : réseau de 5 000 tâches, frise affichée et
/// volet de détails ouvert ; séparateur de la frise glissé 60 images vers
/// le bas, puis celui des détails 60 images vers le haut (volets et table
/// redisposés à chaque image, hauteurs écrites au relâcher). Image médiane
/// < 16 ms, p95 < 32 ms.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn splitter_drag_5000() {
    let (mut app, mut bench) = nav_bench();
    app.dispatch(Action::Pane(plannercraft_ui_egui::action::PaneOp::ShowDetails(
        true,
    )));
    bench.settle(&mut app);
    let drawn = app.view().panes.drawn.unwrap();
    let mut samples = drag_splitter(&mut bench, &mut app, drawn.timeline_splitter.unwrap(), 1.0);
    bench.settle(&mut app);
    assert_eq!(app.state.timeline_height, 132, "frise glissée de 60 points");
    let drawn = app.view().panes.drawn.unwrap();
    samples.extend(drag_splitter(
        &mut bench,
        &mut app,
        drawn.details_splitter.unwrap(),
        -1.0,
    ));
    bench.settle(&mut app);
    assert_eq!(
        app.state.details.height,
        plannercraft_ui_egui::state::DETAILS_HEIGHT_DEFAULT + 60,
        "détails glissés de 60 points"
    );
    assert_budget("splitter_drag_5000", samples, bench.last);
}
// --- UX2 L5 --- (`details_follow_5000`)

/// UX2 L5 (H-UI-112, § 2.5) : réseau de 5 000 tâches, volet de détails
/// ouvert (forme Prédécesseurs et successeurs) ; ↓ maintenue 120 images,
/// la tâche suivie change à chaque image (brouillon relu, successeurs
/// cherchés dans tout le projet). Image médiane < 16 ms, p95 < 32 ms ; rien
/// d'écrit dans le document.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn details_follow_5000() {
    use plannercraft_ui_egui::action::PaneOp;
    use plannercraft_ui_egui::state::DetailsForm;
    let (mut app, mut bench) = nav_bench();
    app.dispatch(Action::Pane(PaneOp::ShowDetails(true)));
    app.dispatch(Action::Pane(PaneOp::SetDetailsForm(
        DetailsForm::PredecessorsSuccessors,
    )));
    bench.settle(&mut app);
    let generation = app.document.as_ref().unwrap().generation();
    for _ in 0..WARMUP {
        bench.frame(&mut app, key_events(Key::ArrowDown, Modifiers::NONE));
    }
    let start = cursor_row(&app);
    let samples: Vec<Duration> = (0..120)
        .map(|_| bench.frame(&mut app, key_events(Key::ArrowDown, Modifiers::NONE)))
        .collect();
    assert_eq!(cursor_row(&app), start + 120, "une ligne par image");
    bench.settle(&mut app);
    let followed = app
        .view()
        .details
        .as_ref()
        .and_then(|d| d.draft.as_ref())
        .map(|d| d.uid);
    assert_eq!(followed, app.selection, "le volet suit la cellule active");
    assert_eq!(app.document.as_ref().unwrap().generation(), generation);
    assert_budget("details_follow_5000", samples, bench.last);
}
// --- UX2 L6 --- (`field_picker_5000`)

/// Seconde passe UX, lot L6 (§ 2.5) : sélecteur de champ ouvert sur le
/// réseau de 5 000 tâches, 6 caractères tapés, un par image (recherche
/// relancée à chaque frappe), cinq fois ; image médiane < 16 ms, p95
/// < 32 ms. Rien d'écrit dans le document.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn field_picker_5000() {
    use plannercraft_ui_egui::action::ColumnOp;
    let (mut app, mut bench) = nav_bench();
    let generation = app.document.as_ref().unwrap().generation();
    let mut samples = Vec::new();
    for _ in 0..5 {
        app.dispatch(Action::Column(ColumnOp::OpenPicker { at: 2 }));
        bench.settle(&mut app);
        for c in "travai".chars() {
            samples.push(bench.frame(&mut app, vec![Event::Text(c.to_string())]));
        }
        assert_eq!(app.view().picker.as_ref().unwrap().query, "travai");
        bench.frame(&mut app, key_events(Key::Escape, Modifiers::NONE));
        bench.settle(&mut app);
        assert!(app.view().picker.is_none());
    }
    assert_eq!(app.document.as_ref().unwrap().generation(), generation);
    assert_budget("field_picker_5000", samples, bench.last);
}

// --- UX2 L7 --- (`wide_table_5000`)

/// Seconde passe UX, lot L7 (§ 2.5) : réseau de 5 000 tâches, ligne de
/// base 0 posée puis une durée modifiée (écarts non nuls), 31 colonnes
/// affichées (les 7 par défaut, Indicateurs comprise, lignes de base 0 à 3, écarts, marges,
/// dates au plus tôt et au plus tard, travail, successeurs…) ; 120 images
/// de molette verticale puis 60 de défilement horizontal de la table ;
/// image médiane < 16 ms, p95 < 32 ms. Textes des colonnes calculés une
/// fois (`RowModel::ensure_columns`), durée de ce calcul rapportée.
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn wide_table_5000() {
    use plannercraft_ui_egui::action::ColumnOp;
    let (mut app, mut bench) = nav_bench();
    set_baseline(&mut app, &mut bench);
    let before = app.state.columns.len();
    let wide = [
        Column::BaselineStart,
        Column::BaselineFinish,
        Column::BaselineDuration,
        Column::BaselineWork,
        Column::Baseline1Start,
        Column::Baseline1Finish,
        Column::Baseline2Start,
        Column::Baseline3Finish,
        Column::StartVariance,
        Column::FinishVariance,
        Column::DurationVariance,
        Column::WorkVariance,
        Column::TotalSlack,
        Column::FreeSlack,
        Column::EarlyStart,
        Column::EarlyFinish,
        Column::LateStart,
        Column::LateFinish,
        Column::Work,
        Column::Successors,
        Column::ConstraintType,
        Column::Deadline,
        Column::PercentComplete,
        Column::Notes,
    ];
    // Actions appliquées à l'image suivante : insertions en fin de table.
    for column in wide {
        app.dispatch(Action::Column(ColumnOp::Insert { at: 64, column }));
    }
    let t0 = Instant::now();
    bench.settle(&mut app);
    let fill = t0.elapsed();
    // Colonnes par défaut (Indicateurs comprise depuis les finitions P1)
    // plus les 24 insérées.
    assert_eq!(app.state.columns.len(), before + wide.len());
    for _ in 0..WARMUP {
        bench.frame(&mut app, wheel_events(-WHEEL_STEP));
    }
    let doc = app.document.as_ref().unwrap();
    let first = doc.project().tasks.at(10).unwrap().uid();
    let rows = app.rows().unwrap();
    assert_ne!(rows.text(rows.index_of(first).unwrap(), Column::LateFinish), "");
    let vertical: Vec<Duration> = (0..120)
        .map(|_| bench.frame(&mut app, wheel_events(-WHEEL_STEP)))
        .collect();
    let horizontal: Vec<Duration> = (0..60)
        .map(|_| {
            app.dispatch(Action::ScrollTableX(40.0));
            bench.frame(&mut app, Vec::new())
        })
        .collect();
    assert!(app.view().table_scroll_x > 0.0, "la table n'a pas défilé");
    eprintln!(
        "perf UI wide_table_5000 : calcul des 24 colonnes ajoutées (image comprise) {:.1} ms",
        ms(fill)
    );
    assert_budget("wide_table_5000 vertical", vertical, bench.last);
    assert_budget("wide_table_5000 horizontal", horizontal, bench.last);
}

/// Lot A1b (ADR-0024, cahier `docs/plans/assistant-chat.md` § 1) : réponse
/// longue de l'assistant affichée en flux sur un réseau de 5 000 tâches ;
/// image médiane < 16 ms, p95 < 32 ms (mêmes seuils que la navigation).
#[test]
#[ignore = "mesure, à lancer en --release : cargo test -p plannercraft-ui-egui --release --test perf -- --ignored --nocapture"]
fn assistant_stream_5000() {
    use std::fmt::Write as _;
    use std::sync::Arc;

    use plannercraft_assistant::testing::{ScriptedTransport, sse_response, text_chunks};
    use plannercraft_ui_egui::assistant::{AssistantOp, AssistantServices};
    use plannercraft_ui_egui::view::side_panel::{SideOp, SideTab};

    let (mut app, mut bench) = nav_bench();
    let mut answer = String::from("## Synthèse du planning\n\n");
    for i in 1..=60 {
        let _ = writeln!(
            answer,
            "- Lot {i} : la **tâche {i}** finit avant la tâche suivante, marge libre conservée ; \
             rien à signaler."
        );
        if i % 15 == 0 {
            let _ = writeln!(
                answer,
                "\nLes lots {} à {i} sont sur le **chemin critique**.\n",
                i - 14
            );
        }
    }
    let transport = Arc::new(ScriptedTransport::manual(vec![sse_response(&text_chunks(
        &answer, 0.01,
    ))]));
    app.set_assistant_services(AssistantServices::with_transport(Arc::clone(&transport) as _));
    app.dispatch(Action::SidePanel(SideOp::Open(SideTab::Assistant)));
    app.dispatch(Action::Assistant(AssistantOp::Enable));
    "sk-or-v1-perf".clone_into(&mut app.assistant_mut().key_draft);
    app.dispatch(Action::Assistant(AssistantOp::SaveKey));
    bench.settle(&mut app);
    "Résume le planning".clone_into(&mut app.assistant_mut().draft);
    app.dispatch(Action::Assistant(AssistantOp::Send));
    bench.settle(&mut app);
    for _ in 0..WARMUP {
        transport.step(4);
        bench.frame(&mut app, Vec::new());
    }
    let mut samples = Vec::new();
    while transport.step(4) > 0 {
        samples.push(bench.frame(&mut app, Vec::new()));
    }
    for _ in 0..60 {
        samples.push(bench.frame(&mut app, Vec::new()));
    }
    let conversation = &app.assistant().conversation;
    assert!(!conversation.busy());
    assert_eq!(conversation.turns()[0].answer, answer);
    assert!(samples.len() > 200, "{} images", samples.len());
    assert_budget("assistant_stream_5000", samples, bench.last);
}
