//! Diagnostics typés (ADR-0005, ADR-0008 § 7.2).
//!
//! Trois niveaux de traitement des erreurs :
//! - chemin chaud : `Result<_, CalendarError>` ;
//! - commandes (J4) : refus de toute opération qui produirait un diagnostic
//!   d'erreur ;
//! - import et recalcul : ils n'échouent jamais et collectent des
//!   diagnostics.
//!
//! La gravité et le sujet se déduisent de la variante : aucune combinaison
//! incohérente n'est représentable. Le code est stable (`calendar.unknown`) ;
//! les traductions (J8) partent du code et des champs. D'autres variantes
//! s'ajouteront (cycle de liens, conflit de contrainte, surutilisation,
//! troncature à l'import).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::duration::Minutes;
use crate::error::CalendarError;
use crate::ids::{AssignmentUid, BaselineNumber, CalendarUid, ResourceUid, TaskUid};
use crate::model::{ConstraintKind, CustomFieldId, LinkType, ProgressOption, WorkContour};
use crate::quantity::Units;
use crate::time::Moment;

/// Gravité d'un diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Le calcul est impossible ou faux pour le sujet.
    Error,
    /// Le calcul se fait, avec une hypothèse signalée.
    Warning,
    /// Information.
    Info,
}

/// Élément du projet concerné par un diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subject {
    /// Le projet.
    Project,
    /// Un calendrier.
    Calendar(CalendarUid),
    /// Une tâche.
    Task(TaskUid),
    /// Un lien.
    Link {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur.
        successor: TaskUid,
    },
    /// Une ressource.
    Resource(ResourceUid),
    /// Une affectation.
    Assignment(AssignmentUid),
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project => f.write_str("le projet"),
            Self::Calendar(c) => write!(f, "le calendrier {c}"),
            Self::Task(t) => write!(f, "la tâche {t}"),
            Self::Link {
                predecessor,
                successor,
            } => write!(f, "le lien {predecessor} → {successor}"),
            Self::Resource(r) => write!(f, "la ressource {r}"),
            Self::Assignment(a) => write!(f, "l'affectation {a}"),
        }
    }
}

/// Passe du calcul où une conversion a échoué (`task.unschedulable`, R-069).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pass {
    /// Passe avant (dates au plus tôt).
    Forward,
    /// Passe arrière (dates au plus tard).
    Backward,
}

impl fmt::Display for Pass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Forward => "passe avant",
            Self::Backward => "passe arrière",
        })
    }
}

/// Borne violée par une contrainte respectée de force (`constraint.override`,
/// R-100) : une par borne, pour savoir quel lien ou quelle date cède.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OverriddenBound {
    /// Début du projet (R-036, R-105).
    ProjectStart,
    /// Fin du projet (planification à rebours).
    ProjectFinish,
    /// Porte d'une récapitulative ancêtre (R-056).
    SummaryGate {
        /// Récapitulative.
        summary: TaskUid,
    },
    /// Plafond hérité d'une récapitulative ancêtre (R-057).
    SummaryCap {
        /// Récapitulative.
        summary: TaskUid,
    },
    /// Lien depuis un prédécesseur.
    Predecessor {
        /// Prédécesseur d'origine.
        predecessor: TaskUid,
        /// Type du lien.
        link: LinkType,
    },
    /// Lien vers un successeur (planification à rebours).
    Successor {
        /// Successeur d'origine.
        successor: TaskUid,
        /// Type du lien.
        link: LinkType,
    },
}

impl fmt::Display for OverriddenBound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProjectStart => f.write_str("le début du projet"),
            Self::ProjectFinish => f.write_str("la fin du projet"),
            Self::SummaryGate { summary } => write!(f, "la borne de la récapitulative {summary}"),
            Self::SummaryCap { summary } => write!(f, "le plafond de la récapitulative {summary}"),
            Self::Predecessor { predecessor, .. } => write!(f, "le lien depuis la tâche {predecessor}"),
            Self::Successor { successor, .. } => write!(f, "le lien vers la tâche {successor}"),
        }
    }
}

