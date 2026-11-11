//! Habillage : barre de menus, écran d'accueil, barre d'état, bandeau des
//! décisions, notifications et historique des messages (le ruban à onglets est dans `view::ribbon`, ADR-0017 ; il
//! remplace depuis le lot L5 l'ancienne barre d'outils de J7 et J8).
//!
//! `menu_bar` et `status_bar` reçoivent l'`Ui` racine et posent leur propre
//! panneau (`egui::Panel`) ; `welcome` dessine dans le panneau central.
//! Aucune chaîne en dur : tout passe par `TextId`.

use plannercraft_core::Severity;

use crate::action::ConfirmChoice;
use crate::action::{Action, MessageOp};
use crate::app::{MAX_MESSAGES, UserMessage};
use crate::column::Column;
use crate::dialog::filter::FilterDialogMode;
use crate::format::format_date;
use crate::i18n::{Arg, TextId};
use crate::io::SaveKind;
use crate::keymap::{Binding, Shortcut, UiBinding, shortcut_for_os, shortcut_text};
use crate::notify::{MessageKind, MessageStore};
use crate::ribbon::icon_data::Icon;
use crate::ribbon::icons::paint_icon;
use crate::ribbon::spec::{PERCENT_STEPS, RibbonCmd, percent_value};
use crate::state::{FilterChoice, MAX_OUTLINE_LEVEL};
use crate::state::{LanguagePref, ThemePref};
use crate::theme::{Palette, ToastTokens};
use crate::timescale::TimeScaleZoom;
use crate::view::task_view::TaskViewOp;

use super::FrameCtx;

/// Choix de langue proposés, avec leur libellé.
const LANGUAGE_CHOICES: [(LanguagePref, TextId); 3] = [
    (LanguagePref::System, TextId::LangSystem),
    (LanguagePref::Fr, TextId::LangFr),
    (LanguagePref::En, TextId::LangEn),
];

/// Choix de thème proposés, avec leur libellé.
const THEME_CHOICES: [(ThemePref, TextId); 3] = [
    (ThemePref::System, TextId::ThemeSystem),
    (ThemePref::Light, TextId::ThemeLight),
    (ThemePref::Dark, TextId::ThemeDark),
];

/// Vues proposées (J7 lots L7, L10, L11 : Gantt, Gantt de suivi, feuille
/// et utilisation des ressources), dans l'ordre du menu et de la liste
/// « Diagramme de Gantt ▾ » du ruban.
const VIEW_CHOICES: [RibbonCmd; 4] = [
    RibbonCmd::GanttChart,
    RibbonCmd::TrackingGantt,
    RibbonCmd::ResourceSheet,
    RibbonCmd::ResourceUsage,
];

/// Clé de la mémoire d'egui où la barre de menus dépose l'état du volet
/// latéral de l'image, relu par le ruban (case « Volet Opérations IA »).
fn side_open_id() -> egui::Id {
    egui::Id::unique("plannercraft.side-open")
}

/// Clé de la mémoire d'egui de l'état « volet ouvert sur Opérations IA »
/// (case Affichage → Opérations IA ; lot A5 : la case Assistant IA suit le
/// volet, ouvert ou fermé).
fn ai_open_id() -> egui::Id {
    egui::Id::unique("plannercraft.ai-open")
}

/// État du volet latéral déposé par [`menu_bar`] à cette image.
fn side_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(side_open_id()))
        .unwrap_or(false)
}

/// État d'une case ou d'une bascule (H-UI-74) : [`RibbonCmd::checked`],
/// sauf le volet latéral, dont l'état est relu ici (même source pour le
/// menu, le ruban et le bouton de l'assistant) : Opérations IA cochée
/// quand son onglet est affiché, Assistant IA quand le volet est ouvert
/// (lot A5, H-UI-161).
pub(crate) fn item_checked(ctx: &egui::Context, fc: &FrameCtx<'_>, cmd: RibbonCmd) -> Option<bool> {
    match cmd {
        RibbonCmd::AiPanel => Some(
            ctx.data(|data| data.get_temp::<bool>(ai_open_id()))
                .unwrap_or(false),
        ),
        RibbonCmd::Assistant => Some(side_open(ctx)),
        _ => cmd.checked(fc),
    }
}

/// Pousse l'action d'une commande partagée par le ruban et la barre de
/// menus (H-UI-70) : [`RibbonCmd::action`], puis le style du Gantt pour les
/// vues Gantt (comme le menu Affichage depuis J7).
pub(crate) fn push_cmd(fc: &mut FrameCtx<'_>, cmd: RibbonCmd) {
    if let Some(action) = cmd.action(fc) {
        fc.actions.push(action);
    }
    if let Some(style) = cmd.gantt_style() {
        fc.actions.push(Action::SetGanttStyle(style));
    }
}

/// Raccourci affiché d'une liaison de `KEYMAP` pour le système courant.
fn binding_shortcut(ui: &egui::Ui, binding: Option<Binding>) -> Option<Shortcut> {
    binding.and_then(|binding| shortcut_for_os(binding, ui.ctx().os()).copied())
}

/// Entrée de menu jumelle d'un élément du ruban (H-UI-70, ADR-0017 D2) :
/// libellé du jumeau ([`RibbonCmd::menu_label`]), activation et action de
/// la commande partagée ; raccourci de `binding`.
fn twin(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, cmd: RibbonCmd, binding: Option<Binding>) {
    let shortcut = binding_shortcut(ui, binding);
    let text = twin_text(fc, cmd);
    twin_item(ui, fc, cmd, text, shortcut);
}

/// Libellé de l'entrée jumelle : [`RibbonCmd::menu_label`] ; « Retirer de
/// la chronologie » quand la tâche active y est (H-UI-79).
fn twin_text(fc: &FrameCtx<'_>, cmd: RibbonCmd) -> String {
    match cmd {
        RibbonCmd::AddToTimeline if cmd.checked(fc) == Some(true) => fc.texts.get(TextId::TimelineRemove),
        RibbonCmd::MarkPercent(step) => fc.texts.get_args(
            TextId::MenuMarkPercent,
            &[("percent", Arg::Count(percent_value(step)))],
        ),
        _ => fc.texts.get(cmd.menu_label()),
    }
}

/// Entrée de menu d'une commande partagée, libellé `text` : case pour les
/// préférences d'affichage ([`RibbonCmd::menu_is_check`]), bouton sinon ;
/// grisée selon [`RibbonCmd::enabled`], raison en infobulle (H-UI-73).
pub(crate) fn twin_item(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    cmd: RibbonCmd,
    text: String,
    shortcut: Option<Shortcut>,
) {
    command_item(ui, fc, cmd, text, shortcut, cmd.menu_is_check());
}

/// Entrée d'une commande partagée : case si `check` (état de
/// [`item_checked`]), bouton avec `shortcut` sinon ; grisée selon
/// [`RibbonCmd::enabled`], raison en infobulle (H-UI-73) ; un clic pousse
/// l'action de la commande ([`push_cmd`]).
fn command_item(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    cmd: RibbonCmd,
    text: String,
    shortcut: Option<Shortcut>,
    check: bool,
) {
    let enabled = cmd.enabled(fc);
    let response = if check {
        let mut on = item_checked(ui.ctx(), fc, cmd).unwrap_or(false);
        ui.add_enabled(enabled.yes, egui::Checkbox::new(&mut on, text))
    } else {
        let mut button = egui::Button::new(text);
        if let Some(shortcut) = shortcut {
            button = button.shortcut_text(shortcut_text(ui.ctx(), &shortcut));
        }
        ui.add_enabled(enabled.yes, button)
    };
    let response = match enabled.reason {
        Some(reason) => response.on_disabled_hover_text(fc.texts.get(reason)),
        None => response,
    };
    if response.clicked() {
        push_cmd(fc, cmd);
    }
}

/// Entrée d'un menu contextuel (table, Gantt, en-tête de l'échelle ;
/// H-UI-100, ADR-0018 D10) : la commande du ruban, libellé
/// [`RibbonCmd::context_label`] (« Retirer de la chronologie » quand la
/// tâche active y est), case pour les bascules de tâche (H-UI-74),
/// raccourci [`RibbonCmd::shortcut`] ; même activation et même action que
/// le ruban et la barre de menus.
pub(crate) fn context_item(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, cmd: RibbonCmd) {
    let text = match cmd {
        RibbonCmd::AddToTimeline | RibbonCmd::MarkPercent(_) => twin_text(fc, cmd),
        _ => fc.texts.get(cmd.context_label()),
    };
    let shortcut = cmd.shortcut(ui.ctx().os());
    command_item(
        ui,
        fc,
        cmd,
        text,
        shortcut,
        cmd.context_is_check() || cmd.menu_is_check(),
    );
}

