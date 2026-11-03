//! Générateur de projets pour l'aller-retour MSPDI (P24, P26) : calendriers,
//! exceptions, semaines datées, tâches (durées, liens ouvrés, écoulés et en
//! %, contraintes, échéances, mode manuel, lignes vides, avancement),
//! planification à rebours, ressources, affectations, lignes de base,
//! retards de nivellement (modèle et `Extras` hérités d'avant J5, R-198),
//! `Extras` valides et invalides, chaînes avec `&<>"\r` et caractères
//! interdits. Déterministe : une graine donne un projet.
//!
//! Partagé par `crates/plannercraft-mspdi/tests/roundtrip.rs` et
//! `tests/schema/main.rs` (`#[path]`).

#![allow(
    dead_code,
    reason = "module partagé : chaque cible n'en utilise qu'une partie"
)]

use plannercraft_core::calendar::{
    Calendar, CalendarException, DayHours, DayRule, DaySetting, MonthItem, MonthPosition, Recurrence,
    TimeRange, WorkWeek,
};
use plannercraft_core::{
    Assignment, AssignmentBaseline, AssignmentUid, BaselineNumber, BuiltinCalendarNames, CalendarUid,
    Constraint, ConversionFactors, Day, DayRange, Duration, DurationUnit, ExtraEntry, Extras, Guid, Lag,
    LagValue, LevelingDelay, Link, LinkType, Minutes, Moment, Money, Month, OutlineLevel, Percent, Priority,
    Project, ProjectInit, Rate, RateUnit, Resource, ResourceBaseline, ResourceKind, ResourceUid,
    ScheduleAnchor, SchedulingMode, SignedPercent, Task, TaskBaseline, TaskType, TaskUid, TimeBasis,
    TimeOfDay, Units, Week, Weekday, Work, WorkContour,
};

/// Générateur pseudo-aléatoire déterministe (xorshift64*).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[usize::try_from(self.below(items.len() as u64)).unwrap()]
    }
    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + i64::try_from(self.below(u64::try_from(hi - lo + 1).unwrap())).unwrap()
    }
}

fn m(s: &str) -> Moment {
    s.parse().unwrap()
}

/// R-198 : retard de nivellement (nul parfois, pour l'image `None`).
fn leveling_delay(r: &mut Rng) -> Option<LevelingDelay> {
    if r.chance(60) {
        return None;
    }
    let minutes = if r.chance(10) { 0 } else { r.int(1, 200_000) };
    let unit = *r.pick(&[
        DurationUnit::Minute,
        DurationUnit::Hour,
        DurationUnit::Day,
        DurationUnit::Week,
        DurationUnit::Month,
    ]);
    LevelingDelay::new(Minutes::new(minutes), unit, r.chance(50))
}

/// `Extras` `LevelingDelay`/`LevelingDelayFormat` d'un projet enregistré
/// avant J5 (dixièmes de minute, reste possible ; format parfois absent).
fn legacy_leveling(r: &mut Rng, extras: &mut Extras) {
    if !r.chance(15) {
        return;
    }
    let mspdi = |key: &str, payload: String| ExtraEntry {
        origin: "mspdi".to_owned(),
        key: key.to_owned(),
        payload,
    };
    let tenths = r.int(0, 50_000);
    extras.0.push(mspdi(
        "LevelingDelay",
        format!("<LevelingDelay>{tenths}</LevelingDelay>"),
    ));
    if r.chance(70) {
        let code = *r.pick(&[3_i64, 7, 8, 39, 19, 21, 99]);
        extras.0.push(mspdi(
            "LevelingDelayFormat",
            format!("<LevelingDelayFormat>{code}</LevelingDelayFormat>"),
        ));
    }
}

/// Chaîne : morceaux avec caractères spéciaux, parfois longue.
pub fn string(rng: &mut Rng) -> String {
    const PIECES: [&str; 12] = [
        "",
        "A",
        "A & B",
        "<x>",
        "\"q\"",
        "l1\r\nl2",
        "é€",
        "a\u{1}b",
        " sp ",
        "\t",
        "x\u{FFFF}",
        "]]>",
    ];
    if rng.chance(3) {
        return "é".repeat(600);
    }
    let n = rng.below(3);
    (0..n).map(|_| *rng.pick(&PIECES)).collect()
}

