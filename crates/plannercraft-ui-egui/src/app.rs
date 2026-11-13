//! Application pure (sans eframe) : document ouvert (une `Session`
//! d'édition), environnement injecté, réducteur d'`Action`, relève des
//! ouvertures asynchrones.
//!
//! Une image : relève des `OpenEvent` → synchronisation du thème et de la
//! langue → raccourcis clavier ([`crate::keymap::KEYMAP`]) → dessin des vues
//! (qui poussent des actions) → réducteur → titre de la fenêtre. `App` se
//! construit sans `Frame` ni stockage : les tests kittest l'utilisent telle
//! quelle avec [`Env::fixed`].
//!
//! Toute édition passe par [`App::execute`] (lot de commandes du registre,
//! ADR-0012 D11), [`App::undo`] ou [`App::redo`] ; après chacune,
//! [`App::refresh`] reconstruit les caches d'affichage.
//!
//! Fichiers (lot L13) : « Nouveau » ([`App::new_project`]), « Enregistrer »
//! et « Enregistrer sous » (octets de `Session::save_bytes` remis au
//! [`FileService`] ; `mark_saved` seulement à la réception du succès, et
//! seulement si le document n'a pas changé entre-temps). Fermer, ouvrir un
//! autre document, créer un projet ou quitter avec un document modifié
//! affiche un bandeau non bloquant (Enregistrer / Ne pas enregistrer /
//! Annuler) ; sur le desktop, la fermeture de la fenêtre est annulée
//! (`ViewportCommand::CancelClose`) jusqu'à la réponse.

use std::collections::BTreeSet;
use std::sync::mpsc::{self, Receiver, Sender};

use plannercraft_automation::error::codes;
use plannercraft_automation::filter::FilterCtx;
use plannercraft_automation::gesture::{GestureError, NotDraggable};
use plannercraft_automation::{
    AnchorSpec, ApplyMode, ApplyOutcome, Batch, CommandError, CommandErrorKind, HistoryError, NewProject,
    Session,
};
use plannercraft_core::{
    Day, Moment, PRODUCT_NAME, Project, Schedule, Severity, TaskUid, TimeOfDay, Weekday,
};
use plannercraft_mspdi::ImportDiagnostic;

use crate::action::{Action, ConfirmChoice, DialogKind, TimelineOp, UiOp};
use crate::clipboard::ClipboardSlot;
use crate::column::Column;
use crate::dialog::link::{self, LinkOutcome, LinkState};
use crate::dialog::task_info::{self, FieldError, TaskInfoState};
use crate::dialog::{DialogBody, DialogState};
use crate::document::{self, LoadedDocument, OpenError};
use crate::edit::TableEdit;
use crate::geom;
use crate::i18n::{Arg, Language, TextId, Texts, error_text_id};
use crate::io::{
    FileService, NoFileService, OpenEvent, PickedFile, SaveEvent, SaveKind, SaveRequest, SaveTarget,
    suggested_file_name,
};
use crate::keymap::KeyState;
use crate::nav::RowRef;
use crate::row_view::{DisplayRow, ModelRow, RowView, VisibleRows};
use crate::rows::RowModel;
use crate::selection::Selection;
use crate::state::{
    FilterChoice, SortSpec, StateLoadError, TABLE_WIDTH_MAX, TABLE_WIDTH_MIN, TaskViewState, ThemePref,
    UiState, ViewKind,
};
use crate::structure::PendingDelete;
use crate::theme::Palette;
use crate::timescale::{
    MAX_RANGE_DAYS, TimeAxis, TimeScaleZoom, ViewSpan, default_range, fit_zoom, zoom_anchor,
};
use crate::view::table::table_id;
use crate::view::{self, DiagnosticsView, FrameCtx, FrameStats, Metrics, Pane, ViewTransient};

/// Messages à l'utilisateur : déplacés dans [`crate::notify`] (seconde
/// passe UX, lot L0) ; réexportés pour les vues qui les lisaient ici.
pub use crate::notify::{MAX_MESSAGES, UserMessage};

/// Provenance d'un document ouvert.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocOrigin {
    /// Nom affiché (nom de fichier, ou nom traduit de l'exemple).
    pub name: String,
    /// Chemin UTF-8 sur le desktop ; `None` sur le web et pour l'exemple.
    pub path: Option<String>,
    /// Projet d'exemple embarqué.
    pub demo: bool,
}

impl DocOrigin {
    /// Origine de la session (nom et chemin, sans le drapeau d'exemple).
    #[must_use]
    pub fn to_session(&self) -> plannercraft_automation::DocOrigin {
        plannercraft_automation::DocOrigin {
            name: self.name.clone(),
            path: self.path.clone(),
        }
    }
}

/// Document ouvert : une session d'édition (modèle, planning calculé par le
/// moteur, historique), recalculée à chaque lot de commandes.
#[derive(Clone, Debug)]
pub struct OpenDocument {
    /// Session d'édition. Ne pas la modifier directement : passer par
    /// [`App::execute`], [`App::undo`] et [`App::redo`], qui reconstruisent
    /// les caches d'affichage.
    pub session: Session,
    /// Diagnostics d'import.
    pub import_diagnostics: Vec<ImportDiagnostic>,
    /// Plage de jours de l'échelle de temps.
    pub range: ViewSpan,
    /// Provenance (nom et chemin aussi tenus par la session, voir
    /// [`OpenDocument::set_origin`]).
    pub origin: DocOrigin,
    /// Numéro d'ouverture (1, 2…) : avec [`OpenDocument::generation`], clé
    /// d'invalidation des caches.
    pub opening: u64,
    /// Calendriers compilés, une fois par génération (N3, J7 lot L7).
    pub(crate) calendar_cache: crate::document::CalendarCache,
    /// Fichier du navigateur lié (version web, File System Access, lot W1,
    /// H-UI-204) : posé par « Ouvrir » ou « Enregistrer sous » quand le
    /// navigateur donne un descripteur, réécrit par « Enregistrer ».
    pub web_file: Option<crate::io::WebFile>,
}

impl OpenDocument {
    /// Document d'un chargement réussi : l'origine est aussi posée sur la
    /// session.
    #[must_use]
    pub fn new(loaded: LoadedDocument, origin: DocOrigin, opening: u64) -> Self {
        let mut session = loaded.session;
        session.set_origin(origin.to_session());
        Self {
            session,
            import_diagnostics: loaded.import_diagnostics,
            range: loaded.range,
            origin,
            opening,
            calendar_cache: crate::document::CalendarCache::new(),
            web_file: loaded.web_file,
        }
    }

    /// Calendriers compilés du document (N3, J7 lot L7), compilés à la
    /// première demande de chaque génération.
    #[must_use]
    pub fn calendars(&self) -> std::sync::Arc<plannercraft_core::CalendarSet> {
        self.calendar_cache.get(self.project(), self.generation())
    }

    /// Compilations des calendriers faites pour ce document (tests de N3).
    #[must_use]
    pub fn calendar_compiles(&self) -> u64 {
        self.calendar_cache.compiles()
    }

    /// Modèle courant.
    #[must_use]
    pub const fn project(&self) -> &Project {
        self.session.project()
    }

    /// Planning courant (calculé par le moteur).
    #[must_use]
    pub const fn schedule(&self) -> &Schedule {
        self.session.schedule()
    }

    /// Génération de la session : +1 à chaque commit, annulation ou
    /// rétablissement.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.session.generation()
    }

    /// Clé des caches d'affichage : (numéro d'ouverture, génération).
    #[must_use]
    pub const fn cache_key(&self) -> (u64, u64) {
        (self.opening, self.generation())
    }

    /// Le document diffère de son dernier enregistrement (R-164).
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.session.is_dirty()
    }

    /// Change l'origine (« Enregistrer sous ») ici et dans la session.
    pub fn set_origin(&mut self, origin: DocOrigin) {
        self.session.set_origin(origin.to_session());
        self.origin = origin;
    }
}

/// Provenance d'une édition passée à [`App::execute`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOrigin {
    /// L'utilisateur (menu, raccourci, saisie) : un refus s'affiche dans
    /// le bandeau.
    User,
    /// Un agent (canal de contrôle, J7) : le refus est rendu à l'appelant,
    /// qui le relaie ; rien ne s'affiche.
    Agent,
    /// « OK » d'une fenêtre (J7, lot L9) : le refus s'affiche dans la
    /// fenêtre, pas dans le bandeau.
    Dialog,
}

/// Observateur du document ouvert (application desktop : rendez-vous du
/// canal de contrôle, ADR-0002). Appelé après chaque ouverture, fermeture,
/// commit, annulation ou rétablissement ; jamais pendant le dessin.
pub trait DocumentObserver {
    /// Le document courant a changé (`None` : aucun document).
    fn document_changed(&mut self, document: Option<&OpenDocument>);
}

/// Environnement injecté : seule porte vers l'horloge, les fichiers et la
/// langue du système.
pub struct Env {
    /// « Maintenant », lu à chaque ouverture (`ReadOptions::current_date`).
    pub now: Box<dyn Fn() -> Moment>,
    /// Langue du système, lue par l'application ; `None` en test (repli :
    /// français).
    pub system_language: Option<Language>,
    /// Sélecteur de fichiers.
    pub files: Box<dyn FileService>,
}

impl Env {
    /// Environnement de test : date fixe, aucun sélecteur, langue du système
    /// absente.
    #[must_use]
    pub fn fixed(now: Moment) -> Self {
        Self {
            now: Box::new(move || now),
            system_language: None,
            files: Box::new(NoFileService),
        }
    }
}

/// Opération suspendue au bandeau de confirmation (document modifié).
#[derive(Debug)]
enum Pending {
    /// Fermer le document.
    Close,
    /// Ouvrir l'exemple.
    OpenDemo,
    /// Lancer le sélecteur de fichiers.
    OpenDialog,
    /// Créer un projet vide.
    NewProject,
    /// Fermer la fenêtre (desktop).
    Quit,
    /// Installer un document arrivé du sélecteur alors que le document
    /// courant a été modifié depuis la demande.
    Install(Box<(LoadedDocument, DocOrigin)>),
    /// Ouvrir un fichier par son chemin (fichier récent, J7 lot L14).
    OpenPath(String),
    /// Restaurer un journal orphelin (reprise après plantage, J7 lot L15).
    #[cfg(not(target_arch = "wasm32"))]
    Restore(String),
    /// Ouvrir un fichier déposé sur la fenêtre (J8, lot L13, H-UI-56).
    OpenDropped(egui::DroppedFileHandle),
    /// Installer un document importé d'un CSV (J8, lot L10, H-UI-40), avec
    /// le nom du fichier et le nombre de remarques de l'import.
    InstallCsv(Box<(LoadedDocument, DocOrigin, (String, usize))>),
}

/// Enregistrement en cours.
#[derive(Debug)]
struct SaveJob {
    /// Clé du document enregistré (numéro d'ouverture, génération) au
    /// moment où ses octets ont été produits.
    key: (u64, u64),
    /// Opération à poursuivre après un enregistrement réussi.
    then: Option<Pending>,
    /// Nombre de valeurs ajustées au format MSPDI (diagnostics
    /// d'écriture), signalé après le succès.
    adjusted: usize,
}

/// Clé du cache de la vue des lignes ([`RowView`]) : elle est reconstruite
/// seulement quand l'un de ces éléments change (ADR-0016 D1).
#[derive(Clone, Debug, PartialEq, Eq)]
struct RowViewKey {
    /// Numéro d'ouverture et génération du document.
    document: (u64, u64),
    /// Langue des textes du `RowModel` (tri par nom).
    language: Language,
    /// Nombre de lignes du modèle.
    rows: usize,
    /// Filtre.
    filter: FilterChoice,
    /// Tri.
    sort: Option<SortSpec>,
    /// Niveau affiché.
    outline_level: Option<u8>,
    /// Version du repli.
    collapse_version: u64,
    /// Version de réapplication du filtre (J8, lot L9).
    reapply_version: u64,
}

/// Vue regroupée des lignes (finitions P3, H-UI-129) : groupes de
/// `plannercraft_render::group` sur les tâches retenues par le filtre
/// (`shown`, par position ; `None` : toutes), libellés dans la langue de
/// l'interface, Début et Fin d'un groupe aux textes des tâches qui portent
/// ses bornes (même format que leur ligne).
fn grouped_view(
    model: &RowModel,
    document: &OpenDocument,
    by: plannercraft_render::style::GroupBy,
    shown: Option<&[bool]>,
    collapsed: &BTreeSet<plannercraft_render::group::GroupKey>,
    language: Language,
) -> RowView {
    use plannercraft_render::group::{group_label, group_tasks, header_text};
    let lang = match language {
        Language::Fr => plannercraft_render::Lang::Fr,
        Language::En => plannercraft_render::Lang::En,
    };
    let keep = |i: usize| shown.is_none_or(|v| v.get(i).copied().unwrap_or(false));
    let groups = group_tasks(document.project(), document.schedule(), by, &keep);
    let entries = groups
        .iter()
        .map(|g| {
            let text = |at: Option<usize>, column: crate::column::Column| {
                at.map_or_else(String::new, |i| model.text(i, column).to_owned())
            };
            let header = crate::row_view::GroupHeader {
                key: g.key.clone(),
                name: header_text(&group_label(by, g, lang), g.tasks.len(), lang),
                start: text(g.start.map(|(_, i)| i), crate::column::Column::Start),
                finish: text(g.finish.map(|(_, i)| i), crate::column::Column::Finish),
                bar: g.start.zip(g.finish).map(|((s, _), (f, _))| (s, f)),
                collapsed: collapsed.contains(&g.key),
            };
            (header, g.tasks.iter().map(|&i| ModelRow::at(i)).collect())
        })
        .collect();
    RowView::from_groups(model.len(), entries)
}

/// Application egui de PlannerCraft, indépendante d'eframe.
#[allow(clippy::struct_excessive_bools, reason = "drapeaux indépendants de l'image")]
pub struct App {
    /// Préférences persistées.
    pub state: UiState,
    /// Document ouvert.
    pub document: Option<OpenDocument>,
    /// Tâche sélectionnée (transitoire) : la tâche active de
    /// [`App::selected`].
    pub selection: Option<TaskUid>,
    /// Tâches sélectionnées (lot L14), resynchronisées sur
    /// [`App::selection`] quand elles divergent.
    pub(crate) selected: Selection,
    /// Suppression de récapitulative en attente de confirmation (lot L14).
    pub(crate) pending_delete: Option<PendingDelete>,
    /// Relevé de la dernière image.
    pub last_frame: FrameStats,
    /// Environnement injecté.
    pub(crate) env: Env,
    /// Messages de la langue courante.
    pub(crate) texts: Texts,
    /// Modèle de lignes du document, dans la langue courante.
    pub(crate) rows: Option<RowModel>,
    /// Vue des lignes (J8, lot L5) : lignes affichées par filtre, tri et
    /// repli, construite sur `rows` ; identité sans état de vue.
    pub(crate) row_view: RowView,
    /// Clé de la vue construite (`None` : à reconstruire).
    row_view_key: Option<RowViewKey>,
    /// État transitoire de la vue.
    pub(crate) view: ViewTransient,
    /// Triangles de repli dans la colonne Nom (J8, lot L9, H-UI-34) : vrais
    /// depuis le lot L15, voir [`App::set_outline_triangles`].
    pub(crate) outline_triangles: bool,
    /// Style effectif du planning et ses palettes (J8, lot L11).
    pub(crate) style_cache: crate::theme::StyleCache,
    /// File des actions en attente.
    pub(crate) actions: Vec<Action>,
    /// Messages affichés : notifications, bandeau, historique (seconde
    /// passe UX, H-UI-104).
    pub(crate) messages: crate::notify::MessageStore,
    /// Instant de l'image courante (secondes d'egui, `ctx.input(time)`) :
    /// date de publication des messages.
    pub(crate) clock: f64,
    /// Émetteur des résultats d'ouverture (cloné vers le sélecteur).
    tx: Sender<OpenEvent>,
    /// Récepteur des résultats d'ouverture.
    rx: Receiver<OpenEvent>,
    /// Thème appliqué au contexte egui (appel seulement au changement).
    applied_theme: Option<ThemePref>,
    /// « Réduire les animations » appliqué au contexte egui (lot A1b).
    applied_motion: Option<bool>,
    /// Durée des animations d'egui avant « Réduire les animations ».
    motion_default: Option<f32>,
    /// Génération du prochain document ouvert.
    next_generation: u64,
    /// Une ouverture par le sélecteur est en cours : indicateur seulement,
    /// une nouvelle demande relance le sélecteur.
    opening: bool,
    /// Diagnostics du document triés et mis en texte (cache invalidé à
    /// l'ouverture, à chaque édition et au changement de langue).
    diagnostics: DiagnosticsView,
    /// Observateur du document (desktop), s'il y en a un.
    observer: Option<Box<dyn DocumentObserver>>,
    /// Dernier titre de fenêtre envoyé à eframe.
    sent_title: String,
    /// Opération en attente de la réponse au bandeau de confirmation.
    confirm: Option<Pending>,
    /// Enregistrement en cours (un seul à la fois).
    saving: Option<SaveJob>,
    /// Émetteur des résultats d'enregistrement (cloné vers le service).
    save_tx: Sender<SaveEvent>,
    /// Récepteur des résultats d'enregistrement.
    save_rx: Receiver<SaveEvent>,
    /// Émetteur des résultats d'export (J8, lot L13), à part des
    /// enregistrements : un export ne touche ni l'origine ni l'état
    /// « modifié » du document.
    export_tx: Sender<SaveEvent>,
    /// Récepteur des résultats d'export.
    export_rx: Receiver<SaveEvent>,
    /// Fermeture de la fenêtre acceptée (« Ne pas enregistrer », ou
    /// enregistrement réussi) : la prochaine demande n'est plus annulée.
    quit_confirmed: bool,
    /// Clé du document au lancement du sélecteur : un document arrivé alors
    /// que le document courant a changé depuis redemande confirmation.
    open_confirmed_for: Option<(u64, u64)>,
    /// Un menu ou une fenêtre surgissante était ouvert à la fin de l'image
    /// précédente : la navigation au clavier est suspendue (les raccourcis
    /// sont lus avant le dessin, quand egui ne sait pas encore si un menu
    /// est ouvert).
    popup_open: bool,
    /// Relais des appels d'un agent (desktop, J7 lot L3) : branché par
    /// [`App::control_inbox`].
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) relay: Option<crate::relay::Relay>,
    /// Écritures proposées en attente (agents rattachés et assistant
    /// intégré, lot A2 d'ADR-0024) ; desktop et web.
    pub(crate) approvals: crate::approvals::Approvals,
    /// Numéro de la prochaine proposition de l'assistant.
    next_proposal: u64,
    /// Volet latéral : opérations IA, « Pourquoi cette date ? » (J8 lot
    /// L12), assistant ; ouvert ou fermé recopié dans
    /// `UiState.side_panel` (lot A5).
    pub(crate) side: crate::view::side_panel::SidePanelState,
    /// Nouveautés de l'assistant arrivées volet fermé (réponse terminée,
    /// erreur, proposition ; lot A5, H-UI-166) : pastille du bouton,
    /// remise à zéro à l'ouverture.
    assistant_badge: u32,
    /// Widget qui avait le focus clavier à la fin de l'image précédente
    /// (egui le retire sur Échap avant l'image suivante ; lot A5).
    last_focus: Option<egui::Id>,
    /// Assistant IA (ADR-0024, lot A1) : conversation, clé, connexion ;
    /// jamais persisté (la clé ne va pas dans `UiState`).
    pub(crate) assistant: crate::assistant::AssistantPanel,
    /// Palette de commandes (interface adoucie, lot U3) : jamais persistée.
    pub(crate) palette: crate::palette::PaletteState,
    /// Journal de reprise après plantage (desktop, J7 lot L15) ou
    /// sauvegarde automatique du navigateur (web, lot W1) : branché par
    /// [`App::set_recovery`], suit le document à chaque changement.
    recovery: Option<Box<dyn plannercraft_automation::recovery::RecoveryLog>>,
    /// Message d'une écriture du journal impossible : reprise après
    /// plantage (desktop) ou sauvegarde du navigateur (web, H-UI-201).
    recovery_cut_text: TextId,
    /// Numéro d'ouverture du planning rouvert depuis la sauvegarde du
    /// navigateur (H-UI-199) : bandeau « Planning retrouvé » tant que ce
    /// document est ouvert et que le bandeau n'est pas masqué.
    restored: Option<u64>,
    /// Journaux orphelins proposés par le bandeau de reprise (le premier est
    /// affiché), posés par [`App::offer_recovery`].
    #[cfg(not(target_arch = "wasm32"))]
    orphans: Vec<plannercraft_automation::recovery::Orphan>,
    /// Dernière copie de lignes (J8, lot L10, H-UI-39) : survit à
    /// l'ouverture d'un autre document.
    pub(crate) clipboard: Option<ClipboardSlot>,
    /// Image d'un « Coller » du menu (desktop) : l'événement `Paste` qui
    /// suit est accepté hors du focus de la table.
    pub(crate) menu_paste_at: Option<u64>,
    /// Émetteur des fichiers choisis pour l'import CSV (cloné vers le
    /// sélecteur).
    pub(crate) pick_tx: Sender<PickedFile>,
    /// Récepteur des fichiers choisis pour l'import CSV.
    pub(crate) pick_rx: Receiver<PickedFile>,
}

impl App {
    /// Crée l'application (état normalisé, aucun effet).
    #[must_use]
    pub fn new(state: UiState, env: Env) -> Self {
        let state = state.normalized();
        // Volet latéral retrouvé ouvert ou fermé (lot A5, H-UI-163).
        let side_open = state.side_panel.open;
        let texts = Texts::new(Language::resolve(state.language, env.system_language));
        let (tx, rx) = mpsc::channel();
        let (save_tx, save_rx) = mpsc::channel();
        let (export_tx, export_rx) = mpsc::channel();
        let (pick_tx, pick_rx) = mpsc::channel();
        Self {
            state,
            document: None,
            selection: None,
            selected: Selection::default(),
            pending_delete: None,
            last_frame: FrameStats::default(),
            env,
            texts,
            rows: None,
            row_view: RowView::default(),
            row_view_key: None,
            view: ViewTransient::default(),
            outline_triangles: true,
            style_cache: crate::theme::StyleCache::default(),
            actions: Vec::new(),
            messages: crate::notify::MessageStore::default(),
            clock: 0.0,
            tx,
            rx,
            applied_theme: None,
            applied_motion: None,
            motion_default: None,
            next_generation: 1,
            opening: false,
            diagnostics: DiagnosticsView::default(),
            observer: None,
            sent_title: PRODUCT_NAME.to_owned(),
            confirm: None,
            saving: None,
            save_tx,
            save_rx,
            export_tx,
            export_rx,
            quit_confirmed: false,
            open_confirmed_for: None,
            popup_open: false,
            #[cfg(not(target_arch = "wasm32"))]
            relay: None,
            approvals: crate::approvals::Approvals::default(),
            next_proposal: 1,
            side: crate::view::side_panel::SidePanelState::opened(side_open),
            assistant_badge: 0,
            last_focus: None,
            assistant: crate::assistant::AssistantPanel::new(crate::assistant::AssistantServices::offline()),
            palette: crate::palette::PaletteState::default(),
            recovery: None,
            recovery_cut_text: TextId::RecoveryJournalCut,
            restored: None,
            #[cfg(not(target_arch = "wasm32"))]
            orphans: Vec::new(),
            clipboard: None,
            menu_paste_at: None,
            pick_tx,
            pick_rx,
        }
    }