/// Entrée jumelle de la barre de menus avec le raccourci de la commande
/// ([`RibbonCmd::shortcut`], le même qu'au ruban et au menu contextuel).
fn twin_cmd(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, cmd: RibbonCmd) {
    let shortcut = cmd.shortcut(ui.ctx().os());
    let text = twin_text(fc, cmd);
    twin_item(ui, fc, cmd, text, shortcut);
}

/// Barre de menus (Fichier, Édition, Affichage, Tâche, Projet, Format,
/// Aide) ; `side_open` : volet latéral ouvert (case du menu Affichage, J8
/// lot L12), déposé aussi pour le ruban de la même image.
pub fn menu_bar(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, side_open: bool, ai_open: bool) {
    let texts = fc.texts;
    ui.ctx().data_mut(|data| {
        data.insert_temp(side_open_id(), side_open);
        data.insert_temp(ai_open_id(), ai_open);
    });
    // Finitions P4 (H-UI-132) : même surface que le ruban, sans filet entre
    // les deux (le filet est sous le ruban).
    let surface = crate::theme::ChromeTokens::for_theme(ui.ctx().theme()).surface;
    let frame = egui::Frame::side_top_panel(ui.style()).fill(surface);
    egui::Panel::top("plannercraft_menu_bar")
        .frame(frame)
        .show_separator_line(false)
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(texts.get(TextId::MenuFile), |ui| file_menu(ui, fc));
                ui.menu_button(texts.get(TextId::MenuEdit), |ui| edit_menu(ui, fc));
                view_menu(ui, fc, side_open);
                // --- J8 L15 : menus Tâche, Projet, Format, Aide ---
                ui.menu_button(texts.get(TextId::MenuTask), |ui| task_menu(ui, fc));
                ui.menu_button(texts.get(TextId::MenuProject), |ui| project_menu(ui, fc));
                ui.menu_button(texts.get(TextId::MenuFormat), |ui| format_menu(ui, fc));
                ui.menu_button(texts.get(TextId::MenuHelp), |ui| {
                    let label = texts.get_args(
                        TextId::MenuAbout,
                        &[("product", Arg::Str(plannercraft_core::PRODUCT_NAME.into()))],
                    );
                    if ui.button(label).clicked() {
                        fc.actions.push(Action::About);
                    }
                });
            });
        });
}

/// Menu Affichage : vues, zoom, thème, langue, vue des tâches, volet IA,
/// graphique seul, surbrillance du critique, diagnostics ; chronologie et
/// repli du ruban (ADR-0017, jumeaux de l'onglet Affichage).
fn view_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, _side_open: bool) {
    let texts = fc.texts;
    let has_doc = fc.doc.is_some();
    ui.menu_button(texts.get(TextId::MenuView), |ui| {
        ui.add_enabled_ui(has_doc, |ui| view_choices(ui, fc));
        ui.separator();
        ui.menu_button(texts.get(TextId::MenuZoom), |ui| {
            ui.add_enabled_ui(has_doc, |ui| {
                zoom_choices(ui, fc);
                ui.separator();
                twin(ui, fc, RibbonCmd::EntireProject, None);
                // Finitions P1 (H-UI-124) : jumeau de Zoom → Tâches
                // sélectionnées.
                twin(ui, fc, RibbonCmd::ZoomSelected, None);
                // Jumeaux de Zoom avant / Zoom arrière (ADR-0017).
                twin(ui, fc, RibbonCmd::ZoomIn, Some(Binding::Ui(UiBinding::ZoomFiner)));
                twin(
                    ui,
                    fc,
                    RibbonCmd::ZoomOut,
                    Some(Binding::Ui(UiBinding::ZoomCoarser)),
                );
            });
        });
        ui.menu_button(texts.get(TextId::MenuTheme), |ui| theme_menu(ui, fc));
        ui.menu_button(texts.get(TextId::MenuLanguage), |ui| language_choices(ui, fc));
        // --- J8 L9 : filtres, tri, niveau ---
        super::task_view::view_menu(ui, fc);
        // --- Finitions P3 (H-UI-129) : Regrouper par, jumeau de la liste
        // de l'onglet Affichage.
        ui.menu_button(texts.get(TextId::MenuGroupBy), |ui| group_entries(ui, fc));
        // --- J8 L11 : surbrillance du critique ---
        twin(ui, fc, RibbonCmd::CriticalTasks, None);
        // --- J8 L15 : graphique seul (H-UI-64) ---
        twin(ui, fc, RibbonCmd::ChartOnly, None);
        // --- J8 L12 : volet IA, « Pourquoi cette date ? » ---
        // Bascule du volet latéral (H-UI-41) : sans document aussi
        // (l'historique de session reste lisible).
        twin(ui, fc, RibbonCmd::AiPanel, None);
        // --- ADR-0024, lot A1 : assistant IA (H-UI-138) ; lot A5 : bascule
        // du volet, Ctrl/Cmd+J (H-UI-161, H-UI-162).
        twin(
            ui,
            fc,
            RibbonCmd::Assistant,
            Some(Binding::Ui(UiBinding::ToggleAssistant)),
        );
        // --- J8 L15 : galerie, mise en page ---
        ui.separator();
        // Bascule du panneau des diagnostics : les compteurs de la
        // barre d'état l'ouvrent aussi, mais rien ne les signale comme
        // cliquables. Toujours active : un panneau resté ouvert après
        // « Fermer » doit pouvoir se refermer.
        twin(ui, fc, RibbonCmd::Diagnostics, None);
        // --- Ruban L2 : chronologie (H-UI-78), repli du ruban (H-UI-72).
        twin(ui, fc, RibbonCmd::TimelineVisible, None);
        // --- Seconde passe UX (L10) : volet de détails (H-UI-112), jumeau
        // de la case Affichage → Fractionner → Détails.
        twin(ui, fc, RibbonCmd::DetailsPane, None);
        twin(
            ui,
            fc,
            RibbonCmd::CollapseRibbon,
            Some(Binding::Ui(UiBinding::ToggleRibbon)),
        );
    });
}

/// Menu Fichier : Nouveau, exemple, Ouvrir, Enregistrer, Enregistrer
/// sous, Fermer, Quitter (desktop) ; raccourcis de
/// [`crate::keymap::KEYMAP`] affichés. Enregistrer et Fermer demandent un
/// document.
fn file_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let has_doc = fc.doc.is_some();
    let new = Binding::Command("project.new");
    menu_item(ui, fc, TextId::MenuNew, Some(new), true, Action::NewProject);
    menu_item(ui, fc, TextId::MenuOpenDemo, None, true, Action::OpenDemo);
    let open = Binding::Ui(UiBinding::OpenFile);
    menu_item(ui, fc, TextId::MenuOpen, Some(open), true, Action::OpenDialog);
    // Fichiers récents (H-UI-28) : chemins du desktop seulement.
    if !cfg!(target_arch = "wasm32") {
        let texts = fc.texts;
        ui.menu_button(texts.get(TextId::MenuRecent), |ui| recent_menu(ui, fc));
    }
    ui.separator();
    twin(ui, fc, RibbonCmd::Save, Some(Binding::Command("project.save")));
    let save_as = Binding::Ui(UiBinding::SaveAs);
    menu_item(ui, fc, TextId::MenuSaveAs, Some(save_as), has_doc, Action::SaveAs);
    // --- J8 L7 : Exporter en PDF ---
    // --- J8 L15 : mise en page, mode impression (H-UI-68) ---
    twin(ui, fc, RibbonCmd::PageSetup, None);
    twin(ui, fc, RibbonCmd::PrintMode, None);
    // --- J8 L10 : Importer CSV ---
    // Fichier → Importer CSV… (H-UI-40) : sans document aussi (le résultat
    // est un nouveau document).
    menu_item(ui, fc, TextId::MenuImportCsv, None, true, Action::ImportCsv);
    // --- J8 L13 ---
    // Fichier → Exporter (PDF, SVG, CSV) : téléchargement sur le web,
    // boîte « Enregistrer sous » sur le desktop.
    let texts = fc.texts;
    ui.add_enabled_ui(has_doc, |ui| {
        ui.menu_button(texts.get(TextId::MenuExport), |ui| {
            twin(ui, fc, RibbonCmd::ExportPdf, None);
            for (id, kind) in [
                (TextId::MenuExportSvg, SaveKind::Svg),
                (TextId::MenuExportCsv, SaveKind::Csv),
            ] {
                menu_item(ui, fc, id, None, true, Action::Export(kind));
            }
        });
    });
    ui.separator();
    // Fermer (H-UI-109) : raccourci Ctrl/Cmd+W affiché (Ctrl+F4 aussi lu
    // hors macOS) ; même fermeture gardée que la touche.
    let close = Binding::Ui(UiBinding::CloseDocument);
    menu_item(
        ui,
        fc,
        TextId::MenuClose,
        Some(close),
        has_doc,
        Action::CloseDocumentGuarded,
    );
    // Quitter : le navigateur ne laisse pas une page fermer son onglet.
    if !cfg!(target_arch = "wasm32") {
        ui.separator();
        let quit = Binding::Ui(UiBinding::Quit);
        menu_item(ui, fc, TextId::MenuQuit, Some(quit), true, Action::Quit);
    }
}

