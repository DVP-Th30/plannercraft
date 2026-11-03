//! Comparaison champ par champ entre le planning enregistré par MS Project
//! (`RecordedSchedule`) et le planning recalculé (`Schedule`), ADR-0009 § 9.5.
//!
//! - **Cas** : une tâche (UID 0 compris) présente dans le fichier et planifiée
//!   par le moteur.
//! - **Exclus**, comptés par raison : manuelle (feuille ou récapitulative,
//!   d'après le modèle), inactive, ligne vide, exclue par le moteur, absente
//!   du modèle, aucune valeur enregistrée comparable.
//! - **Champs**, chacun comparé seulement si la valeur enregistrée existe :
//!   début, fin, dates au plus tôt et au plus tard (égalité de `Moment`),
//!   marges totale et libre (moteur en minutes × 10 contre dixièmes, exact),
//!   critique, durée (récapitulatives et UID 0 seulement).
//! - **Identique** : tous les champs comparables égaux, sans tolérance.
//!
//! Les tâches touchées par une fonction hors J2 ne sont pas exclues : elles
//! sont étiquetées (causes probables) et comptent si elles coïncident.

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::calendar::CalendarKind;
use plannercraft_core::{
    Constraint, ConstraintKind, Driver, Exclusion, LagValue, LinkType, Minutes, Moment, Project, Schedule,
    SchedulingMode, Task, TaskOutcome, TaskRole, TaskSchedule, TaskUid, TimeBasis,
};
use plannercraft_mspdi::{MspdiDocument, RecordedTask, Tenths};
use serde::Serialize;

/// Champ comparé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// `Start`.
    Start,
    /// `Finish`.
    Finish,
    /// `EarlyStart`.
    EarlyStart,
    /// `EarlyFinish`.
    EarlyFinish,
    /// `LateStart`.
    LateStart,
    /// `LateFinish`.
    LateFinish,
    /// `TotalSlack`.
    TotalSlack,
    /// `FreeSlack`.
    FreeSlack,
    /// `Critical`.
    Critical,
    /// `Duration` (récapitulatives et UID 0).
    Duration,
}

impl Field {
    /// Nom stable.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Finish => "finish",
            Self::EarlyStart => "early_start",
            Self::EarlyFinish => "early_finish",
            Self::LateStart => "late_start",
            Self::LateFinish => "late_finish",
            Self::TotalSlack => "total_slack",
            Self::FreeSlack => "free_slack",
            Self::Critical => "critical",
            Self::Duration => "duration",
        }
    }
}

/// Champs de base : tout cas d'un fichier enregistré par MS Project doit les
/// avoir tous (plus [`Field::Duration`] pour une récapitulative ou l'UID 0),
/// sinon la comparaison est incomplète et le plancher pourrait être tenu à
/// vide (relecture J2).
pub const BASE_FIELDS: [Field; 9] = [
    Field::Start,
    Field::Finish,
    Field::EarlyStart,
    Field::EarlyFinish,
    Field::LateStart,
    Field::LateFinish,
    Field::TotalSlack,
    Field::FreeSlack,
    Field::Critical,
];

/// Nature d'un cas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseKind {
    /// Feuille.
    Leaf,
    /// Récapitulative.
    Summary,
    /// Récapitulative du projet (UID 0).
    ProjectSummary,
}

impl CaseKind {
    /// Nom stable.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Leaf => "leaf",
            Self::Summary => "summary",
            Self::ProjectSummary => "project_summary",
        }
    }
}

/// Cause probable d'une divergence, calculée sur le modèle et les valeurs
/// enregistrées (étiquette, jamais une exclusion).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// Contrainte autre que « le plus tôt possible » (J3).
    Constraint,
    /// Échéance (J3).
    Deadline,
    /// Avancement : % achevé > 0 ou dates réelles (J6).
    Progress,
    /// `LevelingDelay` non nul (J5).
    LevelingDelay,
    /// Affectations : calendrier de ressource possible (J5).
    Assigned,
    /// La tâche, un ancêtre ou un prédécesseur est une récapitulative liée
    /// (R-056 à R-058).
    SummaryLink,
    /// Durée nulle (R-061).
    Milestone,
    /// Prédécesseur en planification manuelle (R-063).
    ManualPredecessor,
    /// Calendrier de tâche différent de celui du projet.
    TaskCalendar,
    /// Calendrier du projet dérivé (observation O11).
    ProjectCalendarDerived,
    /// Durée écoulée (R-067).
    Elapsed,
    /// Lien entrant avec décalage (R-041 à R-043, R-047).
    Lag,
    /// Marge enregistrée non multiple de 10 dixièmes.
    SlackNotTenthMultiple,
    /// Un prédécesseur direct ou un ancêtre diverge aussi (ajout au § 9.6 :
    /// distingue une divergence propagée d'une divergence locale).
    DivergentPredecessor,
    /// Une sous-tâche diverge (récapitulatives, UID 0).
    DivergentChild,
}

