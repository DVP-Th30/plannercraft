//! Propriétés du moteur CPM (spécification J2 § 5.2, propriétés P13 à P17 ;
//! spécification J3 § 9.1, P13 amendée, P20 à P22).
//!
//! Nombre de cas : la variable `PROPTEST_CASES`, lue par proptest (256 par
//! défaut en local, 10 000 en CI) ; P13 et P14 la suivent, P15, P16 et P20 à
//! P22 sont plafonnées à 1 024 cas, P17 à 2 000 (spécification). Régressions :
//! `schedule_properties.proptest-regressions`, versionné.
//!
//! Générateur (graine tirée par proptest, xorshift) : 1 à 40 tâches ; liens
//! tirés dans un ordre topologique caché, permuté par rapport aux ID (des
//! liens remontent donc la liste) ; durées 0 à 4 800 (10 % de jalons, 10 %
//! écoulées) ; 0 à 3 prédécesseurs distincts, des quatre types ; décalages
//! ouvrés de −2 000 à 2 000 (15 % écoulés, 10 % en %, de −100 % à 100 %) ;
//! calendriers Standard, 24 heures, Équipe de nuit ou aléatoire, pour le
//! projet et 20 % des tâches ; plan hiérarchique valide une fois sur deux,
//! sans lien entre une récapitulative et sa descendante ; 5 % de tâches
//! inactives ; début du projet en 2026. Forme `CONSTRAINED` (J3) : en plus,
//! 20 % de contraintes datées (dates de −3 à +40 jours autour du début, à la
//! minute), 5 % d'ALAP, 5 % d'échéances, sur toutes les tâches
//! (récapitulatives comprises), option « respecter les contraintes »
//! inactive 30 % du temps.
//!
//! - **P13** (composite, amendée en J3, réseaux `CONSTRAINED`) : (a') liens
//!   respectés par les dates planifiées, sauf ceux qu'un
//!   `constraint.override` nomme (lien, ou porte de la récapitulative) ;
//!   (b') `FS ≥ 0` toujours, `TF ≥ FS` quand `TF ≥ 0`, et hors de la
//!   fermeture amont des plafonds (SNLT, FNLT, MSO, MFO, échéances) `TF ≥ 0`
//!   comme en J2 ; (c) `ES ≤ EF`, `LS ≤ LF`, dates planifiées = au plus tôt,
//!   une épingle a `early == late`, récapitulatives = enveloppe de leurs
//!   enfants ; (d) déterminisme (clone, JSON) et permutation des liens (tout
//!   identique sauf `driver`) ; (e) point fixe sur les feuilles non
//!   contraintes ; (f) `order` topologique ; (g) dates au plus tard
//!   réalisables : feuilles placées à leurs dates au plus tard, aucun lien
//!   violé sauf ceux qui partent d'une épingle (ou d'une récapitulative qui
//!   en contient une), fin du projet jamais retardée (inchangée sans
//!   plafond, en temps ouvré du calendrier du projet) ; (h) retards sans
//!   effet : une feuille non contrainte retardée de sa marge totale ne
//!   décale pas la fin du projet, de sa marge libre aucune autre feuille
//!   (en temps ouvré de leur échelle : la fin d'un plancher de fin peut être
//!   le début d'une plage, R-090 ; échantillon de deux feuilles par sorte
//!   de marge).
//!   (e) et (h) ne s'appliquent pas après une épingle ALAP qui déplace une
//!   tâche : la repasse fige `K(S)` et R-051 sur la passe principale
//!   (R-092).
//! - **P14** : chaîne de marge nulle (ADR-0005 amendée par l'ADR-0009), sans
//!   contrainte.
//! - **P15** : oracle indépendant en temps ouvré (plus long chemin entier),
//!   sur un réseau sans jalon (un jalon lit sa borne en brut, R-031, R-047).
//! - **P16** : monotonie de la fin du projet (liens FS et SS), avec des
//!   contraintes de début (MSO, SNET, SNLT), des ALAP, des échéances et
//!   l'option tirées. Les contraintes de fin (MFO, FNET, FNLT) en sont
//!   exclues : allonger la tâche recule son début, et un successeur SS avec
//!   lui, si bien que la fin du projet peut avancer.
//! - **P17** : chaos (contraintes partout, récapitulatives et manuelles
//!   comprises, dates aux bornes du domaine), aucune panique, cycles
//!   signalés, (a') et (b') hors des tâches dégradées et des composantes qui
//!   contiennent une manuelle.
//! - **P20** : contrainte tenue ou signalée : plancher tenu en temps ouvré ;
//!   plafond tenu, ou `constraint.not_honored` (et marge négative s'il est
//!   dépassé en temps ouvré) ; serrage tenu ; épingle exacte ;
//!   `task.deadline_missed` ⇔ fin planifiée après l'échéance.
//! - **P21** : oracle de P15 étendu (planchers, serrages, épingles,
//!   plafonds, fenêtres, échéances, ALAP) en minutes ouvrées, sans jalon, un
//!   seul calendrier.
//! - **P22** : ALAP : feuilles ALAP de marge ≥ 0 planifiées sur leurs dates
//!   au plus tard ; aucun lien violé ; fin du projet égale à celle du même
//!   réseau tout en ASAP.
//!
//! Planification à rebours (J3, R-106 à R-111), générateur
//! `generate_from_finish` (ancre `F` de 5 à 60 jours après le début tiré,
//! contraintes autour de `F`, ALAP par défaut, ASAP tirée 5 % du temps) :
//! - **P13 à rebours** : forme, déterminisme, permutation des liens (pilotes
//!   et causes exceptés), (a') liens respectés par les dates planifiées (au
//!   plus tard) sauf ceux qu'un `constraint.override { Successor }` nomme,
//!   (b') hors de la fermeture aval des planchers (SNET, FNET, MSO, MFO)
//!   `TF ≥ 0`, (c) dates planifiées = au plus tard, enveloppes, (f) ordre ;
//!   (e') point fixe des dates au plus tard (feuilles ALAP passées en
//!   manuelles, sans ASAP) ; (g') feuilles placées à leurs dates au plus tôt
//!   sans lien violé (sauf vers une épingle), début calculé jamais avancé.
//! - **P16b** : allonger une feuille ne retarde jamais le début calculé
//!   `S*`.
//! - **P17** : une graine sur deux planifiée à rebours ; mêmes contrôles
//!   (a'), (b') hors des tâches dégradées et des composantes manuelles.
//! - **P34** (J5, R-173) : une affectation de travail unique par feuille
//!   automatique active, ressource sans calendrier ou sur le calendrier de
//!   la tâche, sans délai, de `d_a` égal à la durée J4 (durée enregistrée
//!   déplacée pour les unités fixes) : planning identique à celui du même
//!   projet sans affectation (réseaux `CONSTRAINED`, depuis le début et à
//!   rebours) ; l'affectation a les dates de sa tâche.
//! - **P35** (J5, partie moteur, R-171, R-173) : rendre chômé un jour d'un
//!   calendrier de ressource ne fait jamais commencer ni finir une tâche
//!   plus tôt (réseau de feuilles affectées à 1 à 3 ressources, délais,
//!   liens FS et SS ; comme pour P16, une borne de fin — lien FF, SF,
//!   contrainte de fin — recule le début d'une tâche allongée, et un
//!   successeur SS avec lui).
//! - **P37** (J6, partie moteur) : sans avancement, aucun avancement
//!   calculé ni diagnostic d'avancement, et changer la seule date d'état ne
//!   change pas le planning (P13 à P36 restent vertes sans changement).
//! - **P38** (J6, R-203) : une feuille achevée, après des éditions qui ne
//!   touchent pas son avancement, a pour dates planifiées, au plus tôt et au
//!   plus tard ses dates réelles, TS = FS = 0, n'est pas critique ; un
//!   successeur FS sans décalage, automatique, sans contrainte qui l'impose,
//!   commence au plus tôt après sa fin réelle ; implémentations rapide et de référence
//!   identiques.
//! - **P44** (J6, R-204 à R-207) : une feuille automatique en cours (début
//!   réel saisi) a pour début planifié son début réel, `Stop` ≥ début réel,
//!   `Resume` ≥ `Stop`, fin ≥ `Resume` ; sa partie réelle (début réel,
//!   `Stop`) est inchangée par d'autres éditions (durées des autres tâches,
//!   option « respecter les contraintes ») ; ajouter un lien FS entrant ne
//!   recule jamais sa fin (depuis le début, réseau sans contrainte) ;
//!   implémentations rapide et de référence identiques.
//! - **P23** : symétrie avant/arrière (ADR-0010) sur un réseau plat, un
//!   calendrier, décalages ouvrés, sans jalon, sans %, sans échéance :
//!   liens retournés (FS → FS, SS → FF, FF → SS, SF → SF), contraintes
//!   ASAP ↔ ALAP, SNET ↔ FNLT, FNET ↔ SNLT, MSO ↔ MFO, dates miroir en temps
//!   ouvré ; mêmes dates en temps ouvré miroir, mêmes marges totales, mêmes
//!   critiques, fin ↔ début calculé. **P23b** : cas concret sur le
//!   calendrier Standard, symétrique autour du mercredi 12:30, dates miroir
//!   brutes `t' = 2c − t`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::too_many_lines,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::struct_excessive_bools,
    clippy::needless_range_loop,
    clippy::items_after_statements,
    reason = "tests : notations des formules (ES, EF, d, L…) et indices de l'oracle, plus lisibles ainsi"
)]

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::calendar::{
    BuiltinCalendarNames, Calendar, CalendarSet, CompiledCalendar, DayHours, DaySetting, TimeRange, TimeScale,
};
use plannercraft_core::{
    Assignment, AssignmentRole, AssignmentUid, CalendarUid, Constraint, ConstraintKind, Diagnostic, Driver,
    Duration, DurationUnit, Lag, LagValue, Link, LinkType, Minutes, Moment, OutlineLevel, OverriddenBound,
    Project, ProjectInit, Resource, ResourceUid, Schedule, ScheduleAnchor, SchedulingMode, SignedPercent,
    Task, TaskOutcome, TaskRole, TaskSchedule, TaskType, TaskUid, TimeBasis, TimeOfDay, Units, Week, Work,
    percent_of, schedule,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Générateur
// ---------------------------------------------------------------------------

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
}

/// Forme des réseaux tirés.
#[derive(Clone, Copy)]
struct Shape {
    /// Plan hiérarchique possible (une fois sur deux).
    hierarchy: bool,
    /// Types de liens tirés.
    kinds: &'static [LinkType],
    /// Durées et décalages écoulés.
    elapsed: bool,
    /// Calendriers de tâche (20 % des tâches).
    task_calendars: bool,
    /// Tâches inactives (5 %).
    inactive: bool,
    /// Borne basse des décalages en %, en centièmes.
    percent_min: i64,
    /// Décalage en % depuis une récapitulative (sa durée R-054 peut
    /// diminuer quand une sous-tâche est retardée : exclu pour P16).
    percent_from_summary: bool,
    /// Options CPM tirées (seuil 0 ou 1 j, plusieurs chemins 20 %).
    cpm_options: bool,
    /// Jalons (10 % des tâches) : exclus pour P15, dont l'oracle raisonne
    /// en temps ouvré, alors qu'un jalon lit sa borne en brut (R-031, R-047).
    milestones: bool,
    /// Contraintes, échéances et option « respecter les contraintes » tirées
    /// (J3, générateur `CONSTRAINED`).
    constraints: Draw,
}

/// Contraintes tirées par le générateur (spécification J3 § 9.1) : 20 % de
/// contraintes datées, 5 % d'ALAP, 5 % d'échéances, option « respecter les
/// contraintes » inactive 30 % du temps ; dates de −3 à +40 jours autour du
/// début du projet, à la minute.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Draw {
    /// Aucune contrainte (J2).
    None,
    /// Les six types datés, ALAP et échéances.
    All,
    /// Contraintes de début seulement (MSO, SNET, SNLT), ALAP et
    /// échéances : P16 (une contrainte de fin recule le début quand la
    /// durée s'allonge, et un successeur SS avec lui).
    StartSide,
}

const ALL_KINDS: &[LinkType] = &[
    LinkType::FinishToStart,
    LinkType::FinishToStart,
    LinkType::FinishToStart,
    LinkType::StartToStart,
    LinkType::FinishToFinish,
    LinkType::StartToFinish,
];

/// P13, P17.
const GENERAL: Shape = Shape {
    hierarchy: true,
    kinds: ALL_KINDS,
    elapsed: true,
    task_calendars: true,
    inactive: true,
    percent_min: -10_000,
    percent_from_summary: true,
    cpm_options: true,
    milestones: true,
    constraints: Draw::None,
};

/// P13 (amendée), P20, P22 : comme [`GENERAL`], avec des contraintes, des
/// échéances et l'option tirées.
const CONSTRAINED: Shape = Shape {
    constraints: Draw::All,
    ..GENERAL
};

/// P14 : réseau plat, un seul calendrier ouvré, décalages ouvrés.
const FLAT: Shape = Shape {
    hierarchy: false,
    kinds: ALL_KINDS,
    elapsed: false,
    task_calendars: false,
    inactive: false,
    percent_min: -10_000,
    percent_from_summary: true,
    cpm_options: false,
    milestones: true,
    constraints: Draw::None,
};

/// P15 : comme [`FLAT`], sans jalon.
const FLAT_NO_MILESTONE: Shape = Shape {
    milestones: false,
    ..FLAT
};

/// P21 : comme [`FLAT_NO_MILESTONE`], avec des contraintes et des échéances.
const FLAT_CONSTRAINED_NO_MILESTONE: Shape = Shape {
    constraints: Draw::All,
    ..FLAT_NO_MILESTONE
};

/// P16 : liens FS et SS, pourcentages positifs ; contraintes de début,
/// ALAP, échéances et option tirées.
const MONOTONE: Shape = Shape {
    hierarchy: true,
    kinds: &[LinkType::FinishToStart, LinkType::StartToStart],
    elapsed: true,
    task_calendars: true,
    inactive: true,
    percent_min: 0,
    percent_from_summary: false,
    cpm_options: false,
    milestones: true,
    constraints: Draw::StartSide,
};

/// P16b : comme [`MONOTONE`], tous les types de contraintes (à rebours,
/// allonger une tâche ne fait que reculer son début au plus tard).
const MONOTONE_FROM_FINISH: Shape = Shape {
    constraints: Draw::All,
    ..MONOTONE
};

