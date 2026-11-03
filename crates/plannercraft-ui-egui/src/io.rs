//! Ouverture et enregistrement de fichiers sans runtime asynchrone.
//!
//! Le réducteur d'`App` appelle [`FileService::pick_and_open`] ; le résultat
//! revient par un canal `std::sync::mpsc`, relevé (`try_recv`) au début de
//! l'image suivante. Natif : `rfd::AsyncFileDialog` créé sur le fil
//! d'interface et attendu par `pollster` dans un fil dédié, qui lit le
//! fichier (`document::read_path`, borné) et appelle [`document::load`] ; jamais de
//! `rfd::FileDialog` synchrone (sa boucle modale imbriquée fait avorter winit
//! sur macOS, ADR-0011). `FileHandle::read` de rfd n'est jamais utilisé (il
//! fait `unwrap` en interne). Web : `rfd::AsyncFileDialog` (téléversement par
//! un `<input type=file>` généré par rfd) lancé par
//! `wasm_bindgen_futures::spawn_local`, puis lecture par `Blob::arrayBuffer`
//! (promesse rejetée → erreur traduite). Tout chemin qui se termine envoie un
//! [`OpenEvent`]. Le fil dédié n'existe qu'en natif (`std::thread` panique en
//! wasm32).
//!
//! Enregistrement ([`FileService::save`], lot L13) : les octets MSPDI sont
//! produits par `Session::save_bytes` sur le fil d'interface, puis écrits
//! hors de lui. Desktop : chemin connu → `plannercraft_automation::files::write_atomic`
//! dans le fil `plannercraft-save` ; sinon `rfd::AsyncFileDialog::save_file` créé
//! sur le fil d'interface et attendu dans ce même fil, qui écrit ensuite.
//! Web : téléchargement direct par `web_sys` (`Blob`, URL d'objet, ancre
//! `download`), sans la fenêtre de rfd. Tout chemin
//! qui se termine envoie un [`SaveEvent`] ; l'application ne marque le
//! document enregistré qu'à la réception de [`SaveEvent::Saved`].

use std::sync::mpsc::Sender;

use plannercraft_core::Moment;

use crate::app::DocOrigin;
use crate::document::{self, LoadedDocument, OpenError};

/// Résultat d'une demande d'ouverture.
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "un seul événement par ouverture, consommé aussitôt : une boîte n'apporterait rien"
)]
pub enum OpenEvent {
    /// L'utilisateur a refermé le sélecteur sans choisir.
    Cancelled,
    /// Un fichier a été lu : document chargé ou erreur.
    Opened {
        /// Provenance du fichier.
        origin: DocOrigin,
        /// Document chargé, ou raison de l'échec.
        result: Result<LoadedDocument, document::OpenError>,
    },
}

/// Destination d'un enregistrement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveTarget {
    /// Chemin connu (desktop, « Enregistrer » d'un document déjà lié à un
    /// fichier) : écriture directe, sans boîte de dialogue.
    Path(String),
    /// Boîte « Enregistrer sous » (desktop) ou téléchargement (web).
    Dialog {
        /// Nom de fichier proposé (déjà assaini, avec `.xml`).
        suggested_name: String,
        /// Titre de la boîte, traduit.
        title: String,
        /// Libellé du filtre MSPDI, traduit.
        filter: String,
    },
}

/// Demande d'enregistrement : octets à écrire et destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRequest {
    /// Octets du fichier (MSPDI de `Session::save_bytes`, ou export),
    /// écrits tels quels.
    pub bytes: Vec<u8>,
    /// Genre du fichier : extension du filtre de la boîte (desktop), type
    /// MIME du téléchargement (web) ; J8, lot L13.
    pub kind: SaveKind,
    /// Destination.
    pub target: SaveTarget,
    /// Langue des messages d'erreur d'écriture.
    pub language: crate::i18n::Language,
}

