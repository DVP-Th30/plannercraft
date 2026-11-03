//! Application desktop de PlannerCraft (macOS, Windows, Linux).
//!
//! Ouvre une fenêtre eframe titrée avec le nom du produit et y affiche
//! l'interface de `plannercraft-ui-egui`. C'est ici seulement que l'horloge et la
//! langue du système sont lues, puis injectées (`Env`). Une erreur
//! d'initialisation graphique est affichée sur la sortie d'erreur et rend un
//! code de sortie non nul : jamais de panique (règle 10).
//!
//! Canal de contrôle (ADR-0002, ADR-0015 D9) : après l'ouverture d'un fichier
//! par chemin, l'application lance l'écouteur du relais (une fois) et publie
//! son rendez-vous : `plannercraft mcp` s'y rattache et relaie ses appels, que
//! l'application exécute sur la session du document (toute écriture après
//! validation par le bandeau) ; en `--headless`, il refuse d'écrire le même
//! fichier (`file.open_in_app`). Les appels arrivent dans une file bornée
//! vidée par l'interface en début d'image ; l'écouteur la réveille par
//! `request_repaint`.
//!
//! Ligne de commande (J7, lot L14) : `plannercraft-desktop [plan.xml]` ouvre le
//! fichier au démarrage ([`startup_path`], chemin rendu absolu pour les
//! fichiers récents) ; un échec s'affiche dans le bandeau de l'interface,
//! nom du fichier compris. La capture relayée (`app.screenshot`) est
//! permise ici seulement ([`App::enable_screenshots`]).
//!
//! Polices : eframe est pris avec ses features par défaut, dont
//! `system_fonts` ; `NativeOptions::system_font_fallback` reste vrai
//! ([`native_options`]) : un caractère absent des polices d'egui (CJK,
//! arabe…) est cherché dans les polices du système (fontconfig, CoreText,
//! DirectWrite), sans police embarquée (ADR-0015, décision J8).
//!
//! Reprise après plantage (J7, lot L15) : au démarrage, les journaux
//! orphelins du dossier de reprise (`recovery_dir`) qui portent des
//! modifications sont proposés par un bandeau (Restaurer, Ignorer, Plus
//! tard), les autres supprimés ; puis chaque document ouvert tient son
//! journal ([`Recorder`]), supprimé à sa fermeture propre. Un plantage, ou
//! « Quitter » du Dock sur macOS (non interceptable, voir plus bas), laisse
//! le journal : rien n'est perdu au démarrage suivant.
//!
//! macOS : le menu applicatif par défaut de winit est retiré
//! ([`event_loop_hook`]). Son entrée « Quitter » (Cmd+Q) appelle
//! `terminate:`, qui ferme la fenêtre sans `CloseRequested` : le garde des
//! modifications non enregistrées ne la voyait pas. À sa place, un menu
//! applicatif natif minimal (ADR-0019 D4, H-UI-108, crate `muda`) est posé
//! à la création de l'application ([`mac_menu`]) : menu « PlannerCraft » (À propos,
//! Masquer ⌘H, Masquer les autres ⌥⌘H, Tout afficher, Quitter ⌘Q) et menu
//! Fenêtre (Réduire ⌘M, Zoom). « Quitter PlannerCraft » est un élément
//! **personnalisé** : son clic (ou ⌘Q, pris par le menu avant la fenêtre)
//! passe par [`App::shell_quit`], donc par la même garde que Ctrl/Cmd+Q
//! (saisie validée, fenêtre fermée, bandeau « Enregistrer les
//! modifications ? »), jamais par `terminate:`. Les autres éléments sont
//! les éléments prédéfinis du système (masquer, réduire, zoom). Échec de
//! création du menu : journal sur la sortie d'erreur, application sans
//! menu (comportement de J7), jamais de panique. « Quitter » du Dock et la
//! fermeture de session appellent toujours `terminate:` : non
//! interceptables sans délégué Objective-C (limite, docs/features/ui.md § 5).
//!
//! Journal des touches (`PLANNERCRAFT_LOG_KEYS=1`, H-UI-108) : un journaliseur
//! minimal ([`install_key_log`]) imprime sur la sortie d'erreur les appuis
//! reçus par l'interface (cible `plannercraft::keys`) et les clics du menu natif,
//! pour diagnostiquer un raccourci muet sur Mac
//! (`PLANNERCRAFT_LOG_KEYS=1 /Applications/PlannerCraft.app/Contents/MacOS/plannercraft-desktop`).
//!
//! Version (J8, lot L1, C-REL-03) : `plannercraft-desktop --version` (ou `-V`)
//! imprime `PlannerCraft <version>` et sort avant toute fenêtre
//! ([`version_requested`]). Sous Windows, le binaire est du sous-système
//! fenêtré (aucune console ne s'ouvre au double-clic) : la ligne n'est
//! visible que si la sortie est redirigée (`plannercraft-desktop --version | more`,
//! ce que fait la CI) ; la CLI `plannercraft.exe`, binaire console, l'imprime
//! toujours.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use plannercraft_automation::control::{
    ControlError, ListenerHandle, RELAY_QUEUE, RelayCall, Rendezvous, RendezvousGuard, Waker,
    canonical_project, new_token, publish_in, spawn_listener_in, state_dir,
};
use plannercraft_automation::recovery::{Recorder, orphans, recovery_dir};
use plannercraft_core::{LEGACY_PRODUCT_ID, Moment, PRODUCT_NAME, PRODUCT_VERSION};
use plannercraft_ui_egui::i18n::{Arg, TextId, Texts};
use plannercraft_ui_egui::relay::ScreenshotConfig;
use plannercraft_ui_egui::{
    App, DocumentObserver, Env, Language, OpenDocument, PlannerCraftApp, RfdFileService,
};