/// Calendrier de base aléatoire (UID 4) : chaque jour chômé une fois sur
/// quatre, sinon une à trois plages au quart d'heure ; lundi ouvré.
fn random_calendar(r: &mut Rng) -> Calendar {
    let day = |force: bool, r: &mut Rng| {
        if !force && r.chance(25) {
            return DaySetting::NonWorking;
        }
        let k = r.range(1, 3);
        let mut points: Vec<i64> = (0..2 * k).map(|_| r.range(0, 96) * 15).collect();
        points.sort_unstable();
        points.dedup();
        let ranges: Vec<TimeRange> = points
            .chunks(2)
            .filter(|c| c.len() == 2 && c[0] < c[1])
            .map(|c| {
                let t = |m: i64| TimeOfDay::new(u16::try_from(m).unwrap()).unwrap();
                TimeRange::new(t(c[0]), t(c[1])).unwrap()
            })
            .collect();
        if ranges.is_empty() {
            let t = |m: u16| TimeOfDay::new(m).unwrap();
            DaySetting::Working(DayHours::new(vec![TimeRange::new(t(480), t(720)).unwrap()]).unwrap())
        } else {
            DaySetting::Working(DayHours::new(ranges).unwrap())
        }
    };
    let week = Week::from_fn(|wd| day(wd == plannercraft_core::Weekday::Monday, r));
    Calendar::new_base(CalendarUid::new(4), "Aléatoire".to_owned(), week)
}

/// Parents (positions), recalculés à part (R-052).
fn parents(p: &Project) -> Vec<Option<usize>> {
    let mut stack: Vec<(u16, usize)> = Vec::new();
    let mut out = Vec::new();
    for (i, t) in p.tasks.iter().enumerate() {
        if t.is_null {
            out.push(None);
            continue;
        }
        let l = t.outline_level.get();
        while stack.last().is_some_and(|(sl, _)| *sl >= l) {
            stack.pop();
        }
        out.push(stack.last().map(|(_, pos)| *pos));
        stack.push((l, i));
    }
    out
}

fn is_ancestor(parents: &[Option<usize>], anc: usize, mut q: usize) -> bool {
    while let Some(par) = parents[q] {
        if par == anc {
            return true;
        }
        q = par;
    }
    false
}

fn is_summary(parents: &[Option<usize>], q: usize) -> bool {
    parents.contains(&Some(q))
}

fn generate(seed: u64, shape: Shape) -> Project {
    let mut r = Rng(seed | 1);
    let base: Moment = "2026-01-05T08:00".parse().unwrap();
    let mut offset = r.range(0, 300) * 1_440;
    if r.chance(30) {
        offset += r.range(0, 1_439);
    }
    let start = base.checked_add(Minutes::new(offset)).unwrap();
    let mut p = Project::new(ProjectInit {
        name: "Aléatoire".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    p.calendars.push(random_calendar(&mut r)).unwrap();
    p.calendar = CalendarUid::new(u32::try_from(r.range(1, 4)).unwrap());
    if shape.cpm_options {
        p.options.critical_slack_limit_days = u32::try_from(r.range(0, 1)).unwrap();
        p.options.multiple_critical_paths = r.chance(20);
    }
    let n = usize::try_from(r.range(1, 40)).unwrap();
    let hierarchical = shape.hierarchy && r.chance(50);
    let mut prev_level: i64 = 1;
    let options = p.options;
    for i in 1..=n {
        let uid = TaskUid::new(u32::try_from(i).unwrap());
        let mut t = Task::new(uid, format!("T{i}"), &options);
        let minutes = if r.chance(10) && shape.milestones {
            0
        } else {
            r.range(1, 4_800)
        };
        let basis = if shape.elapsed && r.chance(10) {
            TimeBasis::Elapsed
        } else {
            TimeBasis::Working
        };
        t.duration = Duration::new(Minutes::new(minutes), DurationUnit::Day, basis, false).unwrap();
        if hierarchical {
            let level = if i == 1 { 1 } else { r.range(1, prev_level + 1) };
            t.outline_level = OutlineLevel::new(u16::try_from(level).unwrap()).unwrap();
            prev_level = level;
        }
        if shape.task_calendars && r.chance(20) {
            t.calendar = Some(CalendarUid::new(u32::try_from(r.range(1, 4)).unwrap()));
        }
        t.active = !(shape.inactive && r.chance(5));
        p.tasks.push(t).unwrap();
    }
    // Ordre topologique caché : rang de chaque position (Fisher-Yates).
    let mut rank: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = usize::try_from(r.range(0, i64::try_from(i).unwrap())).unwrap();
        rank.swap(i, j);
    }
    let par = parents(&p);
    let uids: Vec<TaskUid> = p.tasks.iter().map(Task::uid).collect();
    for s in 0..n {
        let candidates: Vec<usize> = (0..n)
            .filter(|&q| rank[q] < rank[s] && !is_ancestor(&par, q, s) && !is_ancestor(&par, s, q))
            .collect();
        let k = r.range(0, 3);
        let mut chosen = BTreeSet::new();
        let mut links = Vec::new();
        for _ in 0..k {
            if candidates.is_empty() {
                break;
            }
            let q = candidates
                [usize::try_from(r.range(0, i64::try_from(candidates.len()).unwrap() - 1)).unwrap()];
            if !chosen.insert(q) {
                continue;
            }
            let kind = shape.kinds
                [usize::try_from(r.range(0, i64::try_from(shape.kinds.len()).unwrap() - 1)).unwrap()];
            let lag_basis = |r: &mut Rng, pct: u64| {
                if shape.elapsed && r.chance(pct) {
                    TimeBasis::Elapsed
                } else {
                    TimeBasis::Working
                }
            };
            let lag = if r.chance(50) {
                Lag::ZERO
            } else if r.chance(10) && (shape.percent_from_summary || !is_summary(&par, q)) {
                let basis = lag_basis(&mut r, 30);
                let percent = SignedPercent::new(r.range(shape.percent_min, 10_000)).unwrap();
                Lag::new(basis, LagValue::Percent { percent }).unwrap()
            } else {
                let basis = lag_basis(&mut r, 15);
                let minutes = Minutes::new(r.range(-2_000, 2_000));
                Lag::new(
                    basis,
                    LagValue::Time {
                        minutes,
                        unit: DurationUnit::Day,
                    },
                )
                .unwrap()
            };
            links.push(Link {
                predecessor: uids[q],
                kind,
                lag,
            });
        }
        p.tasks.get_mut(uids[s]).unwrap().predecessors = links;
    }
    if shape.constraints != Draw::None {
        add_constraints(&mut p, &mut r, shape.constraints, start, (-3 * 1_440, 40 * 1_440));
    }
    p
}

/// Tire contraintes, échéances et option (générateur `CONSTRAINED`) sur
/// toutes les tâches, récapitulatives et manuelles comprises ; dates à
/// `start + [lo, hi]` minutes, bornées au domaine.
fn add_constraints(p: &mut Project, r: &mut Rng, draw: Draw, start: Moment, (lo, hi): (i64, i64)) {
    p.options.honor_constraints = !r.chance(30);
    // R-108 : défaut selon le sens (ASAP depuis le début, ALAP à rebours) ;
    // la contrainte « opposée » tirée est l'autre.
    let anchor = p.anchor;
    let opposite = match anchor {
        ScheduleAnchor::FromStart(_) => Constraint::AsLateAsPossible,
        ScheduleAnchor::FromFinish(_) => Constraint::AsSoonAsPossible,
    };
    let uids: Vec<TaskUid> = p.tasks.iter().map(Task::uid).collect();
    let date = |r: &mut Rng| {
        let m = (start.minutes() + r.range(lo, hi)).clamp(Moment::MIN.minutes(), Moment::MAX.minutes());
        Moment::from_minutes(m).unwrap()
    };
    for uid in uids {
        let t = p.tasks.get_mut(uid).unwrap();
        t.constraint = Constraint::default_for(&anchor);
        if r.chance(20) {
            let d = date(r);
            let kinds: &[fn(Moment) -> Constraint] = match draw {
                Draw::StartSide => &[
                    |date| Constraint::MustStartOn { date },
                    |date| Constraint::StartNoEarlierThan { date },
                    |date| Constraint::StartNoLaterThan { date },
                ],
                Draw::All | Draw::None => &[
                    |date| Constraint::MustStartOn { date },
                    |date| Constraint::MustFinishOn { date },
                    |date| Constraint::StartNoEarlierThan { date },
                    |date| Constraint::StartNoLaterThan { date },
                    |date| Constraint::FinishNoEarlierThan { date },
                    |date| Constraint::FinishNoLaterThan { date },
                ],
            };
            let k = usize::try_from(r.range(0, i64::try_from(kinds.len()).unwrap() - 1)).unwrap();
            t.constraint = kinds[k](d);
        } else if r.chance(6) {
            t.constraint = opposite;
        }
        if r.chance(5) {
            t.deadline = Some(date(r));
        }
    }
}

/// Réseau chaotique : tout ce qui peut mal tourner ; `from_finish` force la
/// planification à rebours (sinon tirée 5 % du temps : le tirage est fait
/// dans les deux cas, les graines de régression gardent leur réseau).
fn chaos(seed: u64, from_finish: bool) -> Project {
    let mut p = generate(seed, GENERAL);
    let mut r = Rng(seed.rotate_left(17) | 1);
    let start = match r.range(0, 3) {
        0 => "2149-12-20T08:00",
        1 => "1984-01-02T08:00",
        _ => "2026-10-19T08:00",
    };
    let start: Moment = start.parse().unwrap();
    p.anchor = if r.chance(5) || from_finish {
        ScheduleAnchor::FromFinish(start)
    } else {
        ScheduleAnchor::FromStart(start)
    };
    if r.chance(10) {
        let empty = Calendar::new_base(
            CalendarUid::new(9),
            "Vide".to_owned(),
            Week::from_fn(|_| DaySetting::NonWorking),
        );
        p.calendars.push(empty).unwrap();
        p.calendar = CalendarUid::new(9);
    }
    let uids: Vec<TaskUid> = p.tasks.iter().map(Task::uid).collect();
    let n = i64::try_from(uids.len()).unwrap();
    for uid in uids {
        let t = p.tasks.get_mut(uid).unwrap();
        if r.chance(5) {
            t.is_null = true;
        }
        if r.chance(5) {
            t.calendar = Some(CalendarUid::new(u32::try_from(r.range(5, 9)).unwrap()));
        }
        // Saut de niveau.
        if t.outline_level.get() > 1 && r.chance(10) {
            t.outline_level = OutlineLevel::new(t.outline_level.get() + 2).unwrap();
        }
        if r.chance(10) {
            let entered_start = start.checked_add(Minutes::new(r.range(-20_000, 20_000))).ok();
            let entered_finish = start.checked_add(Minutes::new(r.range(-20_000, 20_000))).ok();
            t.mode = SchedulingMode::Manual {
                start: if r.chance(70) { entered_start } else { None },
                finish: if r.chance(70) { entered_finish } else { None },
                duration: None,
            };
        }
        // Prédécesseur inconnu, lien en double, cycle.
        if r.chance(10) {
            t.predecessors.push(Link {
                predecessor: TaskUid::new(u32::try_from(n + r.range(1, 5)).unwrap()),
                kind: LinkType::FinishToStart,
                lag: Lag::ZERO,
            });
        }
        if r.chance(10) {
            t.predecessors.push(Link {
                predecessor: TaskUid::new(u32::try_from(r.range(1, n)).unwrap()),
                kind: LinkType::FinishToStart,
                lag: Lag::ZERO,
            });
        }
        for l in &mut t.predecessors {
            if r.chance(10) {
                let minutes = Minutes::new(r.range(-87_308_640, 87_308_640));
                l.lag = Lag::new(
                    TimeBasis::Working,
                    LagValue::Time {
                        minutes,
                        unit: DurationUnit::Day,
                    },
                )
                .unwrap();
            }
        }
    }
    // Contraintes partout (récapitulatives, manuelles comprises) et dates
    // aux bornes du domaine (J3).
    add_constraints(&mut p, &mut r, Draw::All, start, (-30_000, 30_000));
    for uid in p.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
        let t = p.tasks.get_mut(uid).unwrap();
        if r.chance(5) {
            let date = if r.chance(50) { Moment::MIN } else { Moment::MAX };
            t.constraint = if r.chance(50) {
                Constraint::MustFinishOn { date }
            } else {
                Constraint::StartNoEarlierThan { date }
            };
        }
        if r.chance(3) {
            t.deadline = Some(if r.chance(50) { Moment::MIN } else { Moment::MAX });
        }
    }
    p
}

/// Réseau planifié à rebours (R-106) : comme [`generate`], ancre `F` de 5
/// à 60 jours après le début tiré, contraintes tirées autour de `F` (de −40
/// à +3 jours), ALAP par défaut et ASAP tirée 5 % du temps (R-108).
fn generate_from_finish(seed: u64, shape: Shape) -> Project {
    let mut p = generate(
        seed,
        Shape {
            constraints: Draw::None,
            ..shape
        },
    );
    let mut r = Rng(seed.rotate_left(41) | 1);
    let ScheduleAnchor::FromStart(start) = p.anchor else {
        panic!("ancre")
    };
    let mut offset = r.range(5, 60) * 1_440;
    if r.chance(30) {
        offset += r.range(0, 1_439);
    }
    let finish = start.checked_add(Minutes::new(offset)).unwrap();
    p.anchor = ScheduleAnchor::FromFinish(finish);
    if shape.constraints == Draw::None {
        for uid in p.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
            p.tasks.get_mut(uid).unwrap().constraint = Constraint::AsLateAsPossible;
        }
    } else {
        add_constraints(
            &mut p,
            &mut r,
            shape.constraints,
            finish,
            (-40 * 1_440, 3 * 1_440),
        );
    }
    p
}

// ---------------------------------------------------------------------------
// Contrôles communs
// ---------------------------------------------------------------------------

/// Calendriers compilés, avec les replis de R-066.
struct Cals {
    set: CalendarSet,
}

impl Cals {
    fn new(p: &Project) -> Self {
        Self {
            set: CalendarSet::compile(p).0,
        }
    }

    fn usable(&self, uid: CalendarUid) -> Option<&CompiledCalendar> {
        self.set.get(uid).ok().filter(|c| c.has_working_time())
    }

    /// Calendrier ouvré effectif de la tâche (R-066) ; `None` si le
    /// calendrier du projet est inutilisable.
    fn of(&self, p: &Project, t: &Task) -> Option<&CompiledCalendar> {
        let project = self.usable(p.calendar)?;
        Some(t.calendar.and_then(|c| self.usable(c)).unwrap_or(project))
    }

