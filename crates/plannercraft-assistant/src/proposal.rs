//! Propositions d'écriture de l'assistant (ADR-0024, lot A2).
//!
//! Le modèle n'a qu'**un** outil d'écriture, [`PROPOSE_TOOL`] : il soumet
//! un lot du registre (`{ label, ops }`, format de `batch.apply`) qui ne
//! s'applique jamais directement. L'interface en fait une écriture en
//! attente du panneau Opérations IA (aperçu `dry_run`, avant/après,
//! Accepter / Refuser, une annulation) ; l'outil rend au modèle le résumé
//! du `dry_run` et la consigne d'attendre la décision de l'utilisateur,
//! qui lui est dite au message suivant ([`decision_note`]).
//!
//! Garde-fous écrits ici, sans interface (testés seuls) :
//! - liste fermée d'opérations ([`ALLOWED_OPS`]) ;
//! - **aucune date** dans un lot (règle 3 : le modèle propose des durées et
//!   des liens, le moteur place les dates) : les champs datés
//!   ([`DATE_FIELDS`]) sont refusés avec un message que le modèle lit ;
//! - bornes (libellé, nombre d'opérations) ;
//! - au plus [`MAX_CORRECTIONS`] corrections après un refus, et une seule
//!   proposition soumise par demande (appliqué par la conversation).

use plannercraft_automation::ToolError;
use plannercraft_automation::text::Language;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Nom de l'outil d'écriture donné au modèle.
pub const PROPOSE_TOOL: &str = "propose_changes";

/// Opérations du registre permises dans une proposition.
pub const ALLOWED_OPS: &[&str] = &[
    "task.add",
    "task.update",
    "task.delete",
    "task.move",
    "task.indent",
    "task.outdent",
    "task.shift",
    "task.set_text_style",
    "link.add",
    "link.update",
    "link.remove",
    "resource.add",
    "assignment.add",
    "assignment.update",
    "assignment.remove",
    "progress.update",
    "custom_field.rename",
    "style.set_group_by",
    "baseline.set",
];

/// Champs datés refusés dans une proposition (règle 3), par opération.
pub const DATE_FIELDS: &[(&str, &[&str])] = &[
    ("task.add", &["start", "finish"]),
    ("task.update", &["start", "finish"]),
    ("progress.update", &["actual_start", "actual_finish"]),
];

/// Opérations au plus dans une proposition (règle 10).
pub const MAX_OPS: usize = 400;
/// Longueur au plus du libellé (caractères).
pub const MAX_LABEL_CHARS: usize = 80;
/// Corrections au plus après un refus du registre, par demande.
pub const MAX_CORRECTIONS: usize = 2;
/// Validation de l'utilisateur attendue au plus (secondes, ADR-0015 D10 :
/// même délai que pour un agent rattaché).
pub const APPROVAL_TIMEOUT_SECS: f64 = 120.0;

/// Issue d'une proposition, affichée sur sa carte et dite au modèle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalStatus {
    /// En attente de l'utilisateur.
    Pending,
    /// Acceptée et appliquée (une entrée d'annulation).
    Accepted,
    /// Refusée par l'utilisateur, avec son motif facultatif.
    Rejected {
        /// Motif saisi.
        #[serde(default)]
        reason: Option<String>,
    },
    /// Document fermé ou remplacé avant la décision : rien d'appliqué.
    Stale,
    /// Sans réponse dans le délai, ou retirée (nouvelle conversation) :
    /// rien d'appliqué.
    Expired,
    /// Acceptée mais refusée par le moteur à l'application (code).
    Failed {
        /// Code stable de l'erreur.
        code: String,
    },
}

impl ProposalStatus {
    /// Proposition encore en attente.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

/// Proposition soumise, gardée sur la ligne d'outil de son tour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalCard {
    /// Identifiant de la proposition (unique dans la session de
    /// l'application).
    pub id: u64,
    /// Libellé court donné par le modèle.
    pub label: String,
    /// Tâches touchées selon le `dry_run` du moteur.
    pub changed_tasks: u32,
    /// Issue.
    pub status: ProposalStatus,
}

/// Proposition acceptée en file par l'hôte : identifiant, libellé, tâches
/// touchées et résultat rendu au modèle.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposed {
    /// Identifiant de la proposition.
    pub id: u64,
    /// Libellé.
    pub label: String,
    /// Tâches touchées (`dry_run`).
    pub changed_tasks: u32,
    /// Résultat JSON rendu au modèle (résumé du `dry_run`).
    pub result: Value,
}