/// Sous-menu Fichier → Fichiers récents (H-UI-28) : le plus récent en
/// tête, nom affiché et chemin en infobulle ; un fichier introuvable est
/// grisé (« (introuvable) ») mais reste cliquable : la tentative
/// d'ouverture le retire de la liste. L'existence n'est relue que pendant
/// que le sous-menu est ouvert.
fn recent_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let texts = fc.texts;
    if fc.state.recent.is_empty() {
        ui.add_enabled(false, egui::Button::new(texts.get(TextId::MenuRecentEmpty)));
        return;
    }
    for (index, recent) in fc.state.recent.iter().enumerate() {
        let exists = std::path::Path::new(&recent.path).exists();
        let label = if exists {
            egui::RichText::new(recent.name.as_str())
        } else {
            let text = texts.get_args(
                TextId::MenuRecentMissing,
                &[("name", Arg::Str(recent.name.as_str().into()))],
            );
            egui::RichText::new(text).weak()
        };
        if ui
            .add(egui::Button::new(label))
            .on_hover_text(recent.path.as_str())
            .clicked()
        {
            fc.actions.push(Action::OpenRecent(index));
        }
    }
}

/// Menu Projet (J7, lot L12 ; dans la barre depuis J8 lot L15) :
/// informations sur le projet, temps de travail, ligne de base,
/// replanifier le travail restant ; grisé sans document.
fn project_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    for cmd in [
        RibbonCmd::ProjectInformation,
        RibbonCmd::WorkingTime,
        RibbonCmd::SetBaseline,
        RibbonCmd::UpdateProject,
        // Seconde passe UX (L10) : Projet → Champs personnalisés… (H-UI-118).
        RibbonCmd::CustomFields,
    ] {
        twin(ui, fc, cmd, None);
    }
}

/// Entrée de menu : libellé traduit, raccourci éventuel de la liaison,
/// action poussée au clic.
fn menu_item(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    label: TextId,
    binding: Option<Binding>,
    enabled: bool,
    action: Action,
) {
    let mut button = egui::Button::new(fc.texts.get(label));
    if let Some(shortcut) = binding.and_then(|binding| shortcut_for_os(binding, ui.ctx().os())) {
        button = button.shortcut_text(shortcut_text(ui.ctx(), shortcut));
    }
    if ui.add_enabled(enabled, button).clicked() {
        fc.actions.push(action);
    }
}

/// Menu Édition : Annuler, Rétablir, actifs seulement s'il y a un lot à
/// annuler ou à rétablir ; puis les six commandes de structure du menu
/// contextuel (grisées sans document, et sans sélection sauf « Insérer une
/// tâche ») ; raccourcis de [`crate::keymap::KEYMAP`] affichés. Rend la
/// structure découvrable sans clic droit ni touche Inser (relecture finale
/// J4, F5) ; enfin le sous-menu Projet (J7, lot L12). Jumeau du ruban
/// (ADR-0017) : défiler jusqu'à la tâche.
fn edit_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let texts = fc.texts;
    let history = fc.doc.map(|doc| doc.session.history());
    // Dernier lot annulable venu d'un agent (relais J7) : « Annuler :
    // agent — libellé » (ou le nombre d'opérations).
    let agent_undo = history
        .as_ref()
        .and_then(|h| h.entries.last())
        .filter(|entry| entry.source == plannercraft_automation::EditSource::Agent)
        .map(|entry| {
            let detail = entry.label.clone().unwrap_or_else(|| {
                let count = i64::try_from(entry.ops).unwrap_or(i64::MAX);
                texts.get_args(TextId::MenuUndoAgentOps, &[("count", Arg::Count(count))])
            });
            texts.get_args(TextId::MenuUndoAgent, &[("detail", Arg::Str(detail.into()))])
        });
    let undo = binding_shortcut(ui, Some(Binding::Command("history.undo")));
    let undo_text = agent_undo.unwrap_or_else(|| texts.get(TextId::MenuUndo));
    twin_item(ui, fc, RibbonCmd::Undo, undo_text, undo);
    twin(ui, fc, RibbonCmd::Redo, Some(Binding::Command("history.redo")));
    ui.separator();
    // Six commandes de structure (relecture finale J4, F5), jumelles du
    // menu contextuel (H-UI-100) : sous tri ou filtre, hausser et abaisser
    // sont grisés (H-UI-32, `RibbonCmd::enabled`).
    for cmd in [
        RibbonCmd::InsertTask,
        RibbonCmd::DeleteTask,
        RibbonCmd::Indent,
        RibbonCmd::Outdent,
        RibbonCmd::Link,
        RibbonCmd::Unlink,
    ] {
        twin_cmd(ui, fc, cmd);
    }
    ui.separator();
    // « Affecter des ressources » (J7, lot L13) : aucun raccourci (S61).
    twin(ui, fc, RibbonCmd::AssignResources, None);
    // --- J8 L9 : rechercher ---
    ui.separator();
    twin(ui, fc, RibbonCmd::Find, Some(Binding::Ui(UiBinding::Find)));
    // --- Ruban L2 : défiler jusqu'à la tâche (H-UI-84).
    twin(ui, fc, RibbonCmd::ScrollToTask, None);
    // --- J8 L10 : couper, copier, coller ---
    ui.separator();
    for cmd in [
        RibbonCmd::Cut,
        RibbonCmd::Copy,
        RibbonCmd::Paste,
        RibbonCmd::PasteSpecial,
    ] {
        twin_cmd(ui, fc, cmd);
    }
    // --- Passe UX (ADR-0018 D10, H-UI-100) : remplir, effacer, tout
    // sélectionner (groupe Editing de Project, S80).
    ui.separator();
    for cmd in [
        RibbonCmd::FillDown,
        RibbonCmd::ClearContents,
        RibbonCmd::SelectAll,
    ] {
        twin_cmd(ui, fc, cmd);
        if cmd == RibbonCmd::FillDown {
            fill_right_item(ui, fc, TextId::MenuFillRight);
        }
    }
    // --- J8 L15 : menu Tâche (%, mode, actif) ---
}

