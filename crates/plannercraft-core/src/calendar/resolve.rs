//! Résolution d'un calendrier (ADR-0008, § 6.3), chemin froid.
//!
//! - [`chain`] : chaîne d'héritage et détection de ses erreurs (R-015) ;
//! - [`resolve_day`] : réglage d'un jour selon la précédence (R-016), avec sa
//!   source. Sert d'oracle indépendant aux tests de la forme compilée et
//!   d'explication (« mercredi 11/11 chômé : exception n° 0 du calendrier
//!   1 ») ;
//! - [`expand_exception`] (ré-exporté de `recurrence`) : dépliage des
//!   récurrences (R-018).

use serde::{Deserialize, Serialize};

use crate::arena::Arena;
use crate::calendar::model::{Calendar, CalendarException, CalendarKind, DayRule, DaySetting, WorkWeek};
pub use crate::calendar::recurrence::expand_exception;
use crate::error::CalendarError;
use crate::ids::CalendarUid;
use crate::time::Day;

/// Borne défensive du suivi d'une chaîne de bases dérivées.
const MAX_CHAIN_STEPS: usize = 8;

/// Chaîne d'héritage valide : `[c]` (base) ou `[c, base]` (dérivé).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarChain<'a> {
    /// Calendrier de base.
    Base(&'a Calendar),
    /// Calendrier dérivé et sa base (qui est un calendrier de base).
    Derived {
        /// Calendrier dérivé.
        calendar: &'a Calendar,
        /// Sa base.
        base: &'a Calendar,
    },
}

impl<'a> CalendarChain<'a> {
    /// Calendrier demandé.
    #[must_use]
    pub const fn calendar(&self) -> &'a Calendar {
        match *self {
            Self::Base(c) | Self::Derived { calendar: c, .. } => c,
        }
    }

    /// Calendrier de base de la chaîne (lui-même pour une base).
    #[must_use]
    pub const fn base(&self) -> &'a Calendar {
        match *self {
            Self::Base(c) | Self::Derived { base: c, .. } => c,
        }
    }
}

/// Chaîne d'héritage du calendrier `uid` (R-015).
///
/// Pour un calendrier dérivé `c` : sa base doit exister et être un calendrier
/// de base. Si la base est elle-même dérivée, la chaîne des bases est suivie
/// (au plus 8 pas) pour distinguer une boucle (`InheritanceCycle`, chemin
/// complet avec le premier UID répété en fin) d'une simple base dérivée
/// (`BaseNotBase`). Un calendrier dont la base est lui-même donne
/// `InheritanceCycle { chain: [c, c] }`.
///
/// # Errors
///
/// [`CalendarError::UnknownCalendar`], [`CalendarError::BaseMissing`],
/// [`CalendarError::BaseNotBase`], [`CalendarError::InheritanceCycle`].
pub fn chain(
    calendars: &Arena<CalendarUid, Calendar>,
    uid: CalendarUid,
) -> Result<CalendarChain<'_>, CalendarError> {
    let calendar = calendars
        .get(uid)
        .ok_or(CalendarError::UnknownCalendar { calendar: uid })?;
    let base_uid = match calendar.kind() {
        CalendarKind::Base { .. } => return Ok(CalendarChain::Base(calendar)),
        CalendarKind::Derived { base, .. } => *base,
    };
    if base_uid == uid {
        return Err(CalendarError::InheritanceCycle {
            chain: vec![uid, uid],
        });
    }
    let base = calendars.get(base_uid).ok_or(CalendarError::BaseMissing {
        calendar: uid,
        base: base_uid,
    })?;
    if base.is_base() {
        return Ok(CalendarChain::Derived { calendar, base });
    }
    // La base est dérivée : boucle ou simple base non-base ?
    let not_base = CalendarError::BaseNotBase {
        calendar: uid,
        base: base_uid,
    };
    let mut path = vec![uid, base_uid];
    let mut current = base;
    for _ in 0..MAX_CHAIN_STEPS {
        let next = match current.kind() {
            CalendarKind::Derived { base, .. } => *base,
            CalendarKind::Base { .. } => return Err(not_base),
        };
        let seen = path.contains(&next);
        path.push(next);
        if seen {
            return Err(CalendarError::InheritanceCycle { chain: path });
        }
        match calendars.get(next) {
            Some(c) => current = c,
            None => return Err(not_base),
        }
    }
    Err(not_base)
}