impl Cause {
    /// Nom stable.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Constraint => "constraint",
            Self::Deadline => "deadline",
            Self::Progress => "progress",
            Self::LevelingDelay => "leveling_delay",
            Self::Assigned => "assigned",
            Self::SummaryLink => "summary_link",
            Self::Milestone => "milestone",
            Self::ManualPredecessor => "manual_predecessor",
            Self::TaskCalendar => "task_calendar",
            Self::ProjectCalendarDerived => "project_calendar_derived",
            Self::Elapsed => "elapsed",
            Self::Lag => "lag",
            Self::SlackNotTenthMultiple => "slack_not_tenth_multiple",
            Self::DivergentPredecessor => "divergent_predecessor",
            Self::DivergentChild => "divergent_child",
        }
    }
}

/// Raison d'exclusion d'une tâche du fichier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Excluded {
    /// Planification manuelle (feuille ou récapitulative).
    Manual,
    /// Inactive (`Active` = 0).
    Inactive,
    /// Ligne vide.
    NullRow,
    /// UID du fichier absent du modèle lu.
    NotInModel,
    /// Exclue par le moteur.
    Engine(Exclusion),
    /// Aucune valeur enregistrée comparable.
    NoComparableValue,
}

impl Excluded {
    /// Clé stable du rapport.
    #[must_use]
    pub fn key(self) -> String {
        match self {
            Self::Manual => "manual".to_owned(),
            Self::Inactive => "inactive".to_owned(),
            Self::NullRow => "null_row".to_owned(),
            Self::NotInModel => "not_in_model".to_owned(),
            Self::NoComparableValue => "no_comparable_value".to_owned(),
            Self::Engine(e) => format!(
                "engine.{}",
                match e {
                    Exclusion::NullRow => "null_row",
                    Exclusion::Inactive => "inactive",
                    Exclusion::InactiveAncestor => "inactive_ancestor",
                    Exclusion::ManualUndated => "manual_undated",
                    Exclusion::EmptySummary => "empty_summary",
                    Exclusion::Unschedulable => "unschedulable",
                }
            ),
        }
    }
}

/// Divergence sur un champ.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Divergence {
    /// Champ.
    pub field: Field,
    /// Valeur enregistrée par MS Project.
    pub recorded: String,
    /// Valeur de PlannerCraft.
    pub computed: String,
    /// PlannerCraft − MS Project, en minutes (temps civil pour une date, minutes
    /// de marge tronquées vers zéro pour une marge) ; absent pour un booléen.
    pub delta_minutes: Option<i64>,
}

/// Un cas comparé.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Case {
    /// UID.
    pub uid: u32,
    /// ID (ordre d'affichage), 0 pour la récapitulative du projet.
    pub id: Option<u32>,
    /// Nature.
    pub kind: CaseKind,
    /// Champs comparés.
    pub compared: Vec<Field>,
    /// Divergences (vide si identique).
    pub divergences: Vec<Divergence>,
    /// Causes probables (calculées pour tous les cas).
    pub causes: BTreeSet<Cause>,
    /// Règles exercées par ce cas (`R-xxx`), pour le statut « vérifié ».
    pub rules: BTreeSet<&'static str>,
}

impl Case {
    /// Tous les champs comparables égaux.
    #[must_use]
    pub fn identical(&self) -> bool {
        self.divergences.is_empty()
    }

    /// Champs de base ([`BASE_FIELDS`], plus la durée pour une
    /// récapitulative ou l'UID 0) que ce cas n'a pas comparés.
    #[must_use]
    pub fn missing_fields(&self) -> Vec<Field> {
        let duration = (self.kind != CaseKind::Leaf).then_some(Field::Duration);
        BASE_FIELDS
            .into_iter()
            .chain(duration)
            .filter(|f| !self.compared.contains(f))
            .collect()
    }
}