/// Résultat d'une demande d'enregistrement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveEvent {
    /// L'utilisateur a refermé la boîte sans choisir : rien n'est écrit.
    Cancelled,
    /// Octets écrits (desktop) ou remis au navigateur (web).
    Saved {
        /// Nom affiché du fichier écrit.
        name: String,
        /// Chemin UTF-8 écrit (desktop) ; `None` sur le web ou pour un
        /// chemin non UTF-8 (le prochain « Enregistrer » redemandera).
        path: Option<String>,
    },
    /// Écriture impossible : détail déjà traduit (cause système comprise).
    Failed(String),
}

/// Accès aux fichiers, injecté par l'application (`Env::files`).
pub trait FileService {
    /// Ouvre le sélecteur de fichiers et envoie le résultat par `tx`, puis
    /// demande une nouvelle image à `ctx`. `title` et `filter` sont déjà
    /// traduits ; `now` est la date courante injectée pour le lecteur MSPDI.
    fn pick_and_open(
        &mut self,
        ctx: &egui::Context,
        now: Moment,
        title: String,
        filter: String,
        tx: Sender<OpenEvent>,
    );

    /// Écrit `request.bytes` vers `request.target` et envoie le résultat par
    /// `tx`, puis demande une nouvelle image à `ctx`. Par défaut, rien
    /// n'est écrit et la réponse est [`SaveEvent::Cancelled`], comme un
    /// utilisateur qui referme la boîte : le document reste modifié.
    fn save(&mut self, ctx: &egui::Context, request: SaveRequest, tx: Sender<SaveEvent>) {
        drop(request);
        deliver(&tx, ctx, SaveEvent::Cancelled);
    }

    /// Ouvre le sélecteur de fichiers et envoie les octets du fichier
    /// choisi (au plus [`MAX_PICKED_BYTES`]) par `tx`, sans les
    /// interpréter (J8, lot L10 : Fichier → Importer CSV…). Par défaut :
    /// [`PickedFile::Cancelled`], comme un utilisateur qui referme la boîte.
    fn pick_bytes(&mut self, ctx: &egui::Context, request: PickRequest, tx: Sender<PickedFile>) {
        drop(request);
        deliver(&tx, ctx, PickedFile::Cancelled);
    }
}

/// Octets au plus d'un fichier lu par [`FileService::pick_bytes`] : la
/// borne de l'import CSV (64 Mio, `CsvLimits::max_bytes`).
pub const MAX_PICKED_BYTES: u64 = 64 << 20;

/// Demande de [`FileService::pick_bytes`] : textes déjà traduits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickRequest {
    /// Titre du sélecteur.
    pub title: String,
    /// Libellé du filtre.
    pub filter: String,
    /// Extensions du filtre, sans point (`csv`, `txt`).
    pub extensions: Vec<String>,
}

/// Résultat de [`FileService::pick_bytes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickedFile {
    /// L'utilisateur a refermé le sélecteur sans choisir.
    Cancelled,
    /// Un fichier a été choisi : octets lus, ou détail de l'échec (cause
    /// système, ou fichier plus grand que [`MAX_PICKED_BYTES`]).
    Picked {
        /// Nom affiché du fichier.
        name: String,
        /// Octets, ou détail de l'échec de lecture.
        result: Result<Vec<u8>, PickError>,
    },
}

/// Échec de lecture d'un fichier choisi.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickError {
    /// Lecture impossible (détail du système ou du navigateur).
    Read(String),
    /// Fichier plus grand que [`MAX_PICKED_BYTES`].
    TooLarge,
}

/// Lit au plus [`MAX_PICKED_BYTES`] octets de `path` (desktop) : un
/// fichier plus grand est refusé sans être lu en entier.
#[cfg(not(target_arch = "wasm32"))]
fn read_bounded(path: &std::path::Path) -> Result<Vec<u8>, PickError> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).map_err(|e| PickError::Read(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_PICKED_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| PickError::Read(e.to_string()))?;
    let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if len > MAX_PICKED_BYTES {
        return Err(PickError::TooLarge);
    }
    Ok(bytes)
}

/// Sélecteur réel : rfd asynchrone (natif : attendu dans un fil dédié ; web :
/// sur l'exécuteur du navigateur).
#[derive(Clone, Copy, Debug, Default)]
pub struct RfdFileService;

