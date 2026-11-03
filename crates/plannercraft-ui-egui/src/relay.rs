//! Relais des appels d'un agent côté application (J7, ADR-0015 D9 à D11 ;
//! lot L3) : file des [`RelayCall`] vidée par le réducteur en début
//! d'image, exécution par [`call_with`] sur la session du document,
//! validation de toute écriture par le bandeau (H-UI-27), `app.ui_state`
//! et `app.ui`.
//!
//! Déroulé d'une écriture (C-CTL-02 à C-CTL-04, C-CTL-07) :
//! 1. `dry_run` du même appel ([`call_with`]) : refus ⇒ erreur rendue
//!    aussitôt ; sinon `pending_approval`, et le résumé du diff
//!    ([`ApprovalSummary`]) s'affiche dans le bandeau ;
//! 2. Accepter : appel expiré ([`CallState::claim`] faux) ⇒ rien ; document
//!    remplacé ⇒ `control.rejected` ; génération changée depuis l'aperçu ⇒
//!    nouvel aperçu, rien d'appliqué ; sinon l'appel est exécuté avec
//!    l'origine `Agent` et le « maintenant » de la demande, puis `ok` ;
//! 3. Refuser ⇒ `control.rejected` ; projet, historique, génération et
//!    drapeau modifié intacts ;
//! 4. délai de l'écouteur, ou client parti ⇒ `Cancel(id)` portant l'état
//!    de l'appel abandonné (ou appel vu expiré) : ce bandeau-là retiré.
//!
//! Lectures (genres `Read`, `Analysis`, `history.list`) servies aussitôt,
//! même JSON que `Workspace::call` (C-CTL-09). `project.open`,
//! `project.new`, `project.import_csv` : `control.not_relayable`
//! (C-CTL-05). Le web n'a pas de canal : ce module n'y est pas compilé.
//!
//! Panneau d'opérations IA (J8 lot L12, H-UI-41) : à l'arrivée d'une
//! écriture, le diff affiché ([`DiffView`], au plus 200 lignes) est calculé
//! **une fois** sur l'état simulé du même lot ([`Session::simulate`], diff
//! détaillé) et gardé dans [`PendingApproval::diff`] ; le volet latéral
//! s'ouvre sur l'onglet « Opérations IA ». Chaque issue (acceptée, refusée,
//! expirée, périmée, échouée) entre dans l'historique de session
//! ([`Relay::history`], 50 entrées au plus).
//!
//! Capture (`app.screenshot`, C-CTL-08, lot L14) : permise seulement si
//! l'application l'active ([`App::enable_screenshots`], desktop) ; sinon,
//! et sous kittest, `control.screenshot_unavailable` aussitôt.
//! `ViewportCommand::Screenshot` est envoyée pendant la vidange de la file ;
//! un fil `plannercraft-screenshot` attend l'image ([`ScreenshotConfig::timeout`],
//! 5 s), la réduit de moitié `scale_down` fois, l'encode en PNG (crate
//! `image`) puis en base64 ([`encode_screenshot`]) et répond lui-même : le
//! fil d'interface n'encode jamais.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Duration;

use base64::Engine as _;
use plannercraft_automation::command::params::ApplyBatchParams;
use plannercraft_automation::command::params::{
    ScreenshotParams, ThemeJson, UiActionParams, UiLanguageJson, ViewJson, ZoomJson,
};
use plannercraft_automation::control::{CallState, RelayCall, RelayKind, Reply};
use plannercraft_automation::diff::{DiffSide, DiffView};
use plannercraft_automation::error::codes;
use plannercraft_automation::registry::{Kind, lookup};
use plannercraft_automation::text::{Language as ToolLanguage, TextContext};
use plannercraft_automation::workspace::relayable;
use plannercraft_automation::{
    ApplyOptions, Batch, CallEnv, Command, EditSource, Patch, Session, ToolError, call_with,
};
use plannercraft_core::{Moment, TaskUid};
use serde_json::{Map, Value, json};

use crate::action::{Action, UiOp};
use crate::app::{App, DocOrigin};
use crate::geom;
use crate::state::{LanguagePref, ThemePref, ViewKind};
use crate::timescale::TimeScaleZoom;
use crate::view::side_panel::{AiHistoryEntry, AiOutcome, AiPanelInput, SideOp, SideTab, push_history};
use crate::view::{ApprovalBanner, ApprovalSummary};

/// Écritures qui demandent une validation sans être du genre `Write`.
const APPROVED_TOOLS: [&str; 5] = [
    "history.redo",
    "history.undo",
    "project.export_mspdi",
    "project.export_pdf",
    "project.save",
];