    /// Installe l'observateur du document (remplace le précédent) et lui
    /// signale aussitôt le document courant.
    pub fn set_observer(&mut self, observer: Box<dyn DocumentObserver>) {
        self.observer = Some(observer);
        self.notify_observer();
    }

    /// Signale le document courant à l'observateur.
    /// Le journal de reprise suit aussi le document (desktop), comme la
    /// sauvegarde automatique du navigateur (web, lot W1).
    pub(crate) fn notify_observer(&mut self) {
        if let Some(observer) = &mut self.observer {
            observer.document_changed(self.document.as_ref());
        }
        self.record_recovery();
    }

    /// Langue des textes du moteur (messages d'erreur des commandes).
    pub(crate) fn text_language(&self) -> plannercraft_automation::text::Language {
        match self.texts.language() {
            Language::Fr => plannercraft_automation::text::Language::Fr,
            Language::En => plannercraft_automation::text::Language::En,
        }
    }

    /// Applique un lot de commandes au document (ADR-0012 D11) puis
    /// reconstruit l'affichage ([`App::refresh`]). Un refus laisse le
    /// document intact ; venant de l'utilisateur, il s'affiche dans le
    /// bandeau, traduit par son code (`error-<code>` des fichiers `.ftl`).
    ///
    /// # Errors
    ///
    /// Le refus du lot ([`CommandError`]), ou `session.none` sans document.
    pub fn execute(&mut self, batch: &Batch, origin: EditOrigin) -> Result<ApplyOutcome, CommandError> {
        let result = match &mut self.document {
            Some(document) => {
                let before = document.generation();
                document
                    .session
                    .apply(batch, ApplyMode::Commit)
                    .map(|outcome| (outcome, document.generation() != before))
            }
            None => Err(CommandError::new(
                codes::SESSION_NONE,
                CommandErrorKind::InvalidParam(match self.texts.language() {
                    Language::Fr => "aucun document ouvert".to_owned(),
                    Language::En => "no open document".to_owned(),
                }),
            )),
        };
        match result {
            Ok((outcome, changed)) => {
                self.clear_titled(TextId::CommandErrorTitle);
                if changed {
                    self.refresh();
                }
                Ok(outcome)
            }
            Err(error) => {
                if origin == EditOrigin::User {
                    self.report_command_error(&error);
                }
                Err(error)
            }
        }
    }

    /// Annule le dernier lot (R-164) puis reconstruit l'affichage. Rien à
    /// annuler : aucun effet, aucun message.
    ///
    /// # Errors
    ///
    /// [`HistoryError::Empty`] (rien à annuler, ou aucun document) ;
    /// [`HistoryError::Inconsistent`], aussi affiché dans le bandeau.
    pub fn undo(&mut self) -> Result<ApplyOutcome, HistoryError> {
        self.history_step(Session::undo)
    }

    /// Rétablit le dernier lot annulé (R-164) puis reconstruit
    /// l'affichage. Rien à rétablir : aucun effet, aucun message.
    ///
    /// # Errors
    ///
    /// Comme [`App::undo`].
    pub fn redo(&mut self) -> Result<ApplyOutcome, HistoryError> {
        self.history_step(Session::redo)
    }

    /// Annulation ou rétablissement.
    fn history_step(
        &mut self,
        step: fn(&mut Session) -> Result<ApplyOutcome, HistoryError>,
    ) -> Result<ApplyOutcome, HistoryError> {
        let Some(document) = &mut self.document else {
            return Err(HistoryError::Empty);
        };
        let result = step(&mut document.session);
        match &result {
            Ok(_) => {
                self.clear_titled(TextId::CommandErrorTitle);
                self.refresh();
            }
            Err(HistoryError::Empty) => {}
            Err(error) => {
                let text = self.texts.get_args(
                    error_text_id(error.code()),
                    &[("detail", Arg::Str(error.to_string().into()))],
                );
                self.push_titled(Severity::Error, Some(TextId::CommandErrorTitle), text);
            }
        }
        result
    }

    /// Reconstruit les caches d'affichage après une édition : modèle de
    /// lignes, diagnostics, plage = union de l'ancienne et de la nouvelle
    /// plage par défaut (la nouvelle seule si l'union dépasse
    /// [`MAX_RANGE_DAYS`]). La sélection est gardée par UID ; si la tâche
    /// active n'existe plus (suppression, annulation d'une insertion,
    /// écriture d'un agent), la tâche qui occupe maintenant sa ligne devient
    /// active, ou la dernière en fin de liste, comme MS Project (relecture
    /// finale J7, F4 : le curseur sautait sur la ligne vide de fin) ; les
    /// défilements sont gardés (bornés).
    pub fn refresh(&mut self) {
        let Some(document) = &mut self.document else {
            return;
        };
        let old_index = self
            .selection
            .and_then(|uid| self.rows.as_ref().and_then(|rows| rows.index_of(uid)));
        let range = default_range(document.project(), document.schedule()).span;
        document.range = union_span(document.range, range);
        let rows = RowModel::build(document, self.texts.format_labels());
        if self
            .selection
            .is_some_and(|uid| !document.project().tasks.contains(uid))
        {
            self.selection = old_index.and_then(|index| {
                rows.row(index)
                    .or_else(|| rows.len().checked_sub(1).and_then(|last| rows.row(last)))
                    .map(|row| row.uid)
            });
        }
        self.rows = Some(rows);
        self.diagnostics = DiagnosticsView::build(document, &self.texts);
        self.selected.retain(|uid| document.project().tasks.contains(uid));
        self.row_view_key = None;
        self.ensure_row_view();
        self.sync_selection();
        self.clamp_gantt_scroll();
        self.notify_observer();
    }

    /// État de la vue des tâches (filtre, tri, niveau affiché ; J8, lot
    /// L5).
    #[must_use]
    pub const fn task_view(&self) -> &TaskViewState {
        &self.view.task_view
    }

    /// Change l'état de la vue des tâches : une vue, rien d'écrit dans le
    /// document (P50) ; la vue des lignes est reconstruite aussitôt, une
    /// sélection devenue masquée est retirée.
    pub fn set_task_view(&mut self, state: TaskViewState) {
        if self.view.task_view != state {
            self.view.task_view = state;
            self.ensure_row_view();
            self.sync_selection();
        }
    }

    /// Récapitulatives repliées (J8, lot L5).
    #[must_use]
    pub const fn collapsed(&self) -> &BTreeSet<TaskUid> {
        &self.view.collapsed
    }

    /// Replie (`true`) ou déplie la tâche `uid` (sans effet sur une feuille,
    /// qui n'a rien à masquer) ; rien d'écrit dans le document.
    pub fn set_collapsed(&mut self, uid: TaskUid, collapsed: bool) {
        let changed = if collapsed {
            self.view.collapsed.insert(uid)
        } else {
            self.view.collapsed.remove(&uid)
        };
        if changed {
            self.view.collapse_version = self.view.collapse_version.wrapping_add(1);
            self.ensure_row_view();
            self.sync_selection();
        }
    }

    /// Inverse le repli de `uid`.
    pub fn toggle_collapsed(&mut self, uid: TaskUid) {
        let collapsed = self.view.collapsed.contains(&uid);
        self.set_collapsed(uid, !collapsed);
    }

    /// Reconstruit la vue des lignes si sa clé a changé (document, édition,
    /// langue, filtre, tri, niveau, repli), puis retire de la sélection les
    /// tâches masquées : la tâche active masquée est remplacée par la
    /// prochaine tâche affichée dans l'ordre des ID (la précédente en fin de
    /// liste ; aucune si rien n'est affiché). Une saisie ouverte sur une
    /// ligne masquée est abandonnée. Sans document : vue vide.
    pub(crate) fn ensure_row_view(&mut self) {
        // Textes des champs affichés (seconde passe UX, lot L7, `rows.rs`).
        self.ensure_row_columns();
        let (Some(document), Some(model)) = (&self.document, &self.rows) else {
            self.row_view = RowView::default();
            self.row_view_key = None;
            return;
        };
        let task_view = &self.view.task_view;
        let key = RowViewKey {
            document: document.cache_key(),
            language: self.texts.language(),
            rows: model.len(),
            filter: task_view.filter.clone(),
            sort: task_view.sort,
            outline_level: task_view.outline_level,
            collapse_version: self.view.collapse_version,
            reapply_version: self.view.reapply_version,
        };
        if self.row_view_key.as_ref() != Some(&key) {
            let project = document.project();
            let ctx = FilterCtx {
                project,
                schedule: document.schedule(),
                status: project.effective_status_date(),
            };
            // H-UI-30 (J8, lot L9) : lignes gardées par le filtre jusqu'à
            // sa réapplication.
            let mut shown = RowView::filter_shown(model, &ctx, task_view);
            self.view.filter_memo.update(
                model,
                &task_view.filter,
                document.cache_key(),
                self.view.reapply_version,
                &mut shown,
            );
            // Regroupement du style du document (finitions P3, H-UI-129) :
            // le filtre d'abord, puis les groupes ; repliés oubliés quand le
            // regroupement change.
            let group_by = plannercraft_automation::command::group_by::group_by_of(project);
            if group_by != self.view.group_by {
                self.view.collapsed_groups.clear();
                self.view.cursor_row = None;
            }
            self.view.group_by = group_by;
            self.row_view = match group_by {
                Some(by) => grouped_view(
                    model,
                    document,
                    by,
                    shown.as_deref(),
                    &self.view.collapsed_groups,
                    self.texts.language(),
                ),
                None => {
                    RowView::build_with_shown(model, &ctx, task_view, &self.view.collapsed, shown.as_deref())
                }
            };
            self.row_view_key = Some(key);
        }
        self.drop_hidden_selection();
    }

    /// Retire de la sélection les tâches que la vue masque (voir
    /// [`App::ensure_row_view`]).
    fn drop_hidden_selection(&mut self) {
        let Some(model) = &self.rows else {
            return;
        };
        let view = &self.row_view;
        let shown = |uid: TaskUid| {
            model
                .index_of(uid)
                .is_some_and(|i| view.display_row(ModelRow::at(i)).is_some())
        };
        if let Some(active) = self.selection
            && !shown(active)
            && let Some(i) = model.index_of(active)
        {
            let visible_at = |j: usize| {
                view.display_row(ModelRow::at(j))
                    .and_then(|_| model.row(j))
                    .map(|row| row.uid)
            };
            let next = (i.saturating_add(1)..model.len())
                .find_map(visible_at)
                .or_else(|| (0..i).rev().find_map(visible_at));
            self.selected.retain(|uid| uid != active);
            self.selection = next;
        }
        if self.selected.uids().iter().any(|&uid| !shown(uid)) {
            self.selected.retain(shown);
        }
        if let Some(editing) = &self.view.edit.editing
            && editing.uid.is_some_and(|uid| !shown(uid))
        {
            self.view.edit.editing = None;
        }
    }

    /// Tâches sélectionnées (lot L14).
    #[must_use]
    pub const fn selected(&self) -> &Selection {
        &self.selected
    }

    /// Ramène [`App::selected`] à la seule [`App::selection`] si la tâche
    /// active a changé par ailleurs (clic simple, tâche supprimée, nouveau
    /// document), puis pose la ligne du curseur sur la tâche active (la
    /// ligne vide sans sélection, J7 lot L1).
    pub(crate) fn sync_selection(&mut self) {
        // Saisie ouverte : sa cellule reste la cellule active ; une sélection
        // changée ailleurs pendant la saisie (Gantt, chronologie, saisie
        // refusée) est ramenée sur elle (H-UI-86 : jamais de saisie ouverte
        // sur une autre cellule que la cellule active).
        if let Some(editing) = &self.view.edit.editing
            && editing.uid.is_none_or(|uid| {
                self.rows
                    .as_ref()
                    .is_some_and(|rows| rows.index_of(uid).is_some())
            })
        {
            self.selection = editing.uid;
            self.view.cursor.column = editing.column;
        }
        if self.selected.focus() != self.selection {
            self.selected = Selection::single(self.selection);
            // Sélection posée ailleurs (Gantt, chronologie, agent) : bloc
            // d'une cellule (ADR-0018).
            self.view.cursor.anchor_column = None;
        }
        self.view.cursor.row = RowRef::from_selection(self.selection);
        if self.selected.len() <= 1 {
            self.view.cursor.anchor = None;
        }
    }

    /// Titre de la fenêtre : nom du document précédé de « * » s'il est
    /// modifié, puis nom du produit.
    #[must_use]
    pub fn window_title(&self) -> String {
        window_title_for(self.document.as_ref())
    }

    /// Crée un projet vide (R-032) : nom traduit « Projet1 », planifié
    /// depuis le lundi qui suit « maintenant », 08:00 (hypothèse, voir
    /// [`next_monday_morning`]). Ne demande aucune confirmation : passer
    /// par [`Action::NewProject`] pour protéger un document modifié.
    pub fn new_project(&mut self) {
        let now = (self.env.now)();
        let name = self.texts.get(TextId::NewProjectName);
        let init = NewProject {
            name: name.clone(),
            anchor: AnchorSpec::Start(next_monday_morning(now)),
            lang: Some(self.text_language()),
        };
        match Session::new_project(init, now) {
            Ok(session) => {
                let range = default_range(session.project(), session.schedule());
                let loaded = LoadedDocument {
                    session,
                    import_diagnostics: Vec::new(),
                    range: range.span,
                    range_truncated: range.truncated,
                    web_file: None,
                };
                let origin = DocOrigin {
                    name,
                    path: None,
                    demo: false,
                };
                self.install(loaded, origin);
            }
            Err(error) => self.report_command_error(&error),
        }
    }

    /// Un bandeau « Enregistrer les modifications ? » attend une réponse.
    #[must_use]
    pub const fn confirmation_pending(&self) -> bool {
        self.confirm.is_some()
    }

    /// Un enregistrement est en cours.
    #[must_use]
    pub const fn is_saving(&self) -> bool {
        self.saving.is_some()
    }

    /// Le document courant est modifié depuis son dernier enregistrement.
    fn document_dirty(&self) -> bool {
        self.document.as_ref().is_some_and(OpenDocument::is_dirty)
    }

    /// Demande une opération qui remplacerait le document : bandeau de
    /// confirmation si le document est modifié, exécution sinon.
    fn request(&mut self, pending: Pending, ctx: &egui::Context) {
        if self.document_dirty() {
            self.confirm = Some(pending);
        } else {
            self.perform(pending, ctx);
        }
    }

    /// Exécute une opération confirmée (ou sans rien à perdre).
    fn perform(&mut self, pending: Pending, ctx: &egui::Context) {
        match pending {
            Pending::Close => self.close_document(),
            Pending::OpenDemo => self.open_demo(),
            Pending::OpenDialog => {
                self.open_confirmed_for = self.document.as_ref().map(OpenDocument::cache_key);
                self.open_dialog(ctx);
            }
            Pending::NewProject => self.new_project(),
            Pending::Quit => {
                self.quit_confirmed = true;
                #[cfg(not(target_arch = "wasm32"))]
                self.recovery_closed();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Pending::Install(boxed) => {
                let (loaded, origin) = *boxed;
                self.install(loaded, origin);
            }
            Pending::OpenPath(path) => self.open_path(&path),
            Pending::OpenDropped(file) => self.open_dropped(&file, ctx),
            #[cfg(not(target_arch = "wasm32"))]
            Pending::Restore(id) => self.restore_orphan(&id),
            Pending::InstallCsv(boxed) => {
                let (loaded, origin, (file, remarks)) = *boxed;
                self.install(loaded, origin);
                if remarks > 0 {
                    let count = i64::try_from(remarks).unwrap_or(i64::MAX);
                    let text = self.texts.get_args(
                        TextId::CiImported,
                        &[("count", Arg::Count(count)), ("name", Arg::Str(file.into()))],
                    );
                    self.push_message(Severity::Warning, text);
                }
            }
        }
    }

    /// Document importé d'un CSV (J8, lot L10) : installé après le bandeau
    /// de confirmation si le document courant est modifié.
    pub(crate) fn request_install_csv(
        &mut self,
        loaded: LoadedDocument,
        origin: DocOrigin,
        remarks: (String, usize),
        ctx: &egui::Context,
    ) {
        self.request(Pending::InstallCsv(Box::new((loaded, origin, remarks))), ctx);
    }

    /// Réponse au bandeau de confirmation.
    fn answer(&mut self, choice: ConfirmChoice, ctx: &egui::Context) {
        let Some(pending) = self.confirm.take() else {
            return;
        };
        match choice {
            ConfirmChoice::Save => self.start_save(false, Some(pending), ctx),
            ConfirmChoice::Discard => self.perform(pending, ctx),
            ConfirmChoice::Cancel => {}
        }
    }

    /// Ferme le document courant, sans confirmation.
    fn close_document(&mut self) {
        self.document = None;
        self.rows = None;
        self.row_view = RowView::default();
        self.row_view_key = None;
        self.diagnostics = DiagnosticsView::default();
        self.selection = None;
        self.view = self.fresh_view(false);
        self.notify_observer();
    }

    /// Lance un enregistrement : octets produits ici (`save_bytes`, qui
    /// n'altère rien), écrits par le [`FileService`]. Chemin connu et pas
    /// « Enregistrer sous » : écriture directe ; sinon boîte de dialogue
    /// (desktop) ou téléchargement (web). Un seul enregistrement à la fois
    /// sur le desktop (deux écritures du même fichier pourraient finir dans
    /// le désordre) ; sur le web, une demande restée sans réponse est
    /// abandonnée, comme pour l'ouverture.
    fn start_save(&mut self, save_as: bool, then: Option<Pending>, ctx: &egui::Context) {
        if self.saving.is_some() {
            if cfg!(target_arch = "wasm32") {
                log::warn!("enregistrement précédent sans réponse : abandonné");
                let (save_tx, save_rx) = mpsc::channel();
                self.save_tx = save_tx;
                self.save_rx = save_rx;
                self.saving = None;
            } else {
                let text = self.texts.get(TextId::SaveInProgress);
                self.push_message(Severity::Info, text);
                return;
            }
        }
        let Some(document) = &self.document else {
            return;
        };
        let written = document.session.save_bytes(&self.state.write_options());
        let target = save_target(document, save_as, &self.texts);
        self.saving = Some(SaveJob {
            key: document.cache_key(),
            then,
            adjusted: written.diagnostics.len(),
        });
        let request = SaveRequest {
            bytes: written.xml,
            kind: SaveKind::Mspdi,
            target,
            language: self.texts.language(),
        };
        self.env.files.save(ctx, request, self.save_tx.clone());
    }

    /// Fichier → Exporter (J8, lot L13) : octets du document courant au
    /// format `kind`, produits par `plannercraft_automation` (règle 3 : dates du
    /// moteur), remis au [`FileService`] ; une erreur devient un message.
    /// PDF : style du document, sinon Classique, mise en page du style
    /// (`project.export_pdf`) ; SVG : style affiché à l'écran.
    fn start_export(&mut self, kind: SaveKind, ctx: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        let lang = self.text_language();
        let session = &document.session;
        let bytes = match kind {
            SaveKind::Mspdi => Ok(session.save_bytes(&self.state.write_options()).xml),
            SaveKind::Csv => {
                let locale = match lang {
                    plannercraft_automation::text::Language::Fr => {
                        plannercraft_automation::csv::CsvLocale::Fr
                    }
                    plannercraft_automation::text::Language::En => {
                        plannercraft_automation::csv::CsvLocale::En
                    }
                };
                let options = plannercraft_automation::csv::CsvExportOptions::new(locale);
                Ok(plannercraft_automation::csv::export(
                    session.project(),
                    session.schedule(),
                    &options,
                ))
            }
            SaveKind::Svg => Ok(export_svg(session, &self.state, lang, self.printed_rows()).into_bytes()),
            SaveKind::Pdf => {
                // Document sans style : modèle par défaut des préférences
                // (H-UI-43 ; Sobre depuis J8 lot L15), comme l'écran, et non
                // le Classique de la CLI.
                let style = plannercraft_automation::command::style::doc_style(session.project())
                    .is_none()
                    .then(|| self.state.default_template.into());
                let params = plannercraft_automation::command::params::ExportPdfParams {
                    path: String::new(),
                    style,
                    page: None,
                    range: None,
                    rows: None,
                };
                let rows = self.printed_rows();
                plannercraft_automation::pdf::export_rows(session, &params, rows.as_ref(), lang)
                    .map(|export| export.bytes)
                    .map_err(|error| error.message)
            }
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(detail) => {
                self.report_export_error(detail);
                return;
            }
        };
        self.deliver_export(kind, bytes, ctx);
    }

    /// Remet les octets d'un export au `FileService` (boîte « Enregistrer
    /// sous » sur le desktop, téléchargement sur le web) ; partagé avec
    /// l'aperçu du mode impression (J8 lot L15).
    pub(crate) fn deliver_export(&mut self, kind: SaveKind, bytes: Vec<u8>, ctx: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        let request = SaveRequest {
            bytes,
            kind,
            target: SaveTarget::Dialog {
                suggested_name: kind.file_name(&document.origin.name),
                title: self.texts.get(TextId::MenuExport),
                filter: kind.extension().to_ascii_uppercase(),
            },
            language: self.texts.language(),
        };
        self.env.files.save(ctx, request, self.export_tx.clone());
    }

    /// Message d'un export impossible.
    fn report_export_error(&mut self, detail: String) {
        let text = self
            .texts
            .get_args(TextId::ExportErrorFailed, &[("detail", Arg::Str(detail.into()))]);
        self.push_titled(Severity::Error, Some(TextId::ExportErrorTitle), text);
    }

    /// Relève les résultats d'export arrivés depuis l'image précédente.
    fn poll_export(&mut self) {
        while let Ok(event) = self.export_rx.try_recv() {
            match event {
                SaveEvent::Cancelled => {}
                SaveEvent::Failed(detail) => self.report_export_error(detail),
                SaveEvent::Saved { name, .. } | SaveEvent::SavedFile { name, .. } => {
                    self.clear_titled(TextId::ExportErrorTitle);
                    let text = self
                        .texts
                        .get_args(TextId::ExportDone, &[("name", Arg::Str(name.into()))]);
                    self.push_message(Severity::Info, text);
                }
            }
        }
    }

    /// Fichiers déposés sur la fenêtre (J8, lot L13, H-UI-56) : le premier
    /// est ouvert comme par « Ouvrir » (bandeau de confirmation si le
    /// document courant est modifié) ; plusieurs ⇒ message nommant celui
    /// qui est ouvert.
    fn take_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        let Some(first) = dropped.first().cloned() else {
            return;
        };
        if dropped.len() > 1 {
            let count = i64::try_from(dropped.len()).unwrap_or(i64::MAX);
            let text = self.texts.get_args(
                TextId::DroppedFirstOnly,
                &[
                    ("count", Arg::Count(count)),
                    ("name", Arg::Str(dropped_name(&first).into())),
                ],
            );
            self.push_message(Severity::Warning, text);
        }
        self.request(Pending::OpenDropped(first), ctx);
    }

