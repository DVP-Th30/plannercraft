//! Sauvegarde automatique du navigateur, côté navigateur (lot W1,
//! ADR-0031) : verrou Web Locks, base `IndexedDB`, écrivain de l'emplacement.
//! Code mince : quoi écrire, quand, et que faire au démarrage est décidé
//! par `plannercraft_ui_egui::autosave` (testé en natif).
//!
//! - Verrou : `navigator.locks.request(LOCK_NAME, { ifAvailable: true }, …)`
//!   (absent de `web-sys` stable : `js_sys::Reflect`) ; obtenu, il est tenu
//!   jusqu'à la fermeture de l'onglet par une promesse jamais résolue.
//! - Lecture : une transaction `readonly`, `getAllKeys` et `getAll` (même
//!   ordre de clés).
//! - Écriture : une transaction `readwrite` par lot de `KvOp`, sans attente
//!   (`IndexedDB` les applique dans l'ordre de création) ; le premier
//!   `abort` d'une transaction (quota plein, base fermée) est retenu et
//!   arrête toute écriture suivante.
//!
//! Aucune attente sans borne au démarrage : verrou et ouverture de la base
//! sont abandonnés après [`START_TIMEOUT_MS`] (l'application démarre alors
//! sans sauvegarde automatique, et le dit).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use eframe::wasm_bindgen::closure::Closure;
use eframe::wasm_bindgen::{JsCast, JsValue};
use plannercraft_ui_egui::autosave::{
    DB_NAME, KvOp, LOCK_NAME, LockState, STORE_NAME, SlotWriter, StartPlan, plan_start,
};
use wasm_bindgen_futures::JsFuture;

/// Délai maximal du verrou et de l'ouverture de la base, en millisecondes.
const START_TIMEOUT_MS: i32 = 3_000;

/// Démarrage décidé et écrivain de l'emplacement (si cet onglet écrit).
pub struct Started {
    /// Décision de démarrage.
    pub plan: StartPlan,
    /// Écrivain `IndexedDB`, présent si `plan.writes()`.
    pub writer: Option<Box<dyn SlotWriter>>,
}

/// Verrou, ouverture et lecture de l'emplacement ; jamais d'échec : un
/// problème devient [`StartPlan::Unavailable`].
pub async fn start() -> Started {
    let lock = acquire_lock().await;
    if lock != LockState::Acquired {
        return Started {
            plan: plan_start(lock, None),
            writer: None,
        };
    }
    let db = match open_db().await {
        Ok(db) => db,
        Err(detail) => {
            return Started {
                plan: plan_start(lock, Some(Err(detail))),
                writer: None,
            };
        }
    };
    let plan = plan_start(lock, Some(read_all(&db).await));
    let writer = plan
        .writes()
        .then(|| Box::new(IdbWriter::new(db)) as Box<dyn SlotWriter>);
    Started { plan, writer }
}

/// Texte d'une exception ou d'une valeur JavaScript.
fn js_text(error: &JsValue) -> String {
    let field = |name: &str| {
        js_sys::Reflect::get(error, &JsValue::from_str(name))
            .ok()
            .and_then(|value| value.as_string())
            .filter(|value| !value.is_empty())
    };
    match (field("name"), field("message")) {
        (Some(name), Some(message)) => format!("{name} : {message}"),
        (Some(text), None) | (None, Some(text)) => text,
        (None, None) => error.as_string().unwrap_or_else(|| format!("{error:?}")),
    }
}

/// `promise`, ou `undefined` après [`START_TIMEOUT_MS`].
async fn with_timeout(promise: js_sys::Promise) -> Result<JsValue, JsValue> {
    let timeout = js_sys::Promise::new(&mut |resolve, _reject| {
        let scheduled = web_sys::window().map(|window| {
            window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, START_TIMEOUT_MS)
        });
        if !matches!(scheduled, Some(Ok(_))) {
            log::warn!("délai de démarrage de la sauvegarde non planifié");
        }
    });
    JsFuture::from(js_sys::Promise::race(&js_sys::Array::of2(&promise, &timeout))).await
}

