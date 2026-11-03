//! Oracle des marges et du % achevé des récapitulatives (ADR-0012, lot L15,
//! dette J4 de l'ADR-0010).
//!
//! L'implémentation rapide (arcs hérités réduits par une propagation
//! descendante, sommes préfixes du % achevé) doit rendre exactement le même
//! `Schedule` que l'implémentation de référence de J3 (remontée des ancêtres
//! pour chaque nœud, sous-arbre parcouru pour chaque récapitulative), gardée
//! dans le moteur derrière `schedule::schedule_reference` : elle a besoin des
//! états internes du calcul et sert aussi de repli à la stratégie rapide.
//!
//! Générateur (xorshift à graine tirée par proptest) : 1 à 60 tâches,
//! hiérarchie profonde une fois sur deux (niveau qui monte d'un cran avec
//! 60 % de chances), liens des quatre types depuis et vers des
//! récapitulatives, dans les deux sens de la liste (cycles possibles,
//! coupés par R-065), liens récapitulative → descendante occasionnels
//! (refusés par le moteur), décalages ouvrés, écoulés, en % et parfois
//! énormes (conversions hors du domaine : chemin de repli), calendriers
//! Standard, 24 heures, Équipe de nuit, pour le projet et 20 % des tâches ;
//! jalons, tâches inactives, manuelles (récapitulatives manuelles datées :
//! héritage coupé, R-064), contraintes datées, ALAP, échéances, % achevé ;
//! planification à rebours une fois sur trois. 2 000 cas.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::schedule::schedule_reference;
use plannercraft_core::{
    CalendarUid, Constraint, Duration, DurationUnit, Lag, LagValue, Link, LinkType, Minutes, Moment,
    OutlineLevel, Percent, Project, ProjectInit, ScheduleAnchor, SchedulingMode, SignedPercent, Task,
    TaskOutcome, TaskRole, TaskUid, TimeBasis, schedule,
};
use proptest::prelude::*;

/// xorshift64*.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Entier dans `[lo, hi]`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = u64::try_from(hi - lo + 1).unwrap();
        lo + i64::try_from(self.next() % span).unwrap()
    }

    /// Vrai avec une probabilité de `pct` %.
    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }

    /// Indice dans `0..n` (`n > 0`).
    fn index(&mut self, n: usize) -> usize {
        usize::try_from(self.range(0, i64::try_from(n).unwrap() - 1)).unwrap()
    }
}

const KINDS: [LinkType; 4] = [
    LinkType::FinishToStart,
    LinkType::StartToStart,
    LinkType::FinishToFinish,
    LinkType::StartToFinish,
];

fn basis(r: &mut Rng, pct: u64) -> TimeBasis {
    if r.chance(pct) {
        TimeBasis::Elapsed
    } else {
        TimeBasis::Working
    }
}

fn lag(r: &mut Rng) -> Lag {
    if r.chance(40) {
        return Lag::ZERO;
    }
    if r.chance(15) {
        let percent = SignedPercent::new(r.range(-10_000, 10_000)).unwrap();
        return Lag::new(basis(r, 30), LagValue::Percent { percent }).unwrap();
    }
    // 3 % de décalages énormes : conversions hors du domaine.
    let minutes = if r.chance(3) {
        r.range(-87_308_640, 87_308_640)
    } else {
        r.range(-3_000, 3_000)
    };
    Lag::new(
        basis(r, 15),
        LagValue::Time {
            minutes: Minutes::new(minutes),
            unit: DurationUnit::Day,
        },
    )
    .unwrap()
}