    /// Ouvre un fichier déposé (desktop) : chemin absolu ⇒ comme un fichier
    /// récent (lié au chemin, ajouté aux fichiers récents) ; sinon octets
    /// lus par egui. Une erreur de lecture devient un message.
    #[cfg(not(target_arch = "wasm32"))]
    fn open_dropped(&mut self, file: &egui::DroppedFileHandle, _ctx: &egui::Context) {
        let path = file.path();
        if path.is_absolute()
            && let Some(path) = path.to_str()
        {
            self.open_path(path);
            return;
        }
        let name = dropped_name(file);
        match file.bytes() {
            Ok(bytes) => {
                let origin = DocOrigin {
                    name,
                    path: None,
                    demo: false,
                };
                self.open_bytes(&bytes, origin);
            }
            Err(detail) => self.report_open_error(&OpenError::Read(detail), &name),
        }
    }

    /// Ouvre un fichier déposé (web) : lecture asynchrone du navigateur,
    /// résultat relevé comme celui du sélecteur ([`Self::poll_io`] : une
    /// modification arrivée entre-temps redemande confirmation).
    #[cfg(target_arch = "wasm32")]
    fn open_dropped(&mut self, file: &egui::DroppedFileHandle, ctx: &egui::Context) {
        self.open_confirmed_for = self.document.as_ref().map(OpenDocument::cache_key);
        if self.opening {
            let (tx, rx) = mpsc::channel();
            self.tx = tx;
            self.rx = rx;
        }
        self.opening = true;
        let now = (self.env.now)();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let origin = DocOrigin {
            name: dropped_name(file),
            path: None,
            demo: false,
        };
        // Taille contrôlée avant la lecture (relecture finale de J8) : un
        // fichier de plusieurs Go ferait tomber le module wasm.
        if let Some(web) = file.web_file()
            && let Err(error) = document::check_blob_size(web.size())
        {
            self.opening = false;
            self.report_open_error(&error, &origin.name);
            return;
        }
        let file = std::sync::Arc::clone(file);
        wasm_bindgen_futures::spawn_local(async move {
            let result = match file.bytes_async().await {
                Ok(bytes) => document::load(&bytes, now),
                Err(detail) => Err(OpenError::Read(detail)),
            };
            if tx.send(OpenEvent::Opened { origin, result }).is_err() {
                log::warn!("fichier déposé : demande abandonnée");
            }
            ctx.request_repaint();
        });
    }

    /// Relève les résultats d'enregistrement arrivés depuis l'image
    /// précédente.
    fn poll_save(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.save_rx.try_recv() {
            let Some(job) = self.saving.take() else {
                continue;
            };
            match event {
                // Annulé : l'opération suspendue est abandonnée, le
                // document reste modifié.
                SaveEvent::Cancelled => {}
                SaveEvent::Failed(detail) => {
                    let text = self
                        .texts
                        .get_args(TextId::SaveErrorWrite, &[("detail", Arg::Str(detail.into()))]);
                    self.push_titled(Severity::Error, Some(TextId::SaveErrorTitle), text);
                }
                SaveEvent::Saved { name, path } => self.finish_save(job, name, path, None, ctx),
                SaveEvent::SavedFile { name, file } => self.finish_save(job, name, None, Some(file), ctx),
            }
        }
    }

    /// Enregistrement réussi : nouvelle origine et fichier du navigateur lié
    /// (`file`, H-UI-204 ; `None` après un téléchargement), `mark_saved` si
    /// le document est resté celui dont les octets ont été écrits, puis
    /// opération suspendue éventuelle. Un document fermé ou remplacé
    /// entre-temps n'est pas touché.
    fn finish_save(
        &mut self,
        job: SaveJob,
        name: String,
        path: Option<String>,
        file: Option<crate::io::WebFile>,
        ctx: &egui::Context,
    ) {
        let Some(document) = &mut self.document else {
            return;
        };
        if document.opening != job.key.0 {
            return;
        }
        if let Some(path) = &path {
            self.state.remember_recent(path, &name);
        }
        document.set_origin(DocOrigin {
            name,
            path,
            demo: false,
        });
        document.web_file = file;
        let saved = document.generation() == job.key.1;
        if saved {
            document.session.mark_saved();
        }
        self.clear_titled(TextId::SaveErrorTitle);
        if job.adjusted > 0 {
            let count = i64::try_from(job.adjusted).unwrap_or(i64::MAX);
            let text = self
                .texts
                .get_args(TextId::SaveAdjusted, &[("count", Arg::Count(count))]);
            self.push_message(Severity::Info, text);
        }
        self.notify_observer();
        if saved && let Some(pending) = job.then {
            self.perform(pending, ctx);
        }
    }

