//! Modifications directes d'une tâche, pour le tableau avant/après du
//! panneau d'opérations IA (lot R1b, H-UI-189, ADR-0028 D2).
//!
//! La comparaison lit les pré- et post-images du delta par entité (R-164),
//! déjà calculé pour le lot : aucune seconde comparaison des projets. Elle
//! parcourt [`TaskFields`] par déstructuration **exhaustive** (sans `..`) :
//! un champ ajouté à `Task` ne compile pas tant qu'il n'a pas reçu une nature
//! ([`FieldKind`]) ou un rattachement motivé à [`FieldKind::Other`].
//!
//! Les valeurs sont mises en texte par les formateurs existants du moteur
//! (`crate::text` : durées, % et listes dans la forme des colonnes du
//! tableau) ; les dates et les valeurs à choix (oui ou non, mode, type de
//! contrainte) restent typées et l'interface les écrit avec les libellés de
//! ses colonnes. Les libellés des champs se choisissent dans l'interface à
//! partir de [`FieldKind`] (couches : ni egui ni locales ici).

use std::collections::BTreeSet;

use plannercraft_core::{
    Assignment, ConstraintKind, CustomFieldId, CustomKind, CustomValue, Decimal, DurationUnit, Moment,
    Progress, Project, SchedulingMode, Task, TaskFields, TaskUid, TimeBasis, Work,
};

use crate::render_text::Labels;
use crate::text::{
    DisplayLabels, display_duration, display_minutes_as, display_percent, display_predecessors,
    format_resource_names, write_decimal,
};

/// Nature d'une modification directe, dans l'ordre d'affichage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldKind {
    /// Position dans la liste (ID d'avant → ID d'après), pour une tâche
    /// **déplacée** au sens de R-163 (amendée le 2026-11-11, ordre).
    Position,
    /// Niveau hiérarchique.
    OutlineLevel,
    /// Nom.
    Name,
    /// Durée saisie.
    Duration,
    /// % achevé saisi.
    PercentComplete,
    /// % travail achevé saisi.
    PercentWorkComplete,
    /// % physique achevé saisi.
    PhysicalPercentComplete,
    /// Début réel saisi.
    ActualStart,
    /// Fin réelle saisie.
    ActualFinish,
    /// Durée réelle saisie.
    ActualDuration,
    /// Durée restante saisie.
    RemainingDuration,
    /// Travail réel saisi.
    ActualWork,
    /// Travail restant saisi.
    RemainingWork,
    /// Mode de planification (automatique ou manuel).
    Mode,
    /// Début saisi d'une tâche manuelle.
    ManualStart,
    /// Fin saisie d'une tâche manuelle.
    ManualFinish,
    /// Durée saisie d'une tâche manuelle.
    ManualDuration,
    /// Type de contrainte.
    ConstraintType,
    /// Date de contrainte.
    ConstraintDate,
    /// Échéance.
    Deadline,
    /// Calendrier de la tâche.
    Calendar,
    /// Prédécesseurs (forme de la colonne Prédécesseurs).
    Predecessors,
    /// Ressources affectées (forme de la colonne « Noms ressources »).
    Resources,
    /// Priorité.
    Priority,
    /// Jalon.
    Milestone,
    /// Active ou non.
    Active,
    /// Notes (sans leur contenu).
    Notes,
    /// Champ personnalisé (nommé par l'interface : alias ou « Texte1 »).
    Custom(CustomFieldId),
    /// Autres données enregistrées (liste motivée dans [`task_changes`]).
    Other,
}

/// Valeur d'avant ou d'après d'une modification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldValue {
    /// Aucune valeur (échéance absente, notes et autres données : la
    /// modification est dite sans valeur).
    None,
    /// Texte mis en forme par le moteur.
    Text(String),
    /// Oui ou non (jalon, active, indicateur personnalisé).
    YesNo(bool),
    /// Mode de planification : vrai si manuel.
    Manual(bool),
    /// Date (début et fin réels, dates saisies, contrainte, échéance, champ
    /// personnalisé de type date), écrite par l'interface avec le formateur
    /// de ses colonnes.
    Date(Moment),
    /// Type de contrainte.
    Constraint(ConstraintKind),
}

/// Modification directe d'un champ d'une tâche (H-UI-189).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldChange {
    /// Nature du champ.
    pub field: FieldKind,
    /// Valeur d'avant.
    pub before: FieldValue,
    /// Valeur d'après.
    pub after: FieldValue,
}

