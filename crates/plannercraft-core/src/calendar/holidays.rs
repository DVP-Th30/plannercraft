//! Jours fériés légaux (R-274) : liste réutilisable, sans interface ni
//! horloge. Seul ensemble fourni : les onze fêtes légales de la France
//! métropolitaine (Code du travail, article L3133-1), hors Alsace-Moselle
//! (Vendredi saint, 26 décembre) et outre-mer (abolition de l'esclavage,
//! fêtes locales).
//!
//! Pâques est calculé par l'algorithme grégorien anonyme (Meeus, Jones,
//! Butcher), en entiers ; les fêtes mobiles s'en déduisent (lundi de Pâques
//! = Pâques + 1, Ascension = Pâques + 39, lundi de Pentecôte = Pâques + 50).

use chrono::NaiveDate;

use crate::time::Day;

/// Fête légale de la France métropolitaine (article L3133-1 du Code du
/// travail).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FrenchHoliday {
    /// 1er janvier.
    NewYear,
    /// Lundi de Pâques.
    EasterMonday,
    /// 1er mai.
    LabourDay,
    /// 8 mai.
    VictoryDay,
    /// Ascension (jeudi, Pâques + 39 jours).
    Ascension,
    /// Lundi de Pentecôte (Pâques + 50 jours).
    WhitMonday,
    /// 14 juillet.
    NationalDay,
    /// Assomption (15 août).
    Assumption,
    /// Toussaint (1er novembre).
    AllSaints,
    /// 11 novembre.
    Armistice,
    /// Noël (25 décembre).
    Christmas,
}

impl FrenchHoliday {
    /// Les onze fêtes, dans l'ordre de l'année (fêtes mobiles à leur place
    /// habituelle).
    pub const ALL: [Self; 11] = [
        Self::NewYear,
        Self::EasterMonday,
        Self::LabourDay,
        Self::VictoryDay,
        Self::Ascension,
        Self::WhitMonday,
        Self::NationalDay,
        Self::Assumption,
        Self::AllSaints,
        Self::Armistice,
        Self::Christmas,
    ];

    /// Nom français.
    #[must_use]
    pub const fn name_fr(self) -> &'static str {
        match self {
            Self::NewYear => "Jour de l'an",
            Self::EasterMonday => "Lundi de Pâques",
            Self::LabourDay => "Fête du Travail",
            Self::VictoryDay => "Victoire 1945",
            Self::Ascension => "Ascension",
            Self::WhitMonday => "Lundi de Pentecôte",
            Self::NationalDay => "Fête nationale",
            Self::Assumption => "Assomption",
            Self::AllSaints => "Toussaint",
            Self::Armistice => "Armistice 1918",
            Self::Christmas => "Noël",
        }
    }

    /// Nom anglais.
    #[must_use]
    pub const fn name_en(self) -> &'static str {
        match self {
            Self::NewYear => "New Year's Day",
            Self::EasterMonday => "Easter Monday",
            Self::LabourDay => "Labour Day",
            Self::VictoryDay => "Victory in Europe Day",
            Self::Ascension => "Ascension Day",
            Self::WhitMonday => "Whit Monday",
            Self::NationalDay => "Bastille Day",
            Self::Assumption => "Assumption Day",
            Self::AllSaints => "All Saints' Day",
            Self::Armistice => "Armistice Day",
            Self::Christmas => "Christmas Day",
        }
    }

    /// Jour de la fête l'année `year` (`None` hors du domaine des dates,
    /// 1984 à 2149).
    #[must_use]
    pub fn day(self, year: i32) -> Option<Day> {
        let fixed = |month: u32, day: u32| NaiveDate::from_ymd_opt(year, month, day);
        let date = match self {
            Self::NewYear => fixed(1, 1),
            Self::LabourDay => fixed(5, 1),
            Self::VictoryDay => fixed(5, 8),
            Self::NationalDay => fixed(7, 14),
            Self::Assumption => fixed(8, 15),
            Self::AllSaints => fixed(11, 1),
            Self::Armistice => fixed(11, 11),
            Self::Christmas => fixed(12, 25),
            Self::EasterMonday => return easter_offset(year, 1),
            Self::Ascension => return easter_offset(year, 39),
            Self::WhitMonday => return easter_offset(year, 50),
        };
        Day::from_naive(date?).ok()
    }
}