const fn tr(lang: Language, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Fr => fr,
        Language::En => en,
    }
}

fn invalid(message: String) -> ToolError {
    ToolError::simple("param.invalid", message)
}

/// Lot lu dans les arguments de [`PROPOSE_TOOL`] : libellé et paramètres
/// de `batch.apply` (`{ ops, label }`), après les garde-fous.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckedProposal {
    /// Libellé (rogné, borné).
    pub label: String,
    /// Paramètres de `batch.apply`.
    pub params: Value,
}

/// Contrôle les arguments de [`PROPOSE_TOOL`] (texte JSON du modèle).
///
/// # Errors
///
/// `param.invalid` en langage clair (le modèle le lit pour se corriger) :
/// JSON illisible, libellé absent, aucune opération, opération hors de la
/// liste, champ daté, trop d'opérations.
pub fn check(arguments: &str, lang: Language) -> Result<CheckedProposal, ToolError> {
    let value: Value = serde_json::from_str(arguments).map_err(|e| {
        invalid(match lang {
            Language::Fr => format!("arguments JSON invalides : {e}"),
            Language::En => format!("invalid JSON arguments: {e}"),
        })
    })?;
    let Value::Object(mut map) = value else {
        return Err(invalid(
            tr(
                lang,
                "objet { label, ops } attendu",
                "object { label, ops } expected",
            )
            .to_owned(),
        ));
    };
    let label = match map.remove("label") {
        Some(Value::String(s)) if !s.trim().is_empty() => {
            s.trim().chars().take(MAX_LABEL_CHARS).collect::<String>()
        }
        _ => {
            return Err(invalid(
                tr(
                    lang,
                    "« label » (libellé court du lot) est obligatoire",
                    "\"label\" (short batch label) is required",
                )
                .to_owned(),
            )
            .with_field("label"));
        }
    };
    let Some(Value::Array(ops)) = map.remove("ops") else {
        return Err(invalid(
            tr(
                lang,
                "« ops » (liste d'opérations) est obligatoire",
                "\"ops\" (list of operations) is required",
            )
            .to_owned(),
        )
        .with_field("ops"));
    };
    if let Some(extra) = map.keys().next() {
        return Err(invalid(match lang {
            Language::Fr => format!("champ inconnu « {extra} » : seuls « label » et « ops » sont permis"),
            Language::En => format!("unknown field \"{extra}\": only \"label\" and \"ops\" are allowed"),
        }));
    }
    if ops.is_empty() {
        return Err(invalid(
            tr(
                lang,
                "le lot ne contient aucune opération",
                "the batch has no operation",
            )
            .to_owned(),
        )
        .with_field("ops"));
    }
    if ops.len() > MAX_OPS {
        return Err(invalid(match lang {
            Language::Fr => format!("{} opérations : {MAX_OPS} au plus par proposition", ops.len()),
            Language::En => format!("{} operations: at most {MAX_OPS} per proposal", ops.len()),
        })
        .with_field("ops"));
    }
    for (index, op) in ops.iter().enumerate() {
        check_op(index, op, lang)?;
    }
    let mut params = Map::new();
    params.insert("ops".to_owned(), Value::Array(ops));
    params.insert("label".to_owned(), Value::String(label.clone()));
    Ok(CheckedProposal {
        label,
        params: Value::Object(params),
    })
}

fn check_op(index: usize, op: &Value, lang: Language) -> Result<(), ToolError> {
    let at = |e: ToolError| ToolError {
        op_index: Some(index),
        ..e
    };
    let Some(name) = op.get("op").and_then(Value::as_str) else {
        return Err(at(invalid(match lang {
            Language::Fr => format!("opération {index} : champ « op » (texte) attendu"),
            Language::En => format!("operation {index}: \"op\" (string) expected"),
        })));
    };
    if !ALLOWED_OPS.contains(&name) {
        return Err(at(invalid(match lang {
            Language::Fr => format!(
                "opération {index} : « {name} » n'est pas permise dans une proposition (permises : {})",
                ALLOWED_OPS.join(", ")
            ),
            Language::En => format!(
                "operation {index}: \"{name}\" is not allowed in a proposal (allowed: {})",
                ALLOWED_OPS.join(", ")
            ),
        })));
    }
    if let Some((_, fields)) = DATE_FIELDS.iter().find(|(op, _)| *op == name)
        && let Some(field) = fields.iter().find(|f| op.get(**f).is_some())
    {
        return Err(at(invalid(match lang {
            Language::Fr => format!(
                "opération {index} ({name}) : le champ daté « {field} » est interdit ; ne donne aucune date, \
                 propose des durées et des liens, le moteur place les dates"
            ),
            Language::En => format!(
                "operation {index} ({name}): the date field \"{field}\" is forbidden; give no date, propose \
                 durations and links, the engine places the dates"
            ),
        })
        .with_field(field)));
    }
    Ok(())
}