/// Écriture relayée en attente de la réponse de l'utilisateur.
#[derive(Debug)]
pub struct PendingApproval {
    /// Identifiant de la requête de l'agent.
    pub id: u64,
    /// Outil demandé.
    pub tool: String,
    /// Paramètres de l'appel.
    pub params: Value,
    /// Résumé du `dry_run` (`None` : appel sans aperçu).
    pub preview: Option<ApprovalSummary>,
    /// Diff affiché par le panneau d'opérations IA (H-UI-41), calculé une
    /// fois à l'arrivée (et à chaque nouvel aperçu) ; `None` : appel sans
    /// aperçu, ou lot non simulable.
    pub diff: Option<DiffView>,
    /// Nom du document visé à l'arrivée (historique du panneau).
    pub document: String,
    /// Génération de la session à l'aperçu.
    pub generation: u64,
    /// Numéro d'ouverture du document visé.
    pub opening: u64,
    /// « Maintenant » de la demande, repris à l'acceptation (règle 5).
    pub now: Moment,
    /// État partagé avec l'écouteur.
    state: Arc<CallState>,
    /// Réponse finale.
    reply: SyncSender<Reply>,
}

impl PendingApproval {
    /// Chemin visé par un enregistrement ou un export.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        self.params.get("path").and_then(Value::as_str)
    }
}

/// Plafond d'une capture rendue (C-CTL-08) : 6 Mio de texte base64, soit
/// une ligne de réponse sous `MAX_LINE_BYTES` (8 Mio) du canal.
pub const SCREENSHOT_MAX_BYTES: usize = 6 * 1024 * 1024;

/// Attente de l'image après la demande de capture (C-CTL-08).
pub const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(5);

/// Réductions de moitié demandées au plus (`ScreenshotParams::scale_down`).
pub const MAX_SCALE_DOWN: u8 = 2;

/// Réductions de moitié ajoutées au plus quand la capture dépasse le
/// plafond (C-CTL-08).
pub const MAX_AUTO_SCALE_DOWN: u8 = 2;

/// Réglages de la capture d'écran relayée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenshotConfig {
    /// Attente de l'image au plus.
    pub timeout: Duration,
    /// Plafond du texte base64 rendu, en octets.
    pub max_bytes: usize,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            timeout: SCREENSHOT_TIMEOUT,
            max_bytes: SCREENSHOT_MAX_BYTES,
        }
    }
}

/// Relais de l'application : file des appels et écritures en attente.
#[derive(Debug)]
pub struct Relay {
    /// File remplie par l'écouteur.
    inbox: Receiver<RelayCall>,
    /// Écritures en attente, la première au bandeau.
    pending: VecDeque<PendingApproval>,
    /// Dossier des rendez-vous (garde ADR-0002 d'un enregistrement relayé) ;
    /// `None` : celui de la plateforme.
    control_dir: Option<PathBuf>,
    /// Capture permise (desktop) ; `None` : `control.screenshot_unavailable`.
    screenshots: Option<ScreenshotConfig>,
    /// Historique de session du panneau d'opérations IA (H-UI-41), le
    /// plus ancien d'abord, 50 entrées au plus.
    history: VecDeque<AiHistoryEntry>,
    /// Diffs affichés calculés depuis le branchement (H-UI-41 : un par
    /// aperçu, jamais par image).
    diff_builds: u64,
}

impl Relay {
    /// Écritures en attente, la première au bandeau.
    #[must_use]
    pub const fn pending(&self) -> &VecDeque<PendingApproval> {
        &self.pending
    }

    /// Historique de session du panneau (le plus ancien d'abord).
    #[must_use]
    pub const fn history(&self) -> &VecDeque<AiHistoryEntry> {
        &self.history
    }

    /// Diffs affichés calculés depuis le branchement du relais.
    #[must_use]
    pub const fn diff_builds(&self) -> u64 {
        self.diff_builds
    }

    /// Inscrit l'issue de `pending` dans l'historique.
    fn record(&mut self, pending: &PendingApproval, outcome: AiOutcome, undo: Option<(u64, u64)>) {
        push_history(
            &mut self.history,
            AiHistoryEntry {
                tool: pending.tool.clone(),
                document: pending.document.clone(),
                at: pending.now,
                outcome,
                undo,
            },
        );
    }

    /// Retire l'écriture affichée au bandeau (tête de file) si c'est
    /// l'appel `id` : Accepter et Refuser ne visent que ce qui est affiché
    /// (deux agents peuvent avoir des appels de même `id`, relecture finale
    /// J7, relais F3).
    fn take_front(&mut self, id: u64) -> Option<PendingApproval> {
        if self.pending.front().is_some_and(|p| p.id == id) {
            self.pending.pop_front()
        } else {
            None
        }
    }

    /// Retire l'appel abandonné par l'écouteur : même `id` **et** même
    /// état partagé (celui que porte le `Cancel`), jamais l'appel d'un
    /// autre agent au même `id`.
    fn take_cancelled(&mut self, id: u64, state: &Arc<CallState>) -> Option<PendingApproval> {
        let pos = self
            .pending
            .iter()
            .position(|p| p.id == id && Arc::ptr_eq(&p.state, state))?;
        self.pending.remove(pos)
    }
}

/// Réponse d'erreur d'outil.
fn tool_reply(id: u64, error: &ToolError) -> Reply {
    Reply::tool_error(id, error)
}

