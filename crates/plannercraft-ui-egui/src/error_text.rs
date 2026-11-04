//! Refus du registre en langage d'utilisateur (seconde passe UX, ADR-0019
//! D1, H-UI-105).
//!
//! [`user_message`] est appelée par `App::report_command_error` et rend la
//! phrase affichée et le détail technique repliable :
//!
//! - la **phrase** vient d'une table par code (lien en double, boucle,
//!   lien vers soi, récapitulative, tâche introuvable, lot qui créerait
//!   une erreur, saisie illisible, valeur refusée, limite, champ
//!   personnalisé, lien absent, indentation, récapitulative supprimée
//!   seule, déplacement dans son sous-arbre, champ calculé, affectation en
//!   double ou à 0 % sur travail fixe, travail fixe non piloté par
//!   l'effort, avancement, ligne de base déjà là, calendrier utilisé, nom de calendrier pris,
//!   exceptions qui se chevauchent, base invalide, ressource affectée,
//!   champ contraire au type de la ressource) ; le sujet y est nommé par son **ID** de tâche (position
//!   dans `project.tasks`), jamais par son UID ; une tâche disparue se dit
//!   « une tâche supprimée » ; un code sans phrase dédiée retombe sur
//!   « Cette modification n'est pas possible. » ;
//! - le **détail** porte ce que la phrase tait : code, numéro d'opération,
//!   champ et texte du moteur ;
//! - un lot refusé pour un autre diagnostic que la boucle cite ses tâches
//!   par leur ID ([`DiagTexts::with_project`]).

use std::borrow::Cow;

use plannercraft_automation::error::codes;
use plannercraft_automation::text::TextError;
use plannercraft_automation::{CommandError, CommandErrorKind};
use plannercraft_core::{Diagnostic, Project, Subject, TaskUid};

use crate::diag_text::DiagTexts;
use crate::i18n::{Arg, TextId, Texts};

/// Codes du registre qu'un lot de l'interface peut produire et qui ont
/// leur phrase dédiée (H-UI-105, test `h_ui_105_every_ui_code_has_sentence`).
/// Les codes `text.*` et `custom.*` sont couverts par préfixe.
pub const UI_CODES: [&str; 28] = [
    codes::LINK_DUPLICATE,
    codes::CALENDAR_IN_USE,
    codes::CALENDAR_NAME_TAKEN,
    codes::CALENDAR_EXCEPTION_OVERLAP,
    codes::CALENDAR_BASE_NOT_BASE,
    codes::RESOURCE_HAS_ASSIGNMENTS,
    codes::ASSIGNMENT_KIND_MISMATCH,
    codes::LINK_NOT_FOUND,
    codes::TASK_OUTLINE_REFUSED,
    codes::TASK_HAS_CHILDREN,
    codes::TASK_MOVE_INTO_SUBTREE,
    codes::TASK_COMPUTED_FIELD,
    codes::ASSIGNMENT_DUPLICATE,
    codes::ASSIGNMENT_ZERO_UNITS_FIXED_WORK,
    codes::TASK_FIXED_WORK_EFFORT_DRIVEN,
    codes::PROGRESS_SUMMARY_NOT_EDITABLE,
    codes::PROGRESS_INACTIVE,
    codes::PROGRESS_INVALID_ORDER,
    codes::PROGRESS_DURATION_BELOW_ACTUAL,
    codes::BASELINE_EXISTS,
    codes::LINK_SELF,
    codes::LINK_SUMMARY_DESCENDANT,
    codes::TASK_NOT_FOUND,
    codes::BATCH_WOULD_CREATE_ERRORS,
    codes::PARAM_INVALID,
    codes::LIMIT_EXCEEDED,
    codes::PARAM_CONFLICT,
    "text.bad_number",
];

/// Phrase et détail d'un refus de commande : `(texte, détail)`. `project` :
/// document visé (sujets en ID) ; `engine` : langue des textes du moteur
/// (repris dans le détail).
#[must_use]
pub fn user_message(
    error: &CommandError,
    project: Option<&Project>,
    texts: &Texts,
    engine: plannercraft_automation::text::Language,
) -> (String, String) {
    let text = sentence(error, project, texts).unwrap_or_else(|| texts.get(TextId::RefusalGeneric));
    (text, detail(error, texts, engine))
}

/// ID affiché de la tâche `uid` (« 12 »), ou « une tâche supprimée ».
fn task_label(uid: TaskUid, project: Option<&Project>, texts: &Texts) -> String {
    project.and_then(|p| p.task_id(uid)).map_or_else(
        || texts.get(TextId::RefusalDeletedTask),
        |id| id.get().to_string(),
    )
}