/// Résultat rendu au modèle pour une proposition mise en file : résumé du
/// `dry_run` (`outcome`, JSON de `batch.apply`) et consigne d'attente.
#[must_use]
pub fn pending_result(id: u64, label: &str, outcome: &Value, lang: Language) -> Value {
    let at = |path: &str| outcome.pointer(path).cloned().unwrap_or(Value::Null);
    let len = |path: &str| {
        outcome
            .pointer(path)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    let warnings: Vec<Value> = outcome
        .pointer("/warnings")
        .and_then(Value::as_array)
        .map(|w| w.iter().take(10).cloned().collect())
        .unwrap_or_default();
    json!({
        "status": "pending_approval",
        "proposal": id,
        "label": label,
        "changed_tasks": at("/diff/changed_total"),
        "created_tasks": len("/created_uids"),
        "finish_delta": at("/diff/finish_delta/text"),
        "critical_entered": len("/diff/critical_entered"),
        "critical_left": len("/diff/critical_left"),
        "warnings": warnings,
        "note": tr(
            lang,
            "Rien n'est appliqué : la proposition attend la décision de l'utilisateur dans le panneau \
             Opérations IA. Résume-la en une ou deux phrases, sans citer de date, et attends ; la décision te \
             sera dite au message suivant.",
            "Nothing is applied: the proposal awaits the user's decision in the AI Operations panel. Summarise \
             it in one or two sentences, without quoting any date, and wait; the decision will be given to you \
             in the next message.",
        ),
    })
}

/// Résultat d'un refus du registre ou d'un garde-fou : l'erreur et la
/// consigne de correction (`left` corrections encore permises).
#[must_use]
pub fn refused_result(error: &ToolError, left: usize, lang: Language) -> Value {
    let hint = if left == 0 {
        tr(
            lang,
            "Plus de correction permise pour cette demande : n'appelle plus propose_changes, explique le \
             blocage à l'utilisateur.",
            "No more corrections allowed for this request: do not call propose_changes again, explain the \
             problem to the user.",
        )
        .to_owned()
    } else {
        match lang {
            Language::Fr => format!(
                "Le lot est refusé, rien n'est soumis. Corrige-le d'après l'erreur et rappelle propose_changes \
                 ({left} correction(s) encore permise(s))."
            ),
            Language::En => format!(
                "The batch is refused, nothing is submitted. Fix it from the error and call propose_changes \
                 again ({left} correction(s) left)."
            ),
        }
    };
    let error = serde_json::to_value(error)
        .unwrap_or_else(|_| json!({ "code": error.code, "message": error.message }));
    json!({ "error": error, "hint": hint })
}

/// Refus sans appel à l'hôte : une proposition est déjà soumise dans ce
/// tour, ou les corrections sont épuisées.
#[must_use]
pub fn blocked_result(already_submitted: bool, lang: Language) -> Value {
    let message = if already_submitted {
        tr(
            lang,
            "une proposition est déjà soumise pour cette demande : une seule par demande ; attends la décision \
             de l'utilisateur",
            "a proposal is already submitted for this request: one per request; wait for the user's decision",
        )
    } else {
        tr(
            lang,
            "corrections épuisées pour cette demande : explique le blocage à l'utilisateur",
            "no corrections left for this request: explain the problem to the user",
        )
    };
    json!({ "error": { "code": "proposal.blocked", "message": message } })
}

/// Phrase dite au modèle, au message suivant, sur la décision de
/// l'utilisateur.
#[must_use]
pub fn decision_note(label: &str, status: &ProposalStatus, lang: Language) -> String {
    let what = match (status, lang) {
        (ProposalStatus::Pending, Language::Fr) => "toujours en attente".to_owned(),
        (ProposalStatus::Pending, Language::En) => "still pending".to_owned(),
        (ProposalStatus::Accepted, Language::Fr) => "acceptée : elle est appliquée au planning".to_owned(),
        (ProposalStatus::Accepted, Language::En) => "accepted: it is applied to the schedule".to_owned(),
        (ProposalStatus::Rejected { reason: Some(r) }, Language::Fr) => {
            format!("refusée, rien n'a été appliqué (motif de l'utilisateur : {r})")
        }
        (ProposalStatus::Rejected { reason: Some(r) }, Language::En) => {
            format!("rejected, nothing was applied (user's reason: {r})")
        }
        (ProposalStatus::Rejected { reason: None }, Language::Fr) => {
            "refusée, rien n'a été appliqué".to_owned()
        }
        (ProposalStatus::Rejected { reason: None }, Language::En) => {
            "rejected, nothing was applied".to_owned()
        }
        (ProposalStatus::Stale, Language::Fr) => {
            "périmée (document fermé ou remplacé), rien n'a été appliqué".to_owned()
        }
        (ProposalStatus::Stale, Language::En) => {
            "stale (document closed or replaced), nothing was applied".to_owned()
        }
        (ProposalStatus::Expired, Language::Fr) => "expirée sans réponse, rien n'a été appliqué".to_owned(),
        (ProposalStatus::Expired, Language::En) => {
            "expired without an answer, nothing was applied".to_owned()
        }
        (ProposalStatus::Failed { code }, Language::Fr) => {
            format!("acceptée mais refusée par le moteur à l'application ({code}), rien n'a été appliqué")
        }
        (ProposalStatus::Failed { code }, Language::En) => {
            format!("accepted but refused by the engine when applied ({code}), nothing was applied")
        }
    };
    match lang {
        Language::Fr => format!("[Décision de l'utilisateur sur la proposition « {label} » : {what}.]"),
        Language::En => format!("[User's decision on the proposal \"{label}\": {what}.]"),
    }
}

/// Définition de [`PROPOSE_TOOL`] au format Chat Completions : schéma
/// compact écrit à la main (les schémas complets du registre pèsent
/// 148 Ko pour `batch.apply`), opérations décrites en une ligne chacune.
#[must_use]
pub fn definition(lang: Language) -> Value {
    let (description, ops_doc, label_doc) = match lang {
        Language::Fr => (
            "Propose un lot de modifications du planning. Rien n'est appliqué : l'utilisateur voit l'avant/après \
             et accepte ou refuse. Un seul lot cohérent par demande. Jamais de date : des durées et des liens, le \
             moteur place les dates.",
            OPS_DOC_FR,
            "Libellé court du lot (« Planning maison 120 m² », « Décaler le second œuvre de 2 semaines »).",
        ),
        Language::En => (
            "Propose a batch of schedule changes. Nothing is applied: the user sees before/after and accepts or \
             rejects. One coherent batch per request. Never a date: durations and links, the engine places the \
             dates.",
            OPS_DOC_EN,
            "Short batch label (\"House plan 120 m²\", \"Shift finishing works by 2 weeks\").",
        ),
    };
    json!({
        "type": "function",
        "function": {
            "name": PROPOSE_TOOL,
            "description": description,
            "parameters": {
                "type": "object",
                "required": ["label", "ops"],
                "additionalProperties": false,
                "properties": {
                    "label": { "type": "string", "maxLength": MAX_LABEL_CHARS, "description": label_doc },
                    "ops": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_OPS,
                        "description": ops_doc,
                        "items": {
                            "type": "object",
                            "required": ["op"],
                            "properties": { "op": { "type": "string", "enum": ALLOWED_OPS } },
                        },
                    },
                },
            },
        },
    })
}