/// Dimanche de Pâques du calendrier grégorien (algorithme anonyme de
/// Meeus, Jones et Butcher), en entiers ; `None` pour une année négative
/// ou hors des dates de chrono.
#[must_use]
#[allow(
    clippy::many_single_char_names,
    reason = "noms de l'algorithme publié, pour le relire ligne à ligne"
)]
pub fn easter_sunday(year: i32) -> Option<NaiveDate> {
    let y = i64::from(year);
    if y < 1583 {
        // Avant la réforme grégorienne : l'algorithme ne vaut pas.
        return None;
    }
    let a = y.checked_rem(19)?;
    let b = y.checked_div(100)?;
    let c = y.checked_rem(100)?;
    let d = b.checked_div(4)?;
    let e = b.checked_rem(4)?;
    let f = b.checked_add(8)?.checked_div(25)?;
    let g = b.checked_sub(f)?.checked_add(1)?.checked_div(3)?;
    let h = a
        .checked_mul(19)?
        .checked_add(b)?
        .checked_sub(d)?
        .checked_sub(g)?
        .checked_add(15)?
        .checked_rem_euclid(30)?;
    let i = c.checked_div(4)?;
    let k = c.checked_rem(4)?;
    let l = e
        .checked_mul(2)?
        .checked_add(i.checked_mul(2)?)?
        .checked_add(32)?
        .checked_sub(h)?
        .checked_sub(k)?
        .checked_rem_euclid(7)?;
    let m = a
        .checked_add(h.checked_mul(11)?)?
        .checked_add(l.checked_mul(22)?)?
        .checked_div(451)?;
    let n = h
        .checked_add(l)?
        .checked_sub(m.checked_mul(7)?)?
        .checked_add(114)?;
    let month = u32::try_from(n.checked_div(31)?).ok()?;
    let day = u32::try_from(n.checked_rem(31)?.checked_add(1)?).ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

/// Pâques + `days` jours, comme [`Day`].
fn easter_offset(year: i32, days: i32) -> Option<Day> {
    Day::from_naive(easter_sunday(year)?)
        .ok()?
        .checked_add_days(days)
        .ok()
}

/// Les fêtes légales de l'année `year`, triées par jour (une fête hors du
/// domaine des dates est omise).
#[must_use]
pub fn french_public_holidays(year: i32) -> Vec<(Day, FrenchHoliday)> {
    let mut out: Vec<(Day, FrenchHoliday)> = FrenchHoliday::ALL
        .iter()
        .filter_map(|h| h.day(year).map(|d| (d, *h)))
        .collect();
    out.sort();
    out
}

/// Les fêtes légales comprises entre `first` et `last` (inclus), triées
/// par jour ; vide si `last < first`. Années parcourues bornées par le
/// domaine des dates (166 au plus).
#[must_use]
pub fn french_public_holidays_between(first: Day, last: Day) -> Vec<(Day, FrenchHoliday)> {
    use chrono::Datelike as _;
    if last < first {
        return Vec::new();
    }
    let (y0, y1) = (first.to_naive().year(), last.to_naive().year());
    (y0..=y1)
        .flat_map(french_public_holidays)
        .filter(|(d, _)| *d >= first && *d <= last)
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    fn day(s: &str) -> Day {
        s.parse().unwrap()
    }

    /// R-274 : Pâques sur des dates publiées (calendrier grégorien), dont
    /// le plus tardif du siècle (25 avril 2038) et le 23 mars 2008.
    #[test]
    fn r274_easter_sunday_known_dates() {
        let cases = [
            (1984, "1984-04-22"),
            (2000, "2000-04-23"),
            (2008, "2008-03-23"),
            (2011, "2011-04-24"),
            (2019, "2019-04-21"),
            (2024, "2024-03-31"),
            (2025, "2025-04-20"),
            (2026, "2026-04-05"),
            (2027, "2027-03-28"),
            (2028, "2028-04-16"),
            (2029, "2029-04-01"),
            (2030, "2030-04-21"),
            (2038, "2038-04-25"),
        ];
        for (year, expected) in cases {
            let got = easter_sunday(year).unwrap();
            assert_eq!(got.to_string(), expected, "Pâques {year}");
        }
        assert_eq!(easter_sunday(1500), None);
    }

    /// R-274 : onze fêtes par an, mobiles déduites de Pâques ; ordre par
    /// date ; plage à cheval sur deux ans.
    #[test]
    fn r274_french_public_holidays_of_a_year() {
        let h = french_public_holidays(2027);
        let got: Vec<(String, FrenchHoliday)> = h.iter().map(|(d, k)| (d.to_string(), *k)).collect();
        let expected = [
            ("2027-01-01", FrenchHoliday::NewYear),
            ("2027-03-29", FrenchHoliday::EasterMonday),
            ("2027-05-01", FrenchHoliday::LabourDay),
            ("2027-05-06", FrenchHoliday::Ascension),
            ("2027-05-08", FrenchHoliday::VictoryDay),
            ("2027-05-17", FrenchHoliday::WhitMonday),
            ("2027-07-14", FrenchHoliday::NationalDay),
            ("2027-08-15", FrenchHoliday::Assumption),
            ("2027-11-01", FrenchHoliday::AllSaints),
            ("2027-11-11", FrenchHoliday::Armistice),
            ("2027-12-25", FrenchHoliday::Christmas),
        ];
        let expected: Vec<(String, FrenchHoliday)> =
            expected.iter().map(|(d, k)| ((*d).to_owned(), *k)).collect();
        assert_eq!(got, expected);
        // 2008 : Ascension et 1er mai le même jour, gardés tous deux.
        let h = french_public_holidays(2008);
        assert_eq!(h.len(), 11);
        assert_eq!(h.iter().filter(|(d, _)| *d == day("2008-05-01")).count(), 2);
        let span = french_public_holidays_between(day("2027-11-02"), day("2028-01-02"));
        let names: Vec<FrenchHoliday> = span.iter().map(|(_, k)| *k).collect();
        assert_eq!(
            names,
            [
                FrenchHoliday::Armistice,
                FrenchHoliday::Christmas,
                FrenchHoliday::NewYear
            ]
        );
        assert_eq!(
            french_public_holidays_between(day("2027-02-01"), day("2027-01-01")).len(),
            0
        );
        // Bord du domaine : rien hors de 1984 à 2149.
        assert_eq!(french_public_holidays(2149).len(), 11);
        assert_eq!(french_public_holidays(2150).len(), 0);
        for k in FrenchHoliday::ALL {
            assert!(!k.name_fr().is_empty() && !k.name_en().is_empty());
        }
    }
}