/// Utilisateur d'un calendrier (`calendar.in_use`) : « le projet », « la
/// tâche 12 » (ID), ressource ou calendrier par son nom.
fn user_label(subject: &Subject, project: Option<&Project>, texts: &Texts) -> String {
    let named = |id: TextId, name: String| texts.get_args(id, &[("name", Arg::Str(name.into()))]);
    match subject {
        Subject::Task(uid) => texts.get_args(
            TextId::RefusalUserTask,
            &[("id", Arg::Str(task_label(*uid, project, texts).into()))],
        ),
        Subject::Resource(uid) => named(
            TextId::RefusalUserResource,
            project
                .and_then(|p| p.resources.get(*uid))
                .map_or_else(|| uid.get().to_string(), |r| r.name.clone()),
        ),
        Subject::Calendar(uid) => named(
            TextId::RefusalUserCalendar,
            project
                .and_then(|p| p.calendars.get(*uid))
                .map_or_else(|| uid.get().to_string(), |c| c.name.clone()),
        ),
        _ => texts.get(TextId::RefusalUserProject),
    }
}

/// Phrase dédiée au code de `error`, s'il en a une.
fn sentence(error: &CommandError, project: Option<&Project>, texts: &Texts) -> Option<String> {
    let code = error.code;
    if code.starts_with("custom.") {
        return Some(texts.get(TextId::RefusalCustomField));
    }
    if code.starts_with("text.") {
        return Some(text_sentence(&error.kind, texts));
    }
    match code {
        codes::LINK_DUPLICATE => {
            let Some(Subject::Link {
                predecessor,
                successor,
            }) = error.subject
            else {
                return None;
            };
            Some(texts.get_args(
                TextId::RefusalLinkDuplicate,
                &[
                    (
                        "successor",
                        Arg::Str(task_label(successor, project, texts).into()),
                    ),
                    (
                        "predecessor",
                        Arg::Str(task_label(predecessor, project, texts).into()),
                    ),
                ],
            ))
        }
        codes::LINK_NOT_FOUND => {
            let Some(Subject::Link {
                predecessor,
                successor,
            }) = error.subject
            else {
                return None;
            };
            Some(texts.get_args(
                TextId::RefusalLinkNotFound,
                &[
                    (
                        "successor",
                        Arg::Str(task_label(successor, project, texts).into()),
                    ),
                    (
                        "predecessor",
                        Arg::Str(task_label(predecessor, project, texts).into()),
                    ),
                ],
            ))
        }
        codes::ASSIGNMENT_DUPLICATE => {
            let task = match (&error.kind, error.subject) {
                (CommandErrorKind::DuplicateAssignment { task, .. }, _) => *task,
                (_, Some(Subject::Task(task))) => task,
                _ => return None,
            };
            Some(texts.get_args(
                TextId::RefusalAssignmentDuplicate,
                &[("id", Arg::Str(task_label(task, project, texts).into()))],
            ))
        }
        codes::CALENDAR_IN_USE => {
            let CommandErrorKind::CalendarInUse { users, total, .. } = &error.kind else {
                return None;
            };
            let mut list: Vec<String> = users.iter().map(|u| user_label(u, project, texts)).collect();
            if *total > users.len() {
                list.push("…".to_owned());
            }
            Some(texts.get_args(
                TextId::RefusalCalendarInUse,
                &[("detail", Arg::Str(list.join(", ").into()))],
            ))
        }
        codes::TASK_NOT_FOUND => {
            // L'UID d'une tâche introuvable n'a pas d'ID : phrase générique,
            // l'UID reste dans le détail.
            let CommandErrorKind::UnknownTask(uid) = error.kind else {
                return None;
            };
            let id = project.and_then(|p| p.task_id(uid))?;
            Some(texts.get_args(
                TextId::RefusalTaskNotFound,
                &[("id", Arg::Str(id.get().to_string().into()))],
            ))
        }
        codes::PARAM_CONFLICT => conflict_sentence(&error.kind, texts),
        codes::BATCH_WOULD_CREATE_ERRORS => {
            let CommandErrorKind::WouldCreateErrors(diags) = &error.kind else {
                return None;
            };
            let first = diags.first()?;
            Some(diagnostic_sentence(first, project, texts))
        }
        _ => fixed_sentence(code).map(|id| texts.get(id)),
    }
}

/// Conflit de champs : seul celui qu'une saisie de cellule peut produire a
/// sa phrase, % travail achevé ou travail réel sur une tâche sans
/// affectation (R-211) ; les autres gardent la phrase générique.
fn conflict_sentence(kind: &CommandErrorKind, texts: &Texts) -> Option<String> {
    match kind {
        CommandErrorKind::ParamConflict(_, "assignments") => {
            Some(texts.get(TextId::RefusalWorkNeedsResources))
        }
        _ => None,
    }
}