    /// Fermeture de la fenêtre demandée (desktop) : annulée si le document
    /// est modifié (bandeau de confirmation) ou si un enregistrement est en
    /// cours (la fenêtre se ferme quand il réussit).
    fn guard_close(&mut self, ctx: &egui::Context) {
        if self.quit_confirmed || !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        if let Some(job) = &mut self.saving {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if job.then.is_none() {
                job.then = Some(Pending::Quit);
            }
        } else if self.document_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Pending::Quit);
        } else {
            // Fermeture acceptée, rien à perdre : journal supprimé.
            #[cfg(not(target_arch = "wasm32"))]
            self.recovery_closed();
        }
    }

    /// Quitter demandé par l'application (Ctrl/Cmd+Q, menu Fichier), et non
    /// par la fenêtre : même garde que [`App::guard_close`]. Pendant un
    /// enregistrement, la fermeture suit son succès ; document modifié :
    /// bandeau ; sinon la fenêtre se ferme. Sans effet sur le web (une page
    /// ne ferme pas son onglet).
    fn request_quit(&mut self, ctx: &egui::Context) {
        if cfg!(target_arch = "wasm32") {
            return;
        }
        if let Some(job) = &mut self.saving {
            if job.then.is_none() {
                job.then = Some(Pending::Quit);
            }
        } else {
            self.request(Pending::Quit, ctx);
        }
    }

    /// Ouvre le projet d'exemple embarqué (synchrone).
    pub fn open_demo(&mut self) {
        let origin = DocOrigin {
            name: self.texts.get(TextId::DemoFileName),
            path: None,
            demo: true,
        };
        self.open_bytes(crate::DEMO_MSPDI, origin);
    }

    /// Ouvre un document à partir de ses octets (synchrone : tests, exemple).
    /// Un échec devient un message ; le document précédent reste affiché.
    pub fn open_bytes(&mut self, bytes: &[u8], origin: DocOrigin) {
        let now = (self.env.now)();
        match document::load(bytes, now) {
            Ok(loaded) => self.install(loaded, origin),
            Err(error) => self.report_open_error(&error, &origin.name),
        }
    }

    /// Ouvre un projet déjà construit, sans MSPDI (N8 : réseaux générés des
    /// tests et de l'exemple `snapshot`) : planning calculé, historique
    /// vide, aucun diagnostic d'import.
    pub fn open_project(&mut self, project: plannercraft_core::Project, origin: DocOrigin) {
        let now = (self.env.now)();
        self.install(document::load_project(project, now), origin);
    }

    /// Ouvre le fichier `path` (synchrone : argument de la ligne de
    /// commande, fichier récent). Un échec devient un message nommant le
    /// fichier ; le document précédent reste affiché. Un fichier récent
    /// introuvable est retiré de la liste (H-UI-28).
    pub fn open_path(&mut self, path: &str) {
        let name = document::file_name(path);
        // Lecture bornée (relecture finale de J8) : un fichier de plus de
        // 256 Mio est refusé avant d'être lu.
        let bytes = match document::read_path(std::path::Path::new(path)) {
            Ok(bytes) => bytes,
            Err(error) => {
                if !std::path::Path::new(path).exists() {
                    self.state.forget_recent(path);
                }
                self.report_open_error(&error, &name);
                return;
            }
        };
        let origin = DocOrigin {
            name,
            path: Some(path.to_owned()),
            demo: false,
        };
        self.open_bytes(&bytes, origin);
    }

    /// Fichier récent `index` (H-UI-28) : absent ⇒ retiré de la liste et
    /// signalé ; sinon ouvert, après confirmation si le document courant est
    /// modifié.
    fn open_recent(&mut self, index: usize, ctx: &egui::Context) {
        let Some(recent) = self.state.recent.get(index).cloned() else {
            return;
        };
        if !std::path::Path::new(&recent.path).exists() {
            self.state.forget_recent(&recent.path);
            let text = self.texts.get_args(
                TextId::OpenErrorMissing,
                &[
                    ("name", Arg::Str(recent.name.into())),
                    ("detail", Arg::Str(recent.path.into())),
                ],
            );
            self.push_titled(Severity::Error, Some(TextId::OpenErrorTitle), text);
            return;
        }
        self.request(Pending::OpenPath(recent.path), ctx);
    }

    /// Met une action en file ; elle sera appliquée par le réducteur en fin
    /// de la prochaine image (les effets sur le contexte egui, comme le
    /// thème, demandent ce contexte).
    pub fn dispatch(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// Messages affichés (bandeau des décisions et notifications), dans
    /// l'ordre de publication (H-UI-104).
    #[must_use]
    pub fn messages(&self) -> &[UserMessage] {
        self.messages.live()
    }

    /// Pile des messages : notifications, bandeau, historique, non lus
    /// (H-UI-104).
    #[must_use]
    pub const fn message_store(&self) -> &crate::notify::MessageStore {
        &self.messages
    }

    /// Messages traduits de la langue courante.
    #[must_use]
    pub const fn texts(&self) -> &Texts {
        &self.texts
    }

    /// Modèle de lignes du document ouvert (dans la langue courante).
    #[must_use]
    pub const fn rows(&self) -> Option<&RowModel> {
        self.rows.as_ref()
    }

    /// Vue des lignes du document ouvert (J8, lot L5) : lignes affichées
    /// (vide sans document).
    #[must_use]
    pub const fn row_view(&self) -> &RowView {
        &self.row_view
    }

    /// Tâches imprimées (relecture finale de J8, F1) : lignes de la vue
    /// affichée (filtre, niveau de plan, récapitulatives repliées), comme
    /// MS Project qui imprime la vue courante ; `None` si toutes les lignes
    /// sont affichées (ou sans document). L'ordre imprimé reste celui des
    /// ID.
    #[must_use]
    pub fn printed_rows(&self) -> Option<std::collections::BTreeSet<TaskUid>> {
        let model = self.rows.as_ref()?;
        // Vue regroupée (finitions P3) : les tâches de tous les groupes,
        // repliés compris ; le rendu les regroupe lui-même.
        if self.row_view.is_grouped() {
            return Some(
                self.row_view
                    .members()
                    .iter()
                    .filter_map(|m| model.row(m.index()).map(|row| row.uid))
                    .collect(),
            );
        }
        if self.row_view.len() >= self.row_view.model_len() {
            return None;
        }
        Some(
            self.row_view
                .order()
                .iter()
                .filter_map(|m| model.row(m.index()).map(|row| row.uid))
                .collect(),
        )
    }

    /// Lignes affichées du document ouvert (modèle lu à travers sa vue).
    #[must_use]
    pub fn visible_rows(&self) -> Option<VisibleRows<'_>> {
        self.rows
            .as_ref()
            .map(|model| VisibleRows::new(model, &self.row_view))
    }

    /// État transitoire de la vue (défilements, ajustement en attente).
    #[must_use]
    pub const fn view(&self) -> &ViewTransient {
        &self.view
    }

    /// État transitoire de la vue, modifiable (tests de kittest : brouillon
    /// d'une fenêtre ouverte ; J8 lot L15). Jamais appelé par les
    /// applications.
    #[doc(hidden)]
    pub const fn view_mut(&mut self) -> &mut ViewTransient {
        &mut self.view
    }

    /// Volet latéral (J8 lot L12).
    #[must_use]
    pub const fn side_panel(&self) -> &crate::view::side_panel::SidePanelState {
        &self.side
    }

    /// Assistant IA (ADR-0024, lot A1).
    #[must_use]
    pub const fn assistant(&self) -> &crate::assistant::AssistantPanel {
        &self.assistant
    }

    /// Assistant IA, modifiable (tests : saisie de la question).
    pub const fn assistant_mut(&mut self) -> &mut crate::assistant::AssistantPanel {
        &mut self.assistant
    }

    /// Branche les services de l'assistant (desktop : réseau réel,
    /// trousseau, navigateur ; web : `fetch`, mémoire). Sans cet appel,
    /// l'assistant est hors ligne (tests).
    pub fn set_assistant_services(&mut self, services: crate::assistant::AssistantServices) {
        self.assistant.set_services(services);
    }

    /// État de l'interface rendu à l'assistant (`ui_state`) : document,
    /// tâche active, sélection (200 UID au plus), vue. Ni préférences ni
    /// fichiers récents.
    #[must_use]
    pub fn assistant_ui_state(&self) -> serde_json::Value {
        let mut selection: Vec<u32> = self.selected.click_order().iter().map(|uid| uid.get()).collect();
        selection.sort_unstable();
        selection.truncate(200);
        serde_json::json!({
            "document": self.document.as_ref().map(|d| d.origin.name.as_str()),
            "active": self.selection.map(TaskUid::get),
            "selection": selection,
            "view": self.state.view,
        })
    }

    /// Document de la conversation affichée (lot A1b) : clé brute
    /// (`demo`, `path:<chemin>`, `name:<nom>`), canonisée seulement quand
    /// elle change (chemin canonique sur le desktop).
    fn sync_assistant_document(&mut self) {
        let raw = self.document.as_ref().map(|d| {
            if d.origin.demo {
                "demo".to_owned()
            } else if let Some(path) = &d.origin.path {
                format!("path:{path}")
            } else {
                format!("name:{}", d.origin.name)
            }
        });
        if raw.as_deref() == self.assistant.doc_raw() {
            return;
        }
        let document = raw.map(|raw| {
            let key = canonical_doc_key(&raw);
            (key, raw)
        });
        self.assistant.switch_document(document);
    }

    /// Conversations de l'assistant gardées par document, conversation
    /// affichée comprise (sauvegarde : stockage d'eframe, jamais `UiState`).
    #[must_use]
    pub fn assistant_archive_json(&mut self) -> String {
        self.assistant.archive_json()
    }

    /// Relit les conversations gardées (démarrage).
    pub fn load_assistant_archive(&mut self, json: &str) {
        self.assistant.load_archive(json);
    }

    /// Environnement d'une commande de l'assistant : langue, contexte de
    /// l'interface (sélection, vue) joint à la question, heure.
    fn assistant_env(&self) -> crate::assistant::AssistantEnv {
        let ui_state = self.assistant_ui_state();
        let context = self.texts.get_args(
            TextId::AssistantContext,
            &[("json", Arg::Str(ui_state.to_string().into()))],
        );
        let minutes = (self.env.now)().time_of_day().minutes();
        crate::assistant::AssistantEnv {
            lang: self.text_language(),
            context: self.document.is_some().then_some(context),
            asked_at: Some(format!("{:02}:{:02}", minutes / 60, minutes % 60)),
            continue_prompt: self.texts.get(TextId::AssistantContinuePrompt),
        }
    }

    /// Mène l'assistant (début d'image) : magasin, connexion, flux, outils
    /// sur une copie de la session (lecture seule).
    fn poll_assistant(&mut self, ctx: &egui::Context) {
        self.assistant.ensure_waker(ctx);
        self.assistant.conversation.set_now(self.clock);
        self.sync_assistant_document();
        self.assistant.start_loads(&self.state.assistant);
        self.assistant.set_focused(ctx.input(|i| i.focused));
        let mut changed = self.assistant.poll_services(&mut self.state.assistant);
        let was_busy = self.assistant.conversation.busy();
        if self.assistant.conversation.busy() {
            let lang = self.text_language();
            let mut host = plannercraft_assistant::tools::SessionHost::new(
                self.document.as_ref().map(|d| &d.session),
                plannercraft_automation::CallEnv::new((self.env.now)(), lang),
                self.assistant_ui_state(),
            );
            host.next_proposal = self.next_proposal;
            let transport = self.assistant.transport();
            changed |= self.assistant.conversation.poll(&mut host, transport.as_ref());
            self.next_proposal = host.next_proposal;
            let proposals = std::mem::take(&mut host.proposals);
            drop(host);
            // Proposition arrivée volet fermé : pastille (lot A5).
            self.note_assistant_events(u32::try_from(proposals.len()).unwrap_or(u32::MAX));
            self.queue_proposals(proposals);
        }
        // Réponse terminée ou en échec volet fermé : pastille (lot A5) ;
        // la boucle tourne ici, à chaque image, que le volet soit affiché
        // ou non (H-UI-166).
        if was_busy && !self.assistant.conversation.busy() {
            let ended = self.assistant.conversation.turns().last().is_some_and(|turn| {
                matches!(
                    turn.state,
                    plannercraft_assistant::conversation::TurnState::Done
                        | plannercraft_assistant::conversation::TurnState::Failed(_)
                )
            });
            if ended {
                self.note_assistant_events(1);
            }
        }
        // Lot A4 : solde annoncé par le relais, session refusée, compte
        // relu à la fin de chaque demande.
        self.assistant
            .after_conversation_poll(was_busy, self.state.assistant.access);
        if changed {
            ctx.request_repaint();
        }
    }

    /// Compte `n` nouveautés de l'assistant si le volet est fermé (lot A5,
    /// H-UI-166).
    fn note_assistant_events(&mut self, n: u32) {
        if !self.side.open {
            self.assistant_badge = self.assistant_badge.saturating_add(n);
        }
    }

    /// Nouveautés de l'assistant arrivées volet fermé (pastille du bouton,
    /// lot A5).
    #[must_use]
    pub const fn assistant_badge(&self) -> u32 {
        self.assistant_badge
    }

    /// Palette de commandes (lot U3) : ouverte, saisie, lignes affichées.
    #[must_use]
    pub const fn palette(&self) -> &crate::palette::PaletteState {
        &self.palette
    }

    /// Ouvre ou ferme la palette (lot U3, H-UI-177) : jamais sous une
    /// fenêtre.
    fn palette_op(&mut self, op: crate::palette::PaletteOp) {
        match op {
            crate::palette::PaletteOp::Open => {
                if self.view.dialog.is_none() && !self.view_dialog_open() {
                    self.palette.open();
                }
            }
            crate::palette::PaletteOp::Close => self.palette.close(),
        }
    }

    /// « Demander à l'assistant » (lot U3, H-UI-182) : volet ouvert sur
    /// l'onglet Assistant, question préremplie, focus demandé ; rien n'est
    /// envoyé.
    fn ask_assistant(&mut self, text: String) {
        self.side_op(crate::view::side_panel::SideOp::Open(
            crate::view::side_panel::SideTab::Assistant,
        ));
        self.assistant.draft = text;
        self.assistant.focus_request = true;
    }

    /// Touches de la palette ouverte (lot U3), lues avant le dessin : ↑ ↓,
    /// Entrée, Tab ; Échap et Ctrl/Cmd+K la referment. Les autres
    /// raccourcis ne sont pas lus pendant qu'elle est ouverte.
    fn palette_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let toggle = crate::keymap::shortcut_for(crate::keymap::Binding::Ui(
            crate::keymap::UiBinding::CommandPalette,
        ));
        let keys = &mut self.palette.keys;
        let close = ctx.input_mut(|input| {
            let down = input.count_and_consume_key(Modifiers::NONE, Key::ArrowDown);
            let up = input.count_and_consume_key(Modifiers::NONE, Key::ArrowUp);
            let delta = i64::try_from(down)
                .unwrap_or(0)
                .saturating_sub(i64::try_from(up).unwrap_or(0));
            keys.moves = i32::try_from(delta).unwrap_or(0);
            keys.run = input.consume_key(Modifiers::NONE, Key::Enter);
            keys.ask = input.consume_key(Modifiers::NONE, Key::Tab);
            input.consume_key(Modifiers::NONE, Key::Escape)
                || toggle.is_some_and(|shortcut| input.consume_shortcut(shortcut))
        });
        if close {
            self.palette.close();
        }
    }

    /// Commande du volet latéral (lot A5) : largeur écrite dans `UiState`
    /// (bornée) ; à l'ouverture sur l'onglet Assistant, focus demandé à la
    /// saisie ; pastille effacée dès que le volet est ouvert.
    fn side_op(&mut self, op: crate::view::side_panel::SideOp) {
        use crate::view::side_panel::SideOp;
        if let SideOp::SetWidth(width) = op {
            self.state.side_panel.width =
                width.clamp(crate::state::SIDE_WIDTH_MIN, crate::state::SIDE_WIDTH_MAX);
            return;
        }
        let shown = self.side.shows_assistant();
        self.side.apply(op);
        if self.side.shows_assistant() && !shown {
            self.assistant.focus_request = true;
        }
        self.sync_side_prefs();
    }

    /// Recopie ouvert ou fermé dans `UiState` (mémorisé, lot A5) et efface
    /// la pastille quand le volet est ouvert.
    fn sync_side_prefs(&mut self) {
        self.state.side_panel.open = self.side.open;
        if self.side.open {
            self.assistant_badge = 0;
        }
    }

    /// Échap réduit le volet de l'assistant au rail (lot A5, H-UI-162 ;
    /// lot R1c) quand
    /// personne d'autre ne l'a prise dans l'image (fenêtre, liste,
    /// surimpression du ruban, réponse en cours arrêtée par la saisie) :
    /// onglet Assistant affiché, question vide, aucune réponse en cours,
    /// aucune fenêtre ouverte, focus dans la saisie ou nulle part à l'image
    /// précédente.
    fn escape_closes_assistant(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let question = view::assistant::question_id();
        let focus_ok = self.last_focus.is_none_or(|id| id == question);
        let free = self.side.shows_assistant()
            && focus_ok
            && self.assistant.draft.trim().is_empty()
            && !self.assistant.conversation.busy()
            && self.view.dialog.is_none()
            && !self.view_dialog_open()
            && self.confirm.is_none()
            && !self.popup_open
            && !self.palette.open
            && self.view.ribbon_overlay.is_none();
        if free && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            actions.push(Action::SidePanel(view::side_panel::SideOp::Close));
            if self.document.is_some() {
                self.view.focus_table_pending = true;
            }
        }
    }

    /// Ctrl/Cmd+J (lot A5, H-UI-162) : lu avant les autres raccourcis,
    /// saisie de texte comprise (la question de l'assistant le referme),
    /// mais pas sous une fenêtre.
    fn assistant_key(&self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if self.view.dialog.is_some() || self.view_dialog_open() {
            return;
        }
        let binding = crate::keymap::Binding::Ui(crate::keymap::UiBinding::ToggleAssistant);
        if let Some(shortcut) = crate::keymap::shortcut_for(binding)
            && ctx.input_mut(|i| i.consume_shortcut(shortcut))
            && let Some(action) = crate::keymap::binding_action(binding)
        {
            actions.push(action);
        }
    }

    /// Une ouverture par le sélecteur de fichiers est en cours.
    #[must_use]
    pub const fn is_opening(&self) -> bool {
        self.opening
    }

    /// Diagnostics du document ouvert, triés par gravité et mis en texte une
    /// fois par document et par langue, avec leurs compteurs (vide sans
    /// document).
    #[must_use]
    pub const fn diagnostics_view(&self) -> &DiagnosticsView {
        &self.diagnostics
    }

    /// Ajoute un message au bandeau : un texte déjà affiché n'est pas
    /// répété, et seuls les [`MAX_MESSAGES`] plus récents sont gardés.
    pub(crate) fn push_message(&mut self, severity: Severity, text: String) {
        self.push_titled(severity, None, text);
    }

    /// Comme [`App::push_message`], avec un titre.
    pub(crate) fn push_titled(&mut self, severity: Severity, title: Option<TextId>, text: String) {
        self.push_detailed(severity, title, text, None);
    }

    /// Comme [`App::push_titled`], avec un détail technique repliable
    /// (H-UI-105) : publié dans [`crate::notify::MessageStore`], sorte
    /// déduite de la gravité et du titre ([`crate::notify::kind_for`]).
    pub(crate) fn push_detailed(
        &mut self,
        severity: Severity,
        title: Option<TextId>,
        text: String,
        detail: Option<String>,
    ) {
        let kind = crate::notify::kind_for(severity, title);
        self.messages.post(UserMessage {
            detail,
            posted_at: self.clock,
            ..UserMessage::new(kind, severity, title, text)
        });
    }

    /// Retire les messages de titre `title` (refus devenus caducs).
    pub(crate) fn clear_titled(&mut self, title: TextId) {
        self.messages.retain(|message| message.title != Some(title));
    }

    /// Affiche un refus de commande : message traduit par son code, détail
    /// du moteur dans la langue de l'interface.
    pub(crate) fn report_command_error(&mut self, error: &CommandError) {
        let project = self.document.as_ref().map(OpenDocument::project);
        let (text, detail) =
            crate::error_text::user_message(error, project, &self.texts, self.text_language());
        let detail = (!detail.is_empty()).then_some(detail);
        self.push_detailed(Severity::Error, Some(TextId::CommandErrorTitle), text, detail);
    }

    /// Signale (une fois) un état d'interface illisible ou trop récent.
    pub fn report_state_load(&mut self, error: &StateLoadError) {
        let text = self.texts.get_args(
            TextId::StateLoadWarning,
            &[("detail", Arg::Str(error.to_string().into()))],
        );
        // État illisible : constat qui attend d'être lu (bandeau, H-UI-104).
        self.messages.post(UserMessage {
            posted_at: self.clock,
            ..UserMessage::new(
                crate::notify::MessageKind::Blocking,
                Severity::Warning,
                None,
                text,
            )
        });
    }

    /// Vue remise à zéro pour un nouveau document (ou aucun), en gardant la
    /// largeur mesurée du Gantt pour ajuster le zoom dès la première image.
    fn fresh_view(&self, needs_fit: bool) -> ViewTransient {
        ViewTransient {
            pending_scroll_y: Some(0.0),
            gantt_scroll_x: 0.0,
            zoom_accum: 0.0,
            needs_fit,
            last_gantt_width: self.view.last_gantt_width,
            last_gantt_left: self.view.last_gantt_left,
            edit: TableEdit::default(),
            // Préférences de la vue des tâches (niveau affiché, surbrillance
            // du critique) ; filtre, tri et repli repartent de zéro.
            task_view: TaskViewState {
                outline_level: self.state.task_view.outline_level,
                highlight_critical: self.state.task_view.highlight_critical,
                ..TaskViewState::default()
            },
            ..ViewTransient::default()
        }
    }

    /// Installe un document chargé et reconstruit les caches.
    fn install(&mut self, loaded: LoadedDocument, origin: DocOrigin) {
        let opening = self.next_generation;
        self.next_generation = opening.saturating_add(1);
        let range_truncated = loaded.range_truncated;
        let document = OpenDocument::new(loaded, origin, opening);
        self.rows = Some(RowModel::build(&document, self.texts.format_labels()));
        self.diagnostics = DiagnosticsView::build(&document, &self.texts);
        if let Some(path) = &document.origin.path {
            self.state.remember_recent(path, &document.origin.name);
        }
        self.document = Some(document);
        self.view = self.fresh_view(true);
        self.view.focus_table_pending = true;
        self.row_view_key = None;
        self.ensure_row_view();
        // Curseur sur la première ligne affichée, colonne Nom, focus à la
        // table (J7 lot L1) ; projet sans tâche : la ligne vide.
        self.selection = self.visible_rows().and_then(|rows| rows.uid(DisplayRow(0)));
        self.selected = Selection::single(self.selection);
        self.sync_selection();
        // Une ouverture réussie rend caduques les erreurs et avertissements
        // d'ouverture précédents.
        let truncated = self.texts.get(TextId::RangeTruncated);
        self.messages
            .retain(|message| message.severity != Severity::Error && message.text != truncated);
        if range_truncated {
            self.push_message(Severity::Warning, truncated);
        }
        self.notify_observer();
    }

    /// Transforme une erreur d'ouverture en message traduit (le titre
    /// `open-error-title` est ajouté par le bandeau), nommant le fichier
    /// `name` s'il est connu (N10).
    pub(crate) fn report_open_error(&mut self, error: &OpenError, name: &str) {
        let (id, named, detail) = match error {
            OpenError::Read(detail) => (TextId::OpenErrorRead, TextId::OpenErrorReadFile, detail.clone()),
            // Détail traduit par code (N9, J8 lot L14).
            OpenError::Mspdi(error) => (
                TextId::OpenErrorMspdi,
                TextId::OpenErrorMspdiFile,
                crate::diag_text::DiagTexts::new(&self.texts).mspdi_error(error),
            ),
        };
        let text = if name.is_empty() {
            self.texts.get_args(id, &[("detail", Arg::Str(detail.into()))])
        } else {
            self.texts.get_args(
                named,
                &[
                    ("name", Arg::Str(name.into())),
                    ("detail", Arg::Str(detail.into())),
                ],
            )
        };
        self.push_titled(Severity::Error, Some(TextId::OpenErrorTitle), text);
    }

    /// Relève les résultats d'ouverture arrivés depuis l'image précédente
    /// (et les fichiers choisis pour l'import CSV, J8 lot L10).
    /// Seule la demande en cours peut répondre : une demande abandonnée
    /// (voir [`App::open_dialog`]) écrit dans un canal fermé.
    fn poll_io(&mut self) {
        // Fichiers choisis pour l'import CSV (J8, lot L10).
        self.poll_picked();
        while let Ok(event) = self.rx.try_recv() {
            self.opening = false;
            match event {
                OpenEvent::Cancelled => {}
                OpenEvent::Opened { origin, result } => match result {
                    Ok(loaded) => {
                        // Le document courant a été modifié depuis la
                        // demande (ou la confirmation) : nouvelle
                        // confirmation avant de le remplacer.
                        let confirmed = self.document.as_ref().is_none_or(|document| {
                            !document.is_dirty() || Some(document.cache_key()) == self.open_confirmed_for
                        });
                        if confirmed {
                            self.install(loaded, origin);
                        } else {
                            self.confirm = Some(Pending::Install(Box::new((loaded, origin))));
                        }
                    }
                    Err(error) => self.report_open_error(&error, &origin.name),
                },
            }
        }
    }

    /// Applique la préférence de thème au contexte egui, seulement si elle a
    /// changé depuis le dernier appel.
    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.state.theme) {
            ctx.set_theme(match self.state.theme {
                ThemePref::System => egui::ThemePreference::System,
                ThemePref::Light => egui::ThemePreference::Light,
                ThemePref::Dark => egui::ThemePreference::Dark,
            });
            // Finitions P4 (H-UI-132) : sélection d'egui à l'accent.
            crate::theme::ChromeTokens::install(ctx);
            self.applied_theme = Some(self.state.theme);
        }
    }

    /// « Réduire les animations » (lot A1b, H-UI-147) : durée des animations
    /// d'egui (défilements, repli) à zéro, ou à sa valeur par défaut ;
    /// seulement si la préférence a changé.
    fn apply_motion(&mut self, ctx: &egui::Context) {
        // Rien à faire au démarrage sans la préférence : le style d'egui
        // garde sa durée par défaut.
        if self.applied_motion.is_none() && !self.state.reduce_motion {
            self.applied_motion = Some(false);
            return;
        }
        if self.applied_motion != Some(self.state.reduce_motion) {
            // Durée d'origine du contexte, relevée avant la première coupure.
            let original = *self
                .motion_default
                .get_or_insert_with(|| ctx.global_style().animation_time);
            let time = if self.state.reduce_motion { 0.0 } else { original };
            ctx.all_styles_mut(|style| style.animation_time = time);
            self.applied_motion = Some(self.state.reduce_motion);
        }
    }

    /// Recrée les messages et le modèle de lignes si la langue résolue a
    /// changé.
    fn apply_language(&mut self) {
        let language = Language::resolve(self.state.language, self.env.system_language);
        if language == self.texts.language() {
            return;
        }
        self.texts = Texts::new(language);
        self.texts_changed();
    }

    /// Pseudo-langue de test (J8 lot L14, `tests/ui/pseudo.rs`) : messages
    /// anglais accentués, allongés de 40 % et encadrés de crochets
    /// ([`crate::i18n::pseudo`]), pour vérifier qu'aucun texte n'est coupé.
    /// La préférence passe à l'anglais (dates, nombres et saisie) ; un
    /// changement de langue rend les vrais messages. Jamais appelée par les
    /// applications.
    #[doc(hidden)]
    pub fn use_pseudo_texts(&mut self) {
        self.state.language = crate::state::LanguagePref::En;
        self.texts = Texts::pseudo();
        self.texts_changed();
    }

    /// Recrée ce qui dépend des messages (nom de l'exemple, modèle de
    /// lignes, diagnostics) après un changement de [`Texts`].
    fn texts_changed(&mut self) {
        if let Some(document) = &mut self.document {
            if document.origin.demo {
                let mut origin = document.origin.clone();
                origin.name = self.texts.get(TextId::DemoFileName);
                document.set_origin(origin);
            }
            self.rows = Some(RowModel::build(document, self.texts.format_labels()));
            self.diagnostics = DiagnosticsView::build(document, &self.texts);
            self.row_view_key = None;
            self.ensure_row_view();
        }
    }

    /// Largeur du Gantt mesurée par la vue à la dernière image (0 tant
    /// qu'aucune image n'a été dessinée).
    fn current_gantt_width(&self) -> f32 {
        let measured = self.view.last_gantt_width;
        if measured.is_finite() && measured > 0.0 {
            measured
        } else {
            0.0
        }
    }

    /// Défilement horizontal maximal du Gantt au zoom courant.
    fn max_gantt_scroll(&self) -> f32 {
        let Some(document) = &self.document else {
            return 0.0;
        };
        let content = TimeAxis::new(document.range, self.state.zoom, 0.0).content_width();
        let max = content - self.current_gantt_width();
        if max.is_finite() { max.max(0.0) } else { 0.0 }
    }

    /// Ramène le défilement horizontal dans `[0, max]`.
    fn clamp_gantt_scroll(&mut self) {
        let max = self.max_gantt_scroll();
        let x = self.view.gantt_scroll_x;
        self.view.gantt_scroll_x = if x.is_finite() { x.clamp(0.0, max) } else { 0.0 };
    }

    /// Ajuste le zoom à la plage du document si c'est demandé et que la
    /// largeur du Gantt est connue. Après une ouverture (défilement nul),
    /// le défilement est seulement borné ; pour le bouton « Ajuster »,
    /// [`App::apply`] ancre ensuite au centre. Rend `true` si le zoom a
    /// changé.
    fn fit_if_needed(&mut self) -> bool {
        if !self.view.needs_fit {
            return false;
        }
        let Some(document) = &self.document else {
            self.view.needs_fit = false;
            return false;
        };
        let width = self.current_gantt_width();
        if !(width.is_finite() && width > 0.0) {
            return false;
        }
        let zoom = fit_zoom(document.range, width);
        self.view.needs_fit = false;
        let changed = zoom != self.state.zoom;
        self.state.zoom = zoom;
        self.clamp_gantt_scroll();
        changed
    }

    /// Change le zoom en gardant à la même place le point du contenu situé
    /// à `anchor` pixels du bord gauche du Gantt (borné à la largeur du
    /// Gantt ; une ancre non finie vaut 0). Arithmétique en pixels
    /// seulement.
    fn zoom_to(&mut self, to: TimeScaleZoom, anchor: f32) {
        let from = self.state.zoom;
        self.view.needs_fit = false;
        if to == from {
            return;
        }
        let anchor = if anchor.is_finite() {
            anchor.clamp(0.0, self.current_gantt_width())
        } else {
            0.0
        };
        self.view.gantt_scroll_x = zoom_anchor(self.view.gantt_scroll_x, anchor, from, to);
        self.state.zoom = to;
        self.clamp_gantt_scroll();
    }

    /// Affichage → Zoom → Tâches sélectionnées (finitions P1, H-UI-124) :
    /// échelle la plus fine où les barres des tâches sélectionnées tiennent
    /// dans la largeur du Gantt ([`crate::timescale::fit_span_zoom`]), puis
    /// défilement jusqu'à la première barre (même marge que « Atteindre la
    /// tâche »). Dates lues du planning (barres affichées des tâches
    /// exclues comprises) ; aucune barre : rien.
    fn zoom_to_selection(&mut self) {
        use plannercraft_core::schedule::TaskOutcome;
        if self.state.view != ViewKind::Gantt {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let schedule = document.schedule();
        let spans = self
            .selected
            .uids()
            .iter()
            .filter_map(|uid| match schedule.get(*uid) {
                Some(TaskOutcome::Scheduled(s)) => Some((s.start, s.finish)),
                Some(TaskOutcome::Excluded { display, .. }) => display.map(|span| (span.start, span.finish)),
                None => None,
            });
        let Some((start, finish)) = spans.reduce(|(a, b), (c, d)| (a.min(c), b.max(d))) else {
            return;
        };
        let range = document.range;
        let width = self.current_gantt_width();
        let Some(zoom) = crate::timescale::fit_span_zoom(start, finish, width) else {
            return;
        };
        self.view.needs_fit = false;
        self.state.zoom = zoom;
        let axis = crate::timescale::TimeAxis::new(range, zoom, 0.0);
        if let Some(x) = crate::nav::scroll_for_x(axis.x(start), width) {
            self.view.gantt_scroll_x = x;
        }
        self.clamp_gantt_scroll();
    }

    /// Centre du Gantt visible, ancre des boutons de zoom et d'« Ajuster ».
    fn gantt_center(&self) -> f32 {
        self.current_gantt_width() / 2.0
    }

    /// Réducteur : applique une action.
    #[allow(clippy::too_many_lines, reason = "réducteur : un bras court par action")]
    pub(crate) fn apply(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::OpenDemo => self.request(Pending::OpenDemo, ctx),
            Action::OpenDialog => self.request(Pending::OpenDialog, ctx),
            Action::CloseDocument => self.request(Pending::Close, ctx),
            Action::NewProject => self.request(Pending::NewProject, ctx),
            Action::Save => self.start_save(false, None, ctx),
            Action::SaveAs => self.start_save(true, None, ctx),
            Action::Quit => self.request_quit(ctx),
            Action::Confirm(choice) => self.answer(choice, ctx),
            Action::Select(selection) => self.selection = selection,
            Action::SetLanguage(language) => {
                self.state.language = language;
                self.apply_language();
            }
            Action::SetTheme(theme) => {
                self.state.theme = theme;
                self.apply_theme(ctx);
            }
            Action::SetReduceMotion(reduce) => self.state.reduce_motion = reduce,
            // Boutons et menu de zoom : ancrés au centre du Gantt (la zone
            // affichée reste à l'écran, comme dans MS Project).
            Action::SetZoom(zoom) => self.zoom_to(zoom, self.gantt_center()),
            Action::FitZoom => {
                if let Some(document) = &self.document {
                    let width = self.current_gantt_width();
                    if width > 0.0 {
                        let zoom = fit_zoom(document.range, width);
                        self.zoom_to(zoom, self.gantt_center());
                    } else {
                        // Largeur encore inconnue : ajustement à la
                        // prochaine image.
                        self.view.needs_fit = true;
                    }
                }
            }
            Action::ZoomStep { steps, anchor_x } => self.zoom_step(steps, anchor_x),
            Action::SetTableWidth(width) => {
                self.state.table_width = width.clamp(TABLE_WIDTH_MIN, TABLE_WIDTH_MAX);
            }
            Action::ScrollGanttX(delta) => {
                if delta.is_finite() {
                    self.view.gantt_scroll_x += delta;
                }
                self.clamp_gantt_scroll();
            }
            Action::ScrollToRow(index) => {
                // Ligne affichée (J8, lot L5).
                let len = self.row_view.len();
                let row = index.min(len.saturating_sub(1));
                self.view.pending_scroll_y = Some(geom::index_to_f32(row) * self.row_h());
            }
            Action::ToggleDiagnostics => self.state.diagnostics_open = !self.state.diagnostics_open,
            Action::DismissMessage => self.messages.clear_live(),
            // Le refus est affiché par `execute` ; rien d'autre à faire ici.
            Action::Exec(batch) => drop(self.execute(&batch, EditOrigin::User)),
            Action::ExecOn { opening, batch } => {
                if self
                    .document
                    .as_ref()
                    .is_some_and(|document| document.opening == opening)
                {
                    drop(self.execute(&batch, EditOrigin::User));
                } else {
                    log::warn!("saisie abandonnée : le document visé n'est plus ouvert");
                }
            }
            Action::Undo => drop(self.undo()),
            Action::Redo => drop(self.redo()),
            Action::SelectWith { uid, mode } => {
                self.sync_selection();
                if let Some(rows) = self
                    .rows
                    .as_ref()
                    .map(|model| VisibleRows::new(model, &self.row_view))
                {
                    self.selected.click(uid, mode, rows);
                    self.selection = self.selected.focus();
                }
            }
            // Feuille des ressources (J7, lot L10) : Suppr supprime la
            // ressource courante ; les autres commandes de structure et la
            // fenêtre d'une tâche visent la table des tâches, absente.
            Action::Structure(op) if self.state.view == ViewKind::ResourceSheet => {
                if op == crate::structure::StructureOp::Delete {
                    self.delete_resource();
                }
            }
            Action::ShowTaskInformation if self.state.view == ViewKind::ResourceSheet => {}
            // Utilisation des ressources (J7, lot L11) : lecture seule, aucune
            // commande de structure ni fenêtre d'une tâche.
            Action::Structure(_) | Action::ShowTaskInformation
                if self.state.view == ViewKind::ResourceUsage => {}
            // H-UI-32 (J8, lot L9) : sous tri ou filtre, déplacer, hausser et
            // abaisser sont refusés, avec un message.
            Action::Structure(op) if self.structure_locked(op) => {
                let text = self.texts.get(self.structure_lock_text());
                self.push_message(Severity::Info, text);
            }
            Action::Structure(op) => self.run_structure(op),
            Action::ConfirmDelete(confirmed) if self.view.resources.pending_delete.is_some() => {
                self.answer_resource_delete(confirmed);
            }
            Action::ConfirmDelete(confirmed) => self.answer_delete(confirmed),
            Action::Nav { mv, .. } if self.state.view == ViewKind::ResourceSheet => {
                if let Some(document) = &self.document {
                    self.view
                        .resources
                        .navigate(document, &self.texts, mv, view::ROW_H);
                }
            }
            Action::Nav { mv, .. } if self.state.view == ViewKind::ResourceUsage => {
                if let Some(document) = &self.document {
                    let ViewTransient { usage, expanded, .. } = &mut self.view;
                    usage.navigate(document, &self.texts, expanded, mv, view::ROW_H);
                }
            }
            Action::Nav { mv, extend } => self.navigate(mv, extend),
            Action::FocusPane(pane) => self.focus_pane(pane, ctx),
            Action::TimescaleStep(n) => {
                // Une graduation fine : un jour de l'échelle (H-UI-18).
                self.scroll_gantt_by(self.state.zoom.px_per_day() * f32::from(n));
            }
            Action::TimescalePage(n) => self.scroll_gantt_by(self.current_gantt_width() * f32::from(n)),
            Action::TimescaleToStart => self.scroll_gantt_to_project(false),
            Action::TimescaleToEnd => self.scroll_gantt_to_project(true),
            Action::ZoomBy(steps) => self.zoom_step(steps, self.gantt_center()),
            Action::ShowDialog(DialogKind::TaskInformation(uid)) => self.open_task_info(uid),
            Action::ShowDialog(DialogKind::Link {
                predecessor,
                successor,
            }) => self.open_link(predecessor, successor),
            Action::GestureRefused(error) => self.report_gesture(error),
            Action::ShowTaskInformation => {
                if let Some(uid) = self.selection {
                    self.open_task_info(uid);
                }
            }
            Action::ShowDialog(
                kind @ (DialogKind::WorkingTime
                | DialogKind::ProjectInformation
                | DialogKind::Baseline
                | DialogKind::Reschedule
                | DialogKind::AssignResources
                | DialogKind::CustomFields
                | DialogKind::Shift),
            ) => self.open_window(kind),
            Action::CloseDialog => self.close_dialog(),
            // Contrat J7 (lot L0) : actions câblées par leur lot.
            Action::SetGanttStyle(style) => self.state.gantt_style = style,
            Action::SetView(kind) => self.set_view(kind),
            Action::OpenRecent(index) => self.open_recent(index, ctx),
            // Bandeau « Planning retrouvé » masqué (H-UI-199), web et desktop.
            Action::Recover(crate::action::RecoverChoice::DismissRestored) => self.restored = None,
            #[cfg(not(target_arch = "wasm32"))]
            Action::Recover(choice) => self.recover(choice, ctx),
            #[cfg(target_arch = "wasm32")]
            other @ Action::Recover(_) => log::debug!("reprise après plantage absente du web : {other:?}"),
            Action::ToggleColumn(column) => self.toggle_column(column),
            Action::SetColumnWidth { column, width } => {
                self.state.set_column_width(column, width);
            }
            Action::ScrollTableX(delta) => {
                if delta.is_finite() {
                    self.view.table_scroll_x += delta;
                }
                self.clamp_table_scroll();
            }
            Action::Approve(id) => self.approve(crate::approvals::ApprovalKey::agent(id)),
            Action::Reject(id) => self.reject(crate::approvals::ApprovalKey::agent(id), None),
            Action::Proposal(op) => self.proposal_op(op),
            Action::Ui(op) => self.apply_ui(op, ctx),
            Action::TaskView(op) => self.apply_task_view(op),
            // J8, lot L10 : presse-papiers et import CSV.
            Action::Clipboard(op) => self.clipboard_op(op, ctx),
            Action::ImportCsv => self.start_csv_import(ctx),
            Action::Export(kind) => self.start_export(kind, ctx),
            // J8, lot L12 : volet latéral.
            Action::SidePanel(op) => self.side_op(op),
            // Interface adoucie, lot U3 : palette de commandes.
            Action::Palette(op) => self.palette_op(op),
            Action::AskAssistant(text) => self.ask_assistant(text),
            // ADR-0024, lot A1 : assistant IA.
            Action::Assistant(op) => {
                let env = self.assistant_env();
                self.assistant.apply(op, &mut self.state.assistant, &env);
            }
            Action::ShowDialog(DialogKind::PasteSpecial) => self.open_paste_special(),
            // J8, lot L15 : galerie, mise en page, menu Tâche, mise en forme.
            Action::ShowDialog(DialogKind::Gallery) => self.open_gallery(),
            Action::ShowDialog(DialogKind::PageSetup { print_mode }) => self.open_page_setup(print_mode),
            Action::TaskCmd(cmd) => self.task_cmd(cmd),
            Action::Format(op) => self.format_op(op),
            Action::SetChartOnly(on) => self.state.chart_only = on,
            Action::SetHighlightCritical(on) => {
                self.state.task_view.highlight_critical = on;
                self.view.task_view.highlight_critical = on;
            }
            Action::About => {
                let text = self.texts.get_args(
                    TextId::AboutText,
                    &[
                        ("product", Arg::Str(PRODUCT_NAME.into())),
                        ("version", Arg::Str(plannercraft_core::PRODUCT_VERSION.into())),
                    ],
                );
                self.push_message(Severity::Info, text);
            }
            // L'aperçu s'ouvre à l'arrivée des octets du fichier choisi.
            Action::ShowDialog(DialogKind::CsvImport) => self.start_csv_import(ctx),
            // --- Ruban et chronologie (ADR-0017, contrat L0) : bras posés
            // une fois ; les fonctions appelées sont servies par leur lot.
            Action::Ribbon(op) => {
                crate::ribbon::reduce(&mut self.state.ribbon, &mut self.view.ribbon_overlay, op);
            }
            Action::Timeline(TimelineOp::SetVisible(on)) => self.state.timeline_visible = on,
            Action::Timeline(op @ (TimelineOp::ToggleSelection | TimelineOp::Remove(_))) => {
                self.timeline_edit(op);
            }
            Action::Timeline(TimelineOp::Activate(uid)) => {
                self.selection = Some(uid);
                self.sync_selection();
                self.scroll_to_task();
                self.clamp_gantt_scroll();
            }
            Action::ScrollToTask => {
                self.scroll_to_task();
                self.clamp_gantt_scroll();
            }
            // Finitions P1 (H-UI-124).
            Action::ZoomToSelection => self.zoom_to_selection(),
            // Finitions P2 (H-UI-126 à H-UI-128).
            Action::TextStyle(op) => self.text_style_op(op),
            Action::Group(op) => self.group_op(op),
            Action::ShowTaskNotes if self.state.view != ViewKind::Gantt => {}
            Action::ShowTaskNotes => {
                if let Some(uid) = self.selection {
                    self.open_task_info_on(uid, crate::dialog::task_info::TaskInfoTab::Notes);
                }
            }
            // --- Passe UX (ADR-0018, contrat L0) : bras posés une fois ; les
            // fonctions appelées vivent dans le fichier du lot qui les sert
            // (`selection.rs` L1, `cells.rs` L2, `structure.rs` et
            // `column.rs` L3). Vues des ressources : sans effet (gestes de
            // bloc reportés), sauf Suppr, qui garde son chemin.
            Action::SelectOp(_)
            | Action::MoveRows { .. }
            | Action::FitColumn(_)
            | Action::Cells(
                crate::action::CellOp::FillDown
                | crate::action::CellOp::Clear
                | crate::action::CellOp::FillTyped(_),
            ) if self.state.view != ViewKind::Gantt => {}
            Action::SelectOp(op) => self.select_op(op),
            Action::Cells(op) => self.cells_op(op, ctx),
            Action::MoveRows { before } => self.move_rows(before),
            Action::FitColumn(column) => self.fit_column(column, ctx),
            // --- Seconde passe UX (ADR-0019, contrat L0) : bras posés une
            // fois ; les fonctions appelées vivent dans le fichier du lot
            // qui les sert (`panes.rs` L4, `column.rs` L6,
            // `view/details.rs` L5, `notify.rs` L1).
            Action::Pane(op) => self.pane_op(op),
            Action::Column(op) => self.column_op(op),
            Action::Details(op) => {
                self.details_op(op);
            }
            Action::Message(crate::action::MessageOp::Dismiss(id)) => self.messages.dismiss(id),
            Action::Message(crate::action::MessageOp::ToggleHistory) => self.messages.toggle_history(),
            Action::CloseDocumentGuarded => self.request(Pending::Close, ctx),
        }
    }

    /// Fenêtres de confirmation d'une suppression : tâche récapitulative
    /// (lot L14 de J4) ou ressource affectée (J7, lot L10).
    fn show_confirmations(&self, ctx: &egui::Context, palette: &Palette, actions: &mut Vec<Action>) {
        if let Some(pending) = &self.pending_delete {
            crate::structure::confirm_modal(ctx, &self.texts, pending, actions);
        }
        if let Some(pending) = &self.view.resources.pending_delete {
            view::resource_sheet::confirm_delete_modal(ctx, &self.texts, palette, pending, actions);
        }
    }

    /// Change la vue principale (J7, lots L10, L11) : une saisie ouverte
    /// dans l'autre grille est abandonnée ; la grille affichée prend le
    /// focus clavier.
    fn set_view(&mut self, kind: ViewKind) {
        if self.state.view == kind {
            return;
        }
        self.state.view = kind;
        self.view.edit.editing = None;
        self.view.resources.edit.editing = None;
        self.view.focus_table_pending = true;
    }

    /// Suppr dans la feuille des ressources (J7, lot L10) : `resource.delete`
    /// de la ressource courante ; affectée (`resource.has_assignments`), une
    /// fenêtre demande confirmation avant la suppression en cascade. Tout
    /// autre refus s'affiche dans le bandeau.
    fn delete_resource(&mut self) {
        if self.view.resources.pending_delete.is_some() {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        self.view.resources.ensure_rows(document, &self.texts);
        let Some(uid) = self.view.resources.cursor.row.item() else {
            return;
        };
        let rows = &self.view.resources.rows;
        let name = rows
            .index_of(uid)
            .map(|i| {
                rows.text(i, crate::resource_rows::ResourceColumn::Name)
                    .to_owned()
            })
            .unwrap_or_default();
        let cache_key = document.cache_key();
        match self.execute(
            &crate::resource_rows::delete_batch(uid, false),
            EditOrigin::Dialog,
        ) {
            Ok(_) => {}
            Err(error) => match error.kind {
                CommandErrorKind::ResourceHasAssignments { count } => {
                    self.view.resources.pending_delete = Some(view::resource_sheet::PendingResourceDelete {
                        uid,
                        name,
                        count,
                        cache_key,
                    });
                }
                _ => self.report_command_error(&error),
            },
        }
    }

    /// Réponse à la confirmation de suppression d'une ressource affectée :
    /// `true` supprime avec ses affectations (si la demande n'est pas
    /// devenue caduque), `false` abandonne.
    fn answer_resource_delete(&mut self, confirmed: bool) {
        let Some(pending) = self.view.resources.pending_delete.take() else {
            return;
        };
        let current = self.document.as_ref().map(OpenDocument::cache_key);
        if confirmed && current == Some(pending.cache_key) {
            // Un refus est affiché par `execute`.
            drop(self.execute(
                &crate::resource_rows::delete_batch(pending.uid, true),
                EditOrigin::User,
            ));
        }
    }

    /// Geste sur une proposition de l'assistant (carte de la conversation,
    /// lot A2) : Accepter, Refuser (motif facultatif de la carte), Voir.
    fn proposal_op(&mut self, op: crate::approvals::ProposalOp) {
        use crate::approvals::{ApprovalKey, ProposalOp};
        match op {
            ProposalOp::Accept(id) => self.approve(ApprovalKey::assistant(id)),
            ProposalOp::Reject(id) => {
                let reason = self.assistant.take_reject_reason(id);
                self.reject(ApprovalKey::assistant(id), reason);
            }
            ProposalOp::View(_) => self.side.apply(view::side_panel::SideOp::Open(
                view::side_panel::SideTab::AiOperations,
            )),
        }
    }

    /// Action d'interface d'un agent (`app.ui`), sans historique ni effet
    /// sur le projet ; les UID ont été vérifiés par le relais (une tâche
    /// disparue entre-temps est ignorée).
    fn apply_ui(&mut self, op: UiOp, ctx: &egui::Context) {
        match op {
            UiOp::Select(uid) => {
                let uid = uid.map(TaskUid::new);
                let known = uid.is_none_or(|uid| {
                    self.rows
                        .as_ref()
                        .is_some_and(|rows| rows.index_of(uid).is_some())
                });
                if known {
                    self.selection = uid;
                }
            }
            UiOp::ScrollTo(uid) => {
                // Ligne affichée ; une tâche masquée par la vue : rien.
                if let Some(d) = self
                    .visible_rows()
                    .and_then(|rows| rows.display_of(TaskUid::new(uid)))
                {
                    self.apply(Action::ScrollToRow(d.index()), ctx);
                }
            }
            UiOp::Zoom(zoom) => self.apply(Action::SetZoom(zoom), ctx),
            UiOp::View(kind) => self.set_view(kind),
            UiOp::Theme(theme) => self.apply(Action::SetTheme(theme), ctx),
            UiOp::Language(language) => self.apply(Action::SetLanguage(language), ctx),
        }
    }

    /// Lance le sélecteur de fichiers. Si une demande précédente n'a jamais
    /// répondu (sélecteur web sans événement d'annulation, lecture
    /// `FileReader` bloquée), elle est abandonnée : un nouveau canal la
    /// remplace, si bien que son résultat éventuel, envoyé dans l'ancien
    /// canal fermé, est ignoré. Jamais de blocage de « Ouvrir » (relecture
    /// A, I3).
    fn open_dialog(&mut self, ctx: &egui::Context) {
        if self.opening {
            log::warn!("ouverture précédente sans réponse : abandonnée, sélecteur relancé");
            let (tx, rx) = mpsc::channel();
            self.tx = tx;
            self.rx = rx;
        }
        self.opening = true;
        let now = (self.env.now)();
        let title = self.texts.get(TextId::DialogOpenTitle);
        let filter = self.texts.get(TextId::FileFilterMspdi);
        self.env
            .files
            .pick_and_open(ctx, now, title, filter, self.tx.clone());
    }

    /// Défilement horizontal du Gantt de `delta` points (borné).
    fn scroll_gantt_by(&mut self, delta: f32) {
        if delta.is_finite() {
            self.view.gantt_scroll_x += delta;
        }
        self.clamp_gantt_scroll();
    }

    /// Alt+Origine / Alt+Fin (H-UI-18) : début du projet au bord gauche du
    /// Gantt, ou fin du projet au bord droit. Abscisses de l'axe (temps →
    /// pixels) seulement.
    fn scroll_gantt_to_project(&mut self, end: bool) {
        let Some(document) = &self.document else {
            return;
        };
        let axis = TimeAxis::new(document.range, self.state.zoom, 0.0);
        let schedule = document.schedule();
        let target = if end {
            axis.x(schedule.project_finish) - self.current_gantt_width()
        } else {
            axis.x(schedule.project_start)
        };
        if target.is_finite() {
            self.view.gantt_scroll_x = target;
        }
        self.clamp_gantt_scroll();
    }

    /// Donne le focus clavier à un volet (clic dans la table ou le Gantt,
    /// fin de saisie) : la table reçoit le focus d'egui (sans le
    /// redemander s'il l'a déjà, pour garder son verrou de focus), le Gantt
    /// le lui retire. Saisie de cellule ouverte : la table ne le prend pas,
    /// la zone de saisie le garde (H-UI-85 : le double-clic qui ouvre la
    /// saisie pousse aussi `FocusPane(Table)`, appliqué après le dessin ; il
    /// retirait le focus à la zone, sans curseur de texte ni fin de saisie
    /// au clic suivant).
    fn focus_pane(&mut self, pane: Pane, ctx: &egui::Context) {
        self.view.focus = pane;
        let editing = self.view.edit.editing.is_some() || self.view.resources.edit.editing.is_some();
        ctx.memory_mut(|memory| match pane {
            Pane::Table => {
                if !editing && !memory.has_focus(table_id()) {
                    memory.request_focus(table_id());
                }
            }
            Pane::Gantt => memory.surrender_focus(table_id()),
        });
    }

    /// Affiche ou masque une colonne (J7, lot L6) ; masquer la colonne du
    /// curseur le ramène sur le Nom (toujours visible).
    fn toggle_column(&mut self, column: Column) {
        if self.state.toggle_column(column) {
            self.columns_changed();
        }
    }

    /// Après un changement des colonnes affichées : curseur ramené au Nom
    /// si sa colonne est masquée, défilement horizontal borné.
    pub(crate) fn columns_changed(&mut self) {
        if !self.state.shown(self.view.cursor.column) {
            self.view.cursor.column = Column::Name;
        }
        self.clamp_table_scroll();
    }

    /// Borne le défilement horizontal de la table à `[0, Σ largeurs −
    /// largeur visible]` (largeur de l'image précédente).
    fn clamp_table_scroll(&mut self) {
        // Colonne « Ajouter une nouvelle colonne » comptée (H-UI-114, L10).
        let total: f32 = self
            .state
            .visible_columns()
            .map(|c| f32::from(c.width))
            .sum::<f32>()
            + view::table::add_column_extra(None, self.state.chart_only);
        let max = total - self.view.last_table_width;
        let max = if max.is_finite() { max.max(0.0) } else { 0.0 };
        let x = self.view.table_scroll_x;
        self.view.table_scroll_x = if x.is_finite() { x.clamp(0.0, max) } else { 0.0 };
    }

    /// Le défilement horizontal suit la colonne du curseur, une colonne de
    /// marge de chaque côté quand elle tient (H-UI-17, J7 lot L6).
    pub(crate) fn follow_cursor_x(&mut self) {
        if self.view.last_table_width.is_nan() || self.view.last_table_width <= 0.0 {
            // Aucune image encore : largeur visible inconnue.
            return;
        }
        let widths: Vec<u16> = self.state.visible_columns().map(|c| c.width).collect();
        let Some(col) = self
            .state
            .visible_columns()
            .position(|c| c.column == self.view.cursor.column)
        else {
            return;
        };
        self.view.table_scroll_x =
            view::table::scroll_to_column(&widths, col, self.view.table_scroll_x, self.view.last_table_width);
    }

    /// Pas de zoom (Ctrl/Cmd + molette) ancré sous le pointeur, `anchor_x`
    /// étant relatif au bord gauche du Gantt.
    fn zoom_step(&mut self, steps: i8, anchor_x: f32) {
        let mut to = self.state.zoom;
        for _ in 0..steps.unsigned_abs() {
            to = if steps > 0 { to.finer() } else { to.coarser() };
        }
        if to != self.state.zoom {
            self.zoom_to(to, anchor_x);
        }
    }

    /// Ouvre « Informations sur la tâche » de `uid` (H-UI-19) : une fenêtre
    /// à la fois (la précédente est remplacée), brouillon relu sur la
    /// session ; une saisie de cellule ouverte est abandonnée. Sans document
    /// ou tâche absente : rien.
    fn open_task_info(&mut self, uid: TaskUid) {
        self.open_task_info_on(uid, crate::dialog::task_info::TaskInfoTab::General);
    }

    /// Comme [`App::open_task_info`], sur l'onglet `tab` (Tâche → Notes du
    /// ruban, ADR-0017, H-UI-84).
    fn open_task_info_on(&mut self, uid: TaskUid, tab: crate::dialog::task_info::TaskInfoTab) {
        let lang = self.text_language();
        let Some(document) = &self.document else {
            return;
        };
        let Some(state) =
            crate::dialog::task_info::open_on(tab, &document.session, uid, document.cache_key(), lang)
        else {
            return;
        };
        self.view.edit.editing = None;
        self.view.dialog = Some(DialogState {
            kind: DialogKind::TaskInformation(uid),
            body: DialogBody::TaskInfo(Box::new(state)),
        });
    }

    /// Ajout ou retrait de tâches de la chronologie (ADR-0017, H-UI-79) :
    /// lot de [`crate::timeline_ops::batch`] appliqué comme une édition
    /// (une entrée d'annulation), refus au bandeau.
    fn timeline_edit(&mut self, op: TimelineOp) {
        self.sync_selection();
        let Some(document) = &self.document else {
            return;
        };
        let default = self.state.default_template;
        match crate::timeline_ops::batch(&document.session, &self.selected, op, default) {
            // Un refus du registre est affiché par `execute`.
            Ok(Some(batch)) => drop(self.execute(&batch, EditOrigin::User)),
            Ok(None) => {}
            Err(error) => {
                let max = i64::try_from(crate::timeline_ops::TimelineOpError::max()).unwrap_or(i64::MAX);
                let text = self.texts.get_args(error.text_id(), &[("max", Arg::Count(max))]);
                self.push_message(Severity::Info, text);
            }
        }
    }

    /// Ouvre la boîte « Dépendance » du lien `predecessor` → `successor`
    /// (H-UI-21) : une fenêtre à la fois ; lien absent : rien.
    fn open_link(&mut self, predecessor: TaskUid, successor: TaskUid) {
        let lang = self.text_language();
        let Some(document) = &self.document else {
            return;
        };
        let key = document.cache_key();
        let Some(state) = LinkState::open(&document.session, &self.texts, predecessor, successor, key, lang)
        else {
            return;
        };
        self.view.edit.editing = None;
        self.view.drag = None;
        self.view.dialog = Some(DialogState {
            kind: DialogKind::Link {
                predecessor,
                successor,
            },
            body: DialogBody::Link(Box::new(state)),
        });
    }

    /// « OK » (ou « Supprimer » si `delete`) de la boîte d'un lien :
    /// document changé depuis l'ouverture ⇒ brouillon relu et
    /// avertissement, rien appliqué ; décalage illisible ou refus du
    /// registre ⇒ message dans la boîte ; rien de changé ⇒ boîte fermée
    /// sans lot ; sinon un seul lot.
    fn commit_link(&mut self, delete: bool) {
        let lang = self.text_language();
        let (
            Some(document),
            Some(DialogState {
                body: DialogBody::Link(state),
                ..
            }),
        ) = (&self.document, &mut self.view.dialog)
        else {
            self.close_dialog();
            return;
        };
        let key = document.cache_key();
        if key != state.key {
            match LinkState::open(
                &document.session,
                &self.texts,
                state.predecessor,
                state.successor,
                key,
                lang,
            ) {
                Some(mut fresh) => {
                    fresh.stale = true;
                    **state = fresh;
                }
                None => self.close_dialog(),
            }
            return;
        }
        let batch = if delete {
            Ok(Some(state.delete_batch()))
        } else {
            state.to_batch(&document.session, &self.texts, lang)
        };
        let batch = match batch {
            Ok(Some(batch)) => batch,
            Ok(None) => {
                self.close_dialog();
                return;
            }
            Err(message) => {
                state.error = Some(message);
                return;
            }
        };
        match self.execute(&batch, EditOrigin::Dialog) {
            Ok(_) => self.close_dialog(),
            Err(error) => {
                let message = task_info::command_message(
                    &self.texts,
                    &error,
                    self.document.as_ref().map(OpenDocument::project),
                    lang,
                );
                if let Some(DialogState {
                    body: DialogBody::Link(state),
                    ..
                }) = &mut self.view.dialog
                {
                    state.error = Some(message);
                }
            }
        }
    }

    /// Affiche le refus d'un geste du Gantt (bandeau), raison traduite.
    fn report_gesture(&mut self, error: GestureError) {
        if error == GestureError::AlreadyLinked {
            // R-260 c, H-UI-107 : rien n'est écrit ; simple information.
            let text = self.texts.get(TextId::LinkAlreadyLinked);
            self.push_message(Severity::Info, text);
            return;
        }
        let reason = match error {
            GestureError::NotDraggable(_, NotDraggable::Summary) => TextId::GanttReasonSummary,
            GestureError::NotDraggable(_, NotDraggable::Started) => TextId::GanttReasonStarted,
            GestureError::NotDraggable(_, NotDraggable::Milestone) => TextId::GanttReasonMilestone,
            GestureError::NotDraggable(_, NotDraggable::Excluded) | GestureError::UnknownTask(_) => {
                TextId::GanttReasonExcluded
            }
            GestureError::NotDraggable(_, NotDraggable::Blank) => TextId::GanttReasonBlank,
            GestureError::SameTask => TextId::GanttReasonSameTask,
            GestureError::Overflow => TextId::GanttReasonOverflow,
            GestureError::AlreadyLinked => TextId::LinkAlreadyLinked,
        };
        let detail = self.texts.get(reason);
        let text = self.texts.get_args(
            error_text_id(error.code()),
            &[("detail", Arg::Str(detail.into()))],
        );
        self.push_titled(Severity::Error, Some(TextId::CommandErrorTitle), text);
    }

    /// Ferme la fenêtre ouverte ; la table reprend le focus clavier.
    /// Commande du menu Tâche ou de la barre d'outils sur la sélection (J8
    /// lot L15) : un seul lot par `plannercraft_automation::helpers` (R-252,
    /// R-253, R-255), une entrée d'annulation ; rien à faire ou pas de
    /// sélection : message au bandeau.
    pub(crate) fn task_cmd(&mut self, cmd: crate::action::TaskCmd) {
        use crate::action::TaskCmd;
        use plannercraft_automation::helpers;
        let Some(document) = &self.document else {
            return;
        };
        let uids: Vec<TaskUid> = self.selected.uids().iter().copied().collect();
        if uids.is_empty() {
            let text = self.texts.get(TextId::TaskCmdNoSelection);
            self.push_message(Severity::Info, text);
            return;
        }
        let session = &document.session;
        let result = match cmd {
            // Finitions P1 (H-UI-121) : lot `task.shift`, compte rendu.
            TaskCmd::Shift(step) => {
                self.shift_step(step);
                return;
            }
            TaskCmd::MarkPercent(step) => helpers::mark_percent(session, &uids, step),
            TaskCmd::ToggleMode => helpers::toggle_mode(session, &uids),
            TaskCmd::ToggleActive => helpers::toggle_active(session, &uids),
        };
        match result {
            Ok(Some(batch)) => drop(self.execute(&batch, EditOrigin::User)),
            Ok(None) => {
                let text = self.texts.get(TextId::TaskCmdNothing);
                self.push_message(Severity::Info, text);
            }
            Err(error) => self.push_message(Severity::Error, error.to_string()),
        }
    }

    pub(crate) fn close_dialog(&mut self) {
        if self.view.dialog.take().is_some() && self.document.is_some() {
            self.view.focus_table_pending = true;
        }
    }

    /// Dessine la fenêtre ouverte, applique sa réponse, puis retire de
    /// l'image les touches qu'elle n'a pas prises.
    fn show_dialog(&mut self, ctx: &egui::Context) {
        /// Réponse de la fenêtre dessinée.
        enum Shown {
            /// « Informations sur la tâche ».
            TaskInfo(task_info::Outcome),
            /// Boîte d'un lien.
            Link(LinkOutcome),
        }
        // Fenêtres du lot L10 de J8 (collage spécial, import CSV) et du lot
        // L15 (galerie, mise en page).
        match self.view.dialog.as_ref().map(|d| &d.body) {
            Some(DialogBody::Gallery(_)) => {
                self.show_gallery(ctx);
                strip_keys(ctx);
                return;
            }
            Some(DialogBody::PageSetup(_)) => {
                self.show_page_setup(ctx);
                strip_keys(ctx);
                return;
            }
            Some(DialogBody::PasteSpecial(_)) => {
                self.show_paste_special(ctx);
                strip_keys(ctx);
                return;
            }
            Some(DialogBody::CsvImport(_)) => {
                self.show_csv_import(ctx);
                strip_keys(ctx);
                return;
            }
            _ => {}
        }
        if self.view.dialog.as_ref().is_some_and(|d| d.body.is_window()) {
            // Fenêtres du lot L12 (`dialog/mod.rs`).
            self.show_window(ctx);
            strip_keys(ctx);
            return;
        }
        let shown = match self.view.dialog.as_mut().map(|dialog| &mut dialog.body) {
            Some(DialogBody::TaskInfo(state)) => Shown::TaskInfo(task_info::show(ctx, &self.texts, state)),
            Some(DialogBody::Link(state)) => Shown::Link(link::show(ctx, &self.texts, state)),
            Some(
                DialogBody::Empty
                | DialogBody::WorkingTime(_)
                | DialogBody::ProjectInfo(_)
                | DialogBody::Baseline(_)
                | DialogBody::Reschedule(_)
                | DialogBody::Assign(_)
                | DialogBody::PasteSpecial(_)
                | DialogBody::CsvImport(_)
                | DialogBody::Gallery(_)
                | DialogBody::PageSetup(_)
                | DialogBody::CustomFields(_)
                | DialogBody::Shift(_),
            )
            | None => Shown::TaskInfo(task_info::Outcome::Cancel),
        };
        match shown {
            Shown::TaskInfo(task_info::Outcome::Open) | Shown::Link(LinkOutcome::Open) => {}
            Shown::TaskInfo(task_info::Outcome::Cancel) | Shown::Link(LinkOutcome::Cancel) => {
                self.close_dialog();
            }
            Shown::TaskInfo(task_info::Outcome::Ok) => self.commit_task_info(),
            Shown::Link(LinkOutcome::Ok) => self.commit_link(false),
            Shown::Link(LinkOutcome::Delete) => self.commit_link(true),
        }
        strip_keys(ctx);
    }

    /// « OK » d'« Informations sur la tâche » : document changé depuis
    /// l'ouverture ⇒ brouillon relu et avertissement, rien appliqué ;
    /// champs illisibles ⇒ refus affichés dans leur onglet ; sinon un seul
    /// lot, dont le refus par le registre s'affiche dans la fenêtre ; rien
    /// de changé ⇒ fenêtre fermée sans lot.
    fn commit_task_info(&mut self) {
        let lang = self.text_language();
        let (
            Some(document),
            Some(DialogState {
                body: DialogBody::TaskInfo(state),
                ..
            }),
        ) = (&self.document, &mut self.view.dialog)
        else {
            self.close_dialog();
            return;
        };
        let key = document.cache_key();
        if key != state.draft.key {
            match TaskInfoState::open(&document.session, state.draft.uid, key, lang) {
                Some(mut fresh) => {
                    fresh.stale = true;
                    fresh.tab = state.tab;
                    **state = fresh;
                }
                None => self.close_dialog(),
            }
            return;
        }
        let batch = match state.draft.to_batch(&document.session, lang) {
            Ok(Some(batch)) => batch,
            Ok(None) => {
                self.close_dialog();
                return;
            }
            Err(errors) => {
                show_errors(state, errors);
                return;
            }
        };
        match self.execute(&batch, EditOrigin::Dialog) {
            Ok(_) => self.close_dialog(),
            Err(error) => {
                let (tab, field) = task_info::locate(&batch, &error);
                let message = task_info::command_message(
                    &self.texts,
                    &error,
                    self.document.as_ref().map(OpenDocument::project),
                    lang,
                );
                if let Some(DialogState {
                    body: DialogBody::TaskInfo(state),
                    ..
                }) = &mut self.view.dialog
                {
                    show_errors(
                        state,
                        vec![FieldError {
                            tab,
                            field,
                            row: None,
                            message,
                        }],
                    );
                }
            }
        }
    }

    /// Clic relâché hors de la table et du Gantt (barre d'outils, menus,
    /// bandeau, réponse d'une confirmation) : egui retire le focus de la
    /// table à tout appui ailleurs et ne le donne à personne ; la table,
    /// volet actif, le reprend à l'image suivante (relecture finale J7, F5 :
    /// la navigation s'arrêtait en silence, cadre du curseur toujours
    /// dessiné). Un clic dans le Gantt (`Pane::Gantt`) ou Échap (D2)
    /// laissent la table sans focus ; un autre widget focalisé (zone de
    /// saisie) le garde ; rien pendant une fenêtre, un menu ou une
    /// confirmation de suppression.
    fn refocus_table_after_click(&mut self, ctx: &egui::Context) {
        let idle = self.document.is_some()
            && !self.palette.open
            && self.view.focus == Pane::Table
            && self.view.dialog.is_none()
            && !self.view_dialog_open()
            && !self.popup_open
            && self.pending_delete.is_none()
            && self.view.resources.pending_delete.is_none()
            && self.view.edit.editing.is_none()
            && self.view.resources.edit.editing.is_none();
        if idle
            && ctx.input(|input| input.pointer.any_released())
            && ctx.memory(|memory| memory.focused().is_none())
        {
            self.view.focus_table_pending = true;
        }
    }

    /// Contextes actifs des raccourcis : document ouvert ; table focalisée
    /// (focus d'egui et volet), sans saisie, menu, fenêtre ni confirmation
    /// de suppression ouverts (ADR-0015 D2).
    fn key_state(&self, ctx: &egui::Context) -> KeyState {
        let document = self.document.is_some();
        let table = document
            && self.view.focus == Pane::Table
            && ctx.memory(|memory| memory.has_focus(table_id()))
            && self.view.edit.editing.is_none()
            && !self.popup_open
            && !self.palette.open
            && self.view.dialog.is_none()
            && self.pending_delete.is_none()
            && self.view.resources.edit.editing.is_none()
            && self.view.resources.pending_delete.is_none();
        KeyState { document, table }
    }

    /// Raccourcis que l'image lirait maintenant, pour la garde du web
    /// (H-UI-55, J8 lot L13) : mêmes cas que [`Self::read_keys`] (fenêtre
    /// non modale : raccourcis gardés à côté d'elle ; fenêtre modale ou
    /// cellule en saisie : aucun ; saisie de texte focalisée : aucun).
    /// Appelée par l'enveloppe web après chaque image.
    #[must_use]
    pub fn key_scope(&self, ctx: &egui::Context) -> crate::keymap::KeyScope {
        use crate::keymap::KeyScope;
        // Saisie de texte hors fenêtre : Ctrl/Cmd+J seulement (lot A5).
        let window = self.view.dialog.is_some() || self.view_dialog_open();
        if ctx.text_edit_focused() {
            return if window {
                KeyScope::Nothing
            } else {
                KeyScope::Typing
            };
        }
        if let Some(dialog) = &self.view.dialog {
            return if matches!(dialog.body, DialogBody::Assign(_)) {
                KeyScope::BesideWindow(KeyState {
                    document: self.document.is_some(),
                    table: false,
                })
            } else {
                KeyScope::Nothing
            };
        }
        if self.view_dialog_open() {
            return KeyScope::Nothing;
        }
        if self.view.edit.editing.is_some() || self.view.resources.edit.editing.is_some() {
            return KeyScope::Typing;
        }
        KeyScope::All(self.key_state(ctx))
    }

    /// Raccourcis de l'image, avant le dessin (la touche consommée
    /// n'atteint aucun widget). Fenêtre ouverte : dessinée d'abord, ses
    /// touches non prises retirées ; saisie de cellule ouverte : aucun.
    fn read_keys(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        // Touches globales de sortie et de fermeture (seconde passe UX,
        // `quit.rs`) : lues avant tout le reste.
        if self.global_keys(ctx, actions) {
            return;
        }
        // Palette ouverte (lot U3) : ses touches seulement.
        if self.palette.open {
            self.palette_keys(ctx);
            return;
        }
        self.assistant_key(ctx, actions);
        if self.view.dialog.is_some() {
            // « Affecter des ressources » (non modale) : les raccourcis à
            // Ctrl/Cmd qui ne touchent pas la structure (enregistrer,
            // annuler, ouvrir, quitter…) restent actifs hors de ses champs
            // (relecture finale J7, F9).
            if self
                .view
                .dialog
                .as_ref()
                .is_some_and(|d| matches!(d.body, DialogBody::Assign(_)))
                && !ctx.text_edit_focused()
            {
                let keys = KeyState {
                    document: self.document.is_some(),
                    table: false,
                };
                crate::keymap::dispatch_beside_window(ctx, keys, actions);
            }
            self.show_dialog(ctx);
        } else if self.view_dialog_open() {
            // Fenêtres « Trier » et « Filtre » (J8, lot L9) : modales.
            self.show_view_dialog(ctx);
            strip_keys(ctx);
        } else if self.view.edit.editing.is_none() && self.view.resources.edit.editing.is_none() {
            // Saisie de cellule ouverte : aucun raccourci, même si sa zone
            // n'a pas encore le focus (relecture finale J7, F2 : Retour
            // arrière supprimait la tâche).
            let keys = self.key_state(ctx);
            // Couper, copier, coller (J8, lot L10) : pris seulement quand la
            // table a le focus ; sinon laissés aux widgets.
            self.take_clipboard_events(ctx, keys.table);
            crate::keymap::dispatch(ctx, keys, actions);
        }
    }

    /// Style du planning à l'écran et palette du thème `theme` (J8, lot
    /// L11) : style effectif du document (`UiState::effective_style`,
    /// H-UI-43), modèle par défaut sans document ; mis en cache par
    /// ouverture, génération et modèle par défaut.
    pub(crate) fn screen_style(&mut self, theme: egui::Theme) -> (crate::theme::ScreenStyle, Palette) {
        let doc_key = self.document.as_ref().map(OpenDocument::cache_key);
        let default = self.state.default_template;
        let (document, state) = (self.document.as_ref(), &self.state);
        let (mut screen, palette) = self
            .style_cache
            .screen(doc_key, default, theme, || match document {
                Some(document) => state.effective_style(document.project()),
                None => plannercraft_render::style::template(default),
            });
        if screen.style.grid.rows == plannercraft_render::style::RowBanding::Phase {
            let rows = self.rows.as_ref();
            screen.phases = self
                .style_cache
                .phases(doc_key, || rows.map(view::gantt::phases).unwrap_or_default());
        }
        // Couleurs de phase (H-UI-65, J8 lot L15) : une fois par génération.
        if !screen.style.phases.is_empty() {
            let style = std::sync::Arc::clone(&screen.style);
            let document = self.document.as_ref();
            screen.phase_colors = self.style_cache.phase_colors(doc_key, || {
                document.map_or_else(crate::theme::PhaseColors::new, |d| {
                    crate::theme::phase_colors(d.project(), &style)
                })
            });
        }
        (screen, palette)
    }

    /// Hauteur d'une ligne de la table et du Gantt (densité du style de la
    /// dernière image, J8 lot L11) ; 22 px avant la première image.
    #[must_use]
    pub fn row_h(&self) -> f32 {
        f32::from(self.style_cache.style().density.row_px())
    }

    /// Une image complète dans `ui` (appelée par [`crate::ui`]) : relève des
    /// ouvertures, réglages, panneaux, vue centrale, puis réducteur.
    #[allow(
        clippy::too_many_lines,
        reason = "suite d'étapes de l'image, une ligne chacune"
    )]
    pub(crate) fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Horloge des messages (H-UI-104) : notifications échues retirées.
        self.clock = ctx.input(|input| input.time);
        self.messages
            .expire(self.clock, view::chrome::hovered_toast(&ctx));
        // Appels relayés d'un agent d'abord (ADR-0015 D9) : la session vue
        // par l'image est celle qui a répondu.
        #[cfg(not(target_arch = "wasm32"))]
        self.drain_relay(&ctx);
        self.poll_assistant(&ctx);
        // Volet ouvert d'office par un agent (relais) : mémorisé, pastille
        // effacée (lot A5).
        self.sync_side_prefs();
        self.tick_approvals();
        self.poll_io();
        self.poll_save(&ctx);
        self.poll_export();
        self.take_dropped_files(&ctx);
        self.guard_close(&ctx);
        self.apply_theme(&ctx);
        self.apply_motion(&ctx);
        self.apply_language();
        self.ensure_row_view();
        // Style effectif du document (J8, lot L11) : palette, densité,
        // police du planning ; relu seulement quand le document change.
        let (screen, palette) = self.screen_style(ctx.theme());
        // Inter pour toute l'interface (lot U1, H-UI-169) : installée à
        // chaque image (sans effet une fois faite), avant le premier texte.
        crate::theme::install_plan_fonts(&ctx);
        // Densité : table et Gantt seulement ; feuilles des ressources à
        // 22 px comme J7.
        let metrics = Metrics {
            row_h: if self.state.view == ViewKind::Gantt {
                screen.row_h()
            } else {
                view::ROW_H
            },
            ..Metrics::default()
        };
        self.sync_selection();
        self.drop_stale_delete();
        let current = self.document.as_ref().map(OpenDocument::cache_key);
        self.view.resources.drop_stale_delete(current);
        if self.view.focus_table_pending && self.document.is_some() {
            self.view.focus_table_pending = false;
            self.focus_pane(Pane::Table, &ctx);
        }
        // Palette refermée (lot U3, H-UI-179) : focus rendu au volet actif
        // de la vue ; une fenêtre ou un champ ouvert par la commande le
        // reprend en se dessinant.
        // Attendu tant que la palette est encore la couche modale du dessus
        // (fermée pendant son dessin : egui refuse le focus dessous une
        // image de plus) ; seulement si personne d'autre n'a pris le focus.
        if self.palette.refocus
            && ctx.memory(egui::Memory::top_modal_layer) != Some(view::palette::layer_id())
        {
            self.palette.refocus = false;
            let query = view::palette::query_id();
            let free = ctx.memory(|memory| memory.focused().is_none_or(|id| id == query));
            ctx.memory_mut(|memory| memory.surrender_focus(query));
            if free && self.document.is_some() {
                self.focus_pane(self.view.focus, &ctx);
            }
        }
        let mut actions = std::mem::take(&mut self.actions);
        // Raccourcis avant le dessin : la touche consommée n'atteint aucun
        // widget. Fenêtre ouverte (J7, lot L9) : la fenêtre est dessinée
        // d'abord et les touches qu'elle n'a pas prises sont retirées de
        // l'image (rien n'atteint la table ni le Gantt).
        // Pinceau armé (H-UI-128) : Échap l'annule avant tout autre usage.
        self.painter_escape(&ctx);
        self.read_keys(&ctx, &mut actions);
        let mut task_view = self.task_view_frame();
        task_view.style = screen;
        // Explication de la tâche active (H-UI-58) : relue seulement quand
        // la tâche, la génération ou la langue changent.
        let lang = self.text_language();
        self.side
            .refresh_explain(self.document.as_ref(), self.selection, lang);
        let mut stats = FrameStats::default();
        #[cfg(not(target_arch = "wasm32"))]
        let recovery_offer = self.recovery_offer();
        let restored_name = self.restored_banner().map(str::to_owned);
        // Nom du document du bandeau de confirmation, s'il y en a un.
        let confirm_name = self.confirm.as_ref().map(|_| {
            self.document
                .as_ref()
                .map(|document| document.origin.name.clone())
                .unwrap_or_default()
        });
        // Interface adoucie (lot U2, H-UI-172) : fond de l'application sous
        // les cartes, peint avant tout panneau.
        view::cards::set_inset(&ctx, ui.max_rect().left() - ctx.content_rect().left());
        view::cards::paint_app_background(&ctx);
        {
            // Champs disjoints : le contexte d'image emprunte l'état en
            // lecture, la vue transitoire est empruntée à part.
            let mut fc = FrameCtx {
                state: &self.state,
                doc: self.document.as_ref(),
                rows: self.rows.as_ref(),
                row_view: Some(&self.row_view),
                texts: &self.texts,
                palette: &palette,
                metrics: &metrics,
                diagnostics: &self.diagnostics,
                selection: self.selection,
                selected: &self.selected,
                actions: &mut actions,
                stats: &mut stats,
                window_open: self.view.dialog.is_some(),
                task_view,
            };
            view::chrome::publish_panels(&ctx, self.side.open, self.side.shows_ai());
            let button = view::ribbon::AssistantButton {
                open: self.side.open,
                badge: self.assistant_badge,
                busy: self.assistant.conversation.busy(),
            };
            view::ribbon::show(ui, &mut fc, &self.state.ribbon, self.view.ribbon_overlay, button);
            view::search::search_bar(ui, &mut fc);
            view::chrome::status_bar(ui, &mut fc, &self.messages);
            view::chrome::confirm_banner(ui, &mut fc, confirm_name.as_deref());
            {
                let document = self
                    .document
                    .as_ref()
                    .map_or("", |document| document.origin.name.as_str());
                let banner = crate::approvals::banner_for(&self.approvals, document);
                // Volet « Opérations IA » visible : il porte la décision, le
                // bandeau J7 se retire (H-UI-41) ; de même pour une
                // proposition de l'assistant quand sa conversation est
                // affichée (la carte porte la décision, lot A2).
                let card_visible = banner.is_some_and(|b| b.assistant)
                    && self.side.open
                    && self.side.tab == view::side_panel::SideTab::Assistant;
                if !self.side.shows_ai() && !card_visible {
                    view::approval::approval_banner(ui, &mut fc, banner);
                }
                #[cfg(not(target_arch = "wasm32"))]
                view::recovery::recovery_banner(ui, &mut fc, recovery_offer);
                view::recovery::restored_banner(ui, &mut fc, restored_name.as_deref());
                let ai = crate::approvals::panel_input(&self.approvals, banner);
                view::side_panel::panel(ui, &mut fc, &self.side, ai, &mut self.assistant, button);
            }
            let outer = view::cards::outer_margin(&ctx);
            // Lot R1c (H-UI-190) : le rail ou le volet est toujours à droite
            // de la vue (la gouttière est la leur) : marge droite nulle.
            let diagnostics_shown = self.state.diagnostics_open
                && view::diagnostics::panel(ui, &mut fc, view::cards::px(outer), 0);
            let view_state = &mut self.view;
            // Interface adoucie (lot U2, H-UI-172) : vue centrale posée sur
            // le fond de l'application, en cartes ; marge extérieure à
            // gauche (à droite : rail ou volet, lot R1c), gouttière en bas
            // (diagnostics ou barre d'état).
            let frame = egui::Frame::NONE.inner_margin(egui::Margin {
                left: view::cards::px(outer),
                right: 0,
                top: 0,
                bottom: if diagnostics_shown {
                    0
                } else {
                    view::cards::px(view::cards::GUTTER)
                },
            });
            if fc.doc.is_some() {
                egui::CentralPanel::default()
                    .frame(frame)
                    .show(ui, |ui| show_view(ui, &mut fc, view_state));
            } else {
                egui::CentralPanel::default().frame(frame).show(ui, |ui| {
                    let card = ui.available_rect_before_wrap();
                    view::cards::paint_fill(ui.painter(), card);
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(view::cards::inner(card).shrink2(egui::vec2(8.0, 2.0)))
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    view::chrome::welcome(&mut child, &mut fc);
                    view::cards::finish(ui.painter(), card);
                    ui.advance_cursor_after_rect(card);
                });
            }
            // Palette de commandes (lot U3), au-dessus de tout.
            view::palette::show(&ctx, &mut fc, &mut self.palette);
        }
        self.show_confirmations(&ctx, &palette, &mut actions);
        // Volet de mise en forme des barres (J8 lot L15, H-UI-59), non modal.
        self.show_format_panel(&ctx, &mut actions);
        // Échap restée libre après le dessin : réduit le volet (lot A5).
        self.escape_closes_assistant(&ctx, &mut actions);
        self.last_focus = ctx.memory(egui::Memory::focused);
        self.last_frame = stats;
        self.popup_open = ctx.any_popup_open();
        // Les actions sont appliquées après le dessin : une image de plus
        // montre leur effet (langue, zoom, document…). Les lots d'édition
        // d'abord (partition stable) : une saisie validée par la perte du
        // focus arrive après le bouton cliqué dans la même image (barre
        // d'outils, menus et bandeau sont dessinés avant la table) ; elle
        // doit modifier le document **avant** qu'une ouverture, un « Nouveau »
        // ou une réponse au bandeau le remplace, pour que la confirmation la
        // voie (relecture finale J4, F2).
        let applied = !actions.is_empty();
        let (edits, others): (Vec<Action>, Vec<Action>) = actions
            .into_iter()
            .partition(|action| matches!(action, Action::Exec(_) | Action::ExecOn { .. }));
        for action in edits.into_iter().chain(others) {
            self.apply(action, &ctx);
        }
        self.sync_side_prefs();
        // Après le réducteur : un clic dans le Gantt a déjà changé de volet.
        self.refocus_table_after_click(&ctx);
        self.ensure_row_view();
        self.sync_selection();
        // Pinceau (H-UI-128) : la sélection suivante reçoit la mise en forme.
        self.painter_tick(&ctx);
        // Ajustement du zoom après une ouverture, dès que la largeur du
        // Gantt est connue : là aussi, une image de plus au bon zoom.
        let fitted = self.fit_if_needed();
        if applied || fitted {
            ctx.request_repaint();
        }
        // Titre de la fenêtre (« * » si modifié), envoyé seulement au
        // changement.
        let title = self.window_title();
        if title != self.sent_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.sent_title = title;
        }
    }
}