/// Réglage résolu d'un jour et sa provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedDay {
    /// Réglage du jour (chômé ou plages).
    pub hours: DaySetting,
    /// Niveau qui l'a donné.
    pub source: DaySource,
}

/// Provenance du réglage d'un jour (R-016).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DaySource {
    /// Exception n° `index` (ordre de stockage) du calendrier `calendar`.
    Exception {
        /// Calendrier qui porte l'exception.
        calendar: CalendarUid,
        /// Indice de l'exception.
        index: usize,
    },
    /// Semaine datée n° `index` du calendrier `calendar`.
    WorkWeek {
        /// Calendrier qui porte la semaine datée.
        calendar: CalendarUid,
        /// Indice de la semaine datée.
        index: usize,
    },
    /// Semaine par défaut du calendrier `calendar`.
    DefaultWeek {
        /// Calendrier.
        calendar: CalendarUid,
    },
}

/// Exceptions d'un calendrier dans l'ordre de précédence : début de période,
/// puis ordre de stockage (R-017).
pub(crate) fn exceptions_in_order(c: &Calendar) -> Vec<(usize, &CalendarException)> {
    let mut order: Vec<(usize, &CalendarException)> = c.exceptions().iter().enumerate().collect();
    order.sort_by_key(|(i, e)| (e.period.first(), *i));
    order
}

/// Semaines datées d'un calendrier dans l'ordre de précédence (R-019).
pub(crate) fn work_weeks_in_order(c: &Calendar) -> Vec<(usize, &WorkWeek)> {
    let mut order: Vec<(usize, &WorkWeek)> = c.work_weeks().iter().enumerate().collect();
    order.sort_by_key(|(i, w)| (w.period.first(), *i));
    order
}

/// Semaine datée qui s'applique au jour `day` : la première, dans l'ordre de
/// précédence, dont la période contient le jour. Elle possède le jour
/// entier : un jour `Inherit` y renvoie à la semaine par défaut du même
/// calendrier, jamais à une autre semaine datée (hypothèse).
pub(crate) fn work_week_owner(order: &[(usize, &WorkWeek)], day: i32) -> Option<usize> {
    order
        .iter()
        .find(|(_, w)| w.period.first().index() <= day && day <= w.period.last().index())
        .map(|(i, _)| *i)
}

/// Exception qui s'applique au jour `day` dans un calendrier (R-017) : la
/// première, dans l'ordre de précédence, dont les jours dépliés contiennent le
/// jour.
fn exception_for(c: &Calendar, day: Day) -> Result<Option<(usize, &CalendarException)>, CalendarError> {
    for (index, e) in exceptions_in_order(c) {
        if e.period.contains(day) && expand_exception(e)?.iter().any(|r| r.contains(day)) {
            return Ok(Some((index, e)));
        }
    }
    Ok(None)
}

/// Règle → réglage, `None` pour `Inherit`.
pub(crate) fn defined(rule: &DayRule) -> Option<DaySetting> {
    match rule {
        DayRule::Inherit => None,
        DayRule::NonWorking => Some(DaySetting::NonWorking),
        DayRule::Working(h) => Some(DaySetting::Working(h.clone())),
    }
}

