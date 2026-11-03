//! Échelle de temps et placement d'une durée (ADR-0008, § 6.6, R-030 et
//! R-031).
//!
//! Le CPM (J2) n'appelle que [`TimeScale::forward_span`],
//! [`TimeScale::backward_span`], [`TimeScale::add`] et
//! [`TimeScale::between`] : les conventions de début, de fin et de jalon ne
//! vivent qu'ici.

use serde::{Deserialize, Serialize};

use crate::calendar::Direction;
use crate::calendar::compiled::CompiledCalendar;
use crate::duration::Minutes;
use crate::error::{CalendarError, TimeError};
use crate::time::Moment;

/// Échelle sur laquelle une durée se consomme : temps ouvré d'un calendrier
/// compilé, ou temps écoulé (civil, R-028).
#[derive(Clone, Copy, Debug)]
pub enum TimeScale<'a> {
    /// Temps ouvré du calendrier.
    Working(&'a CompiledCalendar),
    /// Temps civil, 24 h sur 24 et 7 j sur 7.
    Elapsed,
}

/// Placement d'une durée : début et fin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    /// Début.
    pub start: Moment,
    /// Fin.
    pub finish: Moment,
}

/// Temps civil : `t + d`, l'erreur de domaine portant le sens du dépassement.
fn civil(t: Moment, d: Minutes) -> Result<Moment, CalendarError> {
    t.checked_add(d).map_err(|e| match e {
        TimeError::OutOfDomain | TimeError::Overflow if d.is_negative() => {
            CalendarError::ElapsedOutOfDomain {
                direction: Direction::Before,
            }
        }
        _ => CalendarError::ElapsedOutOfDomain {
            direction: Direction::After,
        },
    })
}

