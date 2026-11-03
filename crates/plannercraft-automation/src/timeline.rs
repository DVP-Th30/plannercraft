//! Données de la chronologie (ADR-0017 D7, H-R16) : extrémités, date
//! d'état, graduations et tâches de la frise, avec leurs textes, tirés du
//! planning du moteur (règle 3 : aucune date calculée ici ni dans
//! l'interface). La géométrie est celle de `plannercraft_render::timeline`.
//!
//! Formes (hypothèses, `docs/features/render.md` H-R16) : extrémités
//! « Début lun 19/10/26 » / « Start Mon 10/19/26 » (dates courtes de
//! H-UI-63, jour de la semaine) ; date d'état « État 06/11 » / « Status
//! 11/06 » ; jalons « 06/11 » ; graduations au premier jour des mois
//! (« oct. », « janv. 27 » / « Oct », « Jan '27 »), des trimestres au-delà
//! de 24 mois (« T1 2027 » / « Q1 2027 »), des années au-delà de 8 ans
//! (« 2027 »), choisies sur la durée du projet en jours entiers.

use chrono::{Datelike, NaiveDate};
use plannercraft_core::{Day, Moment, TaskUid, TimeOfDay, Weekday};
use plannercraft_render::geometry::row_geom_input;
use plannercraft_render::style::{BarRole, MAX_TIMELINE_TASKS, PlanStyle, TimelineSpec};
use plannercraft_render::timeline::{TimelineInput, TimelineItem, TimelineKind};

use crate::Session;
use crate::text::{Language, TextContext, format_date};

/// Graduations au plus (années de 1984 à 2149 : 166).
pub const MAX_TICKS: usize = 400;

/// Durée de projet (jours entiers) au-delà de laquelle les graduations
/// passent aux trimestres (24 mois).
pub const QUARTERS_AFTER_DAYS: i32 = 731;

/// Durée de projet (jours entiers) au-delà de laquelle les graduations
/// passent aux années (8 ans).
pub const YEARS_AFTER_DAYS: i32 = 2_922;

/// Unité des graduations de la frise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickUnit {
    /// Premier jour de chaque mois.
    Month,
    /// Premier jour de chaque trimestre civil.
    Quarter,
    /// 1er janvier.
    Year,
}

/// Unité des graduations pour un projet de `days` jours entiers (H-R16) :
/// mois jusqu'à 24 mois (731 jours), trimestres jusqu'à 8 ans (2 922
/// jours), années au-delà.
#[must_use]
pub const fn tick_unit(days: i32) -> TickUnit {
    if days > YEARS_AFTER_DAYS {
        TickUnit::Year
    } else if days > QUARTERS_AFTER_DAYS {
        TickUnit::Quarter
    } else {
        TickUnit::Month
    }
}

/// Noms courts des mois (mêmes formes que l'en-tête du Gantt, H-R5).
const MONTHS_FR: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc.",
];
const MONTHS_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Jour de la semaine abrégé.
const fn weekday_short(lang: Language, d: Weekday) -> &'static str {
    match (lang, d) {
        (Language::Fr, Weekday::Monday) => "lun",
        (Language::Fr, Weekday::Tuesday) => "mar",
        (Language::Fr, Weekday::Wednesday) => "mer",
        (Language::Fr, Weekday::Thursday) => "jeu",
        (Language::Fr, Weekday::Friday) => "ven",
        (Language::Fr, Weekday::Saturday) => "sam",
        (Language::Fr, Weekday::Sunday) => "dim",
        (Language::En, Weekday::Monday) => "Mon",
        (Language::En, Weekday::Tuesday) => "Tue",
        (Language::En, Weekday::Wednesday) => "Wed",
        (Language::En, Weekday::Thursday) => "Thu",
        (Language::En, Weekday::Friday) => "Fri",
        (Language::En, Weekday::Saturday) => "Sat",
        (Language::En, Weekday::Sunday) => "Sun",
    }
}

