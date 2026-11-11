//! Traductions (J8, lot L14, ADR-0016 D14) : pseudo-langue contre les
//! troncatures, diagnostics (N5) et erreurs d'ouverture (N9) traduits par
//! code.
//!
//! Pseudo-langue : messages anglais accentués, allongés de 40 % et encadrés
//! de crochets (`Texts::pseudo`, `App::use_pseudo_texts`). Un texte est
//! « coupé » quand egui l'a tronqué (`Galley::elided`, points de
//! suspension) ou quand son rectangle déborde horizontalement de son
//! rectangle de découpe (texte rogné par un panneau ou une colonne). Seuls
//! les textes égaux à un message sans argument de `TextId::ALL` sont
//! contrôlés : menus, en-têtes de colonnes, boutons, onglets et libellés des
//! fenêtres ; les noms de tâches et les valeurs des cellules peuvent être
//! tronqués par conception. Un titre de colonne n'est contrôlé que par sa
//! troncature : la dernière colonne visible peut être coupée par le bord de
//! la table (défilement horizontal).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::mpsc::Sender;

use egui::epaint::{ClippedShape, Shape};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::Moment;
use plannercraft_ui_egui::action::DialogKind;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::diag_text::DiagTexts;
use plannercraft_ui_egui::dialog::filter::FilterDialogMode;
use plannercraft_ui_egui::i18n::{Arg, TextId, Texts};
use plannercraft_ui_egui::io::{OpenEvent, PickRequest, PickedFile};
use plannercraft_ui_egui::view::side_panel::{SideOp, SideTab};
use plannercraft_ui_egui::view::task_view::TaskViewOp;
use plannercraft_ui_egui::{
    Action, App, DocOrigin, Env, FileService, Language, LanguagePref, ThemePref, UiState,
};

use crate::{harness_for, now};

/// Sélecteur de test : rend les fichiers du script (import CSV).
struct PickedFiles(VecDeque<PickedFile>);

impl FileService for PickedFiles {
    fn pick_and_open(&mut self, ctx: &egui::Context, _: Moment, _: String, _: String, tx: Sender<OpenEvent>) {
        tx.send(OpenEvent::Cancelled).unwrap();
        ctx.request_repaint();
    }

    fn pick_bytes(&mut self, ctx: &egui::Context, _: PickRequest, tx: Sender<PickedFile>) {
        tx.send(self.0.pop_front().unwrap_or(PickedFile::Cancelled))
            .unwrap();
        ctx.request_repaint();
    }
}

/// Préférences de test dans la langue `language`, thème clair.
fn state(language: LanguagePref) -> UiState {
    UiState {
        language,
        theme: ThemePref::Light,
        ..UiState::default()
    }
}

/// Harnais 1280×720 sur l'exemple, en pseudo-langue ; le sélecteur rend
/// `files`.
fn pseudo_harness(files: Vec<PickedFile>) -> Harness<'static, App> {
    let env = Env {
        files: Box::new(PickedFiles(files.into())),
        ..Env::fixed(now())
    };
    let mut app = App::new(state(LanguagePref::En), env);
    app.dispatch(Action::OpenDemo);
    app.use_pseudo_texts();
    let mut harness = harness_for(app);
    harness.run();
    assert!(harness.state().document.is_some(), "exemple non ouvert");
    assert!(harness.state().texts().is_pseudo());
    harness
}

/// Messages sans argument de la pseudo-langue, par texte affiché.
fn pseudo_labels() -> BTreeMap<String, TextId> {
    let texts = Texts::pseudo();
    TextId::ALL
        .iter()
        .map(|id| (texts.get(*id), *id))
        .filter(|(text, _)| text.starts_with('['))
        .collect()
}

/// Formes à plat (les `Shape::Vec` dépliées), avec leur découpe.
fn flatten<'a>(clip: egui::Rect, shape: &'a Shape, out: &mut Vec<(egui::Rect, &'a Shape)>) {
    if let Shape::Vec(shapes) = shape {
        for inner in shapes {
            flatten(clip, inner, out);
        }
    } else {
        out.push((clip, shape));
    }
}