/// Diagnostic. Les identifiants cités sont des UID.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum Diagnostic {
    /// Calendrier référencé mais inexistant (erreur).
    #[serde(rename = "calendar.unknown")]
    CalendarUnknown {
        /// UID référencé.
        calendar: CalendarUid,
        /// Élément qui le référence.
        referenced_by: Subject,
    },
    /// Base d'un calendrier dérivé inexistante (erreur).
    #[serde(rename = "calendar.base_missing")]
    CalendarBaseMissing {
        /// Calendrier dérivé.
        calendar: CalendarUid,
        /// Base introuvable.
        base: CalendarUid,
    },
    /// Base d'un calendrier dérivé qui n'est pas un calendrier de base (erreur).
    #[serde(rename = "calendar.base_not_base")]
    CalendarBaseNotBase {
        /// Calendrier dérivé.
        calendar: CalendarUid,
        /// Base dérivée.
        base: CalendarUid,
    },
    /// Boucle d'héritage (erreur).
    #[serde(rename = "calendar.inheritance_cycle")]
    CalendarInheritanceCycle {
        /// Chemin complet, premier UID répété en fin.
        chain: Vec<CalendarUid>,
    },
    /// Calendrier sans temps ouvré : conversion impossible (erreur).
    #[serde(rename = "calendar.no_working_time")]
    CalendarNoWorkingTime {
        /// Calendrier.
        calendar: CalendarUid,
    },
    /// Calendrier dont la compilation a échoué sur un invariant interne
    /// (`CalendarError::Inconsistent`) : jamais attendu, anomalie à signaler
    /// (erreur).
    #[serde(rename = "calendar.internal")]
    CalendarInternal {
        /// Calendrier.
        calendar: CalendarUid,
    },
    /// Exceptions qui se chevauchent : la première l'emporte (avertissement).
    #[serde(rename = "calendar.exceptions_overlap")]
    CalendarExceptionsOverlap {
        /// Calendrier.
        calendar: CalendarUid,
        /// Indice de l'exception retenue.
        kept: usize,
        /// Indice de l'exception ignorée sur les jours communs.
        ignored: usize,
    },
    /// Semaines datées qui se chevauchent : la première l'emporte
    /// (avertissement).
    #[serde(rename = "calendar.work_weeks_overlap")]
    CalendarWorkWeeksOverlap {
        /// Calendrier.
        calendar: CalendarUid,
        /// Indice de la semaine retenue.
        kept: usize,
        /// Indice de la semaine ignorée sur les jours communs.
        ignored: usize,
    },
    /// Récurrence de sémantique non documentée, ignorée (avertissement).
    #[serde(rename = "calendar.recurrence_unsupported")]
    CalendarRecurrenceUnsupported {
        /// Calendrier.
        calendar: CalendarUid,
        /// Indice de l'exception.
        exception: usize,
    },
    /// Calendrier du projet dérivé (avertissement).
    #[serde(rename = "project.calendar_not_base")]
    ProjectCalendarNotBase {
        /// Calendrier du projet.
        calendar: CalendarUid,
    },
    /// UID 0 dans la liste des tâches (erreur).
    #[serde(rename = "task.reserved_uid")]
    TaskReservedUid,
    /// Saut de niveau hiérarchique : niveau > précédent + 1, ou premier
    /// niveau différent de 1 (erreur).
    #[serde(rename = "task.outline_jump")]
    TaskOutlineJump {
        /// Tâche.
        task: TaskUid,
    },
    /// Ligne vide porteuse de liens ou d'affectations (avertissement).
    #[serde(rename = "task.null_with_data")]
    TaskNullWithData {
        /// Tâche.
        task: TaskUid,
    },
    /// Durée réelle ou restante négative (erreur).
    #[serde(rename = "task.negative_progress")]
    TaskNegativeProgress {
        /// Tâche.
        task: TaskUid,
    },
    /// Tâche liée à elle-même (erreur).
    #[serde(rename = "link.self")]
    LinkSelf {
        /// Tâche.
        task: TaskUid,
    },
    /// Plusieurs liens entre les deux mêmes tâches (erreur).
    #[serde(rename = "link.duplicate")]
    LinkDuplicate {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur.
        successor: TaskUid,
    },
    /// Lien vers une tâche inexistante (erreur).
    #[serde(rename = "link.unknown_task")]
    LinkUnknownTask {
        /// Prédécesseur introuvable.
        predecessor: TaskUid,
        /// Successeur.
        successor: TaskUid,
    },
    /// Affectation à une tâche inexistante (erreur).
    #[serde(rename = "assignment.unknown_task")]
    AssignmentUnknownTask {
        /// Affectation.
        assignment: AssignmentUid,
        /// Tâche introuvable.
        task: TaskUid,
    },
    /// Affectation d'une ressource inexistante (erreur).
    #[serde(rename = "assignment.unknown_resource")]
    AssignmentUnknownResource {
        /// Affectation.
        assignment: AssignmentUid,
        /// Ressource introuvable.
        resource: ResourceUid,
    },
    /// Conversion sortie du domaine des dates (erreur, émis par le calcul, J2).
    #[serde(rename = "conversion.out_of_domain")]
    ConversionOutOfDomain {
        /// Calendrier.
        calendar: CalendarUid,
        /// Instant de départ.
        from: Moment,
        /// Minutes à ajouter.
        minutes: Minutes,
    },
    /// Liens circulaires entre ces tâches (erreur, R-065) : un diagnostic par
    /// composante fortement connexe du réseau.
    #[serde(rename = "link.cycle")]
    LinkCycle {
        /// Tâches de la composante, triées.
        tasks: Vec<TaskUid>,
    },
    /// Lien ignoré pour rompre un cycle (avertissement, R-065).
    #[serde(rename = "link.ignored_in_cycle")]
    LinkIgnoredInCycle {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur.
        successor: TaskUid,
    },
    /// Lien entre une récapitulative et l'une de ses descendantes, ignoré
    /// (erreur, R-059).
    #[serde(rename = "link.summary_descendant")]
    LinkSummaryDescendant {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur.
        successor: TaskUid,
    },
    /// Lien FF ou SF vers une récapitulative, déplié sur ses sous-tâches
    /// (avertissement, R-058).
    #[serde(rename = "link.finish_to_summary")]
    LinkFinishToSummary {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur (récapitulative).
        successor: TaskUid,
    },
    /// Liens FF ou SF vers une récapitulative ignorés : leur dépliage sur
    /// les feuilles (R-058) dépasserait le budget global d'arcs dépliés
    /// (avertissement, émis une fois ; règle 10).
    #[serde(rename = "link.expansion_limit")]
    LinkExpansionLimit {
        /// Budget d'arcs dépliés.
        limit: u64,
        /// Nombre de liens ignorés.
        ignored: u64,
    },
    /// Lien SS ou SF depuis une récapitulative, à rebours : son plafond de
    /// début s'applique à toutes ses sous-tâches (avertissement, R-110,
    /// miroir de R-058).
    #[serde(rename = "link.start_from_summary")]
    LinkStartFromSummary {
        /// Prédécesseur (récapitulative).
        predecessor: TaskUid,
        /// Successeur.
        successor: TaskUid,
    },
    /// Lien dont le prédécesseur est inactif : sans effet (info, R-062).
    #[serde(rename = "link.inactive")]
    LinkInactive {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur.
        successor: TaskUid,
    },
    /// Lien dont une extrémité n'est pas planifiée (ligne vide, tâche
    /// manuelle sans date, récapitulative vide, tâche non planifiable) :
    /// ignoré (info, R-063, R-069).
    #[serde(rename = "link.excluded_endpoint")]
    LinkExcludedEndpoint {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur.
        successor: TaskUid,
    },
    /// Lien vers une tâche manuelle que ses dates saisies ne respectent pas
    /// (avertissement, R-063).
    #[serde(rename = "link.manual_not_honored")]
    LinkManualNotHonored {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur (tâche manuelle).
        successor: TaskUid,
    },
    /// Lien vers une récapitulative dont la borne ne se calcule pas
    /// (conversion impossible, décalage hors domaine) : ignoré, la
    /// récapitulative et ses sous-tâches restent planifiées sans lui,
    /// dégradées (erreur, R-069).
    #[serde(rename = "link.unschedulable")]
    LinkUnschedulable {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur (récapitulative).
        successor: TaskUid,
        /// Code stable de l'erreur de conversion (`CalendarError::code`).
        cause: String,
    },
    /// Tâche manuelle sans début ni fin : non planifiée (info, R-063).
    #[serde(rename = "task.manual_undated")]
    TaskManualUndated {
        /// Tâche.
        task: TaskUid,
    },
    /// Tâche manuelle dont la fin saisie précède le début : fin ramenée au
    /// début (erreur, R-063).
    #[serde(rename = "task.manual_dates_inverted")]
    TaskManualDatesInverted {
        /// Tâche.
        task: TaskUid,
    },
    /// Calendrier propre d'une tâche inutilisable : calendrier du projet
    /// utilisé (avertissement, R-066).
    #[serde(rename = "task.calendar_fallback")]
    TaskCalendarFallback {
        /// Tâche.
        task: TaskUid,
        /// Calendrier inutilisable.
        calendar: CalendarUid,
    },
    /// Calendrier du projet inutilisable : calendrier Standard intégré
    /// utilisé (erreur, R-066).
    #[serde(rename = "project.calendar_fallback")]
    ProjectCalendarFallback {
        /// Calendrier inutilisable.
        calendar: CalendarUid,
    },
    /// Conversion impossible pendant le calcul d'une tâche (erreur, R-069).
    #[serde(rename = "task.unschedulable")]
    TaskUnschedulable {
        /// Tâche.
        task: TaskUid,
        /// Passe du calcul.
        pass: Pass,
        /// Code stable de l'erreur de conversion (`CalendarError::code`).
        cause: String,
    },
    /// Contrainte respectée de force malgré une borne (option « respecter
    /// les contraintes » active) : un diagnostic par borne violée ; le lien
    /// violé reste et transmet une marge négative (avertissement, R-100).
    #[serde(rename = "constraint.override")]
    ConstraintOverride {
        /// Tâche contrainte.
        task: TaskUid,
        /// Type de la contrainte.
        kind: ConstraintKind,
        /// Borne violée.
        bound: OverriddenBound,
    },
    /// Contrainte non tenue : l'option « respecter les contraintes » est
    /// inactive et les liens l'emportent (avertissement, R-099).
    #[serde(rename = "constraint.not_honored")]
    ConstraintNotHonored {
        /// Tâche contrainte.
        task: TaskUid,
        /// Type de la contrainte.
        kind: ConstraintKind,
        /// Date saisie de la contrainte.
        date: Moment,
    },
    /// Contrainte d'une récapitulative sans effet (MSO, MFO, SNLT, FNET) :
    /// ignorée (avertissement, R-103).
    #[serde(rename = "constraint.summary_ignored")]
    ConstraintSummaryIgnored {
        /// Récapitulative.
        task: TaskUid,
        /// Type de la contrainte.
        kind: ConstraintKind,
    },
    /// Contrainte d'une tâche manuelle : stockée, ignorée par le calcul
    /// (info, R-104).
    #[serde(rename = "constraint.manual_ignored")]
    ConstraintManualIgnored {
        /// Tâche manuelle.
        task: TaskUid,
        /// Type de la contrainte.
        kind: ConstraintKind,
    },
    /// Échéance dépassée : la fin planifiée tombe après l'échéance, en
    /// temps ouvré de l'échelle de placement (avertissement, R-102).
    #[serde(rename = "task.deadline_missed")]
    TaskDeadlineMissed {
        /// Tâche.
        task: TaskUid,
        /// Échéance saisie.
        deadline: Moment,
        /// Fin planifiée.
        finish: Moment,
    },
    /// Ressource surutilisée : un intervalle travaillé dépasse sa capacité
    /// (avertissement, R-186) ; un diagnostic par ressource, sur son premier
    /// intervalle.
    #[serde(rename = "resource.overallocated")]
    ResourceOverallocated {
        /// Ressource.
        resource: ResourceUid,
        /// Début du premier intervalle de surutilisation.
        from: Moment,
        /// Fin de cet intervalle.
        to: Moment,
        /// Pic d'unités effectives sur l'intervalle (1/10 000).
        peak: Units,
    },
    /// Disponibilités datées (`AvailabilityPeriods`) ignorées : capacité
    /// constante `max_units` (info, R-187).
    #[serde(rename = "resource.availability_ignored")]
    ResourceAvailabilityIgnored {
        /// Ressource.
        resource: ResourceUid,
    },
    /// Profil de travail non uniforme, placé comme uniforme (info, R-180).
    #[serde(rename = "assignment.contour_unsupported")]
    AssignmentContourUnsupported {
        /// Affectation.
        assignment: AssignmentUid,
        /// Profil enregistré.
        contour: WorkContour,
    },
    /// Affectation à unités nulles : durée de la tâche gardée
    /// (avertissement, R-181).
    #[serde(rename = "assignment.zero_units")]
    AssignmentZeroUnits {
        /// Affectation.
        assignment: AssignmentUid,
    },
    /// Durée d'affectation `d_a` hors du domaine (erreur, R-170, R-181).
    #[serde(rename = "assignment.duration_out_of_domain")]
    AssignmentDurationOutOfDomain {
        /// Affectation.
        assignment: AssignmentUid,
    },
    /// Calendrier effectif sans temps ouvré (intersection vide) : repli
    /// sur le calendrier de la tâche (avertissement, R-171).
    #[serde(rename = "assignment.calendar_empty")]
    AssignmentCalendarEmpty {
        /// Affectation.
        assignment: AssignmentUid,
        /// Calendrier de la tâche.
        task_calendar: CalendarUid,
        /// Calendrier de la ressource.
        resource_calendar: CalendarUid,
    },
    /// Tâche à travail fixe non pilotée par l'effort, lue telle quelle
    /// (avertissement à l'import, R-193).
    #[serde(rename = "task.fixed_work_effort_driven")]
    TaskFixedWorkEffortDriven {
        /// Tâche.
        task: TaskUid,
    },
    /// Intersection ou union de deux calendriers trop morcelée, ou
    /// calendrier du modèle hors du budget de compilation (`calendar` =
    /// `other`) : plafond de segments atteint (avertissement, R-172).
    #[serde(rename = "calendar.too_many_segments")]
    CalendarTooManySegments {
        /// Premier calendrier.
        calendar: CalendarUid,
        /// Second calendrier.
        other: CalendarUid,
        /// Plafond de segments.
        limit: u32,
    },
    /// Feuille à trop de parts pour la mesure exacte de l'union de leurs
    /// intervalles : durée calculée = plus grand `d_a` (avertissement,
    /// R-173, règle 10).
    #[serde(rename = "assignment.too_many_parts")]
    AssignmentTooManyParts {
        /// Tâche.
        task: TaskUid,
        /// Parts (affectations de travail qui la placent).
        parts: u32,
        /// Plafond.
        limit: u32,
    },
    /// Lien entrant d'une tâche en cours non fractionnable
    /// (`split_in_progress` faux) : son reste suit la partie réelle, le lien
    /// est sans effet (info, R-205).
    #[serde(rename = "progress.link_ignored")]
    ProgressLinkIgnored {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur (tâche en cours).
        successor: TaskUid,
    },
    /// Lien entrant dont la borne est violée par les dates réelles d'une
    /// tâche achevée : les dates réelles l'emportent (info, R-203).
    #[serde(rename = "progress.link_overridden")]
    ProgressLinkOverridden {
        /// Prédécesseur.
        predecessor: TaskUid,
        /// Successeur (tâche achevée).
        successor: TaskUid,
    },
    /// Option d'avancement que le calcul ne suit pas : calcul aux valeurs
    /// par défaut (info, R-218).
    #[serde(rename = "progress.option_unsupported")]
    ProgressOptionUnsupported {
        /// Option.
        option: ProgressOption,
    },
    /// Tâche en cours dans un projet planifié à rebours : reste placé au
    /// plus tôt depuis sa reprise (avertissement, R-207).
    #[serde(rename = "progress.from_finish_in_progress")]
    ProgressFromFinishInProgress {
        /// Tâche.
        task: TaskUid,
    },
    /// Tâche commencée sans début réel : début réel implicite = début au
    /// plus tôt du réseau (info, R-200).
    #[serde(rename = "progress.actual_start_missing")]
    ProgressActualStartMissing {
        /// Tâche.
        task: TaskUid,
    },
    /// Jalon saisi à un % strictement entre 0 et 100 : ramené à 100 %
    /// (avertissement, R-213).
    #[serde(rename = "progress.milestone_rounded")]
    ProgressMilestoneRounded {
        /// Tâche.
        task: TaskUid,
    },
    /// Contrainte de fin d'une tâche en cours : ignorée pour les dates
    /// (info, R-205).
    #[serde(rename = "constraint.ignored_in_progress")]
    ConstraintIgnoredInProgress {
        /// Tâche en cours.
        task: TaskUid,
        /// Type de la contrainte.
        kind: ConstraintKind,
    },
    /// Ligne de base posée avec le coût stocké recopié tel quel : PlannerCraft ne
    /// recalcule pas les coûts (info, R-223, ADR-0014 arbitrage du
    /// 2026-10-20).
    #[serde(rename = "baseline.cost_not_computed")]
    BaselineCostNotComputed {
        /// Numéro de la ligne.
        number: BaselineNumber,
    },
    /// Tâches de la portée sautées (lignes vides, tâches exclues du
    /// calcul) : sans ligne de base (info, R-221).
    #[serde(rename = "baseline.task_skipped")]
    BaselineTaskSkipped {
        /// Nombre de tâches sautées.
        count: u32,
    },
    /// `baseline.clear` sans rien à effacer : aucune entité de la portée
    /// ne porte la ligne (info, J7, R-250 ; aucun changement).
    #[serde(rename = "baseline.nothing_to_clear")]
    BaselineNothingToClear {
        /// Numéro de la ligne.
        number: BaselineNumber,
    },
    /// Invariant interne du calcul violé : jamais attendu, anomalie à
    /// signaler (erreur).
    #[serde(rename = "schedule.internal")]
    ScheduleInternal {
        /// Description.
        detail: String,
    },
    /// Valeur d'un champ personnalisé du mauvais type ou hors de ses
    /// bornes (texte de plus de 255 caractères, nombre au-delà de ±10^14)
    /// (erreur, R-263).
    #[serde(rename = "task.custom_invalid")]
    TaskCustomInvalid {
        /// Tâche.
        task: TaskUid,
        /// Champ.
        field: CustomFieldId,
    },
}

