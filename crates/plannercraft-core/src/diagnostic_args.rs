//! Arguments neutres des diagnostics, pour leur traduction par code
//! (`docs/ui-needs.md` N5, J8 lot L14).
//!
//! Le `Display` de [`Diagnostic`] reste le message français de référence
//! (CLI, MCP, journaux) et ne change pas. Une interface traduit un
//! diagnostic par son code stable ([`Diagnostic::code`]) et ses arguments
//! ([`Diagnostic::args`]) : valeurs sans langue (UID, entiers, instants,
//! types énumérés), mises en forme par l'interface dans sa langue. Les noms
//! d'arguments sont ceux des champs de la variante.

use crate::diagnostic::{Diagnostic, OverriddenBound, Pass, Subject};
use crate::ids::{AssignmentUid, CalendarUid, ResourceUid, TaskUid};
use crate::model::ConstraintKind;
use crate::time::Moment;

/// Valeur d'un argument de diagnostic, sans langue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagArg {
    /// Entier (compteur, numéro, limite, minutes, pourcentage entier) ;
    /// sert aussi à choisir le pluriel.
    Int(i64),
    /// UID d'un élément (tâche, calendrier, ressource, affectation).
    Uid(u32),
    /// Liste d'UID, dans l'ordre (cycle de liens, chaîne d'héritage).
    Uids(Vec<u32>),
    /// Instant.
    Moment(Moment),
    /// Type de contrainte.
    Constraint(ConstraintKind),
    /// Passe du calcul.
    Pass(Pass),
    /// Élément du projet.
    Subject(Subject),
    /// Borne violée par une contrainte respectée de force.
    Bound(OverriddenBound),
    /// Texte brut, non traduit (cause d'une erreur interne, nom MSPDI d'une
    /// option, nom de profil, message d'un analyseur).
    Text(String),
    /// Position dans un fichier lu (ligne et colonne depuis 1, octet depuis
    /// 0) ; utilisée par les diagnostics d'import.
    Position {
        /// Ligne.
        line: u32,
        /// Colonne, en caractères.
        column: u32,
        /// Décalage en octets.
        byte: u64,
    },
}

impl DiagArg {
    /// Entier depuis un `usize` (saturé, jamais de panique).
    #[must_use]
    pub fn count(n: usize) -> Self {
        Self::Int(i64::try_from(n).unwrap_or(i64::MAX))
    }

    /// Entier depuis un `u64` (saturé).
    #[must_use]
    pub fn count64(n: u64) -> Self {
        Self::Int(i64::try_from(n).unwrap_or(i64::MAX))
    }
}

fn task(t: TaskUid) -> DiagArg {
    DiagArg::Uid(t.get())
}

fn cal(c: CalendarUid) -> DiagArg {
    DiagArg::Uid(c.get())
}

fn res(r: ResourceUid) -> DiagArg {
    DiagArg::Uid(r.get())
}

fn asg(a: AssignmentUid) -> DiagArg {
    DiagArg::Uid(a.get())
}

/// Arguments d'un lien (`predecessor`, `successor`).
fn link(predecessor: TaskUid, successor: TaskUid) -> Vec<(&'static str, DiagArg)> {
    vec![("predecessor", task(predecessor)), ("successor", task(successor))]
}