/// « Maintenant » en heure locale, tronqué à la minute. Hors du domaine des
/// dates du cœur (1984-2149), repli sur un instant fixe, signalé.
fn now() -> Moment {
    match Moment::from_naive_floor(chrono::Local::now().naive_local()) {
        Ok((moment, _truncated)) => moment,
        Err(error) => {
            eprintln!(
                "{PRODUCT_NAME} : date système inutilisable ({error}), repli sur {}",
                Moment::MIN
            );
            Moment::MIN
        }
    }
}

/// Rendez-vous du canal de contrôle, suivi du document ouvert.
struct ControlRendezvous {
    /// Dossier des rendez-vous ; `None` : celui de la plateforme
    /// (`state_dir`, lu au premier besoin). Imposé par les tests.
    dir: Option<PathBuf>,
    /// File des appels relayés, vidée par l'application.
    inbox: SyncSender<RelayCall>,
    /// Réveil de l'interface à chaque appel.
    wake: Waker,
    /// Rendez-vous publié, supprimé à la destruction (avant l'arrêt de
    /// l'écouteur : ordre de déclaration).
    guard: Option<RendezvousGuard>,
    /// Écouteur, lancé à la première ouverture d'un fichier par chemin.
    listener: Option<ListenerHandle>,
    /// L'écouteur n'a pas pu démarrer : pas de nouvel essai (signalé une
    /// fois).
    listener_failed: bool,
    /// Chemin (tel qu'ouvert) du document dont le rendez-vous est publié.
    path: Option<String>,
    /// Numéro d'ouverture du dernier document vu : un document neuf change
    /// le jeton de l'écouteur (relecture finale J7, relais F2).
    opening: Option<u64>,
}

impl ControlRendezvous {
    /// Rendez-vous sans écouteur, et la file que l'application vide.
    fn new(wake: Waker) -> (Self, Receiver<RelayCall>) {
        let (inbox, rx) = sync_channel(RELAY_QUEUE);
        let control = Self {
            dir: None,
            inbox,
            wake,
            guard: None,
            listener: None,
            listener_failed: false,
            path: None,
            opening: None,
        };
        (control, rx)
    }

    /// Dossier des rendez-vous (créé au besoin, en 0700, par le canal).
    fn dir(&mut self) -> Result<PathBuf, ControlError> {
        if self.dir.is_none() {
            self.dir = state_dir();
        }
        self.dir.clone().ok_or(ControlError::NoStateDir)
    }

    /// Rendez-vous de l'écouteur pour `project`, l'écouteur étant lancé au
    /// premier besoin (un échec est signalé une fois, sans nouvel essai).
    fn rendezvous(&mut self, project: String) -> Option<Rendezvous> {
        if self.listener.is_none() && !self.listener_failed {
            let inbox = self.inbox.clone();
            let wake = Arc::clone(&self.wake);
            let started = self
                .dir()
                .and_then(|dir| new_token().and_then(|token| spawn_listener_in(&dir, token, inbox, wake)));
            match started {
                Ok(listener) => self.listener = Some(listener),
                Err(error) => {
                    self.listener_failed = true;
                    eprintln!("{PRODUCT_NAME} : canal de contrôle indisponible ({error})");
                }
            }
        }
        self.listener
            .as_ref()
            .map(|listener| listener.rendezvous(Some(project)))
    }

    /// Publie le rendez-vous de `path` (le précédent est retiré).
    fn publish_for(&mut self, path: Option<&str>) {
        self.guard = None;
        self.path = path.map(str::to_owned);
        let Some(project) = path.and_then(|path| canonical_project(Path::new(path))) else {
            return;
        };
        let Some(rendezvous) = self.rendezvous(project) else {
            return;
        };
        match self.dir().and_then(|dir| publish_in(&dir, &rendezvous)) {
            Ok(guard) => self.guard = Some(guard),
            Err(error) => eprintln!("{PRODUCT_NAME} : rendez-vous non publié ({error})"),
        }
    }
}

impl DocumentObserver for ControlRendezvous {
    /// Rendez-vous du chemin du document. Un document **neuf** (autre
    /// numéro d'ouverture : autre fichier, même fichier rouvert, « Nouveau »)
    /// change le jeton de l'écouteur avant la publication : un agent
    /// rattaché au document précédent est refusé
    /// (`control.document_changed`) au lieu d'agir sur celui-ci par les
    /// mêmes UID ; « Enregistrer sous » (même ouverture) garde le jeton.
    fn document_changed(&mut self, document: Option<&OpenDocument>) {
        let path = document.and_then(|document| document.origin.path.as_deref());
        let opening = document.map(|document| document.opening);
        let mut rotated = false;
        if opening.is_some() && opening != self.opening {
            if self.opening.is_some()
                && let Some(listener) = &mut self.listener
            {
                match listener.rotate_token() {
                    Ok(()) => rotated = true,
                    Err(error) => eprintln!("{PRODUCT_NAME} : jeton du canal non renouvelé ({error})"),
                }
            }
            self.opening = opening;
        }
        if rotated || path != self.path.as_deref() {
            self.publish_for(path);
        }
    }
}