impl Diagnostic {
    /// Diagnostic d'un calendrier qui ne se compile pas (héritage, invariant
    /// interne) ou n'a aucun temps ouvré. `None` pour les autres erreurs : un calendrier
    /// inconnu n'a de sens qu'avec l'élément qui le référence
    /// ([`Diagnostic::CalendarUnknown`]), et les erreurs de conversion sont
    /// rapportées par l'appelant avec leur contexte.
    #[must_use]
    pub fn from_calendar_error(e: &CalendarError) -> Option<Self> {
        match e {
            CalendarError::BaseMissing { calendar, base } => Some(Self::CalendarBaseMissing {
                calendar: *calendar,
                base: *base,
            }),
            CalendarError::BaseNotBase { calendar, base } => Some(Self::CalendarBaseNotBase {
                calendar: *calendar,
                base: *base,
            }),
            CalendarError::InheritanceCycle { chain } => {
                Some(Self::CalendarInheritanceCycle { chain: chain.clone() })
            }
            CalendarError::NoWorkingTime { calendar } => {
                Some(Self::CalendarNoWorkingTime { calendar: *calendar })
            }
            CalendarError::Inconsistent { calendar } => Some(Self::CalendarInternal { calendar: *calendar }),
            CalendarError::UnknownCalendar { .. }
            | CalendarError::OutOfDomain { .. }
            | CalendarError::ElapsedOutOfDomain { .. }
            | CalendarError::NegativeSpan
            | CalendarError::TooManySegments { .. }
            | CalendarError::Model(_) => None,
        }
    }