impl Diagnostic {
    /// Arguments nommés du diagnostic (noms des champs de la variante),
    /// sans langue ; voir le module.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "un bras par variante, rien à factoriser")]
    pub fn args(&self) -> Vec<(&'static str, DiagArg)> {
        match self {
            Self::CalendarUnknown {
                calendar,
                referenced_by,
            } => vec![
                ("calendar", cal(*calendar)),
                ("referenced_by", DiagArg::Subject(*referenced_by)),
            ],
            Self::CalendarBaseMissing { calendar, base } | Self::CalendarBaseNotBase { calendar, base } => {
                vec![("calendar", cal(*calendar)), ("base", cal(*base))]
            }
            Self::CalendarInheritanceCycle { chain } => {
                vec![("chain", DiagArg::Uids(chain.iter().map(|c| c.get()).collect()))]
            }
            Self::CalendarNoWorkingTime { calendar }
            | Self::CalendarInternal { calendar }
            | Self::ProjectCalendarNotBase { calendar }
            | Self::ProjectCalendarFallback { calendar } => vec![("calendar", cal(*calendar))],
            Self::CalendarExceptionsOverlap {
                calendar,
                kept,
                ignored,
            }
            | Self::CalendarWorkWeeksOverlap {
                calendar,
                kept,
                ignored,
            } => vec![
                ("calendar", cal(*calendar)),
                ("kept", DiagArg::count(*kept)),
                ("ignored", DiagArg::count(*ignored)),
            ],
            Self::CalendarRecurrenceUnsupported { calendar, exception } => vec![
                ("calendar", cal(*calendar)),
                ("exception", DiagArg::count(*exception)),
            ],
            Self::TaskReservedUid => Vec::new(),
            Self::TaskOutlineJump { task: t }
            | Self::TaskNullWithData { task: t }
            | Self::TaskNegativeProgress { task: t }
            | Self::LinkSelf { task: t }
            | Self::TaskManualUndated { task: t }
            | Self::TaskManualDatesInverted { task: t }
            | Self::TaskFixedWorkEffortDriven { task: t }
            | Self::ProgressFromFinishInProgress { task: t }
            | Self::ProgressActualStartMissing { task: t }
            | Self::ProgressMilestoneRounded { task: t } => vec![("task", task(*t))],
            Self::LinkDuplicate {
                predecessor,
                successor,
            }
            | Self::LinkUnknownTask {
                predecessor,
                successor,
            }
            | Self::LinkIgnoredInCycle {
                predecessor,
                successor,
            }
            | Self::LinkSummaryDescendant {
                predecessor,
                successor,
            }
            | Self::LinkFinishToSummary {
                predecessor,
                successor,
            }
            | Self::LinkStartFromSummary {
                predecessor,
                successor,
            }
            | Self::LinkInactive {
                predecessor,
                successor,
            }
            | Self::LinkExcludedEndpoint {
                predecessor,
                successor,
            }
            | Self::LinkManualNotHonored {
                predecessor,
                successor,
            }
            | Self::ProgressLinkIgnored {
                predecessor,
                successor,
            }
            | Self::ProgressLinkOverridden {
                predecessor,
                successor,
            } => link(*predecessor, *successor),
            Self::AssignmentUnknownTask { assignment, task: t } => {
                vec![("assignment", asg(*assignment)), ("task", task(*t))]
            }
            Self::AssignmentUnknownResource { assignment, resource } => {
                vec![("assignment", asg(*assignment)), ("resource", res(*resource))]
            }
            Self::ConversionOutOfDomain {
                calendar,
                from,
                minutes,
            } => vec![
                ("calendar", cal(*calendar)),
                ("from", DiagArg::Moment(*from)),
                ("minutes", DiagArg::Int(minutes.get())),
            ],
            Self::LinkCycle { tasks } => {
                vec![("tasks", DiagArg::Uids(tasks.iter().map(|t| t.get()).collect()))]
            }
            Self::LinkExpansionLimit { limit, ignored } => vec![
                ("limit", DiagArg::count64(*limit)),
                ("ignored", DiagArg::count64(*ignored)),
            ],
            Self::LinkUnschedulable {
                predecessor,
                successor,
                cause,
            } => {
                let mut args = link(*predecessor, *successor);
                args.push(("cause", DiagArg::Text(cause.clone())));
                args
            }
            Self::TaskCalendarFallback { task: t, calendar } => {
                vec![("task", task(*t)), ("calendar", cal(*calendar))]
            }
            Self::TaskUnschedulable { task: t, pass, cause } => vec![
                ("task", task(*t)),
                ("pass", DiagArg::Pass(*pass)),
                ("cause", DiagArg::Text(cause.clone())),
            ],
            Self::ConstraintOverride { task: t, kind, bound } => vec![
                ("task", task(*t)),
                ("kind", DiagArg::Constraint(*kind)),
                ("bound", DiagArg::Bound(*bound)),
            ],
            Self::ConstraintNotHonored { task: t, kind, date } => vec![
                ("task", task(*t)),
                ("kind", DiagArg::Constraint(*kind)),
                ("date", DiagArg::Moment(*date)),
            ],
            Self::ConstraintSummaryIgnored { task: t, kind }
            | Self::ConstraintManualIgnored { task: t, kind }
            | Self::ConstraintIgnoredInProgress { task: t, kind } => {
                vec![("task", task(*t)), ("kind", DiagArg::Constraint(*kind))]
            }
            Self::TaskDeadlineMissed {
                task: t,
                deadline,
                finish,
            } => vec![
                ("task", task(*t)),
                ("deadline", DiagArg::Moment(*deadline)),
                ("finish", DiagArg::Moment(*finish)),
            ],
            Self::ResourceOverallocated {
                resource,
                from,
                to,
                peak,
            } => vec![
                ("resource", res(*resource)),
                ("from", DiagArg::Moment(*from)),
                ("to", DiagArg::Moment(*to)),
                // Pourcentage entier arrondi vers zéro, comme le `Display`.
                ("peak", DiagArg::Int(peak.get().checked_div(100).unwrap_or(0))),
            ],
            Self::ResourceAvailabilityIgnored { resource } => vec![("resource", res(*resource))],
            Self::AssignmentContourUnsupported { assignment, contour } => vec![
                ("assignment", asg(*assignment)),
                ("contour", DiagArg::Text(format!("{contour:?}"))),
            ],
            Self::AssignmentZeroUnits { assignment } | Self::AssignmentDurationOutOfDomain { assignment } => {
                vec![("assignment", asg(*assignment))]
            }
            Self::AssignmentCalendarEmpty {
                assignment,
                task_calendar,
                resource_calendar,
            } => vec![
                ("assignment", asg(*assignment)),
                ("task_calendar", cal(*task_calendar)),
                ("resource_calendar", cal(*resource_calendar)),
            ],
            Self::CalendarTooManySegments {
                calendar,
                other,
                limit,
            } => vec![
                ("calendar", cal(*calendar)),
                ("other", cal(*other)),
                ("limit", DiagArg::Int(i64::from(*limit))),
            ],
            Self::AssignmentTooManyParts {
                task: t,
                parts,
                limit,
            } => vec![
                ("task", task(*t)),
                ("parts", DiagArg::Int(i64::from(*parts))),
                ("limit", DiagArg::Int(i64::from(*limit))),
            ],
            Self::ProgressOptionUnsupported { option } => {
                vec![("option", DiagArg::Text(option.mspdi_name().to_owned()))]
            }
            Self::BaselineCostNotComputed { number } | Self::BaselineNothingToClear { number } => {
                vec![("number", DiagArg::Int(i64::from(number.get())))]
            }
            Self::BaselineTaskSkipped { count } => vec![("count", DiagArg::Int(i64::from(*count)))],
            Self::ScheduleInternal { detail } => vec![("detail", DiagArg::Text(detail.clone()))],
            Self::TaskCustomInvalid { task: t, field } => vec![
                ("task", task(*t)),
                ("field", DiagArg::Text(field.canonical_name())),
            ],
        }
    }