    /// Échelle de placement de la tâche.
    fn place(&self, p: &Project, t: &Task) -> Option<TimeScale<'_>> {
        if t.duration.basis() == TimeBasis::Elapsed {
            Some(TimeScale::Elapsed)
        } else {
            self.of(p, t).map(TimeScale::Working)
        }
    }
}

/// Tâche planifiée et non dégradée.
fn planned(s: &Schedule, uid: TaskUid) -> Option<TaskSchedule> {
    match s.get(uid) {
        Some(TaskOutcome::Scheduled(ts)) if !ts.degraded => Some(*ts),
        _ => None,
    }
}

/// Une entrée par tâche ; diagnostics triés et sans doublon ; déterminisme.
fn check_shape(p: &Project, s: &Schedule) {
    assert_eq!(s.tasks.len(), p.tasks.len());
    let mut sorted = s.diagnostics.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted, s.diagnostics);
    assert_eq!(*s, schedule(&p.clone()));
}

/// Règle d'une contrainte de feuille automatique active depuis le début
/// (table de la spécification J3 § 3.5, recopiée).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    None,
    Opposite,
    Floor,
    Cap,
    Window,
    Clamped,
    Pin,
}

fn rule(kind: ConstraintKind, honor: bool) -> Rule {
    match kind {
        ConstraintKind::AsSoonAsPossible => Rule::None,
        ConstraintKind::AsLateAsPossible => Rule::Opposite,
        ConstraintKind::StartNoEarlierThan | ConstraintKind::FinishNoEarlierThan => Rule::Floor,
        ConstraintKind::StartNoLaterThan | ConstraintKind::FinishNoLaterThan if honor => Rule::Clamped,
        ConstraintKind::StartNoLaterThan | ConstraintKind::FinishNoLaterThan => Rule::Cap,
        ConstraintKind::MustStartOn | ConstraintKind::MustFinishOn if honor => Rule::Pin,
        ConstraintKind::MustStartOn | ConstraintKind::MustFinishOn => Rule::Window,
    }
}

/// Positions des tâches épinglées par MSO ou MFO (option active) : feuille
/// automatique active.
fn pinned_tasks(p: &Project) -> BTreeSet<TaskUid> {
    let par = parents(p);
    p.tasks
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            !is_summary(&par, *i)
                && !t.is_null
                && t.active
                && t.mode == SchedulingMode::Auto
                && rule(t.constraint.kind(), p.options.honor_constraints) == Rule::Pin
        })
        .map(|(_, t)| t.uid())
        .collect()
}

/// Fermeture amont des plafonds (P13 b') : tâches dont les dates au plus
/// tard peuvent dépendre d'un plafond (SNLT, FNLT, MSO, MFO, échéance), par
/// les liens (prédécesseur → successeur) et la hiérarchie (dans les deux
/// sens). Hors de cette fermeture, la marge totale est ≥ 0 comme en J2.
fn cap_closure(p: &Project) -> BTreeSet<TaskUid> {
    let n = p.tasks.len();
    let par = parents(p);
    // Arêtes inversées : de la tâche plafonnée vers celles qui en dépendent.
    let mut rev: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (si, t) in p.tasks.iter().enumerate() {
        for l in &t.predecessors {
            if let Some(pi) = p.tasks.position(l.predecessor) {
                rev[si].push(pi);
            }
        }
        if let Some(pp) = par[si] {
            rev[si].push(pp);
            rev[pp].push(si);
        }
    }
    let capped = |t: &Task| {
        t.deadline.is_some()
            || matches!(
                t.constraint.kind(),
                ConstraintKind::StartNoLaterThan
                    | ConstraintKind::FinishNoLaterThan
                    | ConstraintKind::MustStartOn
                    | ConstraintKind::MustFinishOn
            )
    };
    let mut seen = vec![false; n];
    let mut stack: Vec<usize> = (0..n).filter(|&i| capped(p.tasks.at(i).unwrap())).collect();
    for &i in &stack {
        seen[i] = true;
    }
    while let Some(x) = stack.pop() {
        for &y in &rev[x] {
            if !seen[y] {
                seen[y] = true;
                stack.push(y);
            }
        }
    }
    (0..n)
        .filter(|&i| seen[i])
        .map(|i| p.tasks.at(i).unwrap().uid())
        .collect()
}

/// Fermeture aval des planchers à rebours (P13 b', R-109) : tâches dont les
/// dates au plus tôt peuvent dépasser leurs dates au plus tard à cause d'un
/// plancher ou d'une épingle (SNET, FNET, MSO, MFO), par les liens
/// (prédécesseur → successeur) et la hiérarchie (dans les deux sens). Hors
/// de cette fermeture, la marge totale est ≥ 0.
fn floor_closure(p: &Project) -> BTreeSet<TaskUid> {
    let n = p.tasks.len();
    let par = parents(p);
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (si, t) in p.tasks.iter().enumerate() {
        for l in &t.predecessors {
            if let Some(pi) = p.tasks.position(l.predecessor) {
                next[pi].push(si);
            }
        }
        if let Some(pp) = par[si] {
            next[si].push(pp);
            next[pp].push(si);
        }
    }
    let floored = |t: &Task| {
        matches!(
            t.constraint.kind(),
            ConstraintKind::StartNoEarlierThan
                | ConstraintKind::FinishNoEarlierThan
                | ConstraintKind::MustStartOn
                | ConstraintKind::MustFinishOn
        )
    };
    let mut seen = vec![false; n];
    let mut stack: Vec<usize> = (0..n).filter(|&i| floored(p.tasks.at(i).unwrap())).collect();
    for &i in &stack {
        seen[i] = true;
    }
    while let Some(x) = stack.pop() {
        for &y in &next[x] {
            if !seen[y] {
                seen[y] = true;
                stack.push(y);
            }
        }
    }
    (0..n)
        .filter(|&i| seen[i])
        .map(|i| p.tasks.at(i).unwrap().uid())
        .collect()
}

/// (a'), (b'), (c) et (f) sur les tâches planifiées non dégradées hors de
/// `skip`. (a') : un lien violé par les dates planifiées doit être nommé par
/// un `constraint.override` (lien ou porte de la récapitulative).
fn check_network(p: &Project, s: &Schedule, skip: &BTreeSet<TaskUid>, check_order: bool) {
    let cals = Cals::new(p);
    let par = parents(p);
    let from_finish = matches!(p.anchor, ScheduleAnchor::FromFinish(_));
    // Marge négative possible : en amont des plafonds depuis le début, en
    // aval des planchers à rebours.
    let closure = if from_finish {
        floor_closure(p)
    } else {
        cap_closure(p)
    };
    let pins = pinned_tasks(p);
    let mut overridden: BTreeSet<(TaskUid, TaskUid)> = BTreeSet::new();
    let mut gate_overridden: BTreeSet<TaskUid> = BTreeSet::new();
    for d in &s.diagnostics {
        if let Diagnostic::ConstraintOverride { task, bound, .. } = d {
            match bound {
                OverriddenBound::Predecessor { predecessor, .. } => {
                    overridden.insert((*predecessor, *task));
                }
                OverriddenBound::SummaryGate { .. } => {
                    gate_overridden.insert(*task);
                }
                // À rebours : la tâche forcée viole le lien vers son
                // successeur (R-109).
                OverriddenBound::Successor { successor, .. } => {
                    overridden.insert((*task, *successor));
                }
                _ => {}
            }
        }
    }
    let cut: BTreeSet<(TaskUid, TaskUid)> = s
        .diagnostics
        .iter()
        .filter_map(|d| match d {
            Diagnostic::LinkIgnoredInCycle {
                predecessor,
                successor,
            } => Some((*predecessor, *successor)),
            _ => None,
        })
        .collect();
    let ok = |uid: TaskUid| planned(s, uid).filter(|_| !skip.contains(&uid));
    let order_pos: BTreeMap<TaskUid, usize> = s.order.iter().enumerate().map(|(i, u)| (*u, i)).collect();

    // (b), (c).
    for (i, t) in p.tasks.iter().enumerate() {
        let Some(ts) = ok(t.uid()) else { continue };
        assert!(ts.early.start <= ts.early.finish, "{}: ES > EF", t.uid());
        assert!(ts.late.start <= ts.late.finish, "{}: LS > LF", t.uid());
        // (b') FS ≥ 0 toujours ; TF ≥ FS quand TF ≥ 0 ; hors de la fermeture
        // amont des plafonds, TF ≥ 0 (donc TF ≥ FS ≥ 0) comme en J2.
        assert!(ts.free_slack >= Minutes::ZERO, "{}: FS < 0 {ts:?}", t.uid());
        if ts.total_slack >= Minutes::ZERO {
            assert!(ts.total_slack >= ts.free_slack, "{}: TF < FS {ts:?}", t.uid());
        }
        if !closure.contains(&t.uid()) {
            assert!(
                ts.total_slack >= Minutes::ZERO,
                "{}: TF < 0 hors plafond {ts:?}",
                t.uid()
            );
        }
        // (c) Dates planifiées = au plus tôt depuis le début, au plus tard à
        // rebours (R-106, R-112) ; une épingle a `early == late`.
        let want = if from_finish { ts.late } else { ts.early };
        assert_eq!((ts.start, ts.finish), (want.start, want.finish), "{}", t.uid());
        if pins.contains(&t.uid()) {
            assert_eq!(ts.early, ts.late, "{}: épingle", t.uid());
        }
        if ts.role == TaskRole::Summary && !ts.manual {
            // Enveloppes des enfants : au plus tôt, et planifiée (R-112).
            let mut span: Option<(Moment, Moment)> = None;
            let mut planned: Option<(Moment, Moment)> = None;
            for (q, c) in p.tasks.iter().enumerate() {
                if par[q] != Some(i) {
                    continue;
                }
                if let Some(TaskOutcome::Scheduled(cs)) = s.get(c.uid()) {
                    span = Some(span.map_or((cs.early.start, cs.early.finish), |(a, b)| {
                        (a.min(cs.early.start), b.max(cs.early.finish))
                    }));
                    planned = Some(planned.map_or((cs.start, cs.finish), |(a, b)| {
                        (a.min(cs.start), b.max(cs.finish))
                    }));
                }
            }
            assert_eq!(span, Some((ts.early.start, ts.early.finish)), "{}", t.uid());
            assert_eq!(planned, Some((ts.start, ts.finish)), "{}", t.uid());
        }
    }
    // (c) Récapitulative du projet : enveloppe des tâches de premier niveau.
    let mut top: Option<(Moment, Moment)> = None;
    let mut top_planned: Option<(Moment, Moment)> = None;
    for (q, t) in p.tasks.iter().enumerate() {
        if par[q].is_some() {
            continue;
        }
        if let Some(TaskOutcome::Scheduled(ts)) = s.get(t.uid()) {
            top = Some(top.map_or((ts.early.start, ts.early.finish), |(a, b)| {
                (a.min(ts.early.start), b.max(ts.early.finish))
            }));
            top_planned = Some(top_planned.map_or((ts.start, ts.finish), |(a, b)| {
                (a.min(ts.start), b.max(ts.finish))
            }));
        }
    }
    assert_eq!(top, s.project_summary.map(|ps| (ps.early.start, ps.early.finish)));
    assert_eq!(top_planned, s.project_summary.map(|ps| (ps.start, ps.finish)));

    // (a), (f) : liens effectifs.
    for (si, st) in p.tasks.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for l in &st.predecessors {
            if !seen.insert(l.predecessor) || l.predecessor == st.uid() {
                continue;
            }
            let Some(pi) = p.tasks.position(l.predecessor) else {
                continue;
            };
            let pt = p.tasks.at(pi).unwrap();
            if cut.contains(&(pt.uid(), st.uid()))
                || is_ancestor(&par, pi, si)
                || is_ancestor(&par, si, pi)
                || !pt.active
                || pt.is_null
                || st.is_null
                || is_summary(&par, pi)
            {
                continue;
            }
            let Some(pe) = ok(pt.uid()) else { continue };
            let lag = match l.lag.value {
                LagValue::Time { minutes, .. } => minutes,
                LagValue::Percent { percent } => percent_of(pe.duration, percent).unwrap(),
            };
            let sigma = if l.lag.basis == TimeBasis::Elapsed {
                TimeScale::Elapsed
            } else {
                let Some(c) = cals.of(p, st) else { continue };
                TimeScale::Working(c)
            };
            // Dates planifiées (au plus tôt depuis le début, au plus tard à
            // rebours).
            let anchor = match l.kind {
                LinkType::FinishToStart | LinkType::FinishToFinish => pe.finish,
                LinkType::StartToStart | LinkType::StartToFinish => pe.start,
            };
            let Ok(bound) = (if lag == Minutes::ZERO {
                Ok(anchor)
            } else {
                sigma.add(anchor, lag)
            }) else {
                continue;
            };
            // Successeur feuille, ou sous-tâches d'une récapitulative (FS/SS
            // par la porte, FF/SF dépliés).
            let targets: Vec<usize> = if is_summary(&par, si) {
                (0..p.tasks.len())
                    .filter(|q| is_ancestor(&par, si, *q) && !is_summary(&par, *q))
                    .collect()
            } else {
                vec![si]
            };
            // À rebours, une tâche manuelle signale ses liens sortants violés
            // (`link.manual_not_honored`, R-106).
            if from_finish && matches!(pt.mode, SchedulingMode::Manual { .. }) {
                continue;
            }
            for q in targets {
                let qt = p.tasks.at(q).unwrap();
                if matches!(qt.mode, SchedulingMode::Manual { .. }) {
                    continue;
                }
                let Some(qe) = ok(qt.uid()) else { continue };
                let Some(place) = cals.place(p, qt) else { continue };
                // (a') Lien violé par une contrainte respectée de force :
                // signalé, pas vérifié.
                // À rebours, `Successor` nomme le successeur d'origine, qui peut
                // être une récapitulative.
                if overridden.contains(&(pt.uid(), qt.uid()))
                    || overridden.contains(&(pt.uid(), st.uid()))
                    || (qt.uid() != st.uid() && gate_overridden.contains(&qt.uid()))
                {
                    continue;
                }
                let w = |a: Moment, b: Moment| place.between(a, b).unwrap_or(Minutes::ZERO);
                match l.kind {
                    LinkType::FinishToStart => assert!(
                        qe.start >= bound,
                        "FS {} → {}: {:?} < {bound}",
                        pt.uid(),
                        qt.uid(),
                        qe.start
                    ),
                    LinkType::StartToStart => {
                        assert!(
                            w(bound, qe.start) >= Minutes::ZERO,
                            "SS {} → {}",
                            pt.uid(),
                            qt.uid()
                        );
                    }
                    LinkType::FinishToFinish | LinkType::StartToFinish => assert!(
                        w(bound, qe.finish) >= Minutes::ZERO,
                        "FF/SF {} → {}",
                        pt.uid(),
                        qt.uid()
                    ),
                }
                if check_order
                    && qt.uid() == st.uid()
                    && let (Some(a), Some(b)) = (order_pos.get(&pt.uid()), order_pos.get(&qt.uid()))
                {
                    assert!(a < b, "ordre : {} avant {}", pt.uid(), qt.uid());
                }
            }
        }
    }
}

