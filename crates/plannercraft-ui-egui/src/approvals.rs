//! File des écritures proposées en attente de l'utilisateur (ADR-0015 D10,
//! H-UI-27, H-UI-41 ; lot A2 d'ADR-0024) : desktop **et** web.
//!
//! Deux sources alimentent la même file, le même bandeau et le même
//! panneau Opérations IA :
//! - un agent MCP rattaché ([`ApprovalOrigin::Agent`], desktop : canal de
//!   contrôle, `crate::relay`) ;
//! - l'assistant intégré ([`ApprovalOrigin::Assistant`], lot A2) : l'outil
//!   `propose_changes` du modèle, déjà essayé en `dry_run` par
//!   `plannercraft_assistant::tools::SessionHost`, sans fil ni canal.
//!
//! Gardes communes (C-CTL-02 à C-CTL-04, C-CTL-07) : document remplacé ⇒
//! périmée, rien d'appliqué ; génération changée depuis l'aperçu ⇒ nouvel
//! aperçu, rien d'appliqué (jamais un aperçu périmé) ; délai écoulé ⇒
//! expirée (agent : délai de l'écouteur ; assistant :
//! [`APPROVAL_TIMEOUT_SECS`] sur l'horloge de l'interface) ; acceptée ⇒ une
//! seule entrée d'annulation, origine `Agent`. Le diff affiché
//! ([`DiffView`]) est calculé **une fois** par aperçu
//! ([`Session::simulate`]). Chaque issue entre dans l'historique de session
//! (50 entrées) ; pour l'assistant, elle change aussi la carte de la
//! proposition dans la conversation et sera dite au modèle.

use std::collections::VecDeque;

use plannercraft_assistant::proposal::{APPROVAL_TIMEOUT_SECS, ProposalStatus};
use plannercraft_assistant::tools::QueuedProposal;
use plannercraft_automation::command::params::ApplyBatchParams;
use plannercraft_automation::diff::{DiffSide, DiffView};
use plannercraft_automation::error::codes;
use plannercraft_automation::registry::{Kind, lookup};
use plannercraft_automation::text::{Language as ToolLanguage, TextContext};
use plannercraft_automation::{
    ApplyOptions, Batch, CallEnv, Command, EditSource, Session, ToolError, call_with,
};
use plannercraft_core::Moment;
use serde_json::{Map, Value};

use crate::app::{App, DocOrigin};
use crate::view::side_panel::{AiHistoryEntry, AiOutcome, AiPanelInput, push_history};
use crate::view::{ApprovalBanner, ApprovalSummary};

/// Outil d'une proposition de l'assistant (un lot du registre).
pub const ASSISTANT_TOOL: &str = "batch.apply";

/// Source d'une écriture en attente.
#[derive(Debug)]
pub enum ApprovalOrigin {
    /// Agent MCP rattaché (desktop) : réponse sur sa connexion.
    #[cfg(not(target_arch = "wasm32"))]
    Agent(crate::relay::AgentLink),
    /// Assistant intégré (lot A2) : proposition retirée après
    /// `deadline` (secondes de l'horloge de l'interface).
    Assistant {
        /// Échéance.
        deadline: f64,
    },
}

/// Écriture proposée en attente de la réponse de l'utilisateur.
#[derive(Debug)]
pub struct PendingApproval {
    /// Identifiant : requête de l'agent, ou proposition de l'assistant.
    pub id: u64,
    /// Outil demandé.
    pub tool: String,
    /// Paramètres de l'appel.
    pub params: Value,
    /// Libellé donné par l'assistant (`None` pour un agent).
    pub label: Option<String>,
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
    /// Source.
    pub origin: ApprovalOrigin,
}