/// Réglage de la boucle d'événements : sur macOS, pas de menu applicatif
/// par défaut (voir l'en-tête du module) ; rien ailleurs.
#[cfg_attr(
    target_os = "macos",
    allow(
        clippy::unnecessary_wraps,
        reason = "`None` hors macOS : même signature sur toutes les plateformes"
    )
)]
fn event_loop_hook() -> Option<eframe::EventLoopBuilderHook> {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS as _;
        Some(Box::new(|builder| {
            builder.with_default_menu(false);
        }))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Élément du menu applicatif natif (macOS), décrit sans `muda` pour être
/// testé sur toutes les plateformes ([`menu_spec`]).
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "menu natif : macOS seulement, décrit et testé partout")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuItemKind {
    /// « À propos de PlannerCraft » (panneau standard du système).
    About,
    /// « Masquer PlannerCraft » (⌘H, élément du système).
    Hide,
    /// « Masquer les autres » (⌥⌘H, élément du système).
    HideOthers,
    /// « Tout afficher » (élément du système).
    ShowAll,
    /// « Quitter PlannerCraft » (⌘Q) : élément **personnalisé**, jamais
    /// `terminate:` ([`App::shell_quit`]).
    Quit,
    /// « Réduire » (⌘M, élément du système).
    Minimize,
    /// « Zoom » (élément du système).
    Zoom,
    /// Séparateur.
    Separator,
}

/// Une entrée de menu : genre, libellé traduit et raccourci affiché par le
/// système (vide : aucun).
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "menu natif : macOS seulement, décrit et testé partout")
)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct MenuEntry {
    kind: MenuItemKind,
    label: String,
    shortcut: &'static str,
}

/// Menu applicatif natif : menu « PlannerCraft » et menu « Fenêtre ».
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "menu natif : macOS seulement, décrit et testé partout")
)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct MenuSpec {
    app_title: String,
    app: Vec<MenuEntry>,
    window_title: String,
    window: Vec<MenuEntry>,
}

/// Identifiant de l'élément personnalisé « Quitter PlannerCraft ».
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "menu natif : macOS seulement, décrit et testé partout")
)]
const QUIT_MENU_ID: &str = "plannercraft.quit";

/// Description du menu natif dans `language` (H-UI-108, H-UI-109 ; libellés
/// des fichiers `.ftl`, `mac-menu-*`).
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "menu natif : macOS seulement, décrit et testé partout")
)]
fn menu_spec(language: Language) -> MenuSpec {
    let texts = Texts::new(language);
    let text = |id: TextId| texts.get_args(id, &[("product", Arg::Str(PRODUCT_NAME.into()))]);
    let entry = |kind: MenuItemKind, id: TextId, shortcut: &'static str| MenuEntry {
        kind,
        label: text(id),
        shortcut,
    };
    let separator = || MenuEntry {
        kind: MenuItemKind::Separator,
        label: String::new(),
        shortcut: "",
    };
    MenuSpec {
        app_title: PRODUCT_NAME.to_owned(),
        app: vec![
            entry(MenuItemKind::About, TextId::MacMenuAbout, ""),
            separator(),
            entry(MenuItemKind::Hide, TextId::MacMenuHide, "⌘H"),
            entry(MenuItemKind::HideOthers, TextId::MacMenuHideOthers, "⌥⌘H"),
            entry(MenuItemKind::ShowAll, TextId::MacMenuShowAll, ""),
            separator(),
            entry(MenuItemKind::Quit, TextId::MacMenuQuit, "⌘Q"),
        ],
        window_title: text(TextId::MacMenuWindow),
        window: vec![
            entry(MenuItemKind::Minimize, TextId::MacMenuMinimize, "⌘M"),
            entry(MenuItemKind::Zoom, TextId::MacMenuZoom, ""),
        ],
    }
}

/// Menu applicatif natif de macOS (`muda`), construit depuis [`MenuSpec`].
#[cfg(target_os = "macos")]
mod mac_menu {
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use plannercraft_core::{PRODUCT_NAME, PRODUCT_VERSION};

    use super::{MenuEntry, MenuItemKind, MenuSpec, QUIT_MENU_ID};

    /// Menu posé sur l'application ; gardé vivant tant qu'elle tourne.
    pub struct NativeMenu {
        _menu: Menu,
        _window: Submenu,
    }

    /// Ajoute `entry` à `submenu`.
    fn append(submenu: &Submenu, entry: &MenuEntry) -> muda::Result<()> {
        let label = Some(entry.label.as_str());
        match entry.kind {
            MenuItemKind::About => {
                let metadata = AboutMetadata {
                    name: Some(PRODUCT_NAME.to_owned()),
                    version: Some(PRODUCT_VERSION.to_owned()),
                    ..AboutMetadata::default()
                };
                submenu.append(&PredefinedMenuItem::about(label, Some(metadata)))
            }
            MenuItemKind::Hide => submenu.append(&PredefinedMenuItem::hide(label)),
            MenuItemKind::HideOthers => submenu.append(&PredefinedMenuItem::hide_others(label)),
            MenuItemKind::ShowAll => submenu.append(&PredefinedMenuItem::show_all(label)),
            MenuItemKind::Minimize => submenu.append(&PredefinedMenuItem::minimize(label)),
            MenuItemKind::Zoom => submenu.append(&PredefinedMenuItem::zoom(label)),
            MenuItemKind::Separator => submenu.append(&PredefinedMenuItem::separator()),
            MenuItemKind::Quit => submenu.append(&MenuItem::with_id(
                QUIT_MENU_ID,
                &entry.label,
                true,
                Some(Accelerator::new(Modifiers::META, Code::KeyQ)),
            )),
        }
    }

