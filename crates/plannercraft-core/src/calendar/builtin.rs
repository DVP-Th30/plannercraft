//! Calendriers fournis dans tout projet neuf (ADR-0008, § 6.2, R-012 à R-014).
//!
//! | Calendrier | Définition | Minutes par semaine |
//! |---|---|---|
//! | Standard | lun-ven [08:00,12:00) et [13:00,17:00) ; sam, dim chômés | 2 400 |
//! | 24 heures | tous les jours [00:00,24:00) | 10 080 |
//! | Équipe de nuit | dim chômé ; lun [23:00,24:00) ; mar-ven [00:00,03:00), [04:00,08:00), [23:00,24:00) ; sam [00:00,03:00), [04:00,08:00) | 2 400 |
//!
//! Sources : support.microsoft.com, « Project Information dialog box » (S6)
//! et « Create a new base calendar » (S7). Découpage par jour de l'Équipe de
//! nuit (la plage de 23:00 appartient au jour qui commence) : hypothèse.

use crate::calendar::model::{Calendar, DayHours, DaySetting, TimeRange};
use crate::ids::CalendarUid;
use crate::time::{TimeOfDay, Week, Weekday};

/// Noms des calendriers fournis, choisis par l'appelant (langue).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinCalendarNames {
    /// Nom du calendrier Standard.
    pub standard: String,
    /// Nom du calendrier 24 heures.
    pub twenty_four_hours: String,
    /// Nom du calendrier Équipe de nuit.
    pub night_shift: String,
}

impl Default for BuiltinCalendarNames {
    /// Noms français : « Standard », « 24 heures », « Équipe de nuit ».
    fn default() -> Self {
        Self {
            standard: "Standard".to_owned(),
            twenty_four_hours: "24 heures".to_owned(),
            night_shift: "Équipe de nuit".to_owned(),
        }
    }
}

/// Jour ouvré depuis des plages en heures entières `(début, fin)`.
///
/// Les données ci-dessous sont fixes et valides (vérifiées par les tests) ;
/// une plage invalide donnerait un jour chômé plutôt qu'une panique.
fn working(hours: &[(u16, u16)]) -> DaySetting {
    let ranges: Option<Vec<TimeRange>> = hours
        .iter()
        .map(|&(from, to)| {
            let from = TimeOfDay::hm(from, 0).ok()?;
            let to = TimeOfDay::hm(to, 0).ok()?;
            TimeRange::new(from, to).ok()
        })
        .collect();
    ranges
        .and_then(|r| DayHours::new(r).ok())
        .map_or(DaySetting::NonWorking, DaySetting::Working)
}

/// Standard : lundi à vendredi 08:00-12:00 et 13:00-17:00 (S6, S7).
#[must_use]
pub fn standard(uid: CalendarUid, name: String) -> Calendar {
    let week = Week::from_fn(|d| match d {
        Weekday::Saturday | Weekday::Sunday => DaySetting::NonWorking,
        Weekday::Monday | Weekday::Tuesday | Weekday::Wednesday | Weekday::Thursday | Weekday::Friday => {
            working(&[(8, 12), (13, 17)])
        }
    });
    Calendar::new_base(uid, name, week)
}

/// 24 heures : tous les jours 00:00-24:00 (S6).
#[must_use]
pub fn twenty_four_hours(uid: CalendarUid, name: String) -> Calendar {
    Calendar::new_base(uid, name, Week::from_fn(|_| working(&[(0, 24)])))
}

/// Équipe de nuit : du lundi soir au samedi matin, 23:00-08:00 avec une pause
/// de 03:00 à 04:00 (S6), découpée par jour civil (hypothèse).
#[must_use]
pub fn night_shift(uid: CalendarUid, name: String) -> Calendar {
    let week = Week::from_fn(|d| match d {
        Weekday::Sunday => DaySetting::NonWorking,
        Weekday::Monday => working(&[(23, 24)]),
        Weekday::Tuesday | Weekday::Wednesday | Weekday::Thursday | Weekday::Friday => {
            working(&[(0, 3), (4, 8), (23, 24)])
        }
        Weekday::Saturday => working(&[(0, 3), (4, 8)]),
    });
    Calendar::new_base(uid, name, week)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::panic,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{BuiltinCalendarNames, night_shift, standard, twenty_four_hours};
    use crate::calendar::model::{Calendar, CalendarKind, DaySetting, TimeRange};
    use crate::ids::CalendarUid;
    use crate::time::{TimeOfDay, Week, Weekday};

    fn week(c: &Calendar) -> &Week<DaySetting> {
        match c.kind() {
            CalendarKind::Base { week } => week,
            CalendarKind::Derived { .. } => panic!("calendrier de base attendu"),
        }
    }

    /// Plages d'un jour en (heure, minute) de début et de fin.
    fn ranges(c: &Calendar, d: Weekday) -> Vec<(u16, u16)> {
        match week(c).get(d) {
            DaySetting::NonWorking => vec![],
            DaySetting::Working(h) => h
                .ranges()
                .iter()
                .map(|r: &TimeRange| (r.from().minutes(), r.to().minutes()))
                .collect(),
        }
    }

    fn total(c: &Calendar) -> i64 {
        Weekday::ALL.iter().map(|d| week(c).get(*d).total().get()).sum()
    }

    fn h(v: u16) -> u16 {
        TimeOfDay::hm(v, 0).unwrap().minutes()
    }

    #[test]
    fn standard_calendar() {
        let c = standard(CalendarUid::new(1), "Standard".to_owned());
        assert!(c.is_base());
        assert_eq!(c.uid(), CalendarUid::new(1));
        assert_eq!(total(&c), 2_400);
        for d in [
            Weekday::Monday,
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
        ] {
            assert_eq!(ranges(&c, d), [(h(8), h(12)), (h(13), h(17))]);
        }
        assert_eq!(ranges(&c, Weekday::Saturday), []);
        assert_eq!(ranges(&c, Weekday::Sunday), []);
        assert!(c.exceptions().is_empty() && c.work_weeks().is_empty());
    }

    #[test]
    fn twenty_four_hours_calendar() {
        let c = twenty_four_hours(CalendarUid::new(2), "24 heures".to_owned());
        assert_eq!(total(&c), 10_080);
        for d in Weekday::ALL {
            assert_eq!(ranges(&c, d), [(0, 1_440)]);
        }
    }

    #[test]
    fn night_shift_calendar() {
        let c = night_shift(CalendarUid::new(3), "Équipe de nuit".to_owned());
        // 60 (lundi) + 4 × 480 (mardi à vendredi) + 420 (samedi) = 2 400.
        assert_eq!(total(&c), 2_400);
        assert_eq!(ranges(&c, Weekday::Sunday), []);
        assert_eq!(ranges(&c, Weekday::Monday), [(h(23), h(24))]);
        for d in [
            Weekday::Tuesday,
            Weekday::Wednesday,
            Weekday::Thursday,
            Weekday::Friday,
        ] {
            assert_eq!(ranges(&c, d), [(0, h(3)), (h(4), h(8)), (h(23), h(24))]);
        }
        assert_eq!(ranges(&c, Weekday::Saturday), [(0, h(3)), (h(4), h(8))]);
    }

    #[test]
    fn default_names_are_french() {
        let n = BuiltinCalendarNames::default();
        assert_eq!(
            (
                n.standard.as_str(),
                n.twenty_four_hours.as_str(),
                n.night_shift.as_str()
            ),
            ("Standard", "24 heures", "Équipe de nuit")
        );
    }
}
