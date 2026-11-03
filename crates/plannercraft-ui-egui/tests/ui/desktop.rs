//! Desktop (J7, lot L14) : capture relayée (C-CTL-08), fichiers récents
//! (H-UI-28), ouverture d'un `Project` construit (N8), exemple `snapshot`
//! (`--generate`, vues, ligne, zoom, taille) et sonde des polices de repli
//! du système.
//!
//! La capture passe par un contexte egui nu (pas de kittest) : le test
//! relève lui-même la `ViewportCommand::Screenshot` émise par l'image et la
//! complète avec une image de synthèse, ou la garde sans réponse (délai).

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : un débordement doit faire échouer le test"
)]

use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::Duration;

use base64::Engine as _;
use egui::{Color32, ColorImage, ScreenshotCallback, ViewportCommand};
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::control::{CallState, RelayCall, RelayKind, Reply, WireError};
use plannercraft_automation::error::codes;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::relay::{MAX_AUTO_SCALE_DOWN, ScreenshotConfig, encode_screenshot};
use plannercraft_ui_egui::{App, DocOrigin, GanttStyle, ViewKind};
use serde_json::{Value, json};

use crate::{click, fr, harness_for, new_app};

/// Image de synthèse : bruit déterministe (xorshift) ou couleur unie.
fn image(width: usize, height: usize, noisy: bool) -> ColorImage {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let pixels = (0..width * height)
        .map(|_| {
            if !noisy {
                return Color32::from_rgb(40, 90, 160);
            }
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let [r, g, b, ..] = x.to_le_bytes();
            Color32::from_rgb(r, g, b)
        })
        .collect();
    ColorImage::new([width, height], pixels)
}

/// PNG décodé d'un résultat de capture : largeur, hauteur.
fn decoded_size(result: &Value) -> (u32, u32) {
    assert_eq!(result["mime_type"], "image/png");
    let data = result["data"].as_str().unwrap();
    let png = base64::engine::general_purpose::STANDARD.decode(data).unwrap();
    assert_eq!(png.len() as u64, result["bytes"].as_u64().unwrap());
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let decoded = image::load_from_memory(&png).unwrap();
    (decoded.width(), decoded.height())
}

/// Application avec relais (exemple ouvert) sur un contexte egui nu.
struct Bare {
    ctx: egui::Context,
    app: App,
    tx: SyncSender<RelayCall>,
}

impl Bare {
    fn new(config: Option<ScreenshotConfig>) -> Self {
        let mut app = new_app();
        let (tx, rx) = sync_channel(64);
        app.control_inbox(rx);
        if let Some(config) = config {
            app.enable_screenshots(config);
        }
        app.open_demo();
        Self {
            ctx: egui::Context::default(),
            app,
            tx,
        }
    }

    /// Une image ; rend les demandes de capture émises.
    fn frame(&mut self) -> Vec<ScreenshotCallback> {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            ..egui::RawInput::default()
        };
        let app = &mut self.app;
        let mut output = self.ctx.run_ui(raw, |ui| plannercraft_ui_egui::ui(ui, app));
        output.textures_delta.clear();
        output
            .viewport_output
            .values()
            .flat_map(|viewport| viewport.commands.iter())
            .filter_map(|command| match command {
                ViewportCommand::Screenshot(callback) => Some(callback.clone()),
                _ => None,
            })
            .collect()
    }

    /// Appel relayé en file ; rend ses réponses.
    fn push(&self, kind: RelayKind) -> Receiver<Reply> {
        let (reply, replies) = sync_channel(4);
        self.tx
            .send(RelayCall {
                id: 7,
                kind,
                state: Arc::new(CallState::new()),
                reply,
            })
            .unwrap();
        replies
    }
}

fn err_code(reply: &Reply) -> &str {
    match reply {
        Reply::Err {
            error: WireError { code, .. },
            ..
        } => code,
        other => panic!("erreur attendue : {other:?}"),
    }
}

/// Réponse finale (le fil de capture répond seul, sans nouvelle image).
fn wait(replies: &Receiver<Reply>) -> Reply {
    replies.recv_timeout(Duration::from_secs(20)).unwrap()
}