/// Envoie une réponse ; un écouteur parti n'est pas une erreur de
/// l'application.
fn send(reply: &SyncSender<Reply>, value: Reply) {
    if reply.try_send(value).is_err() {
        log::debug!("réponse du relais non remise : écouteur parti");
    }
}

/// Résumé d'un `dry_run` (JSON d'`ApplyOutcome`), sans calcul.
#[must_use]
pub fn summary_of(outcome: &Value) -> ApprovalSummary {
    let at = |path: &str| outcome.pointer(path);
    let len = |path: &str| at(path).and_then(Value::as_array).map_or(0, Vec::len);
    ApprovalSummary {
        changed_tasks: at("/diff/changed_total")
            .and_then(Value::as_u64)
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX)),
        finish_delta: at("/diff/finish_delta/minutes").and_then(Value::as_i64),
        finish_delta_text: at("/diff/finish_delta/text")
            .and_then(Value::as_str)
            .map(str::to_owned),
        critical_changed: len("/diff/critical_entered") > 0 || len("/diff/critical_left") > 0,
        warnings: u32::try_from(len("/warnings")).unwrap_or(u32::MAX),
    }
}

/// Erreur d'outil simple.
fn error(code: &'static str, message: &str) -> ToolError {
    ToolError::simple(code, message.to_owned())
}

/// Vrai si l'appel demande une validation : écriture (hors `dry_run`),
/// enregistrement, export, annuler, rétablir.
fn needs_approval(tool: &str, params: &Value) -> bool {
    let write = lookup(tool).is_some_and(|spec| spec.kind == Kind::Write) || APPROVED_TOOLS.contains(&tool);
    write && params.get("dry_run").and_then(Value::as_bool) != Some(true)
}

/// Paramètres d'un `dry_run` (objet, `dry_run: true`), si l'appel en a un.
fn dry_params(tool: &str, params: &Value) -> Option<Value> {
    let batch = tool == "batch.apply" || lookup(tool).is_some_and(|spec| spec.kind == Kind::Write);
    if !batch {
        return None;
    }
    let mut map = match params {
        Value::Null => Map::new(),
        Value::Object(map) => map.clone(),
        // Mal formé : le `dry_run` le refusera tel quel.
        other => return Some(other.clone()),
    };
    map.insert("dry_run".to_owned(), Value::Bool(true));
    Some(Value::Object(map))
}

/// Actions d'interface d'`app.ui`, dans l'ordre du schéma.
///
/// # Errors
///
/// `param.invalid` (paramètres mal formés, aucun champ).
pub fn ui_ops(params: Value) -> Result<Vec<Action>, ToolError> {
    let p: UiActionParams = serde_json::from_value(params).map_err(|e| {
        ToolError::simple(
            codes::PARAM_INVALID,
            format!("paramètres de app.ui invalides : {e}"),
        )
    })?;
    let mut ops = Vec::new();
    match p.select {
        Patch::Keep => {}
        Patch::Set(uid) => ops.push(Action::Ui(UiOp::Select(Some(uid)))),
        Patch::Clear => ops.push(Action::Ui(UiOp::Select(None))),
    }
    if let Some(uid) = p.scroll_to {
        ops.push(Action::Ui(UiOp::ScrollTo(uid)));
    }
    if let Some(zoom) = p.zoom {
        ops.push(match zoom {
            ZoomJson::Auto => Action::FitZoom,
            ZoomJson::Day => Action::Ui(UiOp::Zoom(TimeScaleZoom::Day)),
            ZoomJson::Week => Action::Ui(UiOp::Zoom(TimeScaleZoom::Week)),
            ZoomJson::Month => Action::Ui(UiOp::Zoom(TimeScaleZoom::Month)),
            ZoomJson::Quarter => Action::Ui(UiOp::Zoom(TimeScaleZoom::Quarter)),
        });
    }
    if let Some(v) = p.view {
        ops.push(Action::Ui(UiOp::View(match v {
            ViewJson::Gantt => ViewKind::Gantt,
            ViewJson::ResourceSheet => ViewKind::ResourceSheet,
            ViewJson::ResourceUsage => ViewKind::ResourceUsage,
        })));
    }
    if let Some(t) = p.theme {
        ops.push(Action::Ui(UiOp::Theme(match t {
            ThemeJson::System => ThemePref::System,
            ThemeJson::Light => ThemePref::Light,
            ThemeJson::Dark => ThemePref::Dark,
        })));
    }
    if let Some(l) = p.language {
        ops.push(Action::Ui(UiOp::Language(match l {
            UiLanguageJson::System => LanguagePref::System,
            UiLanguageJson::Fr => LanguagePref::Fr,
            UiLanguageJson::En => LanguagePref::En,
        })));
    }
    if ops.is_empty() {
        return Err(ToolError::simple(
            codes::PARAM_INVALID,
            "app.ui : au moins un champ attendu".to_owned(),
        ));
    }
    Ok(ops)
}

impl App {
    /// Branche la file des appels relayés (desktop : écouteur du canal).
    pub fn control_inbox(&mut self, inbox: Receiver<RelayCall>) {
        self.relay = Some(Relay {
            inbox,
            pending: VecDeque::new(),
            control_dir: None,
            screenshots: None,
            history: VecDeque::new(),
            diff_builds: 0,
        });
    }