    /// Pose le menu `spec` sur l'application et route « Quitter » vers
    /// `on_quit` (fil principal, appelé par le système à chaque clic ou ⌘Q).
    /// Hors du fil principal : rien (muda l'exige), signalé.
    ///
    /// # Errors
    ///
    /// Refus de `muda` (élément non ajouté) : message à journaliser.
    pub fn install(
        spec: &MenuSpec,
        on_quit: impl Fn() + Send + Sync + 'static,
    ) -> Result<NativeMenu, String> {
        if std::thread::current().name() != Some("main") {
            return Err("menu natif hors du fil principal".to_owned());
        }
        let build = || -> muda::Result<NativeMenu> {
            let app = Submenu::new(&spec.app_title, true);
            for entry in &spec.app {
                append(&app, entry)?;
            }
            let window = Submenu::new(&spec.window_title, true);
            for entry in &spec.window {
                append(&window, entry)?;
            }
            let menu = Menu::new();
            menu.append_items(&[&app, &window])?;
            menu.init_for_nsapp();
            window.set_as_windows_menu_for_nsapp();
            Ok(NativeMenu {
                _menu: menu,
                _window: window,
            })
        };
        let menu = build().map_err(|error| error.to_string())?;
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            log::info!(target: "plannercraft::keys", "menu natif : {}", event.id().as_ref());
            if event.id == QUIT_MENU_ID {
                on_quit();
            }
        }));
        Ok(menu)
    }
}

/// Journaliseur minimal du diagnostic des touches : cibles `plannercraft::*`,
/// niveau `Info` et au-dessus, sur la sortie d'erreur (une erreur
/// d'écriture est ignorée : jamais de panique).
struct KeyLog;

impl log::Log for KeyLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info && metadata.target().starts_with("plannercraft::")
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            use std::io::Write as _;
            let _ = writeln!(std::io::stderr(), "[{}] {}", record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Le journal des touches est-il demandé (`PLANNERCRAFT_LOG_KEYS=1`) ?
fn key_log_requested(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| value == "1")
}