    /// Gravité, déduite de la variante.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        match self {
            Self::CalendarUnknown { .. }
            | Self::CalendarBaseMissing { .. }
            | Self::CalendarBaseNotBase { .. }
            | Self::CalendarInheritanceCycle { .. }
            | Self::CalendarNoWorkingTime { .. }
            | Self::CalendarInternal { .. }
            | Self::TaskReservedUid
            | Self::TaskOutlineJump { .. }
            | Self::TaskNegativeProgress { .. }
            | Self::LinkSelf { .. }
            | Self::LinkDuplicate { .. }
            | Self::LinkUnknownTask { .. }
            | Self::AssignmentUnknownTask { .. }
            | Self::AssignmentUnknownResource { .. }
            | Self::ConversionOutOfDomain { .. }
            | Self::LinkCycle { .. }
            | Self::LinkSummaryDescendant { .. }
            | Self::TaskManualDatesInverted { .. }
            | Self::ProjectCalendarFallback { .. }
            | Self::TaskUnschedulable { .. }
            | Self::LinkUnschedulable { .. }
            | Self::AssignmentDurationOutOfDomain { .. }
            | Self::ScheduleInternal { .. }
            | Self::TaskCustomInvalid { .. } => Severity::Error,
            Self::CalendarExceptionsOverlap { .. }
            | Self::CalendarWorkWeeksOverlap { .. }
            | Self::CalendarRecurrenceUnsupported { .. }
            | Self::ProjectCalendarNotBase { .. }
            | Self::TaskNullWithData { .. }
            | Self::LinkIgnoredInCycle { .. }
            | Self::LinkFinishToSummary { .. }
            | Self::LinkExpansionLimit { .. }
            | Self::LinkStartFromSummary { .. }
            | Self::LinkManualNotHonored { .. }
            | Self::TaskCalendarFallback { .. }
            | Self::ConstraintOverride { .. }
            | Self::ConstraintNotHonored { .. }
            | Self::ConstraintSummaryIgnored { .. }
            | Self::TaskDeadlineMissed { .. }
            | Self::ResourceOverallocated { .. }
            | Self::AssignmentZeroUnits { .. }
            | Self::AssignmentCalendarEmpty { .. }
            | Self::TaskFixedWorkEffortDriven { .. }
            | Self::CalendarTooManySegments { .. }
            | Self::AssignmentTooManyParts { .. }
            | Self::ProgressFromFinishInProgress { .. }
            | Self::ProgressMilestoneRounded { .. } => Severity::Warning,
            Self::LinkInactive { .. }
            | Self::LinkExcludedEndpoint { .. }
            | Self::TaskManualUndated { .. }
            | Self::ConstraintManualIgnored { .. }
            | Self::ResourceAvailabilityIgnored { .. }
            | Self::AssignmentContourUnsupported { .. }
            | Self::ProgressLinkIgnored { .. }
            | Self::ProgressLinkOverridden { .. }
            | Self::ProgressOptionUnsupported { .. }
            | Self::ProgressActualStartMissing { .. }
            | Self::ConstraintIgnoredInProgress { .. }
            | Self::BaselineCostNotComputed { .. }
            | Self::BaselineTaskSkipped { .. }
            | Self::BaselineNothingToClear { .. } => Severity::Info,
        }
    }

    /// Élément concerné, déduit des champs.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "un bras par famille de variantes, rien à factoriser"
    )]
    pub fn subject(&self) -> Subject {
        match self {
            Self::CalendarUnknown { referenced_by, .. } => *referenced_by,
            Self::CalendarBaseMissing { calendar, .. }
            | Self::CalendarBaseNotBase { calendar, .. }
            | Self::CalendarNoWorkingTime { calendar }
            | Self::CalendarInternal { calendar }
            | Self::CalendarExceptionsOverlap { calendar, .. }
            | Self::CalendarWorkWeeksOverlap { calendar, .. }
            | Self::CalendarRecurrenceUnsupported { calendar, .. }
            | Self::ConversionOutOfDomain { calendar, .. }
            | Self::CalendarTooManySegments { calendar, .. } => Subject::Calendar(*calendar),
            Self::CalendarInheritanceCycle { chain } => {
                chain.first().map_or(Subject::Project, |c| Subject::Calendar(*c))
            }
            Self::ProjectCalendarNotBase { .. }
            | Self::ProjectCalendarFallback { .. }
            | Self::LinkExpansionLimit { .. }
            | Self::ProgressOptionUnsupported { .. }
            | Self::BaselineCostNotComputed { .. }
            | Self::BaselineTaskSkipped { .. }
            | Self::BaselineNothingToClear { .. }
            | Self::ScheduleInternal { .. } => Subject::Project,
            Self::TaskReservedUid => Subject::Task(TaskUid::PROJECT_SUMMARY),
            Self::TaskOutlineJump { task }
            | Self::TaskNullWithData { task }
            | Self::TaskNegativeProgress { task }
            | Self::TaskManualUndated { task }
            | Self::TaskManualDatesInverted { task }
            | Self::TaskCalendarFallback { task, .. }
            | Self::TaskUnschedulable { task, .. }
            | Self::AssignmentTooManyParts { task, .. }
            | Self::ConstraintOverride { task, .. }
            | Self::ConstraintNotHonored { task, .. }
            | Self::ConstraintSummaryIgnored { task, .. }
            | Self::ConstraintManualIgnored { task, .. }
            | Self::TaskDeadlineMissed { task, .. }
            | Self::TaskFixedWorkEffortDriven { task }
            | Self::ProgressFromFinishInProgress { task }
            | Self::ProgressActualStartMissing { task }
            | Self::ProgressMilestoneRounded { task }
            | Self::TaskCustomInvalid { task, .. }
            | Self::ConstraintIgnoredInProgress { task, .. } => Subject::Task(*task),
            Self::LinkCycle { tasks } => tasks.iter().min().map_or(Subject::Project, |t| Subject::Task(*t)),
            Self::LinkSelf { task } => Subject::Link {
                predecessor: *task,
                successor: *task,
            },
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
            | Self::LinkUnschedulable {
                predecessor,
                successor,
                ..
            }
            | Self::ProgressLinkIgnored {
                predecessor,
                successor,
            }
            | Self::ProgressLinkOverridden {
                predecessor,
                successor,
            } => Subject::Link {
                predecessor: *predecessor,
                successor: *successor,
            },
            Self::AssignmentUnknownTask { assignment, .. }
            | Self::AssignmentUnknownResource { assignment, .. }
            | Self::AssignmentContourUnsupported { assignment, .. }
            | Self::AssignmentZeroUnits { assignment }
            | Self::AssignmentDurationOutOfDomain { assignment }
            | Self::AssignmentCalendarEmpty { assignment, .. } => Subject::Assignment(*assignment),
            Self::ResourceOverallocated { resource, .. } | Self::ResourceAvailabilityIgnored { resource } => {
                Subject::Resource(*resource)
            }
        }
    }

    /// Code stable (égal à l'étiquette serde `code`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::CalendarUnknown { .. } => "calendar.unknown",
            Self::CalendarBaseMissing { .. } => "calendar.base_missing",
            Self::CalendarBaseNotBase { .. } => "calendar.base_not_base",
            Self::CalendarInheritanceCycle { .. } => "calendar.inheritance_cycle",
            Self::CalendarNoWorkingTime { .. } => "calendar.no_working_time",
            Self::CalendarInternal { .. } => "calendar.internal",
            Self::CalendarExceptionsOverlap { .. } => "calendar.exceptions_overlap",
            Self::CalendarWorkWeeksOverlap { .. } => "calendar.work_weeks_overlap",
            Self::CalendarRecurrenceUnsupported { .. } => "calendar.recurrence_unsupported",
            Self::ProjectCalendarNotBase { .. } => "project.calendar_not_base",
            Self::TaskReservedUid => "task.reserved_uid",
            Self::TaskOutlineJump { .. } => "task.outline_jump",
            Self::TaskNullWithData { .. } => "task.null_with_data",
            Self::TaskNegativeProgress { .. } => "task.negative_progress",
            Self::LinkSelf { .. } => "link.self",
            Self::LinkDuplicate { .. } => "link.duplicate",
            Self::LinkUnknownTask { .. } => "link.unknown_task",
            Self::AssignmentUnknownTask { .. } => "assignment.unknown_task",
            Self::AssignmentUnknownResource { .. } => "assignment.unknown_resource",
            Self::ConversionOutOfDomain { .. } => "conversion.out_of_domain",
            Self::LinkCycle { .. } => "link.cycle",
            Self::LinkIgnoredInCycle { .. } => "link.ignored_in_cycle",
            Self::LinkSummaryDescendant { .. } => "link.summary_descendant",
            Self::LinkFinishToSummary { .. } => "link.finish_to_summary",
            Self::LinkExpansionLimit { .. } => "link.expansion_limit",
            Self::LinkStartFromSummary { .. } => "link.start_from_summary",
            Self::LinkInactive { .. } => "link.inactive",
            Self::LinkExcludedEndpoint { .. } => "link.excluded_endpoint",
            Self::LinkManualNotHonored { .. } => "link.manual_not_honored",
            Self::LinkUnschedulable { .. } => "link.unschedulable",
            Self::TaskManualUndated { .. } => "task.manual_undated",
            Self::TaskManualDatesInverted { .. } => "task.manual_dates_inverted",
            Self::TaskCalendarFallback { .. } => "task.calendar_fallback",
            Self::ProjectCalendarFallback { .. } => "project.calendar_fallback",
            Self::TaskUnschedulable { .. } => "task.unschedulable",
            Self::ConstraintOverride { .. } => "constraint.override",
            Self::ConstraintNotHonored { .. } => "constraint.not_honored",
            Self::ConstraintSummaryIgnored { .. } => "constraint.summary_ignored",
            Self::ConstraintManualIgnored { .. } => "constraint.manual_ignored",
            Self::TaskDeadlineMissed { .. } => "task.deadline_missed",
            Self::ResourceOverallocated { .. } => "resource.overallocated",
            Self::ResourceAvailabilityIgnored { .. } => "resource.availability_ignored",
            Self::AssignmentContourUnsupported { .. } => "assignment.contour_unsupported",
            Self::AssignmentZeroUnits { .. } => "assignment.zero_units",
            Self::AssignmentDurationOutOfDomain { .. } => "assignment.duration_out_of_domain",
            Self::AssignmentCalendarEmpty { .. } => "assignment.calendar_empty",
            Self::TaskFixedWorkEffortDriven { .. } => "task.fixed_work_effort_driven",
            Self::CalendarTooManySegments { .. } => "calendar.too_many_segments",
            Self::AssignmentTooManyParts { .. } => "assignment.too_many_parts",
            Self::ProgressLinkIgnored { .. } => "progress.link_ignored",
            Self::ProgressLinkOverridden { .. } => "progress.link_overridden",
            Self::ProgressOptionUnsupported { .. } => "progress.option_unsupported",
            Self::ProgressFromFinishInProgress { .. } => "progress.from_finish_in_progress",
            Self::ProgressActualStartMissing { .. } => "progress.actual_start_missing",
            Self::ProgressMilestoneRounded { .. } => "progress.milestone_rounded",
            Self::TaskCustomInvalid { .. } => "task.custom_invalid",
            Self::ConstraintIgnoredInProgress { .. } => "constraint.ignored_in_progress",
            Self::BaselineCostNotComputed { .. } => "baseline.cost_not_computed",
            Self::BaselineTaskSkipped { .. } => "baseline.task_skipped",
            Self::BaselineNothingToClear { .. } => "baseline.nothing_to_clear",
            Self::ScheduleInternal { .. } => "schedule.internal",
        }
    }
}