/// Menu Tâche (J8 lot L15) : marquer n %, basculer le mode, activer ou
/// désactiver, informations, lier et délier ; grisé sans sélection. Jumeaux
/// du ruban (ADR-0017) : insérer une récapitulative, un jalon, notes,
/// ajouter à la chronologie.
fn task_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    for (step, _) in PERCENT_STEPS {
        twin(ui, fc, RibbonCmd::MarkPercent(step), None);
    }
    ui.separator();
    twin(ui, fc, RibbonCmd::ManualSchedule, None);
    twin(ui, fc, RibbonCmd::Inactivate, None);
    // Finitions P1 : Décaler (sous-menu jumeau de la liste du ruban,
    // H-UI-121) et Inspecter (H-UI-123).
    let enabled = RibbonCmd::ShiftMenu.enabled(fc);
    let label = fc.texts.get(RibbonCmd::ShiftMenu.menu_label());
    let response = ui
        .add_enabled_ui(enabled.yes, |ui| {
            ui.menu_button(label, |ui| shift_entries(ui, fc))
        })
        .response;
    if let Some(reason) = enabled.reason {
        response.on_disabled_hover_text(fc.texts.get(reason));
    }
    twin(ui, fc, RibbonCmd::Inspect, None);
    ui.separator();
    twin(
        ui,
        fc,
        RibbonCmd::TaskInformation,
        Some(Binding::Ui(UiBinding::TaskInformation)),
    );
    twin_cmd(ui, fc, RibbonCmd::Link);
    twin_cmd(ui, fc, RibbonCmd::Unlink);
    // --- Ruban L2 : récapitulative et jalon (H-UI-83 ; au menu Tâche, comme
    // le groupe Insérer de l'onglet Tâche : au menu Édition, leur libellé
    // commencerait par celui d'« Insérer une tâche »), notes (H-UI-84),
    // chronologie (H-UI-79).
    ui.separator();
    twin(ui, fc, RibbonCmd::InsertSummary, None);
    twin(ui, fc, RibbonCmd::InsertMilestone, None);
    twin(ui, fc, RibbonCmd::TaskNotes, None);
    twin(ui, fc, RibbonCmd::AddToTimeline, None);
}

/// Menu Format (J8 lot L15) : galerie des modèles, mise en forme des
/// barres, mois relatifs, mise en page ; colonnes (jumeau du ruban,
/// ADR-0017 ; aussi au clic droit sur un titre de colonne).
fn format_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    twin(ui, fc, RibbonCmd::Gallery, None);
    // Type de la barre de la tâche active, sinon les barres de tâches.
    twin(ui, fc, RibbonCmd::FormatBars, None);
    // H-UI-67 : mois relatifs en option, quel que soit le modèle (relecture
    // finale de J8 ; seul « Planning travaux » les avait).
    twin(ui, fc, RibbonCmd::RelativeMonths, None);
    ui.separator();
    twin(ui, fc, RibbonCmd::PageSetup, None);
    // --- Seconde passe UX (L10) : Insérer une colonne…, Paramètres de
    // champ… (H-UI-114, H-UI-115), jumeaux du groupe Colonnes du ruban.
    ui.separator();
    twin_cmd(ui, fc, RibbonCmd::InsertColumn);
    twin_cmd(ui, fc, RibbonCmd::FieldSettings);
    // --- Ruban L2 : colonnes.
    let has_doc = fc.doc.is_some();
    let texts = fc.texts;
    ui.add_enabled_ui(has_doc, |ui| {
        ui.menu_button(texts.get(TextId::RibbonColumns), |ui| column_choices(ui, fc));
    });
    // --- Finitions P2 (H-UI-127) : jumeaux du groupe Texte de l'onglet
    // Tâche.
    ui.separator();
    twin_cmd(ui, fc, RibbonCmd::Bold);
    twin_cmd(ui, fc, RibbonCmd::Italic);
    for (cmd, fill) in [
        (RibbonCmd::TextColorMenu, false),
        (RibbonCmd::HighlightMenu, true),
    ] {
        let enabled = cmd.enabled(fc);
        let label = fc.texts.get(cmd.menu_label());
        let response = ui
            .add_enabled_ui(enabled.yes, |ui| {
                ui.menu_button(label, |ui| text_color_entries(ui, fc, fill))
            })
            .response;
        if let Some(reason) = enabled.reason {
            response.on_disabled_hover_text(fc.texts.get(reason));
        }
    }
    twin_cmd(ui, fc, RibbonCmd::FormatPainter);
    twin_cmd(ui, fc, RibbonCmd::ClearFormatting);
}

/// Entrées de « Couleur du texte ▾ » (`fill` faux) ou de « Surlignage ▾ »
/// (finitions P2, H-UI-127) : « Automatique » / « Aucun surlignage », la
/// palette des barres (une couleur par case, nom accessible « Couleur
/// #RRGGBB »), puis, si la couleur de la tâche active manque de contraste
/// sur le fond clair (texte à 4,5:1, 7:1 en Contraste élevé) ou rend le
/// texte illisible (surlignage), l'avertissement et la teinte corrigée
/// proposée, comme pour les barres (H-UI-61).
fn text_color_entries(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, fill: bool) {
    use crate::action::TextStyleOp;
    use plannercraft_render::style::{
        Variant, contrast_ratio, min_text_contrast, nearest_compliant, readable_tint,
    };
    let texts = fc.texts;
    let op = |c| {
        if fill {
            TextStyleOp::Fill(c)
        } else {
            TextStyleOp::Color(c)
        }
    };
    let none = if fill {
        TextId::TextHighlightNone
    } else {
        TextId::TextColorAuto
    };
    if ui.button(texts.get(none)).clicked() {
        fc.actions.push(Action::TextStyle(op(None)));
        ui.close();
    }
    let style = std::sync::Arc::clone(&fc.task_view.style.style);
    let current = fc
        .selection
        .and_then(|uid| style.task_text.get(&uid.get()))
        .and_then(|t| if fill { t.fill } else { t.color });
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(6.0 * 24.0);
        for rgb in crate::view::format_panel::SWATCHES {
            if crate::view::format_panel::swatch(ui, texts, TextId::FpSwatch, rgb, current == Some(rgb)) {
                fc.actions.push(Action::TextStyle(op(Some(rgb))));
                ui.close();
            }
        }
    });
    let Some(rgb) = current else {
        return;
    };
    // Contraste sur le fond clair et à l'impression (le thème sombre
    // adapte la couleur de lui-même).
    let tokens = style.tokens(Variant::Light);
    let min = min_text_contrast(&style);
    let (ratio, better) = if fill {
        let inks = [tokens.text, tokens.text_strong, tokens.text_muted];
        let ratio = inks
            .iter()
            .map(|ink| contrast_ratio(*ink, rgb))
            .min()
            .unwrap_or(u32::MAX);
        (ratio, Some(readable_tint(rgb, tokens.bg, &inks, min)))
    } else {
        (
            contrast_ratio(rgb, tokens.bg),
            nearest_compliant(rgb, tokens.bg, min),
        )
    };
    if ratio >= min {
        return;
    }
    let shown = format!("{},{}", ratio / 1000, (ratio % 1000) / 100);
    let shown = if texts.language() == crate::i18n::Language::En {
        shown.replace(',', ".")
    } else {
        shown
    };
    ui.colored_label(
        ui.visuals().warn_fg_color,
        texts.get_args(
            TextId::TextContrastLow,
            &[("ratio", crate::i18n::Arg::Str(shown.into()))],
        ),
    );
    if let Some(better) = better.filter(|b| *b != rgb) {
        let label = texts.get_args(
            TextId::FpUseSuggestion,
            &[("color", crate::i18n::Arg::Str(better.hex().into()))],
        );
        if ui.button(label).clicked() {
            fc.actions.push(Action::TextStyle(op(Some(better))));
            ui.close();
        }
    }
}

/// Contenu de la liste déroulante d'un élément du ruban : mêmes entrées
/// (mêmes actions) que la barre de menus (H-UI-70).
pub(crate) fn dropdown_contents(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, cmd: RibbonCmd) {
    let has_doc = fc.doc.is_some();
    match cmd {
        RibbonCmd::ViewMenu => {
            ui.add_enabled_ui(has_doc, |ui| view_choices(ui, fc));
        }
        RibbonCmd::SortMenu => sort_entries(ui, fc),
        RibbonCmd::OutlineMenu => outline_entries(ui, fc),
        RibbonCmd::FilterMenu => filter_entries(ui, fc),
        // Regrouper par ▾ (finitions P3, H-UI-129) : mêmes entrées qu'au
        // menu Affichage → Regrouper par.
        RibbonCmd::GroupByMenu => group_entries(ui, fc),
        RibbonCmd::TimescaleMenu => {
            ui.add_enabled_ui(has_doc, |ui| zoom_choices(ui, fc));
        }
        RibbonCmd::ThemeMenu => theme_menu(ui, fc),
        RibbonCmd::LanguageMenu => language_choices(ui, fc),
        RibbonCmd::ColumnsMenu => column_choices(ui, fc),
        // Décaler ▾ (finitions P1, H-UI-121) : pas prédéfinis puis
        // « Personnalisé… », mêmes entrées qu'au menu Tâche → Décaler.
        RibbonCmd::ShiftMenu => shift_entries(ui, fc),
        // Couleur du texte ▾, Surlignage ▾ (finitions P2, H-UI-127) : mêmes
        // entrées qu'au menu Format.
        RibbonCmd::TextColorMenu => text_color_entries(ui, fc, false),
        RibbonCmd::HighlightMenu => text_color_entries(ui, fc, true),
        // Remplir ▾, Effacer ▾ (passe UX, lot L5) : une entrée, la commande
        // elle-même (même action, même prédicat, même raccourci).
        RibbonCmd::FillDown | RibbonCmd::ClearContents => {
            if let Some(label) = cmd.dropdown_entry() {
                let shortcut = cmd.shortcut(ui.ctx().os());
                twin_item(ui, fc, cmd, fc.texts.get(label), shortcut);
            }
            // Remplir ▾ → À droite (Ctrl/Cmd+R, R-258 amendée), comme la
            // liste Fill de Project.
            if cmd == RibbonCmd::FillDown {
                fill_right_item(ui, fc, TextId::RibbonFillRight);
            }
        }
        _ => {}
    }
}