/// C-CTL-08 : réduction demandée, réduction automatique au-delà du
/// plafond (deux fois au plus), puis refus `control.screenshot_too_large` ;
/// de bout en bout, la capture demandée par le relais revient en PNG base64
/// à la taille réduite.
#[test]
fn c_ctl_08_screenshot_scaled_or_refused() {
    // Réductions demandées, sous le plafond.
    let flat = image(64, 40, false);
    for (scale, size) in [(0, (64, 40)), (1, (32, 20)), (2, (16, 10))] {
        let result = encode_screenshot(&flat, scale, 6 << 20).unwrap();
        assert_eq!(decoded_size(&result), size);
        assert_eq!(result["scale_down"], scale);
    }
    // Bruit : chaque réduction divise environ par quatre la taille du PNG.
    let noisy = image(256, 256, true);
    let full = encode_screenshot(&noisy, 0, usize::MAX).unwrap();
    let full_len = full["data"].as_str().unwrap().len();
    let half = encode_screenshot(&noisy, 1, usize::MAX).unwrap();
    let half_len = half["data"].as_str().unwrap().len();
    assert!(half_len < full_len / 2, "{half_len} / {full_len}");
    let auto = encode_screenshot(&noisy, 0, half_len).unwrap();
    assert_eq!(auto["scale_down"], 1, "réduite une fois d'elle-même");
    assert_eq!(decoded_size(&auto), (128, 128));
    assert!(auto["data"].as_str().unwrap().len() <= half_len);
    let quarter = encode_screenshot(&noisy, 2, usize::MAX).unwrap();
    let quarter_len = quarter["data"].as_str().unwrap().len();
    let twice = encode_screenshot(&noisy, 0, quarter_len).unwrap();
    assert_eq!(twice["scale_down"], u64::from(MAX_AUTO_SCALE_DOWN));
    let too_large = encode_screenshot(&noisy, 0, 16).unwrap_err();
    assert_eq!(too_large.code, codes::CONTROL_SCREENSHOT_TOO_LARGE);
    // Image incohérente (taille sans pixels) : indisponible, sans panique.
    let broken = ColorImage {
        size: [10, 10],
        source_size: egui::vec2(10.0, 10.0),
        pixels: Vec::new(),
    };
    let e = encode_screenshot(&broken, 0, 6 << 20).unwrap_err();
    assert_eq!(e.code, codes::CONTROL_SCREENSHOT_UNAVAILABLE);

    // De bout en bout : `screenshot { scale_down: 1 }` par le relais.
    let mut bare = Bare::new(Some(ScreenshotConfig::default()));
    let replies = bare.push(RelayKind::Screenshot { scale_down: 1 });
    let shots = bare.frame();
    assert_eq!(shots.len(), 1, "une demande de capture par appel");
    assert_eq!(replies.try_iter().count(), 0, "rien avant l'image");
    for shot in shots {
        shot.complete(Arc::new(image(200, 120, false)));
    }
    let Reply::Ok { result, .. } = wait(&replies) else {
        panic!("capture attendue");
    };
    assert_eq!(decoded_size(&result), (100, 60));
    // Par `execute { tool: "app.screenshot" }` aussi.
    let replies = bare.push(RelayKind::Execute {
        tool: "app.screenshot".to_owned(),
        params: json!({ "scale_down": 2 }),
    });
    let shots = bare.frame();
    for shot in shots {
        shot.complete(Arc::new(image(200, 120, false)));
    }
    let Reply::Ok { result, .. } = wait(&replies) else {
        panic!("capture attendue");
    };
    assert_eq!(decoded_size(&result), (50, 30));
    // Réduction demandée hors bornes : refus immédiat, aucune capture.
    for kind in [
        RelayKind::Screenshot { scale_down: 3 },
        RelayKind::Execute {
            tool: "app.screenshot".to_owned(),
            params: json!({ "scale_down": "x" }),
        },
    ] {
        let replies = bare.push(kind);
        assert_eq!(bare.frame().len(), 0);
        assert_eq!(err_code(&wait(&replies)), codes::PARAM_INVALID);
    }
}