/// Opérations, une ligne chacune (français).
const OPS_DOC_FR: &str = "Opérations dans l'ordre, une transaction. T = tâche : UID (entier) ou \"$x\" d'une tâche \
créée plus haut dans le lot (\"as\":\"$x\"). D = durée {\"text\":\"5d\"} (d jours ouvrés, w semaines, h heures ; \
\"0d\" jalon ; négatif \"-3d\" pour avancer). P = placement \"end\" | {\"after\":T} | {\"before\":T} | \
{\"first_child_of\":T} | {\"last_child_of\":T}.\n\
task.add {name, as?, duration?:D, milestone?:bool, at?:P, notes?}\n\
task.update {uid:T, name?, duration?:D, milestone?, notes?, active?:bool, mode?:\"auto\"|\"manual\", \
custom?:{\"text1\":\"Gros œuvre\"} (text1..30, number1..20, flag1..20)}\n\
task.delete {uid:T, cascade?:bool (obligatoire pour une récapitulative)}\n\
task.move {uid:T, to:P}\n\
task.indent {uid:T} / task.outdent {uid:T} (indenter sous la tâche qui précède = la rendre sous-tâche)\n\
task.shift {tasks:[T], by:D} (décale d'une durée signée)\n\
task.set_text_style {tasks:[T], bold?, italic?, color?:\"#RRGGBB\", fill?:\"#RRGGBB\"}\n\
link.add {pred:T, succ:T, kind?:\"fs\"|\"ss\"|\"ff\"|\"sf\" (défaut fs), lag?:{\"text\":\"2d\"}}\n\
link.update {pred:T, succ:T, kind?, lag?} / link.remove {pred:T, succ:T}\n\
resource.add {name, as?:\"$r\", kind?:\"work\"|\"material\"|\"cost\", max_units?:{\"text\":\"100%\"}}\n\
assignment.add {task:T, resource:UID|\"$r\", units?:{\"text\":\"50%\"}} / assignment.update {task, resource, \
units?} / assignment.remove {task, resource}\n\
progress.update {task:T, percent_complete:{\"text\":\"50%\"}}\n\
custom_field.rename {field:\"text1\", alias:\"Lot\"}\n\
style.set_group_by {group_by:{\"custom\":\"text1\"}|\"resource\"|\"critical\"|\"milestone\"|null}\n\
baseline.set {number?:0..10, tasks?:[T]}\n\
Interdits : start, finish, actual_start, actual_finish et toute autre date.";