/// Entrées de « Décaler ▾ » (H-UI-121) : un pas prédéfini par entrée
/// ([`crate::action::ShiftStep::ALL`]), séparateur, « Personnalisé… » ;
/// chaque entrée est une commande partagée (même action, même prédicat au
/// ruban et au menu Tâche).
fn shift_entries(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    for step in crate::action::ShiftStep::ALL {
        twin_cmd(ui, fc, RibbonCmd::Shift(step));
    }
    ui.separator();
    twin_cmd(ui, fc, RibbonCmd::ShiftCustom);
}

/// Entrée « Remplir à droite » (Ctrl/Cmd+R, R-258 amendée, H-UI-120) :
/// mêmes prédicats que « Remplir vers le bas » ([`RibbonCmd::FillDown`]),
/// action `CellOp::FillRight`, raccourci du `KEYMAP`.
fn fill_right_item(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, label: TextId) {
    let enabled = RibbonCmd::FillDown.enabled(fc);
    let mut button = egui::Button::new(fc.texts.get(label));
    if let Some(shortcut) = binding_shortcut(ui, Some(Binding::Ui(UiBinding::FillRight))) {
        button = button.shortcut_text(shortcut_text(ui.ctx(), &shortcut));
    }
    let response = ui.add_enabled(enabled.yes, button);
    let response = match enabled.reason {
        Some(reason) => response.on_disabled_hover_text(fc.texts.get(reason)),
        None => response,
    };
    if response.clicked() {
        fc.actions.push(Action::Cells(crate::action::CellOp::FillRight));
    }
}

/// Colonnes (Format → Colonnes, liste du ruban ; seconde passe UX, lot
/// L6, COL-02) : « Insérer une colonne », « Paramètres de champ… »
/// (commandes partagées du ruban), puis les cases d'affichage des colonnes
/// de [`Column::ALL`] (ID et Nom grisées) dans un sous-menu « Affichage du
/// tableau ».
fn column_choices(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    twin_cmd(ui, fc, RibbonCmd::InsertColumn);
    twin_cmd(ui, fc, RibbonCmd::FieldSettings);
    ui.separator();
    let texts = fc.texts;
    ui.menu_button(texts.get(TextId::MenuTableDisplay), |ui| {
        for column in Column::ALL {
            let mut visible = fc.state.shown(column);
            let check = egui::Checkbox::new(&mut visible, fc.texts.get(column.title()));
            if ui.add_enabled(column.hideable(), check).clicked() {
                fc.actions.push(Action::ToggleColumn(column));
            }
        }
    });
}

/// Entrée d'un sous-menu de la vue des tâches, raccourci de `binding`.
fn view_entry(
    ui: &mut egui::Ui,
    fc: &mut FrameCtx<'_>,
    label: TextId,
    binding: Option<UiBinding>,
    enabled: bool,
    op: TaskViewOp,
) {
    let mut button = egui::Button::new(fc.texts.get(label));
    if let Some(shortcut) = binding_shortcut(ui, binding.map(Binding::Ui)) {
        button = button.shortcut_text(shortcut_text(ui.ctx(), &shortcut));
    }
    if ui.add_enabled(enabled, button).clicked() {
        fc.actions.push(Action::TaskView(op));
    }
}

/// Liste « Regrouper par ▾ » (finitions P3, H-UI-129) : Aucun groupe,
/// champs personnalisés texte renommés ou remplis, Ressources, Critique,
/// Jalons ; le regroupement courant est coché. Grisée sans document ou hors
/// de la vue Gantt (prédicat de [`RibbonCmd::GroupByMenu`]).
fn group_entries(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let enabled = RibbonCmd::GroupByMenu.enabled(fc).yes;
    let current = fc.task_view.group_by;
    let choices = crate::group_view::choices(fc.doc.map(crate::app::OpenDocument::project), fc.texts);
    ui.add_enabled_ui(enabled, |ui| {
        for (k, (by, label)) in choices.into_iter().enumerate() {
            if k == 1 {
                ui.separator();
            }
            if ui.selectable_label(enabled && current == by, label).clicked() {
                fc.actions
                    .push(Action::Group(crate::group_view::GroupOp::Set(by)));
                ui.close();
            }
        }
    });
}

/// Liste « Trier ▾ » : Trier…, Trier par ID (entrées du menu Affichage).
fn sort_entries(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let has_doc = fc.doc.is_some();
    let sorted = fc.task_view.state.is_sorted();
    view_entry(
        ui,
        fc,
        TextId::MenuSort,
        None,
        has_doc,
        TaskViewOp::OpenSortDialog,
    );
    view_entry(
        ui,
        fc,
        TextId::MenuSortById,
        Some(UiBinding::SortById),
        has_doc && sorted,
        TaskViewOp::ResetSort,
    );
}

/// Liste « Mode plan ▾ » : niveaux 1 à 9, Tous, masquer et afficher les
/// sous-tâches (entrées du menu Affichage).
fn outline_entries(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let has_doc = fc.doc.is_some();
    let texts = fc.texts;
    ui.add_enabled_ui(has_doc, |ui| {
        let current = fc.task_view.state.outline_level;
        for level in 1..=MAX_OUTLINE_LEVEL {
            let label = texts.get_args(TextId::OutlineLevelN, &[("count", Arg::Count(i64::from(level)))]);
            if ui.selectable_label(current == Some(level), label).clicked() {
                fc.actions
                    .push(Action::TaskView(TaskViewOp::SetOutlineLevel(Some(level))));
            }
        }
        if ui
            .selectable_label(current.is_none(), texts.get(TextId::OutlineAll))
            .clicked()
        {
            fc.actions
                .push(Action::TaskView(TaskViewOp::SetOutlineLevel(None)));
        }
    });
    ui.separator();
    let hide = Some(UiBinding::HideSubtasks);
    view_entry(
        ui,
        fc,
        TextId::MenuHideSubtasks,
        hide,
        has_doc,
        TaskViewOp::HideSubtasks,
    );
    let show = Some(UiBinding::ShowSubtasks);
    view_entry(
        ui,
        fc,
        TextId::MenuShowSubtasks,
        show,
        has_doc,
        TaskViewOp::ShowSubtasks,
    );
}

/// Liste « Filtre ▾ » : préréglages, filtres à fenêtre, réappliquer,
/// effacer (entrées du menu Affichage → Filtre).
fn filter_entries(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let has_doc = fc.doc.is_some();
    let texts = fc.texts;
    ui.add_enabled_ui(has_doc, |ui| {
        for (preset, label) in super::task_view::PRESETS {
            let current = fc.task_view.state.filter == FilterChoice::Preset(preset.clone());
            if ui
                .selectable_label(has_doc && current, texts.get(label))
                .clicked()
            {
                fc.actions
                    .push(Action::TaskView(TaskViewOp::SetFilter(FilterChoice::Preset(
                        preset,
                    ))));
            }
        }
    });
    ui.separator();
    for (label, mode) in [
        (TextId::FilterUsingResource, FilterDialogMode::Resource),
        (TextId::FilterDateRange, FilterDialogMode::DateRange),
        (TextId::FilterCustom, FilterDialogMode::Custom),
    ] {
        view_entry(ui, fc, label, None, has_doc, TaskViewOp::OpenFilterDialog(mode));
    }
    ui.separator();
    let filtered = has_doc && fc.task_view.state.is_filtered();
    view_entry(
        ui,
        fc,
        TextId::FilterReapply,
        None,
        filtered,
        TaskViewOp::ReapplyFilter,
    );
    let clear = Some(UiBinding::ClearFilter);
    view_entry(
        ui,
        fc,
        TextId::FilterClear,
        clear,
        filtered,
        TaskViewOp::ClearFilter,
    );
}