/// Sélecteur inerte (tests) : n'ouvre rien et répond aussitôt
/// [`OpenEvent::Cancelled`], comme un utilisateur qui referme la boîte.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoFileService;

/// Envoie un événement ; une application déjà détruite (ou une demande
/// abandonnée) est ignorée.
fn deliver<T>(tx: &Sender<T>, ctx: &egui::Context, event: T) {
    if tx.send(event).is_err() {
        log::warn!("fichiers : l'application ne reçoit plus les résultats");
    }
    ctx.request_repaint();
}

impl FileService for RfdFileService {
    #[cfg(not(target_arch = "wasm32"))]
    fn pick_and_open(
        &mut self,
        ctx: &egui::Context,
        now: Moment,
        title: String,
        filter: String,
        tx: Sender<OpenEvent>,
    ) {
        // Le fil d'attente est créé avant le sélecteur : s'il ne peut pas
        // l'être, l'erreur s'affiche et aucune boîte ne reste ouverte pour
        // rien. Il reçoit le futur du sélecteur par un canal.
        let (dialog_tx, dialog_rx) = std::sync::mpsc::channel::<PickFuture>();
        let thread_ctx = ctx.clone();
        let thread_tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("plannercraft-open".to_owned())
            .spawn(move || {
                // Canal fermé sans futur : demande abandonnée, rien à envoyer.
                let Ok(picked) = dialog_rx.recv() else {
                    return;
                };
                let event = match pollster::block_on(picked) {
                    None => OpenEvent::Cancelled,
                    Some(handle) => open_path(handle.path(), now),
                };
                deliver(&thread_tx, &thread_ctx, event);
            });
        if let Err(error) = spawned {
            let origin = DocOrigin {
                name: String::new(),
                path: None,
                demo: false,
            };
            let result = Err(OpenError::Read(error.to_string()));
            deliver(&tx, ctx, OpenEvent::Opened { origin, result });
            return;
        }
        // Créé ici, sur le fil d'interface : sur macOS, rfd attache le
        // sélecteur à la fenêtre comme feuille
        // (`beginSheetModalForWindow:completionHandler:`), qui rend la main
        // aussitôt ; aucune boucle modale ne tourne pendant le traitement de
        // l'événement en cours. La fenêtre principale est celle de
        // l'application (`mainWindow`) : eframe n'en ouvre qu'une.
        let picked = rfd::AsyncFileDialog::new()
            .set_title(title)
            .add_filter(filter, &["xml"])
            .pick_file();
        if dialog_tx.send(Box::pin(picked)).is_err() {
            // Le fil s'est arrêté avant de recevoir le futur : la demande
            // n'aboutira pas ; `App` la relance au prochain « Ouvrir ».
            log::error!("ouverture : le fil d'attente du sélecteur s'est arrêté");
            deliver(&tx, ctx, OpenEvent::Cancelled);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_and_open(
        &mut self,
        ctx: &egui::Context,
        now: Moment,
        title: String,
        filter: String,
        tx: Sender<OpenEvent>,
    ) {
        let ctx = ctx.clone();
        // Le réducteur s'exécute en fin d'image (dans `requestAnimationFrame`,
        // pas dans le gestionnaire du clic) et `pick_file` ne crée le
        // `<input type=file>` que dans le futur : le navigateur l'accepte grâce
        // à la fenêtre d'activation transitoire qui suit le clic (environ 5 s
        // dans Chromium). Hypothèse, à vérifier sur Safari.
        let dialog = rfd::AsyncFileDialog::new()
            .set_title(title)
            .add_filter(filter, &["xml"]);
        wasm_bindgen_futures::spawn_local(async move {
            // Annulation : rfd résout la promesse à `None` sur l'événement
            // `cancel` du sélecteur. Un navigateur qui n'émet pas `cancel`
            // (Safari < 16.4) ne résout jamais : aucun événement n'arrive,
            // et `App` relance une ouverture demandée pendant l'attente.
            let event = match dialog.pick_file().await {
                None => OpenEvent::Cancelled,
                Some(handle) => {
                    let origin = DocOrigin {
                        name: handle.file_name(),
                        path: None,
                        demo: false,
                    };
                    // Lecture par `Blob::arrayBuffer` : un échec (fichier
                    // supprimé, permission retirée) rejette la promesse et
                    // devient `OpenError::Read` ; un contenu vide est refusé
                    // par le lecteur MSPDI (`OpenError::Mspdi`). Jamais
                    // `FileHandle::read`, qui n'écoute que `onload` (promesse
                    // jamais résolue en cas d'échec).
                    // Taille contrôlée avant la lecture (relecture finale
                    // de J8) : au-delà de la borne du lecteur MSPDI, rien
                    // n'est lu.
                    let result = match document::check_blob_size(handle.inner().size()) {
                        Ok(()) => read_web_file(&handle)
                            .await
                            .and_then(|bytes| document::load(&bytes, now)),
                        Err(error) => Err(error),
                    };
                    OpenEvent::Opened { origin, result }
                }
            };
            deliver(&tx, &ctx, event);
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save(&mut self, ctx: &egui::Context, request: SaveRequest, tx: Sender<SaveEvent>) {
        let SaveRequest {
            bytes,
            kind,
            target,
            language,
        } = request;
        // Même schéma que l'ouverture : le fil est créé d'abord, le futur
        // du sélecteur (s'il y en a un) lui est envoyé ensuite.
        let (dialog_tx, dialog_rx) = std::sync::mpsc::channel::<SaveFuture>();
        let thread_ctx = ctx.clone();
        let thread_tx = tx.clone();
        let known = match &target {
            SaveTarget::Path(path) => Some(std::path::PathBuf::from(path)),
            SaveTarget::Dialog { .. } => None,
        };
        let spawned = std::thread::Builder::new()
            .name("plannercraft-save".to_owned())
            .spawn(move || {
                let path = if let Some(path) = known {
                    path
                } else {
                    // Canal fermé sans futur : demande abandonnée.
                    let Ok(picked) = dialog_rx.recv() else {
                        return;
                    };
                    let Some(handle) = pollster::block_on(picked) else {
                        deliver(&thread_tx, &thread_ctx, SaveEvent::Cancelled);
                        return;
                    };
                    handle.path().to_path_buf()
                };
                let event = write_path(&path, &bytes, language);
                deliver(&thread_tx, &thread_ctx, event);
            });
        if let Err(error) = spawned {
            deliver(&tx, ctx, SaveEvent::Failed(error.to_string()));
            return;
        }
        let SaveTarget::Dialog {
            suggested_name,
            title,
            filter,
        } = target
        else {
            return;
        };
        // Créé sur le fil d'interface (feuille attachée à la fenêtre sur
        // macOS, ADR-0011), attendu dans `plannercraft-save`.
        let picked = rfd::AsyncFileDialog::new()
            .set_title(title)
            .set_file_name(suggested_name)
            .add_filter(filter, &[kind.extension()])
            .save_file();
        if dialog_tx.send(Box::pin(picked)).is_err() {
            log::error!("enregistrement : le fil d'attente du sélecteur s'est arrêté");
            deliver(&tx, ctx, SaveEvent::Cancelled);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_bytes(&mut self, ctx: &egui::Context, request: PickRequest, tx: Sender<PickedFile>) {
        // Même schéma que l'ouverture : fil d'attente d'abord, sélecteur
        // créé sur le fil d'interface ensuite (ADR-0011).
        let (dialog_tx, dialog_rx) = std::sync::mpsc::channel::<PickFuture>();
        let thread_ctx = ctx.clone();
        let thread_tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("plannercraft-pick".to_owned())
            .spawn(move || {
                let Ok(picked) = dialog_rx.recv() else {
                    return;
                };
                let event = match pollster::block_on(picked) {
                    None => PickedFile::Cancelled,
                    Some(handle) => PickedFile::Picked {
                        name: handle
                            .path()
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        result: read_bounded(handle.path()),
                    },
                };
                deliver(&thread_tx, &thread_ctx, event);
            });
        if let Err(error) = spawned {
            let event = PickedFile::Picked {
                name: String::new(),
                result: Err(PickError::Read(error.to_string())),
            };
            deliver(&tx, ctx, event);
            return;
        }
        let picked = rfd::AsyncFileDialog::new()
            .set_title(request.title)
            .add_filter(request.filter, &request.extensions)
            .pick_file();
        if dialog_tx.send(Box::pin(picked)).is_err() {
            log::error!("import : le fil d'attente du sélecteur s'est arrêté");
            deliver(&tx, ctx, PickedFile::Cancelled);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_bytes(&mut self, ctx: &egui::Context, request: PickRequest, tx: Sender<PickedFile>) {
        let ctx = ctx.clone();
        // Téléversement comme l'ouverture (fenêtre d'activation qui suit le
        // clic) ; taille contrôlée avant la lecture du `Blob`.
        let dialog = rfd::AsyncFileDialog::new()
            .set_title(request.title)
            .add_filter(request.filter, &request.extensions);
        wasm_bindgen_futures::spawn_local(async move {
            let event = match dialog.pick_file().await {
                None => PickedFile::Cancelled,
                Some(handle) => {
                    // `Blob.size` (f64) comparé à `MAX_PICKED_BYTES` (64 Mio,
                    // exact en f64) ; une valeur non finie est refusée.
                    let size = handle.inner().size();
                    let too_large = !(0.0..=67_108_864.0).contains(&size);
                    let result = if too_large {
                        Err(PickError::TooLarge)
                    } else {
                        read_web_file(&handle).await.map_err(|error| match error {
                            OpenError::Read(detail) => PickError::Read(detail),
                            OpenError::Mspdi(error) => PickError::Read(error.to_string()),
                        })
                    };
                    PickedFile::Picked {
                        name: handle.file_name(),
                        result,
                    }
                }
            };
            deliver(&tx, &ctx, event);
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn save(&mut self, ctx: &egui::Context, request: SaveRequest, tx: Sender<SaveEvent>) {
        let SaveRequest {
            bytes, kind, target, ..
        } = request;
        // Pas de chemin sur le web : un chemin connu (impossible ici) est
        // traité comme un téléchargement sous son nom.
        let name = match target {
            SaveTarget::Path(path) => path,
            SaveTarget::Dialog { suggested_name, .. } => suggested_name,
        };
        // Téléchargement direct (Blob + ancre `download`, relecture finale
        // J4, F3) : rfd 0.17 affichait sa propre fenêtre, dont « Ok » et
        // « Annuler » rendaient tous deux `Ok` sans rien télécharger, et le
        // document passait à tort à « enregistré ».
        let event = match download(&name, kind.mime(), &bytes) {
            Ok(()) => SaveEvent::Saved { name, path: None },
            Err(detail) => SaveEvent::Failed(detail),
        };
        deliver(&tx, ctx, event);
    }
}

/// Remet `bytes` au navigateur comme téléchargement nommé `name` : `Blob`
/// de type `mime` (MSPDI, CSV, SVG ou PDF, [`SaveKind::mime`]), URL d'objet, clic sur une ancre `download` (rien n'est ajouté à
/// la page), URL révoquée une minute plus tard. Aucun JavaScript écrit à la
/// main (règle 8) : tout passe par `web_sys`. Une exception du navigateur
/// devient le détail de l'erreur.
#[cfg(target_arch = "wasm32")]
fn download(name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
    use wasm_bindgen_futures::wasm_bindgen::JsCast as _;
    use wasm_bindgen_futures::wasm_bindgen::closure::Closure;

    let window = web_sys::window().ok_or_else(|| "window introuvable".to_owned())?;
    let document = window
        .document()
        .ok_or_else(|| "document introuvable".to_owned())?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|error| js_error_text(&error))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|error| js_error_text(&error))?;
    let anchor = document
        .create_element("a")
        .map_err(|error| js_error_text(&error))?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "élément <a> inattendu".to_owned())?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    // Révoquée plus tard : certains navigateurs lisent l'URL après le clic.
    let revoke = Closure::once_into_js(move || {
        if web_sys::Url::revoke_object_url(&url).is_err() {
            log::warn!("URL de téléchargement non révoquée");
        }
    });
    if window
        .set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 60_000)
        .is_err()
    {
        log::warn!("révocation de l'URL de téléchargement non planifiée");
    }
    Ok(())
}

/// Futur d'une boîte « Enregistrer sous » native, envoyé au fil
/// `plannercraft-save`.
#[cfg(not(target_arch = "wasm32"))]
type SaveFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Option<rfd::FileHandle>> + Send>>;

/// Écrit atomiquement (`files::write_atomic`) et rend l'événement
/// correspondant.
#[cfg(not(target_arch = "wasm32"))]
fn write_path(path: &std::path::Path, bytes: &[u8], language: crate::i18n::Language) -> SaveEvent {
    let lang = match language {
        crate::i18n::Language::Fr => plannercraft_automation::text::Language::Fr,
        crate::i18n::Language::En => plannercraft_automation::text::Language::En,
    };
    match plannercraft_automation::files::write_atomic(path, bytes) {
        Ok(()) => SaveEvent::Saved {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: path.to_str().map(str::to_owned),
        },
        Err(error) => SaveEvent::Failed(error.message(lang)),
    }
}

/// Nom de fichier proposé à l'enregistrement : `name` sans les caractères
/// refusés par Windows, macOS ou Linux (`<>:"/\|?*` et contrôles, remplacés
/// par `_`), espaces de bord retirés, suffixé de `.xml` s'il n'y est pas ;
/// « plannercraft.xml » si rien ne reste.
#[must_use]
pub fn suggested_file_name(name: &str) -> String {
    SaveKind::Mspdi.file_name(name)
}

/// Nom assaini (voir [`suggested_file_name`]) suffixé de `extension`
/// (sans le point), sauf s'il l'est déjà (casse ignorée).
fn file_name_with(name: &str, extension: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.');
    let stem = if cleaned.is_empty() {
        plannercraft_core::PRODUCT_NAME
    } else {
        cleaned
    };
    let suffix = format!(".{extension}");
    if stem.to_ascii_lowercase().ends_with(&suffix) {
        stem.to_owned()
    } else {
        format!("{stem}{suffix}")
    }
}

/// Document ouvert depuis un CSV des tâches (H-UI-40, lot L3) : même chemin
/// que l'ouverture MSPDI (nouveau document, planning calculé, historique
/// vide), diagnostics de l'import ligne par ligne et réglages retenus.
#[derive(Debug)]
pub struct CsvLoaded {
    /// Document ouvert.
    pub document: LoadedDocument,
    /// Diagnostics de l'import (R-144, R-145), dans l'ordre des lignes.
    pub diagnostics: Vec<plannercraft_automation::csv::CsvDiagnostic>,
    /// Séparateur, paramètres régionaux et encodage retenus.
    pub detected: plannercraft_automation::csv::Detected,
}

/// Importe un CSV des tâches comme un nouveau document (Fichier → Importer
/// CSV…, H-UI-40). `now` est la date courante injectée : aucune horloge
/// n'est lue ici (ni dans `opts`, que l'appelant remplit).
///
/// # Errors
///
/// [`plannercraft_automation::csv::CsvError`] : refus du fichier entier (trop
/// grand, vide, sans en-tête exploitable, bornes) ; une valeur illisible
/// est un diagnostic de sa ligne.
pub fn load_csv(
    bytes: &[u8],
    opts: &plannercraft_automation::csv::CsvImportOptions,
    now: Moment,
) -> Result<CsvLoaded, plannercraft_automation::csv::CsvError> {
    let imported = plannercraft_automation::csv::import(bytes, opts)?;
    Ok(CsvLoaded {
        document: document::load_project(imported.project, now),
        diagnostics: imported.diagnostics,
        detected: imported.detected,
    })
}

/// Futur d'un sélecteur natif, envoyé au fil d'attente.
#[cfg(not(target_arch = "wasm32"))]
type PickFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Option<rfd::FileHandle>> + Send>>;

/// Lit et calcule un fichier choisi sur le desktop. Chemin lu par
/// `document::read_path` (borné à 256 Mio avant lecture), jamais par
/// `FileHandle::read`.
#[cfg(not(target_arch = "wasm32"))]
fn open_path(path: &std::path::Path, now: Moment) -> OpenEvent {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let origin = DocOrigin {
        name,
        path: path.to_str().map(str::to_owned),
        demo: false,
    };
    let result = document::read_path(path).and_then(|bytes| document::load(&bytes, now));
    OpenEvent::Opened { origin, result }
}

/// Lit le contenu d'un fichier choisi dans le navigateur
/// (`Blob::arrayBuffer`). Toute exception JavaScript devient
/// `OpenError::Read` avec son message.
#[cfg(target_arch = "wasm32")]
async fn read_web_file(handle: &rfd::FileHandle) -> Result<Vec<u8>, OpenError> {
    // `inner` rend le `web_sys::File` d'un descripteur obtenu par
    // `pick_file` (toujours lisible).
    let promise = handle.inner().array_buffer();
    match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(buffer) => Ok(js_sys::Uint8Array::new(&buffer).to_vec()),
        Err(error) => Err(OpenError::Read(js_error_text(&error))),
    }
}

/// Message d'une exception JavaScript (`error.message`), sinon sa forme de
/// débogage.
#[cfg(target_arch = "wasm32")]
fn js_error_text(error: &wasm_bindgen_futures::wasm_bindgen::JsValue) -> String {
    js_sys::Reflect::get(error, &js_sys::JsString::from("message"))
        .ok()
        .and_then(|message| message.as_string())
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| format!("{error:?}"))
}

impl FileService for NoFileService {
    fn pick_and_open(
        &mut self,
        ctx: &egui::Context,
        _now: Moment,
        _title: String,
        _filter: String,
        tx: Sender<OpenEvent>,
    ) {
        deliver(&tx, ctx, OpenEvent::Cancelled);
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{load_csv, suggested_file_name};
    use plannercraft_automation::csv::{CsvError, CsvImportOptions, CsvLocale};
    use plannercraft_core::{Moment, ScheduleAnchor};

    #[test]
    fn load_csv_opens_a_new_document_with_diagnostics() {
        let now: Moment = "2026-10-19T08:00".parse().unwrap();
        let opts = CsvImportOptions::new(ScheduleAnchor::FromStart(now), now);
        let loaded = load_csv(
            "Nom;Durée;Couleur\nCoffrage;3j;bleu\nFerraillage;zz;\n".as_bytes(),
            &opts,
            now,
        )
        .unwrap();
        let session = &loaded.document.session;
        assert_eq!(session.project().tasks.len(), 2);
        assert!(
            session
                .schedule()
                .scheduled(plannercraft_core::TaskUid::new(1))
                .is_some()
        );
        assert!(!session.is_dirty());
        assert_eq!(loaded.detected.locale, CsvLocale::Fr);
        let codes: Vec<&str> = loaded.diagnostics.iter().map(|d| d.code.code()).collect();
        assert_eq!(codes, ["csv.unknown_column", "csv.bad_value"]);
        assert_eq!(loaded.document.import_diagnostics.len(), 0);
        assert_eq!(
            load_csv(b"", &opts, now).map(|_| ()).unwrap_err(),
            CsvError::Empty
        );
    }

    /// Lecture d'un fichier choisi pour l'import (J8, lot L10) : octets
    /// rendus tels quels ; fichier absent ⇒ détail du système, jamais de
    /// panique.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn read_bounded_reads_and_reports_errors() {
        let dir = std::env::temp_dir().join(format!("plannercraft-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("taches.csv");
        std::fs::write(&path, b"Nom;Dur\xc3\xa9e\n").unwrap();
        assert_eq!(super::read_bounded(&path).unwrap(), b"Nom;Dur\xc3\xa9e\n");
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            super::read_bounded(&path),
            Err(super::PickError::Read(_))
        ));
        std::fs::remove_dir(&dir).unwrap();
        assert_eq!(
            super::MAX_PICKED_BYTES,
            plannercraft_automation::csv::CsvLimits::default().max_bytes
        );
    }

    #[test]
    fn suggested_file_name_is_safe_and_xml() {
        assert_eq!(suggested_file_name("Projet1"), "Projet1.xml");
        assert_eq!(suggested_file_name("maison.XML"), "maison.XML");
        assert_eq!(
            suggested_file_name("Exemple : construction d'une maison"),
            "Exemple _ construction d'une maison.xml"
        );
        assert_eq!(suggested_file_name("a/b\\c*?\u{7}"), "a_b_c___.xml");
        assert_eq!(
            suggested_file_name("  ...  "),
            format!("{}.xml", plannercraft_core::PRODUCT_NAME)
        );
    }
}

/// Genre d'un enregistrement (J8, ADR-0016 D13 ; sur le web, chacun est un
/// téléchargement). Contrat posé par le lot L0, câblé en L13.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SaveKind {
    /// Projet MSPDI (`.xml`).
    #[default]
    Mspdi,
    /// Tâches en CSV.
    Csv,
    /// Gantt en SVG.
    Svg,
    /// Planning en PDF.
    Pdf,
}

impl SaveKind {
    /// Les quatre genres.
    pub const ALL: [Self; 4] = [Self::Mspdi, Self::Csv, Self::Svg, Self::Pdf];

    /// Extension du fichier, sans le point.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mspdi => "xml",
            Self::Csv => "csv",
            Self::Svg => "svg",
            Self::Pdf => "pdf",
        }
    }

    /// Type MIME du téléchargement (web) : le navigateur en déduit
    /// l'application qui ouvre le fichier.
    #[must_use]
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Mspdi => "application/xml",
            Self::Csv => "text/csv;charset=utf-8",
            Self::Svg => "image/svg+xml",
            Self::Pdf => "application/pdf",
        }
    }

    /// Nom de fichier proposé pour le document `name` : caractères refusés
    /// remplacés (comme [`suggested_file_name`]), extension MSPDI d'origine
    /// retirée pour un export (« maison.xml » ⇒ « maison.pdf »).
    #[must_use]
    pub fn file_name(self, name: &str) -> String {
        let trimmed = name.trim();
        let stem = match self {
            Self::Mspdi => trimmed,
            Self::Csv | Self::Svg | Self::Pdf => trimmed
                .len()
                .checked_sub(4)
                .and_then(|cut| trimmed.split_at_checked(cut))
                .filter(|(_, ext)| ext.eq_ignore_ascii_case(".xml"))
                .map_or(trimmed, |(stem, _)| stem),
        };
        file_name_with(stem, self.extension())
    }
}

#[cfg(test)]
mod save_kind_tests {
    use super::SaveKind;