/// Date sans heure (`JJ/MM/AAAA`, `MM/DD/YYYY`), R-143.
fn date_part(m: Moment, ctx: &TextContext) -> String {
    let full = format_date(m, ctx);
    full.split_once(' ').map_or(full.clone(), |(d, _)| d.to_owned())
}

/// Date courte `JJ/MM/AA` (H-UI-63).
fn date_short(m: Moment, ctx: &TextContext) -> String {
    let day = date_part(m, ctx);
    match day.rsplit_once('/') {
        Some((head, year)) if year.len() == 4 => format!("{head}/{}", year.get(2..).unwrap_or(year)),
        _ => day,
    }
}

/// Jour et mois `JJ/MM` (`MM/DD` en anglais).
fn day_month(m: Moment, ctx: &TextContext) -> String {
    let day = date_part(m, ctx);
    day.rsplit_once('/')
        .map_or(day.clone(), |(head, _)| head.to_owned())
}

/// Étiquette d'extrémité : mot, jour de la semaine, date courte.
fn bound_label(word: &str, m: Moment, ctx: &TextContext, lang: Language) -> String {
    format!(
        "{word} {} {}",
        weekday_short(lang, m.weekday()),
        date_short(m, ctx)
    )
}

/// Texte d'une graduation au premier jour de `d`.
fn tick_label(d: NaiveDate, unit: TickUnit, lang: Language) -> String {
    let month = usize::try_from(d.month0()).unwrap_or(0);
    let year = d.year();
    let yy = year.rem_euclid(100);
    match unit {
        TickUnit::Year => year.to_string(),
        TickUnit::Quarter => {
            let q = (d.month0() / 3).saturating_add(1);
            match lang {
                Language::Fr => format!("T{q} {year}"),
                Language::En => format!("Q{q} {year}"),
            }
        }
        TickUnit::Month => {
            let name = match lang {
                Language::Fr => MONTHS_FR.get(month),
                Language::En => MONTHS_EN.get(month),
            }
            .copied()
            .unwrap_or("");
            match (month, lang) {
                (0, Language::Fr) => format!("{name} {yy:02}"),
                (0, Language::En) => format!("{name} '{yy:02}"),
                _ => name.to_owned(),
            }
        }
    }
}

/// Premier jour de la période suivant (ou contenant, s'il en est le
/// premier) `d`.
fn first_on_or_after(d: NaiveDate, unit: TickUnit) -> Option<NaiveDate> {
    let step: u32 = match unit {
        TickUnit::Month => 1,
        TickUnit::Quarter => 3,
        TickUnit::Year => 12,
    };
    let month0 = d.month0().checked_div(step).unwrap_or(0).saturating_mul(step);
    let first = NaiveDate::from_ymd_opt(d.year(), month0.checked_add(1)?, 1)?;
    if first >= d {
        Some(first)
    } else {
        next_period(first, unit)
    }
}

/// Premier jour de la période suivante.
fn next_period(d: NaiveDate, unit: TickUnit) -> Option<NaiveDate> {
    let months = match unit {
        TickUnit::Month => 1,
        TickUnit::Quarter => 3,
        TickUnit::Year => 12,
    };
    d.checked_add_months(chrono::Months::new(months))
}

/// Graduations de `start` à `finish` (bornes comprises), au plus
/// [`MAX_TICKS`] : instants à 00:00 du premier jour de chaque période du
/// calendrier civil, textes dans la langue.
#[must_use]
pub fn ticks(start: Moment, finish: Moment, lang: Language) -> Vec<(Moment, String)> {
    let days = finish.day().index().saturating_sub(start.day().index());
    let unit = tick_unit(days);
    let mut out = Vec::new();
    let Ok(midnight) = TimeOfDay::new(0) else {
        return out;
    };
    let mut cur = first_on_or_after(start.day().to_naive(), unit);
    while let Some(d) = cur {
        if out.len() >= MAX_TICKS {
            break;
        }
        let Some(m) = Day::from_naive(d)
            .ok()
            .and_then(|day| Moment::at(day, midnight).ok())
        else {
            break;
        };
        if m > finish {
            break;
        }
        if m >= start {
            out.push((m, tick_label(d, unit, lang)));
        }
        cur = next_period(d, unit);
    }
    out
}