impl fmt::Display for Diagnostic {
    /// Message français ; les traductions (J8) partent du code et des champs.
    #[allow(clippy::too_many_lines, reason = "un bras par variante, rien à factoriser")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CalendarUnknown {
                calendar,
                referenced_by,
            } => {
                write!(f, "calendrier {calendar} inconnu, référencé par {referenced_by}")
            }
            Self::CalendarBaseMissing { calendar, base } => {
                write!(
                    f,
                    "le calendrier {calendar} hérite du calendrier {base}, qui n'existe pas"
                )
            }
            Self::CalendarBaseNotBase { calendar, base } => write!(
                f,
                "le calendrier {calendar} hérite du calendrier {base}, qui n'est pas un calendrier de base"
            ),
            Self::CalendarInheritanceCycle { chain } => {
                let chain: Vec<String> = chain.iter().map(ToString::to_string).collect();
                write!(f, "boucle d'héritage entre calendriers : {}", chain.join(" → "))
            }
            Self::CalendarNoWorkingTime { calendar } => write!(
                f,
                "le calendrier {calendar} n'a aucun temps ouvré : aucune durée ne peut y être placée"
            ),
            Self::CalendarInternal { calendar } => write!(
                f,
                "le calendrier {calendar} n'a pas pu être compilé (incohérence interne, anomalie à signaler)"
            ),
            Self::CalendarExceptionsOverlap {
                calendar,
                kept,
                ignored,
            } => write!(
                f,
                "calendrier {calendar} : les exceptions n° {kept} et n° {ignored} se chevauchent ; \
                 la n° {kept} l'emporte sur les jours communs"
            ),
            Self::CalendarWorkWeeksOverlap {
                calendar,
                kept,
                ignored,
            } => write!(
                f,
                "calendrier {calendar} : les semaines de travail n° {kept} et n° {ignored} se chevauchent ; \
                 la n° {kept} l'emporte sur les jours communs"
            ),
            Self::CalendarRecurrenceUnsupported { calendar, exception } => write!(
                f,
                "calendrier {calendar} : la récurrence de l'exception n° {exception} n'est pas prise en charge ; \
                 l'exception est ignorée"
            ),
            Self::ProjectCalendarNotBase { calendar } => write!(
                f,
                "le calendrier du projet ({calendar}) n'est pas un calendrier de base"
            ),
            Self::TaskReservedUid => {
                f.write_str("une tâche porte l'UID 0, réservé à la tâche récapitulative du projet")
            }
            Self::TaskOutlineJump { task } => write!(
                f,
                "la tâche {task} saute un niveau hiérarchique (ou le premier niveau n'est pas 1)"
            ),
            Self::TaskNullWithData { task } => {
                write!(
                    f,
                    "la ligne vide {task} porte des liens ou des affectations, ignorés"
                )
            }
            Self::TaskNegativeProgress { task } => {
                write!(f, "la tâche {task} a une durée réelle ou restante négative")
            }
            Self::LinkSelf { task } => write!(f, "la tâche {task} est liée à elle-même"),
            Self::LinkDuplicate {
                predecessor,
                successor,
            } => {
                write!(
                    f,
                    "plusieurs liens de la tâche {predecessor} vers la tâche {successor}"
                )
            }
            Self::LinkUnknownTask {
                predecessor,
                successor,
            } => write!(
                f,
                "la tâche {successor} est liée à la tâche {predecessor}, qui n'existe pas"
            ),
            Self::AssignmentUnknownTask { assignment, task } => {
                write!(
                    f,
                    "l'affectation {assignment} porte sur la tâche {task}, qui n'existe pas"
                )
            }
            Self::AssignmentUnknownResource { assignment, resource } => write!(
                f,
                "l'affectation {assignment} porte sur la ressource {resource}, qui n'existe pas"
            ),
            Self::ConversionOutOfDomain {
                calendar,
                from,
                minutes,
            } => write!(
                f,
                "calendrier {calendar} : {} min à partir du {from} sortent du domaine des dates (1984-2149)",
                minutes.get()
            ),
            Self::LinkCycle { tasks } => {
                let tasks: Vec<String> = tasks.iter().map(ToString::to_string).collect();
                write!(f, "liens circulaires entre les tâches {}", tasks.join(", "))
            }
            Self::LinkIgnoredInCycle {
                predecessor,
                successor,
            } => write!(
                f,
                "le lien {predecessor} → {successor} est ignoré pour rompre un cycle"
            ),
            Self::LinkSummaryDescendant {
                predecessor,
                successor,
            } => write!(
                f,
                "le lien {predecessor} → {successor} relie une récapitulative à l'une de ses sous-tâches : ignoré"
            ),
            Self::LinkFinishToSummary {
                predecessor,
                successor,
            } => write!(
                f,
                "le lien {predecessor} → {successor} (fin à fin ou début à fin) vers une récapitulative \
                 s'applique à chacune de ses sous-tâches"
            ),
            Self::LinkExpansionLimit { limit, ignored } => write!(
                f,
                "{ignored} lien(s) fin à fin ou début à fin vers une récapitulative ignoré(s) : \
                 leur report sur les sous-tâches dépasserait {limit} arcs"
            ),
            Self::LinkStartFromSummary {
                predecessor,
                successor,
            } => write!(
                f,
                "le lien {predecessor} → {successor} (début à début ou début à fin) depuis une \
                 récapitulative plafonne le début de chacune de ses sous-tâches (planification à rebours)"
            ),
            Self::LinkInactive {
                predecessor,
                successor,
            } => write!(
                f,
                "le lien {predecessor} → {successor} part d'une tâche inactive : sans effet"
            ),
            Self::LinkExcludedEndpoint {
                predecessor,
                successor,
            } => write!(
                f,
                "le lien {predecessor} → {successor} touche une tâche non planifiée : ignoré"
            ),
            Self::LinkManualNotHonored {
                predecessor,
                successor,
            } => write!(
                f,
                "la tâche manuelle {successor} ne respecte pas son lien avec la tâche {predecessor}"
            ),
            Self::LinkUnschedulable {
                predecessor,
                successor,
                cause,
            } => write!(
                f,
                "le lien {predecessor} → {successor} vers une récapitulative ne peut pas être appliqué ({cause}) : ignoré"
            ),
            Self::TaskManualUndated { task } => {
                write!(f, "la tâche manuelle {task} n'a ni début ni fin : non planifiée")
            }
            Self::TaskManualDatesInverted { task } => write!(
                f,
                "la tâche manuelle {task} finit avant de commencer : fin ramenée au début"
            ),
            Self::TaskCalendarFallback { task, calendar } => write!(
                f,
                "le calendrier {calendar} de la tâche {task} est inutilisable : calendrier du projet utilisé"
            ),
            Self::ProjectCalendarFallback { calendar } => write!(
                f,
                "le calendrier du projet ({calendar}) est inutilisable : calendrier Standard intégré utilisé"
            ),
            Self::TaskUnschedulable { task, pass, cause } => {
                write!(f, "la tâche {task} ne peut pas être planifiée ({pass} : {cause})")
            }
            Self::ConstraintOverride { task, kind, bound } => write!(
                f,
                "la contrainte {} de la tâche {task} est respectée malgré {bound}",
                constraint_name(*kind)
            ),
            Self::ConstraintNotHonored { task, kind, date } => write!(
                f,
                "la contrainte {} du {date} de la tâche {task} n'est pas respectée : les liens l'emportent",
                constraint_name(*kind)
            ),
            Self::ConstraintSummaryIgnored { task, kind } => write!(
                f,
                "la contrainte {} de la récapitulative {task} est ignorée",
                constraint_name(*kind)
            ),
            Self::ConstraintManualIgnored { task, kind } => write!(
                f,
                "la contrainte {} de la tâche manuelle {task} est ignorée par le calcul",
                constraint_name(*kind)
            ),
            Self::TaskDeadlineMissed {
                task,
                deadline,
                finish,
            } => write!(
                f,
                "la tâche {task} finit le {finish}, après son échéance du {deadline}"
            ),
            Self::ResourceOverallocated {
                resource,
                from,
                to,
                peak,
            } => write!(
                f,
                "la ressource {resource} est surutilisée du {from} au {to} (pic {} %)",
                fixed_percent(*peak)
            ),
            Self::ResourceAvailabilityIgnored { resource } => write!(
                f,
                "les disponibilités datées de la ressource {resource} sont ignorées : capacité constante"
            ),
            Self::AssignmentContourUnsupported { assignment, contour } => write!(
                f,
                "le profil {contour:?} de l'affectation {assignment} est placé comme uniforme"
            ),
            Self::AssignmentZeroUnits { assignment } => write!(
                f,
                "l'affectation {assignment} a des unités nulles : la durée de la tâche est gardée"
            ),
            Self::AssignmentDurationOutOfDomain { assignment } => {
                write!(f, "la durée de l'affectation {assignment} sort du domaine")
            }
            Self::AssignmentCalendarEmpty {
                assignment,
                task_calendar,
                resource_calendar,
            } => write!(
                f,
                "les calendriers {task_calendar} (tâche) et {resource_calendar} (ressource) de l'affectation {assignment} n'ont aucun temps ouvré commun"
            ),
            Self::TaskFixedWorkEffortDriven { task } => write!(
                f,
                "la tâche {task} est à travail fixe sans être pilotée par l'effort"
            ),
            Self::CalendarTooManySegments {
                calendar,
                other,
                limit,
            } if calendar == other => write!(
                f,
                "le calendrier {calendar} (ou sa combinaison) dépasse le budget de {limit} segments : remplacé par un calendrier de repli"
            ),
            Self::CalendarTooManySegments {
                calendar,
                other,
                limit,
            } => write!(
                f,
                "la combinaison des calendriers {calendar} et {other} dépasse {limit} segments"
            ),
            Self::AssignmentTooManyParts { task, parts, limit } => write!(
                f,
                "la tâche {task} a {parts} affectations de travail (plus de {limit}) : sa durée est celle de la plus longue"
            ),
            Self::ProgressLinkIgnored {
                predecessor,
                successor,
            } => write!(
                f,
                "le lien {predecessor} → {successor} est sans effet : la tâche en cours {successor} ne se fractionne pas"
            ),
            Self::ProgressLinkOverridden {
                predecessor,
                successor,
            } => write!(
                f,
                "les dates réelles de la tâche achevée {successor} l'emportent sur son lien avec la tâche {predecessor}"
            ),
            Self::ProgressOptionUnsupported { option } => write!(
                f,
                "l'option d'avancement {} n'est pas suivie : calcul aux valeurs par défaut",
                option.mspdi_name()
            ),
            Self::ProgressFromFinishInProgress { task } => write!(
                f,
                "la tâche en cours {task} d'un projet planifié à rebours : son reste est placé au plus tôt"
            ),
            Self::ProgressActualStartMissing { task } => write!(
                f,
                "la tâche commencée {task} n'a pas de début réel : son début planifié est pris"
            ),
            Self::ProgressMilestoneRounded { task } => {
                write!(f, "le jalon {task} est passé à 100 % achevé")
            }
            Self::ConstraintIgnoredInProgress { task, kind } => write!(
                f,
                "la contrainte {} de la tâche en cours {task} est ignorée pour ses dates",
                constraint_name(*kind)
            ),
            Self::BaselineCostNotComputed { number } => write!(
                f,
                "ligne de base {} : coûts recopiés tels quels, sans recalcul",
                number.get()
            ),
            Self::BaselineTaskSkipped { count } => {
                write!(f, "{count} tâche(s) non planifiée(s) sans ligne de base")
            }
            Self::BaselineNothingToClear { number } => {
                write!(f, "ligne de base {} : rien à effacer", number.get())
            }
            Self::ScheduleInternal { detail } => {
                write!(
                    f,
                    "incohérence interne du calcul (anomalie à signaler) : {detail}"
                )
            }
            Self::TaskCustomInvalid { task, field } => write!(
                f,
                "la tâche {task} a une valeur de {} du mauvais type ou hors des bornes",
                field.canonical_name()
            ),
        }
    }
}