fn guid(rng: &mut Rng) -> Option<Guid> {
    rng.chance(40).then(|| {
        let mut b = [0_u8; 16];
        for x in &mut b {
            *x = u8::try_from(rng.below(256)).unwrap();
        }
        Guid::from_bytes(b)
    })
}

fn moment_near(rng: &mut Rng, base: Moment) -> Moment {
    Moment::from_minutes(base.minutes() + rng.int(-20 * 1440, 60 * 1440)).unwrap()
}

fn duration(rng: &mut Rng, max: i64) -> Duration {
    let unit = *rng.pick(&[
        DurationUnit::Minute,
        DurationUnit::Hour,
        DurationUnit::Day,
        DurationUnit::Week,
        DurationUnit::Month,
    ]);
    let basis = if rng.chance(15) {
        TimeBasis::Elapsed
    } else {
        TimeBasis::Working
    };
    Duration::new(Minutes::new(rng.int(0, max)), unit, basis, rng.chance(30)).unwrap()
}

fn work(rng: &mut Rng) -> Work {
    Work::new(Minutes::new(rng.int(0, 20_000))).unwrap()
}

fn hours(rng: &mut Rng) -> DayHours {
    let options: [&[(u16, u16)]; 4] = [
        &[(480, 720), (780, 1_020)],
        &[(0, 1_440)],
        &[(540, 780), (900, 1_140)],
        &[(1_080, 1_200), (1_260, 1_380), (1_380, 1_440)],
    ];
    let ranges = rng
        .pick(&options)
        .iter()
        .map(|(a, b)| TimeRange::new(TimeOfDay::new(*a).unwrap(), TimeOfDay::new(*b).unwrap()).unwrap())
        .collect();
    DayHours::new(ranges).unwrap()
}

fn setting(rng: &mut Rng) -> DaySetting {
    if rng.chance(50) {
        DaySetting::NonWorking
    } else {
        DaySetting::Working(hours(rng))
    }
}

fn rule(rng: &mut Rng) -> DayRule {
    match rng.below(3) {
        0 => DayRule::Inherit,
        1 => DayRule::NonWorking,
        _ => DayRule::Working(hours(rng)),
    }
}

fn recurrence(rng: &mut Rng) -> Recurrence {
    let every = u16::try_from(rng.int(1, 3)).unwrap();
    let month = *rng.pick(&Month::ALL);
    let position = *rng.pick(&[MonthPosition::First, MonthPosition::Second, MonthPosition::Last]);
    let item = *rng.pick(&[MonthItem::Day, MonthItem::Weekday, MonthItem::On(Weekday::Friday)]);
    let day = u8::try_from(rng.int(1, 28)).unwrap();
    match rng.below(9) {
        0 => Recurrence::Period,
        1 => Recurrence::Daily { every },
        2 => Recurrence::Weekly {
            every,
            days: Week::from_fn(|d| d == Weekday::Monday || (d == Weekday::Saturday && rng.chance(50))),
        },
        3 => Recurrence::MonthlyByDay { every, day },
        4 => Recurrence::MonthlyByPosition {
            every,
            position,
            item,
        },
        5 => Recurrence::YearlyByDay { month, day },
        6 => Recurrence::YearlyByPosition {
            month,
            position,
            item,
        },
        7 => Recurrence::ByDayCount { every },
        _ => Recurrence::ByWeekdayCount { every },
    }
}

/// `Extras` d'une entité, valides et invalides, depuis un menu.
fn extras(rng: &mut Rng, menu: &[(&str, &str)]) -> Extras {
    let mut out = Vec::new();
    let mut used: Vec<&str> = Vec::new();
    for _ in 0..rng.below(4) {
        let (key, payload) = *rng.pick(menu);
        if used.contains(&key) && !key.ends_with("ExtendedAttribute") {
            continue;
        }
        used.push(key);
        let origin = if rng.chance(10) { "plannercraft" } else { "mspdi" };
        out.push(ExtraEntry {
            origin: origin.to_owned(),
            key: key.to_owned(),
            payload: payload.to_owned(),
        });
    }
    Extras(out)
}