fn generate(seed: u64) -> Project {
    let mut r = Rng(seed | 1);
    let base: Moment = "2026-01-05T08:00".parse().unwrap();
    let mut offset = r.range(0, 300) * 1_440;
    if r.chance(30) {
        offset += r.range(0, 1_439);
    }
    let start = base.checked_add(Minutes::new(offset)).unwrap();
    let from_finish = r.chance(33);
    let anchor = if from_finish {
        ScheduleAnchor::FromFinish(start.checked_add(Minutes::new(120 * 1_440)).unwrap())
    } else {
        ScheduleAnchor::FromStart(start)
    };
    let mut p = Project::new(ProjectInit {
        name: "Oracle".to_owned(),
        anchor,
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    p.calendar = CalendarUid::new(u32::try_from(r.range(1, 3)).unwrap());
    p.options.honor_constraints = !r.chance(30);
    p.options.critical_slack_limit_days = u32::try_from(r.range(0, 1)).unwrap();
    p.options.multiple_critical_paths = r.chance(15);
    let n = usize::try_from(r.range(1, 60)).unwrap();
    let deep = r.chance(50);
    let options = p.options;
    let mut level: i64 = 1;
    for i in 1..=n {
        let uid = TaskUid::new(u32::try_from(i).unwrap());
        let mut t = Task::new(uid, format!("T{i}"), &options);
        let minutes = if r.chance(10) { 0 } else { r.range(1, 4_800) };
        t.duration =
            Duration::new(Minutes::new(minutes), DurationUnit::Day, basis(&mut r, 10), false).unwrap();
        if i > 1 {
            level = if deep {
                if r.chance(60) {
                    level + 1
                } else {
                    r.range(1, level)
                }
            } else {
                r.range(1, level + 1)
            };
        }
        t.outline_level = OutlineLevel::new(u16::try_from(level).unwrap()).unwrap();
        if r.chance(20) {
            t.calendar = Some(CalendarUid::new(u32::try_from(r.range(1, 3)).unwrap()));
        }
        t.active = !r.chance(5);
        t.progress.percent_complete =
            Percent::new(if r.chance(50) { 0 } else { r.range(0, 10_000) }).unwrap();
        if r.chance(6) {
            let s = start.checked_add(Minutes::new(r.range(0, 30) * 1_440)).unwrap();
            t.mode = SchedulingMode::Manual {
                start: Some(s),
                finish: Some(s.checked_add(Minutes::new(r.range(0, 10) * 1_440)).unwrap()),
                duration: None,
            };
        }
        let date = start.checked_add(Minutes::new(r.range(-3, 40) * 1_440)).unwrap();
        if r.chance(15) {
            t.constraint = match r.range(0, 5) {
                0 => Constraint::MustStartOn { date },
                1 => Constraint::MustFinishOn { date },
                2 => Constraint::StartNoEarlierThan { date },
                3 => Constraint::StartNoLaterThan { date },
                4 => Constraint::FinishNoEarlierThan { date },
                _ => Constraint::FinishNoLaterThan { date },
            };
        } else if r.chance(5) {
            t.constraint = if from_finish {
                Constraint::AsSoonAsPossible
            } else {
                Constraint::AsLateAsPossible
            };
        }
        if r.chance(5) {
            t.deadline = Some(date);
        }
        p.tasks.push(t).unwrap();
    }
    // Liens : 0 à 3 prédécesseurs par tâche, 80 % vers l'avant de la liste.
    let uids: Vec<TaskUid> = p.tasks.iter().map(Task::uid).collect();
    for s in 0..n {
        let mut links: Vec<Link> = Vec::new();
        for _ in 0..r.range(0, 3) {
            let q = if s > 0 && r.chance(80) {
                r.index(s)
            } else {
                r.index(n)
            };
            if q == s || links.iter().any(|l| l.predecessor == uids[q]) {
                continue;
            }
            links.push(Link {
                predecessor: uids[q],
                kind: KINDS[r.index(KINDS.len())],
                lag: lag(&mut r),
            });
        }
        p.tasks.get_mut(uids[s]).unwrap().predecessors = links;
    }
    p
}

/// Égalité complète des deux implémentations.
fn same(p: &Project) {
    let fast = schedule(p);
    let reference = schedule_reference(p);
    assert_eq!(fast, reference);
}

/// La récapitulative la plus profonde de `p` (pour vérifier que le
/// générateur produit bien des hiérarchies profondes).
fn depth(p: &Project) -> u16 {
    p.tasks.iter().map(|t| t.outline_level.get()).max().unwrap_or(1)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn slack_rollup_equal_oracle(seed in any::<u64>()) {
        same(&generate(seed));
    }
}

/// Les cas tirés couvrent ce que l'oracle doit départager : hiérarchies
/// profondes, récapitulatives planifiées à % achevé non nul, tâches
/// dégradées (chemin de repli) et marges libres non nulles.
#[test]
fn oracle_generator_coverage() {
    let (mut deep, mut summaries, mut degraded, mut free) = (0, 0, 0, 0);
    for seed in 0..400u64 {
        let p = generate(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        if depth(&p) >= 10 {
            deep += 1;
        }
        let s = schedule(&p);
        for o in s.tasks.values() {
            let TaskOutcome::Scheduled(ts) = o else { continue };
            if ts.role == TaskRole::Summary && ts.percent_complete.get() > 0 {
                summaries += 1;
            }
            if ts.degraded {
                degraded += 1;
            }
            if ts.free_slack > Minutes::ZERO {
                free += 1;
            }
        }
    }
    eprintln!(
        "{deep} profonds, {summaries} récapitulatives avancées, {degraded} dégradées, {free} marges libres"
    );
    assert!(deep >= 30, "{deep} réseaux profonds");
    assert!(summaries >= 100, "{summaries} récapitulatives avancées");
    assert!(degraded >= 1, "{degraded} tâches dégradées");
    assert!(free >= 100, "{free} marges libres non nulles");
}

/// Hiérarchie de 300 niveaux, liens FS, SS et FF depuis chaque
/// récapitulative : égalité exacte sur un cas profond déterministe.
#[test]
fn deep_ladder_equal_oracle() {
    let start: Moment = "2026-11-02T08:00".parse().unwrap();
    let mut p = Project::new(ProjectInit {
        name: "Échelle".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    let options = p.options;
    let depth = 300u32;
    for i in 1..=depth {
        let mut t = Task::new(TaskUid::new(i), format!("S{i}"), &options);
        t.outline_level = OutlineLevel::new(u16::try_from(i).unwrap()).unwrap();
        t.duration = Duration::new(Minutes::new(480), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        t.progress.percent_complete = Percent::new(i64::from(i % 7) * 1_000).unwrap();
        p.tasks.push(t).unwrap();
    }
    // Successeurs de premier niveau, liés depuis les récapitulatives.
    for j in 0..30u32 {
        let uid = TaskUid::new(depth + 1 + j);
        let mut t = Task::new(uid, format!("Z{j}"), &options);
        t.duration = Duration::new(Minutes::new(960), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        for k in 0..10u32 {
            let from = 1 + (j * 10 + k) % (depth - 1);
            t.predecessors.push(Link {
                predecessor: TaskUid::new(from),
                kind: KINDS[usize::try_from((j + k) % 3).unwrap()],
                lag: Lag::new(
                    TimeBasis::Working,
                    LagValue::Time {
                        minutes: Minutes::new(i64::from(k) * 60),
                        unit: DurationUnit::Hour,
                    },
                )
                .unwrap(),
            });
        }
        p.tasks.push(t).unwrap();
    }
    same(&p);
}