/// Un côté de la comparaison : projet (pour les ID, noms des calendriers et
/// des ressources, facteurs de conversion).
#[derive(Clone, Copy)]
pub(crate) struct Side<'a> {
    pub project: &'a Project,
    pub assignments: &'a [&'a Assignment],
}

/// Date typée (écrite par l'interface au format de ses colonnes).
fn date(m: Option<Moment>) -> FieldValue {
    m.map_or(FieldValue::None, FieldValue::Date)
}

/// Contexte d'écriture des valeurs.
pub(crate) struct Writer {
    pub labels: Labels,
}

impl Writer {
    fn work(&self, w: Work, p: &Project) -> FieldValue {
        FieldValue::Text(display_minutes_as(
            w.minutes(),
            DurationUnit::Hour,
            TimeBasis::Working,
            &p.options.factors,
            &self.labels,
        ))
    }

    fn custom(&self, id: CustomFieldId, v: Option<&CustomValue>) -> FieldValue {
        match v {
            None if id.kind() == CustomKind::Flag => FieldValue::YesNo(false),
            None => FieldValue::None,
            Some(CustomValue::Text(s)) => FieldValue::Text(s.clone()),
            Some(CustomValue::Number(n)) => match Decimal::new(*n, 4) {
                Ok(d) => {
                    let mut out = String::new();
                    write_decimal(&mut out, d, self.labels.decimal_sep());
                    FieldValue::Text(out)
                }
                Err(_) => FieldValue::Text(self.labels.unknown().to_owned()),
            },
            Some(CustomValue::Date(m)) => date(Some(*m)),
            Some(CustomValue::Flag(b)) => FieldValue::YesNo(*b),
        }
    }
}

/// Champs saisis d'une tâche manuelle (début, fin, durée).
fn manual_parts(
    mode: &SchedulingMode,
) -> (
    bool,
    Option<Moment>,
    Option<Moment>,
    Option<plannercraft_core::Duration>,
) {
    match mode {
        SchedulingMode::Auto => (false, None, None, None),
        SchedulingMode::Manual {
            start,
            finish,
            duration,
        } => (true, *start, *finish, *duration),
    }
}