impl TimeScale<'_> {
    /// Instant à `d` minutes de `t` (signé) : `add_working` (R-021 à R-023)
    /// ou temps civil (R-028).
    ///
    /// # Errors
    ///
    /// Erreurs de [`CompiledCalendar::add_working`] ;
    /// [`CalendarError::ElapsedOutOfDomain`] en temps écoulé.
    pub fn add(&self, t: Moment, d: Minutes) -> Result<Moment, CalendarError> {
        match self {
            Self::Working(c) => c.add_working(t, d),
            Self::Elapsed => civil(t, d),
        }
    }

    /// Minutes de `a` à `b` sur l'échelle (signé).
    ///
    /// # Errors
    ///
    /// Erreurs de [`CompiledCalendar::working_between`] (jamais attendues).
    pub fn between(&self, a: Moment, b: Moment) -> Result<Minutes, CalendarError> {
        match self {
            Self::Working(c) => c.working_between(a, b),
            Self::Elapsed => Ok(a.minutes_until(b)),
        }
    }

    /// Passe avant : placement d'une durée `d >= 0` au plus tôt à partir de
    /// `from`.
    /// - temps ouvré, `d > 0` (R-030) : début = instant ouvré suivant `from`,
    ///   fin = début + d (R-021) ;
    /// - temps ouvré, `d = 0` (R-031, jalon) : `from` s'il termine une minute
    ///   ouvrée ou se trouve dans une plage (`previous_working_moment(from)
    ///   == from`), sinon l'instant ouvré suivant ; début = fin ;
    /// - temps écoulé : début = `from`, fin = `from + d`, sans recalage.
    ///
    /// # Errors
    ///
    /// [`CalendarError::NegativeSpan`] si `d < 0` ; erreurs des conversions
    /// (`NoWorkingTime`, `OutOfDomain`, `ElapsedOutOfDomain`).
    pub fn forward_span(&self, from: Moment, d: Minutes) -> Result<Span, CalendarError> {
        if d.is_negative() {
            return Err(CalendarError::NegativeSpan);
        }
        match self {
            Self::Elapsed => Ok(Span {
                start: from,
                finish: civil(from, d)?,
            }),
            Self::Working(c) if d == Minutes::ZERO => {
                let probe = c.probe(from)?;
                let t = if c.previous_of(probe) == Ok(from) {
                    from
                } else {
                    c.next_of(probe)?
                };
                Ok(Span { start: t, finish: t })
            }
            Self::Working(c) => {
                // `next_working_moment` puis `add_working`, fusionnés.
                let (start, finish) = c.span_after(from, d.get())?;
                Ok(Span { start, finish })
            }
        }
    }

    /// Dernier instant `x ≤ y` dont le placement au plus tôt commence au plus
    /// tard en `y` (en `Moment` brut) : `forward_span(x, d).start ≤ y`, avec
    /// `x` le plus grand possible (R-047). Sert à convertir un plafond de
    /// début, qui peut tomber hors du temps ouvré de l'échelle (décalage
    /// écoulé, successeur sur un autre calendrier, plafond hérité d'une
    /// récapitulative) :
    /// - temps écoulé : `y` ;
    /// - temps ouvré, `d > 0` : `y` s'il commence une minute ouvrée
    ///   (`next_working_moment(y) == y`), sinon le début de la dernière minute
    ///   ouvrée avant `y` (`previous_working_moment(y) − 1 min`) ;
    /// - temps ouvré, `d = 0` (jalon, R-031) : `y` s'il termine une minute
    ///   ouvrée ou en commence une, sinon `previous_working_moment(y)`.
    ///
    /// Exemple (Standard) : `y` = mer 22:00, `d` = 480 → mer 16:59 (placé à
    /// jeu 08:00, la tâche commencerait après `y`).
    ///
    /// # Errors
    ///
    /// [`CalendarError::NegativeSpan`] si `d < 0` ; erreurs des conversions
    /// (aucun temps ouvré avant `y` : `OutOfDomain { Before }`).
    pub fn latest_start(&self, y: Moment, d: Minutes) -> Result<Moment, CalendarError> {
        if d.is_negative() {
            return Err(CalendarError::NegativeSpan);
        }
        let Self::Working(c) = self else { return Ok(y) };
        let probe = c.probe(y)?;
        if c.next_of(probe) == Ok(y) {
            return Ok(y);
        }
        let previous = c.previous_of(probe)?;
        if d == Minutes::ZERO {
            return Ok(previous);
        }
        previous
            .checked_add(Minutes::new(-1))
            .map_err(|_| CalendarError::OutOfDomain {
                calendar: c.uid(),
                direction: Direction::Before,
            })
    }

    /// Passe arrière, symétrique : placement d'une durée `d >= 0` au plus tard
    /// avant `to`.
    /// - temps ouvré, `d > 0` (R-030) : fin = instant ouvré précédant `to`,
    ///   début = fin − d (R-022) ;
    /// - temps ouvré, `d = 0` (R-031, jalon) : `to` si sa minute est ouvrée ou
    ///   s'il commence une plage (`next_working_moment(to) == to`), sinon
    ///   l'instant ouvré précédent ; début = fin ;
    /// - temps écoulé : fin = `to`, début = `to − d`.
    ///
    /// # Errors
    ///
    /// [`CalendarError::NegativeSpan`] si `d < 0` ; erreurs des conversions.
    pub fn backward_span(&self, to: Moment, d: Minutes) -> Result<Span, CalendarError> {
        let Some(back) = d.checked_neg().filter(|_| !d.is_negative()) else {
            return Err(CalendarError::NegativeSpan);
        };
        match self {
            Self::Elapsed => Ok(Span {
                start: civil(to, back)?,
                finish: to,
            }),
            Self::Working(c) if d == Minutes::ZERO => {
                let probe = c.probe(to)?;
                let t = if c.next_of(probe) == Ok(to) {
                    to
                } else {
                    c.previous_of(probe)?
                };
                Ok(Span { start: t, finish: t })
            }
            Self::Working(c) => {
                // `previous_working_moment` puis `add_working(−d)`, fusionnés.
                let (start, finish) = c.span_before(to, d.get())?;
                Ok(Span { start, finish })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Span, TimeScale};
    use crate::arena::Arena;
    use crate::calendar::Direction;
    use crate::calendar::builtin::standard;
    use crate::calendar::compiled::CompiledCalendar;
    use crate::duration::Minutes;
    use crate::error::CalendarError;
    use crate::ids::CalendarUid;
    use crate::time::Moment;

    fn m(s: &str) -> Moment {
        s.parse().unwrap()
    }

    fn std_cal() -> CompiledCalendar {
        let a = Arena::try_from(vec![standard(CalendarUid::new(1), "Standard".to_owned())]).unwrap();
        CompiledCalendar::compile(&a, CalendarUid::new(1)).unwrap()
    }

    #[test]
    fn negative_span_refused() {
        let c = std_cal();
        for scale in [TimeScale::Working(&c), TimeScale::Elapsed] {
            let t = m("2026-10-19T08:00");
            assert_eq!(
                scale.forward_span(t, Minutes::new(-1)),
                Err(CalendarError::NegativeSpan)
            );
            assert_eq!(
                scale.backward_span(t, Minutes::new(-1)),
                Err(CalendarError::NegativeSpan)
            );
            assert_eq!(
                scale.backward_span(t, Minutes::new(i64::MIN)),
                Err(CalendarError::NegativeSpan)
            );
        }
    }

    #[test]
    fn elapsed_scale() {
        let e = TimeScale::Elapsed;
        let t = m("2026-10-24T10:00");
        assert_eq!(e.add(t, Minutes::new(60)), Ok(m("2026-10-24T11:00")));
        assert_eq!(e.between(t, m("2026-10-25T10:00")), Ok(Minutes::new(1_440)));
        assert_eq!(
            e.backward_span(t, Minutes::new(1_440)),
            Ok(Span {
                start: m("2026-10-23T10:00"),
                finish: t
            })
        );
        assert_eq!(
            e.add(Moment::MAX, Minutes::new(1)),
            Err(CalendarError::ElapsedOutOfDomain {
                direction: Direction::After
            })
        );
        assert_eq!(
            e.add(Moment::MIN, Minutes::new(-1)),
            Err(CalendarError::ElapsedOutOfDomain {
                direction: Direction::Before
            })
        );
        assert_eq!(
            e.add(Moment::MIN, Minutes::new(i64::MAX)),
            Err(CalendarError::ElapsedOutOfDomain {
                direction: Direction::After
            })
        );
        assert_eq!(
            e.forward_span(Moment::MAX, Minutes::new(1)),
            Err(CalendarError::ElapsedOutOfDomain {
                direction: Direction::After
            })
        );
    }

    #[test]
    fn working_scale_delegates() {
        let c = std_cal();
        let w = TimeScale::Working(&c);
        let t = m("2026-10-19T08:00");
        assert_eq!(w.add(t, Minutes::new(480)), Ok(m("2026-10-19T17:00")));
        assert_eq!(w.between(t, m("2026-10-20T08:00")), Ok(Minutes::new(480)));
        // Fin de domaine : passe avant impossible.
        assert_eq!(
            w.forward_span(m("2149-12-31T17:00"), Minutes::new(60)),
            Err(CalendarError::OutOfDomain {
                calendar: CalendarUid::new(1),
                direction: Direction::After
            })
        );
        // Lun 1984-01-02 08:00 commence une plage : le jalon y reste ; à
        // 07:00, il faudrait reculer avant la première minute ouvrée.
        let first = m("1984-01-02T08:00");
        assert_eq!(
            w.backward_span(first, Minutes::ZERO),
            Ok(Span {
                start: first,
                finish: first
            })
        );
        assert_eq!(
            w.backward_span(m("1984-01-02T07:00"), Minutes::ZERO),
            Err(CalendarError::OutOfDomain {
                calendar: CalendarUid::new(1),
                direction: Direction::Before
            })
        );
    }

    #[test]
    fn latest_start_never_places_after_the_cap() {
        let c = std_cal();
        let w = TimeScale::Working(&c);
        let day = Minutes::new(480);
        // Mer 22:00 hors temps ouvré : dernière minute ouvrée mer 16:59 ;
        // placée là, la tâche commence à 16:59 ≤ 22:00.
        assert_eq!(
            w.latest_start(m("2026-10-21T22:00"), day),
            Ok(m("2026-10-21T16:59"))
        );
        // Jeu 01:00 : même réponse ; jeu 08:00 commence une minute ouvrée.
        assert_eq!(
            w.latest_start(m("2026-10-22T01:00"), day),
            Ok(m("2026-10-21T16:59"))
        );
        assert_eq!(
            w.latest_start(m("2026-10-22T08:00"), day),
            Ok(m("2026-10-22T08:00"))
        );
        // Pause de midi : 12:30 → 11:59.
        assert_eq!(
            w.latest_start(m("2026-10-22T12:30"), day),
            Ok(m("2026-10-22T11:59"))
        );
        for y in ["2026-10-21T22:00", "2026-10-22T12:30", "2026-10-24T10:00"] {
            let start = w.latest_start(m(y), day).unwrap();
            assert!(w.forward_span(start, day).unwrap().start <= m(y), "{y}");
            let next = start.checked_add(Minutes::new(1)).unwrap();
            assert!(w.forward_span(next, day).unwrap().start > m(y), "{y}");
        }
        // Jalon : 17:00 termine une minute ouvrée, 08:00 en commence une ;
        // 22:00 → 17:00 (R-031).
        let zero = Minutes::ZERO;
        assert_eq!(
            w.latest_start(m("2026-10-21T17:00"), zero),
            Ok(m("2026-10-21T17:00"))
        );
        assert_eq!(
            w.latest_start(m("2026-10-22T08:00"), zero),
            Ok(m("2026-10-22T08:00"))
        );
        assert_eq!(
            w.latest_start(m("2026-10-21T22:00"), zero),
            Ok(m("2026-10-21T17:00"))
        );
        // Temps écoulé : identité ; durée négative refusée.
        let cap = m("2026-10-21T22:00");
        assert_eq!(TimeScale::Elapsed.latest_start(cap, day), Ok(cap));
        assert_eq!(
            w.latest_start(cap, Minutes::new(-1)),
            Err(CalendarError::NegativeSpan)
        );
        // Aucun temps ouvré avant : erreur, jamais de panique.
        assert!(w.latest_start(m("1984-01-02T07:00"), day).is_err());
    }
}
