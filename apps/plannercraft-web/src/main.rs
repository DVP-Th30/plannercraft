//! Version web de PlannerCraft : même interface que le desktop, compilée en
//! `wasm32-unknown-unknown` et servie par trunk (`trunk serve` dans ce
//! dossier).
//!
//! Aucun JavaScript écrit à la main (règle 8) : trunk génère la colle
//! wasm-bindgen, `index.html` ne contient que le canevas. La CI vérifie la
//! compilation en `wasm32-unknown-unknown` (clippy, sans trunk) ; seul
//! `trunk build` attend le packaging web. « Maintenant » vient de la date du
//! navigateur et la langue de `navigator.language`, injectés par `Env`.
//!
//! L'onglet porte le titre de la fenêtre desktop (« *nom — PlannerCraft » si le
//! document est modifié) : eframe web ignore `ViewportCommand::Title`, un
//! observateur du document ([`web::TabObserver`]) pose `document.title`.
//! Le même observateur tient un drapeau « modifié » ([`UnsavedFlag`]) : tant
//! qu'il est levé, l'écouteur `beforeunload` demande confirmation avant de
//! fermer ou de recharger l'onglet (relecture finale J4, F3).
//!
//! Au démarrage, `<html>` reçoit `data-plannercraft-started="1"` : la
//! vérification de la release y lit la preuve que le wasm a tourné
//! (C-REL-06, `docs/features/release.md`).
//!
//! Chargement (H-UI-57, J8 lot L13) : `index.html` affiche « Chargement… »
//! en CSS seul ; Rust retire ce bloc quand `WebRunner::start` a réussi, ou
//! y écrit un message traduit ([`start_error_message`]) s'il échoue
//! (navigateur sans WebGL2 ni WebGPU, par exemple).
//!
//! Raccourcis (H-UI-55, J8 lot L13) : eframe ne retient au navigateur
//! qu'une liste figée de touches (Ctrl/Cmd+O, P, S, flèches…). Un écouteur
//! `keydown` en phase de capture sur `window` appelle `preventDefault` pour
//! les entrées interceptables de `KEYMAP` que l'image courante lirait
//! ([`plannercraft_ui_egui::keymap::web_intercepts`], périmètre photographié
//! après chaque image par [`web::WebApp`]) : Ctrl/Cmd+F ouvre la recherche
//! de PlannerCraft et non celle du navigateur. L'évènement n'est pas arrêté :
//! eframe le reçoit ensuite comme avant.
//!
//! Compilée pour une cible native (ce que fait `cargo check --workspace`), le
//! binaire se contente d'indiquer qu'il faut passer par trunk.

use std::cell::Cell;
use std::rc::Rc;

use plannercraft_ui_egui::i18n::{Arg, TextId, Texts};
use plannercraft_ui_egui::{DocumentObserver, Language, OpenDocument};

/// Drapeau partagé « le document ouvert est modifié », lu par l'écouteur
/// `beforeunload` et tenu par l'observateur du document.
#[cfg_attr(
    not(any(target_arch = "wasm32", test)),
    allow(dead_code, reason = "utilisé par la seule cible wasm32 (et les tests)")
)]
#[derive(Clone, Debug, Default)]
pub struct UnsavedFlag(Rc<Cell<bool>>);

#[cfg_attr(
    not(any(target_arch = "wasm32", test)),
    allow(dead_code, reason = "utilisé par la seule cible wasm32 (et les tests)")
)]
impl UnsavedFlag {
    /// Le document courant a des modifications non enregistrées.
    #[must_use]
    pub fn is_set(&self) -> bool {
        self.0.get()
    }
}

impl DocumentObserver for UnsavedFlag {
    fn document_changed(&mut self, document: Option<&OpenDocument>) {
        self.0.set(document.is_some_and(OpenDocument::is_dirty));
    }
}