/// Modifications directes de la tâche (avant `prev`, après `next`), dans l'ordre
/// de [`FieldKind`] ; `moved` : (ID d'avant, ID d'après) si la tâche est
/// déplacée (R-163) ; `assignments_changed` : une affectation de la tâche a
/// changé (les listes de ressources des deux côtés sont alors comparées).
///
/// Rattachés à « Autres données » ([`FieldKind::Other`]), un seul élément
/// par tâche : GUID, ligne vide, WBS (données d'échange MSPDI, non saisies
/// dans l'interface) ; travail, type de tâche, pilotage par l'effort (le plus
/// souvent conséquences d'une durée ou d'une affectation, J5) ; ignorer le
/// calendrier des ressources, récurrente, date de création, retard de
/// nivellement (lus d'un fichier, sans commande de saisie courante) ;
/// lignes de base (posées par `baseline.set`, un tout) ; reprise saisie
/// (`progress.reschedule`) ; affectation changée sans que la liste des
/// ressources change (travail, retard, profil). Les `Extras` (données hors
/// modèle, souvent jetées par R-161 à la suite d'une autre modification)
/// ne comptent que si rien d'autre n'a changé : la ligne n'est jamais vide
/// pour une tâche du delta.
#[allow(
    clippy::too_many_lines,
    reason = "un bloc par champ de Task, dans l'ordre d'affichage : lu comme une table"
)]
pub(crate) fn task_changes(
    prev: &Task,
    next: &Task,
    sides: (Side<'_>, Side<'_>),
    moved: Option<(u32, u32)>,
    assignments_changed: bool,
    writer: &Writer,
) -> Vec<FieldChange> {
    let (bs, as_) = sides;
    let TaskFields {
        uid: _,
        guid,
        name,
        is_null,
        outline_level,
        wbs,
        duration,
        work,
        task_type,
        effort_driven,
        milestone,
        constraint,
        deadline,
        calendar,
        ignore_resource_calendar,
        priority,
        leveling_delay,
        progress,
        predecessors,
        notes,
        mode,
        active,
        recurring,
        created,
        baselines,
        custom,
        extras,
    } = prev.fields();
    let after = next.fields();
    let mut out: Vec<FieldChange> = Vec::new();
    let mut push = |field: FieldKind, before: FieldValue, after: FieldValue| {
        out.push(FieldChange { field, before, after });
    };
    let text = |s: String| FieldValue::Text(s);
    let (fb, fa) = (&bs.project.options.factors, &as_.project.options.factors);
    let l = &writer.labels;

    if let Some((id_b, id_a)) = moved {
        push(
            FieldKind::Position,
            text(id_b.to_string()),
            text(id_a.to_string()),
        );
    }
    if outline_level != after.outline_level {
        push(
            FieldKind::OutlineLevel,
            text(outline_level.get().to_string()),
            text(after.outline_level.get().to_string()),
        );
    }
    if name != after.name {
        push(
            FieldKind::Name,
            text(name.to_owned()),
            text(after.name.to_owned()),
        );
    }
    if duration != after.duration {
        push(
            FieldKind::Duration,
            text(display_duration(duration, fb, l)),
            text(display_duration(after.duration, fa, l)),
        );
    }
    let Progress {
        percent_complete,
        percent_work_complete,
        physical_percent_complete,
        actual_start,
        actual_finish,
        actual_duration,
        remaining_duration,
        actual_work,
        remaining_work,
        resume,
    } = *progress;
    let y = after.progress;
    for (field, p0, p1) in [
        (FieldKind::PercentComplete, percent_complete, y.percent_complete),
        (
            FieldKind::PercentWorkComplete,
            percent_work_complete,
            y.percent_work_complete,
        ),
        (
            FieldKind::PhysicalPercentComplete,
            physical_percent_complete,
            y.physical_percent_complete,
        ),
    ] {
        if p0 != p1 {
            push(field, text(display_percent(p0, l)), text(display_percent(p1, l)));
        }
    }
    if actual_start != y.actual_start {
        push(FieldKind::ActualStart, date(actual_start), date(y.actual_start));
    }
    if actual_finish != y.actual_finish {
        push(
            FieldKind::ActualFinish,
            date(actual_finish),
            date(y.actual_finish),
        );
    }
    // Durées réelle et restante dans l'unité de la tâche, comme leurs
    // colonnes.
    let in_unit = |m: plannercraft_core::Minutes,
                   d: &plannercraft_core::Duration,
                   f: &plannercraft_core::ConversionFactors| {
        text(display_minutes_as(m, d.unit(), d.basis(), f, l))
    };
    if actual_duration != y.actual_duration {
        push(
            FieldKind::ActualDuration,
            in_unit(actual_duration, duration, fb),
            in_unit(y.actual_duration, after.duration, fa),
        );
    }
    if remaining_duration != y.remaining_duration {
        push(
            FieldKind::RemainingDuration,
            in_unit(remaining_duration, duration, fb),
            in_unit(y.remaining_duration, after.duration, fa),
        );
    }
    if actual_work != y.actual_work {
        push(
            FieldKind::ActualWork,
            writer.work(actual_work, bs.project),
            writer.work(y.actual_work, as_.project),
        );
    }
    if remaining_work != y.remaining_work {
        push(
            FieldKind::RemainingWork,
            writer.work(remaining_work, bs.project),
            writer.work(y.remaining_work, as_.project),
        );
    }
    let (m0, s0, f0, d0) = manual_parts(mode);
    let (m1, s1, f1, d1) = manual_parts(after.mode);
    if m0 != m1 {
        push(FieldKind::Mode, FieldValue::Manual(m0), FieldValue::Manual(m1));
    }
    if s0 != s1 {
        push(FieldKind::ManualStart, date(s0), date(s1));
    }
    if f0 != f1 {
        push(FieldKind::ManualFinish, date(f0), date(f1));
    }
    if d0 != d1 {
        let dur = |d: Option<plannercraft_core::Duration>, f: &plannercraft_core::ConversionFactors| {
            d.map_or(FieldValue::None, |d| text(display_duration(&d, f, l)))
        };
        push(FieldKind::ManualDuration, dur(d0, fb), dur(d1, fa));
    }
    if constraint.kind() != after.constraint.kind() {
        push(
            FieldKind::ConstraintType,
            FieldValue::Constraint(constraint.kind()),
            FieldValue::Constraint(after.constraint.kind()),
        );
    }
    if constraint.date() != after.constraint.date() {
        push(
            FieldKind::ConstraintDate,
            date(constraint.date()),
            date(after.constraint.date()),
        );
    }
    if deadline != after.deadline {
        push(FieldKind::Deadline, date(deadline), date(after.deadline));
    }
    if calendar != after.calendar {
        let cal = |p: &Project, c: Option<plannercraft_core::CalendarUid>| {
            c.map_or(FieldValue::None, |c| {
                text(
                    p.calendars
                        .get(c)
                        .map_or_else(|| l.unknown().to_owned(), |c| c.name.clone()),
                )
            })
        };
        push(
            FieldKind::Calendar,
            cal(bs.project, calendar),
            cal(as_.project, after.calendar),
        );
    }
    if predecessors != after.predecessors {
        push(
            FieldKind::Predecessors,
            text(display_predecessors(predecessors, bs.project, fb, l)),
            text(display_predecessors(after.predecessors, as_.project, fa, l)),
        );
    }
    let mut other = false;
    if assignments_changed {
        let names = |s: Side<'_>| format_resource_names(s.assignments.iter().copied(), s.project, l);
        let (n0, n1) = (names(bs), names(as_));
        if n0 == n1 {
            // Affectation changée sans que la liste change (travail,
            // retard, profil) : autres données.
            other = true;
        } else {
            push(FieldKind::Resources, text(n0), text(n1));
        }
    }
    if priority != after.priority {
        push(
            FieldKind::Priority,
            text(priority.get().to_string()),
            text(after.priority.get().to_string()),
        );
    }
    if milestone != after.milestone {
        push(
            FieldKind::Milestone,
            FieldValue::YesNo(milestone),
            FieldValue::YesNo(after.milestone),
        );
    }
    if active != after.active {
        push(
            FieldKind::Active,
            FieldValue::YesNo(active),
            FieldValue::YesNo(after.active),
        );
    }
    if notes != after.notes {
        push(FieldKind::Notes, FieldValue::None, FieldValue::None);
    }
    // Champs personnalisés : réunion des clés des deux côtés (BTreeMap :
    // ordre des identifiants).
    let ids: BTreeSet<CustomFieldId> = custom.keys().chain(after.custom.keys()).copied().collect();
    for id in ids {
        let (v0, v1) = (custom.get(&id), after.custom.get(&id));
        if v0 != v1 {
            push(
                FieldKind::Custom(id),
                writer.custom(id, v0),
                writer.custom(id, v1),
            );
        }
    }
    other = other
        || guid != after.guid
        || is_null != after.is_null
        || wbs != after.wbs
        || work != after.work
        || task_type != after.task_type
        || effort_driven != after.effort_driven
        || ignore_resource_calendar != after.ignore_resource_calendar
        || leveling_delay != after.leveling_delay
        || recurring != after.recurring
        || created != after.created
        || baselines != after.baselines
        || resume != y.resume;
    let extras_only = extras != after.extras && out.is_empty();
    if other || extras_only {
        out.push(FieldChange {
            field: FieldKind::Other,
            before: FieldValue::None,
            after: FieldValue::None,
        });
    }
    out
}