/// Vue centrale du document selon `UiState::view` (J7, lot L10).
fn show_view(ui: &mut egui::Ui, fc: &mut FrameCtx<'_>, view_state: &mut ViewTransient) {
    match fc.state.view {
        ViewKind::ResourceSheet | ViewKind::ResourceUsage => {
            // Interface adoucie (lot U2, H-UI-172) : la feuille et
            // l'utilisation des ressources dans une carte (le Gantt découpe
            // lui-même ses volets en cartes).
            let card = ui.available_rect_before_wrap();
            view::cards::paint_fill(ui.painter(), card);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(view::cards::inner(card))
                    .layout(egui::Layout::top_down(egui::Align::Min))
                    .id_salt("plannercraft.resource_view"),
            );
            child.set_clip_rect(view::cards::inner(card).intersect(ui.clip_rect()));
            if fc.state.view == ViewKind::ResourceSheet {
                view::resource_sheet::show(&mut child, fc, &mut view_state.resources);
            } else {
                view::usage::show(&mut child, fc, view_state);
            }
            view::cards::finish(ui.painter(), card);
            ui.advance_cursor_after_rect(card);
        }
        ViewKind::Gantt => view::main_view::show(ui, fc, view_state),
    }
    // Seconde passe UX (lot L6) : sélecteur de champ ou « Paramètres de
    // champ » ouverts, au-dessus de la vue.
    crate::dialog::field_picker::show(ui.ctx(), fc, &mut view_state.picker);
}