/// Message affiché à la place de « Chargement… » quand l'interface ne
/// peut pas démarrer (H-UI-57) : traduit dans la langue du navigateur
/// (français par défaut), avec le détail rendu par eframe.
#[must_use]
pub fn start_error_message(language: Option<Language>, detail: &str) -> String {
    let texts = Texts::new(language.unwrap_or(Language::Fr));
    texts.get_args(
        TextId::WebStartFailed,
        &[
            ("product", Arg::Str(plannercraft_core::PRODUCT_NAME.into())),
            ("detail", Arg::Str(detail.into())),
        ],
    )
}

#[cfg(target_arch = "wasm32")]
mod web {
    use eframe::wasm_bindgen::closure::Closure;
    use eframe::wasm_bindgen::{JsCast as _, JsValue};
    use plannercraft_core::Moment;
    use std::cell::Cell;
    use std::rc::Rc;

    use eframe::egui;
    use plannercraft_ui_egui::app::window_title_for;
    use plannercraft_ui_egui::keymap::{KeyScope, web_intercepts};
    use plannercraft_ui_egui::{
        DocumentObserver, Env, Language, OpenDocument, PlannerCraftApp, RfdFileService,
    };

    use super::{UnsavedFlag, start_error_message};

    /// Identifiant du canevas déclaré dans `index.html`.
    const CANVAS_ID: &str = "plannercraft_canvas";

    /// Attribut posé sur `<html>` au démarrage du wasm (C-REL-06).
    const STARTED_ATTR: &str = "data-plannercraft-started";

    /// Bloc « Chargement… » de `index.html` (H-UI-57).
    const LOADING_ID: &str = "plannercraft_loading";

    /// Classe posée sur le bloc de chargement quand le démarrage échoue
    /// (animation arrêtée, message lisible).
    const FAILED_CLASS: &str = "failed";

    /// Enveloppe web de [`PlannerCraftApp`] : après chaque image, photographie
    /// les raccourcis lus ([`plannercraft_ui_egui::App::key_scope`]) pour la garde
    /// `keydown` (H-UI-55).
    pub struct WebApp {
        /// Application commune au desktop et au web.
        inner: PlannerCraftApp,
        /// Périmètre lu par l'écouteur `keydown`.
        scope: Rc<Cell<KeyScope>>,
    }

    impl eframe::App for WebApp {
        fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
            self.inner.raw_input_hook(ctx, raw_input);
        }

        fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
            self.inner.ui(ui, frame);
            self.scope.set(self.inner.app().key_scope(ui.ctx()));
        }

        fn save(&mut self, storage: &mut dyn eframe::Storage) {
            self.inner.save(storage);
        }

        fn persist_egui_memory(&self) -> bool {
            self.inner.persist_egui_memory()
        }
    }

    /// Modificateurs d'un évènement clavier, lus comme eframe les lit
    /// (`web/input.rs` : Cmd ou Ctrl valent « commande »), pour que la
    /// garde et l'application voient le même raccourci.
    fn modifiers_of(event: &web_sys::KeyboardEvent) -> egui::Modifiers {
        egui::Modifiers {
            alt: event.alt_key(),
            ctrl: event.ctrl_key(),
            shift: event.shift_key(),
            mac_cmd: event.meta_key(),
            command: event.ctrl_key() || event.meta_key(),
        }
    }

    /// Garde des raccourcis (H-UI-55) : `keydown` en phase de capture sur
    /// `window`, avant le navigateur et avant eframe ; `preventDefault` pour
    /// les entrées interceptables que l'image lirait. L'écouteur vit autant
    /// que la page (`forget`).
    fn guard_keys(window: &web_sys::Window, scope: Rc<Cell<KeyScope>>) -> Result<(), JsValue> {
        let listener =
            Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
                if event.is_composing() {
                    return;
                }
                // Touche logique d'abord, physique en repli (claviers non
                // latins), comme eframe.
                let key = egui::Key::from_name(&event.key()).or_else(|| egui::Key::from_name(&event.code()));
                if let Some(key) = key
                    && web_intercepts(scope.get(), modifiers_of(&event), key)
                {
                    event.prevent_default();
                }
            });
        window.add_event_listener_with_callback_and_bool(
            "keydown",
            listener.as_ref().unchecked_ref(),
            true,
        )?;
        listener.forget();
        Ok(())
    }

    /// Fin du chargement (H-UI-57) : bloc retiré si l'interface a démarré,
    /// sinon remplacé par le message traduit.
    fn finish_loading(document: &web_sys::Document, failure: Option<&str>) {
        let Some(loading) = document.get_element_by_id(LOADING_ID) else {
            return;
        };
        match failure {
            None => loading.remove(),
            Some(message) => {
                loading.set_text_content(Some(message));
                if loading.set_attribute("class", FAILED_CLASS).is_err() {
                    log::warn!("bloc de chargement : classe non posée");
                }
            }
        }
    }

    /// Détail lisible d'une erreur JavaScript.
    fn js_detail(error: &JsValue) -> String {
        error.as_string().unwrap_or_else(|| format!("{error:?}"))
    }

    /// Observateur du document : titre de l'onglet et drapeau « modifié »
    /// tenus à jour (ouverture, édition, enregistrement, fermeture).
    pub struct TabObserver {
        /// Document HTML de la page.
        document: web_sys::Document,
        /// Drapeau lu par l'écouteur `beforeunload`.
        unsaved: UnsavedFlag,
    }

    impl DocumentObserver for TabObserver {
        fn document_changed(&mut self, document: Option<&OpenDocument>) {
            self.document.set_title(&window_title_for(document));
            self.unsaved.document_changed(document);
        }
    }

    /// Écouteur `beforeunload` : fermer ou recharger l'onglet d'un document
    /// modifié demande confirmation (boîte du navigateur). L'écouteur vit
    /// autant que la page (`forget`).
    fn guard_unload(window: &web_sys::Window, unsaved: UnsavedFlag) -> Result<(), JsValue> {
        let listener = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
            move |event: web_sys::BeforeUnloadEvent| {
                if unsaved.is_set() {
                    event.prevent_default();
                    // Navigateurs plus anciens : une valeur non vide.
                    event.set_return_value(plannercraft_core::PRODUCT_NAME);
                }
            },
        );
        window.add_event_listener_with_callback("beforeunload", listener.as_ref().unchecked_ref())?;
        listener.forget();
        Ok(())
    }

    /// Point d'entrée appelé par la colle wasm-bindgen au chargement.
    pub fn main() {
        // Un échec d'initialisation du journal n'empêche pas l'application de tourner.
        let _ = eframe::WebLogger::init(log::LevelFilter::Info);
        wasm_bindgen_futures::spawn_local(async {
            let result = start().await;
            let Some(document) = web_sys::window().and_then(|window| window.document()) else {
                return;
            };
            match result {
                Ok(()) => finish_loading(&document, None),
                Err(error) => {
                    log::error!(
                        "{} : échec du démarrage : {error:?}",
                        plannercraft_core::PRODUCT_NAME
                    );
                    let message = start_error_message(Language::from_system(), &js_detail(&error));
                    finish_loading(&document, Some(&message));
                }
            }
        });
    }

    /// « Maintenant » d'après l'horloge locale du navigateur, tronqué à la
    /// minute ; repli sur un instant fixe (signalé) hors du domaine du cœur.
    fn now() -> Moment {
        let date = js_sys::Date::new_0();
        let naive = i32::try_from(date.get_full_year())
            .ok()
            .zip(date.get_month().checked_add(1))
            .and_then(|(year, month)| chrono::NaiveDate::from_ymd_opt(year, month, date.get_date()))
            .and_then(|day| day.and_hms_opt(date.get_hours(), date.get_minutes(), 0));
        match naive.map(Moment::from_naive_floor) {
            Some(Ok((moment, _truncated))) => moment,
            Some(Err(error)) => {
                log::warn!(
                    "date du navigateur inutilisable ({error}), repli sur {}",
                    Moment::MIN
                );
                Moment::MIN
            }
            None => {
                log::warn!("date du navigateur invalide, repli sur {}", Moment::MIN);
                Moment::MIN
            }
        }
    }

    async fn start() -> Result<(), JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("window introuvable"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("document introuvable"))?;
        document.set_title(plannercraft_core::PRODUCT_NAME);
        // C-REL-06 (J8, lot L1) : preuve que le wasm a tourné, lue dans le
        // DOM par la vérification de la release (`chromium --headless`),
        // posée avant l'initialisation graphique (absente en CI).
        if let Some(root) = document.document_element() {
            root.set_attribute(STARTED_ATTR, "1")?;
        }
        let unsaved = UnsavedFlag::default();
        guard_unload(&window, unsaved.clone())?;
        let scope = Rc::new(Cell::new(KeyScope::Nothing));
        guard_keys(&window, Rc::clone(&scope))?;
        let tab = TabObserver {
            document: document.clone(),
            unsaved,
        };
        let canvas = document
            .get_element_by_id(CANVAS_ID)
            .ok_or_else(|| JsValue::from_str("canevas introuvable"))?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .map_err(|_| JsValue::from_str("l'élément n'est pas un canevas"))?;
        eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| {
                    let env = Env {
                        now: Box::new(now),
                        system_language: Language::from_system(),
                        files: Box::new(RfdFileService),
                    };
                    let mut app = PlannerCraftApp::new(cc, env);
                    app.app_mut().set_observer(Box::new(tab));
                    // Assistant IA (ADR-0024, lot A1) : `fetch`, clé en
                    // mémoire, connexion OpenRouter par redirection.
                    app.app_mut()
                        .set_assistant_services(plannercraft_ui_egui::assistant::AssistantServices::web());
                    Ok(Box::new(WebApp { inner: app, scope }))
                }),
            )
            .await
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::main();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    eprintln!(
        "{} (web) se construit avec trunk pour la cible wasm32-unknown-unknown : `trunk serve` dans apps/plannercraft-web.",
        plannercraft_core::PRODUCT_NAME
    );
    std::process::ExitCode::FAILURE
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_automation::Batch;
    use plannercraft_ui_egui::{App, EditOrigin, Env};

    use plannercraft_ui_egui::Language;

    use super::{UnsavedFlag, start_error_message};

    /// H-UI-57 : message de démarrage impossible traduit, détail repris ;
    /// langue inconnue ⇒ français.
    #[test]
    fn h_ui_57_start_error_message() {
        let fr = start_error_message(Some(Language::Fr), "WebGL2 indisponible");
        assert!(fr.contains("WebGL2 indisponible"), "{fr}");
        assert!(fr.contains("navigateur"), "{fr}");
        let en = start_error_message(Some(Language::En), "no adapter");
        assert!(en.contains("no adapter"), "{en}");
        assert!(en.contains("browser"), "{en}");
        assert_ne!(fr, en);
        assert_eq!(
            start_error_message(None, "x"),
            start_error_message(Some(Language::Fr), "x")
        );
    }

    /// Relecture finale J4, F3 : le drapeau lu par `beforeunload` suit le
    /// document (levé par une édition, baissé par l'annulation et la
    /// fermeture).
    #[test]
    fn web_unsaved_flag_follows_document() {
        let flag = UnsavedFlag::default();
        let mut app = App::new(
            plannercraft_ui_egui::UiState::default(),
            Env::fixed("2027-01-15T08:00".parse().unwrap()),
        );
        app.set_observer(Box::new(flag.clone()));
        app.open_demo();
        assert!(!flag.is_set());
        let batch =
            Batch::from_json_str(r#"{"format_version":1,"ops":[{"op":"task.update","uid":3,"name":"X"}]}"#)
                .unwrap();
        app.execute(&batch, EditOrigin::User).unwrap();
        assert!(flag.is_set());
        app.undo().unwrap();
        assert!(!flag.is_set());
        app.execute(&batch, EditOrigin::User).unwrap();
        app.open_demo();
        assert!(!flag.is_set());
    }
}