/// Unités (1/10 000) en pourcentage entier arrondi vers zéro, pour les
/// messages.
const fn fixed_percent(u: Units) -> i64 {
    u.get() / 100
}

/// Nom français d'un type de contrainte, pour les messages.
const fn constraint_name(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::AsSoonAsPossible => "« le plus tôt possible »",
        ConstraintKind::AsLateAsPossible => "« le plus tard possible »",
        ConstraintKind::MustStartOn => "« doit commencer le »",
        ConstraintKind::MustFinishOn => "« doit finir le »",
        ConstraintKind::StartNoEarlierThan => "« début au plus tôt le »",
        ConstraintKind::StartNoLaterThan => "« début au plus tard le »",
        ConstraintKind::FinishNoEarlierThan => "« fin au plus tôt le »",
        ConstraintKind::FinishNoLaterThan => "« fin au plus tard le »",
    }
}

/// Liste de diagnostics en cours de collecte.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Diagnostics(Vec<Diagnostic>);

impl Diagnostics {
    /// Liste vide.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Ajoute un diagnostic.
    pub fn push(&mut self, d: Diagnostic) {
        self.0.push(d);
    }

    /// Ajoute les diagnostics d'une autre liste.
    pub fn extend(&mut self, o: Self) {
        self.0.extend(o.0);
    }

    /// Liste triée (ordre dérivé) et dédoublonnée : ordre déterministe.
    #[must_use]
    pub fn finish(mut self) -> Vec<Diagnostic> {
        self.0.sort();
        self.0.dedup();
        self.0
    }