    /// Permet la capture de la fenêtre (`app.screenshot`) : desktop
    /// seulement, après [`App::control_inbox`] (sans relais, rien).
    pub fn enable_screenshots(&mut self, config: ScreenshotConfig) {
        if let Some(relay) = &mut self.relay {
            relay.screenshots = Some(config);
        }
    }

    /// Dossier des rendez-vous lu par la garde d'un enregistrement relayé
    /// (tests ; défaut : celui de la plateforme).
    pub fn set_control_dir(&mut self, dir: PathBuf) {
        if let Some(relay) = &mut self.relay {
            relay.control_dir = Some(dir);
        }
    }

    /// Relais branché, avec ses écritures en attente.
    #[must_use]
    pub const fn relay(&self) -> Option<&Relay> {
        self.relay.as_ref()
    }

    /// Environnement d'un appel relayé (« maintenant » lu maintenant).
    fn call_env(&self, now: Moment, source: EditSource) -> CallEnv {
        CallEnv {
            source,
            ..CallEnv::new(now, self.text_language())
        }
    }

    /// Vide la file des appels relayés (début d'image) : sert les
    /// lectures, met les écritures en attente, retire les appels expirés
    /// et ceux d'un document remplacé.
    pub(crate) fn drain_relay(&mut self, ctx: &egui::Context) {
        let Some(relay) = &self.relay else {
            return;
        };
        let calls: Vec<RelayCall> = relay.inbox.try_iter().collect();
        for call in calls {
            self.serve_call(call, ctx);
        }
        let opening = self.document.as_ref().map(|document| document.opening);
        let Some(relay) = &mut self.relay else {
            return;
        };
        let mut kept = VecDeque::new();
        let drained: Vec<PendingApproval> = relay.pending.drain(..).collect();
        for pending in drained {
            if !pending.state.is_pending() {
                relay.record(&pending, AiOutcome::Expired, None);
                continue;
            }
            if Some(pending.opening) == opening {
                kept.push_back(pending);
            } else if pending.state.claim() {
                let e = error(
                    codes::CONTROL_REJECTED,
                    "document fermé ou remplacé : rien n'a été appliqué",
                );
                send(&pending.reply, tool_reply(pending.id, &e));
                relay.record(&pending, AiOutcome::Stale, None);
            }
        }
        relay.pending = kept;
    }

    /// Sert un appel relayé.
    fn serve_call(&mut self, call: RelayCall, ctx: &egui::Context) {
        let RelayCall {
            id,
            kind,
            state,
            reply,
        } = call;
        if let RelayKind::Cancel(target) = kind {
            if let Some(relay) = &mut self.relay
                && let Some(cancelled) = relay.take_cancelled(target, &state)
            {
                relay.record(&cancelled, AiOutcome::Expired, None);
            }
            return;
        }
        if !state.is_pending() {
            return;
        }
        match kind {
            RelayKind::UiState => self.reply_now(&state, &reply, id, Ok(self.ui_state_json())),
            RelayKind::Screenshot { scale_down } => self.screenshot(ctx, scale_down, &state, &reply, id),
            RelayKind::Execute { tool, params } => self.serve_execute(id, tool, params, state, reply, ctx),
            RelayKind::Cancel(_) => {}
        }
    }

    /// Réponse immédiate d'un appel pris maintenant.
    #[allow(clippy::unused_self, reason = "méthode du relais, comme les autres réponses")]
    fn reply_now(
        &self,
        state: &CallState,
        reply: &SyncSender<Reply>,
        id: u64,
        result: Result<Value, ToolError>,
    ) {
        if !state.claim() {
            return;
        }
        send(
            reply,
            match result {
                Ok(result) => Reply::Ok { id, result },
                Err(e) => tool_reply(id, &e),
            },
        );
    }