/// Comparaison d'un fichier.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileComparison {
    /// Cas, dans l'ordre des UID.
    pub cases: Vec<Case>,
    /// Tâches exclues, comptées par raison.
    pub excluded: BTreeMap<Excluded, u32>,
}

impl FileComparison {
    /// Nombre de cas.
    #[must_use]
    pub fn case_count(&self) -> u32 {
        count(self.cases.iter())
    }

    /// Nombre de cas identiques.
    #[must_use]
    pub fn identical_count(&self) -> u32 {
        count(self.cases.iter().filter(|c| c.identical()))
    }

    /// Nombre de comparaisons par champ, sur tous les cas.
    #[must_use]
    pub fn compared_by_field(&self) -> BTreeMap<Field, u32> {
        let mut out = BTreeMap::new();
        for field in self.cases.iter().flat_map(|c| c.compared.iter()) {
            let n = out.entry(*field).or_insert(0_u32);
            *n = n.saturating_add(1);
        }
        out
    }

    /// Comparaisons incomplètes : cas auxquels manque un champ de base, plus
    /// tâches exclues faute de valeur enregistrée comparable.
    #[must_use]
    pub fn incomplete_count(&self) -> u32 {
        let missing = count(self.cases.iter().filter(|c| !c.missing_fields().is_empty()));
        let empty = self
            .excluded
            .get(&Excluded::NoComparableValue)
            .copied()
            .unwrap_or(0);
        missing.saturating_add(empty)
    }
}

fn count<T>(iter: impl Iterator<Item = T>) -> u32 {
    u32::try_from(iter.count()).unwrap_or(u32::MAX)
}

/// Structure du plan déduite des niveaux (R-052) et des liens, pour les
/// étiquettes : parent, récapitulatives, successeurs.
struct Outline {
    parent: BTreeMap<TaskUid, TaskUid>,
    summaries: BTreeSet<TaskUid>,
    has_successor: BTreeSet<TaskUid>,
}

impl Outline {
    fn new(project: &Project) -> Self {
        let mut parent = BTreeMap::new();
        let mut summaries = BTreeSet::new();
        let mut stack: Vec<(u16, TaskUid)> = Vec::new();
        for task in project.tasks.iter().filter(|t| !t.is_null) {
            let level = task.outline_level.get();
            while stack.last().is_some_and(|(l, _)| *l >= level) {
                stack.pop();
            }
            if let Some((_, p)) = stack.last() {
                parent.insert(task.uid(), *p);
                summaries.insert(*p);
            }
            stack.push((level, task.uid()));
        }
        let has_successor = project
            .tasks
            .iter()
            .flat_map(|t| t.predecessors.iter().map(|l| l.predecessor))
            .collect();
        Self {
            parent,
            summaries,
            has_successor,
        }
    }

    fn ancestors(&self, uid: TaskUid) -> impl Iterator<Item = TaskUid> + '_ {
        let mut current = uid;
        // Borne : une chaîne de parents ne peut dépasser le nombre de tâches.
        (0..=self.parent.len()).map_while(move |_| {
            let p = *self.parent.get(&current)?;
            current = p;
            Some(p)
        })
    }

    /// Récapitulative qui porte un lien (entrant ou sortant).
    fn is_linked_summary(&self, project: &Project, uid: TaskUid) -> bool {
        self.summaries.contains(&uid)
            && (self.has_successor.contains(&uid)
                || project.tasks.get(uid).is_some_and(|t| !t.predecessors.is_empty()))
    }

    /// Aucun successeur, ni propre ni hérité d'un ancêtre.
    fn is_final(&self, uid: TaskUid) -> bool {
        !self.has_successor.contains(&uid) && self.ancestors(uid).all(|a| !self.has_successor.contains(&a))
    }
}

/// Compare un document lu au planning recalculé.
#[must_use]
pub fn compare(doc: &MspdiDocument, schedule: &Schedule) -> FileComparison {
    let project = &doc.project;
    let outline = Outline::new(project);
    let mut out = FileComparison::default();
    for (&uid, recorded) in &doc.recorded.tasks {
        match case_for(project, &outline, schedule, uid, recorded) {
            Ok(case) => out.cases.push(case),
            Err(reason) => {
                let n = out.excluded.entry(reason).or_insert(0);
                *n = n.saturating_add(1);
            }
        }
    }
    label_propagation(&mut out.cases, project, &outline);
    out
}

