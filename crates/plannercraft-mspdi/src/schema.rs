//! Table unique des éléments MSPDI (R-087, R-121, ADR-0010) : pour chaque
//! parent, la séquence du schéma `mspdi_pj15.xsd` (noms dans l'ordre,
//! `minOccurs`, `maxOccurs`, `maxLength`), la classe de chaque élément et
//! l'ordre observé dans les fichiers enregistrés par MS Project (O19).
//!
//! Lecture et écriture viennent de cette seule table : le lecteur décide par
//! la classe ce qu'il lit, capture en `Extras` ou compte comme calculé non
//! recalculé ; l'écrivain suit la séquence du profil choisi
//! ([`Profile::Strict`] = pj15, [`Profile::Observed`] = ordre observé). Un
//! nom absent de la table est hors schéma (champs agiles, `StartText`,
//! `BuildNumber`, sections d'interface…) : il est conservé verbatim
//! (classe [`Class::Preserved`]), ancré sur son frère précédent.
//!
//! La propriété P27 (`tests/schema`) compare cette table au XSD analysé :
//! noms, ordre, `minOccurs`, `maxOccurs` (« plusieurs » : non borné ou
//! supérieur à 1) et `maxLength` ; le profil observé est une permutation de la
//! séquence pj15 plus `GUID` (sauf `WorkWeek`, voir [`Parent::WorkWeek`]).
//! Les tables ont été produites depuis le XSD par un script de travail, puis
//! les classes affectées à la main selon la spécification J3 (§ 5.2).

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Parent d'une séquence d'éléments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Parent {
    /// Racine `Project`.
    Project,
    /// `Calendars/Calendar`.
    Calendar,
    /// `Calendar/WeekDays/WeekDay`.
    WeekDay,
    /// `Calendar/Exceptions/Exception`.
    Exception,
    /// `Calendar/WorkWeeks/WorkWeek`. Le schéma pj15 y place des `WeekDay`
    /// directs, sans plages (`DayType`, `DayWorking`) ; Microsoft signale que
    /// MS Project n'écrit pas correctement cet élément (note du schéma). Le
    /// profil observé garde la forme lue depuis J2 (`WeekDays/WeekDay` avec
    /// `WorkingTimes`, R-082) : c'est la seule séquence dont le profil
    /// observé n'est pas une permutation de pj15.
    WorkWeek,
    /// `Tasks/Task`.
    Task,
    /// `Task/PredecessorLink`.
    PredecessorLink,
    /// `Task/Baseline`.
    TaskBaseline,
    /// `Resources/Resource`.
    Resource,
    /// `Resource/Baseline`.
    ResourceBaseline,
    /// `Assignments/Assignment`.
    Assignment,
    /// `Assignment/Baseline`.
    AssignmentBaseline,
}

impl Parent {
    /// Tous les parents de la table.
    pub const ALL: [Self; 12] = [
        Self::Project,
        Self::Calendar,
        Self::WeekDay,
        Self::Exception,
        Self::WorkWeek,
        Self::Task,
        Self::PredecessorLink,
        Self::TaskBaseline,
        Self::Resource,
        Self::ResourceBaseline,
        Self::Assignment,
        Self::AssignmentBaseline,
    ];

    /// Chemin du parent dans le schéma, sous `Project` (vide pour la racine).
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::Project => "",
            Self::Calendar => "Calendars/Calendar",
            Self::WeekDay => "Calendars/Calendar/WeekDays/WeekDay",
            Self::Exception => "Calendars/Calendar/Exceptions/Exception",
            Self::WorkWeek => "Calendars/Calendar/WorkWeeks/WorkWeek",
            Self::Task => "Tasks/Task",
            Self::PredecessorLink => "Tasks/Task/PredecessorLink",
            Self::TaskBaseline => "Tasks/Task/Baseline",
            Self::Resource => "Resources/Resource",
            Self::ResourceBaseline => "Resources/Resource/Baseline",
            Self::Assignment => "Assignments/Assignment",
            Self::AssignmentBaseline => "Assignments/Assignment/Baseline",
        }
    }
}

/// Classe d'un élément (spécification J3, § 5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// M : lu dans le modèle, toujours réécrit.
    Modeled,
    /// M? : lu dans le modèle, réécrit seulement s'il a une valeur (option,
    /// liste non vide, ou selon le sens de planification et le rôle de la
    /// tâche : `StartDate`, `FinishDate`, `Duration`).
    ModeledOpt,
    /// D : déduit du modèle (`ID`, `OutlineNumber`, `Summary`,
    /// `IsBaseCalendar`).
    Derived,
    /// C : calculé par PlannerCraft ; lu dans `RecordedSchedule`, écrit depuis les
    /// valeurs calculées (R-122). Depuis J5 (R-198) : `Start`/`Finish`
    /// d'affectation, `Work` et `OverAllocated` de ressource ; depuis J6
    /// (R-214) : `Stop` de tâche, `ActualStart`, `ActualFinish`, `Stop`,
    /// `Resume` d'affectation.
    Computed,
    /// X : calculé, non recalculé par PlannerCraft (coûts, valeur acquise,
    /// écarts, pics d'unités, surutilisation d'une affectation) ; ni lu ni
    /// conservé,
    /// compté (`SourceInfo::dropped_computed`) : MS Project le recalcule.
    Dropped,
    /// P : non modélisé, capturé verbatim en `Extras` (R-087, R-123).
    Preserved,
    /// F : écrit depuis les options d'écriture (`SaveVersion`).
    Format,
}

/// Un élément d'une séquence du schéma.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// Nom local.
    pub name: &'static str,
    /// Classe.
    pub class: Class,
    /// `minOccurs` (0 ou 1).
    pub min: u8,
    /// `maxOccurs` > 1 ou non borné.
    pub many: bool,
    /// `maxLength` d'une chaîne, en caractères.
    pub max_len: Option<u16>,
}

impl Slot {
    const fn new(name: &'static str, class: Class, min: u8, many: bool, max_len: Option<u16>) -> Self {
        Self {
            name,
            class,
            min,
            many,
            max_len,
        }
    }

    /// Un élément de cette classe est toujours réécrit (classes M, D, F) :
    /// il peut servir d'ancre à un élément hors schéma (R-087). Un élément P
    /// sert d'ancre seulement quand il a été capturé (il est alors réécrit).
    #[must_use]
    pub const fn always_written(&self) -> bool {
        matches!(self.class, Class::Modeled | Class::Derived | Class::Format)
    }
}

/// Profil d'ordre d'écriture (R-121).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Profile {
    /// Ordre observé dans les fichiers enregistrés par MS Project (O19),
    /// `GUID` et éléments hors schéma compris. Défaut.
    #[default]
    Observed,
    /// Ordre pj15, sans `GUID` ni élément hors schéma.
    Strict,
}

/// Séquence d'un parent : ordre pj15 et ordre observé.
#[derive(Debug)]
pub struct Sequence {
    /// Parent.
    pub parent: Parent,
    /// Séquence pj15, avec les classes.
    pub strict: &'static [Slot],
    /// Ordre observé (noms) ; vide : identique à `strict`.
    pub observed: &'static [&'static str],
}