const TASK_EXTRAS: [(&str, &str); 9] = [
    ("Contact", "<Contact>c</Contact>"),
    ("Hyperlink", "<Hyperlink>h &amp; i</Hyperlink>"),
    (
        "ExtendedAttribute",
        "<ExtendedAttribute><FieldID>188743731</FieldID><Value>v</Value></ExtendedAttribute>",
    ),
    ("Name>Zzz", "<Zzz>1</Zzz>"),
    ("^>Yyy", "<Yyy/>"),
    ("Duration>Qqq", "<Qqq/>"),
    ("Contact>Www", "<Www><a/></Www>"),
    ("Name", "<Name>x</Name>"),
    ("Hyperlink", "<Hyperlink>x</Hyperlink><Evil/>"),
];

/// Un projet généré (graine).
#[must_use]
#[allow(clippy::too_many_lines, reason = "générateur de test : une étape par entité")]
pub fn project(seed: u64) -> Project {
    let mut rng = Rng::new(seed);
    let r = &mut rng;
    let start =
        Moment::from_minutes(m("2025-01-06T08:00").minutes() + r.int(0, 700) * 1_440 + r.int(0, 8) * 60)
            .unwrap();
    let anchor = if r.chance(25) {
        ScheduleAnchor::FromFinish(Moment::from_minutes(start.minutes() + 30 * 1_440).unwrap())
    } else {
        ScheduleAnchor::FromStart(start)
    };
    let mut p = Project::new(ProjectInit {
        name: string(r),
        anchor,
        current_date: moment_near(r, start),
        calendar_names: BuiltinCalendarNames::default(),
    });
    p.info.title = string(r);
    p.info.author = string(r);
    p.info.company = string(r);
    p.info.guid = guid(r);
    if r.chance(30) {
        p.info.creation_date = None;
    }
    p.status_date = r.chance(30).then(|| moment_near(r, start));
    let per_day = *r.pick(&[480_u32, 450, 420, 600, 1_440]);
    p.options.factors = ConversionFactors::new(per_day, per_day * 5, *r.pick(&[20_u32, 21, 30])).unwrap();
    p.options.default_start_time = TimeOfDay::new(*r.pick(&[480_u16, 540, 450])).unwrap();
    p.options.default_finish_time = TimeOfDay::new(*r.pick(&[1_020_u16, 1_080, 1_440])).unwrap();
    p.options.default_duration_unit = *r.pick(&[DurationUnit::Day, DurationUnit::Hour, DurationUnit::Week]);
    p.options.default_work_unit = *r.pick(&[DurationUnit::Hour, DurationUnit::Day]);
    p.options.default_task_type =
        *r.pick(&[TaskType::FixedUnits, TaskType::FixedDuration, TaskType::FixedWork]);
    p.options.new_tasks_effort_driven = r.chance(50);
    p.options.new_tasks_estimated = r.chance(50);
    p.options.week_start_day = *r.pick(&Weekday::ALL);
    p.options.honor_constraints = r.chance(50);
    p.options.critical_slack_limit_days = u32::try_from(r.below(4)).unwrap();
    p.options.multiple_critical_paths = r.chance(30);
    // P43 (R-214, R-218) : options de la date d'état.
    p.options.progress.split_in_progress = r.chance(70);
    p.options.progress.task_updates_resource = r.chance(80);
    p.options.progress.move_completed_ends_back = r.chance(10);
    p.options.progress.move_remaining_starts_back = r.chance(10);
    p.options.progress.move_remaining_starts_forward = r.chance(10);
    p.options.progress.move_completed_ends_forward = r.chance(10);

    // Calendriers : dérivé du Standard, base supplémentaire, exceptions,
    // semaines datées (périodes disjointes par construction).
    let mut cal_uids = vec![CalendarUid::new(1), CalendarUid::new(2), CalendarUid::new(3)];
    if r.chance(60) {
        let mut d = Calendar::new_derived(CalendarUid::new(10), string(r), CalendarUid::new(1));
        for day in Weekday::ALL {
            let _ = d.set_day(day, rule(r));
        }
        d.guid = guid(r);
        cal_uids.push(CalendarUid::new(10));
        p.calendars.push(d).unwrap();
    }
    if r.chance(40) {
        let week = Week::from_fn(|_| setting(r));
        let mut b = Calendar::new_base(CalendarUid::new(11), string(r), week);
        b.extras = extras(
            r,
            &[
                ("IsBaselineCalendar", "<IsBaselineCalendar>0</IsBaselineCalendar>"),
                ("Name>Zzz", "<Zzz/>"),
            ],
        );
        cal_uids.push(CalendarUid::new(11));
        p.calendars.push(b).unwrap();
    }
    for uid in cal_uids.clone() {
        let Some(c) = p.calendars.get_mut(uid) else {
            continue;
        };
        for k in 0..r.below(3) {
            let year = 2026 + i32::try_from(k).unwrap();
            let first: Day = format!("{year}-03-0{}", 1 + r.below(5)).parse().unwrap();
            let last = first
                .checked_add_days(i32::try_from(r.below(40)).unwrap())
                .unwrap();
            let _ = c.add_exception(CalendarException {
                name: string(r),
                period: DayRange::new(first, last).unwrap(),
                setting: setting(r),
                recurrence: recurrence(r),
                occurrences: r.chance(20).then(|| u32::try_from(r.int(1, 4)).unwrap()),
            });
        }
        for k in 0..r.below(2) {
            let first: Day = format!("2030-0{}-01", 1 + k * 3).parse().unwrap();
            let _ = c.add_work_week(WorkWeek {
                name: string(r),
                period: DayRange::new(first, first.checked_add_days(20).unwrap()).unwrap(),
                days: Week::from_fn(|_| rule(r)),
            });
        }
    }

    // Tâches.
    let n = r.below(9);
    let mut level: u16 = 0;
    let mut real: Vec<TaskUid> = Vec::new();
    for i in 0..n {
        let uid = TaskUid::new(u32::try_from(1 + i * 3).unwrap());
        let mut t = Task::new(uid, string(r), &p.options);
        let next = if level == 0 {
            1
        } else {
            u16::try_from(r.int(1, i64::from(level) + 1)).unwrap()
        };
        t.outline_level = OutlineLevel::new(next).unwrap();
        if r.chance(10) {
            // Ligne vide : son niveau ne compte pas dans la hiérarchie.
            t.is_null = true;
            p.tasks.push(t).unwrap();
            continue;
        }
        level = next;
        t.duration = duration(r, 30_000);
        t.work = work(r);
        t.task_type = *r.pick(&[TaskType::FixedUnits, TaskType::FixedDuration, TaskType::FixedWork]);
        t.effort_driven = r.chance(50);
        t.milestone = r.chance(10);
        t.priority = Priority::new(u16::try_from(r.int(0, 1_000)).unwrap()).unwrap();
        t.guid = guid(r);
        t.wbs = r.chance(30).then(|| string(r));
        t.notes = string(r);
        t.created = r.chance(30).then(|| moment_near(r, start));
        t.calendar = r.chance(30).then(|| *r.pick(&cal_uids));
        t.ignore_resource_calendar = r.chance(20);
        t.active = r.chance(90);
        t.recurring = r.chance(5);
        let date = moment_near(r, start);
        t.constraint = match r.below(8) {
            0 => Constraint::AsSoonAsPossible,
            1 => Constraint::AsLateAsPossible,
            2 => Constraint::MustStartOn { date },
            3 => Constraint::MustFinishOn { date },
            4 => Constraint::StartNoEarlierThan { date },
            5 => Constraint::StartNoLaterThan { date },
            6 => Constraint::FinishNoEarlierThan { date },
            _ => Constraint::FinishNoLaterThan { date },
        };
        t.deadline = r.chance(20).then(|| moment_near(r, start));
        let pr = &mut t.progress;
        pr.percent_complete = Percent::new(r.int(0, 10_000)).unwrap();
        pr.percent_work_complete = Percent::new(r.int(0, 10_000)).unwrap();
        pr.physical_percent_complete = Percent::new(r.int(0, 10_000)).unwrap();
        pr.actual_start = r.chance(20).then(|| moment_near(r, start));
        pr.actual_finish = r.chance(10).then(|| moment_near(r, start));
        pr.actual_duration = Minutes::new(r.int(0, 2_000));
        pr.remaining_duration = Minutes::new(r.int(0, 2_000));
        pr.actual_work = work(r);
        pr.remaining_work = work(r);
        // P43 : reprise saisie (gardée seulement sur une feuille en cours).
        pr.resume = r.chance(25).then(|| moment_near(r, start));
        if r.chance(15) {
            t.mode = SchedulingMode::Manual {
                start: r.chance(60).then(|| moment_near(r, start)),
                finish: r.chance(60).then(|| moment_near(r, start)),
                duration: r.chance(50).then(|| duration(r, 5_000)),
            };
        }
        // Liens vers des tâches antérieures non vides, sans doublon.
        let mut preds: Vec<TaskUid> = Vec::new();
        for _ in 0..r.below(3) {
            if real.is_empty() {
                break;
            }
            let pred = *r.pick(&real);
            if preds.contains(&pred) {
                continue;
            }
            preds.push(pred);
            let kind = *r.pick(&[
                LinkType::FinishToStart,
                LinkType::StartToStart,
                LinkType::FinishToFinish,
                LinkType::StartToFinish,
            ]);
            let basis = if r.chance(20) {
                TimeBasis::Elapsed
            } else {
                TimeBasis::Working
            };
            let value = if r.chance(25) {
                LagValue::Percent {
                    percent: SignedPercent::new(r.int(-10_000, 10_000)).unwrap(),
                }
            } else {
                LagValue::Time {
                    minutes: Minutes::new(r.int(-2_000, 5_000)),
                    unit: *r.pick(&[DurationUnit::Minute, DurationUnit::Hour, DurationUnit::Day]),
                }
            };
            t.predecessors.push(Link {
                predecessor: pred,
                kind,
                lag: Lag::new(basis, value).unwrap(),
            });
        }
        for _ in 0..r.below(3) {
            let n = BaselineNumber::new(u8::try_from(r.below(11)).unwrap()).unwrap();
            t.baselines.insert(
                n,
                TaskBaseline {
                    start: r.chance(70).then(|| moment_near(r, start)),
                    finish: r.chance(70).then(|| moment_near(r, start)),
                    duration: r.chance(70).then(|| duration(r, 10_000)),
                    work: r.chance(50).then(|| work(r)),
                    cost: r.chance(50).then(|| Money::new(r.int(0, 10_000_000)).unwrap()),
                    fixed_cost: r.chance(30).then(|| Money::new(r.int(0, 100_000)).unwrap()),
                },
            );
        }
        t.extras = extras(r, &TASK_EXTRAS);
        t.leveling_delay = leveling_delay(r);
        legacy_leveling(r, &mut t.extras);
        if let Some(n) = t.baselines.keys().next()
            && r.chance(50)
        {
            t.extras.0.push(ExtraEntry {
                origin: "mspdi".to_owned(),
                key: format!("Baseline#{}/Interim", n.get()),
                payload: "<Interim>0</Interim>".to_owned(),
            });
        }
        real.push(uid);
        p.tasks.push(t).unwrap();
    }
    if r.chance(50) {
        let mut s = Task::new(TaskUid::PROJECT_SUMMARY, string(r), &p.options);
        s.extras = extras(r, &TASK_EXTRAS[..3]);
        if r.chance(20) {
            s.mode = SchedulingMode::Manual {
                start: Some(start),
                finish: None,
                duration: Some(duration(r, 3_000)),
            };
        }
        p.summary_task = Some(s);
    }

    // Ressources et affectations.
    let mut res: Vec<ResourceUid> = Vec::new();
    for i in 0..r.below(4) {
        let uid = ResourceUid::new(u32::try_from(i + 1).unwrap());
        let mut x = Resource::new(uid, string(r));
        x.initials = string(r);
        x.group = string(r);
        x.notes = string(r);
        x.guid = guid(r);
        x.kind = match r.below(3) {
            0 => ResourceKind::Material { label: string(r) },
            1 => ResourceKind::Cost,
            _ => ResourceKind::Work,
        };
        x.max_units = Units::new(r.int(0, 30_000)).unwrap();
        let per = *r.pick(&[
            RateUnit::Minute,
            RateUnit::Hour,
            RateUnit::Day,
            RateUnit::Week,
            RateUnit::Month,
            RateUnit::Year,
            RateUnit::MaterialUnit,
        ]);
        x.standard_rate = Rate {
            amount: Money::new(r.int(0, 10_000_000)).unwrap(),
            per,
        };
        x.overtime_rate = Rate {
            amount: Money::new(r.int(0, 100_000)).unwrap(),
            per: RateUnit::Hour,
        };
        x.cost_per_use = Money::new(r.int(0, 100_000)).unwrap();
        x.calendar = r.chance(40).then(|| *r.pick(&cal_uids));
        x.is_generic = r.chance(10);
        x.is_inactive = r.chance(10);
        if r.chance(40) {
            x.baselines.insert(
                BaselineNumber::new(0).unwrap(),
                ResourceBaseline {
                    work: Some(work(r)),
                    cost: r.chance(50).then(|| Money::new(r.int(0, 1_000_000)).unwrap()),
                },
            );
        }
        x.extras = extras(
            r,
            &[
                ("Phonetics", "<Phonetics>p</Phonetics>"),
                ("Name>Zzz", "<Zzz/>"),
                ("Cost", "<Cost>1</Cost>"),
            ],
        );
        res.push(uid);
        p.resources.push(x).unwrap();
    }
    for i in 0..r.below(5) {
        if real.is_empty() {
            break;
        }
        let task = *r.pick(&real);
        let resource = if res.is_empty() || r.chance(20) {
            None
        } else {
            Some(*r.pick(&res))
        };
        let mut a = Assignment::new(AssignmentUid::new(u32::try_from(i + 1).unwrap()), task, resource);
        a.units = Units::new(r.int(0, 30_000)).unwrap();
        a.work = work(r);
        a.actual_work = work(r);
        a.remaining_work = work(r);
        a.delay = Minutes::new(r.int(0, 3_000));
        a.contour = *r.pick(&[
            WorkContour::Flat,
            WorkContour::Bell,
            WorkContour::Contoured,
            WorkContour::Turtle,
        ]);
        a.guid = guid(r);
        if r.chance(40) {
            a.baselines.insert(
                BaselineNumber::new(1).unwrap(),
                AssignmentBaseline {
                    start: Some(moment_near(r, start)),
                    finish: None,
                    work: Some(work(r)),
                    cost: None,
                },
            );
        }
        a.extras = extras(
            r,
            &[
                ("Hyperlink", "<Hyperlink>h</Hyperlink>"),
                ("Notes", "<Notes>n</Notes>"),
                ("UID>Zzz", "<Zzz/>"),
            ],
        );
        a.leveling_delay = leveling_delay(r);
        legacy_leveling(r, &mut a.extras);
        // R-199 : répartition d'un profil modifié à la main (type 1, gardée
        // à la lecture) et réel (type 2) ; le placement qu'elle décrit
        // diffère presque toujours de celui du moteur (jetée à l'écriture
        // avec les valeurs du moteur, `schedule_image`).
        if a.contour == WorkContour::Contoured && r.chance(50) {
            let at = plannercraft_mspdi::scalar::format_datetime(moment_near(r, start));
            for kind in [1, 2] {
                a.extras.0.push(ExtraEntry {
                    origin: "mspdi".to_owned(),
                    key: "TimephasedData".to_owned(),
                    payload: format!(
                        "<TimephasedData><Type>{kind}</Type><UID>{}</UID><Start>{at}</Start><Finish>{at}</Finish><Unit>2</Unit><Value>PT1H0M0S</Value></TimephasedData>",
                        i + 1
                    ),
                });
            }
        }
        p.assignments.push(a).unwrap();
    }

    p.extras = extras(
        r,
        &[
            ("UID", "<UID>7</UID>"),
            ("@xmlns:q", "urn:q"),
            ("Assignments>Pp", "<Pp/>"),
            ("CalendarUID>Views", "<Views><View><Name>v</Name></View></Views>"),
            ("CurrencyCode", "<CurrencyCode>USD</CurrencyCode>"),
            ("Bogus>Zz", "<Zz/>"),
        ],
    );
    p
}