    /// Un exemple de chaque variante (UID et valeurs arbitraires), dans
    /// l'ordre de déclaration : sert aux contrôles de couverture des
    /// traductions (un code sans message traduit est détecté par un test).
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "un exemple par variante")]
    pub fn examples() -> Vec<Self> {
        use crate::model::{LinkType, ProgressOption, WorkContour};
        use crate::quantity::Units;
        use crate::{BaselineNumber, Minutes};
        let (t1, t2, t3) = (TaskUid::new(1), TaskUid::new(2), TaskUid::new(3));
        let (c1, c2) = (CalendarUid::new(1), CalendarUid::new(2));
        let a = AssignmentUid::new(4);
        let r = ResourceUid::new(5);
        let m = Moment::MIN;
        let number = BaselineNumber::new(1).unwrap_or(BaselineNumber::PRIMARY);
        vec![
            Self::CalendarUnknown {
                calendar: c2,
                referenced_by: Subject::Task(t1),
            },
            Self::CalendarBaseMissing {
                calendar: c1,
                base: c2,
            },
            Self::CalendarBaseNotBase {
                calendar: c1,
                base: c2,
            },
            Self::CalendarInheritanceCycle {
                chain: vec![c1, c2, c1],
            },
            Self::CalendarNoWorkingTime { calendar: c1 },
            Self::CalendarInternal { calendar: c1 },
            Self::CalendarExceptionsOverlap {
                calendar: c1,
                kept: 1,
                ignored: 2,
            },
            Self::CalendarWorkWeeksOverlap {
                calendar: c1,
                kept: 1,
                ignored: 2,
            },
            Self::CalendarRecurrenceUnsupported {
                calendar: c1,
                exception: 3,
            },
            Self::ProjectCalendarNotBase { calendar: c1 },
            Self::TaskReservedUid,
            Self::TaskOutlineJump { task: t1 },
            Self::TaskNullWithData { task: t1 },
            Self::TaskNegativeProgress { task: t1 },
            Self::LinkSelf { task: t1 },
            Self::LinkDuplicate {
                predecessor: t1,
                successor: t2,
            },
            Self::LinkUnknownTask {
                predecessor: t1,
                successor: t2,
            },
            Self::AssignmentUnknownTask {
                assignment: a,
                task: t1,
            },
            Self::AssignmentUnknownResource {
                assignment: a,
                resource: r,
            },
            Self::ConversionOutOfDomain {
                calendar: c1,
                from: m,
                minutes: Minutes::new(480),
            },
            Self::LinkCycle {
                tasks: vec![t1, t2, t3],
            },
            Self::LinkIgnoredInCycle {
                predecessor: t1,
                successor: t2,
            },
            Self::LinkSummaryDescendant {
                predecessor: t1,
                successor: t2,
            },
            Self::LinkFinishToSummary {
                predecessor: t1,
                successor: t2,
            },
            Self::LinkExpansionLimit {
                limit: 100,
                ignored: 2,
            },
            Self::LinkStartFromSummary {
                predecessor: t1,
                successor: t2,
            },
            Self::LinkInactive {
                predecessor: t1,
                successor: t2,
            },
            Self::LinkExcludedEndpoint {
                predecessor: t1,
                successor: t2,
            },
            Self::LinkManualNotHonored {
                predecessor: t1,
                successor: t2,
            },
            Self::LinkUnschedulable {
                predecessor: t1,
                successor: t2,
                cause: "x".to_owned(),
            },
            Self::TaskManualUndated { task: t1 },
            Self::TaskManualDatesInverted { task: t1 },
            Self::TaskCalendarFallback {
                task: t1,
                calendar: c2,
            },
            Self::ProjectCalendarFallback { calendar: c1 },
            Self::TaskUnschedulable {
                task: t1,
                pass: Pass::Forward,
                cause: "x".to_owned(),
            },
            Self::ConstraintOverride {
                task: t1,
                kind: ConstraintKind::MustStartOn,
                bound: OverriddenBound::Predecessor {
                    predecessor: t2,
                    link: LinkType::FinishToStart,
                },
            },
            Self::ConstraintNotHonored {
                task: t1,
                kind: ConstraintKind::StartNoLaterThan,
                date: m,
            },
            Self::ConstraintSummaryIgnored {
                task: t1,
                kind: ConstraintKind::MustFinishOn,
            },
            Self::ConstraintManualIgnored {
                task: t1,
                kind: ConstraintKind::StartNoEarlierThan,
            },
            Self::TaskDeadlineMissed {
                task: t1,
                deadline: m,
                finish: m,
            },
            Self::ResourceOverallocated {
                resource: r,
                from: m,
                to: m,
                peak: Units::new(15_000).unwrap_or(Units::FULL),
            },
            Self::ResourceAvailabilityIgnored { resource: r },
            Self::AssignmentContourUnsupported {
                assignment: a,
                contour: WorkContour::BackLoaded,
            },
            Self::AssignmentZeroUnits { assignment: a },
            Self::AssignmentDurationOutOfDomain { assignment: a },
            Self::AssignmentCalendarEmpty {
                assignment: a,
                task_calendar: c1,
                resource_calendar: c2,
            },
            Self::TaskFixedWorkEffortDriven { task: t1 },
            Self::CalendarTooManySegments {
                calendar: c1,
                other: c2,
                limit: 10_000,
            },
            Self::AssignmentTooManyParts {
                task: t1,
                parts: 70,
                limit: 64,
            },
            Self::ProgressLinkIgnored {
                predecessor: t1,
                successor: t2,
            },
            Self::ProgressLinkOverridden {
                predecessor: t1,
                successor: t2,
            },
            Self::ProgressOptionUnsupported {
                option: ProgressOption::MoveCompletedEndsForward,
            },
            Self::ProgressFromFinishInProgress { task: t1 },
            Self::ProgressActualStartMissing { task: t1 },
            Self::ProgressMilestoneRounded { task: t1 },
            Self::ConstraintIgnoredInProgress {
                task: t1,
                kind: ConstraintKind::FinishNoLaterThan,
            },
            Self::BaselineCostNotComputed { number },
            Self::BaselineTaskSkipped { count: 2 },
            Self::BaselineNothingToClear { number },
            Self::ScheduleInternal {
                detail: "x".to_owned(),
            },
            Self::TaskCustomInvalid {
                task: t1,
                field: crate::model::CustomFieldId::TEXT1,
            },
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
mod tests {
    use std::collections::BTreeSet;

    use super::{DiagArg, Diagnostic};

    /// Rang de la variante : `match` sans joker, un ajout de variante casse
    /// la compilation tant que [`Diagnostic::examples`] n'est pas complété.
    #[allow(clippy::too_many_lines, reason = "un bras par variante")]
    const fn rank(d: &Diagnostic) -> usize {
        use Diagnostic as D;
        match d {
            D::CalendarUnknown { .. } => 0,
            D::CalendarBaseMissing { .. } => 1,
            D::CalendarBaseNotBase { .. } => 2,
            D::CalendarInheritanceCycle { .. } => 3,
            D::CalendarNoWorkingTime { .. } => 4,
            D::CalendarInternal { .. } => 5,
            D::CalendarExceptionsOverlap { .. } => 6,
            D::CalendarWorkWeeksOverlap { .. } => 7,
            D::CalendarRecurrenceUnsupported { .. } => 8,
            D::ProjectCalendarNotBase { .. } => 9,
            D::TaskReservedUid => 10,
            D::TaskOutlineJump { .. } => 11,
            D::TaskNullWithData { .. } => 12,
            D::TaskNegativeProgress { .. } => 13,
            D::LinkSelf { .. } => 14,
            D::LinkDuplicate { .. } => 15,
            D::LinkUnknownTask { .. } => 16,
            D::AssignmentUnknownTask { .. } => 17,
            D::AssignmentUnknownResource { .. } => 18,
            D::ConversionOutOfDomain { .. } => 19,
            D::LinkCycle { .. } => 20,
            D::LinkIgnoredInCycle { .. } => 21,
            D::LinkSummaryDescendant { .. } => 22,
            D::LinkFinishToSummary { .. } => 23,
            D::LinkExpansionLimit { .. } => 24,
            D::LinkStartFromSummary { .. } => 25,
            D::LinkInactive { .. } => 26,
            D::LinkExcludedEndpoint { .. } => 27,
            D::LinkManualNotHonored { .. } => 28,
            D::LinkUnschedulable { .. } => 29,
            D::TaskManualUndated { .. } => 30,
            D::TaskManualDatesInverted { .. } => 31,
            D::TaskCalendarFallback { .. } => 32,
            D::ProjectCalendarFallback { .. } => 33,
            D::TaskUnschedulable { .. } => 34,
            D::ConstraintOverride { .. } => 35,
            D::ConstraintNotHonored { .. } => 36,
            D::ConstraintSummaryIgnored { .. } => 37,
            D::ConstraintManualIgnored { .. } => 38,
            D::TaskDeadlineMissed { .. } => 39,
            D::ResourceOverallocated { .. } => 40,
            D::ResourceAvailabilityIgnored { .. } => 41,
            D::AssignmentContourUnsupported { .. } => 42,
            D::AssignmentZeroUnits { .. } => 43,
            D::AssignmentDurationOutOfDomain { .. } => 44,
            D::AssignmentCalendarEmpty { .. } => 45,
            D::TaskFixedWorkEffortDriven { .. } => 46,
            D::CalendarTooManySegments { .. } => 47,
            D::AssignmentTooManyParts { .. } => 48,
            D::ProgressLinkIgnored { .. } => 49,
            D::ProgressLinkOverridden { .. } => 50,
            D::ProgressOptionUnsupported { .. } => 51,
            D::ProgressFromFinishInProgress { .. } => 52,
            D::ProgressActualStartMissing { .. } => 53,
            D::ProgressMilestoneRounded { .. } => 54,
            D::ConstraintIgnoredInProgress { .. } => 55,
            D::BaselineCostNotComputed { .. } => 56,
            D::BaselineTaskSkipped { .. } => 57,
            D::BaselineNothingToClear { .. } => 58,
            D::ScheduleInternal { .. } => 59,
            D::TaskCustomInvalid { .. } => 60,
        }
    }

    /// Un exemple par variante, dans l'ordre ; codes tous distincts.
    #[test]
    fn n5_examples_cover_every_variant() {
        let examples = Diagnostic::examples();
        let ranks: Vec<usize> = examples.iter().map(rank).collect();
        assert_eq!(ranks, (0..61).collect::<Vec<_>>());
        let codes: BTreeSet<&str> = examples.iter().map(Diagnostic::code).collect();
        assert_eq!(codes.len(), examples.len());
    }

    /// Les noms d'arguments sont distincts ; un message qui cite un UID ou
    /// un nombre dans son `Display` le porte en argument.
    #[test]
    fn n5_args_named_and_complete() {
        for d in Diagnostic::examples() {
            let args = d.args();
            let names: BTreeSet<&str> = args.iter().map(|(n, _)| *n).collect();
            assert_eq!(names.len(), args.len(), "{}", d.code());
            let text = d.to_string();
            // « UID 0 » de `task.reserved_uid` est une constante du message.
            let has_digit = text.chars().any(|c| c.is_ascii_digit()) && d != Diagnostic::TaskReservedUid;
            assert!(!has_digit || !args.is_empty(), "{} : {text}", d.code());
        }
        let d = Diagnostic::ResourceOverallocated {
            resource: crate::ResourceUid::new(5),
            from: crate::Moment::MIN,
            to: crate::Moment::MIN,
            peak: crate::Units::new(15_099).unwrap(),
        };
        assert!(d.args().contains(&("peak", DiagArg::Int(150))));
        assert_eq!(DiagArg::count(usize::MAX), DiagArg::Int(i64::MAX));
    }
}