/// Textes de l'interface coupés à la dernière image : (message, raison).
fn cut_labels(harness: &Harness<'static, App>, labels: &BTreeMap<String, TextId>) -> Vec<String> {
    let mut shapes = Vec::new();
    for ClippedShape { clip_rect, shape, .. } in &harness.output().shapes {
        flatten(*clip_rect, shape, &mut shapes);
    }
    // Titre de colonne : tronqué à la largeur de sa colonne (`elided`) ;
    // une colonne en partie hors de la table (défilement horizontal) n'est
    // pas une troncature.
    let texts = Texts::pseudo();
    let column_titles: BTreeSet<String> = Column::ALL.iter().map(|c| texts.get(c.title())).collect();
    let mut cut = Vec::new();
    for (clip, shape) in shapes {
        let Shape::Text(text) = shape else { continue };
        let galley = &text.galley;
        let full = galley.job.text.as_str();
        let Some(id) = labels.get(full) else { continue };
        let rect = galley.rect.translate(text.pos.to_vec2());
        if galley.elided {
            cut.push(format!("{id:?} tronqué : {full:?}"));
        } else if !column_titles.contains(full)
            && (rect.left() < clip.left() - 0.5 || rect.right() > clip.right() + 0.5)
        {
            cut.push(format!(
                "{id:?} rogné : {full:?} ({:.0}..{:.0} hors de {:.0}..{:.0})",
                rect.left(),
                rect.right(),
                clip.left(),
                clip.right()
            ));
        }
    }
    cut
}

/// Messages contrôlés présents à la dernière image.
fn seen_ids(harness: &Harness<'static, App>, labels: &BTreeMap<String, TextId>) -> BTreeSet<TextId> {
    let mut shapes = Vec::new();
    for ClippedShape { clip_rect, shape, .. } in &harness.output().shapes {
        flatten(*clip_rect, shape, &mut shapes);
    }
    shapes
        .iter()
        .filter_map(|(_, shape)| match shape {
            Shape::Text(t) => labels.get(t.galley.job.text.as_str()).copied(),
            _ => None,
        })
        .collect()
}

/// Libellé pseudo d'un message.
fn pseudo(id: TextId) -> String {
    Texts::pseudo().get(id)
}

/// Contrôle l'image courante ; `what` nomme l'écran dans le message.
fn assert_not_cut(
    harness: &Harness<'static, App>,
    labels: &BTreeMap<String, TextId>,
    what: &str,
    at_least: usize,
) {
    let cut = cut_labels(harness, labels);
    assert!(
        cut.is_empty(),
        "{what} : textes coupés en pseudo-langue :\n{}",
        cut.join("\n")
    );
    let seen = seen_ids(harness, labels);
    assert!(
        seen.len() >= at_least,
        "{what} : {} textes contrôlés (au moins {at_least} attendus) : {seen:?}",
        seen.len()
    );
}

/// Ouvre le menu `menu` de la barre (clic), contrôle, puis le referme.
fn check_menu(
    harness: &mut Harness<'static, App>,
    labels: &BTreeMap<String, TextId>,
    menu: TextId,
    at_least: usize,
) {
    crate::click(harness, &pseudo(menu));
    assert_not_cut(harness, labels, &format!("menu {menu:?}"), at_least);
    close_menus(harness);
}

/// Referme les menus ouverts (sous-menu, puis menu de la barre du haut).
fn close_menus(harness: &mut Harness<'static, App>) {
    for _ in 0..3 {
        if !egui::Popup::is_any_open(&harness.ctx) {
            break;
        }
        harness.key_press(egui::Key::Escape);
        harness.run();
    }
}