impl PendingApproval {
    /// Chemin visé par un enregistrement ou un export.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        self.params.get("path").and_then(Value::as_str)
    }

    /// Proposition de l'assistant intégré.
    #[must_use]
    pub const fn is_assistant(&self) -> bool {
        matches!(self.origin, ApprovalOrigin::Assistant { .. })
    }

    /// Clé de la proposition (Accepter / Refuser).
    #[must_use]
    pub const fn key(&self) -> ApprovalKey {
        ApprovalKey {
            id: self.id,
            assistant: self.is_assistant(),
        }
    }

    /// Toujours attendue : agent encore en attente, ou échéance de
    /// l'assistant pas atteinte à `clock`.
    #[must_use]
    pub fn live(&self, clock: f64) -> bool {
        match &self.origin {
            #[cfg(not(target_arch = "wasm32"))]
            ApprovalOrigin::Agent(link) => link.is_pending(),
            ApprovalOrigin::Assistant { deadline } => clock < *deadline,
        }
    }

    /// Échéance d'une proposition de l'assistant (`None` pour un agent :
    /// l'écouteur tranche).
    #[must_use]
    pub const fn deadline(&self) -> Option<f64> {
        match &self.origin {
            #[cfg(not(target_arch = "wasm32"))]
            ApprovalOrigin::Agent(_) => None,
            ApprovalOrigin::Assistant { deadline } => Some(*deadline),
        }
    }

    /// Prend la réponse (un seul côté tranche : l'écouteur de l'agent peut
    /// l'avoir prise à l'expiration). Toujours vrai pour l'assistant.
    fn claim(&self) -> bool {
        match &self.origin {
            #[cfg(not(target_arch = "wasm32"))]
            ApprovalOrigin::Agent(link) => link.claim(),
            ApprovalOrigin::Assistant { .. } => true,
        }
    }

    /// Réponse finale à l'agent (rien pour l'assistant : l'issue lui est
    /// dite par la conversation).
    #[cfg_attr(
        target_arch = "wasm32",
        allow(
            clippy::unused_self,
            clippy::needless_pass_by_value,
            reason = "web : aucun agent à qui répondre"
        )
    )]
    fn answer(&self, result: Result<Value, &ToolError>) {
        #[cfg(not(target_arch = "wasm32"))]
        if let ApprovalOrigin::Agent(link) = &self.origin {
            link.answer(self.id, result);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = result;
    }
}

/// Désignation d'une écriture en attente : identifiant et source (un agent
/// et l'assistant peuvent avoir le même numéro).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApprovalKey {
    /// Identifiant.
    pub id: u64,
    /// Proposition de l'assistant.
    pub assistant: bool,
}

impl ApprovalKey {
    /// Écriture d'un agent.
    #[must_use]
    pub const fn agent(id: u64) -> Self {
        Self { id, assistant: false }
    }

    /// Proposition de l'assistant.
    #[must_use]
    pub const fn assistant(id: u64) -> Self {
        Self { id, assistant: true }
    }
}

/// Geste sur une proposition de l'assistant depuis sa carte (lot A2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalOp {
    /// Accepter.
    Accept(u64),
    /// Refuser (motif facultatif lu dans la saisie de la carte).
    Reject(u64),
    /// Voir : ouvre l'onglet Opérations IA.
    View(u64),
}

/// File des écritures en attente et historique de session.
#[derive(Debug, Default)]
pub struct Approvals {
    /// Écritures en attente, la première au bandeau.
    pub(crate) pending: VecDeque<PendingApproval>,
    /// Historique de session du panneau (H-UI-41), le plus ancien
    /// d'abord, 50 entrées au plus.
    pub(crate) history: VecDeque<AiHistoryEntry>,
    /// Diffs affichés calculés depuis le lancement (un par aperçu, jamais
    /// par image).
    pub(crate) diff_builds: u64,
}

impl Approvals {
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

    /// Diffs affichés calculés depuis le lancement.
    #[must_use]
    pub const fn diff_builds(&self) -> u64 {
        self.diff_builds
    }

    /// Inscrit l'issue de `pending` dans l'historique.
    pub(crate) fn record(&mut self, pending: &PendingApproval, outcome: AiOutcome, undo: Option<(u64, u64)>) {
        push_history(
            &mut self.history,
            AiHistoryEntry {
                tool: pending.tool.clone(),
                label: pending.label.clone(),
                document: pending.document.clone(),
                at: pending.now,
                outcome,
                undo,
            },
        );
    }

    /// Retire l'écriture `key` : pour un agent, seulement si elle est
    /// affichée (tête de file : deux agents peuvent avoir le même `id`,
    /// relecture finale J7, relais F3) ; pour l'assistant, où qu'elle soit
    /// (ses identifiants sont uniques, la carte de la conversation la
    /// désigne).
    fn take(&mut self, key: ApprovalKey) -> Option<PendingApproval> {
        if key.assistant {
            let pos = self.pending.iter().position(|p| p.key() == key)?;
            return self.pending.remove(pos);
        }
        if self.pending.front().is_some_and(|p| p.key() == key) {
            self.pending.pop_front()
        } else {
            None
        }
    }
}