/// Choix exclusif : une étiquette sélectionnable ; rend `true` au clic. Le
/// nom accessible est le libellé traduit, avec l'état sélectionné.
fn choice(ui: &mut egui::Ui, selected: bool, text: &str) -> bool {
    let enabled = ui.is_enabled();
    let response = ui.selectable_label(selected, text);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, text));
    response.clicked()
}

/// Zoom exclusif Jour / Semaine / Mois / Trimestre ; aucun n'est surligné
/// sans document (l'ouverture choisira le zoom ajusté).
fn zoom_choices(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let has_doc = fc.doc.is_some();
    for zoom in TimeScaleZoom::ALL {
        if choice(ui, has_doc && fc.state.zoom == zoom, &fc.texts.get(zoom.label())) {
            fc.actions.push(Action::SetZoom(zoom));
        }
    }
}

/// Vue exclusive : Diagramme de Gantt / Gantt de suivi (J7, lot L7,
/// H-UI-22 ; style du Gantt, qui ramène à la vue Gantt) / Feuille des
/// ressources (lot L10) / Utilisation des ressources (lot L11) ; aucune
/// n'est surlignée sans document. Menu Affichage et liste « Diagramme de
/// Gantt ▾ » du ruban (commandes partagées, H-UI-70).
fn view_choices(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    for cmd in VIEW_CHOICES {
        if choice(ui, cmd.view_selected(fc), &fc.texts.get(cmd.menu_label())) {
            push_cmd(fc, cmd);
        }
    }
}

/// Thème exclusif Système / Clair / Sombre.
fn theme_choices(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    for (pref, label) in THEME_CHOICES {
        if choice(ui, fc.state.theme == pref, &fc.texts.get(label)) {
            fc.actions.push(Action::SetTheme(pref));
        }
    }
}

/// Liste « Thème » (menu Affichage et liste du ruban) : thèmes, puis
/// « Réduire les animations » (lot A1b, H-UI-147).
fn theme_menu(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    theme_choices(ui, fc);
    ui.separator();
    let mut reduce = fc.state.reduce_motion;
    if ui
        .checkbox(&mut reduce, fc.texts.get(TextId::ReduceMotion))
        .clicked()
    {
        fc.actions.push(Action::SetReduceMotion(reduce));
    }
}

/// Langue exclusive Système / Français / English.
fn language_choices(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    for (pref, label) in LANGUAGE_CHOICES {
        if choice(ui, fc.state.language == pref, &fc.texts.get(label)) {
            fc.actions.push(Action::SetLanguage(pref));
        }
    }
}

/// Écran d'accueil (aucun document ouvert) : titre, boutons d'ouverture et
/// de création,
/// choix de la langue et du thème.
pub fn welcome(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let texts = fc.texts;
    ui.vertical_centered(|ui| {
        ui.add_space(fc.metrics.header_h);
        ui.heading(texts.get_args(
            TextId::WelcomeTitle,
            &[("product", Arg::Str(plannercraft_core::PRODUCT_NAME.into()))],
        ));
        ui.label(texts.get(TextId::WelcomeSubtitle));
        ui.add_space(fc.metrics.row_h);
        let big = egui::vec2(fc.metrics.header_h * 8.0, fc.metrics.header_h);
        let demo = ui.add(egui::Button::new(texts.get(TextId::WelcomeOpenDemo)).min_size(big));
        // Focus par défaut sur « Ouvrir l'exemple » (Entrée l'active), sans
        // le voler à un autre widget déjà focalisé.
        if ui.memory(|memory| memory.focused().is_none()) {
            demo.request_focus();
        }
        if demo.clicked() {
            fc.actions.push(Action::OpenDemo);
        }
        if ui
            .add(egui::Button::new(texts.get(TextId::WelcomeOpenFile)).min_size(big))
            .clicked()
        {
            fc.actions.push(Action::OpenDialog);
        }
        // « Nouveau projet » : sur le web, Ctrl+N appartient au navigateur
        // (relecture finale J4, F5).
        if ui
            .add(egui::Button::new(texts.get(TextId::WelcomeNewProject)).min_size(big))
            .clicked()
        {
            fc.actions.push(Action::NewProject);
        }
        if cfg!(target_arch = "wasm32") {
            ui.label(texts.get(TextId::WelcomeWebNote));
        }
        ui.add_space(fc.metrics.row_h);
        ui.label(texts.get(TextId::WelcomeLanguage));
        let labels = LANGUAGE_CHOICES.map(|(_, label)| texts.get(label));
        centered_row(ui, &labels, |ui| language_choices(ui, fc));
        ui.label(texts.get(TextId::WelcomeTheme));
        let labels = THEME_CHOICES.map(|(_, label)| texts.get(label));
        centered_row(ui, &labels, |ui| theme_choices(ui, fc));
    });
}

/// Rangée de choix centrée : egui ne connaît la largeur d'une rangée
/// horizontale qu'après l'avoir dessinée ; on la mesure donc d'avance
/// (galleys des libellés, marges des boutons, espacement).
fn centered_row(ui: &mut egui::Ui, labels: &[String], add_contents: impl FnOnce(&mut egui::Ui)) {
    let spacing = ui.spacing().item_spacing.x;
    let padding = ui.spacing().button_padding.x * 2.0;
    let mut width = 0.0_f32;
    for label in labels {
        let galley = egui::WidgetText::from(label.as_str()).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Button,
        );
        width += galley.size().x + padding + spacing;
    }
    ui.horizontal(|ui| {
        let pad = f32::midpoint(ui.available_width() - width, spacing).max(0.0);
        if pad.is_finite() {
            ui.add_space(pad);
        }
        add_contents(ui);
    });
}

/// Barre d'état (nom précédé de « * » si le document est modifié, nombre de
/// tâches, plage, diagnostics, bouton de l'historique des messages), puis
/// bandeau des décisions (`Blocking`, sous le ruban), notifications (en
/// surimpression, en bas à droite au-dessus de la barre) et fenêtre de
/// l'historique (H-UI-104). Les notifications ne déplacent aucune vue.
pub fn status_bar(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, messages: &MessageStore) {
    // Interface adoucie (lot U1, H-UI-167) : barre d'état sur le fond de
    // l'application, sous la surface des vues.
    let app_bg = crate::theme::ChromeTokens::for_theme(ui.ctx().theme()).app;
    let frame = egui::Frame::side_top_panel(ui.style()).fill(app_bg);
    let bar = egui::Panel::bottom("plannercraft_status_bar")
        .frame(frame)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Bouton de l'historique à droite, seulement s'il y a un
                // message (barre inchangée sinon).
                if messages.history().is_empty() {
                    status_contents(ui, fc);
                } else {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        history_button(ui, fc, messages);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            status_contents(ui, fc);
                        });
                    });
                }
            });
        });
    let bar_h = bar.response.rect.height();
    let bar_h = if bar_h.is_finite() {
        bar_h.clamp(0.0, 200.0)
    } else {
        0.0
    };
    message_banner(ui, fc, messages.banner());
    let ctx = ui.ctx().clone();
    let hovered = if messages.history_open() {
        history_window(&ctx, fc, messages, bar_h);
        None
    } else {
        toasts(&ctx, fc, messages.toasts(), bar_h)
    };
    ctx.data_mut(|data| data.insert_temp(hovered_toast_id(), hovered));
    // Repeint à l'échéance de la prochaine notification (aucun repeint
    // immédiat : l'application reste au repos entre deux échéances).
    let now = ctx.input(|input| input.time);
    if let Some(left) = messages.next_expiry(now)
        && let Ok(delay) = std::time::Duration::try_from_secs_f64(left + TOAST_REPAINT_SLACK)
    {
        ctx.request_repaint_after(delay);
    }
}