/// Planning sans les pilotes (permutation des liens, P13 d).
fn without_drivers(s: &Schedule) -> Schedule {
    let mut s = s.clone();
    for o in s.tasks.values_mut() {
        if let TaskOutcome::Scheduled(ts) = o {
            ts.driver = Driver::ProjectStart;
        }
    }
    s
}

// ---------------------------------------------------------------------------
// P13
// ---------------------------------------------------------------------------

fn p13(p: &Project) {
    let s = schedule(p);
    check_shape(p, &s);
    // (d) Aller-retour JSON du projet et du planning.
    let pj = serde_json::to_string(p).unwrap();
    let p2: Project = serde_json::from_str(&pj).unwrap();
    assert_eq!(schedule(&p2), s);
    let sj = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<Schedule>(&sj).unwrap(), s);
    // (d) Permutation des liens de chaque tâche : seul le pilote change.
    let mut permuted = p.clone();
    let uids: Vec<TaskUid> = permuted.tasks.iter().map(Task::uid).collect();
    for uid in &uids {
        permuted.tasks.get_mut(*uid).unwrap().predecessors.reverse();
    }
    assert_eq!(without_drivers(&schedule(&permuted)), without_drivers(&s));
    // (a), (b), (c), (f).
    check_network(p, &s, &BTreeSet::new(), true);
    // (e) Point fixe : feuilles actives automatiques non contraintes (ASAP)
    // → manuelles à leurs dates au plus tôt ; les feuilles contraintes
    // restent automatiques (une manuelle ignore sa contrainte, R-104). Pas
    // de point fixe exact après une épingle ALAP qui déplace une tâche : la
    // repasse fige `K(S)` et R-051 sur les dates de la passe principale
    // (R-092), que les dates rejouées ne reproduisent pas.
    let alap_moved = s.tasks.values().any(|o| {
        matches!(
            o,
            TaskOutcome::Scheduled(ts) if ts.driver == Driver::Constraint {
                kind: ConstraintKind::AsLateAsPossible
            }
        )
    });
    let mut fixed = p.clone();
    for uid in uids {
        let t = fixed.tasks.get_mut(uid).unwrap();
        if let Some(TaskOutcome::Scheduled(ts)) = s.get(t.uid())
            && ts.role == TaskRole::Leaf
            && t.active
            && t.constraint == Constraint::AsSoonAsPossible
        {
            t.mode = SchedulingMode::Manual {
                start: Some(ts.early.start),
                finish: Some(ts.early.finish),
                duration: None,
            };
        }
    }
    let f = schedule(&fixed);
    for (uid, o) in s.tasks.iter().filter(|_| !alap_moved) {
        if let (TaskOutcome::Scheduled(a), Some(TaskOutcome::Scheduled(b))) = (o, f.tasks.get(uid)) {
            assert_eq!(a.early, b.early, "point fixe {uid}");
            assert_eq!(a.late, b.late, "point fixe {uid}");
            assert_eq!(a.total_slack, b.total_slack, "point fixe {uid}");
            assert_eq!(a.free_slack, b.free_slack, "point fixe {uid}");
        }
    }
    assert!(
        alap_moved
            || !f
                .diagnostics
                .iter()
                .any(|d| d.code() == "link.manual_not_honored"),
        "point fixe : {:?}",
        f.diagnostics
    );
    // (g) Dates au plus tard réalisables (S16, S17, R-047) : feuilles
    // actives placées à leurs dates au plus tard, aucun lien violé et fin du
    // projet inchangée.
    if s.tasks
        .values()
        .any(|o| matches!(o, TaskOutcome::Scheduled(ts) if ts.degraded))
    {
        return;
    }
    let mut late = p.clone();
    for uid in late.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
        let t = late.tasks.get_mut(uid).unwrap();
        if let Some(TaskOutcome::Scheduled(ts)) = s.get(uid)
            && ts.role == TaskRole::Leaf
            && t.active
        {
            t.mode = SchedulingMode::Manual {
                start: Some(ts.late.start),
                finish: Some(ts.late.finish),
                duration: None,
            };
        }
    }
    let l = schedule(&late);
    // Décalage en % depuis une récapitulative : sa durée R-054 change quand
    // ses sous-tâches sont déplacées, le décalage aussi (comme pour P16).
    let par = parents(p);
    let summary_percent: BTreeSet<(TaskUid, TaskUid)> = p
        .tasks
        .iter()
        .flat_map(|t| t.predecessors.iter().map(move |lk| (lk, t.uid())))
        .filter(|(lk, _)| matches!(lk.lag.value, LagValue::Percent { .. }))
        .filter(|(lk, _)| {
            p.tasks
                .position(lk.predecessor)
                .is_some_and(|q| is_summary(&par, q))
        })
        .map(|(lk, s)| (lk.predecessor, s))
        .collect();
    // Une épingle (MSO, MFO) n'est pas plafonnée par ses successeurs ni par
    // les plafonds hérités de ses récapitulatives : seuls ses liens sortants
    // et ceux de ses récapitulatives ancêtres peuvent être violés par les
    // dates au plus tard.
    let mut pins = pinned_tasks(p);
    for (i, t) in p.tasks.iter().enumerate() {
        if pins.contains(&t.uid()) {
            let mut q = i;
            while let Some(a) = par[q] {
                pins.insert(p.tasks.at(a).unwrap().uid());
                q = a;
            }
        }
    }
    let broken: Vec<&Diagnostic> = l
        .diagnostics
        .iter()
        .filter(|d| match d {
            Diagnostic::LinkManualNotHonored {
                predecessor,
                successor,
            } => !summary_percent.contains(&(*predecessor, *successor)) && !pins.contains(predecessor),
            _ => false,
        })
        .collect();
    assert!(broken.is_empty(), "dates au plus tard : {broken:?}");
    // Fin du projet : jamais décalée ; égale sans plafond (un plafond peut
    // ramener toutes les fins au plus tard avant elle).
    assert!(
        l.project_finish <= s.project_finish,
        "dates au plus tard : fin du projet"
    );
    // Égalité en temps ouvré du calendrier du projet : la fin au plus tôt
    // d'un FNET peut être le début d'une plage (R-090, O14), la fin au plus
    // tard le dernier instant ouvré avant elle (R-030), même temps ouvré.
    if cap_closure(p).is_empty()
        && let Some(cal) = Cals::new(p).usable(p.calendar)
    {
        assert_eq!(
            cal.working_between(l.project_finish, s.project_finish).unwrap(),
            Minutes::ZERO,
            "dates au plus tard : fin du projet"
        );
    }
    // (h) Retards sans effet (S16, S17) : une feuille retardée de sa marge
    // totale ne décale pas la fin du projet ; retardée de sa marge libre,
    // elle ne décale aucune autre feuille. Échantillon : la plus grande marge
    // de chaque sorte, et une feuille tirée par la taille du réseau.
    // Même limite qu'en (e) : après une épingle ALAP qui déplace une tâche,
    // retarder un prédécesseur dans sa marge déplace la date au plus tôt de
    // la passe principale de l'ALAP, donc l'application de `K(S)` (R-057) à
    // ses dates au plus tard, figée sur la passe principale (R-092).
    if summary_percent.is_empty() && !alap_moved {
        delays(p, &s, &par);
    }
}

/// P13 (h) : retard d'une feuille de sa marge totale, puis de sa marge libre.
fn delays(p: &Project, s: &Schedule, par: &[Option<usize>]) {
    let cals = Cals::new(p);
    for total in [true, false] {
        let slack = |ts: &TaskSchedule| if total { ts.total_slack } else { ts.free_slack };
        let eligible: Vec<(usize, TaskSchedule)> = p
            .tasks
            .iter()
            .enumerate()
            .filter_map(|(i, t)| match s.get(t.uid()) {
                // Feuilles non contraintes seulement : la feuille retardée
                // devient manuelle et perdrait sa contrainte (R-104), ce qui
                // changerait les dates au plus tard de ses prédécesseurs (et
                // l'épingle d'une ALAP).
                Some(TaskOutcome::Scheduled(ts))
                    if ts.role == TaskRole::Leaf
                        && t.active
                        && !ts.manual
                        && t.constraint == Constraint::AsSoonAsPossible
                        && slack(ts) > Minutes::ZERO =>
                {
                    Some((i, *ts))
                }
                _ => None,
            })
            .collect();
        let Some(&largest) = eligible
            .iter()
            .max_by_key(|(i, ts)| (slack(ts), std::cmp::Reverse(*i)))
        else {
            continue;
        };
        let other = eligible[p.tasks.len() % eligible.len()];
        for (i, ts) in [largest, other] {
            let t = p.tasks.at(i).unwrap();
            let m = cals.place(p, t).unwrap();
            let span = m
                .forward_span(m.add(ts.early.start, slack(&ts)).unwrap(), ts.duration)
                .unwrap();
            let mut q = p.clone();
            q.tasks.get_mut(t.uid()).unwrap().mode = SchedulingMode::Manual {
                start: Some(span.start),
                finish: Some(span.finish),
                duration: None,
            };
            let r = schedule(&q);
            // Comparaisons en temps ouvré : une fin au plus tôt posée par un
            // plancher de fin (FNET, MFO en fenêtre) est le début d'une plage
            // (R-090, O14) ; placée par ses bornes, la même tâche finit à la
            // fin de la plage précédente, même temps ouvré (R-030).
            if total {
                let project = cals.usable(p.calendar).map(TimeScale::Working);
                let same = project.map_or(r.project_finish == s.project_finish, |c| {
                    c.between(r.project_finish, s.project_finish).unwrap() == Minutes::ZERO
                });
                assert!(same, "retard de TF de {}", t.uid());
                continue;
            }
            for (k, o) in p.tasks.iter().enumerate() {
                if k == i || is_summary(par, k) {
                    continue;
                }
                if let (Some(TaskOutcome::Scheduled(a)), Some(TaskOutcome::Scheduled(b))) =
                    (s.get(o.uid()), r.get(o.uid()))
                {
                    let place = cals.place(p, o).unwrap();
                    let w = |x: Moment, y: Moment| place.between(x, y).unwrap();
                    assert!(
                        w(a.early.start, b.early.start) == Minutes::ZERO
                            && w(a.early.finish, b.early.finish) == Minutes::ZERO,
                        "retard de FS de {} : {} déplacée {:?} → {:?}",
                        t.uid(),
                        o.uid(),
                        a.early,
                        b.early
                    );
                }
            }
        }
    }
}

/// P13 à rebours (R-106 à R-111) : forme, (d) déterminisme et permutation
/// des liens (pilotes et causes exceptés), (a'), (b'), (c), (f) ; (e')
/// point fixe des dates au plus tard : feuilles automatiques ALAP (défaut)
/// passées en manuelles à leurs dates au plus tard, dates au plus tard et
/// début calculé inchangés ; (g') dates au plus tôt réalisables : feuilles
/// placées à leurs dates au plus tôt, aucun lien violé (sauf vers une
/// épingle) et début calculé jamais avancé (inchangé sans plancher).
fn p13_from_finish(p: &Project) {
    let s = schedule(p);
    check_shape(p, &s);
    let pj = serde_json::to_string(p).unwrap();
    let p2: Project = serde_json::from_str(&pj).unwrap();
    assert_eq!(schedule(&p2), s);
    let mut permuted = p.clone();
    let uids: Vec<TaskUid> = permuted.tasks.iter().map(Task::uid).collect();
    for uid in &uids {
        permuted.tasks.get_mut(*uid).unwrap().predecessors.reverse();
    }
    let causes = |s: &Schedule| {
        let mut s = without_drivers(s);
        for o in s.tasks.values_mut() {
            if let TaskOutcome::Scheduled(ts) = o {
                ts.late_driver = plannercraft_core::LateDriver::ProjectFinish;
            }
        }
        s
    };
    assert_eq!(causes(&schedule(&permuted)), causes(&s));
    check_network(p, &s, &BTreeSet::new(), true);
    if s.tasks
        .values()
        .any(|o| matches!(o, TaskOutcome::Scheduled(ts) if ts.degraded))
    {
        return;
    }
    // (e') Point fixe des dates au plus tard.
    let mut fixed = p.clone();
    for uid in &uids {
        let t = fixed.tasks.get_mut(*uid).unwrap();
        if let Some(TaskOutcome::Scheduled(ts)) = s.get(t.uid())
            && ts.role == TaskRole::Leaf
            && t.active
            && t.constraint == Constraint::AsLateAsPossible
        {
            t.mode = SchedulingMode::Manual {
                start: Some(ts.late.start),
                finish: Some(ts.late.finish),
                duration: None,
            };
        }
    }
    let f = schedule(&fixed);
    // L'épingle d'une tâche ASAP (R-108) suit ses dates au plus tôt, qui
    // changent quand ses prédécesseurs deviennent manuels (une manuelle a
    // ses dates saisies au plus tôt comme au plus tard) : pas de point fixe
    // exact s'il y en a une.
    let asap = p
        .tasks
        .iter()
        .any(|t| t.active && t.constraint == Constraint::AsSoonAsPossible);
    if !asap {
        for (uid, o) in &s.tasks {
            if let (TaskOutcome::Scheduled(a), Some(TaskOutcome::Scheduled(b))) = (o, f.tasks.get(uid)) {
                assert_eq!(a.late, b.late, "point fixe {uid}");
            }
        }
        assert_eq!(f.project_start, s.project_start, "point fixe : début calculé");
    }
    // (g') Feuilles à leurs dates au plus tôt.
    let par = parents(p);
    let mut pins = pinned_tasks(p);
    for (i, t) in p.tasks.iter().enumerate() {
        if pins.contains(&t.uid()) {
            let mut q = i;
            while let Some(a) = par[q] {
                pins.insert(p.tasks.at(a).unwrap().uid());
                q = a;
            }
        }
    }
    let mut early = p.clone();
    for uid in &uids {
        let t = early.tasks.get_mut(*uid).unwrap();
        if let Some(TaskOutcome::Scheduled(ts)) = s.get(*uid)
            && ts.role == TaskRole::Leaf
            && t.active
        {
            t.mode = SchedulingMode::Manual {
                start: Some(ts.early.start),
                finish: Some(ts.early.finish),
                duration: None,
            };
        }
    }
    let e = schedule(&early);
    let broken: Vec<&Diagnostic> = e
        .diagnostics
        .iter()
        .filter(
            |d| matches!(d, Diagnostic::LinkManualNotHonored { successor, .. } if !pins.contains(successor)),
        )
        .collect();
    assert!(broken.is_empty(), "dates au plus tôt : {broken:?}");
    assert!(
        e.project_start >= s.project_start,
        "dates au plus tôt : début calculé"
    );
    if floor_closure(p).is_empty() {
        assert_eq!(
            e.project_start, s.project_start,
            "dates au plus tôt : début calculé"
        );
    }
}