    /// Noms, extensions et types MIME des quatre genres (J8, lot L13).
    #[test]
    fn save_kind_names_and_mime() {
        let ext: Vec<&str> = SaveKind::ALL.iter().map(|k| k.extension()).collect();
        assert_eq!(ext, ["xml", "csv", "svg", "pdf"]);
        let mime: Vec<&str> = SaveKind::ALL.iter().map(|k| k.mime()).collect();
        assert_eq!(
            mime,
            [
                "application/xml",
                "text/csv;charset=utf-8",
                "image/svg+xml",
                "application/pdf"
            ]
        );
        assert_eq!(SaveKind::Mspdi.file_name("maison"), "maison.xml");
        assert_eq!(SaveKind::Mspdi.file_name("maison.XML"), "maison.XML");
        assert_eq!(SaveKind::Pdf.file_name("maison.xml"), "maison.pdf");
        assert_eq!(SaveKind::Svg.file_name("Maison.XML"), "Maison.svg");
        assert_eq!(SaveKind::Csv.file_name("a/b : c"), "a_b _ c.csv");
        assert_eq!(SaveKind::Pdf.file_name("plan.pdf"), "plan.pdf");
        assert_eq!(SaveKind::Pdf.file_name("été"), "été.pdf");
        assert_eq!(SaveKind::Pdf.file_name("é"), "é.pdf");
        assert_eq!(
            SaveKind::Pdf.file_name(".xml"),
            format!("{}.pdf", plannercraft_core::PRODUCT_NAME)
        );
        assert_eq!(SaveKind::default(), SaveKind::Mspdi);
    }
}