/// Contenu de la barre d'état (document, tâches, plage, diagnostics,
/// filtre).
fn status_contents(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>) {
    let texts = fc.texts;
    let Some(doc) = fc.doc else {
        ui.label(texts.get(TextId::StatusNoDocument));
        return;
    };
    // « * » si modifié, comme le titre de la fenêtre : sur le web,
    // seul indicateur visible dans la page (relecture finale J4, F4).
    ui.strong(crate::app::document_label(doc));
    ui.separator();
    let count = fc
        .rows
        .map(crate::rows::RowModel::len)
        .filter(|&len| len > 0)
        .unwrap_or_else(|| doc.project().tasks.len());
    let count = i64::try_from(count).unwrap_or(i64::MAX);
    ui.label(texts.get_args(TextId::StatusTasks, &[("count", Arg::Count(count))]));
    ui.separator();
    let labels = texts.format_labels();
    ui.label(texts.get_args(
        TextId::StatusRange,
        &[
            (
                "start",
                Arg::Str(format_date(doc.schedule().project_start, labels).into()),
            ),
            (
                "finish",
                Arg::Str(format_date(doc.schedule().project_finish, labels).into()),
            ),
        ],
    ));
    ui.separator();
    // Cliquer sur un compteur ouvre ou referme le panneau des
    // diagnostics. Compteurs calculés une fois par document (cache
    // d'`App`, `DiagnosticsView`).
    let errors = i64::try_from(fc.diagnostics.errors).unwrap_or(i64::MAX);
    let warnings = i64::try_from(fc.diagnostics.warnings).unwrap_or(i64::MAX);
    let counters = [
        (
            TextId::StatusErrors,
            errors,
            ui.visuals().error_fg_color,
            Severity::Error,
        ),
        (
            TextId::StatusWarnings,
            warnings,
            ui.visuals().warn_fg_color,
            Severity::Warning,
        ),
    ];
    // H-UI-119 (lot L10) : une pastille ouvre le volet filtré sur sa
    // gravité ; la même pastille, volet ouvert, le referme ; l'autre change
    // le filtre. Volet fermé : filtre retiré (la case du menu Affichage
    // l'ouvre sur tous les diagnostics).
    let open = fc.state.diagnostics_open;
    if !open {
        super::diagnostics::closed(ui.ctx());
    }
    let filter = super::diagnostics::filter(ui.ctx());
    // Interface adoucie (lot U1, H-UI-171) : sans erreur ni avertissement,
    // un point vert et « Planning cohérent » (même clic que l'ancienne
    // pastille « aucune erreur » : ouvre ou referme le panneau) ; sinon les
    // seuls compteurs non nuls, mêmes clics qu'avant.
    if errors == 0 && warnings == 0 {
        let ok = crate::theme::ChromeTokens::for_theme(ui.ctx().theme()).ok;
        let (dot, _) = ui.allocate_exact_size(egui::vec2(STATUS_DOT, STATUS_DOT), egui::Sense::hover());
        ui.painter().circle_filled(dot.center(), STATUS_DOT / 2.0, ok);
        if ui
            .selectable_label(open, texts.get(TextId::StatusConsistent))
            .clicked()
        {
            super::diagnostics::set_filter(ui.ctx(), None);
            fc.actions.push(Action::ToggleDiagnostics);
        }
    }
    for (id, count, color, severity) in counters {
        if count == 0 {
            continue;
        }
        let text = egui::RichText::new(texts.get_args(id, &[("count", Arg::Count(count))])).color(color);
        let selected = open && filter.is_none_or(|f| f == severity);
        if ui.selectable_label(selected, text).clicked() {
            if open && filter == Some(severity) {
                fc.actions.push(Action::ToggleDiagnostics);
            } else {
                super::diagnostics::set_filter(ui.ctx(), Some(severity));
                if !open {
                    fc.actions.push(Action::ToggleDiagnostics);
                }
            }
        }
    }
    // --- J8 L9 : « Filtre : Critiques » (rien sans filtre).
    super::task_view::status_indicator(ui, fc);
}

/// Diamètre du point vert de « Planning cohérent » (lot U1, H-UI-171).
const STATUS_DOT: f32 = 7.0;

/// Largeur maximale d'une notification (H-UI-104).
pub const TOAST_W: f32 = 420.0;

/// Bouton qui ferme une notification (signe de multiplication U+00D7,
/// symbole et non texte : rien à traduire).
pub const TOAST_CLOSE_GLYPH: &str = "\u{d7}";

/// Marge entre les notifications et le bord de la fenêtre ou la barre
/// d'état.
const TOAST_MARGIN: f32 = 12.0;

/// Marge ajoutée au délai de repeint, pour que l'image tombe après
/// l'échéance.
const TOAST_REPAINT_SLACK: f64 = 0.05;

/// Clé de la mémoire d'egui où la zone des notifications dépose la
/// notification survolée de l'image (survol = pause, relu par `App` à
/// l'image suivante).
fn hovered_toast_id() -> egui::Id {
    egui::Id::unique("plannercraft.toast-hovered")
}

/// Notification survolée à l'image précédente, s'il y en a une.
#[must_use]
pub fn hovered_toast(ctx: &egui::Context) -> Option<u64> {
    ctx.data(|data| data.get_temp::<Option<u64>>(hovered_toast_id()))
        .flatten()
}

/// Liseré et icône d'un message selon sa sorte (H-UI-104, jetons de
/// [`ToastTokens`]).
const fn toast_accent(kind: MessageKind, tokens: &ToastTokens) -> (egui::Color32, Icon) {
    match kind {
        MessageKind::Refusal | MessageKind::Blocking => (tokens.error, Icon::CircleX),
        MessageKind::Warning => (tokens.warning, Icon::TriangleAlert),
        MessageKind::Info => (tokens.info, Icon::Info),
    }
}

/// Côté de l'icône d'une notification.
const TOAST_ICON: f32 = 16.0;

/// Largeur du liseré gauche d'une notification.
const TOAST_STRIPE: f32 = 4.0;

/// Bouton de l'historique dans la barre d'état : « Messages », puis le
/// compte des non lus s'il y en a.
fn history_button(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, messages: &MessageStore) {
    let texts = fc.texts;
    let mut label = texts.get(TextId::NotifyHistory);
    let unread = messages.unread();
    if unread > 0 {
        let count = i64::try_from(unread).unwrap_or(i64::MAX);
        label.push_str(" · ");
        label.push_str(&texts.get_args(TextId::NotifyUnread, &[("count", Arg::Count(count))]));
    }
    if ui.selectable_label(messages.history_open(), label).clicked() {
        fc.actions.push(Action::Message(MessageOp::ToggleHistory));
    }
}

/// Corps d'un message : titre, texte, « Détails » repliable (fermé par
/// défaut) ; `salt` distingue les repliables d'une même voie.
fn message_body(ui: &mut egui::Ui, fc: &FrameCtx<'_>, message: &UserMessage, fg: egui::Color32, salt: &str) {
    let texts = fc.texts;
    if let Some(title) = message.title {
        ui.label(egui::RichText::new(texts.get(title)).strong().color(fg));
    }
    ui.label(egui::RichText::new(message.text.as_str()).color(fg));
    if let Some(detail) = message.detail.as_deref().filter(|d| !d.is_empty()) {
        egui::CollapsingHeader::new(texts.get(TextId::NotifyDetails))
            .id_salt((salt, message.id))
            .default_open(false)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(detail).small().monospace().color(fg));
            });
    }
}