/// Vue principale à 1280×720 : barre de menus, barre d'outils, en-têtes de
/// colonnes, barre d'état, chaque menu et ses sous-menus, puis le volet
/// latéral (deux onglets) : aucun texte de l'interface coupé.
#[test]
fn pseudo_main_view_no_truncation() {
    let labels = pseudo_labels();
    let mut harness = pseudo_harness(Vec::new());
    assert_not_cut(&harness, &labels, "vue principale", 10);
    // En-têtes de colonnes, menus et ruban (barre d'outils jusqu'au lot L5)
    // effectivement contrôlés.
    // (Comparaison par texte : « Duration » est aussi le libellé de la
    // fenêtre des informations sur la tâche.)
    let seen: BTreeSet<String> = seen_ids(&harness, &labels).into_iter().map(pseudo).collect();
    // Lot U2 : ruban compact par défaut (libellé de « Diagramme de Gantt ▾ »,
    // onglets de la barre du haut) ; menus dans le bouton « Menu ».
    for id in [
        TextId::ColId,
        TextId::ColName,
        TextId::ColDuration,
        TextId::ColStart,
        TextId::ColFinish,
        TextId::MenuGanttStandard,
        TextId::RibbonTabTask,
    ] {
        assert!(seen.contains(&pseudo(id)), "{id:?} non dessiné : {seen:?}");
    }
    // Ruban complet (H-UI-174) : noms des groupes compris.
    harness
        .state_mut()
        .dispatch(Action::Ribbon(plannercraft_ui_egui::ribbon::RibbonOp::ToggleFull));
    harness.run();
    assert_not_cut(&harness, &labels, "ruban complet", 10);
    let seen: BTreeSet<String> = seen_ids(&harness, &labels).into_iter().map(pseudo).collect();
    assert!(
        seen.contains(&pseudo(TextId::RibbonGroupSchedule)),
        "groupe Planification non dessiné : {seen:?}"
    );
    harness
        .state_mut()
        .dispatch(Action::Ribbon(plannercraft_ui_egui::ribbon::RibbonOp::ToggleFull));
    harness.run();
    check_menu(&mut harness, &labels, TextId::MenuFile, 8);
    check_menu(&mut harness, &labels, TextId::MenuEdit, 8);
    check_menu(&mut harness, &labels, TextId::MenuView, 6);
    // Menus de la barre ajoutés par le lot L15 de J8.
    check_menu(&mut harness, &labels, TextId::MenuTask, 4);
    check_menu(&mut harness, &labels, TextId::MenuProject, 4);
    check_menu(&mut harness, &labels, TextId::MenuFormat, 3);
    // Sous-menus : ouverts par un clic sur leur entrée.
    for (menu, sub, at_least) in [
        (TextId::MenuView, TextId::MenuZoom, 4),
        (TextId::MenuView, TextId::MenuTheme, 3),
        (TextId::MenuView, TextId::MenuLanguage, 3),
        (TextId::MenuView, TextId::MenuFilter, 6),
        (TextId::MenuView, TextId::MenuOutlineLevel, 3),
        (TextId::MenuFile, TextId::MenuRecent, 1),
    ] {
        crate::click(&mut harness, &pseudo(menu));
        crate::click(&mut harness, &pseudo(sub));
        assert_not_cut(&harness, &labels, &format!("sous-menu {sub:?}"), at_least);
        close_menus(&mut harness);
    }
    // Volet latéral : opérations IA, puis « Pourquoi cette date ? ».
    for tab in [SideTab::AiOperations, SideTab::WhyThisDate] {
        harness.state_mut().dispatch(Action::SidePanel(SideOp::Open(tab)));
        harness.run();
        assert_not_cut(&harness, &labels, &format!("volet {tab:?}"), 12);
    }
    // Témoin : une colonne Durée trop étroite tronque son titre, et le
    // contrôle le voit.
    harness.state_mut().dispatch(Action::SetColumnWidth {
        column: Column::Duration,
        width: 40,
    });
    harness.run();
    let cut = cut_labels(&harness, &labels);
    assert!(
        cut.iter()
            .any(|c| c.contains(&format!("{:?}", pseudo(TextId::ColDuration)))),
        "troncature témoin non vue : {cut:?}"
    );
}

/// Fenêtres Trier, Filtre personnalisé, Filtre par ressource, Filtre par
/// dates, Importer CSV (aperçu), galerie des modèles, mise en page et
/// mode impression (J8 lot L15) : aucun texte coupé.
#[test]
fn pseudo_dialogs_no_truncation() {
    let labels = pseudo_labels();
    let csv = PickedFile::Picked {
        name: "taches.csv".to_owned(),
        result: Ok("Name,Duration,Color\nFormwork,3d,blue\nRebar,2d,\n"
            .as_bytes()
            .to_vec()),
    };
    let mut harness = pseudo_harness(vec![csv]);
    let dialogs = [
        ("Trier", TaskViewOp::OpenSortDialog),
        (
            "Filtre personnalisé",
            TaskViewOp::OpenFilterDialog(FilterDialogMode::Custom),
        ),
        (
            "Filtre par ressource",
            TaskViewOp::OpenFilterDialog(FilterDialogMode::Resource),
        ),
        (
            "Filtre par dates",
            TaskViewOp::OpenFilterDialog(FilterDialogMode::DateRange),
        ),
    ];
    for (what, op) in dialogs {
        harness.state_mut().dispatch(Action::TaskView(op));
        harness.run();
        assert_not_cut(&harness, &labels, what, 3);
        harness.key_press(egui::Key::Escape);
        harness.run();
    }
    harness.state_mut().dispatch(Action::ImportCsv);
    harness.run();
    harness.run();
    assert!(
        harness.query_by_label(&pseudo(TextId::CiImport)).is_some(),
        "aperçu de l'import CSV non ouvert"
    );
    assert_not_cut(&harness, &labels, "Importer CSV", 8);
    harness.key_press(egui::Key::Escape);
    harness.run();
    for (what, kind, at_least) in [
        ("galerie", DialogKind::Gallery, 6),
        ("mise en page", DialogKind::PageSetup { print_mode: false }, 12),
        ("mode impression", DialogKind::PageSetup { print_mode: true }, 12),
    ] {
        harness.state_mut().dispatch(Action::ShowDialog(kind));
        harness.run();
        assert_not_cut(&harness, &labels, what, at_least);
        harness.key_press(egui::Key::Escape);
        harness.run();
        assert!(
            harness.state().view().dialog.is_none(),
            "{what} : Échap ne ferme pas"
        );
    }
}