/// C-CTL-08 : aucune image dans le délai ⇒ `control.screenshot_unavailable` ;
/// l'image arrivée trop tard ne produit aucune seconde réponse. Sans
/// activation (kittest, web), indisponible aussitôt.
#[test]
fn screenshot_unavailable_times_out() {
    let config = ScreenshotConfig {
        timeout: Duration::from_millis(50),
        ..ScreenshotConfig::default()
    };
    let mut bare = Bare::new(Some(config));
    let replies = bare.push(RelayKind::Screenshot { scale_down: 0 });
    let held = bare.frame();
    assert_eq!(held.len(), 1);
    assert_eq!(err_code(&wait(&replies)), codes::CONTROL_SCREENSHOT_UNAVAILABLE);
    for shot in held {
        shot.complete(Arc::new(image(8, 8, false)));
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(replies.try_iter().count(), 0, "réponse en double");
    // Demande abandonnée par l'intégration (callback détruit) : aussitôt.
    let replies = bare.push(RelayKind::Screenshot { scale_down: 0 });
    drop(bare.frame());
    assert_eq!(err_code(&wait(&replies)), codes::CONTROL_SCREENSHOT_UNAVAILABLE);
    // Capture non activée : aucune demande, indisponible.
    let mut off = Bare::new(None);
    let replies = off.push(RelayKind::Screenshot { scale_down: 0 });
    assert_eq!(off.frame().len(), 0);
    assert_eq!(err_code(&wait(&replies)), codes::CONTROL_SCREENSHOT_UNAVAILABLE);
}

/// Dossier temporaire propre au test, supprimé à la fin.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("plannercraft-l14-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

/// H-UI-28 : 10 fichiers au plus, le plus récent en tête, sans doublon ;
/// un fichier disparu est grisé (« (introuvable) ») dans Fichier → Fichiers
/// récents, puis retiré à la tentative d'ouverture, avec un message.
#[test]
fn h_ui_28_recent_files_cap_and_missing() {
    let dir = TempDir::new("recent");
    let paths: Vec<String> = (0..11)
        .map(|i| {
            let path = dir.0.join(format!("plan{i}.xml"));
            std::fs::write(&path, plannercraft_ui_egui::DEMO_MSPDI).unwrap();
            path.to_str().unwrap().to_owned()
        })
        .collect();
    let mut app = new_app();
    for path in &paths {
        app.open_path(path);
    }
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    let recent: Vec<&str> = app.state.recent.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0], paths[10]);
    assert_eq!(recent[9], paths[1], "le plus ancien est sorti");
    assert!(!recent.contains(&paths[0].as_str()));
    // Rouvert : remonte en tête, sans doublon.
    app.open_path(&paths[5]);
    assert_eq!(app.state.recent.len(), 10);
    assert_eq!(app.state.recent[0].path, paths[5]);
    assert_eq!(app.state.recent[0].name, "plan5.xml");
    assert_eq!(app.state.recent.iter().filter(|r| r.path == paths[5]).count(), 1);

    // Menu : un fichier présent s'ouvre, un fichier disparu est grisé puis
    // retiré au clic.
    std::fs::remove_file(&paths[7]).unwrap();
    let mut harness = harness_for(app);
    harness.run();
    let texts = Texts::new(Language::Fr);
    let missing = texts.get_args(
        TextId::MenuRecentMissing,
        &[("name", Arg::Str("plan7.xml".into()))],
    );
    click(&mut harness, &fr(TextId::MenuFile));
    click(&mut harness, &fr(TextId::MenuRecent));
    assert!(
        harness.query_by_label(&missing).is_some(),
        "entrée absente non signalée"
    );
    assert!(harness.query_by_label("plan8.xml").is_some());
    click(&mut harness, &missing);
    let app = harness.state();
    assert_eq!(app.state.recent.len(), 9);
    assert!(app.state.recent.iter().all(|r| r.path != paths[7]));
    let expected = texts.get_args(
        TextId::OpenErrorMissing,
        &[
            ("name", Arg::Str("plan7.xml".into())),
            ("detail", Arg::Str(paths[7].as_str().into())),
        ],
    );
    assert_eq!(
        app.messages().last().map(|m| m.text.as_str()),
        Some(expected.as_str())
    );
    assert_eq!(
        app.document.as_ref().unwrap().origin.name,
        "plan5.xml",
        "document gardé"
    );
    // Fichier présent : ouvert par le menu.
    click(&mut harness, &fr(TextId::MenuFile));
    click(&mut harness, &fr(TextId::MenuRecent));
    click(&mut harness, "plan8.xml");
    let app = harness.state();
    assert_eq!(
        app.document.as_ref().unwrap().origin.path.as_deref(),
        Some(paths[8].as_str())
    );
    assert_eq!(app.state.recent[0].path, paths[8]);
    // Fichier disparu entre le menu et l'ouverture : retiré aussi.
    std::fs::remove_file(&paths[9]).unwrap();
    harness.state_mut().open_path(&paths[9]);
    assert!(harness.state().state.recent.iter().all(|r| r.path != paths[9]));
    let last = &harness.state().messages().last().unwrap().text;
    assert!(last.contains("plan9.xml"), "{last}");
}