/// Tâches visées par les affectations changées du lot (côtés avant et
/// après), triées par UID.
pub(crate) fn assignment_tasks<'a>(
    changed: impl IntoIterator<
        Item = &'a (
            plannercraft_core::AssignmentUid,
            Option<Assignment>,
            Option<Assignment>,
        ),
    >,
) -> BTreeSet<TaskUid> {
    let mut out = BTreeSet::new();
    for (_, b, a) in changed {
        for x in [b.as_ref(), a.as_ref()].into_iter().flatten() {
            out.insert(x.task);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net et lisiblement"
    )]

    use plannercraft_core::{
        BaselineNumber, Constraint, CustomFieldId, CustomValue, Duration, DurationUnit, ExtraEntry, Guid,
        LevelingDelay, Link, Minutes, Moment, OutlineLevel, Percent, Priority, Project, ProjectInit,
        ScheduleAnchor, SchedulingMode, Task, TaskBaseline, TaskType, TaskUid, TimeBasis, Work,
    };

    use super::{FieldKind, FieldValue, Side, Writer, task_changes};
    use crate::render_text::Labels;
    use crate::text::Language;

    fn at(s: &str) -> Moment {
        s.parse().unwrap()
    }

    fn project() -> Project {
        let start = at("2026-11-02T08:00");
        let mut p = Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(start),
            current_date: start,
            calendar_names: plannercraft_core::BuiltinCalendarNames::default(),
        });
        for u in 1..=2 {
            let t = Task::new(TaskUid::new(u), format!("T{u}"), &p.options);
            p.tasks.push(t).unwrap();
        }
        p
    }

    fn changes(mutate: &dyn Fn(&mut Task, &Project)) -> Vec<(FieldKind, FieldValue, FieldValue)> {
        let before = project();
        let mut after = before.clone();
        let snapshot = after.clone();
        mutate(after.tasks.get_mut(TaskUid::new(2)).unwrap(), &snapshot);
        let b = before.tasks.get(TaskUid::new(2)).unwrap();
        let a = after.tasks.get(TaskUid::new(2)).unwrap();
        assert_ne!(b, a, "mutation sans effet");
        let w = Writer {
            labels: Labels(Language::Fr),
        };
        let sides = (
            Side {
                project: &before,
                assignments: &[],
            },
            Side {
                project: &after,
                assignments: &[],
            },
        );
        task_changes(b, a, sides, None, false, &w)
            .into_iter()
            .map(|c| (c.field, c.before, c.after))
            .collect()
    }

    fn t(s: &str) -> FieldValue {
        FieldValue::Text(s.to_owned())
    }

    /// H-UI-189 : chaque champ enregistré de `Task` produit au moins une
    /// modification, de la nature attendue (ou « Autres données »).
    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "une ligne par champ de Task, lue comme une table"
    )]
    fn h_ui_189_every_task_field_has_a_kind() {
        type Mutation = Box<dyn Fn(&mut Task, &Project)>;
        let m = at("2026-11-04T08:00");
        let cases: Vec<(FieldKind, Mutation)> = vec![
            (
                FieldKind::OutlineLevel,
                Box::new(|t, _| t.outline_level = OutlineLevel::new(2).unwrap()),
            ),
            (FieldKind::Name, Box::new(|t, _| t.name = "Autre".to_owned())),
            (
                FieldKind::Duration,
                Box::new(|t, _| {
                    t.duration =
                        Duration::new(Minutes::new(960), DurationUnit::Day, TimeBasis::Working, false)
                            .unwrap();
                }),
            ),
            (
                FieldKind::PercentComplete,
                Box::new(|t, _| t.progress.percent_complete = Percent::FULL),
            ),
            (
                FieldKind::PercentWorkComplete,
                Box::new(|t, _| t.progress.percent_work_complete = Percent::new(5_000).unwrap()),
            ),
            (
                FieldKind::PhysicalPercentComplete,
                Box::new(|t, _| t.progress.physical_percent_complete = Percent::new(5_000).unwrap()),
            ),
            (
                FieldKind::ActualStart,
                Box::new(move |t, _| t.progress.actual_start = Some(m)),
            ),
            (
                FieldKind::ActualFinish,
                Box::new(move |t, _| t.progress.actual_finish = Some(m)),
            ),
            (
                FieldKind::ActualDuration,
                Box::new(|t, _| t.progress.actual_duration = Minutes::new(240)),
            ),
            (
                FieldKind::RemainingDuration,
                Box::new(|t, _| t.progress.remaining_duration = Minutes::new(240)),
            ),
            (
                FieldKind::ActualWork,
                Box::new(|t, _| t.progress.actual_work = Work::new(Minutes::new(240)).unwrap()),
            ),
            (
                FieldKind::RemainingWork,
                Box::new(|t, _| t.progress.remaining_work = Work::new(Minutes::new(240)).unwrap()),
            ),
            (
                FieldKind::Other,
                Box::new(move |t, _| t.progress.resume = Some(m)),
            ),
            (
                FieldKind::Mode,
                Box::new(|t, _| {
                    t.mode = SchedulingMode::Manual {
                        start: None,
                        finish: None,
                        duration: None,
                    };
                }),
            ),
            (
                FieldKind::ManualStart,
                Box::new(move |t, _| {
                    t.mode = SchedulingMode::Manual {
                        start: Some(m),
                        finish: None,
                        duration: None,
                    };
                }),
            ),
            (
                FieldKind::ManualFinish,
                Box::new(move |t, _| {
                    t.mode = SchedulingMode::Manual {
                        start: None,
                        finish: Some(m),
                        duration: None,
                    };
                }),
            ),
            (
                FieldKind::ManualDuration,
                Box::new(|t, _| {
                    t.mode = SchedulingMode::Manual {
                        start: None,
                        finish: None,
                        duration: Some(t.duration),
                    };
                }),
            ),
            (
                FieldKind::ConstraintType,
                Box::new(|t, _| t.constraint = Constraint::AsLateAsPossible),
            ),
            (
                FieldKind::ConstraintDate,
                Box::new(move |t, _| t.constraint = Constraint::MustStartOn { date: m }),
            ),
            (FieldKind::Deadline, Box::new(move |t, _| t.deadline = Some(m))),
            (
                FieldKind::Calendar,
                Box::new(|t, p| t.calendar = Some(p.calendar)),
            ),
            (
                FieldKind::Predecessors,
                Box::new(|t, _| t.predecessors.push(Link::finish_to_start(TaskUid::new(1)))),
            ),
            (
                FieldKind::Priority,
                Box::new(|t, _| t.priority = Priority::new(700).unwrap()),
            ),
            (FieldKind::Milestone, Box::new(|t, _| t.milestone = true)),
            (FieldKind::Active, Box::new(|t, _| t.active = false)),
            (FieldKind::Notes, Box::new(|t, _| t.notes = "n".to_owned())),
            (
                FieldKind::Custom(CustomFieldId::TEXT1),
                Box::new(|t, _| {
                    t.custom
                        .insert(CustomFieldId::TEXT1, CustomValue::Text("Lot A".to_owned()));
                }),
            ),
            (
                FieldKind::Other,
                Box::new(|t, _| t.guid = Some(Guid::from_bytes([7; 16]))),
            ),
            (FieldKind::Other, Box::new(|t, _| t.is_null = true)),
            (FieldKind::Other, Box::new(|t, _| t.wbs = Some("1.2".to_owned()))),
            (
                FieldKind::Other,
                Box::new(|t, _| t.work = Work::new(Minutes::new(480)).unwrap()),
            ),
            (
                FieldKind::Other,
                Box::new(|t, _| t.task_type = TaskType::FixedWork),
            ),
            (
                FieldKind::Other,
                Box::new(|t, _| t.effort_driven = !t.effort_driven),
            ),
            (
                FieldKind::Other,
                Box::new(|t, _| t.ignore_resource_calendar = true),
            ),
            (
                FieldKind::Other,
                Box::new(|t, _| {
                    t.leveling_delay = Some(LevelingDelay {
                        minutes: Minutes::new(480),
                        unit: DurationUnit::Day,
                        elapsed: false,
                    });
                }),
            ),
            (FieldKind::Other, Box::new(|t, _| t.recurring = true)),
            (FieldKind::Other, Box::new(move |t, _| t.created = Some(m))),
            (
                FieldKind::Other,
                Box::new(move |t, _| {
                    t.baselines.insert(
                        BaselineNumber::PRIMARY,
                        TaskBaseline {
                            start: Some(m),
                            ..TaskBaseline::default()
                        },
                    );
                }),
            ),
            (
                FieldKind::Other,
                Box::new(|t, _| {
                    t.extras.0.push(ExtraEntry {
                        origin: "mspdi".to_owned(),
                        key: "k".to_owned(),
                        payload: "v".to_owned(),
                    });
                }),
            ),
        ];
        for (kind, mutate) in &cases {
            let got = changes(mutate.as_ref());
            assert!(
                got.iter().any(|c| c.0 == *kind),
                "{kind:?} attendu, obtenu {got:?}"
            );
        }
        // Valeurs mises en forme par le moteur (forme des colonnes).
        let got = changes(&|t, _| t.progress.percent_complete = Percent::FULL);
        assert_eq!(
            got,
            vec![(FieldKind::PercentComplete, t("0\u{a0}%"), t("100\u{a0}%"))]
        );
        let got = changes(&|t, _| t.notes = "secret".to_owned());
        assert_eq!(got, vec![(FieldKind::Notes, FieldValue::None, FieldValue::None)]);
        let got = changes(&move |t, _| t.deadline = Some(m));
        assert_eq!(
            got,
            vec![(FieldKind::Deadline, FieldValue::None, FieldValue::Date(m))]
        );
        let got = changes(&|t, _| t.milestone = true);
        assert_eq!(
            got,
            vec![(
                FieldKind::Milestone,
                FieldValue::YesNo(false),
                FieldValue::YesNo(true)
            )]
        );
        // Données hors modèle jetées avec une autre modification : rien de
        // plus que cette modification.
        let got = changes(&|t, _| {
            t.name = "X".to_owned();
            t.extras.0.push(ExtraEntry {
                origin: "mspdi".to_owned(),
                key: "k".to_owned(),
                payload: "v".to_owned(),
            });
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, FieldKind::Name);
    }
}