/// Opérations, une ligne chacune (anglais).
const OPS_DOC_EN: &str = "Operations in order, one transaction. T = task: UID (integer) or \"$x\" of a task \
created earlier in the batch (\"as\":\"$x\"). D = duration {\"text\":\"5d\"} (d working days, w weeks, h hours; \
\"0d\" milestone; negative \"-3d\" to pull in). P = placement \"end\" | {\"after\":T} | {\"before\":T} | \
{\"first_child_of\":T} | {\"last_child_of\":T}.\n\
task.add {name, as?, duration?:D, milestone?:bool, at?:P, notes?}\n\
task.update {uid:T, name?, duration?:D, milestone?, notes?, active?:bool, mode?:\"auto\"|\"manual\", \
custom?:{\"text1\":\"Structure\"} (text1..30, number1..20, flag1..20)}\n\
task.delete {uid:T, cascade?:bool (required for a summary)}\n\
task.move {uid:T, to:P}\n\
task.indent {uid:T} / task.outdent {uid:T} (indent under the previous task = make it a subtask)\n\
task.shift {tasks:[T], by:D} (shifts by a signed duration)\n\
task.set_text_style {tasks:[T], bold?, italic?, color?:\"#RRGGBB\", fill?:\"#RRGGBB\"}\n\
link.add {pred:T, succ:T, kind?:\"fs\"|\"ss\"|\"ff\"|\"sf\" (default fs), lag?:{\"text\":\"2d\"}}\n\
link.update {pred:T, succ:T, kind?, lag?} / link.remove {pred:T, succ:T}\n\
resource.add {name, as?:\"$r\", kind?:\"work\"|\"material\"|\"cost\", max_units?:{\"text\":\"100%\"}}\n\
assignment.add {task:T, resource:UID|\"$r\", units?:{\"text\":\"50%\"}} / assignment.update {task, resource, \
units?} / assignment.remove {task, resource}\n\
progress.update {task:T, percent_complete:{\"text\":\"50%\"}}\n\
custom_field.rename {field:\"text1\", alias:\"Trade\"}\n\
style.set_group_by {group_by:{\"custom\":\"text1\"}|\"resource\"|\"critical\"|\"milestone\"|null}\n\
baseline.set {number?:0..10, tasks?:[T]}\n\
Forbidden: start, finish, actual_start, actual_finish and any other date.";

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net"
    )]

    use plannercraft_automation::{Kind, REGISTRY};

    use super::*;

    #[test]
    fn allowed_ops_are_registry_writes() {
        for id in ALLOWED_OPS {
            let spec = REGISTRY.iter().find(|s| s.id == *id).unwrap();
            assert_eq!(spec.kind, Kind::Write, "{id}");
        }
        for (op, _) in DATE_FIELDS {
            assert!(ALLOWED_OPS.contains(op), "{op}");
        }
    }

    /// Schéma d'écriture : objectif < 6 000 jetons (4 octets par jeton,
    /// estimation prudente pour du JSON).
    #[test]
    fn propose_schema_under_budget() {
        for lang in [Language::Fr, Language::En] {
            let bytes = definition(lang).to_string().len();
            println!(
                "schéma propose_changes ({lang:?}) : {bytes} octets ≈ {} jetons",
                bytes / 4
            );
            assert!(bytes < 6_000 * 4, "{bytes}");
        }
    }

    #[test]
    fn check_accepts_a_plan() {
        let args = r#"{"label":"Petit planning","ops":[
            {"op":"task.add","name":"Études","as":"$a","duration":{"text":"10d"}},
            {"op":"task.add","name":"Permis obtenu","as":"$b","milestone":true,"duration":{"text":"0d"}},
            {"op":"link.add","pred":"$a","succ":"$b"}]}"#;
        let checked = check(args, Language::Fr).unwrap();
        assert_eq!(checked.label, "Petit planning");
        assert_eq!(checked.params["ops"].as_array().unwrap().len(), 3);
        assert_eq!(checked.params["label"], "Petit planning");
    }

    #[test]
    fn check_refuses_dates_and_foreign_ops() {
        let dated = r#"{"label":"x","ops":[{"op":"task.add","name":"A","start":"2026-11-02T08:00"}]}"#;
        let e = check(dated, Language::Fr).unwrap_err();
        assert_eq!(e.code, "param.invalid");
        assert_eq!(e.op_index, Some(0));
        assert_eq!(e.field.as_deref(), Some("start"));
        assert!(e.message.contains("le moteur place les dates"), "{}", e.message);
        let anchor = r#"{"label":"x","ops":[{"op":"project.set_anchor","start":"2026-11-02T08:00"}]}"#;
        assert!(
            check(anchor, Language::En)
                .unwrap_err()
                .message
                .contains("not allowed")
        );
        let progress =
            r#"{"label":"x","ops":[{"op":"progress.update","task":1,"actual_finish":"2026-11-02T08:00"}]}"#;
        assert_eq!(
            check(progress, Language::Fr).unwrap_err().field.as_deref(),
            Some("actual_finish")
        );
    }

    #[test]
    fn check_refuses_shape_errors() {
        for (args, needle) in [
            ("{oops", "JSON"),
            (r#"{"ops":[{"op":"task.add","name":"A"}]}"#, "label"),
            (r#"{"label":"x"}"#, "ops"),
            (r#"{"label":"x","ops":[]}"#, "aucune"),
            (r#"{"label":"x","ops":[{"name":"A"}]}"#, "op"),
            (
                r#"{"label":"x","ops":[{"op":"task.add","name":"A"}],"dry_run":false}"#,
                "dry_run",
            ),
        ] {
            let e = check(args, Language::Fr).unwrap_err();
            assert!(e.message.contains(needle), "{args} : {}", e.message);
        }
    }

    #[test]
    fn notes_in_both_languages() {
        let rejected = ProposalStatus::Rejected {
            reason: Some("trop long".to_owned()),
        };
        assert_eq!(
            decision_note("Lot", &rejected, Language::Fr),
            "[Décision de l'utilisateur sur la proposition « Lot » : refusée, rien n'a été appliqué (motif de \
             l'utilisateur : trop long).]"
        );
        assert!(decision_note("Lot", &ProposalStatus::Accepted, Language::En).contains("accepted"));
    }

    #[test]
    fn refused_result_counts_corrections() {
        let e = ToolError::simple("link.cycle", "boucle".to_owned());
        let v = refused_result(&e, 2, Language::Fr);
        assert_eq!(v["error"]["code"], "link.cycle");
        assert!(v["hint"].as_str().unwrap().contains("2 correction"));
        let last = refused_result(&e, 0, Language::Fr);
        assert!(last["hint"].as_str().unwrap().contains("Plus de correction"));
    }
}