/// Application anglaise, aucun sélecteur.
fn en_harness() -> Harness<'static, App> {
    let mut harness = harness_for(App::new(state(LanguagePref::En), Env::fixed(now())));
    harness.run();
    harness
}

/// N5 : les diagnostics montrés à l'ouverture sont dans la langue de
/// l'interface (traduits par code), affichés tels quels dans le panneau, et
/// retraduits au changement de langue.
#[test]
fn n5_open_diagnostics_translated_en() {
    let xml = std::str::from_utf8(plannercraft_ui_egui::DEMO_MSPDI)
        .unwrap()
        .replacen("<PredecessorUID>", "<PredecessorUID>777", 1);
    let mut harness = en_harness();
    harness
        .state_mut()
        .open_bytes(xml.as_bytes(), DocOrigin::default());
    harness.state_mut().dispatch(Action::ToggleDiagnostics);
    harness.run();
    let english: Vec<String> = harness
        .state()
        .diagnostics_view()
        .entries
        .iter()
        .map(|(_, text)| text.clone())
        .collect();
    assert_ne!(english.len(), 0);
    // Mêmes textes que le traducteur anglais appliqué aux diagnostics du
    // document ; aucun texte français.
    let en = Texts::new(Language::En);
    let tr = DiagTexts::new(&en);
    let doc = harness.state().document.as_ref().unwrap();
    let mut expected: Vec<String> = doc.import_diagnostics.iter().map(|d| tr.import(d)).collect();
    expected.extend(doc.schedule().diagnostics.iter().map(|d| tr.core(d)));
    for text in &english {
        assert!(expected.contains(text), "{text} absent de {expected:?}");
        for french in ["tâche", "lien", "inconnu", "ligne", "référence", "élément"] {
            assert!(!text.contains(french), "texte français : {text}");
        }
    }
    let first = &english[0];
    assert!(harness.query_by_label(first).is_some(), "non affiché : {first}");
    // Retour au français : retraduit.
    harness
        .state_mut()
        .dispatch(Action::SetLanguage(LanguagePref::Fr));
    harness.run();
    let french: Vec<String> = harness
        .state()
        .diagnostics_view()
        .entries
        .iter()
        .map(|(_, text)| text.clone())
        .collect();
    assert_eq!(french.len(), english.len());
    assert_ne!(french, english);
}

/// N9 : une erreur de lecture MSPDI s'affiche dans la langue de
/// l'interface (titre et détail traduits par code).
#[test]
fn n9_open_error_translated_en() {
    let mut harness = en_harness();
    harness
        .state_mut()
        .open_bytes(b"<html><body/></html>", DocOrigin::default());
    harness.run();
    let en = Texts::new(Language::En);
    let detail = "not an MSPDI document: root \"html\" instead of \"Project\"";
    let expected = en.get_args(TextId::OpenErrorMspdi, &[("detail", Arg::Str(detail.into()))]);
    let messages = harness.state().messages();
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert_eq!(messages[0].text, expected);
    assert_eq!(messages[0].title, Some(TextId::OpenErrorTitle));
    assert!(harness.query_by_label(&en.get(TextId::OpenErrorTitle)).is_some());
    // Document vide : autre code, autre message.
    harness.state_mut().open_bytes(b"", DocOrigin::default());
    harness.run();
    let empty = en.get_args(
        TextId::OpenErrorMspdi,
        &[("detail", Arg::Str("empty document".into()))],
    );
    assert!(
        harness.state().messages().iter().any(|m| m.text == empty),
        "{:?}",
        harness.state().messages()
    );
}