/// N8 : un `Project` construit s'ouvre sans MSPDI ; rien de modifié, aucun
/// fichier récent (pas de chemin), lignes construites.
#[test]
fn n8_open_project_direct() {
    let mut app = new_app();
    let project = plannercraft_tests::generate::ui_network(2, 19, 80);
    app.open_project(
        project,
        DocOrigin {
            name: "réseau".to_owned(),
            path: None,
            demo: false,
        },
    );
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    let doc = app.document.as_ref().unwrap();
    assert_eq!(doc.project().tasks.len(), 40);
    assert!(!doc.is_dirty());
    assert_eq!(
        doc.schedule().diagnostics.len(),
        0,
        "{:?}",
        doc.schedule().diagnostics
    );
    assert_eq!(doc.origin.name, "réseau");
    assert_eq!(app.rows().unwrap().len(), 40);
    assert_eq!(app.state.recent.len(), 0);
    let mut harness = harness_for(app);
    harness.run();
    assert!(harness.state().last_frame.table_rows.end > 0);
}

/// Exemple `snapshot`, inclus tel quel : chaque vue se prépare sur un
/// réseau généré (sans rendu ici ; rendu et PNG sous `PLANNERCRAFT_UI_SNAPSHOTS=1`).
#[allow(dead_code, reason = "`main` et `run` de l'exemple ne servent pas au test")]
#[path = "../../examples/snapshot.rs"]
mod snapshot_example;

/// Arguments de l'exemple.
fn snapshot_args(args: &[&str]) -> Result<snapshot_example::Args, String> {
    snapshot_example::parse_args(args.iter().map(std::ffi::OsString::from))
}