/// Index d'une séquence : nom → rang pj15, nom → rang observé, et noms de
/// chaque profil avec leur classe (écriture sans recherche par nom).
struct Index {
    strict: BTreeMap<&'static str, usize>,
    observed: BTreeMap<&'static str, usize>,
    strict_classes: Vec<(&'static str, Option<Class>)>,
    observed_classes: Vec<(&'static str, Option<Class>)>,
}

impl Sequence {
    /// Élément pj15 de ce nom.
    #[must_use]
    pub fn slot(&self, name: &str) -> Option<&'static Slot> {
        let i = *self.index().strict.get(name)?;
        self.strict.get(i)
    }

    /// Classe d'un élément ; `None` : hors schéma (conservé, R-087), sauf
    /// `GUID`, modélisé.
    #[must_use]
    pub fn class(&self, name: &str) -> Option<Class> {
        self.slot(name).map(|s| s.class)
    }

    /// Noms dans l'ordre du profil.
    pub fn names(&self, profile: Profile) -> impl Iterator<Item = &'static str> + '_ {
        let observed = matches!(profile, Profile::Observed) && !self.observed.is_empty();
        let (a, b) = if observed {
            (None, Some(self.observed.iter().copied()))
        } else {
            (Some(self.strict.iter().map(|s| s.name)), None)
        };
        a.into_iter().flatten().chain(b.into_iter().flatten())
    }

    /// Noms du profil avec leur classe (`None` : hors schéma, `GUID`
    /// compris), dans l'ordre.
    #[must_use]
    pub fn classed(&self, profile: Profile) -> &[(&'static str, Option<Class>)] {
        let index = self.index();
        if matches!(profile, Profile::Observed) && !self.observed.is_empty() {
            &index.observed_classes
        } else {
            &index.strict_classes
        }
    }

    /// Rang d'un nom dans le profil ; `None` hors du profil.
    #[must_use]
    pub fn position(&self, profile: Profile, name: &str) -> Option<usize> {
        let index = self.index();
        if matches!(profile, Profile::Observed) && !self.observed.is_empty() {
            index.observed.get(name).copied()
        } else {
            index.strict.get(name).copied()
        }
    }

    fn index(&self) -> &Index {
        let cell = match self.parent {
            Parent::Project => &INDEXES.0[0],
            Parent::Calendar => &INDEXES.0[1],
            Parent::WeekDay => &INDEXES.0[2],
            Parent::Exception => &INDEXES.0[3],
            Parent::WorkWeek => &INDEXES.0[4],
            Parent::Task => &INDEXES.0[5],
            Parent::PredecessorLink => &INDEXES.0[6],
            Parent::TaskBaseline => &INDEXES.0[7],
            Parent::Resource => &INDEXES.0[8],
            Parent::ResourceBaseline => &INDEXES.0[9],
            Parent::Assignment => &INDEXES.0[10],
            Parent::AssignmentBaseline => &INDEXES.0[11],
        };
        cell.get_or_init(|| {
            let strict: BTreeMap<&'static str, usize> =
                self.strict.iter().enumerate().map(|(i, s)| (s.name, i)).collect();
            let class = |n: &str| strict.get(n).and_then(|i| self.strict.get(*i)).map(|s| s.class);
            let strict_classes = self.strict.iter().map(|s| (s.name, Some(s.class))).collect();
            let observed_classes = self.observed.iter().map(|n| (*n, class(n))).collect();
            Index {
                observed: self.observed.iter().enumerate().map(|(i, n)| (*n, i)).collect(),
                strict,
                strict_classes,
                observed_classes,
            }
        })
    }
}

/// Index construits à la première utilisation, un par parent.
struct Indexes([OnceLock<Index>; 12]);

static INDEXES: Indexes = Indexes([const { OnceLock::new() }; 12]);

/// Séquence d'un parent.
#[must_use]
pub const fn sequence(parent: Parent) -> &'static Sequence {
    match parent {
        Parent::Project => &SEQ_PROJECT,
        Parent::Calendar => &SEQ_CALENDAR,
        Parent::WeekDay => &SEQ_WEEK_DAY,
        Parent::Exception => &SEQ_EXCEPTION,
        Parent::WorkWeek => &SEQ_WORK_WEEK,
        Parent::Task => &SEQ_TASK,
        Parent::PredecessorLink => &SEQ_PREDECESSOR_LINK,
        Parent::TaskBaseline => &SEQ_TASK_BASELINE,
        Parent::Resource => &SEQ_RESOURCE,
        Parent::ResourceBaseline => &SEQ_RESOURCE_BASELINE,
        Parent::Assignment => &SEQ_ASSIGNMENT,
        Parent::AssignmentBaseline => &SEQ_ASSIGNMENT_BASELINE,
    }
}

macro_rules! seq {
    ($name:ident, $parent:ident, $strict:ident, $observed:ident) => {
        static $name: Sequence = Sequence {
            parent: Parent::$parent,
            strict: $strict,
            observed: $observed,
        };
    };
}

seq!(SEQ_PROJECT, Project, PROJECT, PROJECT_OBSERVED);
seq!(SEQ_CALENDAR, Calendar, CALENDAR, CALENDAR_OBSERVED);
seq!(SEQ_WEEK_DAY, WeekDay, WEEK_DAY, WEEK_DAY_OBSERVED);
seq!(SEQ_EXCEPTION, Exception, EXCEPTION, EXCEPTION_OBSERVED);
seq!(SEQ_WORK_WEEK, WorkWeek, WORK_WEEK, WORK_WEEK_OBSERVED);
seq!(SEQ_TASK, Task, TASK, TASK_OBSERVED);
seq!(
    SEQ_PREDECESSOR_LINK,
    PredecessorLink,
    PREDECESSOR_LINK,
    PREDECESSOR_LINK_OBSERVED
);
seq!(
    SEQ_TASK_BASELINE,
    TaskBaseline,
    TASK_BASELINE,
    TASK_BASELINE_OBSERVED
);
seq!(SEQ_RESOURCE, Resource, RESOURCE, RESOURCE_OBSERVED);
seq!(
    SEQ_RESOURCE_BASELINE,
    ResourceBaseline,
    RESOURCE_BASELINE,
    RESOURCE_BASELINE_OBSERVED
);
seq!(SEQ_ASSIGNMENT, Assignment, ASSIGNMENT, ASSIGNMENT_OBSERVED);
seq!(
    SEQ_ASSIGNMENT_BASELINE,
    AssignmentBaseline,
    ASSIGNMENT_BASELINE,
    ASSIGNMENT_BASELINE_OBSERVED
);

// --- Tables (générées depuis pj15, classes selon la spécification J3) -------

use Class::{
    Computed as C, Derived as D, Dropped as X, Format as F, Modeled as M, ModeledOpt as MO, Preserved as P,
};

/// `Project` : séquence pj15.
const PROJECT: &[Slot] = &[
    Slot::new("SaveVersion", F, 1, false, None),
    Slot::new("UID", P, 0, false, Some(16)),
    Slot::new("Name", M, 0, false, Some(255)),
    Slot::new("Title", M, 0, false, Some(512)),
    Slot::new("Subject", M, 0, false, Some(512)),
    Slot::new("Category", M, 0, false, Some(512)),
    Slot::new("Company", M, 0, false, Some(512)),
    Slot::new("Manager", M, 0, false, Some(512)),
    Slot::new("Author", M, 0, false, Some(512)),
    Slot::new("CreationDate", MO, 0, false, None),
    Slot::new("Revision", P, 0, false, None),
    Slot::new("LastSaved", P, 0, false, None),
    Slot::new("ScheduleFromStart", M, 0, false, None),
    Slot::new("StartDate", MO, 0, false, None),
    Slot::new("FinishDate", MO, 0, false, None),
    Slot::new("FYStartDate", P, 0, false, None),
    Slot::new("CriticalSlackLimit", M, 0, false, None),
    Slot::new("CurrencyDigits", P, 0, false, None),
    Slot::new("CurrencySymbol", P, 0, false, Some(20)),
    Slot::new("CurrencyCode", P, 1, false, Some(3)),
    Slot::new("CurrencySymbolPosition", P, 0, false, None),
    Slot::new("CalendarUID", M, 0, false, None),
    Slot::new("DefaultStartTime", M, 0, false, None),
    Slot::new("DefaultFinishTime", M, 0, false, None),
    Slot::new("MinutesPerDay", M, 0, false, None),
    Slot::new("MinutesPerWeek", M, 0, false, None),
    Slot::new("DaysPerMonth", M, 0, false, None),
    Slot::new("DefaultTaskType", M, 0, false, None),
    Slot::new("DefaultFixedCostAccrual", P, 0, false, None),
    Slot::new("DefaultStandardRate", P, 0, false, None),
    Slot::new("DefaultOvertimeRate", P, 0, false, None),
    Slot::new("DurationFormat", M, 0, false, None),
    Slot::new("WorkFormat", M, 0, false, None),
    Slot::new("EditableActualCosts", P, 0, false, None),
    Slot::new("HonorConstraints", M, 0, false, None),
    Slot::new("EarnedValueMethod", P, 0, false, None),
    Slot::new("InsertedProjectsLikeSummary", P, 0, false, None),
    Slot::new("MultipleCriticalPaths", M, 0, false, None),
    Slot::new("NewTasksEffortDriven", M, 0, false, None),
    Slot::new("NewTasksEstimated", M, 0, false, None),
    Slot::new("SplitsInProgressTasks", M, 0, false, None),
    Slot::new("SpreadActualCost", P, 0, false, None),
    Slot::new("SpreadPercentComplete", P, 0, false, None),
    Slot::new("TaskUpdatesResource", M, 0, false, None),
    Slot::new("FiscalYearStart", P, 0, false, None),
    Slot::new("WeekStartDay", M, 0, false, None),
    Slot::new("MoveCompletedEndsBack", M, 0, false, None),
    Slot::new("MoveRemainingStartsBack", M, 0, false, None),
    Slot::new("MoveRemainingStartsForward", M, 0, false, None),
    Slot::new("MoveCompletedEndsForward", M, 0, false, None),
    Slot::new("BaselineForEarnedValue", P, 0, false, None),
    Slot::new("AutoAddNewResourcesAndTasks", P, 0, false, None),
    Slot::new("StatusDate", MO, 0, false, None),
    Slot::new("CurrentDate", M, 0, false, None),
    Slot::new("MicrosoftProjectServerURL", P, 0, false, None),
    Slot::new("Autolink", P, 0, false, None),
    Slot::new("NewTaskStartDate", P, 0, false, None),
    Slot::new("DefaultTaskEVMethod", P, 0, false, None),
    Slot::new("ProjectExternallyEdited", P, 0, false, None),
    Slot::new("ExtendedCreationDate", P, 0, false, None),
    Slot::new("ActualsInSync", P, 0, false, None),
    Slot::new("RemoveFileProperties", P, 0, false, None),
    Slot::new("AdminProject", P, 0, false, None),
    Slot::new("BaslineCalendar", P, 0, false, None),
    Slot::new("NewTasksAreManual", P, 0, false, None),
    Slot::new("UpdateManuallyScheduledTasksWhenEditingLinks", P, 0, false, None),
    Slot::new(
        "KeepTaskOnNearestWorkingTimeWhenMadeAutoScheduled",
        P,
        0,
        false,
        None,
    ),
    Slot::new("OutlineCodes", P, 0, false, None),
    Slot::new("WBSMasks", P, 0, false, None),
    Slot::new("ExtendedAttributes", P, 0, false, None),
    Slot::new("Calendars", MO, 0, false, None),
    Slot::new("Tasks", M, 0, false, None),
    Slot::new("Resources", MO, 0, false, None),
    Slot::new("Assignments", MO, 0, false, None),
];

/// `Project` : ordre observé (O19, R-121).
const PROJECT_OBSERVED: &[&str] = &[
    "SaveVersion",
    "UID",
    "Name",
    "GUID",
    "Title",
    "Subject",
    "Category",
    "Company",
    "Manager",
    "Author",
    "CreationDate",
    "Revision",
    "LastSaved",
    "ScheduleFromStart",
    "StartDate",
    "FinishDate",
    "FYStartDate",
    "CriticalSlackLimit",
    "CurrencyDigits",
    "CurrencySymbol",
    "CurrencyCode",
    "CurrencySymbolPosition",
    "CalendarUID",
    "DefaultStartTime",
    "DefaultFinishTime",
    "MinutesPerDay",
    "MinutesPerWeek",
    "DaysPerMonth",
    "DefaultTaskType",
    "DefaultFixedCostAccrual",
    "DefaultStandardRate",
    "DefaultOvertimeRate",
    "DurationFormat",
    "WorkFormat",
    "EditableActualCosts",
    "HonorConstraints",
    "EarnedValueMethod",
    "InsertedProjectsLikeSummary",
    "MultipleCriticalPaths",
    "NewTasksEffortDriven",
    "NewTasksEstimated",
    "SplitsInProgressTasks",
    "SpreadActualCost",
    "SpreadPercentComplete",
    "TaskUpdatesResource",
    "FiscalYearStart",
    "WeekStartDay",
    "MoveCompletedEndsBack",
    "MoveRemainingStartsBack",
    "MoveRemainingStartsForward",
    "MoveCompletedEndsForward",
    "BaselineForEarnedValue",
    "AutoAddNewResourcesAndTasks",
    "StatusDate",
    "CurrentDate",
    "MicrosoftProjectServerURL",
    "Autolink",
    "NewTaskStartDate",
    "NewTasksAreManual",
    "DefaultTaskEVMethod",
    "ProjectExternallyEdited",
    "ExtendedCreationDate",
    "ActualsInSync",
    "RemoveFileProperties",
    "AdminProject",
    "BaslineCalendar",
    "UpdateManuallyScheduledTasksWhenEditingLinks",
    "KeepTaskOnNearestWorkingTimeWhenMadeAutoScheduled",
    "OutlineCodes",
    "WBSMasks",
    "ExtendedAttributes",
    "Calendars",
    "Tasks",
    "Resources",
    "Assignments",
];

/// `Calendar` : séquence pj15.
const CALENDAR: &[Slot] = &[
    Slot::new("UID", M, 1, false, None),
    Slot::new("Name", M, 0, false, Some(512)),
    Slot::new("IsBaseCalendar", D, 0, false, None),
    Slot::new("IsBaselineCalendar", P, 0, false, None),
    Slot::new("BaseCalendarUID", M, 0, false, None),
    Slot::new("WeekDays", MO, 0, false, None),
    Slot::new("Exceptions", MO, 0, false, None),
    Slot::new("WorkWeeks", MO, 0, false, None),
];

/// `Calendar` : ordre observé (O19, R-121).
const CALENDAR_OBSERVED: &[&str] = &[
    "UID",
    "GUID",
    "Name",
    "IsBaseCalendar",
    "IsBaselineCalendar",
    "BaseCalendarUID",
    "WeekDays",
    "Exceptions",
    "WorkWeeks",
];

/// `WeekDay` : séquence pj15.
const WEEK_DAY: &[Slot] = &[
    Slot::new("DayType", M, 1, false, None),
    Slot::new("DayWorking", M, 0, false, None),
    Slot::new("TimePeriod", MO, 0, false, None),
    Slot::new("WorkingTimes", MO, 0, false, None),
];

/// `WeekDay` : ordre observé identique à pj15.
const WEEK_DAY_OBSERVED: &[&str] = &[];

/// `Exception` : séquence pj15.
const EXCEPTION: &[Slot] = &[
    Slot::new("EnteredByOccurrences", M, 0, false, None),
    Slot::new("TimePeriod", M, 0, false, None),
    Slot::new("Occurrences", MO, 0, false, None),
    Slot::new("Name", M, 0, false, Some(512)),
    Slot::new("Type", M, 0, false, None),
    Slot::new("Period", MO, 0, false, None),
    Slot::new("DaysOfWeek", MO, 0, false, None),
    Slot::new("MonthItem", MO, 0, false, None),
    Slot::new("MonthPosition", MO, 0, false, None),
    Slot::new("Month", MO, 0, false, None),
    Slot::new("MonthDay", MO, 0, false, None),
    Slot::new("DayWorking", M, 0, false, None),
    Slot::new("WorkingTimes", MO, 0, false, None),
];

/// `Exception` : ordre observé identique à pj15.
const EXCEPTION_OBSERVED: &[&str] = &[];

/// `WorkWeek` : séquence pj15.
const WORK_WEEK: &[Slot] = &[
    Slot::new("TimePeriod", M, 0, false, None),
    Slot::new("Name", M, 0, false, Some(512)),
    Slot::new("WeekDay", MO, 0, true, None),
];

/// `WorkWeek` : ordre observé (O19, R-121).
const WORK_WEEK_OBSERVED: &[&str] = &["TimePeriod", "Name", "WeekDays"];

/// `Task` : séquence pj15.
const TASK: &[Slot] = &[
    Slot::new("UID", M, 1, false, None),
    Slot::new("ID", D, 0, false, None),
    Slot::new("Name", M, 0, false, Some(512)),
    Slot::new("Type", M, 0, false, None),
    Slot::new("IsNull", M, 0, false, None),
    Slot::new("CreateDate", MO, 0, false, None),
    Slot::new("Contact", P, 0, false, Some(512)),
    Slot::new("WBS", MO, 0, false, None),
    Slot::new("WBSLevel", P, 0, false, None),
    Slot::new("OutlineNumber", D, 0, false, Some(512)),
    Slot::new("OutlineLevel", M, 0, false, None),
    Slot::new("Priority", M, 0, false, None),
    Slot::new("Start", C, 0, false, None),
    Slot::new("Finish", C, 0, false, None),
    Slot::new("Duration", MO, 0, false, None),
    Slot::new("DurationFormat", M, 0, false, None),
    Slot::new("Work", M, 0, false, None),
    Slot::new("Stop", C, 0, false, None),
    Slot::new("Resume", MO, 0, false, None),
    Slot::new("ResumeValid", P, 0, false, None),
    Slot::new("EffortDriven", M, 0, false, None),
    Slot::new("Recurring", M, 0, false, None),
    Slot::new("OverAllocated", X, 0, false, None),
    Slot::new("Estimated", M, 0, false, None),
    Slot::new("Milestone", M, 0, false, None),
    Slot::new("Summary", D, 0, false, None),
    Slot::new("DisplayAsSummary", P, 0, false, None),
    Slot::new("Critical", C, 0, false, None),
    Slot::new("IsSubproject", P, 0, false, None),
    Slot::new("IsSubprojectReadOnly", P, 0, false, None),
    Slot::new("SubprojectName", P, 0, false, Some(512)),
    Slot::new("ExternalTask", P, 0, false, None),
    Slot::new("ExternalTaskProject", P, 0, false, Some(512)),
    Slot::new("EarlyStart", C, 0, false, None),
    Slot::new("EarlyFinish", C, 0, false, None),
    Slot::new("LateStart", C, 0, false, None),
    Slot::new("LateFinish", C, 0, false, None),
    Slot::new("StartVariance", C, 0, false, None),
    Slot::new("FinishVariance", C, 0, false, None),
    Slot::new("WorkVariance", C, 0, false, None),
    Slot::new("FreeSlack", C, 0, false, None),
    Slot::new("StartSlack", C, 0, false, None),
    Slot::new("FinishSlack", C, 0, false, None),
    Slot::new("TotalSlack", C, 0, false, None),
    Slot::new("FixedCost", P, 0, false, None),
    Slot::new("FixedCostAccrual", P, 0, false, None),
    Slot::new("PercentComplete", M, 0, false, None),
    Slot::new("PercentWorkComplete", M, 0, false, None),
    Slot::new("Cost", X, 0, false, None),
    Slot::new("OvertimeCost", X, 0, false, None),
    Slot::new("OvertimeWork", X, 0, false, None),
    Slot::new("ActualStart", MO, 0, false, None),
    Slot::new("ActualFinish", MO, 0, false, None),
    Slot::new("ActualDuration", M, 0, false, None),
    Slot::new("ActualCost", X, 0, false, None),
    Slot::new("ActualOvertimeCost", X, 0, false, None),
    Slot::new("ActualWork", M, 0, false, None),
    Slot::new("ActualOvertimeWork", P, 0, false, None),
    Slot::new("RegularWork", X, 0, false, None),
    Slot::new("RemainingDuration", M, 0, false, None),
    Slot::new("RemainingCost", X, 0, false, None),
    Slot::new("RemainingWork", M, 0, false, None),
    Slot::new("RemainingOvertimeCost", X, 0, false, None),
    Slot::new("RemainingOvertimeWork", P, 0, false, None),
    Slot::new("ACWP", X, 0, false, None),
    Slot::new("CV", X, 0, false, None),
    Slot::new("ConstraintType", M, 0, false, None),
    Slot::new("CalendarUID", M, 0, false, None),
    Slot::new("ConstraintDate", MO, 0, false, None),
    Slot::new("Deadline", MO, 0, false, None),
    Slot::new("LevelAssignments", P, 0, false, None),
    Slot::new("LevelingCanSplit", P, 0, false, None),
    Slot::new("LevelingDelay", MO, 0, false, None),
    Slot::new("LevelingDelayFormat", MO, 0, false, None),
    Slot::new("PreLeveledStart", P, 0, false, None),
    Slot::new("PreLeveledFinish", P, 0, false, None),
    Slot::new("Hyperlink", P, 0, false, Some(512)),
    Slot::new("HyperlinkAddress", P, 0, false, Some(512)),
    Slot::new("HyperlinkSubAddress", P, 0, false, Some(512)),
    Slot::new("IgnoreResourceCalendar", M, 0, false, None),
    Slot::new("Notes", M, 0, false, None),
    Slot::new("HideBar", P, 0, false, None),
    Slot::new("Rollup", P, 0, false, None),
    Slot::new("BCWS", X, 0, false, None),
    Slot::new("BCWP", X, 0, false, None),
    Slot::new("PhysicalPercentComplete", M, 0, false, None),
    Slot::new("EarnedValueMethod", P, 0, false, None),
    Slot::new("PredecessorLink", MO, 0, true, None),
    Slot::new("ActualWorkProtected", P, 0, false, None),
    Slot::new("ActualOvertimeWorkProtected", P, 0, false, None),
    Slot::new("ExtendedAttribute", P, 0, true, None),
    Slot::new("Baseline", MO, 0, true, None),
    Slot::new("OutlineCode", P, 0, true, None),
    Slot::new("IsPublished", P, 0, false, None),
    Slot::new("StatusManager", P, 0, false, None),
    Slot::new("CommitmentStart", P, 0, false, None),
    Slot::new("CommitmentFinish", P, 0, false, None),
    Slot::new("CommitmentType", P, 0, false, None),
    Slot::new("Active", M, 0, false, None),
    Slot::new("Manual", M, 0, false, None),
    Slot::new("ManualStart", MO, 0, false, None),
    Slot::new("ManualFinish", MO, 0, false, None),
    Slot::new("ManualDuration", MO, 0, false, None),
    Slot::new("TimephasedData", P, 0, true, None),
];

/// `Task` : ordre observé (O19, R-121).
const TASK_OBSERVED: &[&str] = &[
    "UID",
    "GUID",
    "ID",
    "Name",
    "Active",
    "Manual",
    "Type",
    "IsNull",
    "CreateDate",
    "Contact",
    "WBS",
    "WBSLevel",
    "OutlineNumber",
    "OutlineLevel",
    "Priority",
    "Start",
    "Finish",
    "Duration",
    "ManualStart",
    "ManualFinish",
    "ManualDuration",
    "DurationFormat",
    "Work",
    "Stop",
    "Resume",
    "ResumeValid",
    "EffortDriven",
    "Recurring",
    "OverAllocated",
    "Estimated",
    "Milestone",
    "Summary",
    "DisplayAsSummary",
    "Critical",
    "IsSubproject",
    "IsSubprojectReadOnly",
    "SubprojectName",
    "ExternalTask",
    "ExternalTaskProject",
    "EarlyStart",
    "EarlyFinish",
    "LateStart",
    "LateFinish",
    "StartVariance",
    "FinishVariance",
    "WorkVariance",
    "FreeSlack",
    "TotalSlack",
    "StartSlack",
    "FinishSlack",
    "FixedCost",
    "FixedCostAccrual",
    "PercentComplete",
    "PercentWorkComplete",
    "Cost",
    "OvertimeCost",
    "OvertimeWork",
    "ActualStart",
    "ActualFinish",
    "ActualDuration",
    "ActualCost",
    "ActualOvertimeCost",
    "ActualWork",
    "ActualOvertimeWork",
    "RegularWork",
    "RemainingDuration",
    "RemainingCost",
    "RemainingWork",
    "RemainingOvertimeCost",
    "RemainingOvertimeWork",
    "ACWP",
    "CV",
    "ConstraintType",
    "CalendarUID",
    "ConstraintDate",
    "Deadline",
    "LevelAssignments",
    "LevelingCanSplit",
    "LevelingDelay",
    "LevelingDelayFormat",
    "PreLeveledStart",
    "PreLeveledFinish",
    "Hyperlink",
    "HyperlinkAddress",
    "HyperlinkSubAddress",
    "IgnoreResourceCalendar",
    "Notes",
    "HideBar",
    "Rollup",
    "BCWS",
    "BCWP",
    "PhysicalPercentComplete",
    "EarnedValueMethod",
    "PredecessorLink",
    "ActualWorkProtected",
    "ActualOvertimeWorkProtected",
    "IsPublished",
    "StatusManager",
    "CommitmentStart",
    "CommitmentFinish",
    "CommitmentType",
    "ExtendedAttribute",
    "Baseline",
    "OutlineCode",
    "TimephasedData",
];

/// `PredecessorLink` : séquence pj15.
const PREDECESSOR_LINK: &[Slot] = &[
    Slot::new("PredecessorUID", M, 0, false, None),
    Slot::new("Type", M, 0, false, None),
    Slot::new("CrossProject", M, 0, false, None),
    Slot::new("CrossProjectName", P, 0, false, None),
    Slot::new("LinkLag", M, 0, false, None),
    Slot::new("LagFormat", M, 0, false, None),
];

/// `PredecessorLink` : ordre observé identique à pj15.
const PREDECESSOR_LINK_OBSERVED: &[&str] = &[];

/// `TaskBaseline` : séquence pj15.
const TASK_BASELINE: &[Slot] = &[
    Slot::new("TimephasedData", P, 0, true, None),
    Slot::new("Number", M, 0, false, None),
    Slot::new("Interim", P, 0, false, None),
    Slot::new("Start", MO, 0, false, None),
    Slot::new("Finish", MO, 0, false, None),
    Slot::new("Duration", MO, 0, false, None),
    Slot::new("DurationFormat", MO, 0, false, None),
    Slot::new("EstimatedDuration", MO, 0, false, None),
    Slot::new("Work", MO, 0, false, None),
    Slot::new("Cost", MO, 0, false, None),
    Slot::new("BCWS", X, 0, false, None),
    Slot::new("BCWP", X, 0, false, None),
    Slot::new("FixedCost", MO, 0, false, None),
];

/// `TaskBaseline` : ordre observé identique à pj15.
const TASK_BASELINE_OBSERVED: &[&str] = &[];

/// `Resource` : séquence pj15.
const RESOURCE: &[Slot] = &[
    Slot::new("UID", M, 1, false, None),
    Slot::new("ID", D, 0, false, None),
    Slot::new("Name", M, 0, false, Some(512)),
    Slot::new("Type", M, 0, false, None),
    Slot::new("IsNull", P, 0, false, None),
    Slot::new("Initials", M, 0, false, Some(512)),
    Slot::new("Phonetics", P, 0, false, Some(512)),
    Slot::new("NTAccount", P, 0, false, Some(512)),
    Slot::new("MaterialLabel", M, 0, false, Some(512)),
    Slot::new("Code", P, 0, false, Some(512)),
    Slot::new("Group", M, 0, false, Some(512)),
    Slot::new("WorkGroup", P, 0, false, None),
    Slot::new("EmailAddress", P, 0, false, Some(512)),
    Slot::new("Hyperlink", P, 0, false, Some(512)),
    Slot::new("HyperlinkAddress", P, 0, false, Some(512)),
    Slot::new("HyperlinkSubAddress", P, 0, false, Some(512)),
    Slot::new("MaxUnits", M, 0, false, None),
    Slot::new("PeakUnits", X, 0, false, None),
    Slot::new("OverAllocated", C, 0, false, None),
    Slot::new("AvailableFrom", P, 0, false, None),
    Slot::new("AvailableTo", P, 0, false, None),
    Slot::new("Start", X, 0, false, None),
    Slot::new("Finish", X, 0, false, None),
    Slot::new("CanLevel", P, 0, false, None),
    Slot::new("AccrueAt", P, 0, false, None),
    Slot::new("Work", C, 0, false, None),
    Slot::new("RegularWork", X, 0, false, None),
    Slot::new("OvertimeWork", X, 0, false, None),
    Slot::new("ActualWork", X, 0, false, None),
    Slot::new("RemainingWork", X, 0, false, None),
    Slot::new("ActualOvertimeWork", X, 0, false, None),
    Slot::new("RemainingOvertimeWork", X, 0, false, None),
    Slot::new("PercentWorkComplete", X, 0, false, None),
    Slot::new("StandardRate", M, 0, false, None),
    Slot::new("StandardRateFormat", M, 0, false, None),
    Slot::new("Cost", X, 0, false, None),
    Slot::new("OvertimeRate", M, 0, false, None),
    Slot::new("OvertimeRateFormat", M, 0, false, None),
    Slot::new("OvertimeCost", X, 0, false, None),
    Slot::new("CostPerUse", M, 0, false, None),
    Slot::new("ActualCost", X, 0, false, None),
    Slot::new("ActualOvertimeCost", X, 0, false, None),
    Slot::new("RemainingCost", X, 0, false, None),
    Slot::new("RemainingOvertimeCost", X, 0, false, None),
    Slot::new("WorkVariance", C, 0, false, None),
    Slot::new("CostVariance", X, 0, false, None),
    Slot::new("SV", X, 0, false, None),
    Slot::new("CV", X, 0, false, None),
    Slot::new("ACWP", X, 0, false, None),
    Slot::new("CalendarUID", M, 0, false, None),
    Slot::new("Notes", M, 0, false, None),
    Slot::new("BCWS", X, 0, false, None),
    Slot::new("BCWP", X, 0, false, None),
    Slot::new("IsGeneric", M, 0, false, None),
    Slot::new("IsInactive", M, 0, false, None),
    Slot::new("IsEnterprise", P, 0, false, None),
    Slot::new("BookingType", P, 0, false, None),
    Slot::new("ActualWorkProtected", P, 0, false, None),
    Slot::new("ActualOvertimeWorkProtected", P, 0, false, None),
    Slot::new("ActiveDirectoryGUID", P, 0, false, Some(16)),
    Slot::new("CreationDate", P, 0, false, None),
    Slot::new("ExtendedAttribute", P, 0, true, None),
    Slot::new("Baseline", MO, 0, true, None),
    Slot::new("OutlineCode", P, 0, true, None),
    Slot::new("IsCostResource", M, 0, false, None),
    Slot::new("AssnOwner", P, 0, false, None),
    Slot::new("AssnOwnerGuid", P, 0, false, None),
    Slot::new("IsBudget", P, 0, false, None),
    Slot::new("AvailabilityPeriods", P, 0, false, None),
    Slot::new("Rates", P, 0, false, None),
    Slot::new("TimephasedData", P, 0, true, None),
];

/// `Resource` : ordre observé (O19, R-121).
const RESOURCE_OBSERVED: &[&str] = &[
    "UID",
    "GUID",
    "ID",
    "Name",
    "Type",
    "IsNull",
    "Initials",
    "Phonetics",
    "NTAccount",
    "MaterialLabel",
    "Code",
    "Group",
    "WorkGroup",
    "EmailAddress",
    "Hyperlink",
    "HyperlinkAddress",
    "HyperlinkSubAddress",
    "MaxUnits",
    "PeakUnits",
    "OverAllocated",
    "AvailableFrom",
    "AvailableTo",
    "Start",
    "Finish",
    "CanLevel",
    "AccrueAt",
    "Work",
    "RegularWork",
    "OvertimeWork",
    "ActualWork",
    "RemainingWork",
    "ActualOvertimeWork",
    "RemainingOvertimeWork",
    "PercentWorkComplete",
    "StandardRate",
    "StandardRateFormat",
    "Cost",
    "OvertimeRate",
    "OvertimeRateFormat",
    "OvertimeCost",
    "CostPerUse",
    "ActualCost",
    "ActualOvertimeCost",
    "RemainingCost",
    "RemainingOvertimeCost",
    "WorkVariance",
    "CostVariance",
    "SV",
    "CV",
    "ACWP",
    "CalendarUID",
    "Notes",
    "BCWS",
    "BCWP",
    "IsGeneric",
    "IsInactive",
    "IsEnterprise",
    "BookingType",
    "ActualWorkProtected",
    "ActualOvertimeWorkProtected",
    "ActiveDirectoryGUID",
    "CreationDate",
    "ExtendedAttribute",
    "OutlineCode",
    "IsCostResource",
    "AssnOwner",
    "AssnOwnerGuid",
    "IsBudget",
    "Baseline",
    "AvailabilityPeriods",
    "Rates",
    "TimephasedData",
];

/// `ResourceBaseline` : séquence pj15.
const RESOURCE_BASELINE: &[Slot] = &[
    Slot::new("Number", M, 1, false, None),
    Slot::new("Work", MO, 0, false, None),
    Slot::new("Cost", MO, 0, false, None),
    Slot::new("BCWS", X, 0, false, None),
    Slot::new("BCWP", X, 0, false, None),
];

/// `ResourceBaseline` : ordre observé identique à pj15.
const RESOURCE_BASELINE_OBSERVED: &[&str] = &[];

/// `Assignment` : séquence pj15.
const ASSIGNMENT: &[Slot] = &[
    Slot::new("UID", M, 1, false, None),
    Slot::new("TaskUID", M, 0, false, None),
    Slot::new("ResourceUID", M, 0, false, None),
    Slot::new("PercentWorkComplete", X, 0, false, None),
    Slot::new("ActualCost", X, 0, false, None),
    Slot::new("ActualFinish", C, 0, false, None),
    Slot::new("ActualOvertimeCost", X, 0, false, None),
    Slot::new("ActualOvertimeWork", P, 0, false, None),
    Slot::new("ActualStart", C, 0, false, None),
    Slot::new("ActualWork", M, 0, false, None),
    Slot::new("ACWP", X, 0, false, None),
    Slot::new("Confirmed", P, 0, false, None),
    Slot::new("Cost", X, 0, false, None),
    Slot::new("CostRateTable", P, 0, false, None),
    Slot::new("CostVariance", X, 0, false, None),
    Slot::new("CV", X, 0, false, None),
    Slot::new("Delay", M, 0, false, None),
    Slot::new("Finish", C, 0, false, None),
    Slot::new("FinishVariance", C, 0, false, None),
    Slot::new("Hyperlink", P, 0, false, Some(512)),
    Slot::new("HyperlinkAddress", P, 0, false, Some(512)),
    Slot::new("HyperlinkSubAddress", P, 0, false, Some(512)),
    Slot::new("WorkVariance", C, 0, false, None),
    Slot::new("HasFixedRateUnits", P, 0, false, None),
    Slot::new("FixedMaterial", P, 0, false, None),
    Slot::new("LevelingDelay", MO, 0, false, None),
    Slot::new("LevelingDelayFormat", MO, 0, false, None),
    Slot::new("LinkedFields", P, 0, false, None),
    Slot::new("Milestone", X, 0, false, None),
    Slot::new("Notes", P, 0, false, None),
    Slot::new("Overallocated", X, 0, false, None),
    Slot::new("OvertimeCost", X, 0, false, None),
    Slot::new("OvertimeWork", P, 0, false, None),
    Slot::new("PeakUnits", X, 0, false, None),
    Slot::new("RateScale", P, 0, false, None),
    Slot::new("RegularWork", X, 0, false, None),
    Slot::new("RemainingCost", X, 0, false, None),
    Slot::new("RemainingOvertimeCost", X, 0, false, None),
    Slot::new("RemainingOvertimeWork", P, 0, false, None),
    Slot::new("RemainingWork", M, 0, false, None),
    Slot::new("ResponsePending", P, 0, false, None),
    Slot::new("Start", C, 0, false, None),
    Slot::new("Stop", C, 0, false, None),
    Slot::new("Resume", C, 0, false, None),
    Slot::new("StartVariance", C, 0, false, None),
    Slot::new("Summary", X, 0, false, None),
    Slot::new("SV", X, 0, false, None),
    Slot::new("Units", M, 0, false, None),
    Slot::new("UpdateNeeded", P, 0, false, None),
    Slot::new("VAC", X, 0, false, None),
    Slot::new("Work", M, 0, false, None),
    Slot::new("WorkContour", M, 0, false, None),
    Slot::new("BCWS", X, 0, false, None),
    Slot::new("BCWP", X, 0, false, None),
    Slot::new("BookingType", P, 0, false, None),
    Slot::new("ActualWorkProtected", P, 0, false, None),
    Slot::new("ActualOvertimeWorkProtected", P, 0, false, None),
    Slot::new("CreationDate", P, 0, false, None),
    Slot::new("AssnOwner", P, 0, false, None),
    Slot::new("AssnOwnerGuid", P, 0, false, None),
    Slot::new("BudgetCost", P, 0, false, None),
    Slot::new("BudgetWork", P, 0, false, None),
    Slot::new("ExtendedAttribute", P, 0, true, None),
    Slot::new("Baseline", MO, 0, true, None),
    Slot::new("f404000", P, 0, false, None),
    Slot::new("f404001", P, 0, false, None),
    Slot::new("f404002", P, 0, false, None),
    Slot::new("f404003", P, 0, false, None),
    Slot::new("f404004", P, 0, false, None),
    Slot::new("f404005", P, 0, false, None),
    Slot::new("f404006", P, 0, false, None),
    Slot::new("f404007", P, 0, false, None),
    Slot::new("f404008", P, 0, false, None),
    Slot::new("f404009", P, 0, false, None),
    Slot::new("f40400a", P, 0, false, None),
    Slot::new("f40400b", P, 0, false, None),
    Slot::new("f40400c", P, 0, false, None),
    Slot::new("f40400d", P, 0, false, None),
    Slot::new("f40400e", P, 0, false, None),
    Slot::new("f40400f", P, 0, false, None),
    Slot::new("f404010", P, 0, false, None),
    Slot::new("f404011", P, 0, false, None),
    Slot::new("f404012", P, 0, false, None),
    Slot::new("f404013", P, 0, false, None),
    Slot::new("f404014", P, 0, false, None),
    Slot::new("f404015", P, 0, false, None),
    Slot::new("f404016", P, 0, false, None),
    Slot::new("f404017", P, 0, false, None),
    Slot::new("f404018", P, 0, false, None),
    Slot::new("f404019", P, 0, false, None),
    Slot::new("f40401a", P, 0, false, None),
    Slot::new("f40401b", P, 0, false, None),
    Slot::new("f40401c", P, 0, false, None),
    Slot::new("f40401d", P, 0, false, None),
    Slot::new("f40401e", P, 0, false, None),
    Slot::new("f40401f", P, 0, false, None),
    Slot::new("f404020", P, 0, false, None),
    Slot::new("f404021", P, 0, false, None),
    Slot::new("f404022", P, 0, false, None),
    Slot::new("f404023", P, 0, false, None),
    Slot::new("f404024", P, 0, false, None),
    Slot::new("f404025", P, 0, false, None),
    Slot::new("f404026", P, 0, false, None),
    Slot::new("f404027", P, 0, false, None),
    Slot::new("f404028", P, 0, false, None),
    Slot::new("f404029", P, 0, false, None),
    Slot::new("f40402a", P, 0, false, None),
    Slot::new("f40402b", P, 0, false, None),
    Slot::new("f40402c", P, 0, false, None),
    Slot::new("f40402d", P, 0, false, None),
    Slot::new("f40402e", P, 0, false, None),
    Slot::new("f40402f", P, 0, false, None),
    Slot::new("f404030", P, 0, false, None),
    Slot::new("f404031", P, 0, false, None),
    Slot::new("f404032", P, 0, false, None),
    Slot::new("f404033", P, 0, false, None),
    Slot::new("f404034", P, 0, false, None),
    Slot::new("f404035", P, 0, false, None),
    Slot::new("f404036", P, 0, false, None),
    Slot::new("f404037", P, 0, false, None),
    Slot::new("f404038", P, 0, false, None),
    Slot::new("f404039", P, 0, false, None),
    Slot::new("f40403a", P, 0, false, None),
    Slot::new("f40403b", P, 0, false, None),
    Slot::new("f40403c", P, 0, false, None),
    Slot::new("f40403d", P, 0, false, None),
    Slot::new("f40403e", P, 0, false, None),
    Slot::new("f40403f", P, 0, false, None),
    Slot::new("f404040", P, 0, false, None),
    Slot::new("f404041", P, 0, false, None),
    Slot::new("f404042", P, 0, false, None),
    Slot::new("f404043", P, 0, false, None),
    Slot::new("f404044", P, 0, false, None),
    Slot::new("f404045", P, 0, false, None),
    Slot::new("f404046", P, 0, false, None),
    Slot::new("f404047", P, 0, false, None),
    Slot::new("f404048", P, 0, false, None),
    Slot::new("f404049", P, 0, false, None),
    Slot::new("f40404a", P, 0, false, None),
    Slot::new("f40404b", P, 0, false, None),
    Slot::new("f40404c", P, 0, false, None),
    Slot::new("f40404d", P, 0, false, None),
    Slot::new("f40404e", P, 0, false, None),
    Slot::new("f40404f", P, 0, false, None),
    Slot::new("f404050", P, 0, false, None),
    Slot::new("f404051", P, 0, false, None),
    Slot::new("f404052", P, 0, false, None),
    Slot::new("f404053", P, 0, false, None),
    Slot::new("f404054", P, 0, false, None),
    Slot::new("f404055", P, 0, false, None),
    Slot::new("f404056", P, 0, false, None),
    Slot::new("f404057", P, 0, false, None),
    Slot::new("f404058", P, 0, false, None),
    Slot::new("f404059", P, 0, false, None),
    Slot::new("f40405a", P, 0, false, None),
    Slot::new("f40405b", P, 0, false, None),
    Slot::new("f40405c", P, 0, false, None),
    Slot::new("f40405d", P, 0, false, None),
    Slot::new("f40405e", P, 0, false, None),
    Slot::new("f40405f", P, 0, false, None),
    Slot::new("f404060", P, 0, false, None),
    Slot::new("f404061", P, 0, false, None),
    Slot::new("f404062", P, 0, false, None),
    Slot::new("f404063", P, 0, false, None),
    Slot::new("f404064", P, 0, false, None),
    Slot::new("f404065", P, 0, false, None),
    Slot::new("f404066", P, 0, false, None),
    Slot::new("f404067", P, 0, false, None),
    Slot::new("f404068", P, 0, false, None),
    Slot::new("f404069", P, 0, false, None),
    Slot::new("f40406a", P, 0, false, None),
    Slot::new("f40406b", P, 0, false, None),
    Slot::new("f40406c", P, 0, false, None),
    Slot::new("f40406d", P, 0, false, None),
    Slot::new("f40406e", P, 0, false, None),
    Slot::new("f40406f", P, 0, false, None),
    Slot::new("f404070", P, 0, false, None),
    Slot::new("f404071", P, 0, false, None),
    Slot::new("f404072", P, 0, false, None),
    Slot::new("f404073", P, 0, false, None),
    Slot::new("f404074", P, 0, false, None),
    Slot::new("f404075", P, 0, false, None),
    Slot::new("f404076", P, 0, false, None),
    Slot::new("f404077", P, 0, false, None),
    Slot::new("f404078", P, 0, false, None),
    Slot::new("f404079", P, 0, false, None),
    Slot::new("f40407a", P, 0, false, None),
    Slot::new("f40407b", P, 0, false, None),
    Slot::new("f40407c", P, 0, false, None),
    Slot::new("f40407d", P, 0, false, None),
    Slot::new("f40407e", P, 0, false, None),
    Slot::new("f40407f", P, 0, false, None),
    Slot::new("f404080", P, 0, false, None),
    Slot::new("f404081", P, 0, false, None),
    Slot::new("f404082", P, 0, false, None),
    Slot::new("f404083", P, 0, false, None),
    Slot::new("f404084", P, 0, false, None),
    Slot::new("f404085", P, 0, false, None),
    Slot::new("f404086", P, 0, false, None),
    Slot::new("f404087", P, 0, false, None),
    Slot::new("f404088", P, 0, false, None),
    Slot::new("f404089", P, 0, false, None),
    Slot::new("f40408a", P, 0, false, None),
    Slot::new("f40408b", P, 0, false, None),
    Slot::new("f40408c", P, 0, false, None),
    Slot::new("f40408d", P, 0, false, None),
    Slot::new("f40408e", P, 0, false, None),
    Slot::new("f40408f", P, 0, false, None),
    Slot::new("f404090", P, 0, false, None),
    Slot::new("f404091", P, 0, false, None),
    Slot::new("f404092", P, 0, false, None),
    Slot::new("f404093", P, 0, false, None),
    Slot::new("f404094", P, 0, false, None),
    Slot::new("f404095", P, 0, false, None),
    Slot::new("f404096", P, 0, false, None),
    Slot::new("f404097", P, 0, false, None),
    Slot::new("f404098", P, 0, false, None),
    Slot::new("f404099", P, 0, false, None),
    Slot::new("f40409a", P, 0, false, None),
    Slot::new("f40409b", P, 0, false, None),
    Slot::new("f40409c", P, 0, false, None),
    Slot::new("f40409d", P, 0, false, None),
    Slot::new("f40409e", P, 0, false, None),
    Slot::new("f40409f", P, 0, false, None),
    Slot::new("f4040a0", P, 0, false, None),
    Slot::new("f4040a1", P, 0, false, None),
    Slot::new("f4040a2", P, 0, false, None),
    Slot::new("f4040a3", P, 0, false, None),
    Slot::new("f4040a4", P, 0, false, None),
    Slot::new("f4040a5", P, 0, false, None),
    Slot::new("f4040a6", P, 0, false, None),
    Slot::new("f4040a7", P, 0, false, None),
    Slot::new("f4040a8", P, 0, false, None),
    Slot::new("f4040a9", P, 0, false, None),
    Slot::new("f4040aa", P, 0, false, None),
    Slot::new("f4040ab", P, 0, false, None),
    Slot::new("f4040ac", P, 0, false, None),
    Slot::new("f4040ad", P, 0, false, None),
    Slot::new("f4040ae", P, 0, false, None),
    Slot::new("f4040af", P, 0, false, None),
    Slot::new("f4040b0", P, 0, false, None),
    Slot::new("f4040b1", P, 0, false, None),
    Slot::new("f4040b2", P, 0, false, None),
    Slot::new("f4040b3", P, 0, false, None),
    Slot::new("f4040b4", P, 0, false, None),
    Slot::new("f4040b5", P, 0, false, None),
    Slot::new("f4040b6", P, 0, false, None),
    Slot::new("f4040b7", P, 0, false, None),
    Slot::new("f4040b8", P, 0, false, None),
    Slot::new("f4040b9", P, 0, false, None),
    Slot::new("f4040ba", P, 0, false, None),
    Slot::new("f4040bb", P, 0, false, None),
    Slot::new("f4040bc", P, 0, false, None),
    Slot::new("f4040bd", P, 0, false, None),
    Slot::new("f4040be", P, 0, false, None),
    Slot::new("f4040bf", P, 0, false, None),
    Slot::new("f4040c0", P, 0, false, None),
    Slot::new("f4040c1", P, 0, false, None),
    Slot::new("f4040c2", P, 0, false, None),
    Slot::new("f4040c3", P, 0, false, None),
    Slot::new("f4040c4", P, 0, false, None),
    Slot::new("f4040c5", P, 0, false, None),
    Slot::new("f4040c6", P, 0, false, None),
    Slot::new("f4040c7", P, 0, false, None),
    Slot::new("f4040c8", P, 0, false, None),
    Slot::new("TimephasedData", P, 0, true, None),
];

/// `Assignment` : ordre observé (O19, R-121).
const ASSIGNMENT_OBSERVED: &[&str] = &[
    "UID",
    "GUID",
    "TaskUID",
    "ResourceUID",
    "PercentWorkComplete",
    "ActualCost",
    "ActualFinish",
    "ActualOvertimeCost",
    "ActualOvertimeWork",
    "ActualStart",
    "ActualWork",
    "ACWP",
    "Confirmed",
    "Cost",
    "CostRateTable",
    "RateScale",
    "CostVariance",
    "CV",
    "Delay",
    "Finish",
    "FinishVariance",
    "Hyperlink",
    "HyperlinkAddress",
    "HyperlinkSubAddress",
    "WorkVariance",
    "HasFixedRateUnits",
    "FixedMaterial",
    "LevelingDelay",
    "LevelingDelayFormat",
    "LinkedFields",
    "Milestone",
    "Notes",
    "Overallocated",
    "OvertimeCost",
    "OvertimeWork",
    "PeakUnits",
    "RegularWork",
    "RemainingCost",
    "RemainingOvertimeCost",
    "RemainingOvertimeWork",
    "RemainingWork",
    "ResponsePending",
    "Start",
    "Stop",
    "Resume",
    "StartVariance",
    "Summary",
    "SV",
    "Units",
    "UpdateNeeded",
    "VAC",
    "Work",
    "WorkContour",
    "BCWS",
    "BCWP",
    "BookingType",
    "ActualWorkProtected",
    "ActualOvertimeWorkProtected",
    "CreationDate",
    "AssnOwner",
    "AssnOwnerGuid",
    "BudgetCost",
    "BudgetWork",
    "ExtendedAttribute",
    "Baseline",
    "f404000",
    "f404001",
    "f404002",
    "f404003",
    "f404004",
    "f404005",
    "f404006",
    "f404007",
    "f404008",
    "f404009",
    "f40400a",
    "f40400b",
    "f40400c",
    "f40400d",
    "f40400e",
    "f40400f",
    "f404010",
    "f404011",
    "f404012",
    "f404013",
    "f404014",
    "f404015",
    "f404016",
    "f404017",
    "f404018",
    "f404019",
    "f40401a",
    "f40401b",
    "f40401c",
    "f40401d",
    "f40401e",
    "f40401f",
    "f404020",
    "f404021",
    "f404022",
    "f404023",
    "f404024",
    "f404025",
    "f404026",
    "f404027",
    "f404028",
    "f404029",
    "f40402a",
    "f40402b",
    "f40402c",
    "f40402d",
    "f40402e",
    "f40402f",
    "f404030",
    "f404031",
    "f404032",
    "f404033",
    "f404034",
    "f404035",
    "f404036",
    "f404037",
    "f404038",
    "f404039",
    "f40403a",
    "f40403b",
    "f40403c",
    "f40403d",
    "f40403e",
    "f40403f",
    "f404040",
    "f404041",
    "f404042",
    "f404043",
    "f404044",
    "f404045",
    "f404046",
    "f404047",
    "f404048",
    "f404049",
    "f40404a",
    "f40404b",
    "f40404c",
    "f40404d",
    "f40404e",
    "f40404f",
    "f404050",
    "f404051",
    "f404052",
    "f404053",
    "f404054",
    "f404055",
    "f404056",
    "f404057",
    "f404058",
    "f404059",
    "f40405a",
    "f40405b",
    "f40405c",
    "f40405d",
    "f40405e",
    "f40405f",
    "f404060",
    "f404061",
    "f404062",
    "f404063",
    "f404064",
    "f404065",
    "f404066",
    "f404067",
    "f404068",
    "f404069",
    "f40406a",
    "f40406b",
    "f40406c",
    "f40406d",
    "f40406e",
    "f40406f",
    "f404070",
    "f404071",
    "f404072",
    "f404073",
    "f404074",
    "f404075",
    "f404076",
    "f404077",
    "f404078",
    "f404079",
    "f40407a",
    "f40407b",
    "f40407c",
    "f40407d",
    "f40407e",
    "f40407f",
    "f404080",
    "f404081",
    "f404082",
    "f404083",
    "f404084",
    "f404085",
    "f404086",
    "f404087",
    "f404088",
    "f404089",
    "f40408a",
    "f40408b",
    "f40408c",
    "f40408d",
    "f40408e",
    "f40408f",
    "f404090",
    "f404091",
    "f404092",
    "f404093",
    "f404094",
    "f404095",
    "f404096",
    "f404097",
    "f404098",
    "f404099",
    "f40409a",
    "f40409b",
    "f40409c",
    "f40409d",
    "f40409e",
    "f40409f",
    "f4040a0",
    "f4040a1",
    "f4040a2",
    "f4040a3",
    "f4040a4",
    "f4040a5",
    "f4040a6",
    "f4040a7",
    "f4040a8",
    "f4040a9",
    "f4040aa",
    "f4040ab",
    "f4040ac",
    "f4040ad",
    "f4040ae",
    "f4040af",
    "f4040b0",
    "f4040b1",
    "f4040b2",
    "f4040b3",
    "f4040b4",
    "f4040b5",
    "f4040b6",
    "f4040b7",
    "f4040b8",
    "f4040b9",
    "f4040ba",
    "f4040bb",
    "f4040bc",
    "f4040bd",
    "f4040be",
    "f4040bf",
    "f4040c0",
    "f4040c1",
    "f4040c2",
    "f4040c3",
    "f4040c4",
    "f4040c5",
    "f4040c6",
    "f4040c7",
    "f4040c8",
    "TimephasedData",
];

/// `AssignmentBaseline` : séquence pj15.
const ASSIGNMENT_BASELINE: &[Slot] = &[
    Slot::new("TimephasedData", P, 0, true, None),
    Slot::new("Number", M, 1, false, None),
    Slot::new("Start", MO, 0, false, None),
    Slot::new("Finish", MO, 0, false, None),
    Slot::new("Work", MO, 0, false, None),
    Slot::new("Cost", MO, 0, false, None),
    Slot::new("BCWS", X, 0, false, None),
    Slot::new("BCWP", X, 0, false, None),
];

/// `AssignmentBaseline` : ordre observé identique à pj15.
const ASSIGNMENT_BASELINE_OBSERVED: &[&str] = &[];

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use std::collections::BTreeSet;