/// P16b : à rebours, allonger une feuille ne retarde jamais le début calculé
/// `S*` (R-107).
fn p16b(p: &Project, seed: u64) {
    let s = schedule(p);
    let par = parents(p);
    let mut r = Rng(seed.rotate_left(29) | 1);
    let leaves: Vec<usize> = (0..p.tasks.len()).filter(|&q| !is_summary(&par, q)).collect();
    let q = leaves[usize::try_from(r.range(0, i64::try_from(leaves.len()).unwrap() - 1)).unwrap()];
    let mut longer = p.clone();
    let uid = p.tasks.at(q).unwrap().uid();
    let t = longer.tasks.get_mut(uid).unwrap();
    let d = t.duration;
    t.duration = Duration::new(
        Minutes::new(d.minutes().get() + r.range(1, 2_400)),
        d.unit(),
        d.basis(),
        d.is_estimated(),
    )
    .unwrap();
    let s2 = schedule(&longer);
    assert!(
        s2.project_start <= s.project_start,
        "allonger {uid} retarde le début calculé : {} > {}",
        s2.project_start,
        s.project_start
    );
}

/// Réseau de P23 : forme `SYMMETRIC`, décalages en % convertis en minutes
/// (durée du prédécesseur), sans échéance ; option « plusieurs chemins
/// critiques » tirée.
fn symmetric(seed: u64) -> Project {
    let mut p = generate(seed, FLAT_CONSTRAINED_NO_MILESTONE);
    let mut r = Rng(seed.rotate_left(7) | 1);
    p.options.multiple_critical_paths = r.chance(30);
    let durations: BTreeMap<TaskUid, Minutes> =
        p.tasks.iter().map(|t| (t.uid(), t.duration.minutes())).collect();
    for uid in p.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
        let t = p.tasks.get_mut(uid).unwrap();
        t.deadline = None;
        for l in &mut t.predecessors {
            if let LagValue::Percent { percent } = l.lag.value {
                let minutes = percent_of(durations[&l.predecessor], percent).unwrap();
                l.lag = Lag::new(
                    l.lag.basis,
                    LagValue::Time {
                        minutes,
                        unit: DurationUnit::Day,
                    },
                )
                .unwrap();
            }
        }
    }
    p
}

/// P23 : symétrie avant/arrière (ADR-0010, R-106 à R-110). Réseau plat, un
/// calendrier, décalages ouvrés, sans jalon, sans %, sans échéance : `N`
/// planifié depuis son début `s`, son miroir `N^R` à rebours depuis `F'`
/// (liens retournés : FS → FS, SS → FF, FF → SS, SF → SF, même décalage ;
/// contraintes ASAP ↔ ALAP, SNET ↔ FNLT, FNET ↔ SNLT, MSO ↔ MFO, dates
/// miroir en temps ouvré ; même option, même R-051). Avec
/// `u(t) = W(t) − W(s)` et `v(t) = W(F') − W(t)` : `u(ES) = v(LF^R)`,
/// `u(EF) = v(LS^R)`, `u(LS) = v(EF^R)`, `u(LF) = v(ES^R)`, mêmes marges
/// totales, mêmes tâches critiques, `u(fin de N) = v(S*)`.
fn p23(seed: u64) {
    let n = symmetric(seed);
    let (set, _) = CalendarSet::compile(&n);
    let cal = set.get(n.calendar).unwrap();
    let origin: Moment = "2025-01-01T00:00".parse().unwrap();
    let w = |m: Moment| cal.working_between(origin, m).unwrap().get();
    let ScheduleAnchor::FromStart(s0) = n.anchor else {
        panic!("ancre")
    };
    let mut r = Rng(seed.rotate_left(13) | 1);
    let f0 = s0
        .checked_add(Minutes::new(r.range(0, 90) * 1_440 + r.range(0, 1_439)))
        .unwrap();
    // Date miroir : W(c') = W(F') + W(s) − W(c).
    let mirror = |c: Moment| {
        let d = w(c) - w(s0);
        cal.add_working(f0, Minutes::new(-d)).unwrap()
    };
    p23_check(&n, f0, &mirror);
}

/// P23b : cas concret de P23 sur le calendrier Standard sans exception,
/// symétrique autour du mercredi 12:30 (lun 08:00 ↔ ven 17:00) : `N` depuis
/// lun 19/10/2026 08:00, `N^R` à rebours depuis ven 23/10 17:00, dates
/// miroir brutes `t' = 2c − t` (c = mer 21/10 12:30), comparées en temps
/// ouvré.
fn p23b(seed: u64) {
    let mut n = symmetric(seed);
    let s0: Moment = "2026-10-19T08:00".parse().unwrap();
    let c: Moment = "2026-10-21T12:30".parse().unwrap();
    n.anchor = ScheduleAnchor::FromStart(s0);
    n.calendar = CalendarUid::new(1);
    let mirror = |t: Moment| Moment::from_minutes(2 * c.minutes() - t.minutes()).unwrap();
    p23_check(&n, mirror(s0), &mirror);
}

/// Contrôle commun de P23 et P23b : `N` (depuis le début) et son miroir
/// `N^R` à rebours depuis `f0`, dates de contrainte par `mirror`.
fn p23_check(n: &Project, f0: Moment, mirror: &dyn Fn(Moment) -> Moment) {
    let (set, _) = CalendarSet::compile(n);
    let cal = set.get(n.calendar).unwrap();
    let origin: Moment = "2025-01-01T00:00".parse().unwrap();
    let w = |m: Moment| cal.working_between(origin, m).unwrap().get();
    let ScheduleAnchor::FromStart(s0) = n.anchor else {
        panic!("ancre")
    };
    let mut m = n.clone();
    m.anchor = ScheduleAnchor::FromFinish(f0);
    for t in m.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
        m.tasks.get_mut(t).unwrap().predecessors.clear();
    }
    for t in &n.tasks {
        for l in &t.predecessors {
            let kind = match l.kind {
                LinkType::FinishToStart => LinkType::FinishToStart,
                LinkType::StartToStart => LinkType::FinishToFinish,
                LinkType::FinishToFinish => LinkType::StartToStart,
                LinkType::StartToFinish => LinkType::StartToFinish,
            };
            m.tasks.get_mut(l.predecessor).unwrap().predecessors.push(Link {
                predecessor: t.uid(),
                kind,
                lag: l.lag,
            });
        }
        let c = match t.constraint {
            Constraint::AsSoonAsPossible => Constraint::AsLateAsPossible,
            Constraint::AsLateAsPossible => Constraint::AsSoonAsPossible,
            Constraint::StartNoEarlierThan { date } => Constraint::FinishNoLaterThan { date: mirror(date) },
            Constraint::FinishNoLaterThan { date } => Constraint::StartNoEarlierThan { date: mirror(date) },
            Constraint::FinishNoEarlierThan { date } => Constraint::StartNoLaterThan { date: mirror(date) },
            Constraint::StartNoLaterThan { date } => Constraint::FinishNoEarlierThan { date: mirror(date) },
            Constraint::MustStartOn { date } => Constraint::MustFinishOn { date: mirror(date) },
            Constraint::MustFinishOn { date } => Constraint::MustStartOn { date: mirror(date) },
        };
        m.tasks.get_mut(t.uid()).unwrap().constraint = c;
    }
    let a = schedule(n);
    let b = schedule(&m);
    let u = |t: Moment| w(t) - w(s0);
    let v = |t: Moment| w(f0) - w(t);
    for t in &n.tasks {
        let x = a.scheduled(t.uid()).unwrap();
        let y = b.scheduled(t.uid()).unwrap();
        let got = (
            u(x.early.start),
            u(x.early.finish),
            u(x.late.start),
            u(x.late.finish),
        );
        let want = (
            v(y.late.finish),
            v(y.late.start),
            v(y.early.finish),
            v(y.early.start),
        );
        assert_eq!(got, want, "tâche {} (ES, EF, LS, LF)", t.uid());
        assert_eq!(x.total_slack, y.total_slack, "tâche {} : TF", t.uid());
        assert_eq!(x.critical, y.critical, "tâche {} : critique", t.uid());
    }
    assert_eq!(u(a.project_finish), v(b.project_start), "fin ↔ début calculé");
}

// ---------------------------------------------------------------------------
// P14
// ---------------------------------------------------------------------------

/// Chaîne de marge nulle : en remontant les pilotes depuis une tâche qui
/// finit à la fin du projet, chaque tâche a une marge totale nulle.
fn p14(p: &Project) {
    let s = schedule(p);
    let n = p.tasks.len();
    for (uid, o) in &s.tasks {
        let TaskOutcome::Scheduled(ts) = o else {
            panic!("{uid} non planifiée : {o:?}")
        };
        if ts.early.finish != s.project_finish {
            continue;
        }
        let mut cur = (*uid, *ts);
        for _ in 0..=n {
            assert_eq!(
                cur.1.total_slack,
                Minutes::ZERO,
                "chaîne depuis {uid} : {} {:?}",
                cur.0,
                cur.1
            );
            match cur.1.driver {
                Driver::ProjectStart => break,
                Driver::Link { predecessor, .. } => {
                    cur = (predecessor, *s.scheduled(predecessor).unwrap());
                }
                d => panic!("pilote inattendu {d:?}"),
            }
        }
        assert_eq!(cur.1.driver, Driver::ProjectStart, "chaîne sans fin depuis {uid}");
    }
}

// ---------------------------------------------------------------------------
// P15 : oracle indépendant en temps ouvré
// ---------------------------------------------------------------------------

/// Dates et marges de l'oracle, en minutes ouvrées depuis une origine.
struct Oracle {
    es: Vec<i64>,
    ef: Vec<i64>,
    ls: Vec<i64>,
    lf: Vec<i64>,
    free: Vec<i64>,
}