/// Réglage du jour `day` par les semaines (datées puis par défaut) d'un
/// niveau ; `None` si ce niveau hérite.
fn from_weeks(c: &Calendar, day: Day) -> Option<ResolvedDay> {
    let wd = day.weekday();
    let calendar = c.uid();
    let order = work_weeks_in_order(c);
    if let Some(index) = work_week_owner(&order, day.index())
        && let Some(hours) = c.work_weeks().get(index).and_then(|w| defined(w.days.get(wd)))
    {
        return Some(ResolvedDay {
            hours,
            source: DaySource::WorkWeek { calendar, index },
        });
    }
    let hours = match c.kind() {
        CalendarKind::Base { week } => Some(week.get(wd).clone()),
        CalendarKind::Derived { week, .. } => defined(week.get(wd)),
    };
    hours.map(|hours| ResolvedDay {
        hours,
        source: DaySource::DefaultWeek { calendar },
    })
}

/// Réglage du jour `day` dans le calendrier `uid` (R-016). Du plus
/// prioritaire au moins prioritaire, jour de semaine par jour de semaine
/// (`Inherit` passe au niveau suivant) :
/// 1. exception du dérivé ;
/// 2. exception de la base ;
/// 3. semaine datée du dérivé ;
/// 4. semaine par défaut du dérivé ;
/// 5. semaine datée de la base ;
/// 6. semaine par défaut de la base (toujours définie).
///
/// Chemin froid : déplie les exceptions dont la période contient le jour à
/// chaque appel.
///
/// # Errors
///
/// Erreurs de [`chain`] ; [`CalendarError::Inconsistent`] si la base ne
/// donne aucun réglage (impossible : sa semaine est complète).
pub fn resolve_day(
    calendars: &Arena<CalendarUid, Calendar>,
    uid: CalendarUid,
    day: Day,
) -> Result<ResolvedDay, CalendarError> {
    let ch = chain(calendars, uid)?;
    let derived = match ch {
        CalendarChain::Base(_) => None,
        CalendarChain::Derived { calendar, .. } => Some(calendar),
    };
    let base = ch.base();
    for c in derived.into_iter().chain([base]) {
        if let Some((index, e)) = exception_for(c, day)? {
            return Ok(ResolvedDay {
                hours: e.setting.clone(),
                source: DaySource::Exception {
                    calendar: c.uid(),
                    index,
                },
            });
        }
    }
    if let Some(r) = derived.and_then(|c| from_weeks(c, day)) {
        return Ok(r);
    }
    from_weeks(base, day).ok_or(CalendarError::Inconsistent { calendar: uid })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{CalendarChain, DaySource, chain, resolve_day};
    use crate::arena::Arena;
    use crate::calendar::builtin::standard;
    use crate::calendar::model::{
        Calendar, CalendarException, DayHours, DayRule, DaySetting, Recurrence, TimeRange, WorkWeek,
    };
    use crate::error::CalendarError;
    use crate::ids::CalendarUid;
    use crate::time::{Day, DayRange, TimeOfDay, Week, Weekday};

    fn uid(v: u32) -> CalendarUid {
        CalendarUid::new(v)
    }

    fn derived(v: u32, base: u32) -> Calendar {
        Calendar::new_derived(uid(v), format!("D{v}"), uid(base))
    }

    fn arena(items: Vec<Calendar>) -> Arena<CalendarUid, Calendar> {
        Arena::try_from(items).unwrap()
    }

    #[test]
    fn valid_chains() {
        let a = arena(vec![standard(uid(1), "Standard".to_owned()), derived(4, 1)]);
        let c = chain(&a, uid(1)).unwrap();
        assert!(matches!(c, CalendarChain::Base(_)));
        assert_eq!(c.base().uid(), uid(1));
        let c = chain(&a, uid(4)).unwrap();
        assert_eq!((c.calendar().uid(), c.base().uid()), (uid(4), uid(1)));
    }

    #[test]
    fn chain_errors() {
        let a = arena(vec![
            standard(uid(1), "Standard".to_owned()),
            derived(4, 1),
            derived(5, 6),
            derived(6, 5),
            derived(7, 4),
            derived(8, 99),
            derived(9, 9),
            derived(10, 11),
            derived(11, 12),
        ]);
        assert_eq!(
            chain(&a, uid(2)).unwrap_err(),
            CalendarError::UnknownCalendar { calendar: uid(2) }
        );
        assert_eq!(
            chain(&a, uid(8)).unwrap_err(),
            CalendarError::BaseMissing {
                calendar: uid(8),
                base: uid(99)
            }
        );
        // 7 → 4 → 1 : 4 est dérivé, pas une boucle.
        assert_eq!(
            chain(&a, uid(7)).unwrap_err(),
            CalendarError::BaseNotBase {
                calendar: uid(7),
                base: uid(4)
            }
        );
        // 10 → 11 → 12 (absent) : base dérivée.
        assert_eq!(
            chain(&a, uid(10)).unwrap_err(),
            CalendarError::BaseNotBase {
                calendar: uid(10),
                base: uid(11)
            }
        );
        assert_eq!(
            chain(&a, uid(5)).unwrap_err(),
            CalendarError::InheritanceCycle {
                chain: vec![uid(5), uid(6), uid(5)]
            }
        );
        assert_eq!(
            chain(&a, uid(9)).unwrap_err(),
            CalendarError::InheritanceCycle {
                chain: vec![uid(9), uid(9)]
            }
        );
    }

    #[test]
    fn long_derived_chain_is_bounded() {
        // 20 → 21 → … → 39 → 40 (absent) : plus de 8 pas, BaseNotBase.
        let mut items = vec![];
        for v in 20..40 {
            items.push(derived(v, v + 1));
        }
        let a = arena(items);
        assert_eq!(
            chain(&a, uid(20)).unwrap_err(),
            CalendarError::BaseNotBase {
                calendar: uid(20),
                base: uid(21)
            }
        );
    }

    #[test]
    fn cycle_not_through_start() {
        // 30 → 31 → 32 → 31 : boucle qui ne repasse pas par 30.
        let a = arena(vec![derived(30, 31), derived(31, 32), derived(32, 31)]);
        assert_eq!(
            chain(&a, uid(30)).unwrap_err(),
            CalendarError::InheritanceCycle {
                chain: vec![uid(30), uid(31), uid(32), uid(31)]
            }
        );
    }

    fn day(s: &str) -> Day {
        s.parse().unwrap()
    }

    fn hours(from: u16, to: u16) -> DayHours {
        DayHours::new(vec![
            TimeRange::new(TimeOfDay::hm(from, 0).unwrap(), TimeOfDay::hm(to, 0).unwrap()).unwrap(),
        ])
        .unwrap()
    }

    fn holiday(d: &str) -> CalendarException {
        CalendarException {
            name: String::new(),
            period: DayRange::single(day(d)),
            setting: DaySetting::NonWorking,
            recurrence: Recurrence::Period,
            occurrences: None,
        }
    }

    /// (minutes du jour, source).
    fn resolved(a: &Arena<CalendarUid, Calendar>, c: u32, d: &str) -> (i64, DaySource) {
        let r = resolve_day(a, uid(c), day(d)).unwrap();
        (r.hours.total().get(), r.source)
    }

    #[test]
    fn resolve_day_sources() {
        let mut base = standard(uid(1), "Standard".to_owned());
        base.add_exception(holiday("2026-12-25")).unwrap();
        base.add_work_week(WorkWeek {
            name: "été".to_owned(),
            period: DayRange::new(day("2027-07-01"), day("2027-08-31")).unwrap(),
            days: Week::from_fn(|d| match d {
                Weekday::Saturday | Weekday::Sunday => DayRule::Inherit,
                _ => DayRule::Working(hours(8, 15)),
            }),
        })
        .unwrap();
        let mut paul = derived(4, 1);
        paul.set_day(Weekday::Friday, DayRule::Working(hours(8, 12)))
            .unwrap();
        paul.add_exception(holiday("2026-12-31")).unwrap();
        paul.add_work_week(WorkWeek {
            name: "inventaire".to_owned(),
            period: DayRange::new(day("2027-01-04"), day("2027-01-08")).unwrap(),
            days: Week::from_fn(|d| match d {
                Weekday::Monday => DayRule::NonWorking,
                _ => DayRule::Inherit,
            }),
        })
        .unwrap();
        let a = arena(vec![base, paul]);
        let default = |c| DaySource::DefaultWeek { calendar: uid(c) };
        // Base : semaine par défaut, exception, semaine datée.
        assert_eq!(resolved(&a, 1, "2026-12-21"), (480, default(1)));
        assert_eq!(
            resolved(&a, 1, "2026-12-25"),
            (
                0,
                DaySource::Exception {
                    calendar: uid(1),
                    index: 0
                }
            )
        );
        assert_eq!(
            resolved(&a, 1, "2027-07-05"),
            (
                420,
                DaySource::WorkWeek {
                    calendar: uid(1),
                    index: 0
                }
            )
        );
        // Samedi de la semaine datée : `Inherit` → semaine par défaut.
        assert_eq!(resolved(&a, 1, "2027-07-03"), (0, default(1)));
        // Dérivé : son vendredi, la base pour le lundi.
        assert_eq!(resolved(&a, 4, "2026-12-18"), (240, default(4)));
        assert_eq!(resolved(&a, 4, "2026-12-21"), (480, default(1)));
        // Exception de la base contre jour redéfini du dérivé : l'exception.
        assert_eq!(
            resolved(&a, 4, "2026-12-25"),
            (
                0,
                DaySource::Exception {
                    calendar: uid(1),
                    index: 0
                }
            )
        );
        assert_eq!(
            resolved(&a, 4, "2026-12-31"),
            (
                0,
                DaySource::Exception {
                    calendar: uid(4),
                    index: 0
                }
            )
        );
        // Semaine datée du dérivé : lundi chômé ; mardi hérité → base.
        assert_eq!(
            resolved(&a, 4, "2027-01-04"),
            (
                0,
                DaySource::WorkWeek {
                    calendar: uid(4),
                    index: 0
                }
            )
        );
        assert_eq!(resolved(&a, 4, "2027-01-05"), (480, default(1)));
        // Semaine par défaut du dérivé contre semaine datée de la base.
        assert_eq!(resolved(&a, 4, "2027-07-02"), (240, default(4)));
        assert_eq!(
            resolved(&a, 4, "2027-07-05"),
            (
                420,
                DaySource::WorkWeek {
                    calendar: uid(1),
                    index: 0
                }
            )
        );
        assert_eq!(
            resolve_day(&a, uid(9), day("2027-07-05")),
            Err(CalendarError::UnknownCalendar { calendar: uid(9) })
        );
    }

    #[test]
    fn resolve_day_overlapping_work_weeks() {
        // Lecture d'un fichier : deux semaines datées qui se chevauchent. La
        // première par date de début possède les jours communs, y compris ses
        // jours `Inherit` (renvoyés à la semaine par défaut).
        let ww = |from: &str, to: &str, rule: DayRule| WorkWeek {
            name: String::new(),
            period: DayRange::new(day(from), day(to)).unwrap(),
            days: Week::from_fn(|d| {
                if d == Weekday::Monday {
                    rule.clone()
                } else {
                    DayRule::Inherit
                }
            }),
        };
        let mut json = serde_json::to_value(standard(uid(1), "Standard".to_owned())).unwrap();
        json["work_weeks"] = serde_json::to_value(vec![
            ww("2026-11-09", "2026-11-30", DayRule::Working(hours(8, 10))),
            ww("2026-11-01", "2026-11-15", DayRule::Inherit),
        ])
        .unwrap();
        let a = arena(vec![serde_json::from_value(json).unwrap()]);
        // Lun 09/11 : semaine n° 1 (début 01/11), lundi hérité → 480.
        assert_eq!(
            resolved(&a, 1, "2026-11-09"),
            (480, DaySource::DefaultWeek { calendar: uid(1) })
        );
        // Lun 16/11 : seule la n° 0 couvre → 120.
        assert_eq!(
            resolved(&a, 1, "2026-11-16"),
            (
                120,
                DaySource::WorkWeek {
                    calendar: uid(1),
                    index: 0
                }
            )
        );
    }
}