    /// Capture de la fenêtre (C-CTL-08) : demandée au contexte egui, puis
    /// attendue, réduite et encodée par un fil dédié qui répond lui-même.
    fn screenshot(
        &self,
        ctx: &egui::Context,
        scale_down: u8,
        state: &Arc<CallState>,
        reply: &SyncSender<Reply>,
        id: u64,
    ) {
        if scale_down > MAX_SCALE_DOWN {
            let e = ToolError::simple(
                codes::PARAM_INVALID,
                format!("app.screenshot : scale_down vaut {MAX_SCALE_DOWN} au plus"),
            )
            .with_field("scale_down");
            return self.reply_now(state, reply, id, Err(e));
        }
        let Some(config) = self.relay.as_ref().and_then(|relay| relay.screenshots) else {
            let e = error(
                codes::CONTROL_SCREENSHOT_UNAVAILABLE,
                "capture de la fenêtre indisponible dans cet environnement",
            );
            return self.reply_now(state, reply, id, Err(e));
        };
        let (image_tx, image_rx) = std::sync::mpsc::channel::<Arc<egui::ColorImage>>();
        let thread_state = Arc::clone(state);
        let thread_reply = reply.clone();
        let spawned = std::thread::Builder::new()
            .name("plannercraft-screenshot".to_owned())
            .spawn(move || {
                // Délai écoulé ou demande abandonnée (callback détruit sans
                // image) : indisponible.
                let result = match image_rx.recv_timeout(config.timeout) {
                    Ok(image) => encode_screenshot(&image, scale_down, config.max_bytes),
                    Err(_) => Err(error(
                        codes::CONTROL_SCREENSHOT_UNAVAILABLE,
                        "aucune image de la fenêtre dans le délai (fenêtre masquée ?)",
                    )),
                };
                if thread_state.claim() {
                    send(
                        &thread_reply,
                        match result {
                            Ok(result) => Reply::Ok { id, result },
                            Err(e) => tool_reply(id, &e),
                        },
                    );
                }
            });
        if let Err(spawn_error) = spawned {
            let e = ToolError::simple(
                codes::CONTROL_SCREENSHOT_UNAVAILABLE,
                format!("capture impossible : {spawn_error}"),
            );
            return self.reply_now(state, reply, id, Err(e));
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(
            move |image| {
                if image_tx.send(image).is_err() {
                    log::debug!("capture arrivée après le délai : ignorée");
                }
            },
        )));
    }

    /// Sert `execute { tool, params }`.
    fn serve_execute(
        &mut self,
        id: u64,
        tool: String,
        params: Value,
        state: Arc<CallState>,
        reply: SyncSender<Reply>,
        ctx: &egui::Context,
    ) {
        match tool.as_str() {
            "app.ui_state" => return self.reply_now(&state, &reply, id, Ok(self.ui_state_json())),
            "app.screenshot" => {
                return match screenshot_params(params) {
                    Ok(scale_down) => self.screenshot(ctx, scale_down, &state, &reply, id),
                    Err(e) => self.reply_now(&state, &reply, id, Err(e)),
                };
            }
            "app.ui" => {
                let result = self.queue_ui(params);
                return self.reply_now(&state, &reply, id, result);
            }
            _ => {}
        }
        let now = (self.env.now)();
        let env = self.call_env(now, EditSource::User);
        if let Err(e) = relayable(&tool, env.lang) {
            // Refus du routeur lui-même (id inconnu, ouverture), avec ou
            // sans document : même erreur que `call_with`.
            return self.reply_now(&state, &reply, id, Err(e));
        }
        let Some(document) = &mut self.document else {
            let e = error(codes::SESSION_NONE, "aucun projet ouvert dans l'application");
            return self.reply_now(&state, &reply, id, Err(e));
        };
        if !needs_approval(&tool, &params) {
            if !state.claim() {
                return;
            }
            let result = call_with(&mut document.session, &env, &tool, params);
            send(
                &reply,
                match result {
                    Ok(result) => Reply::Ok { id, result },
                    Err(e) => tool_reply(id, &e),
                },
            );
            return;
        }
        if let Err(e) = self.guard_save_path(&tool, &params) {
            return self.reply_now(&state, &reply, id, Err(e));
        }
        let Some(document) = &mut self.document else {
            return;
        };
        let preview = match dry_params(&tool, &params) {
            None => None,
            Some(dry) => match call_with(&mut document.session, &env, &tool, dry) {
                Ok(outcome) => Some(summary_of(&outcome)),
                Err(e) => return self.reply_now(&state, &reply, id, Err(e)),
            },
        };
        let diff = preview
            .as_ref()
            .and_then(|_| preview_diff(&document.session, &tool, &params, env.lang));
        let pending = PendingApproval {
            id,
            tool,
            params,
            preview,
            diff,
            document: document.origin.name.clone(),
            generation: document.generation(),
            opening: document.opening,
            now,
            state,
            reply,
        };
        send(&pending.reply, Reply::PendingApproval { id });
        if let Some(relay) = &mut self.relay {
            if pending.preview.is_some() {
                relay.diff_builds = relay.diff_builds.saturating_add(1);
            }
            relay.pending.push_back(pending);
        }
        // Volet ouvert d'office à chaque demande (H-UI-41), sans prendre le
        // focus clavier.
        self.side.apply(SideOp::Open(SideTab::AiOperations));
    }

    /// Garde ADR-0002 d'un enregistrement relayé vers un chemin choisi par
    /// l'agent : refus si une autre application tient ce fichier.
    fn guard_save_path(&self, tool: &str, params: &Value) -> Result<(), ToolError> {
        if !matches!(
            tool,
            "project.save" | "project.export_mspdi" | "project.export_pdf"
        ) {
            return Ok(());
        }
        let Some(path) = params.get("path").and_then(Value::as_str) else {
            return Ok(());
        };
        let path = std::path::Path::new(path);
        let dir = self
            .relay
            .as_ref()
            .and_then(|relay| relay.control_dir.clone())
            .or_else(plannercraft_automation::control::state_dir);
        let held = dir
            .and_then(|dir| plannercraft_automation::control::find_live_for_in(&dir, path))
            .is_some_and(|r| r.pid != std::process::id());
        if held {
            return Err(ToolError::simple(
                codes::FILE_OPEN_IN_APP,
                format!("fichier tenu par une autre application : {}", path.display()),
            )
            .with_field("path"));
        }
        Ok(())
    }

    /// `app.ui` : actions mises en file (appliquées en fin d'image).
    fn queue_ui(&mut self, params: Value) -> Result<Value, ToolError> {
        let ops = ui_ops(params)?;
        for op in &ops {
            let uid = match op {
                Action::Ui(UiOp::Select(Some(uid)) | UiOp::ScrollTo(uid)) => Some(*uid),
                _ => None,
            };
            if let Some(uid) = uid {
                let known = self
                    .rows
                    .as_ref()
                    .is_some_and(|rows| rows.index_of(TaskUid::new(uid)).is_some());
                if !known {
                    return Err(ToolError::simple(
                        codes::TASK_NOT_FOUND,
                        format!("tâche {uid} absente de la vue"),
                    ));
                }
            }
        }
        let applied = ops.len();
        self.actions.extend(ops);
        Ok(json!({ "applied": applied }))
    }

    /// `app.ui_state` : préférences, sélection, vue, lignes visibles,
    /// document modifié, génération, numéro d'ouverture. Les fichiers récents
    /// (`recent`) ne sont **pas** rendus : chemins et noms d'autres projets
    /// de l'utilisateur, sans rapport avec la session de l'agent (relecture
    /// finale J7, relais F7).
    #[must_use]
    pub fn ui_state_json(&self) -> Value {
        let mut ui_state = self
            .state
            .to_json()
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .unwrap_or(Value::Null);
        if let Value::Object(map) = &mut ui_state {
            map.remove("recent");
        }
        let mut selection: Vec<u32> = self.selected.click_order().iter().map(|uid| uid.get()).collect();
        selection.sort_unstable();
        // Lignes affichées (J8, lot L5 : la vue filtrée, triée ou repliée).
        let visible = self.visible_rows().map(|rows| {
            let top = self.view.last_scroll_y;
            let range =
                geom::visible_rows(top, top + self.view.last_body_height, self.row_h(), rows.len(), 0);
            json!([range.start, range.end.saturating_sub(1).max(range.start)])
        });
        json!({
            "ui_state": ui_state,
            "selection": selection,
            "active": self.selection.map(TaskUid::get),
            "view": self.state.view,
            "visible_rows": visible,
            "dirty": self.document.as_ref().is_some_and(crate::app::OpenDocument::is_dirty),
            "generation": self.document.as_ref().map(crate::app::OpenDocument::generation),
            "opening": self.document.as_ref().map(|document| document.opening),
        })
    }

    /// Accepter (C-CTL-02, C-CTL-07) : voir l'en-tête du module.
    pub(crate) fn approve(&mut self, id: u64) {
        let Some(relay) = &mut self.relay else {
            return;
        };
        let Some(mut pending) = relay.take_front(id) else {
            return;
        };
        if !pending.state.is_pending() {
            relay.record(&pending, AiOutcome::Expired, None);
            return;
        }
        let lang = self.text_language();
        let Some(document) = &mut self.document else {
            return self.reject_pending(
                &pending,
                "aucun document : rien n'a été appliqué",
                AiOutcome::Stale,
            );
        };
        if document.opening != pending.opening {
            return self.reject_pending(
                &pending,
                "document fermé ou remplacé : rien n'a été appliqué",
                AiOutcome::Stale,
            );
        }
        if document.generation() != pending.generation {
            // Aperçu périmé : nouvel aperçu (diff affiché compris), rien
            // d'appliqué.
            let env = CallEnv::new(pending.now, lang);
            if let Some(dry) = dry_params(&pending.tool, &pending.params) {
                match call_with(&mut document.session, &env, &pending.tool, dry) {
                    Ok(outcome) => {
                        pending.preview = Some(summary_of(&outcome));
                        pending.diff = preview_diff(&document.session, &pending.tool, &pending.params, lang);
                    }
                    Err(e) => {
                        if pending.state.claim() {
                            send(&pending.reply, tool_reply(pending.id, &e));
                            if let Some(relay) = &mut self.relay {
                                relay.record(&pending, AiOutcome::Failed(e.code.to_owned()), None);
                            }
                        }
                        return;
                    }
                }
            }
            pending.generation = document.generation();
            if let Some(relay) = &mut self.relay {
                if pending.preview.is_some() {
                    relay.diff_builds = relay.diff_builds.saturating_add(1);
                }
                relay.pending.push_front(pending);
            }
            return;
        }
        if !pending.state.claim() {
            if let Some(relay) = &mut self.relay {
                relay.record(&pending, AiOutcome::Expired, None);
            }
            return;
        }
        let env = CallEnv {
            source: EditSource::Agent,
            ..CallEnv::new(pending.now, lang)
        };
        let before = document.generation();
        let result = call_with(&mut document.session, &env, &pending.tool, pending.params.clone());
        let changed = document.generation() != before;
        let undo = changed.then(|| (document.opening, document.session.history_position()));
        let outcome = match &result {
            Ok(_) => AiOutcome::Accepted,
            Err(e) => AiOutcome::Failed(e.code.to_owned()),
        };
        let origin = document.session.origin().clone();
        if origin.path != document.origin.path {
            document.origin = DocOrigin {
                name: origin.name,
                path: origin.path,
                demo: false,
            };
        }
        send(
            &pending.reply,
            match result {
                Ok(result) => Reply::Ok { id, result },
                Err(e) => tool_reply(id, &e),
            },
        );
        if let Some(relay) = &mut self.relay {
            relay.record(&pending, outcome, undo);
        }
        if changed {
            self.refresh();
        } else {
            self.notify_observer();
        }
        self.after_banner();
    }

    /// Refuser (C-CTL-03) : rien n'est appliqué.
    pub(crate) fn reject(&mut self, id: u64) {
        let Some(relay) = &mut self.relay else {
            return;
        };
        let Some(pending) = relay.take_front(id) else {
            return;
        };
        self.reject_pending(
            &pending,
            "refusé par l'utilisateur : rien n'a été appliqué",
            AiOutcome::Rejected,
        );
    }

    /// Répond `control.rejected` à une écriture en attente ; l'issue
    /// `outcome` entre dans l'historique (expirée si l'appel l'était déjà).
    fn reject_pending(&mut self, pending: &PendingApproval, message: &str, outcome: AiOutcome) {
        let outcome = if pending.state.claim() {
            send(
                &pending.reply,
                tool_reply(pending.id, &error(codes::CONTROL_REJECTED, message)),
            );
            outcome
        } else {
            AiOutcome::Expired
        };
        if let Some(relay) = &mut self.relay {
            relay.record(pending, outcome, None);
        }
        self.after_banner();
    }

    /// Plus aucun bandeau : le focus clavier revient à la table.
    fn after_banner(&mut self) {
        let empty = self.relay.as_ref().is_none_or(|relay| relay.pending.is_empty());
        if empty && self.document.is_some() {
            self.view.focus_table_pending = true;
        }
    }
}