    use super::{Class, Parent, Profile, sequence};

    #[test]
    fn names_are_unique_and_observed_is_a_permutation_plus_guid() {
        for parent in Parent::ALL {
            let seq = sequence(parent);
            let strict: Vec<&str> = seq.names(Profile::Strict).collect();
            let set: BTreeSet<&str> = strict.iter().copied().collect();
            assert_eq!(set.len(), strict.len(), "{parent:?} : nom en double");
            if parent == Parent::WorkWeek {
                continue;
            }
            let observed: Vec<&str> = seq.names(Profile::Observed).collect();
            let mut extra: BTreeSet<&str> = observed.iter().copied().collect();
            for n in &strict {
                assert!(extra.remove(n), "{parent:?} : {n} absent du profil observé");
            }
            let guid: BTreeSet<&str> = ["GUID"].into_iter().collect();
            assert!(extra.is_empty() || extra == guid, "{parent:?} : {extra:?}");
            assert_eq!(observed.len(), strict.len() + extra.len());
        }
    }

    #[test]
    fn lookups() {
        let task = sequence(Parent::Task);
        assert_eq!(task.class("Start"), Some(Class::Computed));
        assert_eq!(task.class("Cost"), Some(Class::Dropped));
        assert_eq!(task.class("ExtendedAttribute"), Some(Class::Preserved));
        assert_eq!(task.class("StartText"), None);
        // J5 (R-198) : retards de nivellement modélisés, valeurs
        // d'affectation et de ressource relues.
        assert_eq!(task.class("LevelingDelay"), Some(Class::ModeledOpt));
        let assignment = sequence(Parent::Assignment);
        assert_eq!(assignment.class("LevelingDelayFormat"), Some(Class::ModeledOpt));
        assert_eq!(assignment.class("Start"), Some(Class::Computed));
        assert_eq!(assignment.class("Finish"), Some(Class::Computed));
        assert_eq!(assignment.class("Overallocated"), Some(Class::Dropped));
        let resource = sequence(Parent::Resource);
        assert_eq!(resource.class("Work"), Some(Class::Computed));
        assert_eq!(resource.class("OverAllocated"), Some(Class::Computed));
        assert_eq!(resource.class("PeakUnits"), Some(Class::Dropped));
        assert_eq!(task.slot("Name").unwrap().max_len, Some(512));
        assert!(task.slot("PredecessorLink").unwrap().many);
        assert_eq!(task.position(Profile::Strict, "UID"), Some(0));
        assert_eq!(task.position(Profile::Observed, "GUID"), Some(1));
        assert_eq!(task.position(Profile::Strict, "GUID"), None);
        let observed: Vec<&str> = task.names(Profile::Observed).take(6).collect();
        assert_eq!(observed, ["UID", "GUID", "ID", "Name", "Active", "Manual"]);
        let strict: Vec<&str> = task.names(Profile::Strict).take(4).collect();
        assert_eq!(strict, ["UID", "ID", "Name", "Type"]);
        let project = sequence(Parent::Project);
        assert_eq!(project.slot("Name").unwrap().max_len, Some(255));
        assert_eq!(project.slot("CurrencyCode").unwrap().min, 1);
        assert_eq!(project.class("SaveVersion"), Some(Class::Format));
    }
}