/// Pose les refus d'une tentative ; l'onglet affiché passe au premier
/// onglet en refus s'il n'en a lui-même aucun.
fn show_errors(state: &mut TaskInfoState, errors: Vec<FieldError>) {
    if !errors.iter().any(|e| e.tab == state.tab)
        && let Some(first) = errors.iter().map(|e| e.tab).min()
    {
        state.tab = first;
    }
    state.errors = errors;
}

/// Journal du document : reprise après plantage (desktop, J7 lot L15) et
/// sauvegarde automatique du navigateur (web, lot W1, ADR-0031).
impl App {
    /// Branche le journal de reprise : il suit aussitôt le document courant,
    /// puis chaque ouverture, changement, enregistrement et fermeture.
    pub fn set_recovery(&mut self, log: Box<dyn plannercraft_automation::recovery::RecoveryLog>) {
        self.recovery = Some(log);
        self.record_recovery();
    }

    /// Journalise l'état du document ; une écriture impossible (disque
    /// plein, stockage du navigateur refusé) coupe le journal et s'affiche
    /// une fois, sans jamais bloquer l'édition.
    fn record_recovery(&mut self) {
        let Some(log) = &mut self.recovery else {
            return;
        };
        let now = (self.env.now)();
        log.record(
            self.document
                .as_ref()
                .map(|document| (document.opening, &document.session)),
            now,
        );
        if let Some(detail) = log.take_issue() {
            let text = self
                .texts
                .get_args(self.recovery_cut_text, &[("detail", Arg::Str(detail.into()))]);
            self.push_message(Severity::Warning, text);
        }
    }