/// `scale_down` des paramètres d'`app.screenshot` (absent : 0).
fn screenshot_params(params: Value) -> Result<u8, ToolError> {
    let params = if params.is_null() { json!({}) } else { params };
    let p: ScreenshotParams = serde_json::from_value(params).map_err(|e| {
        ToolError::simple(
            codes::PARAM_INVALID,
            format!("paramètres de app.screenshot invalides : {e}"),
        )
    })?;
    Ok(p.scale_down.unwrap_or(0))
}

/// Pixels RGBA non prémultipliés d'une image egui, avec sa largeur et sa
/// hauteur ; `None` si la taille ne correspond pas aux pixels.
fn rgba_of(image: &egui::ColorImage) -> Option<(Vec<u8>, usize, usize)> {
    let [width, height] = image.size;
    if width.checked_mul(height)? != image.pixels.len() || width == 0 || height == 0 {
        return None;
    }
    let bytes = image
        .pixels
        .iter()
        .flat_map(egui::Color32::to_srgba_unmultiplied)
        .collect();
    Some((bytes, width, height))
}

/// Réduction de moitié (moyenne entière de blocs de 2 × 2, bord impair
/// moyenné sur ce qui existe) ; au moins un pixel de côté.
fn halve(rgba: &[u8], width: usize, height: usize) -> (Vec<u8>, usize, usize) {
    let (w, h) = ((width / 2).max(1), (height / 2).max(1));
    let mut out = Vec::with_capacity(w.saturating_mul(h).saturating_mul(4));
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = (
                    x.saturating_mul(2).saturating_add(dx),
                    y.saturating_mul(2).saturating_add(dy),
                );
                if sx >= width || sy >= height {
                    continue;
                }
                let at = sy.saturating_mul(width).saturating_add(sx).saturating_mul(4);
                let Some(px) = rgba.get(at..at.saturating_add(4)) else {
                    continue;
                };
                for (total, &v) in sum.iter_mut().zip(px) {
                    *total = total.saturating_add(u32::from(v));
                }
                count = count.saturating_add(1);
            }
            let count = std::num::NonZeroU32::new(count).unwrap_or(std::num::NonZeroU32::MIN);
            out.extend(sum.map(|total| u8::try_from(total / count).unwrap_or(u8::MAX)));
        }
    }
    (out, w, h)
}