#[test]
fn snapshot_generate_and_views() {
    use snapshot_example::{Source, View};

    // Arguments.
    let args = snapshot_args(&[
        "--generate",
        "100",
        "usage",
        "--row",
        "30",
        "--zoom",
        "quarter",
        "--size",
        "1024x600",
        "u.png",
    ])
    .unwrap();
    assert_eq!(args.source, Source::Generate(100));
    assert_eq!(args.view, View::Usage);
    assert_eq!(args.row, Some(30));
    assert_eq!(args.zoom, Some(plannercraft_ui_egui::TimeScaleZoom::Quarter));
    assert_eq!(args.size, (1024.0, 600.0));
    assert_eq!(args.output, std::path::PathBuf::from("u.png"));
    assert_eq!(args.style, None);
    // Modèle par défaut (J8, lot L11).
    let styled = snapshot_args(&["plan.xml", "main", "--style", "contraste-eleve"]).unwrap();
    assert_eq!(
        styled.style,
        Some(plannercraft_render::style::TemplateId::ContrasteEleve)
    );
    assert!(snapshot_args(&["plan.xml", "main", "--style", "rose"]).is_err());
    let file = snapshot_args(&["plan.xml", "main"]).unwrap();
    assert_eq!(file.source, Source::File("plan.xml".into()));
    assert_eq!(
        file.output,
        std::path::PathBuf::from("plannercraft-snapshot-main.png")
    );
    for bad in [
        &["--generate", "0", "main"][..],
        &["--generate", "x", "main"],
        &["plan.xml", "inconnue"],
        &["plan.xml"],
        &["plan.xml", "main", "--zoom", "year"],
        &["plan.xml", "main", "--size", "10x10"],
        &["plan.xml", "main", "--bogus"],
        &["plan.xml", "main", "a.png", "b.png"],
    ] {
        assert!(snapshot_args(bad).is_err(), "{bad:?}");
    }

    // Vues sur un réseau généré de 100 tâches (5 lots de 20).
    for (view, kind, style) in [
        ("main", ViewKind::Gantt, GanttStyle::Standard),
        ("tracking", ViewKind::Gantt, GanttStyle::Tracking),
        ("resources", ViewKind::ResourceSheet, GanttStyle::Standard),
        ("usage", ViewKind::ResourceUsage, GanttStyle::Standard),
    ] {
        let args = snapshot_args(&["--generate", "100", view, "--row", "60", "--zoom", "week"]).unwrap();
        let harness = snapshot_example::prepared(&args, None).unwrap();
        let app = harness.state();
        let doc = app.document.as_ref().unwrap();
        assert_eq!(doc.project().tasks.len(), 100, "{view}");
        assert_eq!(doc.project().resources.len(), 4, "{view}");
        assert_eq!(app.state.view, kind, "{view}");
        assert_eq!(app.state.gantt_style, style, "{view}");
        assert_eq!(
            app.state.zoom,
            plannercraft_ui_egui::TimeScaleZoom::Week,
            "{view}"
        );
        let uid = app.rows().unwrap().row(60).unwrap().uid;
        assert_eq!(app.selection, Some(uid), "{view}");
        if kind == ViewKind::Gantt {
            assert!(
                app.last_frame.table_rows.contains(&60),
                "{view} : {:?}",
                app.last_frame.table_rows
            );
        }
    }
    // Ligne absente : erreur, pas de panique.
    let args = snapshot_args(&["--generate", "20", "main", "--row", "500"]).unwrap();
    assert!(snapshot_example::prepared(&args, None).is_err());

    // Rendu réel (lavapipe) seulement sous PLANNERCRAFT_UI_SNAPSHOTS=1.
    if crate::common::snapshot_mode() == crate::common::SnapshotMode::Required {
        let out = std::env::temp_dir().join(format!("plannercraft-l14-snapshot-{}.png", std::process::id()));
        let args = snapshot_args(&[
            "--generate",
            "200",
            "usage",
            "--size",
            "800x500",
            out.to_str().unwrap(),
        ])
        .unwrap();
        snapshot_example::run(&args).unwrap();
        let png = image::open(&out).unwrap();
        assert_eq!((png.width(), png.height()), (800, 500));
        drop(std::fs::remove_file(&out));
    }
}