/// Demande le verrou sans attendre qu'il se libère (`ifAvailable`).
async fn acquire_lock() -> LockState {
    let Some(window) = web_sys::window() else {
        return LockState::Unsupported;
    };
    let navigator: JsValue = window.navigator().into();
    let locks = js_sys::Reflect::get(&navigator, &JsValue::from_str("locks")).unwrap_or(JsValue::UNDEFINED);
    if locks.is_undefined() || locks.is_null() {
        return LockState::Unsupported;
    }
    let Some(request) = js_sys::Reflect::get(&locks, &JsValue::from_str("request"))
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
    else {
        return LockState::Unsupported;
    };
    // Promesse résolue par le rappel du verrou : vrai s'il est obtenu.
    let mut resolver = None;
    let answered = js_sys::Promise::new(&mut |resolve, _reject| resolver = Some(resolve));
    let Some(resolve) = resolver else {
        return LockState::Unsupported;
    };
    let callback = Closure::once_into_js(move |lock: JsValue| -> JsValue {
        let held = !lock.is_null() && !lock.is_undefined();
        if resolve
            .call1(&JsValue::UNDEFINED, &JsValue::from_bool(held))
            .is_err()
        {
            log::warn!("verrou de la sauvegarde : réponse perdue");
        }
        if held {
            // Tenu tant que l'onglet vit : promesse jamais résolue.
            js_sys::Promise::new(&mut |_resolve, _reject| {}).into()
        } else {
            JsValue::UNDEFINED
        }
    });
    let options = js_sys::Object::new();
    if js_sys::Reflect::set(&options, &JsValue::from_str("ifAvailable"), &JsValue::TRUE).is_err() {
        return LockState::Unsupported;
    }
    if let Err(error) = request.call3(&locks, &JsValue::from_str(LOCK_NAME), options.as_ref(), &callback) {
        log::warn!("verrou de la sauvegarde refusé : {}", js_text(&error));
        return LockState::Unsupported;
    }
    match with_timeout(answered).await.map(|value| value.as_bool()) {
        Ok(Some(true)) => LockState::Acquired,
        Ok(Some(false)) => LockState::HeldElsewhere,
        Ok(None) | Err(_) => LockState::Unsupported,
    }
}

/// Promesse d'une requête `IndexedDB` : résolue au succès, rejetée avec le
/// texte de l'erreur.
fn request_promise(request: &web_sys::IdbRequest) -> js_sys::Promise {
    js_sys::Promise::new(&mut |resolve, reject| {
        let success = Closure::once_into_js(move |_event: JsValue| {
            if resolve.call0(&JsValue::UNDEFINED).is_err() {
                log::warn!("IndexedDB : réponse perdue");
            }
        });
        let failed = request.clone();
        let failure = Closure::once_into_js(move |_event: JsValue| {
            let detail = match failed.error() {
                Ok(Some(error)) => format!("{} : {}", error.name(), error.message()),
                _ => "IndexedDB".to_owned(),
            };
            if reject
                .call1(&JsValue::UNDEFINED, &JsValue::from_str(&detail))
                .is_err()
            {
                log::warn!("IndexedDB : erreur perdue");
            }
        });
        request.set_onsuccess(Some(success.unchecked_ref()));
        request.set_onerror(Some(failure.unchecked_ref()));
    })
}

/// Ouvre la base (version 1), crée le magasin à la première ouverture.
async fn open_db() -> Result<web_sys::IdbDatabase, String> {
    let window = web_sys::window().ok_or_else(|| "window".to_owned())?;
    let factory = window
        .indexed_db()
        .map_err(|error| js_text(&error))?
        .ok_or_else(|| "IndexedDB".to_owned())?;
    let request = factory
        .open_with_u32(DB_NAME, 1)
        .map_err(|error| js_text(&error))?;
    let upgraded = request.clone();
    let upgrade = Closure::once_into_js(move |_event: JsValue| {
        let created = upgraded
            .result()
            .and_then(JsCast::dyn_into::<web_sys::IdbDatabase>)
            .and_then(|db| {
                if db.object_store_names().contains(STORE_NAME) {
                    Ok(())
                } else {
                    db.create_object_store(STORE_NAME).map(drop)
                }
            });
        if let Err(error) = created {
            log::warn!("IndexedDB : magasin non créé ({})", js_text(&error));
        }
    });
    request.set_onupgradeneeded(Some(upgrade.unchecked_ref()));
    let done = with_timeout(request_promise(&request))
        .await
        .map_err(|error| js_text(&error))?;
    if done.is_undefined() && request.ready_state() != web_sys::IdbRequestReadyState::Done {
        return Err("IndexedDB : délai dépassé".to_owned());
    }
    request
        .result()
        .and_then(JsCast::dyn_into::<web_sys::IdbDatabase>)
        .map_err(|error| js_text(&error))
}