fn case_for(
    project: &Project,
    outline: &Outline,
    schedule: &Schedule,
    uid: TaskUid,
    recorded: &RecordedTask,
) -> Result<Case, Excluded> {
    let (task, kind, ts) = if uid == TaskUid::PROJECT_SUMMARY {
        let task = project.summary_task.as_ref().ok_or(Excluded::NotInModel)?;
        let ts = schedule
            .project_summary
            .as_ref()
            .ok_or(Excluded::NoComparableValue)?;
        (task, CaseKind::ProjectSummary, ts)
    } else {
        let task = project.tasks.get(uid).ok_or(Excluded::NotInModel)?;
        if task.is_null {
            return Err(Excluded::NullRow);
        }
        if matches!(task.mode, SchedulingMode::Manual { .. }) {
            return Err(Excluded::Manual);
        }
        if !task.active {
            return Err(Excluded::Inactive);
        }
        let ts = match schedule.get(uid) {
            Some(TaskOutcome::Scheduled(ts)) => ts,
            Some(TaskOutcome::Excluded { reason, .. }) => return Err(Excluded::Engine(*reason)),
            None => return Err(Excluded::NotInModel),
        };
        let kind = match ts.role {
            TaskRole::Leaf => CaseKind::Leaf,
            TaskRole::Summary => CaseKind::Summary,
        };
        (task, kind, ts)
    };
    let (compared, divergences) = compare_fields(kind, recorded, ts);
    if compared.is_empty() {
        return Err(Excluded::NoComparableValue);
    }
    let causes = causes(project, outline, task, kind, recorded);
    let mut rules = rules(outline, task, kind, ts, &compared);
    // Calendrier du projet dérivé (R-113) : mesure sur sa base, placement
    // sur le calendrier désigné, pour une tâche sans calendrier propre.
    if causes.contains(&Cause::ProjectCalendarDerived) && task.calendar.is_none() {
        rules.insert("R-113");
    }
    let id = if kind == CaseKind::ProjectSummary {
        Some(0)
    } else {
        project.task_id(uid).map(plannercraft_core::TaskId::get)
    };
    Ok(Case {
        uid: uid.get(),
        id,
        kind,
        compared,
        divergences,
        causes,
        rules,
    })
}

fn compare_fields(kind: CaseKind, r: &RecordedTask, ts: &TaskSchedule) -> (Vec<Field>, Vec<Divergence>) {
    let mut compared = Vec::new();
    let mut divergences = Vec::new();
    let moments = [
        (Field::Start, r.start, ts.start),
        (Field::Finish, r.finish, ts.finish),
        (Field::EarlyStart, r.early_start, ts.early.start),
        (Field::EarlyFinish, r.early_finish, ts.early.finish),
        (Field::LateStart, r.late_start, ts.late.start),
        (Field::LateFinish, r.late_finish, ts.late.finish),
    ];
    for (field, rec, comp) in moments {
        if let Some(rec) = rec {
            compared.push(field);
            if rec != comp {
                divergences.push(moment_divergence(field, rec, comp));
            }
        }
    }
    let slacks = [
        (Field::TotalSlack, r.total_slack, ts.total_slack),
        (Field::FreeSlack, r.free_slack, ts.free_slack),
    ];
    for (field, rec, comp) in slacks {
        if let Some(rec) = rec {
            compared.push(field);
            let comp_tenths = comp.get().checked_mul(10);
            if comp_tenths != Some(rec.0) {
                divergences.push(Divergence {
                    field,
                    recorded: tenths_text(rec),
                    computed: format!("{} min", comp.get()),
                    delta_minutes: comp_tenths.and_then(|c| c.checked_sub(rec.0)).map(|d| d / 10),
                });
            }
        }
    }
    if let Some(rec) = r.critical {
        compared.push(Field::Critical);
        if rec != ts.critical {
            divergences.push(Divergence {
                field: Field::Critical,
                recorded: rec.to_string(),
                computed: ts.critical.to_string(),
                delta_minutes: None,
            });
        }
    }
    if kind != CaseKind::Leaf
        && let Some(rec) = r.duration
    {
        compared.push(Field::Duration);
        if rec != ts.duration {
            divergences.push(Divergence {
                field: Field::Duration,
                recorded: format!("{} min", rec.get()),
                computed: format!("{} min", ts.duration.get()),
                delta_minutes: ts.duration.get().checked_sub(rec.get()),
            });
        }
    }
    (compared, divergences)
}