    /// Vrai si au moins un diagnostic est une erreur.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(|d| d.severity() == Severity::Error)
    }

    /// Nombre de diagnostics collectés (doublons compris).
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Vrai si aucun diagnostic.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Diagnostics dans l'ordre de collecte.
    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Diagnostic, Diagnostics, OverriddenBound, Pass, Severity, Subject};
    use crate::duration::Minutes;
    use crate::error::tests::assert_tag;
    use crate::ids::{AssignmentUid, CalendarUid, ResourceUid, TaskUid};
    use crate::model::{ConstraintKind, LinkType};
    use crate::time::Moment;

    /// Un exemple de chaque variante.
    #[allow(clippy::too_many_lines, reason = "une entrée par variante, rien à factoriser")]
    fn all() -> Vec<Diagnostic> {
        let c = CalendarUid::new(5);
        let t = TaskUid::new(3);
        let a = AssignmentUid::new(9);
        vec![
            Diagnostic::CalendarUnknown {
                calendar: c,
                referenced_by: Subject::Task(t),
            },
            Diagnostic::CalendarBaseMissing {
                calendar: c,
                base: CalendarUid::new(99),
            },
            Diagnostic::CalendarBaseNotBase {
                calendar: c,
                base: CalendarUid::new(6),
            },
            Diagnostic::CalendarInheritanceCycle {
                chain: vec![c, CalendarUid::new(6), c],
            },
            Diagnostic::CalendarNoWorkingTime { calendar: c },
            Diagnostic::CalendarExceptionsOverlap {
                calendar: c,
                kept: 0,
                ignored: 1,
            },
            Diagnostic::CalendarWorkWeeksOverlap {
                calendar: c,
                kept: 0,
                ignored: 1,
            },
            Diagnostic::CalendarRecurrenceUnsupported {
                calendar: c,
                exception: 2,
            },
            Diagnostic::ProjectCalendarNotBase { calendar: c },
            Diagnostic::TaskReservedUid,
            Diagnostic::TaskOutlineJump { task: t },
            Diagnostic::TaskNullWithData { task: t },
            Diagnostic::TaskNegativeProgress { task: t },
            Diagnostic::LinkSelf { task: t },
            Diagnostic::LinkDuplicate {
                predecessor: TaskUid::new(1),
                successor: t,
            },
            Diagnostic::LinkUnknownTask {
                predecessor: TaskUid::new(42),
                successor: t,
            },
            Diagnostic::AssignmentUnknownTask {
                assignment: a,
                task: TaskUid::new(42),
            },
            Diagnostic::AssignmentUnknownResource {
                assignment: a,
                resource: ResourceUid::new(42),
            },
            Diagnostic::ConversionOutOfDomain {
                calendar: c,
                from: "2149-12-31T16:00".parse::<Moment>().unwrap(),
                minutes: Minutes::new(61),
            },
            Diagnostic::CalendarInternal { calendar: c },
            // Diagnostics du calcul (J2), indices 20 à 33.
            Diagnostic::LinkCycle {
                tasks: vec![TaskUid::new(2), t],
            },
            Diagnostic::LinkIgnoredInCycle {
                predecessor: t,
                successor: TaskUid::new(2),
            },
            Diagnostic::LinkSummaryDescendant {
                predecessor: TaskUid::new(1),
                successor: t,
            },
            Diagnostic::LinkFinishToSummary {
                predecessor: TaskUid::new(1),
                successor: t,
            },
            Diagnostic::LinkInactive {
                predecessor: TaskUid::new(1),
                successor: t,
            },
            Diagnostic::LinkExcludedEndpoint {
                predecessor: TaskUid::new(1),
                successor: t,
            },
            Diagnostic::LinkManualNotHonored {
                predecessor: TaskUid::new(1),
                successor: t,
            },
            Diagnostic::LinkUnschedulable {
                predecessor: TaskUid::new(1),
                successor: t,
                cause: "calendar.out_of_domain".to_owned(),
            },
            Diagnostic::TaskManualUndated { task: t },
            Diagnostic::TaskManualDatesInverted { task: t },
            Diagnostic::TaskCalendarFallback { task: t, calendar: c },
            Diagnostic::ProjectCalendarFallback { calendar: c },
            Diagnostic::TaskUnschedulable {
                task: t,
                pass: Pass::Forward,
                cause: "calendar.out_of_domain".to_owned(),
            },
            // Planification à rebours (J3), indice 33 (code retiré
            // `schedule.from_finish_unsupported`, jamais réattribué).
            Diagnostic::LinkStartFromSummary {
                predecessor: TaskUid::new(1),
                successor: t,
            },
            Diagnostic::ScheduleInternal {
                detail: "nœud non visité".to_owned(),
            },
            // Contraintes (J3), indices 35 et suivants.
            Diagnostic::ConstraintOverride {
                task: t,
                kind: ConstraintKind::StartNoLaterThan,
                bound: OverriddenBound::Predecessor {
                    predecessor: TaskUid::new(1),
                    link: LinkType::FinishToStart,
                },
            },
            Diagnostic::ConstraintNotHonored {
                task: t,
                kind: ConstraintKind::StartNoLaterThan,
                date: "2026-10-20T08:00".parse::<Moment>().unwrap(),
            },
            Diagnostic::TaskDeadlineMissed {
                task: t,
                deadline: "2026-10-20T17:00".parse::<Moment>().unwrap(),
                finish: "2026-10-21T17:00".parse::<Moment>().unwrap(),
            },
            Diagnostic::ConstraintSummaryIgnored {
                task: t,
                kind: ConstraintKind::MustStartOn,
            },
            Diagnostic::ConstraintManualIgnored {
                task: t,
                kind: ConstraintKind::MustStartOn,
            },
            // Relecture finale J3, indice 40.
            Diagnostic::LinkExpansionLimit {
                limit: 4_000_000,
                ignored: 2,
            },
        ]
    }

    #[test]
    fn codes_match_serde_tags_and_round_trip() {
        for d in all() {
            assert_tag(&d, d.code());
            let prefix = d.code().split('.').next().unwrap();
            assert!(
                [
                    "calendar",
                    "project",
                    "task",
                    "link",
                    "assignment",
                    "conversion",
                    "schedule",
                    "constraint"
                ]
                .contains(&prefix)
            );
            let json = serde_json::to_string(&d).unwrap();
            assert_eq!(serde_json::from_str::<Diagnostic>(&json).unwrap(), d);
            assert_ne!(d.to_string(), "");
        }
    }

    #[test]
    fn severities() {
        let warnings: Vec<&str> = all()
            .iter()
            .filter(|d| d.severity() == Severity::Warning)
            .map(Diagnostic::code)
            .collect();
        assert_eq!(
            warnings,
            [
                "calendar.exceptions_overlap",
                "calendar.work_weeks_overlap",
                "calendar.recurrence_unsupported",
                "project.calendar_not_base",
                "task.null_with_data",
                "link.ignored_in_cycle",
                "link.finish_to_summary",
                "link.manual_not_honored",
                "task.calendar_fallback",
                "link.start_from_summary",
                "constraint.override",
                "constraint.not_honored",
                "task.deadline_missed",
                "constraint.summary_ignored",
                "link.expansion_limit",
            ]
        );
        let infos: Vec<&str> = all()
            .iter()
            .filter(|d| d.severity() == Severity::Info)
            .map(Diagnostic::code)
            .collect();
        assert_eq!(
            infos,
            [
                "link.inactive",
                "link.excluded_endpoint",
                "task.manual_undated",
                "constraint.manual_ignored"
            ]
        );
    }

    #[test]
    fn subjects() {
        let c = CalendarUid::new(5);
        let t = TaskUid::new(3);
        assert_eq!(all()[0].subject(), Subject::Task(t));
        assert_eq!(all()[3].subject(), Subject::Calendar(c));
        assert_eq!(
            Diagnostic::CalendarInheritanceCycle { chain: vec![] }.subject(),
            Subject::Project
        );
        assert_eq!(all()[8].subject(), Subject::Project);
        assert_eq!(all()[9].subject(), Subject::Task(TaskUid::PROJECT_SUMMARY));
        assert_eq!(
            all()[13].subject(),
            Subject::Link {
                predecessor: t,
                successor: t
            }
        );
        assert_eq!(
            all()[15].subject(),
            Subject::Link {
                predecessor: TaskUid::new(42),
                successor: t
            }
        );
        assert_eq!(all()[17].subject(), Subject::Assignment(AssignmentUid::new(9)));
        assert_eq!(all()[18].subject(), Subject::Calendar(c));
        assert_eq!(all()[19].subject(), Subject::Calendar(c));
        assert_eq!(all()[19].severity(), Severity::Error);
        // Diagnostics du calcul : sujet du cycle = plus petit UID.
        assert_eq!(all()[20].subject(), Subject::Task(TaskUid::new(2)));
        assert_eq!(
            Diagnostic::LinkCycle { tasks: vec![] }.subject(),
            Subject::Project
        );
        assert_eq!(
            all()[21].subject(),
            Subject::Link {
                predecessor: t,
                successor: TaskUid::new(2)
            }
        );
        for i in 22..=27 {
            assert_eq!(
                all()[i].subject(),
                Subject::Link {
                    predecessor: TaskUid::new(1),
                    successor: t
                }
            );
        }
        assert_eq!(all()[27].severity(), Severity::Error);
        for i in [28, 29, 30, 32] {
            assert_eq!(all()[i].subject(), Subject::Task(t));
        }
        for i in [31, 34] {
            assert_eq!(all()[i].subject(), Subject::Project);
        }
        assert_eq!(
            all()[33].subject(),
            Subject::Link {
                predecessor: TaskUid::new(1),
                successor: t
            }
        );
        assert_eq!(all()[33].severity(), Severity::Warning);
        assert_eq!(all()[32].severity(), Severity::Error);
        assert_eq!(
            all()[32].to_string(),
            "la tâche 3 ne peut pas être planifiée (passe avant : calendar.out_of_domain)"
        );
        assert_eq!(
            Subject::Link {
                predecessor: TaskUid::new(1),
                successor: t
            }
            .to_string(),
            "le lien 1 → 3"
        );
        // Plafond du dépliage R-058 : sujet le projet.
        assert_eq!(all()[40].subject(), Subject::Project);
        assert_eq!(all()[40].code(), "link.expansion_limit");
    }

    #[test]
    fn constraint_diagnostics() {
        // Contraintes (J3) : sujet la tâche contrainte, borne sérialisée.
        let t = TaskUid::new(3);
        for i in [35, 36, 37, 38, 39] {
            assert_eq!(all()[i].subject(), Subject::Task(t));
        }
        assert_eq!(
            all()[39].to_string(),
            "la contrainte « doit commencer le » de la tâche manuelle 3 est ignorée par le calcul"
        );
        assert_eq!(
            all()[37].to_string(),
            "la tâche 3 finit le 2026-10-21T17:00, après son échéance du 2026-10-20T17:00"
        );
        assert_eq!(
            all()[35].to_string(),
            "la contrainte « début au plus tard le » de la tâche 3 est respectée malgré le lien depuis la tâche 1"
        );
        assert_eq!(
            serde_json::to_string(&all()[35]).unwrap(),
            r#"{"code":"constraint.override","task":3,"kind":"start_no_later_than","bound":{"kind":"predecessor","predecessor":1,"link":"finish_to_start"}}"#
        );
    }

    #[test]
    fn finish_sorts_and_dedups() {
        let mut d = Diagnostics::new();
        assert!(d.is_empty() && !d.has_errors());
        d.push(Diagnostic::TaskNullWithData {
            task: TaskUid::new(2),
        });
        assert!(!d.has_errors());
        d.push(Diagnostic::LinkSelf {
            task: TaskUid::new(1),
        });
        let mut other = Diagnostics::new();
        other.push(Diagnostic::TaskNullWithData {
            task: TaskUid::new(1),
        });
        other.push(Diagnostic::LinkSelf {
            task: TaskUid::new(1),
        });
        d.extend(other);
        assert_eq!(d.len(), 4);
        assert!(d.has_errors());
        assert_eq!(d.iter().count(), 4);
        assert_eq!(
            d.finish(),
            [
                Diagnostic::TaskNullWithData {
                    task: TaskUid::new(1)
                },
                Diagnostic::TaskNullWithData {
                    task: TaskUid::new(2)
                },
                Diagnostic::LinkSelf {
                    task: TaskUid::new(1)
                },
            ]
        );
    }

    #[test]
    fn display_examples() {
        assert_eq!(
            Diagnostic::CalendarInheritanceCycle {
                chain: vec![CalendarUid::new(5), CalendarUid::new(6), CalendarUid::new(5)]
            }
            .to_string(),
            "boucle d'héritage entre calendriers : 5 → 6 → 5"
        );
        assert_eq!(
            Diagnostic::CalendarUnknown {
                calendar: CalendarUid::new(8),
                referenced_by: Subject::Resource(ResourceUid::new(2))
            }
            .to_string(),
            "calendrier 8 inconnu, référencé par la ressource 2"
        );
    }

    #[test]
    fn contract_too_many_parts() {
        // Relecture finale J5 (R-173, règle 10).
        let t = TaskUid::new(3);
        let d = Diagnostic::AssignmentTooManyParts {
            task: t,
            parts: 1_001,
            limit: 1_000,
        };
        assert_eq!(d.code(), "assignment.too_many_parts");
        assert_eq!(d.severity(), Severity::Warning);
        assert_eq!(d.subject(), Subject::Task(t));
        assert!(d.to_string().contains("1001"), "{d}");
    }

    #[test]
    fn contract_j5_diagnostics() {
        let c = CalendarUid::new(5);
        let r = ResourceUid::new(4);
        let a = AssignmentUid::new(9);
        let t = TaskUid::new(3);
        let at = |s: &str| s.parse::<Moment>().unwrap();
        let cases = [
            (
                Diagnostic::ResourceOverallocated {
                    resource: r,
                    from: at("2026-10-23T18:00"),
                    to: at("2026-10-23T20:00"),
                    peak: crate::quantity::Units::new(20_000).unwrap(),
                },
                "resource.overallocated",
                Severity::Warning,
                Subject::Resource(r),
            ),
            (
                Diagnostic::ResourceAvailabilityIgnored { resource: r },
                "resource.availability_ignored",
                Severity::Info,
                Subject::Resource(r),
            ),
            (
                Diagnostic::AssignmentContourUnsupported {
                    assignment: a,
                    contour: crate::model::WorkContour::Bell,
                },
                "assignment.contour_unsupported",
                Severity::Info,
                Subject::Assignment(a),
            ),
            (
                Diagnostic::AssignmentZeroUnits { assignment: a },
                "assignment.zero_units",
                Severity::Warning,
                Subject::Assignment(a),
            ),
            (
                Diagnostic::AssignmentDurationOutOfDomain { assignment: a },
                "assignment.duration_out_of_domain",
                Severity::Error,
                Subject::Assignment(a),
            ),
            (
                Diagnostic::AssignmentCalendarEmpty {
                    assignment: a,
                    task_calendar: c,
                    resource_calendar: CalendarUid::new(6),
                },
                "assignment.calendar_empty",
                Severity::Warning,
                Subject::Assignment(a),
            ),
            (
                Diagnostic::TaskFixedWorkEffortDriven { task: t },
                "task.fixed_work_effort_driven",
                Severity::Warning,
                Subject::Task(t),
            ),
            (
                Diagnostic::CalendarTooManySegments {
                    calendar: c,
                    other: CalendarUid::new(6),
                    limit: 100_000,
                },
                "calendar.too_many_segments",
                Severity::Warning,
                Subject::Calendar(c),
            ),
        ];
        for (d, code, severity, subject) in cases {
            assert_eq!(d.code(), code);
            assert_tag(&d, code);
            assert_eq!(d.severity(), severity, "{code}");
            assert_eq!(d.subject(), subject, "{code}");
            let json = serde_json::to_string(&d).unwrap();
            assert_eq!(serde_json::from_str::<Diagnostic>(&json).unwrap(), d);
            assert_ne!(d.to_string(), "");
        }
        assert_eq!(
            Diagnostic::ResourceOverallocated {
                resource: r,
                from: at("2026-10-23T18:00"),
                to: at("2026-10-23T20:00"),
                peak: crate::quantity::Units::new(20_000).unwrap(),
            }
            .to_string(),
            "la ressource 4 est surutilisée du 2026-10-23T18:00 au 2026-10-23T20:00 (pic 200 %)"
        );
        assert_eq!(
            Diagnostic::from_calendar_error(&crate::error::CalendarError::TooManySegments { limit: 1 }),
            None
        );
    }

    #[test]
    fn contract_j6_diagnostics() {
        let t = TaskUid::new(3);
        let link = Subject::Link {
            predecessor: TaskUid::new(1),
            successor: t,
        };
        let n = crate::ids::BaselineNumber::new(1).unwrap();
        let cases = [
            (
                Diagnostic::ProgressLinkIgnored {
                    predecessor: TaskUid::new(1),
                    successor: t,
                },
                "progress.link_ignored",
                Severity::Info,
                link,
            ),
            (
                Diagnostic::ProgressLinkOverridden {
                    predecessor: TaskUid::new(1),
                    successor: t,
                },
                "progress.link_overridden",
                Severity::Info,
                link,
            ),
            (
                Diagnostic::ProgressOptionUnsupported {
                    option: crate::model::ProgressOption::MoveCompletedEndsBack,
                },
                "progress.option_unsupported",
                Severity::Info,
                Subject::Project,
            ),
            (
                Diagnostic::ProgressFromFinishInProgress { task: t },
                "progress.from_finish_in_progress",
                Severity::Warning,
                Subject::Task(t),
            ),
            (
                Diagnostic::ProgressActualStartMissing { task: t },
                "progress.actual_start_missing",
                Severity::Info,
                Subject::Task(t),
            ),
            (
                Diagnostic::ProgressMilestoneRounded { task: t },
                "progress.milestone_rounded",
                Severity::Warning,
                Subject::Task(t),
            ),
            (
                Diagnostic::ConstraintIgnoredInProgress {
                    task: t,
                    kind: ConstraintKind::FinishNoLaterThan,
                },
                "constraint.ignored_in_progress",
                Severity::Info,
                Subject::Task(t),
            ),
            (
                Diagnostic::BaselineCostNotComputed { number: n },
                "baseline.cost_not_computed",
                Severity::Info,
                Subject::Project,
            ),
            (
                Diagnostic::BaselineTaskSkipped { count: 2 },
                "baseline.task_skipped",
                Severity::Info,
                Subject::Project,
            ),
            (
                Diagnostic::BaselineNothingToClear { number: n },
                "baseline.nothing_to_clear",
                Severity::Info,
                Subject::Project,
            ),
        ];
        for (d, code, severity, subject) in cases {
            assert_eq!(d.code(), code);
            assert_tag(&d, code);
            assert_eq!(d.severity(), severity, "{code}");
            assert_eq!(d.subject(), subject, "{code}");
            let json = serde_json::to_string(&d).unwrap();
            assert_eq!(serde_json::from_str::<Diagnostic>(&json).unwrap(), d);
            assert_ne!(d.to_string(), "");
        }
        assert_eq!(
            serde_json::to_string(&Diagnostic::ProgressOptionUnsupported {
                option: crate::model::ProgressOption::TaskUpdatesResource
            })
            .unwrap(),
            r#"{"code":"progress.option_unsupported","option":"task_updates_resource"}"#
        );
        assert_eq!(
            Diagnostic::BaselineCostNotComputed { number: n }.to_string(),
            "ligne de base 1 : coûts recopiés tels quels, sans recalcul"
        );
    }
}