/// Phrase sans argument d'un code du registre, s'il en a une.
fn fixed_sentence(code: &str) -> Option<TextId> {
    Some(match code {
        codes::CALENDAR_NAME_TAKEN => TextId::RefusalCalendarNameTaken,
        codes::CALENDAR_EXCEPTION_OVERLAP => TextId::RefusalCalendarOverlap,
        codes::CALENDAR_BASE_NOT_BASE => TextId::RefusalCalendarBase,
        codes::RESOURCE_HAS_ASSIGNMENTS => TextId::RefusalResourceAssigned,
        codes::ASSIGNMENT_KIND_MISMATCH => TextId::RefusalAssignmentKind,
        codes::TASK_OUTLINE_REFUSED => TextId::RefusalOutline,
        codes::TASK_HAS_CHILDREN => TextId::RefusalHasChildren,
        codes::TASK_MOVE_INTO_SUBTREE => TextId::RefusalMoveIntoSubtree,
        codes::TASK_COMPUTED_FIELD => TextId::RefusalComputedField,
        codes::ASSIGNMENT_ZERO_UNITS_FIXED_WORK => TextId::RefusalZeroUnitsFixedWork,
        codes::TASK_FIXED_WORK_EFFORT_DRIVEN => TextId::RefusalFixedWorkEffortDriven,
        codes::PROGRESS_SUMMARY_NOT_EDITABLE => TextId::RefusalProgressSummary,
        codes::PROGRESS_INACTIVE => TextId::RefusalProgressInactive,
        codes::PROGRESS_INVALID_ORDER => TextId::RefusalProgressOrder,
        codes::PROGRESS_DURATION_BELOW_ACTUAL => TextId::RefusalDurationBelowActual,
        codes::BASELINE_EXISTS => TextId::RefusalBaselineExists,
        codes::LINK_SELF => TextId::RefusalLinkSelf,
        codes::LINK_SUMMARY_DESCENDANT => TextId::RefusalLinkSummary,
        codes::PARAM_INVALID => TextId::RefusalParamInvalid,
        codes::LIMIT_EXCEEDED => TextId::RefusalLimitExceeded,
        _ => return None,
    })
}

/// Saisie illisible : le morceau fautif s'il est connu (unité, type de
/// lien), sinon « Une valeur saisie n'est pas valable. ».
fn text_sentence(kind: &CommandErrorKind, texts: &Texts) -> String {
    let piece = match kind {
        CommandErrorKind::Text(TextError::UnknownUnit { unit }) => Some(unit.as_str()),
        CommandErrorKind::Text(TextError::BadLinkType { code }) => Some(code.as_str()),
        _ => None,
    };
    match piece {
        Some(piece) => texts.get_args(
            TextId::RefusalTextParse,
            &[("text", Arg::Str(Cow::Borrowed(piece)))],
        ),
        None => texts.get(TextId::RefusalParamInvalid),
    }
}

/// Premier diagnostic d'un lot refusé : boucle de liens nommée par les ID
/// de ses tâches (triés) ; autre diagnostic : son message traduit.
fn diagnostic_sentence(d: &Diagnostic, project: Option<&Project>, texts: &Texts) -> String {
    if let Diagnostic::LinkCycle { tasks } = d {
        let mut ids: Vec<u32> = tasks
            .iter()
            .filter_map(|uid| project.and_then(|p| p.task_id(*uid)))
            .map(plannercraft_core::TaskId::get)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if !ids.is_empty() {
            let list: Vec<String> = ids.iter().map(ToString::to_string).collect();
            return texts.get_args(
                TextId::RefusalLinkCycle,
                &[("tasks", Arg::Str(list.join(", ").into()))],
            );
        }
    }
    let detail = DiagTexts::new(texts).with_project(project).core(d);
    texts.get_args(TextId::RefusalBatchErrors, &[("detail", Arg::Str(detail.into()))])
}

/// Détail repliable : code (et code du premier diagnostic d'un lot
/// refusé), opération, champ, puis texte du moteur.
fn detail(error: &CommandError, texts: &Texts, engine: plannercraft_automation::text::Language) -> String {
    let mut code = error.code.to_owned();
    if let CommandErrorKind::WouldCreateErrors(diags) = &error.kind
        && let Some(first) = diags.first()
    {
        code.push_str(", ");
        code.push_str(first.code());
    }
    let mut head = texts.get_args(TextId::RefusalDetailCode, &[("code", Arg::Str(code.into()))]);
    if let Some(n) = error.op_index {
        head.push_str(", ");
        let n = i64::try_from(n).unwrap_or(i64::MAX);
        head.push_str(&texts.get_args(TextId::RefusalDetailOperation, &[("n", Arg::Count(n))]));
    }
    if let Some(field) = &error.field {
        head.push_str(", ");
        head.push_str(field);
    }
    head.push('\n');
    head.push_str(&error.message(engine));
    head
}