    /// Démarre la sauvegarde automatique du navigateur (version web, lot
    /// W1, H-UI-199 à H-UI-201) d'après `plan` : rouvre le dernier planning
    /// gardé (bandeau « Planning retrouvé »), ou démarre vide en disant
    /// pourquoi il n'y a pas de sauvegarde automatique ; puis, si cet onglet
    /// tient la sauvegarde, branche le journal sur `writer`. À appeler une
    /// fois, après l'observateur et avant toute ouverture.
    pub fn start_browser_autosave(
        &mut self,
        plan: crate::autosave::StartPlan,
        writer: Option<Box<dyn crate::autosave::SlotWriter>>,
    ) {
        use crate::autosave::StartPlan;
        let owner = plan.writes();
        match plan {
            StartPlan::Restore(content) => self.restore_browser_slot(&content),
            StartPlan::Empty => {}
            StartPlan::Unreadable(detail) => {
                let text = self
                    .texts
                    .get_args(TextId::AutosaveUnreadable, &[("detail", Arg::Str(detail.into()))]);
                self.push_message(Severity::Warning, text);
            }
            StartPlan::OtherTab => {
                let text = self.texts.get(TextId::AutosaveOtherTab);
                self.push_message(Severity::Warning, text);
            }
            StartPlan::Unavailable(detail) => {
                let text = self.texts.get_args(
                    TextId::AutosaveUnavailable,
                    &[("detail", Arg::Str(detail.into()))],
                );
                self.push_message(Severity::Warning, text);
            }
        }
        if owner && let Some(writer) = writer {
            self.recovery_cut_text = TextId::AutosaveStopped;
            self.set_recovery(Box::new(crate::autosave::BrowserAutosave::new(writer)));
        }
    }

    /// Rouvre le planning gardé par le navigateur (H-UI-199) : rejeu du
    /// journal, document installé tel qu'il était (nom, état « modifié »,
    /// modifications depuis la dernière base annulables), sans fichier lié ;
    /// bandeau « Planning retrouvé ». Un rejeu arrêté sur une entrée refusée
    /// rouvre ce qui précède et le dit ; une base illisible : message, page
    /// vide.
    fn restore_browser_slot(&mut self, content: &crate::autosave::SlotContent) {
        let lang = self.text_language();
        match plannercraft_automation::recovery::replay_bytes(&content.base, &content.ops, lang) {
            Ok((session, report)) => {
                let name = session.origin().name.clone();
                let range = default_range(session.project(), session.schedule());
                let loaded = LoadedDocument {
                    session,
                    import_diagnostics: Vec::new(),
                    range: range.span,
                    range_truncated: range.truncated,
                    web_file: None,
                };
                let origin = DocOrigin {
                    name: name.clone(),
                    path: None,
                    demo: false,
                };
                self.install(loaded, origin);
                self.restored = self.document.as_ref().map(|document| document.opening);
                if report.stopped_at.is_some() || content.dropped > 0 {
                    let count = i64::try_from(report.applied).unwrap_or(i64::MAX);
                    let text = self.texts.get_args(
                        TextId::RecoveryPartial,
                        &[
                            ("name", Arg::Str(name.into())),
                            ("count", Arg::Count(count)),
                            (
                                "detail",
                                Arg::Str(
                                    report
                                        .error
                                        .unwrap_or_else(|| self.texts.get(TextId::AutosaveGap))
                                        .into(),
                                ),
                            ),
                        ],
                    );
                    self.push_message(Severity::Warning, text);
                }
            }
            Err(error) => {
                let text = self.texts.get_args(
                    TextId::AutosaveUnreadable,
                    &[("detail", Arg::Str(error.to_string().into()))],
                );
                self.push_message(Severity::Warning, text);
            }
        }
    }

    /// Planning rouvert depuis la sauvegarde du navigateur, encore ouvert
    /// et bandeau non masqué (H-UI-199) : son nom.
    #[must_use]
    pub fn restored_banner(&self) -> Option<&str> {
        let document = self.document.as_ref()?;
        (self.restored == Some(document.opening)).then_some(document.origin.name.as_str())
    }
}

/// Reprise après plantage (desktop, J7 lot L15).
#[cfg(not(target_arch = "wasm32"))]
impl App {
    /// Propose les journaux orphelins qui portent des modifications, ou qui
    /// viennent d'une autre version (bandeau de reprise, un à la fois).
    pub fn offer_recovery(&mut self, orphans: Vec<plannercraft_automation::recovery::Orphan>) {
        self.orphans = orphans
            .into_iter()
            .filter(plannercraft_automation::recovery::Orphan::worth_offering)
            .collect();
    }

    /// Orphelin affiché par le bandeau de reprise (le premier).
    fn recovery_offer(&self) -> Option<crate::view::recovery::RecoveryOffer<'_>> {
        self.orphans
            .first()
            .map(|o| crate::view::recovery::RecoveryOffer {
                id: &o.id,
                name: &o.name,
                entries: o.entries,
                checkpointed: o.checkpointed,
                file_changed: o.file_changed,
                kept: o.kept,
            })
    }

    /// Journaux orphelins encore proposés.
    #[must_use]
    pub fn recovery_offers(&self) -> &[plannercraft_automation::recovery::Orphan] {
        &self.orphans
    }

    /// Réponse au bandeau de reprise après plantage (J7, lot L15) :
    /// Restaurer (après confirmation si le document courant est modifié),
    /// Ignorer (journal supprimé) ou Plus tard (bandeau retiré, journaux
    /// gardés pour le prochain démarrage).
    fn recover(&mut self, choice: crate::action::RecoverChoice, ctx: &egui::Context) {
        match choice {
            crate::action::RecoverChoice::Restore(id) => self.request(Pending::Restore(id), ctx),
            crate::action::RecoverChoice::Discard(id) => {
                let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.orphans)
                    .into_iter()
                    .partition(|o| o.id == id);
                self.orphans = kept;
                for orphan in gone {
                    if let Err(error) = orphan.discard() {
                        self.report_recovery_discard(&error);
                    }
                }
            }
            crate::action::RecoverChoice::Later => self.orphans.clear(),
            crate::action::RecoverChoice::DismissRestored => self.restored = None,
        }
    }

    /// Fermeture propre de l'application : journal supprimé.
    fn recovery_closed(&mut self) {
        if let Some(log) = &mut self.recovery {
            log.closed();
        }
    }

    /// Suppression d'un journal impossible : message.
    fn report_recovery_discard(&mut self, error: &std::io::Error) {
        let text = self.texts.get_args(
            TextId::RecoveryDiscardFailed,
            &[("detail", Arg::Str(error.to_string().into()))],
        );
        self.push_message(Severity::Warning, text);
    }

    /// Restaure l'orphelin `id` : rejeu du journal, document installé et
    /// marqué modifié (son contenu n'est pas sur le disque), journal
    /// orphelin supprimé (le document a déjà le sien). Un rejeu arrêté sur
    /// une entrée refusée restaure ce qui précède et le dit ; un journal
    /// illisible reste proposé (Ignorer le supprime). Fichier changé sur le
    /// disque depuis la base : document installé **sans chemin**, nommé
    /// « plan (récupéré).xml », pour qu'Enregistrer demande un nom au lieu
    /// d'écraser la version plus récente (relecture finale J7, F3).
    fn restore_orphan(&mut self, id: &str) {
        let Some(orphan) = self.orphans.iter().find(|o| o.id == id).cloned() else {
            return;
        };
        let lang = self.text_language();
        match plannercraft_automation::recovery::replay(&orphan, lang) {
            Ok((mut session, report)) => {
                self.orphans.retain(|o| o.id != id);
                session.mark_unsaved();
                let range = default_range(session.project(), session.schedule());
                let loaded = LoadedDocument {
                    session,
                    import_diagnostics: Vec::new(),
                    range: range.span,
                    range_truncated: range.truncated,
                    web_file: None,
                };
                let origin = if orphan.file_changed {
                    DocOrigin {
                        name: recovered_name(&self.texts, &orphan.name),
                        path: None,
                        demo: false,
                    }
                } else {
                    DocOrigin {
                        name: orphan.name.clone(),
                        path: orphan.path.clone(),
                        demo: false,
                    }
                };
                self.install(loaded, origin);
                if let Err(error) = orphan.discard() {
                    self.report_recovery_discard(&error);
                }
                if report.stopped_at.is_some() {
                    let count = i64::try_from(report.applied).unwrap_or(i64::MAX);
                    let text = self.texts.get_args(
                        TextId::RecoveryPartial,
                        &[
                            ("name", Arg::Str(orphan.name.as_str().into())),
                            ("count", Arg::Count(count)),
                            ("detail", Arg::Str(report.error.unwrap_or_default().into())),
                        ],
                    );
                    self.push_message(Severity::Warning, text);
                }
            }
            Err(error) => {
                let text = self.texts.get_args(
                    TextId::RecoveryFailed,
                    &[
                        ("name", Arg::Str(orphan.name.as_str().into())),
                        ("detail", Arg::Str(error.to_string().into())),
                    ],
                );
                self.push_titled(Severity::Error, None, text);
            }
        }
    }
}

/// Nom d'un document récupéré dont le fichier a changé : « plan
/// (récupéré).xml » (extension gardée).
#[cfg(not(target_arch = "wasm32"))]
fn recovered_name(texts: &Texts, name: &str) -> String {
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    let stem = texts.get_args(TextId::RecoveredFileName, &[("stem", Arg::Str(stem.into()))]);
    format!("{stem}{ext}")
}

/// Destination d'« Enregistrer » (`save_as` faux) ou d'« Enregistrer sous »
/// (vrai) : chemin connu (desktop) ⇒ écriture directe ; fichier du
/// navigateur lié (web, H-UI-204) ⇒ réécriture ; sinon boîte (desktop),
/// sélecteur ou téléchargement (web, voir [`crate::io::web_save_route`]).
#[must_use]
pub fn save_target(document: &OpenDocument, save_as: bool, texts: &Texts) -> SaveTarget {
    let suggested_name = suggested_file_name(&document.origin.name);
    let title = texts.get(TextId::DialogSaveTitle);
    let filter = texts.get(TextId::FileFilterMspdi);
    match (&document.origin.path, document.web_file, save_as) {
        (Some(path), _, false) => SaveTarget::Path(path.clone()),
        (None, Some(file), false) => SaveTarget::File {
            file,
            suggested_name,
            title,
            filter,
        },
        _ => SaveTarget::Dialog {
            suggested_name,
            title,
            filter,
        },
    }
}