/// Installe [`KeyLog`] si `PLANNERCRAFT_LOG_KEYS=1` (H-UI-108). Un autre
/// journaliseur déjà installé est gardé.
fn install_key_log() {
    static LOGGER: KeyLog = KeyLog;
    if key_log_requested(std::env::var_os("PLANNERCRAFT_LOG_KEYS").as_deref())
        && log::set_logger(&LOGGER).is_ok()
    {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// Application desktop : l'interface de `plannercraft-ui-egui` et, sur macOS, le
/// menu natif dont « Quitter » arrive par `shell` (une demande par clic,
/// lue à chaque image : [`App::shell_quit`]).
struct DesktopApp {
    inner: PlannerCraftApp,
    /// Demandes « Quitter » du menu natif.
    shell: Option<Receiver<()>>,
    /// Menu natif posé (macOS) ; gardé vivant avec l'application.
    #[cfg(target_os = "macos")]
    _menu: Option<mac_menu::NativeMenu>,
}

impl DesktopApp {
    /// Transmet à l'interface les « Quitter » du menu natif reçus depuis
    /// la dernière image.
    fn poll_shell(&mut self) {
        let Some(shell) = &self.shell else {
            return;
        };
        while shell.try_recv().is_ok() {
            self.inner.app_mut().shell_quit();
        }
    }
}

impl eframe::App for DesktopApp {
    fn raw_input_hook(&mut self, ctx: &eframe::egui::Context, raw_input: &mut eframe::egui::RawInput) {
        self.inner.raw_input_hook(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
        self.poll_shell();
        self.inner.ui(ui, frame);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.inner.save(storage);
    }

    fn persist_egui_memory(&self) -> bool {
        self.inner.persist_egui_memory()
    }
}

/// Pose le menu natif (macOS) dans la langue de l'interface au démarrage ;
/// rend le récepteur de ses « Quitter ». Ailleurs : rien.
#[cfg(target_os = "macos")]
fn native_menu(
    ctx: &eframe::egui::Context,
    language: Language,
) -> (Option<Receiver<()>>, Option<mac_menu::NativeMenu>) {
    let (tx, rx) = sync_channel::<()>(8);
    let ctx = ctx.clone();
    let on_quit = move || {
        // File pleine : une demande est déjà en attente, rien de perdu.
        let _ = tx.try_send(());
        ctx.request_repaint();
    };
    match mac_menu::install(&menu_spec(language), on_quit) {
        Ok(menu) => (Some(rx), Some(menu)),
        Err(error) => {
            eprintln!("{PRODUCT_NAME} : menu natif indisponible ({error})");
            (None, None)
        }
    }
}

/// Reprise après plantage : orphelins de `root` proposés (ceux sans
/// modification supprimés ; ceux d'une autre version ou trop grands gardés
/// et proposés, jamais supprimés sans l'accord de l'utilisateur), puis
/// journal de chaque document dans `root`.
fn start_recovery(app: &mut App, root: PathBuf) {
    let (offered, stale): (Vec<_>, Vec<_>) = orphans(&root)
        .into_iter()
        .partition(plannercraft_automation::recovery::Orphan::worth_offering);
    for orphan in stale {
        if let Err(error) = orphan.discard() {
            eprintln!("{PRODUCT_NAME} : journal de reprise non supprimé ({error})");
        }
    }
    app.offer_recovery(offered);
    app.set_recovery(Box::new(Recorder::in_dir(root)));
}

/// Usage affiché pour un argument refusé.
const USAGE: &str = "usage : plannercraft-desktop [fichier.xml | --version]";

/// Le seul argument est `--version` ou `-V` : la version est imprimée et
/// l'application sort sans ouvrir de fenêtre (C-REL-03).
fn version_requested(args: &[std::ffi::OsString]) -> bool {
    matches!(args, [only] if only.to_str().is_some_and(|arg| arg == "--version" || arg == "-V"))
}

/// Fichier à ouvrir au démarrage : premier argument, rendu absolu (les
/// fichiers récents restent valables quel que soit le dossier de
/// lancement). Aucun argument : `None`.
///
/// # Errors
///
/// Option inconnue, chemin non UTF-8 ou plus d'un argument : message à
/// afficher (l'application démarre quand même, sans fichier).
fn startup_path(mut args: impl Iterator<Item = std::ffi::OsString>) -> Result<Option<String>, String> {
    let Some(first) = args.next() else {
        return Ok(None);
    };
    if args.next().is_some() {
        return Err(format!("un seul fichier attendu\n{USAGE}"));
    }
    let path = PathBuf::from(&first);
    if first.to_str().is_some_and(|text| text.starts_with('-')) {
        return Err(format!("option inconnue : {}\n{USAGE}", path.display()));
    }
    let absolute = std::path::absolute(&path).unwrap_or(path);
    absolute
        .into_os_string()
        .into_string()
        .map(Some)
        .map_err(|raw| format!("chemin non UTF-8 refusé : {}", PathBuf::from(raw).display()))
}

/// Reprend les préférences d'interface d'une version antérieure nommée
/// `planrs` (renommage du 2026-11-03, `docs/rename.md`) : le dossier de
/// stockage d'eframe dépend du nom passé à `run_native` (`storage_dir`,
/// `~/.local/share/planrs`, `~/Library/Application Support/planrs`,
/// `%APPDATA%\planrs\data`). Son contenu (`app.ron`) passe dans le
/// nouveau dossier s'il n'y est pas déjà ; la clé d'état qu'il contient
/// (`planrs.ui_state`) est relue par `PlannerCraftApp::new`.
fn adopt_legacy_preferences(storage_dir: impl Fn(&str) -> Option<PathBuf>) -> std::io::Result<usize> {
    match (storage_dir(LEGACY_PRODUCT_ID), storage_dir(PRODUCT_NAME)) {
        (Some(old), Some(new)) => plannercraft_automation::files::adopt_legacy_dir(&old, &new),
        _ => Ok(0),
    }
}

/// Reprise de l'état d'une version antérieure, avant toute lecture :
/// préférences d'eframe d'abord, puis répertoire d'état (rendez-vous et
/// journaux de reprise). Cet ordre compte sur macOS, où les deux anciens
/// dossiers n'en font qu'un (`Application Support/planrs`) : `app.ron` va
/// au dossier d'eframe, le reste au dossier d'état. Un échec est signalé et
/// n'empêche pas le démarrage (l'ancien dossier reste en place).
fn adopt_legacy_state() {
    if let Err(error) = adopt_legacy_preferences(eframe::storage_dir) {
        eprintln!("{PRODUCT_NAME} : préférences de planrs non reprises ({error})");
    }
    if let Err(error) = plannercraft_automation::control::adopt_legacy_state_dir() {
        eprintln!("{PRODUCT_NAME} : dossier d'état de planrs non repris ({error})");
    }
}

/// Icône de la fenêtre (barre des tâches Windows et Linux ; macOS prend
/// celle du `.app`), dessinée par `cargo xtask icon` (ADR-0021).
const WINDOW_ICON_PNG: &[u8] = include_bytes!("../../../assets/icon/plannercraft-256.png");

/// Options de la fenêtre : titre, icône, tailles, boucle d'événements ;
/// repli des polices du système gardé (`system_font_fallback`, défaut
/// d'eframe).
fn native_options() -> eframe::NativeOptions {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(PRODUCT_NAME)
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([800.0, 500.0]);
    // PNG illisible : fenêtre sans icône plutôt qu'un échec au lancement.
    match eframe::icon_data::from_png_bytes(WINDOW_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(error) => eprintln!("{PRODUCT_NAME} : icône de fenêtre illisible ({error})"),
    }
    eframe::NativeOptions {
        viewport,
        event_loop_builder: event_loop_hook(),
        ..Default::default()
    }
}

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if version_requested(&args) {
        // Sortie fermée ou absente : code d'échec, jamais de panique (la
        // macro `println!` paniquerait).
        use std::io::Write as _;
        return match writeln!(std::io::stdout(), "{PRODUCT_NAME} {PRODUCT_VERSION}") {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };
    }
    let startup = startup_path(args.into_iter()).unwrap_or_else(|error| {
        eprintln!("{PRODUCT_NAME} : {error}");
        None
    });
    install_key_log();
    adopt_legacy_state();
    let options = native_options();
    let result = eframe::run_native(
        PRODUCT_NAME,
        options,
        Box::new(|cc| {
            let env = Env {
                now: Box::new(now),
                system_language: Language::from_system(),
                files: Box::new(RfdFileService),
            };
            let mut app = PlannerCraftApp::new(cc, env);
            let ctx = cc.egui_ctx.clone();
            let (control, inbox) = ControlRendezvous::new(Arc::new(move || ctx.request_repaint()));
            app.app_mut().control_inbox(inbox);
            app.app_mut().enable_screenshots(ScreenshotConfig::default());
            match recovery_dir() {
                Some(root) => start_recovery(app.app_mut(), root),
                None => {
                    eprintln!("{PRODUCT_NAME} : reprise après plantage indisponible (aucun dossier d'état)");
                }
            }
            app.app_mut().set_observer(Box::new(control));
            // Après l'observateur : le rendez-vous suit le fichier ouvert.
            if let Some(path) = &startup {
                app.app_mut().open_path(path);
            }
            #[cfg(target_os = "macos")]
            let (shell, menu) = {
                let language = Language::resolve(app.app_mut().state.language, Language::from_system());
                native_menu(&cc.egui_ctx, language)
            };
            #[cfg(not(target_os = "macos"))]
            let shell = None;
            Ok(Box::new(DesktopApp {
                inner: app,
                shell,
                #[cfg(target_os = "macos")]
                _menu: menu,
            }))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{PRODUCT_NAME} : impossible de démarrer l'interface : {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use std::path::Path;

    use plannercraft_automation::control::find_live_for_in;
    use plannercraft_ui_egui::document;
    use plannercraft_ui_egui::{DocOrigin, DocumentObserver, OpenDocument};

    use super::{ControlRendezvous, WINDOW_ICON_PNG, startup_path, version_requested};

    /// L'icône de fenêtre embarquée se décode, au côté attendu.
    #[test]
    fn desktop_window_icon_decodes() {
        let icon = eframe::icon_data::from_png_bytes(WINDOW_ICON_PNG).unwrap();
        assert_eq!((icon.width, icon.height), (256, 256));
    }

    /// `--version` et `-V`, seuls, demandent la version ; tout autre
    /// argument suit le chemin d'ouverture (C-REL-03).
    #[test]
    fn desktop_version_requested_only_alone() {
        use std::ffi::OsString;

        assert!(version_requested(&[OsString::from("--version")]));
        assert!(version_requested(&[OsString::from("-V")]));
        assert!(!version_requested(&[]));
        assert!(!version_requested(&[OsString::from("plan.xml")]));
        assert!(!version_requested(&[
            OsString::from("--version"),
            OsString::from("a.xml")
        ]));
    }

    /// macOS : le menu par défaut de winit (« Quitter » = `terminate:`, sans
    /// `CloseRequested`) est retiré ; ailleurs, aucun réglage.
    #[test]
    fn desktop_event_loop_hook_only_on_macos() {
        assert_eq!(super::event_loop_hook().is_some(), cfg!(target_os = "macos"));
    }

    /// Document de l'exemple, ouvert depuis `path`.
    fn demo_at(path: &Path) -> OpenDocument {
        let loaded = document::load(
            plannercraft_ui_egui::DEMO_MSPDI,
            "2027-01-15T08:00".parse().unwrap(),
        )
        .unwrap();
        let origin = DocOrigin {
            name: "maison.xml".to_owned(),
            path: Some(path.to_str().unwrap().to_owned()),
            demo: false,
        };
        OpenDocument::new(loaded, origin, 1)
    }

    /// Relecture finale J7, relais F2 : un document neuf (autre ouverture)
    /// change le jeton publié ; « Enregistrer sous » (même ouverture, autre
    /// chemin) le garde.
    #[cfg(unix)]
    #[test]
    fn desktop_new_document_rotates_token() {
        let root = std::env::temp_dir().join(format!("plannercraft-rot-{}", std::process::id()));
        let dir = root.join("control");
        std::fs::create_dir_all(&root).unwrap();
        let first = root.join("a.xml");
        let second = root.join("b.xml");
        for file in [&first, &second] {
            std::fs::write(file, plannercraft_ui_egui::DEMO_MSPDI).unwrap();
        }
        let (mut observer, _inbox) = ControlRendezvous::new(std::sync::Arc::new(|| {}));
        observer.dir = Some(dir.clone());
        let doc = demo_at(&first);
        observer.document_changed(Some(&doc));
        let token = find_live_for_in(&dir, &first).unwrap().token;
        // Même fichier, autre ouverture : nouveau jeton.
        let mut reopened = demo_at(&first);
        reopened.opening = 2;
        observer.document_changed(Some(&reopened));
        let rotated = find_live_for_in(&dir, &first).unwrap().token;
        assert_ne!(rotated, token);
        // « Enregistrer sous » : même ouverture, autre chemin, même jeton.
        let mut saved_as = reopened.clone();
        saved_as.set_origin(plannercraft_ui_egui::DocOrigin {
            name: "b.xml".to_owned(),
            path: Some(second.to_str().unwrap().to_owned()),
            demo: false,
        });
        observer.document_changed(Some(&saved_as));
        assert_eq!(find_live_for_in(&dir, &second).unwrap().token, rotated);
        assert!(find_live_for_in(&dir, &first).is_none());
        drop(observer);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Le rendez-vous suit le chemin du document : publié à l'ouverture
    /// (résumé tenu), déplacé avec le chemin, retiré à la fermeture.
    #[cfg(unix)]
    #[test]
    fn desktop_rendezvous_follows_document_path() {
        let root = std::env::temp_dir().join(format!("plannercraft-l11-{}", std::process::id()));
        let dir = root.join("control");
        std::fs::create_dir_all(&root).unwrap();
        let first = root.join("a.xml");
        let second = root.join("b.xml");
        for file in [&first, &second] {
            std::fs::write(file, plannercraft_ui_egui::DEMO_MSPDI).unwrap();
        }
        let (mut observer, _inbox) = ControlRendezvous::new(std::sync::Arc::new(|| {}));
        observer.dir = Some(dir.clone());
        observer.document_changed(Some(&demo_at(&first)));
        assert!(find_live_for_in(&dir, &first).is_some());
        observer.document_changed(Some(&demo_at(&second)));
        assert!(find_live_for_in(&dir, &first).is_none());
        assert!(find_live_for_in(&dir, &second).is_some());
        observer.document_changed(None);
        assert!(find_live_for_in(&dir, &second).is_none());
        drop(observer);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Arguments : aucun, un chemin (rendu absolu), une option ou deux
    /// fichiers refusés ; le fichier donné s'ouvre (et entre aux fichiers
    /// récents), un fichier illisible s'affiche dans le bandeau avec son nom.
    #[test]
    fn desktop_open_from_argument() {
        use std::ffi::OsString;

        use plannercraft_ui_egui::{App, Env, UiState};

        assert_eq!(startup_path(std::iter::empty()), Ok(None));
        let relative = startup_path([OsString::from("plan.xml")].into_iter())
            .unwrap()
            .unwrap();
        assert!(Path::new(&relative).is_absolute(), "{relative}");
        assert!(relative.ends_with("plan.xml"));
        assert!(startup_path([OsString::from("--aide")].into_iter()).is_err());
        assert!(startup_path([OsString::from("a.xml"), OsString::from("b.xml")].into_iter()).is_err());

        let root = std::env::temp_dir().join(format!("plannercraft-l14-arg-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let good = root.join("maison.xml");
        std::fs::write(&good, plannercraft_ui_egui::DEMO_MSPDI).unwrap();
        let bad = root.join("tronqué.xml");
        std::fs::write(&bad, b"").unwrap();
        let now = "2027-01-15T08:00".parse().unwrap();
        let mut app = App::new(UiState::default(), Env::fixed(now));
        let path = startup_path([good.clone().into_os_string()].into_iter())
            .unwrap()
            .unwrap();
        app.open_path(&path);
        let doc = app.document.as_ref().unwrap();
        assert_eq!(doc.origin.name, "maison.xml");
        assert_eq!(doc.origin.path.as_deref(), Some(path.as_str()));
        assert_eq!(
            app.state.recent.first().map(|r| r.path.as_str()),
            Some(path.as_str())
        );
        assert!(app.messages().is_empty(), "{:?}", app.messages());
        app.open_path(bad.to_str().unwrap());
        assert_eq!(
            app.document.as_ref().unwrap().opening,
            1,
            "le document reste affiché"
        );
        let message = &app.messages().last().unwrap().text;
        assert!(message.contains("tronqué.xml"), "{message}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Démarrage : un orphelin modifié est proposé, un orphelin sans
    /// modification supprimé ; le document ouvert ensuite tient son journal
    /// (verrou tenu : pas un orphelin), supprimé à sa fermeture.
    #[test]
    fn desktop_start_recovery_offers_and_cleans() {
        use plannercraft_automation::recovery::{Journal, orphans};
        use plannercraft_ui_egui::{Action, App, Env, UiState};

        let root = std::env::temp_dir().join(format!("plannercraft-l15-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let now = "2027-01-15T08:00".parse().unwrap();
        let mut session = document::load(plannercraft_ui_egui::DEMO_MSPDI, now)
            .unwrap()
            .session;
        // Sans modification : supprimé ; base non enregistrée : proposé.
        drop(Journal::create(&root, &session).unwrap());
        session.mark_unsaved();
        drop(Journal::create(&root, &session).unwrap());
        assert_eq!(orphans(&root).len(), 2);

        let mut app = App::new(UiState::default(), Env::fixed(now));
        super::start_recovery(&mut app, root.clone());
        assert_eq!(app.recovery_offers().len(), 1);
        assert!(app.recovery_offers().iter().all(|o| !o.saved));
        assert_eq!(orphans(&root).len(), 1, "orphelin sans modification supprimé");
        app.open_demo();
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            2,
            "journal du document"
        );
        assert_eq!(orphans(&root).len(), 1, "journal vivant vu orphelin");
        app.dispatch(Action::CloseDocument);
        let ctx = eframe::egui::Context::default();
        let mut output = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
            plannercraft_ui_egui::ui(ui, &mut app);
        });
        output.textures_delta.clear();
        assert!(app.document.is_none());
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "journal supprimé à la fermeture"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Relecture finale J7, relais F5 : un journal d'une autre version de
    /// format n'est jamais supprimé au démarrage ; il est proposé (Ignorer
    /// le supprime).
    #[test]
    fn desktop_start_recovery_keeps_foreign_journal() {
        use plannercraft_ui_egui::{App, Env, UiState};

        let root = std::env::temp_dir().join(format!("plannercraft-foreign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("9-0");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("base.json"), br#"{"v":2}"#).unwrap();
        let now = "2027-01-15T08:00".parse().unwrap();
        let mut app = App::new(UiState::default(), Env::fixed(now));
        super::start_recovery(&mut app, root.clone());
        assert!(
            dir.join("base.json").exists(),
            "journal d'une autre version supprimé"
        );
        assert_eq!(app.recovery_offers().len(), 1);
        assert!(app.recovery_offers().iter().all(|o| o.kept));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Renommage du 2026-11-03 : les préférences d'eframe de `planrs`
    /// passent dans le dossier de PlannerCraft s'il n'en a pas, sans
    /// écraser celles qui y sont déjà.
    #[test]
    fn desktop_adopts_legacy_preferences() {
        let root = std::env::temp_dir().join(format!("plannercraft-legacy-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = |name: &str| Some(root.join(name));
        std::fs::create_dir_all(root.join("planrs")).unwrap();
        std::fs::write(
            root.join("planrs").join("app.ron"),
            "(\"planrs.ui_state\": \"{}\")",
        )
        .unwrap();

        assert_eq!(super::adopt_legacy_preferences(dir).unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(root.join("PlannerCraft").join("app.ron")).unwrap(),
            "(\"planrs.ui_state\": \"{}\")"
        );
        assert!(!root.join("planrs").exists());

        // Nouveau dossier déjà rempli : rien n'est écrasé.
        std::fs::create_dir_all(root.join("planrs")).unwrap();
        std::fs::write(root.join("planrs").join("app.ron"), "ancien").unwrap();
        assert_eq!(super::adopt_legacy_preferences(dir).unwrap(), 0);
        assert_ne!(
            std::fs::read_to_string(root.join("PlannerCraft").join("app.ron")).unwrap(),
            "ancien"
        );
        assert_eq!(super::adopt_legacy_preferences(|_: &str| None).unwrap(), 0);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// H-UI-108, H-UI-109 : menu natif de macOS, décrit partout. Menu
    /// « PlannerCraft » (À propos, Masquer ⌘H, Masquer les autres ⌥⌘H, Tout
    /// afficher, Quitter ⌘Q) et Fenêtre (Réduire ⌘M, Zoom) ; « Quitter » est
    /// le seul élément personnalisé (aucun `terminate:`), libellés en
    /// français et en anglais.
    #[test]
    fn macos_menu_spec() {
        use plannercraft_ui_egui::Language;

        use super::{MenuItemKind as K, menu_spec};

        let fr = menu_spec(Language::Fr);
        let kinds: Vec<K> = fr.app.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            [
                K::About,
                K::Separator,
                K::Hide,
                K::HideOthers,
                K::ShowAll,
                K::Separator,
                K::Quit
            ]
        );
        let labels: Vec<&str> = fr.app.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "À propos de PlannerCraft",
                "",
                "Masquer PlannerCraft",
                "Masquer les autres",
                "Tout afficher",
                "",
                "Quitter PlannerCraft"
            ]
        );
        let shortcuts: Vec<&str> = fr.app.iter().map(|e| e.shortcut).collect();
        assert_eq!(shortcuts, ["", "", "⌘H", "⌥⌘H", "", "", "⌘Q"]);
        assert_eq!(fr.window_title, "Fenêtre");
        let window: Vec<(K, &str, &str)> = fr
            .window
            .iter()
            .map(|e| (e.kind, e.label.as_str(), e.shortcut))
            .collect();
        assert_eq!(window, [(K::Minimize, "Réduire", "⌘M"), (K::Zoom, "Zoom", "")]);
        // Un seul « Quitter », dans le menu de l'application.
        let quits = fr
            .app
            .iter()
            .chain(&fr.window)
            .filter(|e| e.kind == K::Quit)
            .count();
        assert_eq!(quits, 1);

        let en = menu_spec(Language::En);
        let labels: Vec<&str> = en.app.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "About PlannerCraft",
                "",
                "Hide PlannerCraft",
                "Hide Others",
                "Show All",
                "",
                "Quit PlannerCraft"
            ]
        );
        assert_eq!(en.window_title, "Window");
        assert_eq!(en.window.first().unwrap().label, "Minimize");
        assert_eq!(super::QUIT_MENU_ID, "plannercraft.quit");
    }

    /// H-UI-108 : le journal des touches ne s'active que pour
    /// `PLANNERCRAFT_LOG_KEYS=1` ; le journaliseur ne garde que les cibles
    /// `plannercraft::*` au niveau `Info` ou plus grave.
    #[test]
    fn desktop_key_log_switch_and_filter() {
        use std::ffi::OsStr;

        use log::Log as _;

        assert!(super::key_log_requested(Some(OsStr::new("1"))));
        assert!(!super::key_log_requested(Some(OsStr::new("0"))));
        assert!(!super::key_log_requested(Some(OsStr::new("oui"))));
        assert!(!super::key_log_requested(None));
        let meta = |target: &'static str, level| log::Metadata::builder().target(target).level(level).build();
        let logger = super::KeyLog;
        assert!(logger.enabled(&meta("plannercraft::keys", log::Level::Info)));
        assert!(!logger.enabled(&meta("plannercraft::keys", log::Level::Debug)));
        assert!(!logger.enabled(&meta("wgpu_core", log::Level::Warn)));
    }

    /// Polices : le repli sur les polices du système reste actif (feature
    /// `system_fonts` d'eframe, option par défaut).
    #[test]
    fn desktop_keeps_system_font_fallback() {
        assert!(super::native_options().system_font_fallback);
    }
}