/// Données de la frise de la session `s` pour la liste `spec`, textes en
/// `lang` : extrémités et date d'état du planning, graduations, tâches de
/// `spec` présentes dans le projet (au plus [`MAX_TIMELINE_TASKS`], UID en
/// double ignorés), avec leurs dates planifiées (placement d'affichage
/// R-062 pour une tâche inactive). Un UID absent du projet, une ligne vide
/// ou une tâche sans placement est ignoré.
#[must_use]
pub fn input(s: &Session, spec: &TimelineSpec, lang: Language) -> TimelineInput {
    let project = s.project();
    let schedule = s.schedule();
    let ctx = TextContext::for_project(project, lang);
    let (start_word, finish_word, status_word) = match lang {
        Language::Fr => ("Début", "Fin", "État"),
        Language::En => ("Start", "Finish", "Status"),
    };
    let start = schedule.project_start;
    let finish = schedule.project_finish;
    let status = project.effective_status_date();
    let mut uids: Vec<u32> = spec.tasks.clone();
    uids.sort_unstable();
    uids.dedup();
    let items = uids
        .into_iter()
        .filter_map(|raw| {
            let uid = TaskUid::new(raw);
            let task = project.tasks.get(uid)?;
            let geom = row_geom_input(task, schedule.tasks.get(&uid))?;
            let (task_start, task_finish) = (geom.start?, geom.finish?);
            let milestone = match geom.role {
                BarRole::Milestone | BarRole::MilestoneCritical => true,
                BarRole::Inactive => task.milestone || task.duration.is_zero(),
                _ => false,
            };
            Some(TimelineItem {
                uid,
                id: project.task_id(uid).map_or(0, plannercraft_core::TaskId::get),
                kind: if milestone {
                    TimelineKind::Milestone
                } else {
                    TimelineKind::Bar
                },
                start: task_start,
                finish: task_finish,
                name: task.name.clone(),
                date_label: day_month(task_start, &ctx),
                inactive: geom.role == BarRole::Inactive,
            })
        })
        .take(MAX_TIMELINE_TASKS)
        .collect();
    TimelineInput {
        start,
        finish,
        status: Some(status),
        start_label: bound_label(start_word, start, &ctx, lang),
        finish_label: bound_label(finish_word, finish, &ctx, lang),
        status_label: Some(format!("{status_word} {}", day_month(status, &ctx))),
        ticks: ticks(start, finish, lang),
        items,
    }
}