/// Nom affiché d'un fichier déposé : dernier composant de son chemin (le
/// web ne donne que le nom).
fn dropped_name(file: &egui::DroppedFileHandle) -> String {
    file.path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// SVG du planning de `session` (J8, lot L13) : style affiché à l'écran
/// (`UiState::effective_style`), plage du projet, largeur automatique,
/// textes formatés par `SessionTexts` (règle 3), lignes de la vue affichée
/// (`rows`, relecture finale de J8, F1) ; frise de la chronologie en tête
/// si `page.timeline` (H-UI-82, comme l'aperçu et le PDF).
fn export_svg(
    session: &Session,
    state: &UiState,
    lang: plannercraft_automation::text::Language,
    rows: Option<std::collections::BTreeSet<TaskUid>>,
) -> String {
    let style = state.effective_style(session.project());
    let req = plannercraft_render::GanttRequest {
        range: plannercraft_render::RangeSpec::Auto,
        zoom: plannercraft_render::Zoom::Auto { width_px: 1_200 },
        filter: plannercraft_render::RowFilter {
            uids: rows,
            ..plannercraft_render::RowFilter::default()
        },
        language: match lang {
            plannercraft_automation::text::Language::Fr => plannercraft_render::Lang::Fr,
            plannercraft_automation::text::Language::En => plannercraft_render::Lang::En,
        },
        theme: plannercraft_render::Theme::Light,
        limits: plannercraft_render::RenderLimits::default(),
    };
    let texts = plannercraft_automation::render_text::SessionTexts { session, lang };
    // H-UI-82 : frise en tête quand la mise en page la demande, comme le PDF.
    let timeline = plannercraft_automation::timeline::print_input(session, &style, lang);
    let (svg, _report) = plannercraft_render::svg_styled_timeline(
        session.project(),
        session.schedule(),
        &req,
        &style,
        &texts,
        timeline.as_ref(),
    );
    svg
}

/// Titre de la fenêtre pour `document` (voir [`App::window_title`]). Le web
/// le pose aussi sur l'onglet (`document.title`) depuis son observateur :
/// eframe web ignore `ViewportCommand::Title`.
#[must_use]
pub fn window_title_for(document: Option<&OpenDocument>) -> String {
    match document {
        None => PRODUCT_NAME.to_owned(),
        Some(document) => format!("{} \u{2014} {PRODUCT_NAME}", document_label(document)),
    }
}

/// Nom du document précédé de « * » s'il est modifié (titre de la fenêtre
/// et barre d'état).
#[must_use]
pub fn document_label(document: &OpenDocument) -> String {
    let star = if document.is_dirty() { "*" } else { "" };
    format!("{star}{}", document.origin.name)
}

/// Début d'un projet neuf : le lundi strictement après le jour de `now`,
/// à 08:00 (hypothèse : début de journée du calendrier Standard ; MS
/// Project propose la date du jour). Repli sur `now` hors du domaine des
/// dates.
#[must_use]
pub fn next_monday_morning(now: Moment) -> Moment {
    let day = now.day();
    let ahead = match day.weekday() {
        Weekday::Sunday => 1,
        Weekday::Monday => 7,
        Weekday::Tuesday => 6,
        Weekday::Wednesday => 5,
        Weekday::Thursday => 4,
        Weekday::Friday => 3,
        Weekday::Saturday => 2,
    };
    // 480 minutes : 08:00.
    day.checked_add_days(ahead)
        .ok()
        .zip(TimeOfDay::new(480).ok())
        .and_then(|(monday, morning)| Moment::at(monday, morning).ok())
        .unwrap_or(now)
}

/// Retire de l'image les touches qu'une fenêtre ouverte n'a pas prises
/// (rien n'atteint la table ni le Gantt).
fn strip_keys(ctx: &egui::Context) {
    ctx.input_mut(|input| {
        input.events.retain(|event| {
            !matches!(
                event,
                egui::Event::Key { .. }
                    | egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Copy
                    | egui::Event::Cut
                    | egui::Event::Ime(_)
            )
        });
    });
}

/// Nombre de jours d'une plage (0 si vide ou inversée).
fn span_days(span: ViewSpan) -> i64 {
    i64::from(span.end.index())
        .saturating_sub(i64::from(span.start.index()))
        .max(0)
}

/// Union de deux plages affichées ; `new` seule si l'union dépasse
/// [`MAX_RANGE_DAYS`] (largeur du contenu représentable en `f32`).
fn union_span(old: ViewSpan, new: ViewSpan) -> ViewSpan {
    let start: Day = old.start.min(new.start);
    let end: Day = old.end.max(new.end);
    let union = ViewSpan { start, end };
    if span_days(union) <= i64::from(MAX_RANGE_DAYS) {
        union
    } else {
        new
    }
}

/// Clé d'archive de l'assistant pour la clé brute `raw` : chemin canonique
/// sur le desktop (même document par deux chemins), clé brute sinon.
fn canonical_doc_key(raw: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = raw.strip_prefix("path:")
        && let Ok(canonical) = std::fs::canonicalize(path)
    {
        return format!("path:{}", canonical.display());
    }
    raw.to_owned()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_core::Moment;

    use plannercraft_core::Severity;

    use super::{App, Env};
    use crate::action::Action;
    use crate::i18n::{Language, TextId};
    use crate::state::{LanguagePref, ThemePref, UiState};

    /// Application de test : date fixe, français, thème clair.
    fn test_app() -> App {
        let now: Moment = "2027-01-15T08:00".parse().unwrap();
        let state = UiState {
            language: LanguagePref::Fr,
            theme: ThemePref::Light,
            ..UiState::default()
        };
        App::new(state, Env::fixed(now))
    }

    /// Une image sans GPU (textures ignorées : aucun moteur de rendu).
    fn frame(ctx: &egui::Context, app: &mut App) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| crate::ui(ui, app));
        output.textures_delta.clear();
    }

    /// H-UI-82 : Exporter → SVG imprime la frise quand la case
    /// Chronologie de Mise en page est enregistrée (comme le PDF).
    #[test]
    fn h_ui_82_svg_export_prints_timeline() {
        let mut app = test_app();
        app.open_demo();
        let lang = plannercraft_automation::text::Language::Fr;
        let doc = app.document.as_mut().unwrap();
        let env = plannercraft_automation::CallEnv::new((app.env.now)(), lang);
        let mut page = app.state.effective_style(doc.session.project()).page;
        let mut export = |timeline: bool| {
            page.timeline = timeline;
            let params = serde_json::json!({ "patch": { "page": serde_json::to_value(&page).unwrap() } });
            plannercraft_automation::call_with(&mut doc.session, &env, "style.apply", params).unwrap();
            super::export_svg(&doc.session, &app.state, lang, None)
        };
        let without = export(false);
        let with = export(true);
        assert!(with.len() > without.len(), "frise absente du SVG");
        assert!(with.contains("Début") && !without.contains("Début"));
    }

    #[test]
    fn open_demo_loads_document() {
        let mut app = test_app();
        app.open_demo();
        let document = app.document.as_ref().expect("exemple chargé");
        assert!(document.origin.demo);
        assert_eq!(document.opening, 1);
        assert_eq!(document.generation(), 0);
        assert!(app.messages().is_empty(), "{:?}", app.messages());
    }

    #[test]
    fn invalid_bytes_become_message() {
        let mut app = test_app();
        app.open_bytes(b"pas du MSPDI", super::DocOrigin::default());
        assert!(app.document.is_none());
        assert_eq!(app.messages().len(), 1);
    }

    /// Ouverture synchrone réussie après un échec : nouvelle génération,
    /// erreur purgée, sélection remise à zéro, diagnostics recalculés.
    #[test]
    fn open_bytes_success_installs_new_generation() {
        let mut app = test_app();
        app.open_demo();
        app.selection = app.rows().and_then(|rows| rows.row(2)).map(|row| row.uid);
        app.open_bytes(b"pas du MSPDI", super::DocOrigin::default());
        assert_eq!(app.messages().len(), 1);
        assert_eq!(app.document.as_ref().unwrap().opening, 1);
        app.open_bytes(crate::DEMO_MSPDI, super::DocOrigin::default());
        assert_eq!(app.document.as_ref().unwrap().opening, 2);
        assert!(app.messages().is_empty(), "{:?}", app.messages());
        // J7 L1 : curseur (et sélection) sur la première ligne.
        assert_eq!(
            app.selection,
            app.rows().and_then(|rows| rows.row(0)).map(|row| row.uid)
        );
        assert!(app.view().needs_fit);
        assert_eq!(app.diagnostics_view(), &crate::view::DiagnosticsView::default());
    }

    /// Relecture A, B1 : une plage tronquée à 20 ans est signalée par un
    /// avertissement traduit, une seule fois.
    #[test]
    fn truncated_range_warns_once() {
        let mut app = test_app();
        for _ in 0..2 {
            let mut loaded =
                crate::document::load(crate::DEMO_MSPDI, "2027-01-15T08:00".parse().unwrap()).unwrap();
            loaded.range_truncated = true;
            app.install(loaded, super::DocOrigin::default());
        }
        let expected = app.texts().get(TextId::RangeTruncated);
        assert_eq!(app.messages().len(), 1, "{:?}", app.messages());
        assert_eq!(app.messages()[0].severity, Severity::Warning);
        assert_eq!(app.messages()[0].text, expected);
        // Un document non tronqué retire l'avertissement.
        app.open_demo();
        assert!(app.messages().is_empty(), "{:?}", app.messages());
    }

    /// Le bandeau garde au plus `MAX_MESSAGES` messages, sans doublon.
    #[test]
    fn messages_are_capped_and_deduplicated() {
        let mut app = test_app();
        for i in 0..10 {
            app.push_message(Severity::Error, format!("m{i}"));
            app.push_message(Severity::Error, format!("m{i}"));
        }
        let texts: Vec<&str> = app.messages().iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["m7", "m8", "m9"]);
        assert_eq!(texts.len(), super::MAX_MESSAGES);
        // Le même échec répété n'empile pas de lignes.
        let mut app = test_app();
        for _ in 0..40 {
            app.open_bytes(b"<x", super::DocOrigin::default());
        }
        assert_eq!(app.messages().len(), 1);
    }

    /// Diagnostics triés et comptés une fois : cache recalculé au changement
    /// de langue, vidé à la fermeture ; « → » rendu en « -> ».
    #[test]
    fn diagnostics_view_is_cached_per_document() {
        let xml = std::str::from_utf8(crate::DEMO_MSPDI).unwrap().replacen(
            "<PredecessorUID>",
            "<PredecessorUID>777",
            1,
        );
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.open_bytes(xml.as_bytes(), super::DocOrigin::default());
        assert!(app.document.is_some(), "{:?}", app.messages());
        let view = app.diagnostics_view().clone();
        assert!(!view.entries.is_empty(), "un lien pendant doit être signalé");
        assert!(view.entries.iter().all(|(_, text)| !text.contains('\u{2192}')));
        let ranks: Vec<u8> = view
            .entries
            .iter()
            .map(|(severity, _)| match severity {
                Severity::Error => 0,
                Severity::Warning => 1,
                Severity::Info => 2,
            })
            .collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "{ranks:?}");
        let errors = view.entries.iter().filter(|(s, _)| *s == Severity::Error).count();
        assert_eq!(view.errors, errors);
        app.dispatch(Action::SetLanguage(LanguagePref::En));
        frame(&ctx, &mut app);
        assert_eq!(app.diagnostics_view().entries.len(), view.entries.len());
        app.dispatch(Action::CloseDocument);
        frame(&ctx, &mut app);
        assert_eq!(app.diagnostics_view().entries.len(), 0);
    }

    /// Projet neuf : lundi strictement suivant, 08:00.
    #[test]
    fn next_monday_morning_is_strictly_after() {
        let cases = [
            ("2027-01-15T08:00", "2027-01-18T08:00"), // vendredi
            ("2027-01-17T23:59", "2027-01-18T08:00"), // dimanche
            ("2027-01-18T07:00", "2027-01-25T08:00"), // lundi
            ("2027-01-16T12:00", "2027-01-18T08:00"), // samedi
        ];
        for (now, expected) in cases {
            let now: Moment = now.parse().unwrap();
            assert_eq!(
                super::next_monday_morning(now),
                expected.parse().unwrap(),
                "{now}"
            );
        }
    }

    /// Relecture finale J4, F2 : un lot visant un autre document (numéro
    /// d'ouverture différent) est abandonné ; le bon numéro l'applique.
    #[test]
    fn exec_on_other_opening_is_dropped() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.open_demo();
        let opening = app.document.as_ref().unwrap().opening;
        let batch = plannercraft_automation::Batch::from_json_str(
            r#"{"format_version":1,"ops":[{"op":"task.update","uid":3,"name":"X"}]}"#,
        )
        .unwrap();
        app.dispatch(Action::ExecOn {
            opening: opening.saturating_add(1),
            batch: batch.clone(),
        });
        frame(&ctx, &mut app);
        assert_eq!(app.document.as_ref().unwrap().generation(), 0);
        assert!(app.messages().is_empty(), "{:?}", app.messages());
        app.dispatch(Action::ExecOn { opening, batch });
        frame(&ctx, &mut app);
        assert_eq!(app.document.as_ref().unwrap().generation(), 1);
    }

    #[test]
    fn welcome_frame_and_reducer() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        frame(&ctx, &mut app);
        assert_eq!(ctx.theme(), egui::Theme::Light);
        app.dispatch(Action::SetLanguage(LanguagePref::En));
        app.dispatch(Action::SetTheme(ThemePref::Dark));
        frame(&ctx, &mut app);
        frame(&ctx, &mut app);
        assert_eq!(app.texts().language(), Language::En);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        app.dispatch(Action::OpenDemo);
        frame(&ctx, &mut app);
        assert!(app.document.is_some());
        frame(&ctx, &mut app);
    }
}

/// Version web, lot W1 (ADR-0031) : sauvegarde automatique du navigateur
/// vue par l'application (H-UI-199 à H-UI-201) et enregistrement direct
/// dans le fichier (H-UI-202 à H-UI-204), testés en natif avec un faux
/// stockage et un faux service de fichiers.
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod browser_tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::mpsc::Sender;

    use plannercraft_automation::Batch;
    use plannercraft_core::{Moment, TaskUid};

    use super::{App, EditOrigin, Env};
    use crate::action::{Action, RecoverChoice};
    use crate::autosave::{LockState, MemorySlot, StartPlan, plan_start};
    use crate::i18n::{Language, TextId, Texts};
    use crate::io::{
        FileFailure, FileService, OpenEvent, SaveEvent, SaveKind, SaveRequest, SaveTarget, WebFile,
        WebSaveRoute, classify_file_error, file_failure_detail, web_save_route,
    };
    use crate::state::{LanguagePref, UiState};

    fn now() -> Moment {
        "2027-01-15T08:00".parse().unwrap()
    }

    fn app_with(files: Box<dyn FileService>) -> App {
        let state = UiState {
            language: LanguagePref::Fr,
            ..UiState::default()
        };
        let mut env = Env::fixed(now());
        env.files = files;
        App::new(state, env)
    }

    fn app() -> App {
        app_with(Box::new(crate::io::NoFileService))
    }

    fn frame(ctx: &egui::Context, app: &mut App) {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| crate::ui(ui, app));
        output.textures_delta.clear();
    }

    fn rename(app: &mut App, name: &str) {
        let batch = Batch::from_json_str(&format!(
            r#"{{"format_version":1,"ops":[{{"op":"task.update","uid":3,"name":"{name}"}}]}}"#
        ))
        .unwrap();
        app.execute(&batch, EditOrigin::User).unwrap();
    }

    fn task3(app: &App) -> String {
        let document = app.document.as_ref().unwrap();
        let task = document
            .project()
            .tasks
            .iter()
            .find(|t| t.uid() == TaskUid::new(3))
            .unwrap();
        task.name.clone()
    }

    /// Onglet qui tient la sauvegarde, sur `slot`, relu comme au démarrage.
    fn reopen(slot: &MemorySlot) -> App {
        let mut app = app();
        let plan = plan_start(LockState::Acquired, Some(Ok(slot.entries())));
        app.start_browser_autosave(plan, Some(Box::new(slot.clone())));
        app
    }

    fn history_has(app: &App, id: TextId) -> usize {
        let texts = Texts::new(Language::Fr);
        let probe = texts.get(id);
        // Début du message, avant sa première variable.
        let head: String = probe.chars().take(30).collect();
        app.messages
            .history()
            .iter()
            .filter(|m| m.text.starts_with(&head))
            .count()
    }

    /// H-UI-199 : le dernier planning de ce navigateur est rouvert
    /// automatiquement, modifications non enregistrées comprises : même nom,
    /// état « modifié », modifications depuis la dernière base annulables
    /// jusqu'à l'état enregistré ; bandeau « Planning retrouvé » masquable,
    /// retiré par un autre document ; « Nouveau planning vide » remplace la
    /// sauvegarde.
    #[test]
    fn h_ui_199_restores_last_plan_with_unsaved_edits() {
        let slot = MemorySlot::new();
        let mut first = reopen(&slot);
        assert!(first.document.is_none(), "rien à rouvrir la première fois");
        assert_eq!(first.restored_banner(), None);
        first.open_demo();
        let name = first.document.as_ref().unwrap().origin.name.clone();
        let original = task3(&first);
        rename(&mut first, "Terrassement W1");
        drop(first);
        let mut app = reopen(&slot);
        let document = app.document.as_ref().unwrap();
        assert_eq!(document.origin.name, name);
        assert!(document.origin.path.is_none() && document.web_file.is_none());
        assert!(document.is_dirty());
        assert_eq!(task3(&app), "Terrassement W1");
        assert_eq!(app.restored_banner(), Some(name.as_str()));
        // Annuler revient à l'état enregistré (la base du journal).
        app.undo().unwrap();
        assert_eq!(task3(&app), original);
        assert!(!app.document.as_ref().unwrap().is_dirty());
        // Masquer le bandeau.
        let ctx = egui::Context::default();
        app.dispatch(Action::Recover(RecoverChoice::DismissRestored));
        frame(&ctx, &mut app);
        assert_eq!(app.restored_banner(), None);
        // « Nouveau planning vide » : la sauvegarde suit le nouveau projet.
        // L'annulation a ramené la sauvegarde à l'état enregistré.
        let mut app = reopen(&slot);
        assert!(app.restored_banner().is_some());
        assert!(!app.document.as_ref().unwrap().is_dirty());
        assert_eq!(task3(&app), original);
        app.dispatch(Action::NewProject);
        frame(&ctx, &mut app);
        assert_eq!(app.restored_banner(), None, "autre document : bandeau retiré");
        let new_name = app.document.as_ref().unwrap().origin.name.clone();
        drop(app);
        let app = reopen(&slot);
        assert_eq!(app.document.as_ref().unwrap().origin.name, new_name);
        assert!(!app.document.as_ref().unwrap().is_dirty());
    }

    /// H-UI-199 : le style du document (gardé dans le projet) revient avec
    /// lui ; la vue (état d'interface d'eframe) n'en dépend pas.
    #[test]
    fn h_ui_199_document_style_is_kept() {
        let slot = MemorySlot::new();
        let mut first = reopen(&slot);
        first.open_demo();
        let batch =
            Batch::from_json_str(r#"{"format_version":1,"ops":[{"op":"style.apply","template":"sobre"}]}"#)
                .unwrap();
        first.execute(&batch, EditOrigin::User).unwrap();
        let style = |app: &App| {
            plannercraft_automation::command::style::doc_style(app.document.as_ref().unwrap().project())
        };
        let before = style(&first);
        assert!(before.is_some());
        drop(first);
        let app = reopen(&slot);
        assert_eq!(style(&app), before);
    }

    /// H-UI-199 : un planning enregistré (téléchargé) est rouvert aussi, non
    /// modifié ; fermer le document vide la sauvegarde : page vide au
    /// prochain démarrage, sans planning fantôme.
    #[test]
    fn h_ui_199_saved_plan_reopens_and_close_clears() {
        let slot = MemorySlot::new();
        let mut first = reopen(&slot);
        first.open_demo();
        drop(first);
        let mut app = reopen(&slot);
        assert!(app.document.is_some());
        assert!(!app.document.as_ref().unwrap().is_dirty());
        let ctx = egui::Context::default();
        app.dispatch(Action::CloseDocument);
        frame(&ctx, &mut app);
        assert!(app.document.is_none());
        assert_eq!(
            plan_start(LockState::Acquired, Some(Ok(slot.entries()))),
            StartPlan::Empty
        );
        let app = reopen(&slot);
        assert!(app.document.is_none());
    }

    /// H-UI-200 : un second onglet (verrou tenu ailleurs) démarre vide, le
    /// dit, et n'écrit jamais dans la sauvegarde de l'autre ; sans verrou
    /// ni stockage, pas de sauvegarde, dit aussi.
    #[test]
    fn h_ui_200_second_tab_never_touches_the_slot() {
        let slot = MemorySlot::new();
        let mut owner = reopen(&slot);
        owner.open_demo();
        rename(&mut owner, "Propriétaire");
        let kept = slot.entries();
        let mut guest = app();
        let plan = plan_start(LockState::HeldElsewhere, None);
        guest.start_browser_autosave(plan, Some(Box::new(slot.clone())));
        assert!(guest.document.is_none());
        assert_eq!(history_has(&guest, TextId::AutosaveOtherTab), 1);
        guest.open_demo();
        rename(&mut guest, "Invité");
        assert_eq!(slot.entries(), kept, "l'invité n'écrit rien");
        let mut none = app();
        none.start_browser_autosave(plan_start(LockState::Unsupported, None), None);
        assert_eq!(history_has(&none, TextId::AutosaveUnavailable), 1);
    }

    /// H-UI-201 : stockage en panne (quota plein…) : l'édition continue,
    /// un message traduit le dit une fois, jamais de panique.
    #[test]
    fn h_ui_201_storage_failure_message_once() {
        let slot = MemorySlot::new();
        let mut app = reopen(&slot);
        app.open_demo();
        slot.fail("QuotaExceededError : quota plein");
        for name in ["A", "B", "C"] {
            rename(&mut app, name);
        }
        assert_eq!(task3(&app), "C");
        assert_eq!(history_has(&app, TextId::AutosaveStopped), 1);
        assert!(
            app.messages
                .history()
                .iter()
                .any(|m| m.text.contains("QuotaExceededError"))
        );
    }

    /// H-UI-202 : chemin d'un enregistrement web. Sans sélecteurs du
    /// navigateur (Safari, Firefox) ou pour un export : téléchargement ;
    /// sinon fichier lié réécrit, ou sélecteur.
    #[test]
    fn h_ui_202_web_save_route() {
        let dialog = SaveTarget::Dialog {
            suggested_name: "a.xml".to_owned(),
            title: String::new(),
            filter: String::new(),
        };
        let file = SaveTarget::File {
            file: WebFile(7),
            suggested_name: "a.xml".to_owned(),
            title: String::new(),
            filter: String::new(),
        };
        for kind in SaveKind::ALL {
            assert_eq!(web_save_route(false, kind, &file), WebSaveRoute::Download);
            assert_eq!(web_save_route(false, kind, &dialog), WebSaveRoute::Download);
        }
        for kind in [SaveKind::Pdf, SaveKind::Svg, SaveKind::Csv] {
            assert_eq!(
                web_save_route(true, kind, &file),
                WebSaveRoute::Download,
                "{kind:?}"
            );
        }
        assert_eq!(
            web_save_route(true, SaveKind::Mspdi, &file),
            WebSaveRoute::Rewrite(WebFile(7))
        );
        assert_eq!(web_save_route(true, SaveKind::Mspdi, &dialog), WebSaveRoute::Pick);
    }

    /// H-UI-203 : refus de permission, fichier disparu : message traduit
    /// qui nomme le fichier ; annulation du sélecteur : aucun message.
    #[test]
    fn h_ui_203_file_errors_are_translated() {
        assert_eq!(classify_file_error("AbortError"), FileFailure::Cancelled);
        assert_eq!(classify_file_error("NotAllowedError"), FileFailure::Denied);
        assert_eq!(classify_file_error("SecurityError"), FileFailure::Denied);
        assert_eq!(classify_file_error("NotFoundError"), FileFailure::Missing);
        assert_eq!(classify_file_error("TypeError"), FileFailure::Other);
        assert_eq!(
            file_failure_detail(FileFailure::Cancelled, "a.xml", "x", Language::Fr),
            None
        );
        let denied = file_failure_detail(FileFailure::Denied, "a.xml", "x", Language::Fr).unwrap();
        assert!(denied.contains("a.xml") && denied.contains("permis"), "{denied}");
        let missing = file_failure_detail(FileFailure::Missing, "a.xml", "x", Language::En).unwrap();
        assert!(
            missing.contains("a.xml") && missing.contains("cannot be found"),
            "{missing}"
        );
        assert_eq!(
            file_failure_detail(FileFailure::Other, "a.xml", "disque plein", Language::Fr).as_deref(),
            Some("disque plein")
        );
    }

    /// Service simulé : répond d'avance par `reply` et garde les demandes.
    struct FakeFiles {
        reply: Rc<RefCell<Option<SaveEvent>>>,
        requests: Rc<RefCell<Vec<SaveRequest>>>,
    }

    impl FileService for FakeFiles {
        fn pick_and_open(
            &mut self,
            ctx: &egui::Context,
            now: Moment,
            _: String,
            _: String,
            tx: Sender<OpenEvent>,
        ) {
            // Ouverture par le sélecteur du navigateur : fichier lié.
            let mut loaded = crate::document::load(crate::DEMO_MSPDI, now).unwrap();
            loaded.web_file = Some(WebFile(1));
            let origin = super::DocOrigin {
                name: "chantier.xml".to_owned(),
                path: None,
                demo: false,
            };
            tx.send(OpenEvent::Opened {
                origin,
                result: Ok(loaded),
            })
            .unwrap();
            ctx.request_repaint();
        }

        fn save(&mut self, ctx: &egui::Context, request: SaveRequest, tx: Sender<SaveEvent>) {
            self.requests.borrow_mut().push(request);
            if let Some(event) = self.reply.borrow_mut().take() {
                tx.send(event).unwrap();
            }
            ctx.request_repaint();
        }
    }

    /// H-UI-204 : un fichier ouvert par le sélecteur du navigateur est lié
    /// au document ; « Enregistrer » vise ce fichier, « Enregistrer sous »
    /// le sélecteur ; un échec laisse le document modifié et lié ; un
    /// enregistrement réussi dans un autre fichier lie ce dernier, un
    /// téléchargement délie.
    #[test]
    fn h_ui_204_file_binding_follows_document() {
        let reply = Rc::new(RefCell::new(None));
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut app = app_with(Box::new(FakeFiles {
            reply: Rc::clone(&reply),
            requests: Rc::clone(&requests),
        }));
        let ctx = egui::Context::default();
        app.dispatch(Action::OpenDialog);
        frame(&ctx, &mut app);
        frame(&ctx, &mut app);
        assert_eq!(app.document.as_ref().unwrap().web_file, Some(WebFile(1)));
        rename(&mut app, "X");
        // Enregistrer : fichier lié ; refus ⇒ toujours modifié et lié.
        *reply.borrow_mut() = Some(SaveEvent::Failed("refus".to_owned()));
        app.dispatch(Action::Save);
        frame(&ctx, &mut app);
        frame(&ctx, &mut app);
        assert!(matches!(
            requests.borrow()[0].target,
            SaveTarget::File { file: WebFile(1), .. }
        ));
        let document = app.document.as_ref().unwrap();
        assert!(document.is_dirty());
        assert_eq!(document.web_file, Some(WebFile(1)));
        // Enregistrer sous : sélecteur ; succès ⇒ nouveau fichier lié.
        *reply.borrow_mut() = Some(SaveEvent::SavedFile {
            name: "copie.xml".to_owned(),
            file: WebFile(2),
        });
        app.dispatch(Action::SaveAs);
        frame(&ctx, &mut app);
        frame(&ctx, &mut app);
        assert!(matches!(requests.borrow()[1].target, SaveTarget::Dialog { .. }));
        let document = app.document.as_ref().unwrap();
        assert!(!document.is_dirty());
        assert_eq!(document.web_file, Some(WebFile(2)));
        assert_eq!(document.origin.name, "copie.xml");
        // Téléchargement (navigateur sans sélecteurs) : plus de fichier lié.
        rename(&mut app, "Y");
        *reply.borrow_mut() = Some(SaveEvent::Saved {
            name: "copie.xml".to_owned(),
            path: None,
        });
        app.dispatch(Action::Save);
        frame(&ctx, &mut app);
        frame(&ctx, &mut app);
        assert!(matches!(
            requests.borrow()[2].target,
            SaveTarget::File { file: WebFile(2), .. }
        ));
        assert_eq!(app.document.as_ref().unwrap().web_file, None);
        assert!(!app.document.as_ref().unwrap().is_dirty());
    }
}