fn moment_divergence(field: Field, rec: Moment, comp: Moment) -> Divergence {
    Divergence {
        field,
        recorded: rec.to_string(),
        computed: comp.to_string(),
        delta_minutes: comp.minutes().checked_sub(rec.minutes()),
    }
}

/// `4800` dixièmes → « 480 min » ; `768` → « 76.8 min ».
#[must_use]
pub fn tenths_text(t: Tenths) -> String {
    let (q, r) = (t.0 / 10, (t.0 % 10).abs());
    if r == 0 {
        format!("{q} min")
    } else if t.0 < 0 && q == 0 {
        format!("-0.{r} min")
    } else {
        format!("{q}.{r} min")
    }
}

fn causes(
    project: &Project,
    outline: &Outline,
    task: &Task,
    kind: CaseKind,
    r: &RecordedTask,
) -> BTreeSet<Cause> {
    let mut out = BTreeSet::new();
    let uid = task.uid();
    let project_calendar_derived = project
        .calendars
        .get(project.calendar)
        .is_some_and(|c| matches!(c.kind(), CalendarKind::Derived { .. }));
    if project_calendar_derived {
        out.insert(Cause::ProjectCalendarDerived);
    }
    let slack_odd = |s: Option<Tenths>| s.is_some_and(|t| t.0 % 10 != 0);
    if slack_odd(r.total_slack) || slack_odd(r.free_slack) {
        out.insert(Cause::SlackNotTenthMultiple);
    }
    if kind == CaseKind::ProjectSummary {
        return out;
    }
    if task.constraint != Constraint::AsSoonAsPossible {
        out.insert(Cause::Constraint);
    }
    if task.deadline.is_some() {
        out.insert(Cause::Deadline);
    }
    let p = &task.progress;
    if p.percent_complete.get() > 0 || p.actual_start.is_some() || p.actual_finish.is_some() {
        out.insert(Cause::Progress);
    }
    // R-198 : le retard de nivellement est lu dans le modèle.
    if task.leveling_delay.is_some() {
        out.insert(Cause::LevelingDelay);
    }
    if r.assignments > 0 {
        out.insert(Cause::Assigned);
    }
    let pred_is_summary = task
        .predecessors
        .iter()
        .any(|l| outline.summaries.contains(&l.predecessor));
    if outline.is_linked_summary(project, uid)
        || outline
            .ancestors(uid)
            .any(|a| outline.is_linked_summary(project, a))
        || pred_is_summary
    {
        out.insert(Cause::SummaryLink);
    }
    if kind == CaseKind::Leaf && task.duration.is_zero() {
        out.insert(Cause::Milestone);
    }
    let manual_pred = task.predecessors.iter().any(|l| {
        project
            .tasks
            .get(l.predecessor)
            .is_some_and(|p| matches!(p.mode, SchedulingMode::Manual { .. }))
    });
    if manual_pred {
        out.insert(Cause::ManualPredecessor);
    }
    if task.calendar.is_some_and(|c| c != project.calendar) {
        out.insert(Cause::TaskCalendar);
    }
    if task.duration.basis() == TimeBasis::Elapsed {
        out.insert(Cause::Elapsed);
    }
    let lagged = task.predecessors.iter().any(|l| match l.lag.value {
        LagValue::Time { minutes, .. } => minutes != Minutes::ZERO,
        LagValue::Percent { percent } => percent.get() != 0,
    });
    if lagged {
        out.insert(Cause::Lag);
    }
    out
}