/// PNG de pixels RGBA.
fn png_of(rgba: &[u8], width: usize, height: usize) -> Result<Vec<u8>, String> {
    use image::ImageEncoder as _;
    let (Ok(w), Ok(h)) = (u32::try_from(width), u32::try_from(height)) else {
        return Err("image trop grande".to_owned());
    };
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(png)
}

/// Longueur du texte base64 (avec remplissage) de `n` octets.
const fn base64_len(n: usize) -> usize {
    n.div_ceil(3).saturating_mul(4)
}

/// Capture rendue au MCP (C-CTL-08) : `image` réduite de moitié
/// `scale_down` fois, encodée en PNG puis en base64 ; tant que le texte
/// dépasse `max_bytes`, réduite encore de moitié ([`MAX_AUTO_SCALE_DOWN`]
/// fois au plus). Résultat : `{ mime_type, data, width, height, scale_down,
/// bytes }` (`scale_down` : réductions faites au total, `bytes` : taille du
/// PNG).
///
/// # Errors
///
/// `control.screenshot_too_large` si le texte dépasse encore `max_bytes`
/// après les réductions ; `control.screenshot_unavailable` si l'image est
/// vide ou incohérente, ou si l'encodage échoue.
pub fn encode_screenshot(
    image: &egui::ColorImage,
    scale_down: u8,
    max_bytes: usize,
) -> Result<Value, ToolError> {
    let unavailable = |detail: &str| {
        ToolError::simple(
            codes::CONTROL_SCREENSHOT_UNAVAILABLE,
            format!("capture inutilisable : {detail}"),
        )
    };
    let (mut rgba, mut width, mut height) = rgba_of(image).ok_or_else(|| unavailable("image vide"))?;
    let mut halvings = 0u8;
    for _ in 0..scale_down {
        (rgba, width, height) = halve(&rgba, width, height);
        halvings = halvings.saturating_add(1);
    }
    let mut extra = 0u8;
    loop {
        let png = png_of(&rgba, width, height).map_err(|e| unavailable(&e))?;
        if base64_len(png.len()) <= max_bytes {
            return Ok(json!({
                "mime_type": "image/png",
                "data": base64::engine::general_purpose::STANDARD.encode(&png),
                "width": width,
                "height": height,
                "scale_down": halvings,
                "bytes": png.len(),
            }));
        }
        if extra >= MAX_AUTO_SCALE_DOWN {
            return Err(ToolError::simple(
                codes::CONTROL_SCREENSHOT_TOO_LARGE,
                format!(
                    "capture trop grande : {} octets en base64 après {halvings} réductions (plafond {max_bytes})",
                    base64_len(png.len())
                ),
            ));
        }
        (rgba, width, height) = halve(&rgba, width, height);
        halvings = halvings.saturating_add(1);
        extra = extra.saturating_add(1);
    }
}