/// Textes des refus en langage d'utilisateur (H-UI-105) : phrase
/// générique, détail (code, opération), tâche supprimée, et une phrase par
/// code du registre que l'interface produit.
pub const L1_TEXTS: [TextId; 37] = [
    TextId::RefusalGeneric,
    TextId::RefusalDetailCode,
    TextId::RefusalDetailOperation,
    TextId::RefusalDeletedTask,
    TextId::RefusalLinkDuplicate,
    TextId::RefusalLinkCycle,
    TextId::RefusalLinkSelf,
    TextId::RefusalLinkSummary,
    TextId::RefusalTaskNotFound,
    TextId::RefusalBatchErrors,
    TextId::RefusalParamInvalid,
    TextId::RefusalLimitExceeded,
    TextId::RefusalTextParse,
    TextId::RefusalCustomField,
    TextId::RefusalOutline,
    TextId::RefusalHasChildren,
    TextId::RefusalMoveIntoSubtree,
    TextId::RefusalAssignmentDuplicate,
    TextId::RefusalZeroUnitsFixedWork,
    TextId::RefusalFixedWorkEffortDriven,
    TextId::RefusalProgressSummary,
    TextId::RefusalProgressInactive,
    TextId::RefusalProgressOrder,
    TextId::RefusalDurationBelowActual,
    TextId::RefusalLinkNotFound,
    TextId::RefusalBaselineExists,
    TextId::RefusalComputedField,
    TextId::RefusalCalendarInUse,
    TextId::RefusalUserProject,
    TextId::RefusalUserTask,
    TextId::RefusalUserResource,
    TextId::RefusalUserCalendar,
    TextId::RefusalCalendarNameTaken,
    TextId::RefusalCalendarOverlap,
    TextId::RefusalCalendarBase,
    TextId::RefusalResourceAssigned,
    TextId::RefusalAssignmentKind,
];

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_automation::error::codes;
    use plannercraft_automation::text::{Language as Engine, TextError};
    use plannercraft_automation::{CommandError, CommandErrorKind};
    use plannercraft_core::{Diagnostic, Subject, TaskUid};

    use super::{UI_CODES, user_message};
    use crate::i18n::{Language, Texts};

    fn uid(n: u32) -> TaskUid {
        TaskUid::new(n)
    }

    /// Erreur représentative d'un code de [`UI_CODES`] (sujets inconnus du
    /// projet absent : tâches dites « supprimées »).
    fn sample(code: &'static str) -> CommandError {
        let kind = match code {
            codes::LINK_DUPLICATE => CommandErrorKind::DuplicateLink,
            codes::LINK_SELF => CommandErrorKind::SelfLink,
            codes::LINK_SUMMARY_DESCENDANT => CommandErrorKind::SummaryDescendantLink,
            codes::TASK_NOT_FOUND => CommandErrorKind::UnknownTask(uid(3)),
            codes::BATCH_WOULD_CREATE_ERRORS => {
                CommandErrorKind::WouldCreateErrors(vec![Diagnostic::LinkCycle {
                    tasks: vec![uid(2), uid(3)],
                }])
            }
            codes::LIMIT_EXCEEDED => CommandErrorKind::Limit { what: "ops", max: 5 },
            codes::PARAM_CONFLICT => CommandErrorKind::ParamConflict("percent_work_complete", "assignments"),
            "text.bad_number" => CommandErrorKind::Text(TextError::BadNumber { at: 2 }),
            _ => CommandErrorKind::InvalidParam("x".to_owned()),
        };
        CommandError::new(code, kind)
            .with_subject(Subject::Link {
                predecessor: uid(2),
                successor: uid(3),
            })
            .with_op_index(4)
    }

    /// Aucune phrase ne cite le code, le numéro d'opération ni les mots du
    /// registre ; le détail les porte.
    #[test]
    fn sentences_hide_codes_detail_keeps_them() {
        for lang in [Language::Fr, Language::En] {
            let texts = Texts::new(lang);
            for code in UI_CODES {
                let error = sample(code);
                let (text, detail) = user_message(&error, None, &texts, Engine::Fr);
                assert!(!text.contains(code), "{code}: {text}");
                assert!(
                    !text.contains("opération") && !text.contains("operation"),
                    "{text}"
                );
                assert!(detail.contains(code), "{code}: {detail}");
            }
        }
    }
}