/// Exemple `snapshot`, options du ruban et de la chronologie (ADR-0017,
/// lot L5) : `--p`, `--tab`, `--ribbon-collapsed`, vues `timeline` et
/// `print-timeline` (préparées sans rendu).
#[test]
fn snapshot_ribbon_and_timeline_views() {
    use plannercraft_ui_egui::ribbon::RibbonTab;
    use snapshot_example::{Source, TIMELINE_TASKS, View};

    let args = snapshot_args(&["--p", "timeline", "--tab", "format", "--ribbon-collapsed"]).unwrap();
    assert_eq!(args.source, Source::P);
    assert_eq!(args.view, View::Timeline);
    assert_eq!(args.tab, Some(RibbonTab::Format));
    assert!(args.ribbon_collapsed);
    assert!(snapshot_args(&["--p", "main", "--tab", "accueil"]).is_err());
    assert!(snapshot_args(&["--p", "main", "--tab"]).is_err());
    let harness = snapshot_example::prepared(&args, None).unwrap();
    let app = harness.state();
    assert_eq!(app.state.ribbon.tab, RibbonTab::Format);
    assert!(app.state.ribbon.collapsed);
    let doc = app.document.as_ref().unwrap();
    assert_eq!(doc.project().tasks.len(), 7);
    assert_eq!(
        app.state.effective_style(doc.project()).timeline.tasks,
        TIMELINE_TASKS.to_vec()
    );
    let drawn = app.view().timeline.drawn.clone().expect("frise dessinée");
    assert_eq!(drawn.elements.len(), TIMELINE_TASKS.len());

    let args = snapshot_args(&["--p", "print-timeline"]).unwrap();
    let harness = snapshot_example::prepared(&args, None).unwrap();
    let app = harness.state();
    assert!(!app.state.ribbon.collapsed);
    let Some(plannercraft_ui_egui::dialog::DialogBody::PageSetup(state)) =
        app.view().dialog.as_ref().map(|d| &d.body)
    else {
        panic!("mode impression non ouvert");
    };
    assert!(state.page.timeline, "frise cochée d'office");

    // Réseau généré : seules les tâches existantes vont sur la frise.
    let args = snapshot_args(&["--generate", "20", "timeline"]).unwrap();
    let harness = snapshot_example::prepared(&args, None).unwrap();
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    let on = app.state.effective_style(doc.project()).timeline.tasks;
    assert!(on.iter().all(|n| TIMELINE_TASKS.contains(n)), "{on:?}");
}

/// Polices (ADR-0015, `ui.md` § 5) : le fournisseur de polices du système
/// qu'eframe installe sur le desktop (feature `system_fonts`, option
/// `system_font_fallback` par défaut, vérifiée par `apps/plannercraft`) trouve les
/// noms chinois de A6 (exceptions 清明节, 劳动节, O44), absents des polices
/// d'egui. Sans police CJK sur la machine, la sonde le dit et s'arrête ;
/// `PLANNERCRAFT_FONTS=required` (job CI `ui`, `fonts-noto-cjk`) en fait un échec.
/// La table s'affiche ensuite avec un nom CJK sous kittest, qui panique sur
/// un glyphe absent.
#[test]
fn fonts_system_fallback_probe() {
    const NAMES: &str = "清明节劳动节";
    let required = std::env::var("PLANNERCRAFT_FONTS").as_deref() == Ok("required");
    let font = egui::FontId::proportional(14.0);
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();
    assert!(
        !ctx.fonts_mut(|fonts| fonts.has_glyphs(&font, NAMES)),
        "les polices d'egui ne couvrent pas le CJK : la sonde ne prouverait rien"
    );
    ctx.add_font_provider(Arc::new(egui_system_font_provider::SystemFontProvider::new()));
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.label(NAMES);
    });
    output.textures_delta.clear();
    if !ctx.fonts_mut(|fonts| fonts.has_glyphs(&font, NAMES)) {
        assert!(
            !required,
            "aucune police CJK trouvée par le système (installer fonts-noto-cjk)"
        );
        eprintln!("sonde des polices : aucune police CJK sur cette machine, vérification sautée");
        return;
    }
    // Nom de tâche CJK dans la table : aucun glyphe manquant (kittest
    // panique sinon).
    let xml = std::str::from_utf8(plannercraft_ui_egui::DEMO_MSPDI)
        .unwrap()
        .replacen("Plans de l'architecte", "清明节 劳动节", 1);
    let mut harness = harness_for(new_app());
    harness
        .ctx
        .add_font_provider(Arc::new(egui_system_font_provider::SystemFontProvider::new()));
    harness.run();
    harness
        .state_mut()
        .open_bytes(xml.as_bytes(), DocOrigin::default());
    harness.run();
    let app = harness.state();
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    let first = &app.document.as_ref().unwrap().project().tasks;
    assert!(first.iter().any(|t| t.name == "清明节 劳动节"));
    let row = app.rows().unwrap().row(2).unwrap();
    assert_eq!(
        app.document
            .as_ref()
            .unwrap()
            .project()
            .tasks
            .get(row.uid)
            .unwrap()
            .name,
        "清明节 劳动节"
    );
}