/// Toutes les clés et valeurs de l'emplacement.
async fn read_all(db: &web_sys::IdbDatabase) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let read = || -> Result<(web_sys::IdbRequest, web_sys::IdbRequest), JsValue> {
        let tx = db.transaction_with_str(STORE_NAME)?;
        let store = tx.object_store(STORE_NAME)?;
        Ok((store.get_all_keys()?, store.get_all()?))
    };
    let (keys, values) = read().map_err(|error| js_text(&error))?;
    // Les deux écouteurs sont posés avant toute attente : la seconde
    // requête peut aboutir pendant qu'on attend la première.
    let keys_done = request_promise(&keys);
    let values_done = request_promise(&values);
    JsFuture::from(keys_done).await.map_err(|error| js_text(&error))?;
    JsFuture::from(values_done)
        .await
        .map_err(|error| js_text(&error))?;
    let keys = keys.result().map_err(|error| js_text(&error))?;
    let values = values.result().map_err(|error| js_text(&error))?;
    let keys = js_sys::Array::from(&keys);
    let values = js_sys::Array::from(&values);
    let mut entries = BTreeMap::new();
    for (key, value) in keys.iter().zip(values.iter()) {
        // Autre forme (clé non textuelle, valeur non binaire) : ignorée.
        if let (Some(key), Ok(value)) = (key.as_string(), value.dyn_into::<js_sys::Uint8Array>()) {
            entries.insert(key, value.to_vec());
        }
    }
    Ok(entries)
}

/// Écrivain `IndexedDB` : une transaction par lot, sans attente.
struct IdbWriter {
    db: web_sys::IdbDatabase,
    /// Premier échec (transaction annulée ou refusée).
    failure: Rc<RefCell<Option<String>>>,
    /// Écouteur `abort` commun à toutes les transactions (vit autant que
    /// la page).
    on_abort: js_sys::Function,
}

impl IdbWriter {
    fn new(db: web_sys::IdbDatabase) -> Self {
        let failure = Rc::new(RefCell::new(None));
        let seen = Rc::clone(&failure);
        let listener = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let detail = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::IdbTransaction>().ok())
                .and_then(|tx| tx.error())
                .map_or_else(
                    || "transaction IndexedDB annulée".to_owned(),
                    |error| format!("{} : {}", error.name(), error.message()),
                );
            let mut slot = seen.borrow_mut();
            if slot.is_none() {
                log::warn!("sauvegarde automatique : {detail}");
                *slot = Some(detail);
            }
        });
        let on_abort: js_sys::Function = listener.as_ref().unchecked_ref::<js_sys::Function>().clone();
        listener.forget();
        Self {
            db,
            failure,
            on_abort,
        }
    }

    /// Ouvre la transaction et y place les écritures ; une exception
    /// annule la transaction (rien d'écrit).
    fn try_submit(&self, ops: Vec<KvOp>) -> Result<(), JsValue> {
        let tx = self
            .db
            .transaction_with_str_and_mode(STORE_NAME, web_sys::IdbTransactionMode::Readwrite)?;
        tx.set_onabort(Some(&self.on_abort));
        let placed = (|| -> Result<(), JsValue> {
            let store = tx.object_store(STORE_NAME)?;
            for op in ops {
                match op {
                    KvOp::Put { key, value } => {
                        let bytes = js_sys::Uint8Array::from(value.as_slice());
                        store.put_with_key(bytes.as_ref(), &JsValue::from_str(&key))?;
                    }
                    KvOp::Delete(key) => {
                        store.delete(&JsValue::from_str(&key))?;
                    }
                    KvOp::DeleteRange { start, end } => {
                        let range = web_sys::IdbKeyRange::bound_with_lower_open_and_upper_open(
                            &JsValue::from_str(&start),
                            &JsValue::from_str(&end),
                            false,
                            true,
                        )?;
                        store.delete(range.as_ref())?;
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = placed {
            if tx.abort().is_err() {
                log::warn!("sauvegarde automatique : transaction non annulée");
            }
            return Err(error);
        }
        Ok(())
    }
}

impl SlotWriter for IdbWriter {
    fn submit(&mut self, ops: Vec<KvOp>) {
        if self.failure.borrow().is_some() {
            return;
        }
        if let Err(error) = self.try_submit(ops) {
            let mut slot = self.failure.borrow_mut();
            if slot.is_none() {
                *slot = Some(js_text(&error));
            }
        }
    }

    fn failure(&self) -> Option<String> {
        self.failure.borrow().clone()
    }
}