/// Règles exercées par un cas (pour le statut « vérifié » des fiches).
fn rules(
    outline: &Outline,
    task: &Task,
    kind: CaseKind,
    ts: &TaskSchedule,
    compared: &[Field],
) -> BTreeSet<&'static str> {
    let has = |f: Field| compared.contains(&f);
    let late = has(Field::LateStart) || has(Field::LateFinish);
    let mut out = BTreeSet::new();
    if has(Field::TotalSlack) {
        out.insert("R-048");
        out.insert("R-083");
    }
    if has(Field::FreeSlack) {
        out.insert("R-049");
        out.insert("R-083");
    }
    if has(Field::Critical) {
        out.insert("R-050");
    }
    if kind == CaseKind::ProjectSummary {
        out.insert("R-068");
        out.insert("R-084");
        if has(Field::Finish) || has(Field::EarlyFinish) {
            out.insert("R-045");
        }
        return out;
    }
    let uid = task.uid();
    if outline.parent.contains_key(&uid) || kind == CaseKind::Summary {
        out.insert("R-052");
    }
    if kind == CaseKind::Summary {
        out.insert("R-053");
        if has(Field::Duration) {
            out.insert("R-054");
        }
        if late || has(Field::TotalSlack) || has(Field::Critical) {
            out.insert("R-060");
        }
        if outline.has_successor.contains(&uid) {
            out.insert("R-057");
        }
    } else {
        // Feuille automatique : placement et durée lue en ISO 8601.
        out.insert("R-070");
        if task.duration.is_zero() {
            out.insert("R-031");
            out.insert("R-061");
        } else {
            out.insert("R-030");
        }
        if late {
            out.insert(if outline.is_final(uid) { "R-046" } else { "R-047" });
        }
    }
    match ts.driver {
        Driver::ProjectStart => {
            out.insert("R-036");
        }
        Driver::SummaryGate { .. } => {
            out.insert("R-056");
        }
        Driver::Link { predecessor, link } => {
            out.insert("R-074");
            out.insert(match link {
                LinkType::FinishToStart => "R-037",
                LinkType::StartToStart => "R-038",
                LinkType::FinishToFinish => "R-039",
                LinkType::StartToFinish => "R-040",
            });
            if outline.summaries.contains(&predecessor) {
                out.insert("R-057");
            }
        }
        Driver::Constraint { kind } => {
            // Contrainte qui pilote le début (ADR-0010) : normalisation de
            // la date (R-090) et règle du type.
            out.insert("R-090");
            out.insert(match kind {
                ConstraintKind::AsSoonAsPossible => "R-091",
                ConstraintKind::AsLateAsPossible => "R-092",
                ConstraintKind::StartNoEarlierThan => "R-093",
                ConstraintKind::FinishNoEarlierThan => "R-094",
                ConstraintKind::StartNoLaterThan => "R-095",
                ConstraintKind::FinishNoLaterThan => "R-096",
                ConstraintKind::MustStartOn => "R-097",
                ConstraintKind::MustFinishOn => "R-098",
            });
        }
        // Début réel (J6) : étiqueté par les lots du moteur (R-203, R-204).
        Driver::Manual | Driver::Rollup | Driver::Actual => {}
    }
    if task.predecessors.len() >= 2 {
        out.insert("R-044");
    }
    out
}

/// Étiquettes qui dépendent des autres cas : prédécesseur ou ancêtre
/// divergent, sous-tâche divergente.
fn label_propagation(cases: &mut [Case], project: &Project, outline: &Outline) {
    let divergent: BTreeSet<TaskUid> = cases
        .iter()
        .filter(|c| !c.identical())
        .map(|c| TaskUid::new(c.uid))
        .collect();
    let mut divergent_parents: BTreeSet<TaskUid> = BTreeSet::new();
    for uid in &divergent {
        if *uid == TaskUid::PROJECT_SUMMARY {
            continue;
        }
        divergent_parents.extend(outline.ancestors(*uid));
        divergent_parents.insert(TaskUid::PROJECT_SUMMARY);
    }
    for case in cases.iter_mut().filter(|c| !c.identical()) {
        let uid = TaskUid::new(case.uid);
        if divergent_parents.contains(&uid) {
            case.causes.insert(Cause::DivergentChild);
        }
        if uid == TaskUid::PROJECT_SUMMARY {
            continue;
        }
        let pred_divergent = project
            .tasks
            .get(uid)
            .is_some_and(|t| t.predecessors.iter().any(|l| divergent.contains(&l.predecessor)));
        if pred_divergent || outline.ancestors(uid).any(|a| divergent.contains(&a)) {
            case.causes.insert(Cause::DivergentPredecessor);
        }
    }
}