/// Données de la frise à imprimer avec le style `st` : celles de
/// [`input`] si la mise en page la demande (`page.timeline`, H-UI-82),
/// sinon `None`.
#[must_use]
pub fn print_input(s: &Session, st: &PlanStyle, lang: Language) -> Option<TimelineInput> {
    st.page.timeline.then(|| input(s, &st.timeline, lang))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::*;

    fn at(s: &str) -> Moment {
        s.parse().unwrap()
    }

    /// H-R16 : graduations des mois jusqu'à 24 mois, des trimestres
    /// jusqu'à 8 ans, des années au-delà ; premier jour de la période à
    /// 00:00, bornes comprises.
    #[test]
    fn h_r16_ticks_months_quarters_years() {
        assert_eq!(tick_unit(29), TickUnit::Month);
        assert_eq!(tick_unit(731), TickUnit::Month);
        assert_eq!(tick_unit(732), TickUnit::Quarter);
        assert_eq!(tick_unit(2_922), TickUnit::Quarter);
        assert_eq!(tick_unit(2_923), TickUnit::Year);
        // P : du 19/10/2026 au 17/11/2026, une graduation au 1er novembre.
        let t = ticks(at("2026-10-19T08:00"), at("2026-11-17T17:00"), Language::Fr);
        assert_eq!(t, vec![(at("2026-11-01T00:00"), "nov.".to_owned())]);
        // Mois, janvier avec l'année ; début au 1er à 00:00 compris.
        let t = ticks(at("2026-12-01T00:00"), at("2027-02-10T17:00"), Language::Fr);
        let labels: Vec<&str> = t.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(labels, ["déc.", "janv. 27", "févr."]);
        let t = ticks(at("2026-12-01T00:00"), at("2027-02-10T17:00"), Language::En);
        let labels: Vec<&str> = t.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(labels, ["Dec", "Jan '27", "Feb"]);
        // Trimestres : trois ans.
        let t = ticks(at("2026-10-19T08:00"), at("2029-10-19T17:00"), Language::Fr);
        assert_eq!(
            t.first().unwrap(),
            &(at("2027-01-01T00:00"), "T1 2027".to_owned())
        );
        assert_eq!(t.len(), 12);
        let t = ticks(at("2026-10-19T08:00"), at("2029-10-19T17:00"), Language::En);
        assert_eq!(t[1].1, "Q2 2027");
        // Années : dix ans.
        let t = ticks(at("2026-10-19T08:00"), at("2036-10-19T17:00"), Language::Fr);
        let labels: Vec<&str> = t.iter().map(|(_, s)| s.as_str()).collect();
        assert_eq!(labels.first(), Some(&"2027"));
        assert_eq!(labels.len(), 10);
        // Domaine entier : borné.
        assert!(ticks(Moment::MIN, Moment::MAX, Language::Fr).len() <= MAX_TICKS);
        // Fin avant le début : aucune graduation.
        assert_eq!(
            ticks(at("2027-01-01T00:00"), at("2026-01-01T00:00"), Language::Fr).len(),
            0
        );
    }

    #[test]
    fn timeline_labels_short_forms() {
        let ctx_fr = TextContext::for_options(&plannercraft_core::ProjectOptions::default(), Language::Fr);
        let m = at("2026-10-19T08:00");
        assert_eq!(
            bound_label("Début", m, &ctx_fr, Language::Fr),
            "Début lun 19/10/26"
        );
        assert_eq!(day_month(m, &ctx_fr), "19/10");
    }

    /// Projet « P » du repère (`docs/plans/ribbon.md` § 2), tâche 6
    /// inactive si `inactive_6`.
    fn session_p(inactive_6: bool) -> Session {
        use plannercraft_core::calendar::BuiltinCalendarNames;
        use plannercraft_core::{
            Duration, DurationUnit, Lag, Link, LinkType, Minutes, OutlineLevel, Project, ProjectInit,
            ScheduleAnchor, Task, TimeBasis,
        };
        let start = at("2026-10-19T08:00");
        let mut p = Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(start),
            current_date: start,
            calendar_names: BuiltinCalendarNames::default(),
        });
        p.status_date = Some(at("2026-11-06T17:00"));
        let rows: [(&str, i64, u16); 7] = [
            ("Gros œuvre", 0, 1),
            ("Fondations", 5, 2),
            ("Murs", 10, 2),
            ("Livraison", 0, 2),
            ("Second œuvre", 0, 1),
            ("Plâtrerie", 4, 2),
            ("Peinture", 3, 2),
        ];
        for (i, (name, days, level)) in rows.into_iter().enumerate() {
            let uid = TaskUid::new(u32::try_from(i).unwrap() + 1);
            let options = p.options;
            let mut t = Task::new(uid, name.to_owned(), &options);
            t.outline_level = OutlineLevel::new(level).unwrap();
            t.duration = Duration::new(
                Minutes::new(days * 480),
                DurationUnit::Day,
                TimeBasis::Working,
                false,
            )
            .unwrap();
            t.active = !(inactive_6 && i == 5);
            p.tasks.push(t).unwrap();
        }
        for (pred, succ) in [(2, 3), (3, 4), (3, 6), (6, 7)] {
            p.tasks
                .get_mut(TaskUid::new(succ))
                .unwrap()
                .predecessors
                .push(Link {
                    predecessor: TaskUid::new(pred),
                    kind: LinkType::FinishToStart,
                    lag: Lag::ZERO,
                });
        }
        Session::from_project(
            p,
            crate::session::DocOrigin {
                name: "P".to_owned(),
                path: None,
            },
            start,
            Language::Fr,
        )
    }

    /// Règle 3 : extrémités, date d'état et dates des éléments égales à
    /// celles du planning du moteur ; textes formatés ici.
    #[test]
    fn timeline_input_dates_from_engine() {
        let s = session_p(false);
        let spec = TimelineSpec {
            tasks: vec![2, 3, 4, 6],
        };
        let input = input(&s, &spec, Language::Fr);
        let sched = s.schedule();
        assert_eq!(input.start, sched.project_start);
        assert_eq!(input.finish, sched.project_finish);
        assert_eq!(input.finish, at("2026-11-17T17:00"));
        assert_eq!(input.start_label, "Début lun 19/10/26");
        assert_eq!(input.finish_label, "Fin mar 17/11/26");
        assert_eq!(input.status, Some(at("2026-11-06T17:00")));
        assert_eq!(input.status_label.as_deref(), Some("État 06/11"));
        assert_eq!(input.ticks, vec![(at("2026-11-01T00:00"), "nov.".to_owned())]);
        let uids: Vec<u32> = input.items.iter().map(|it| it.uid.get()).collect();
        assert_eq!(uids, [2, 3, 4, 6]);
        for it in &input.items {
            let Some(plannercraft_core::TaskOutcome::Scheduled(ts)) = sched.tasks.get(&it.uid) else {
                panic!("{:?} non planifiée", it.uid);
            };
            assert_eq!((it.start, it.finish), (ts.start, ts.finish));
            assert_eq!(it.id, it.uid.get());
            assert!(!it.inactive);
        }
        let murs = &input.items[1];
        assert_eq!(
            (murs.start, murs.finish),
            (at("2026-10-26T08:00"), at("2026-11-06T17:00"))
        );
        assert_eq!(murs.kind, TimelineKind::Bar);
        let livraison = &input.items[2];
        assert_eq!(livraison.kind, TimelineKind::Milestone);
        assert_eq!(livraison.date_label, "06/11");
        let en = input_en(&s, &spec);
        assert_eq!(en.start_label, "Start Mon 10/19/26");
        assert_eq!(en.finish_label, "Finish Tue 11/17/26");
        assert_eq!(en.status_label.as_deref(), Some("Status 11/06"));
        // Impression : seulement si la mise en page la demande.
        let mut st = plannercraft_render::style::template(plannercraft_render::style::TemplateId::Sobre);
        st.timeline = spec.clone();
        assert_eq!(print_input(&s, &st, Language::Fr), None);
        st.page.timeline = true;
        assert_eq!(print_input(&s, &st, Language::Fr).unwrap().items.len(), 4);
    }

    fn input_en(s: &Session, spec: &TimelineSpec) -> TimelineInput {
        input(s, spec, Language::En)
    }

    /// UID absent (tâche supprimée), doublons et désordre ignorés ; tâche
    /// inactive gardée, peinte en `Inactive` (placement d'affichage R-062).
    #[test]
    fn timeline_input_skips_deleted_uid() {
        let s = session_p(true);
        let spec = TimelineSpec {
            tasks: vec![99, 6, 3, 3, 0],
        };
        let input = input(&s, &spec, Language::Fr);
        let uids: Vec<u32> = input.items.iter().map(|it| it.uid.get()).collect();
        assert_eq!(uids, [3, 6]);
        assert!(input.items[1].inactive);
        assert!(!input.items[0].inactive);
        // Disposition : jamais de panique, deux éléments dessinés.
        let l = plannercraft_render::timeline::layout(&input, 800 * 16);
        assert_eq!(l.hidden, 0);
    }
}