/// Zone des notifications, en bas à droite au-dessus de la barre d'état
/// (`bar_h`), la plus récente en bas ; « × » sur chacune. Rend la
/// notification survolée.
fn toasts(ctx: &egui::Context, fc: &mut FrameCtx<'_>, toasts: &[UserMessage], bar_h: f32) -> Option<u64> {
    if toasts.is_empty() {
        return None;
    }
    let tokens = ToastTokens::of(fc.palette);
    let close = fc.texts.get(TextId::NotifyClose);
    let mut hovered = None;
    let mut dismissed = Vec::new();
    egui::Area::new(egui::Id::unique("plannercraft.toasts"))
        .order(egui::Order::Foreground)
        .anchor(
            egui::Align2::RIGHT_BOTTOM,
            egui::vec2(-TOAST_MARGIN, -(bar_h + TOAST_MARGIN)),
        )
        .show(ctx, |ui| {
            // Largeur fixe (≤ `TOAST_W`, bornée par la fenêtre) : cartes
            // alignées, quelle que soit la longueur de la phrase.
            let screen = ctx.content_rect().width();
            let width = if screen.is_finite() {
                (screen - 2.0 * TOAST_MARGIN).clamp(160.0, TOAST_W)
            } else {
                TOAST_W
            };
            ui.set_width(width);
            for toast in toasts {
                let (accent, icon) = toast_accent(toast.kind, &tokens);
                let frame = egui::Frame::new()
                    .fill(tokens.bg)
                    .stroke(egui::Stroke::new(1.0, tokens.border))
                    .corner_radius(egui::CornerRadius::same(6))
                    .shadow(egui::Shadow {
                        offset: [0, 3],
                        blur: 14,
                        spread: 0,
                        color: tokens.shadow,
                    })
                    .inner_margin(egui::Margin {
                        left: 14,
                        right: 6,
                        top: 8,
                        bottom: 8,
                    });
                let response = frame
                    .show(ui, |ui| {
                        ui.set_width(width - 22.0);
                        ui.horizontal_top(|ui| {
                            let (icon_rect, _) = ui.allocate_exact_size(
                                egui::vec2(TOAST_ICON, TOAST_ICON),
                                egui::Sense::hover(),
                            );
                            paint_icon(ui.painter(), icon_rect, icon, accent);
                            ui.add_space(4.0);
                            ui.vertical(|ui| {
                                ui.set_width((width - 22.0 - TOAST_ICON - 36.0).max(80.0));
                                message_body(ui, fc, toast, tokens.text, "plannercraft_toast_detail");
                            });
                            // « × » (H-UI-104) : nom distinct des « Fermer » des
                            // fenêtres ; le texte traduit en infobulle.
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                let close_button = egui::Button::new(
                                    egui::RichText::new(TOAST_CLOSE_GLYPH).color(tokens.text),
                                )
                                .frame(false)
                                .small();
                                if ui.add(close_button).on_hover_text(close.as_str()).clicked() {
                                    dismissed.push(toast.id);
                                }
                            });
                        });
                    })
                    .response;
                // Liseré gauche, couleur de la sorte (coins gauches arrondis
                // comme la carte).
                let card = response.rect;
                if card.is_finite() && card.is_positive() {
                    let stripe = egui::Rect::from_min_max(
                        card.left_top(),
                        egui::pos2(card.left() + TOAST_STRIPE, card.bottom()),
                    );
                    ui.painter().rect_filled(
                        stripe,
                        egui::CornerRadius {
                            nw: 6,
                            sw: 6,
                            ne: 0,
                            se: 0,
                        },
                        accent,
                    );
                }
                if ui.rect_contains_pointer(card) {
                    hovered = Some(toast.id);
                }
                ui.add_space(8.0);
            }
        });
    for id in dismissed {
        fc.actions.push(Action::Message(MessageOp::Dismiss(id)));
    }
    hovered
}

/// Fenêtre de l'historique (les [`crate::notify::MAX_HISTORY`] derniers
/// messages, le plus récent en haut, détails repliables), ancrée comme les
/// notifications.
fn history_window(ctx: &egui::Context, fc: &mut FrameCtx<'_>, messages: &MessageStore, bar_h: f32) {
    let texts = fc.texts;
    let mut open = true;
    egui::Window::new(texts.get(TextId::NotifyHistoryTitle))
        .id(egui::Id::unique("plannercraft.message-history"))
        .anchor(
            egui::Align2::RIGHT_BOTTOM,
            egui::vec2(-TOAST_MARGIN, -(bar_h + TOAST_MARGIN)),
        )
        .collapsible(false)
        .resizable(false)
        .default_width(TOAST_W)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_max_width(TOAST_W);
            if messages.history().is_empty() {
                ui.label(texts.get(TextId::NotifyHistoryEmpty));
                return;
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                let fg = ui.visuals().text_color();
                for message in messages.history().iter().rev() {
                    message_body(ui, fc, message, fg, "plannercraft_history_detail");
                    ui.separator();
                }
            });
        });
    if !open {
        fc.actions.push(Action::Message(MessageOp::ToggleHistory));
    }
}

/// Bandeau « Enregistrer les modifications de « nom » ? » (Enregistrer /
/// Ne pas enregistrer / Annuler), non bloquant : rien n'est dessiné si
/// `pending` vaut `None`. `pending` est le nom du document modifié.
pub fn confirm_banner(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, pending: Option<&str>) {
    let Some(name) = pending else {
        return;
    };
    let texts = fc.texts;
    let fg = fc.palette.text_strong;
    let frame = egui::Frame::side_top_panel(ui.style()).fill(fc.palette.message_warning_bg);
    egui::Panel::top("plannercraft_confirm_banner")
        .frame(frame)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let title = texts.get_args(TextId::ConfirmUnsavedTitle, &[("name", Arg::Str(name.into()))]);
                ui.label(egui::RichText::new(title).strong().color(fg));
                let choices = [
                    (TextId::ConfirmSave, ConfirmChoice::Save),
                    (TextId::ConfirmDiscard, ConfirmChoice::Discard),
                    (TextId::ConfirmCancel, ConfirmChoice::Cancel),
                ];
                for (label, choice) in choices {
                    if ui.button(texts.get(label)).clicked() {
                        fc.actions.push(Action::Confirm(choice));
                    }
                }
            });
        });
}

/// Les [`MAX_MESSAGES`] messages les plus récents (fin de la liste) : `App`
/// borne déjà sa liste, le bandeau la borne aussi à l'affichage pour ne
/// jamais manger la vue (D-fix-9).
fn recent_messages(messages: &[UserMessage]) -> &[UserMessage] {
    let skip = messages.len().saturating_sub(MAX_MESSAGES);
    messages.get(skip..).unwrap_or(messages)
}

/// Fond et liseré du bandeau selon la gravité la plus haute affichée.
fn banner_colors(
    has_error: bool,
    palette: &Palette,
    visuals: &egui::Visuals,
) -> (egui::Color32, egui::Color32) {
    if has_error {
        (palette.message_error_bg, palette.critical)
    } else {
        (palette.message_warning_bg, visuals.warn_fg_color)
    }
}

/// Bandeau des décisions (`Blocking` : ouverture, enregistrement ou
/// export échoués, état illisible), jamais de modale. Texte fort
/// (`text_strong`) sur fond teinté (contraste ≥ 4,5:1, testé dans `theme`),
/// liseré gauche coloré selon la gravité ; au plus [`MAX_MESSAGES`]
/// messages, les plus récents ; « Fermer le message » ne ferme que le
/// sien (H-UI-104).
fn message_banner(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, messages: &[UserMessage]) {
    let messages = recent_messages(messages);
    if messages.is_empty() {
        return;
    }
    let texts = fc.texts;
    let palette = fc.palette;
    let has_error = messages.iter().any(|message| message.severity == Severity::Error);
    let (fill, accent) = banner_colors(has_error, palette, ui.visuals());
    let fg = palette.text_strong;
    let frame = egui::Frame::side_top_panel(ui.style()).fill(fill);
    egui::Panel::top("plannercraft_message_banner")
        .frame(frame)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            if rect.is_finite() {
                let bar =
                    egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + 3.0, rect.bottom()));
                ui.painter()
                    .rect_filled(bar.expand2(egui::vec2(0.0, 2.0)), 0.0, accent);
            }
            for message in messages {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        message_body(ui, fc, message, fg, "plannercraft_banner_detail");
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(texts.get(TextId::MessageDismiss)).clicked() {
                            fc.actions.push(Action::Message(MessageOp::Dismiss(message.id)));
                        }
                    });
                });
            }
        });
}

// --- J8 L10 : couper, copier, coller ---

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_core::Severity;

    use super::recent_messages;
    use crate::app::{MAX_MESSAGES, UserMessage};

    fn message(n: usize) -> UserMessage {
        UserMessage::new(
            crate::notify::MessageKind::Warning,
            Severity::Warning,
            None,
            n.to_string(),
        )
    }

    #[test]
    fn banner_keeps_most_recent_messages() {
        let none: Vec<UserMessage> = Vec::new();
        assert_eq!(recent_messages(&none).len(), 0);
        let two: Vec<UserMessage> = (0..2).map(message).collect();
        assert_eq!(recent_messages(&two), two.as_slice());
        let forty: Vec<UserMessage> = (0..40).map(message).collect();
        let shown = recent_messages(&forty);
        assert_eq!(shown.len(), MAX_MESSAGES);
        assert_eq!(shown.first().unwrap().text, (40 - MAX_MESSAGES).to_string());
        assert_eq!(shown.last().unwrap().text, "39");
    }
}