/// Lot d'un appel d'écriture relayé (`batch.apply` ou une commande du
/// genre `Write`), lu comme le routeur de `call_with` le lit ; `None` pour
/// tout autre appel ou des paramètres mal formés (le `dry_run` les a déjà
/// refusés).
fn relayed_batch(tool: &str, params: &Value) -> Option<Batch> {
    let mut map = match params {
        Value::Object(map) => map.clone(),
        Value::Null => Map::new(),
        _ => return None,
    };
    map.remove("dry_run");
    map.remove("detail");
    if tool == "batch.apply" {
        let p: ApplyBatchParams = serde_json::from_value(Value::Object(map)).ok()?;
        return Some(p.into_batch());
    }
    if !lookup(tool).is_some_and(|spec| spec.kind == Kind::Write) || map.contains_key("op") {
        return None;
    }
    map.insert("op".to_owned(), Value::String(tool.to_owned()));
    let op: Command = serde_json::from_value(Value::Object(map)).ok()?;
    Some(Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: None,
        ops: vec![op],
        label: None,
    })
}

/// Diff affiché d'une écriture proposée (H-UI-41) : le même lot simulé sur
/// une copie ([`Session::simulate`], diff détaillé), comparé à l'état
/// courant ; `None` si l'appel n'est pas un lot ou si la simulation échoue
/// (le résumé du bandeau reste).
fn preview_diff(session: &Session, tool: &str, params: &Value, lang: ToolLanguage) -> Option<DiffView> {
    let batch = relayed_batch(tool, params)?;
    let options = ApplyOptions {
        detail: true,
        ..ApplyOptions::default()
    };
    let sim = session.simulate(&batch, options).ok()?;
    let before = DiffSide {
        project: session.project(),
        schedule: session.schedule(),
    };
    let after = DiffSide {
        project: &sim.project,
        schedule: &sim.schedule,
    };
    let text = TextContext::for_project(session.project(), lang);
    Some(DiffView::new(&sim.outcome.diff, before, after, &text))
}

/// Données du panneau d'opérations IA : tête de file (`banner`), son diff
/// affiché et l'historique de session.
#[must_use]
pub fn panel_input<'a>(relay: &'a Relay, banner: Option<ApprovalBanner<'a>>) -> AiPanelInput<'a> {
    AiPanelInput {
        banner,
        diff: relay.pending.front().and_then(|p| p.diff.as_ref()),
        history: Some(&relay.history),
    }
}

/// Bandeau de l'écriture en tête de `relay`, sur le document `document`
/// (il ne prend jamais le focus clavier, voir `view::approval`).
#[must_use]
pub fn banner_for<'a>(relay: &'a Relay, document: &'a str) -> Option<ApprovalBanner<'a>> {
    let front = relay.pending.front()?;
    Some(ApprovalBanner {
        id: front.id,
        tool: &front.tool,
        document,
        summary: front.preview.as_ref(),
        path: front.path(),
        queued: relay.pending.len().saturating_sub(1),
    })
}