/// Plus long chemin entier par relaxation jusqu'à stabilité (réseau plat,
/// un seul calendrier, décalages ouvrés). Formules en temps ouvré : FS
/// `ES ≥ EF_p + L`, SS `ES ≥ ES_p + L`, FF `ES ≥ EF_p + L − d`, SF
/// `ES ≥ ES_p + L − d`, `ES ≥ W(début)` ; retour symétrique.
fn oracle(p: &Project, w_start: i64) -> Oracle {
    let n = p.tasks.len();
    let d: Vec<i64> = p.tasks.iter().map(|t| t.duration.minutes().get()).collect();
    // Liens : (pred, succ, type, L).
    let mut links = Vec::new();
    for (si, t) in p.tasks.iter().enumerate() {
        for l in &t.predecessors {
            let pi = p.tasks.position(l.predecessor).unwrap();
            let lag = match l.lag.value {
                LagValue::Time { minutes, .. } => minutes.get(),
                LagValue::Percent { percent } => percent_of(Minutes::new(d[pi]), percent).unwrap().get(),
            };
            links.push((pi, si, l.kind, lag));
        }
    }
    let mut es = vec![w_start; n];
    loop {
        let mut changed = false;
        for &(pi, si, kind, lag) in &links {
            let c = match kind {
                LinkType::FinishToStart => es[pi] + d[pi] + lag,
                LinkType::StartToStart => es[pi] + lag,
                LinkType::FinishToFinish => es[pi] + d[pi] + lag - d[si],
                LinkType::StartToFinish => es[pi] + lag - d[si],
            };
            if c > es[si] {
                es[si] = c;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let ef: Vec<i64> = (0..n).map(|i| es[i] + d[i]).collect();
    let finish = ef.iter().copied().max().unwrap_or(w_start);
    let mut lf = vec![finish; n];
    loop {
        let mut changed = false;
        for &(pi, si, kind, lag) in &links {
            let ls_s = lf[si] - d[si];
            let c = match kind {
                LinkType::FinishToStart => ls_s - lag,
                LinkType::StartToStart => ls_s - lag + d[pi],
                LinkType::FinishToFinish => lf[si] - lag,
                LinkType::StartToFinish => lf[si] - lag + d[pi],
            };
            if c < lf[pi] {
                lf[pi] = c;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let ls: Vec<i64> = (0..n).map(|i| lf[i] - d[i]).collect();
    let mut free: Vec<i64> = (0..n).map(|i| lf[i] - ef[i]).collect();
    for &(pi, si, kind, lag) in &links {
        let gap = match kind {
            LinkType::FinishToStart => es[si] - lag - ef[pi],
            LinkType::StartToStart => es[si] - lag - es[pi],
            LinkType::FinishToFinish => ef[si] - lag - ef[pi],
            LinkType::StartToFinish => ef[si] - lag - es[pi],
        };
        free[pi] = free[pi].min(gap);
    }
    Oracle { es, ef, ls, lf, free }
}

fn p15(p: &Project) {
    let s = schedule(p);
    let (set, _) = CalendarSet::compile(p);
    let cal = set.get(p.calendar).unwrap();
    let origin: Moment = "2025-01-01T00:00".parse().unwrap();
    let w = |m: Moment| cal.working_between(origin, m).unwrap().get();
    let ScheduleAnchor::FromStart(start) = p.anchor else {
        panic!("ancre")
    };
    let o = oracle(p, w(start));
    for (i, t) in p.tasks.iter().enumerate() {
        let ts = s.scheduled(t.uid()).unwrap();
        let got = (
            w(ts.early.start),
            w(ts.early.finish),
            w(ts.late.start),
            w(ts.late.finish),
            ts.total_slack.get(),
            ts.free_slack.get(),
        );
        let want = (o.es[i], o.ef[i], o.ls[i], o.lf[i], o.lf[i] - o.ef[i], o.free[i]);
        assert_eq!(got, want, "tâche {} (ES, EF, LS, LF, TF, FS)", t.uid());
    }
}

// ---------------------------------------------------------------------------
// P16 : monotonie
// ---------------------------------------------------------------------------

fn p16(p: &Project, seed: u64) {
    let s = schedule(p);
    let par = parents(p);
    let mut r = Rng(seed.rotate_left(29) | 1);
    let leaves: Vec<usize> = (0..p.tasks.len()).filter(|&q| !is_summary(&par, q)).collect();
    let q = leaves[usize::try_from(r.range(0, i64::try_from(leaves.len()).unwrap() - 1)).unwrap()];
    let mut longer = p.clone();
    let uid = p.tasks.at(q).unwrap().uid();
    let t = longer.tasks.get_mut(uid).unwrap();
    let d = t.duration;
    t.duration = Duration::new(
        Minutes::new(d.minutes().get() + r.range(1, 2_400)),
        d.unit(),
        d.basis(),
        d.is_estimated(),
    )
    .unwrap();
    let s2 = schedule(&longer);
    assert!(
        s2.project_finish >= s.project_finish,
        "allonger {uid} avance la fin du projet : {} < {}",
        s2.project_finish,
        s.project_finish
    );
}

// ---------------------------------------------------------------------------
// P20 : contrainte tenue ou signalée
// ---------------------------------------------------------------------------

/// Ancrage R-090 recalculé à partir des primitives publiques de
/// `TimeScale`.
fn anchor_of(
    scale: TimeScale<'_>,
    kind: ConstraintKind,
    date: Moment,
    d: Minutes,
) -> Option<plannercraft_core::Span> {
    if kind.is_start_side() {
        return scale.forward_span(date, d).ok();
    }
    let f = scale.backward_span(date, Minutes::ZERO).ok()?.finish;
    let start = scale.add(f, Minutes::new(-d.get())).ok()?;
    Some(plannercraft_core::Span { start, finish: f })
}

/// P20 : plancher toujours tenu (en temps ouvré) ; plafond tenu, ou
/// `constraint.not_honored` présent (et marge totale négative s'il est
/// dépassé en temps ouvré) ; serrage tenu ; épingle exacte (`early == late
/// == A`) ; `task.deadline_missed` ⇔ fin planifiée après l'échéance en temps
/// ouvré de l'échelle de placement.
fn p20(p: &Project) {
    let s = schedule(p);
    let cals = Cals::new(p);
    let par = parents(p);
    let honor = p.options.honor_constraints;
    let has = |code: &str, uid: TaskUid| {
        s.diagnostics
            .iter()
            .any(|d| d.code() == code && d.subject() == plannercraft_core::Subject::Task(uid))
    };
    for (i, t) in p.tasks.iter().enumerate() {
        let uid = t.uid();
        let Some(ts) = planned(&s, uid) else { continue };
        let summary = is_summary(&par, i);
        let place = if summary {
            cals.of(p, t).map(TimeScale::Working)
        } else {
            cals.place(p, t)
        };
        let Some(place) = place else { continue };
        let w = |a: Moment, b: Moment| place.between(a, b).unwrap().get();
        if let Some(dl) = t.deadline {
            assert_eq!(
                w(dl, ts.finish) > 0,
                has("task.deadline_missed", uid),
                "échéance de {uid} : {ts:?}"
            );
        }
        let kind = t.constraint.kind();
        let Some(date) = t.constraint.date() else { continue };
        if t.mode != SchedulingMode::Auto {
            continue;
        }
        if summary {
            // R-103 : FNLT d'une récapitulative, tenu ou signalé.
            if kind == ConstraintKind::FinishNoLaterThan {
                let f = place.backward_span(date, Minutes::ZERO).unwrap().finish;
                assert!(
                    w(f, ts.finish) <= 0 || has("constraint.not_honored", uid),
                    "FNLT de la récapitulative {uid}"
                );
            }
            continue;
        }
        let Some(a) = anchor_of(place, kind, date, ts.duration) else {
            continue;
        };
        let start = kind.is_start_side();
        // Dépassements de l'ancrage par les dates au plus tôt : brut (début)
        // et en temps ouvré.
        let (above_raw, above_wt, below_wt) = if start {
            (
                ts.early.start > a.start,
                w(a.start, ts.early.start) > 0,
                w(ts.early.start, a.start) > 0,
            )
        } else {
            let above = w(a.finish, ts.early.finish) > 0;
            (above, above, w(ts.early.finish, a.finish) > 0)
        };
        match rule(kind, honor) {
            Rule::None | Rule::Opposite => {}
            Rule::Floor => assert!(!below_wt, "plancher de {uid} : {ts:?} {a:?}"),
            Rule::Window | Rule::Cap => {
                if rule(kind, honor) == Rule::Window {
                    assert!(!below_wt, "fenêtre de {uid} : {ts:?} {a:?}");
                }
                if above_raw {
                    assert!(has("constraint.not_honored", uid), "plafond de {uid} non signalé");
                }
                if above_wt {
                    assert!(ts.total_slack < Minutes::ZERO, "plafond de {uid} : TF {ts:?}");
                }
            }
            Rule::Clamped => assert!(!above_raw, "serrage de {uid} : {ts:?} {a:?}"),
            Rule::Pin => {
                assert_eq!(ts.early, a, "épingle de {uid}");
                assert_eq!(ts.late, a, "épingle de {uid}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// P21 : oracle indépendant avec contraintes
// ---------------------------------------------------------------------------

/// P21 : oracle de P15 étendu aux contraintes (planchers, serrages,
/// épingles, plafonds, fenêtres), aux échéances et à l'épingle ALAP, en
/// minutes ouvrées (réseau plat, un seul calendrier, décalages ouvrés, sans
/// jalon). En temps ouvré, une date de contrainte `c` vaut `W(c)` des deux
/// côtés (R-090 : `s*` et `f*` ont le temps ouvré de `c`).
fn oracle_constrained(p: &Project, w_start: i64, w: &dyn Fn(Moment) -> i64) -> Oracle {
    let n = p.tasks.len();
    let honor = p.options.honor_constraints;
    let d: Vec<i64> = p.tasks.iter().map(|t| t.duration.minutes().get()).collect();
    let rules: Vec<(Rule, bool, i64)> = p
        .tasks
        .iter()
        .map(|t| {
            let kind = t.constraint.kind();
            (
                rule(kind, honor),
                kind.is_start_side(),
                t.constraint.date().map_or(0, w),
            )
        })
        .collect();
    let deadlines: Vec<Option<i64>> = p.tasks.iter().map(|t| t.deadline.map(w)).collect();
    let mut links = Vec::new();
    for (si, t) in p.tasks.iter().enumerate() {
        for l in &t.predecessors {
            let pi = p.tasks.position(l.predecessor).unwrap();
            let lag = match l.lag.value {
                LagValue::Time { minutes, .. } => minutes.get(),
                LagValue::Percent { percent } => percent_of(Minutes::new(d[pi]), percent).unwrap().get(),
            };
            links.push((pi, si, l.kind, lag));
        }
    }
    // Début au plus tôt d'une tâche depuis son début « naturel ».
    let place = |i: usize, nat: i64| {
        let (r, start, c) = rules[i];
        let a = if start { c } else { c - d[i] };
        match r {
            Rule::Floor | Rule::Window => nat.max(a),
            Rule::Clamped => nat.min(a),
            Rule::Pin => a,
            Rule::None | Rule::Opposite | Rule::Cap => nat,
        }
    };
    let forward = |pins: &[Option<i64>]| {
        let mut es: Vec<i64> = (0..n)
            .map(|i| pins[i].unwrap_or_else(|| place(i, w_start)))
            .collect();
        loop {
            let mut nat: Vec<i64> = vec![w_start; n];
            for &(pi, si, kind, lag) in &links {
                let c = match kind {
                    LinkType::FinishToStart => es[pi] + d[pi] + lag,
                    LinkType::StartToStart => es[pi] + lag,
                    LinkType::FinishToFinish => es[pi] + d[pi] + lag - d[si],
                    LinkType::StartToFinish => es[pi] + lag - d[si],
                };
                nat[si] = nat[si].max(c);
            }
            let next: Vec<i64> = (0..n)
                .map(|i| pins[i].unwrap_or_else(|| place(i, nat[i])))
                .collect();
            if next == es {
                return es;
            }
            es = next;
        }
    };
    let mut es = forward(&vec![None; n]);
    let ef: Vec<i64> = (0..n).map(|i| es[i] + d[i]).collect();
    let finish = ef.iter().copied().max().unwrap_or(w_start);
    // Plafond propre de chaque tâche (contrainte, échéance) ; épingle MSO/MFO.
    let own_cap = |i: usize| {
        let (r, start, c) = rules[i];
        let a_finish = if start { c + d[i] } else { c };
        let mut cap = i64::MAX;
        if matches!(r, Rule::Cap | Rule::Window | Rule::Clamped | Rule::Pin) {
            cap = a_finish;
        }
        if let Some(dl) = deadlines[i]
            && r != Rule::Pin
        {
            cap = cap.min(dl);
        }
        cap
    };
    let mut lf: Vec<i64> = (0..n).map(|i| finish.min(own_cap(i))).collect();
    loop {
        let mut next: Vec<i64> = (0..n).map(|i| finish.min(own_cap(i))).collect();
        for &(pi, si, kind, lag) in &links {
            if rules[pi].0 == Rule::Pin {
                continue;
            }
            let ls_s = lf[si] - d[si];
            let c = match kind {
                LinkType::FinishToStart => ls_s - lag,
                LinkType::StartToStart => ls_s - lag + d[pi],
                LinkType::FinishToFinish => lf[si] - lag,
                LinkType::StartToFinish => lf[si] - lag + d[pi],
            };
            next[pi] = next[pi].min(c);
        }
        if next == lf {
            break;
        }
        lf = next;
    }
    let ls: Vec<i64> = (0..n).map(|i| lf[i] - d[i]).collect();
    // R-092 : ALAP de marge ≥ 0 épinglées sur leurs dates au plus tard,
    // puis une repasse avant.
    let pins: Vec<Option<i64>> = (0..n)
        .map(|i| (rules[i].0 == Rule::Opposite && lf[i] - ef[i] >= 0 && ls[i] != es[i]).then_some(ls[i]))
        .collect();
    if pins.iter().any(Option::is_some) {
        es = forward(&pins);
    }
    let ef: Vec<i64> = (0..n).map(|i| es[i] + d[i]).collect();
    let mut free: Vec<i64> = (0..n).map(|i| lf[i] - ef[i]).collect();
    for &(pi, si, kind, lag) in &links {
        let gap = match kind {
            LinkType::FinishToStart => es[si] - lag - ef[pi],
            LinkType::StartToStart => es[si] - lag - es[pi],
            LinkType::FinishToFinish => ef[si] - lag - ef[pi],
            LinkType::StartToFinish => ef[si] - lag - es[pi],
        };
        free[pi] = free[pi].min(gap);
    }
    let free = free.into_iter().map(|f| f.max(0)).collect();
    Oracle { es, ef, ls, lf, free }
}

fn p21(p: &Project) {
    let s = schedule(p);
    let (set, _) = CalendarSet::compile(p);
    let cal = set.get(p.calendar).unwrap();
    let origin: Moment = "2025-01-01T00:00".parse().unwrap();
    let w = |m: Moment| cal.working_between(origin, m).unwrap().get();
    let ScheduleAnchor::FromStart(start) = p.anchor else {
        panic!("ancre")
    };
    let o = oracle_constrained(p, w(start), &w);
    for (i, t) in p.tasks.iter().enumerate() {
        let ts = s.scheduled(t.uid()).unwrap();
        let got = (
            w(ts.early.start),
            w(ts.early.finish),
            w(ts.late.start),
            w(ts.late.finish),
            ts.total_slack.get(),
            ts.free_slack.get(),
        );
        let want = (o.es[i], o.ef[i], o.ls[i], o.lf[i], o.lf[i] - o.ef[i], o.free[i]);
        assert_eq!(got, want, "tâche {} (ES, EF, LS, LF, TF, FS)", t.uid());
    }
}

// ---------------------------------------------------------------------------
// P22 : ALAP
// ---------------------------------------------------------------------------

/// P22 : les feuilles ALAP de marge totale ≥ 0 sont planifiées sur leurs
/// dates au plus tard ; aucun lien n'est violé par les dates planifiées
/// (P13 a') ; la fin du projet est celle du même réseau tout en ASAP.
fn p22(p: &Project) {
    let s = schedule(p);
    let mut asap = p.clone();
    for uid in asap.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
        let t = asap.tasks.get_mut(uid).unwrap();
        if t.constraint == Constraint::AsLateAsPossible {
            t.constraint = Constraint::AsSoonAsPossible;
        }
    }
    assert_eq!(schedule(&asap).project_finish, s.project_finish, "fin du projet");
    let cals = Cals::new(p);
    let par = parents(p);
    for (i, t) in p.tasks.iter().enumerate() {
        if is_summary(&par, i)
            || t.constraint != Constraint::AsLateAsPossible
            || t.mode != SchedulingMode::Auto
        {
            continue;
        }
        let Some(ts) = planned(&s, t.uid()) else { continue };
        let Some(place) = cals.place(p, t) else { continue };
        if ts.total_slack >= Minutes::ZERO {
            assert_eq!(
                place.between(ts.start, ts.late.start).unwrap(),
                Minutes::ZERO,
                "ALAP {} : {ts:?}",
                t.uid()
            );
        }
    }
    check_network(p, &s, &BTreeSet::new(), false);
}

// ---------------------------------------------------------------------------
// P17 : chaos
// ---------------------------------------------------------------------------

/// Tâches d'une composante (liens et hiérarchie) qui contient une manuelle.
fn manual_components(p: &Project) -> BTreeSet<TaskUid> {
    let n = p.tasks.len();
    let mut root: Vec<usize> = (0..n).collect();
    fn find(root: &mut [usize], mut x: usize) -> usize {
        while root[x] != x {
            root[x] = root[root[x]];
            x = root[x];
        }
        x
    }
    let par = parents(p);
    for (si, t) in p.tasks.iter().enumerate() {
        let join = |a: usize, b: usize, root: &mut Vec<usize>| {
            let (ra, rb) = (find(root, a), find(root, b));
            root[ra] = rb;
        };
        if let Some(pp) = par[si] {
            join(si, pp, &mut root);
        }
        for l in &t.predecessors {
            if let Some(pi) = p.tasks.position(l.predecessor) {
                join(si, pi, &mut root);
            }
        }
    }
    let tainted: BTreeSet<usize> = p
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t.mode, SchedulingMode::Manual { .. }))
        .map(|(i, _)| find(&mut root, i))
        .collect();
    (0..n)
        .filter(|&i| tainted.contains(&find(&mut root, i)))
        .map(|i| p.tasks.at(i).unwrap().uid())
        .collect()
}

/// Réseau plat : les composantes fortement connexes des liens effectifs
/// sont exactement les cycles signalés (`link.cycle`).
fn check_cycles(p: &Project, s: &Schedule) {
    let n = p.tasks.len();
    let undated = |t: &Task| {
        t.active
            && matches!(
                t.mode,
                SchedulingMode::Manual {
                    start: None,
                    finish: None,
                    ..
                }
            )
    };
    let mut adj = vec![Vec::new(); n];
    for (si, t) in p.tasks.iter().enumerate() {
        for l in &t.predecessors {
            let Some(pi) = p.tasks.position(l.predecessor) else {
                continue;
            };
            let pt = p.tasks.at(pi).unwrap();
            // R-062 : un lien d'une inactive vers une active est ignoré ; entre
            // deux inactives, il est gardé pour l'affichage (et peut boucler).
            if pi == si || pt.is_null || t.is_null || undated(pt) || undated(t) || (!pt.active && t.active) {
                continue;
            }
            adj[pi].push(si);
        }
    }
    let reach = |from: usize| {
        let mut seen = vec![false; n];
        let mut stack = vec![from];
        while let Some(x) = stack.pop() {
            for &y in &adj[x] {
                if !seen[y] {
                    seen[y] = true;
                    stack.push(y);
                }
            }
        }
        seen
    };
    let reachable: Vec<Vec<bool>> = (0..n).map(reach).collect();
    let mut sccs: BTreeSet<Vec<TaskUid>> = BTreeSet::new();
    for i in 0..n {
        let comp: Vec<TaskUid> = (0..n)
            .filter(|&j| j == i || (reachable[i][j] && reachable[j][i]))
            .map(|j| p.tasks.at(j).unwrap().uid())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if comp.len() >= 2 {
            sccs.insert(comp);
        }
    }
    let reported: BTreeSet<Vec<TaskUid>> = s
        .diagnostics
        .iter()
        .filter_map(|d| match d {
            Diagnostic::LinkCycle { tasks } => Some(tasks.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(reported, sccs);
}

fn p17(p: &Project) {
    let s = schedule(p);
    check_shape(p, &s);
    for (uid, o) in &s.tasks {
        if let TaskOutcome::Scheduled(ts) = o {
            assert!(ts.early.start <= ts.early.finish, "{uid}");
            assert!(ts.late.start <= ts.late.finish, "{uid}");
        }
    }
    if p.tasks.iter().all(|t| t.outline_level.get() == 1) {
        check_cycles(p, &s);
    }
    let usable = CalendarSet::compile(p)
        .0
        .get(p.calendar)
        .is_ok_and(CompiledCalendar::has_working_time);
    if usable {
        check_network(p, &s, &manual_components(p), false);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::default())]

    #[test]
    fn p13_cpm_composite(seed in any::<u64>()) {
        p13(&generate(seed, CONSTRAINED));
    }

    #[test]
    fn p14_zero_slack_chain(seed in any::<u64>()) {
        p14(&generate(seed, FLAT));
    }

    #[test]
    fn p13_from_finish_composite(seed in any::<u64>()) {
        p13_from_finish(&generate_from_finish(seed, CONSTRAINED));
    }
}

/// Nombre de cas de `PROPTEST_CASES` (lu par proptest), plafonné.
fn capped(cap: u32) -> ProptestConfig {
    ProptestConfig::with_cases(ProptestConfig::default().cases.min(cap))
}

proptest! {
    #![proptest_config(capped(1_024))]

    #[test]
    fn p15_matches_working_time_oracle(seed in any::<u64>()) {
        p15(&generate(seed, FLAT_NO_MILESTONE));
    }

    #[test]
    fn p16_project_finish_is_monotonic(seed in any::<u64>()) {
        p16(&generate(seed, MONOTONE), seed);
    }

    #[test]
    fn p20_constraint_held_or_reported(seed in any::<u64>()) {
        p20(&generate(seed, CONSTRAINED));
    }

    #[test]
    fn p21_matches_constrained_oracle(seed in any::<u64>()) {
        p21(&generate(seed, FLAT_CONSTRAINED_NO_MILESTONE));
    }

    #[test]
    fn p22_alap_on_late_dates(seed in any::<u64>()) {
        p22(&generate(seed, CONSTRAINED));
    }

    #[test]
    fn p16b_computed_start_is_monotonic(seed in any::<u64>()) {
        p16b(&generate_from_finish(seed, MONOTONE_FROM_FINISH), seed);
    }

    #[test]
    fn p23_forward_backward_symmetry(seed in any::<u64>()) {
        p23(seed);
    }

    #[test]
    fn p23b_standard_calendar_raw_mirror(seed in any::<u64>()) {
        p23b(seed);
    }
}

proptest! {
    #![proptest_config(capped(2_000))]

    #[test]
    fn p17_chaos(seed in any::<u64>()) {
        p17(&chaos(seed, seed % 2 == 1));
    }
}

/// Graines fixes, indépendantes de proptest (exécutées à chaque fois).
#[test]
fn fixed_seeds() {
    for seed in 0..100 {
        p13(&generate(seed, GENERAL));
        p13(&generate(seed, CONSTRAINED));
        p14(&generate(seed, FLAT));
        p15(&generate(seed, FLAT_NO_MILESTONE));
        p16(&generate(seed, MONOTONE), seed);
        p17(&chaos(seed, seed % 2 == 1));
        p20(&generate(seed, CONSTRAINED));
        p21(&generate(seed, FLAT_CONSTRAINED_NO_MILESTONE));
        p22(&generate(seed, CONSTRAINED));
        p13_from_finish(&generate_from_finish(seed, GENERAL));
        p13_from_finish(&generate_from_finish(seed, CONSTRAINED));
        p16b(&generate_from_finish(seed, MONOTONE_FROM_FINISH), seed);
        p17(&chaos(seed, true));
        p23(seed);
        p23b(seed);
    }
}

/// Régressions trouvées par P17, rejouées explicitement.
#[test]
fn p17_regressions() {
    // Lien FS dont le transfert sort du domaine (début du projet en 2149)
    // vers une récapitulative : la porte échoue (R-069) ; le prédécesseur
    // avait une marge libre négative tant que l'arc comptait encore.
    p17(&chaos(10_145_788_266_605_795_041, false));
    // Porte d'une sous-récapitulative en échec : ses sous-tâches, planifiées
    // sans la porte du parent, faussaient la marge libre des prédécesseurs du
    // parent (dates au plus tard de repli depuis, `degraded`).
    p17(&chaos(40_512_168_739_394_516, false));
    // Lien FF vers une tâche qui finit sur son dernier temps ouvré avant la
    // fin du domaine : l'ancre reportée (R-047) est la fin du domaine.
    p17(&chaos(16_281_757_938_398_638_621, false));
}

// ---------------------------------------------------------------------------
// J5 : affectations (P34, P35 partie moteur)
// ---------------------------------------------------------------------------

/// P34 : sur le projet `oracle` (sans affectation), chaque feuille
/// automatique active à durée ouvrée reçoit, six fois sur dix, une
/// affectation de travail unique sans délai, ressource sans calendrier ou
/// sur le calendrier de la tâche, de travail `d × U / 10 000` exact (donc
/// `d_a = d`) ; la durée enregistrée d'une tâche à unités fixes est
/// déplacée d'une heure une fois sur deux (elle ne place plus la tâche,
/// R-170). Le planning doit être celui de l'oracle, et chaque affectation
/// porter les dates de sa tâche.
fn p34(oracle: &Project, seed: u64) {
    let base = schedule(oracle);
    let mut r = Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1);
    let mut p = oracle.clone();
    let mut assigned = Vec::new();
    let mut next = 1u32;
    for t in &oracle.tasks {
        let Some(TaskOutcome::Scheduled(s)) = base.get(t.uid()) else {
            continue;
        };
        if s.role != TaskRole::Leaf || s.manual || t.duration.basis() != TimeBasis::Working || !r.chance(60) {
            continue;
        }
        let d = t.duration.minutes().get();
        let units = match r.range(0, 3) {
            0 => 10_000,
            1 if d % 2 == 0 => 5_000,
            2 if d % 4 == 0 => 2_500,
            _ => 20_000,
        };
        let ruid = ResourceUid::new(next);
        let mut res = Resource::new(ruid, format!("R{next}"));
        res.calendar = if r.chance(50) { t.calendar } else { None };
        p.resources.push(res).unwrap();
        let auid = AssignmentUid::new(next);
        let mut a = Assignment::new(auid, t.uid(), Some(ruid));
        a.units = Units::new(units).unwrap();
        a.work = Work::new(Minutes::new(d * units / 10_000)).unwrap();
        p.assignments.push(a).unwrap();
        if t.task_type == TaskType::FixedUnits && r.chance(50) {
            let task = p.tasks.get_mut(t.uid()).unwrap();
            task.duration =
                Duration::new(Minutes::new(d + 60), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        }
        assigned.push((t.uid(), auid));
        next += 1;
    }
    let got = schedule(&p);
    // Dates seulement : affectations, charges, travail (R-184) et
    // surutilisation (R-186) retirés.
    let mut stripped = got.clone();
    stripped.assignments.clear();
    stripped.resources.clear();
    stripped
        .diagnostics
        .retain(|d| !matches!(d, plannercraft_core::Diagnostic::ResourceOverallocated { .. }));
    for o in stripped.tasks.values_mut() {
        if let TaskOutcome::Scheduled(ts) = o {
            ts.work = Work::ZERO;
        }
    }
    if let Some(ps) = stripped.project_summary.as_mut() {
        ps.work = Work::ZERO;
    }
    assert_eq!(stripped, base, "P34, graine {seed}");
    for (task, auid) in assigned {
        let s = got.scheduled(task).unwrap();
        let a = got.assignments.get(&auid).unwrap();
        assert_eq!(a.role, AssignmentRole::Work, "P34, graine {seed}");
        // Dates planifiées de la tâche (R-112, R-173 amendée) : au plus tôt
        // depuis le début, au plus tard à rebours.
        let planned = plannercraft_core::Span {
            start: s.start,
            finish: s.finish,
        };
        assert_eq!(a.span, planned, "P34, graine {seed}, tâche {task}");
        assert_eq!(a.late, Some(s.late), "P34, graine {seed}, tâche {task}");
    }
}

/// Réseau de P35 : 1 à 12 feuilles automatiques, liens FS et SS tirés vers
/// l'avant, 1 à 4 ressources de calendriers dérivés du Standard (un
/// par ressource), 1 à 3 affectations par feuille (travail 0 à 2 400, 50 à
/// 200 %, délai une fois sur trois), une feuille sur cinq sur le calendrier
/// aléatoire (UID 4). Rend aussi un jour et une ressource à rendre chômés.
fn p35_network(seed: u64) -> (Project, CalendarUid, plannercraft_core::Day) {
    let mut r = Rng(seed | 1);
    let start: Moment = "2026-10-19T08:00".parse().unwrap();
    let mut p = Project::new(ProjectInit {
        name: "P35".to_owned(),
        anchor: ScheduleAnchor::FromStart(start),
        current_date: start,
        calendar_names: BuiltinCalendarNames::default(),
    });
    p.calendars.push(random_calendar(&mut r)).unwrap();
    let resources = u32::try_from(r.range(1, 4)).unwrap();
    for k in 1..=resources {
        let cal = CalendarUid::new(10 + k);
        p.calendars
            .push(Calendar::new_derived(cal, format!("R{k}"), CalendarUid::new(1)))
            .unwrap();
        let mut res = Resource::new(ResourceUid::new(k), format!("R{k}"));
        res.calendar = Some(cal);
        p.resources.push(res).unwrap();
    }
    let n = u32::try_from(r.range(1, 12)).unwrap();
    let options = p.options;
    let mut next = 1u32;
    for i in 1..=n {
        let mut t = Task::new(TaskUid::new(i), format!("T{i}"), &options);
        t.duration = Duration::new(Minutes::new(480), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        if r.chance(20) {
            t.calendar = Some(CalendarUid::new(4));
        }
        for _ in 0..r.range(0, 2) {
            if i == 1 {
                break;
            }
            let pred = TaskUid::new(u32::try_from(r.range(1, i64::from(i) - 1)).unwrap());
            if t.predecessors.iter().any(|l| l.predecessor == pred) {
                continue;
            }
            let kind = if r.chance(50) {
                LinkType::FinishToStart
            } else {
                LinkType::StartToStart
            };
            t.predecessors.push(Link {
                predecessor: pred,
                kind,
                lag: Lag::new(
                    TimeBasis::Working,
                    LagValue::Time {
                        minutes: Minutes::new(r.range(0, 480)),
                        unit: DurationUnit::Day,
                    },
                )
                .unwrap(),
            });
        }
        p.tasks.push(t).unwrap();
        for _ in 0..r.range(1, 3) {
            let res = ResourceUid::new(u32::try_from(r.range(1, i64::from(resources))).unwrap());
            let mut a = Assignment::new(AssignmentUid::new(next), TaskUid::new(i), Some(res));
            a.work = Work::new(Minutes::new(r.range(0, 2_400))).unwrap();
            a.units = Units::new(r.range(1, 4) * 5_000).unwrap();
            if r.chance(33) {
                a.delay = Minutes::new(r.range(0, 960));
            }
            p.assignments.push(a).unwrap();
            next += 1;
        }
    }
    let victim = CalendarUid::new(10 + u32::try_from(r.range(1, i64::from(resources))).unwrap());
    let day = start
        .day()
        .checked_add_days(i32::try_from(r.range(0, 30)).unwrap())
        .unwrap();
    (p, victim, day)
}

/// P35 (partie moteur) : rendre chômé le jour `day` du calendrier de
/// ressource `victim` ne fait commencer ni finir aucune tâche plus tôt, ni
/// finir le projet plus tôt.
fn p35(seed: u64) {
    let (p, victim, day) = p35_network(seed);
    let before = schedule(&p);
    let mut q = p.clone();
    q.calendars
        .get_mut(victim)
        .unwrap()
        .add_exception(plannercraft_core::calendar::CalendarException {
            name: "Chômé".to_owned(),
            period: plannercraft_core::DayRange::single(day),
            setting: DaySetting::NonWorking,
            recurrence: plannercraft_core::calendar::Recurrence::Period,
            occurrences: None,
        })
        .unwrap();
    let after = schedule(&q);
    assert!(
        after.project_finish >= before.project_finish,
        "P35, graine {seed}"
    );
    for t in &p.tasks {
        let (Some(a), Some(b)) = (before.scheduled(t.uid()), after.scheduled(t.uid())) else {
            panic!("P35, graine {seed} : tâche {} non planifiée", t.uid());
        };
        assert!(
            b.early.finish >= a.early.finish,
            "P35, graine {seed}, tâche {}",
            t.uid()
        );
        assert!(
            b.early.start >= a.early.start,
            "P35, graine {seed}, tâche {}",
            t.uid()
        );
    }
}

proptest! {
    #![proptest_config(capped(1_024))]

    #[test]
    fn p34_single_assignment_equals_j4(seed in any::<u64>()) {
        p34(&generate(seed, CONSTRAINED), seed);
        p34(&generate_from_finish(seed, CONSTRAINED), seed);
    }

    #[test]
    fn p35_resource_day_off_never_shortens(seed in any::<u64>()) {
        p35(seed);
    }
}

/// Graines fixes de P34 et P35.
#[test]
fn fixed_seeds_assignments() {
    for seed in 0..100 {
        p34(&generate(seed, GENERAL), seed);
        p34(&generate(seed, CONSTRAINED), seed);
        p34(&generate_from_finish(seed, CONSTRAINED), seed);
        p35(seed);
    }
}

// ---------------------------------------------------------------------------
// P37, P38 (J6)
// ---------------------------------------------------------------------------

/// P37 (partie moteur) : un projet sans avancement n'a aucun avancement
/// calculé, aucun diagnostic d'avancement, et la date d'état ne change rien.
fn p37(p: &Project) {
    let s = schedule(p);
    for o in s.tasks.values() {
        if let TaskOutcome::Scheduled(ts) = o {
            assert_eq!(ts.progress, None);
            assert!(!matches!(ts.driver, Driver::Actual));
        }
    }
    assert!(s.project_summary.is_none_or(|ps| ps.progress.is_none()));
    assert!(
        !s.diagnostics.iter().any(|d| d.code().starts_with("progress.")),
        "{:?}",
        s.diagnostics
    );
    let mut q = p.clone();
    let start = match p.anchor {
        ScheduleAnchor::FromStart(m) | ScheduleAnchor::FromFinish(m) => m,
    };
    q.status_date = Some(start.checked_add(Minutes::new(7 * 1_440 + 75)).unwrap());
    assert_eq!(schedule(&q), s);
}

/// Feuilles achevées tirées par P38 : ~30 % des feuilles planifiées (non
/// dégradées) de `base`, dates réelles autour de leurs dates planifiées, à
/// la minute (instants chômés compris).
fn complete_some(p: &mut Project, base: &Schedule, r: &mut Rng) -> Vec<TaskUid> {
    let mut done = Vec::new();
    for uid in p.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
        let Some(ts) = planned(base, uid) else { continue };
        if ts.role != TaskRole::Leaf || !r.chance(30) {
            continue;
        }
        let start = ts
            .start
            .checked_add(Minutes::new(r.range(-2_000, 2_000)))
            .unwrap();
        let finish = start.checked_add(Minutes::new(r.range(0, 3_000))).unwrap();
        let t = p.tasks.get_mut(uid).unwrap();
        t.progress.actual_start = Some(start);
        t.progress.actual_finish = Some(finish);
        done.push(uid);
    }
    done
}

/// Contrôles de P38 sur le planning `s` de `p`, feuilles achevées `done`.
fn p38_check(p: &Project, s: &Schedule, done: &[TaskUid], seed: u64) {
    check_shape(p, s);
    assert_eq!(
        *s,
        plannercraft_core::schedule::schedule_reference(p),
        "P38, graine {seed} : rapide ≠ référence"
    );
    for &uid in done {
        let t = p.tasks.get(uid).unwrap();
        let Some(TaskOutcome::Scheduled(ts)) = s.get(uid) else {
            continue;
        };
        let span = plannercraft_core::Span {
            start: t.progress.actual_start.unwrap(),
            finish: t.progress.actual_finish.unwrap(),
        };
        assert_eq!(
            (ts.start, ts.finish),
            (span.start, span.finish),
            "P38, graine {seed}, {uid}"
        );
        assert_eq!(ts.early, span, "P38, graine {seed}, {uid}");
        assert_eq!(ts.late, span, "P38, graine {seed}, {uid}");
        assert_eq!(ts.total_slack, Minutes::ZERO, "P38, graine {seed}, {uid}");
        assert_eq!(ts.free_slack, Minutes::ZERO, "P38, graine {seed}, {uid}");
        assert!(!ts.critical, "P38, graine {seed}, {uid}");
        assert_eq!(ts.driver, Driver::Actual, "P38, graine {seed}, {uid}");
        assert_eq!(
            ts.progress.map(|p| p.state),
            Some(plannercraft_core::ProgressState::Complete),
            "P38, graine {seed}, {uid}"
        );
    }
    // Successeurs FS sans décalage depuis une achevée : automatiques, non
    // achevés, sans contrainte qui leur impose une date (ASAP, ALAP, SNET,
    // FNET seulement), non dégradés : début au plus tôt ≥ fin réelle (à
    // rebours, les dates planifiées sont au plus tard et peuvent la
    // précéder, marge négative).
    for succ in &p.tasks {
        let Some(ts) = planned(s, succ.uid()) else {
            continue;
        };
        if ts.role != TaskRole::Leaf
            || ts.manual
            || done.contains(&succ.uid())
            || !matches!(
                succ.constraint.kind(),
                ConstraintKind::AsSoonAsPossible
                    | ConstraintKind::AsLateAsPossible
                    | ConstraintKind::StartNoEarlierThan
                    | ConstraintKind::FinishNoEarlierThan
            )
        {
            continue;
        }
        for l in &succ.predecessors {
            if l.kind != LinkType::FinishToStart || l.lag != Lag::ZERO || !done.contains(&l.predecessor) {
                continue;
            }
            // Lien coupé pour rompre un cycle (R-065, via une récapitulative).
            let cut = Diagnostic::LinkIgnoredInCycle {
                predecessor: l.predecessor,
                successor: succ.uid(),
            };
            let Some(pred) = planned(s, l.predecessor).filter(|_| !s.diagnostics.contains(&cut)) else {
                continue;
            };
            assert!(
                ts.early.start >= pred.finish,
                "P38, graine {seed} : {} commence au plus tôt avant la fin réelle de {}",
                succ.uid(),
                l.predecessor
            );
        }
    }
}

/// P38 : feuilles achevées tirées, puis éditions qui ne touchent pas leur
/// avancement (durées des autres tâches, option « respecter les
/// contraintes »).
fn p38(seed: u64, from_finish: bool) {
    let mut p = if from_finish {
        generate_from_finish(seed, CONSTRAINED)
    } else {
        generate(seed, CONSTRAINED)
    };
    let base = schedule(&p);
    let mut r = Rng((seed ^ 0x51_7CC1_B727_220A) | 1);
    let done = complete_some(&mut p, &base, &mut r);
    let s = schedule(&p);
    p38_check(&p, &s, &done, seed);
    for uid in p.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
        if done.contains(&uid) || !r.chance(40) {
            continue;
        }
        let t = p.tasks.get_mut(uid).unwrap();
        let minutes = Minutes::new(r.range(0, 4_800));
        t.duration = Duration::new(minutes, DurationUnit::Day, t.duration.basis(), false).unwrap();
    }
    if r.chance(30) {
        p.options.honor_constraints = !p.options.honor_constraints;
    }
    let s = schedule(&p);
    p38_check(&p, &s, &done, seed);
}

proptest! {
    #![proptest_config(capped(1_024))]

    #[test]
    fn p37_no_progress_unchanged(seed in any::<u64>()) {
        p37(&generate(seed, CONSTRAINED));
        p37(&generate_from_finish(seed, CONSTRAINED));
    }

    #[test]
    fn p38_completed_leaf_pinned(seed in any::<u64>()) {
        p38(seed, false);
        p38(seed, true);
    }
}

/// Graines fixes de P37 et P38.
#[test]
fn fixed_seeds_progress() {
    for seed in 0..100 {
        p37(&generate(seed, GENERAL));
        p38(seed, false);
        p38(seed, true);
    }
}

// ---------------------------------------------------------------------------
// P44 (J6)
// ---------------------------------------------------------------------------

/// Feuilles en cours tirées par P44 : ~30 % des feuilles automatiques
/// planifiées (non dégradées) de `base`, début réel autour du début planifié
/// (instants chômés compris), durée réelle de 0 à D, une reprise saisie une
/// fois sur cinq.
fn start_some(p: &mut Project, base: &Schedule, r: &mut Rng) -> Vec<TaskUid> {
    let mut running = Vec::new();
    for uid in p.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
        let Some(ts) = planned(base, uid) else { continue };
        if ts.role != TaskRole::Leaf || ts.manual || !r.chance(30) {
            continue;
        }
        let start = ts
            .start
            .checked_add(Minutes::new(r.range(-2_000, 2_000)))
            .unwrap();
        let t = p.tasks.get_mut(uid).unwrap();
        let d = t.duration.minutes().get().max(0);
        t.progress.actual_start = Some(start);
        t.progress.actual_duration = Minutes::new(r.range(0, d));
        if r.chance(20) {
            t.progress.resume = Some(start.checked_add(Minutes::new(r.range(0, 6_000))).unwrap());
        }
        running.push(uid);
    }
    running
}

/// Contrôles de P44 sur le planning `s` de `p` ; rend (début réel, `Stop`)
/// de chaque feuille en cours planifiée.
fn p44_check(
    p: &Project,
    s: &Schedule,
    running: &[TaskUid],
    seed: u64,
) -> BTreeMap<TaskUid, (Moment, Moment)> {
    check_shape(p, s);
    assert_eq!(
        *s,
        plannercraft_core::schedule::schedule_reference(p),
        "P44, graine {seed} : rapide ≠ référence"
    );
    let mut parts = BTreeMap::new();
    for &uid in running {
        let Some(ts) = planned(s, uid) else { continue };
        let t = p.tasks.get(uid).unwrap();
        let Some(pr) = ts.progress else {
            panic!("P44, graine {seed}, {uid} : avancement absent");
        };
        assert_eq!(
            pr.state,
            plannercraft_core::ProgressState::Started,
            "P44, graine {seed}, {uid}"
        );
        assert_eq!(
            pr.actual_start,
            t.progress.actual_start.unwrap(),
            "P44, graine {seed}, {uid}"
        );
        assert_eq!(ts.early.start, pr.actual_start, "P44, graine {seed}, {uid}");
        assert_eq!(ts.start, pr.actual_start, "P44, graine {seed}, {uid}");
        assert_eq!(ts.driver, Driver::Actual, "P44, graine {seed}, {uid}");
        assert!(pr.stop >= pr.actual_start, "P44, graine {seed}, {uid}");
        assert!(pr.resume >= pr.stop, "P44, graine {seed}, {uid}");
        assert!(ts.early.finish >= pr.resume, "P44, graine {seed}, {uid}");
        parts.insert(uid, (pr.actual_start, pr.stop));
    }
    parts
}

/// P44 : invariants, partie réelle stable sous d'autres éditions ; rend le
/// nombre de feuilles en cours contrôlées.
fn p44(seed: u64, from_finish: bool) -> usize {
    let mut p = if from_finish {
        generate_from_finish(seed, CONSTRAINED)
    } else {
        generate(seed, CONSTRAINED)
    };
    let base = schedule(&p);
    let mut r = Rng((seed ^ 0x2F44_A1C3_9E11_0B57) | 1);
    let running = start_some(&mut p, &base, &mut r);
    if r.chance(30) {
        p.options.progress.split_in_progress = false;
    }
    let s = schedule(&p);
    let before = p44_check(&p, &s, &running, seed);
    for uid in p.tasks.iter().map(Task::uid).collect::<Vec<_>>() {
        if running.contains(&uid) || !r.chance(40) {
            continue;
        }
        let t = p.tasks.get_mut(uid).unwrap();
        let minutes = Minutes::new(r.range(0, 4_800));
        t.duration = Duration::new(minutes, DurationUnit::Day, t.duration.basis(), false).unwrap();
    }
    if r.chance(30) {
        p.options.honor_constraints = !p.options.honor_constraints;
    }
    let s = schedule(&p);
    let after = p44_check(&p, &s, &running, seed);
    for (uid, part) in &before {
        if let Some(a) = after.get(uid) {
            assert_eq!(a, part, "P44, graine {seed}, {uid} : partie réelle déplacée");
        }
    }
    before.len()
}

/// P44 : ajouter un lien FS entrant à une feuille en cours ne recule jamais
/// sa fin (depuis le début, sans contrainte : aucune épingle ALAP).
fn p44_link(seed: u64) {
    let mut p = generate(seed, GENERAL);
    let base = schedule(&p);
    let mut r = Rng((seed ^ 0x7A44_0C1D_55E3_9B21) | 1);
    let running = start_some(&mut p, &base, &mut r);
    let s = schedule(&p);
    p44_check(&p, &s, &running, seed);
    let cycles = |s: &Schedule| {
        s.diagnostics
            .iter()
            .filter(|d| matches!(d, Diagnostic::LinkIgnoredInCycle { .. }))
            .count()
    };
    let leaves: Vec<TaskUid> = p
        .tasks
        .iter()
        .map(Task::uid)
        .filter(|&u| planned(&s, u).is_some_and(|t| t.role == TaskRole::Leaf))
        .collect();
    if leaves.is_empty() {
        return;
    }
    for &uid in &running {
        let Some(before) = planned(&s, uid) else { continue };
        let Some(&pred) =
            leaves.get(usize::try_from(r.range(0, i64::try_from(leaves.len()).unwrap() - 1)).unwrap_or(0))
        else {
            continue;
        };
        let t = p.tasks.get(uid).unwrap();
        if pred == uid || t.predecessors.iter().any(|l| l.predecessor == pred) {
            continue;
        }
        let mut q = p.clone();
        q.tasks.get_mut(uid).unwrap().predecessors.push(Link {
            predecessor: pred,
            kind: LinkType::FinishToStart,
            lag: Lag::ZERO,
        });
        let after = schedule(&q);
        if cycles(&after) != cycles(&s) {
            continue;
        }
        let Some(a) = planned(&after, uid) else { continue };
        assert!(
            a.early.finish >= before.early.finish,
            "P44, graine {seed} : lien {pred} → {uid} recule la fin"
        );
    }
}

proptest! {
    #![proptest_config(capped(1_024))]

    #[test]
    fn p44_in_progress_leaf(seed in any::<u64>()) {
        let _ = p44(seed, false) + p44(seed, true);
        p44_link(seed);
    }
}

/// Graines fixes de P44.
#[test]
fn fixed_seeds_in_progress() {
    let mut checked = 0;
    for seed in 0..100 {
        checked += p44(seed, false) + p44(seed, true);
        p44_link(seed);
    }
    // Le tirage produit bien des feuilles en cours planifiées.
    assert!(checked > 200, "{checked} feuilles en cours contrôlées");
}