/// Issue d'une proposition pour la carte de l'assistant.
fn status_of(outcome: &AiOutcome, reason: Option<String>) -> ProposalStatus {
    match outcome {
        AiOutcome::Accepted => ProposalStatus::Accepted,
        AiOutcome::Rejected => ProposalStatus::Rejected { reason },
        AiOutcome::Expired => ProposalStatus::Expired,
        AiOutcome::Stale => ProposalStatus::Stale,
        AiOutcome::Failed(code) => ProposalStatus::Failed { code: code.clone() },
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

/// Paramètres d'un `dry_run` (objet, `dry_run: true`), si l'appel en a un.
pub(crate) fn dry_params(tool: &str, params: &Value) -> Option<Value> {
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

/// Lot d'un appel d'écriture (`batch.apply` ou une commande du genre
/// `Write`), lu comme le routeur de `call_with` le lit ; `None` pour tout
/// autre appel ou des paramètres mal formés (le `dry_run` les a déjà
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
pub(crate) fn preview_diff(
    session: &Session,
    tool: &str,
    params: &Value,
    lang: ToolLanguage,
) -> Option<DiffView> {
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
pub fn panel_input<'a>(approvals: &'a Approvals, banner: Option<ApprovalBanner<'a>>) -> AiPanelInput<'a> {
    AiPanelInput {
        banner,
        diff: approvals.pending.front().and_then(|p| p.diff.as_ref()),
        history: Some(&approvals.history),
    }
}

/// Bandeau de l'écriture en tête de file, sur le document `document`
/// (il ne prend jamais le focus clavier, voir `view::approval`).
#[must_use]
pub fn banner_for<'a>(approvals: &'a Approvals, document: &'a str) -> Option<ApprovalBanner<'a>> {
    let front = approvals.pending.front()?;
    Some(ApprovalBanner {
        id: front.id,
        tool: &front.tool,
        label: front.label.as_deref(),
        assistant: front.is_assistant(),
        document,
        summary: front.preview.as_ref(),
        path: front.path(),
        queued: approvals.pending.len().saturating_sub(1),
    })
}

impl App {
    /// File des écritures en attente (agents et assistant).
    #[must_use]
    pub const fn approvals(&self) -> &Approvals {
        &self.approvals
    }

    /// Inscrit l'issue de `pending` ; pour l'assistant, change aussi la
    /// carte de la proposition (et la décision sera dite au modèle).
    fn conclude(
        &mut self,
        pending: &PendingApproval,
        outcome: AiOutcome,
        undo: Option<(u64, u64)>,
        reason: Option<String>,
    ) {
        if pending.is_assistant() {
            self.assistant
                .conversation
                .set_proposal_status(pending.id, status_of(&outcome, reason));
        }
        self.approvals.record(pending, outcome, undo);
    }

    /// Met en attente les propositions de l'assistant essayées pendant ce
    /// passage (lot A2) : résumé du `dry_run`, diff affiché calculé une
    /// fois, échéance. Le volet n'est pas basculé (l'utilisateur lit la
    /// conversation ; la carte porte « Voir »).
    pub(crate) fn queue_proposals(&mut self, proposals: Vec<QueuedProposal>) {
        let lang = self.text_language();
        let now = (self.env.now)();
        let clock = self.clock;
        for proposal in proposals {
            let Some(document) = &self.document else {
                // Document fermé pendant la demande : périmée aussitôt.
                self.assistant
                    .conversation
                    .set_proposal_status(proposal.id, ProposalStatus::Stale);
                continue;
            };
            let diff = preview_diff(&document.session, ASSISTANT_TOOL, &proposal.params, lang);
            let pending = PendingApproval {
                id: proposal.id,
                tool: ASSISTANT_TOOL.to_owned(),
                params: proposal.params,
                label: Some(proposal.label),
                preview: Some(summary_of(&proposal.outcome)),
                diff,
                document: document.origin.name.clone(),
                generation: document.generation(),
                opening: document.opening,
                now,
                origin: ApprovalOrigin::Assistant {
                    deadline: clock + APPROVAL_TIMEOUT_SECS,
                },
            };
            self.approvals.diff_builds = self.approvals.diff_builds.saturating_add(1);
            self.approvals.pending.push_back(pending);
        }
    }

    /// Gardes de début d'image : écritures expirées retirées, celles d'un
    /// document remplacé refusées (périmées), propositions de l'assistant
    /// retirées de la conversation (nouvelle conversation, régénération,
    /// désactivation) abandonnées ; rappel de l'image à la prochaine
    /// échéance.
    pub(crate) fn tick_approvals(&mut self, ctx: &egui::Context) {
        if self.approvals.pending.is_empty() {
            return;
        }
        let opening = self.document.as_ref().map(|document| document.opening);
        let clock = self.clock;
        let live_proposals = self.assistant.conversation.pending_proposals();
        let drained: Vec<PendingApproval> = self.approvals.pending.drain(..).collect();
        let mut next_deadline: Option<f64> = None;
        for pending in drained {
            if pending.is_assistant() && !live_proposals.contains(&pending.id) {
                self.approvals.record(&pending, AiOutcome::Expired, None);
                continue;
            }
            if !pending.live(clock) {
                self.conclude(&pending, AiOutcome::Expired, None, None);
                continue;
            }
            if Some(pending.opening) != opening {
                if pending.claim() {
                    let e = ToolError::simple(
                        codes::CONTROL_REJECTED,
                        "document fermé ou remplacé : rien n'a été appliqué".to_owned(),
                    );
                    pending.answer(Err(&e));
                    self.conclude(&pending, AiOutcome::Stale, None, None);
                }
                continue;
            }
            if let Some(deadline) = pending.deadline() {
                next_deadline = Some(next_deadline.map_or(deadline, |d: f64| d.min(deadline)));
            }
            self.approvals.pending.push_back(pending);
        }
        if let Some(deadline) = next_deadline {
            let wait = (deadline - clock).clamp(0.05, APPROVAL_TIMEOUT_SECS);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
    }

    /// Accepter (C-CTL-02, C-CTL-07) : voir l'en-tête du module.
    pub(crate) fn approve(&mut self, key: ApprovalKey) {
        let Some(mut pending) = self.approvals.take(key) else {
            return;
        };
        if !pending.live(self.clock) {
            self.conclude(&pending, AiOutcome::Expired, None, None);
            return;
        }
        let lang = self.text_language();
        let Some(document) = &mut self.document else {
            return self.reject_pending(
                &pending,
                "aucun document : rien n'a été appliqué",
                AiOutcome::Stale,
                None,
            );
        };
        if document.opening != pending.opening {
            return self.reject_pending(
                &pending,
                "document fermé ou remplacé : rien n'a été appliqué",
                AiOutcome::Stale,
                None,
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
                        if pending.claim() {
                            pending.answer(Err(&e));
                            self.conclude(&pending, AiOutcome::Failed(e.code.to_owned()), None, None);
                        }
                        return;
                    }
                }
            }
            pending.generation = document.generation();
            if pending.preview.is_some() {
                self.approvals.diff_builds = self.approvals.diff_builds.saturating_add(1);
            }
            self.approvals.pending.push_front(pending);
            return;
        }
        if !pending.claim() {
            self.conclude(&pending, AiOutcome::Expired, None, None);
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
        pending.answer(match &result {
            Ok(value) => Ok(value.clone()),
            Err(e) => Err(e),
        });
        self.conclude(&pending, outcome, undo, None);
        if changed {
            self.refresh();
        } else {
            self.notify_observer();
        }
        self.after_banner(&pending);
    }

    /// Refuser (C-CTL-03) : rien n'est appliqué ; `reason` : motif
    /// facultatif de l'utilisateur (assistant), dit au modèle.
    pub(crate) fn reject(&mut self, key: ApprovalKey, reason: Option<String>) {
        let Some(pending) = self.approvals.take(key) else {
            return;
        };
        self.reject_pending(
            &pending,
            "refusé par l'utilisateur : rien n'a été appliqué",
            AiOutcome::Rejected,
            reason,
        );
    }

    /// Répond `control.rejected` à une écriture en attente (agent) ; l'issue
    /// `outcome` entre dans l'historique (expirée si l'appel l'était déjà).
    fn reject_pending(
        &mut self,
        pending: &PendingApproval,
        message: &str,
        outcome: AiOutcome,
        reason: Option<String>,
    ) {
        let outcome = if pending.claim() {
            pending.answer(Err(&ToolError::simple(
                codes::CONTROL_REJECTED,
                message.to_owned(),
            )));
            outcome
        } else {
            AiOutcome::Expired
        };
        self.conclude(pending, outcome, None, reason);
        self.after_banner(pending);
    }

    /// Plus aucun bandeau : le focus clavier revient à la table (pas après
    /// une proposition de l'assistant : l'utilisateur est dans la
    /// conversation).
    fn after_banner(&mut self, decided: &PendingApproval) {
        if !decided.is_assistant() && self.approvals.pending.is_empty() && self.document.is_some() {
            self.view.focus_table_pending = true;
        }
    }
}
