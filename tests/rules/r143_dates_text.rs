//! R-143 — Dates en texte : nom de jour facultatif ignoré ; JJ/MM/AAAA
//! (français) ou MM/JJ/AAAA (anglais), séparateurs `/ . -` ; année sur 4 ou
//! 2 chiffres (84 à 99 → 19xx, 00 à 83 → 20xx) ; ISO toujours admis ; heure
//! facultative, sinon l'heure par défaut du rôle (R-033).

use plannercraft_automation::text::{DateRole, Language, TextContext, TextError, format_date, parse_date};
use plannercraft_core::{Moment, ProjectOptions, TimeOfDay};
use plannercraft_tests::moment;

fn ctx(language: Language) -> TextContext {
    TextContext::for_options(&ProjectOptions::default(), language)
}

#[test]
fn r143_french_dates() {
    let fr = ctx(Language::Fr);
    // « 19/10/2026 08:00 » → lun 19/10/2026 08:00.
    assert_eq!(
        parse_date("19/10/2026 08:00", DateRole::Start, &fr),
        Ok(moment("2026-10-19T08:00"))
    );
    // « lun. 19/10/26 » en fin, sans heure → 17:00 (R-033) ; en début → 08:00.
    assert_eq!(
        parse_date("lun. 19/10/26", DateRole::Finish, &fr),
        Ok(moment("2026-10-19T17:00"))
    );
    assert_eq!(
        parse_date("lundi 19/10/26", DateRole::Start, &fr),
        Ok(moment("2026-10-19T08:00"))
    );
    // Séparateurs `.` et `-`, heure sur un chiffre.
    assert_eq!(
        parse_date("19.10.2026 8:30", DateRole::Start, &fr),
        Ok(moment("2026-10-19T08:30"))
    );
    assert_eq!(
        parse_date("19-10-2026", DateRole::Start, &fr),
        Ok(moment("2026-10-19T08:00"))
    );
    // Années sur deux chiffres : 84 → 1984, 83 → 2083.
    assert_eq!(
        parse_date("02/01/84", DateRole::Start, &fr),
        Ok(moment("1984-01-02T08:00"))
    );
    assert_eq!(
        parse_date("02/01/83", DateRole::Start, &fr),
        Ok(moment("2083-01-02T08:00"))
    );
    // Secondes tolérées et tronquées (R-004).
    assert_eq!(
        parse_date("19/10/2026 08:00:00", DateRole::Start, &fr),
        Ok(moment("2026-10-19T08:00"))
    );
}

#[test]
fn r143_english_dates() {
    let en = ctx(Language::En);
    // « 10/19/2026 5:00 PM » → 17:00 ; 12 AM = 00:00, 12 PM = 12:00.
    assert_eq!(
        parse_date("10/19/2026 5:00 PM", DateRole::Start, &en),
        Ok(moment("2026-10-19T17:00"))
    );
    assert_eq!(
        parse_date("Mon 10/19/26 12:00 am", DateRole::Start, &en),
        Ok(moment("2026-10-19T00:00"))
    );
    assert_eq!(
        parse_date("10/19/2026 12:30PM", DateRole::Start, &en),
        Ok(moment("2026-10-19T12:30"))
    );
    assert_eq!(
        parse_date("10/19/2026", DateRole::Finish, &en),
        Ok(moment("2026-10-19T17:00"))
    );
}

#[test]
fn r143_iso_always_accepted() {
    for language in [Language::Fr, Language::En] {
        let c = ctx(language);
        assert_eq!(
            parse_date("2026-10-19", DateRole::Finish, &c),
            Ok(moment("2026-10-19T17:00"))
        );
        assert_eq!(
            parse_date("2026-10-19T08:00", DateRole::Finish, &c),
            Ok(moment("2026-10-19T08:00"))
        );
        assert_eq!(
            parse_date("2026-10-19 13:15", DateRole::Finish, &c),
            Ok(moment("2026-10-19T13:15"))
        );
    }
}

#[test]
fn r143_default_time_24_00() {
    // Heure de fin par défaut 24:00 : le lendemain 00:00 (R-033).
    let options = ProjectOptions {
        default_finish_time: TimeOfDay::END_OF_DAY,
        ..ProjectOptions::default()
    };
    let c = TextContext::for_options(&options, Language::Fr);
    assert_eq!(
        parse_date("19/10/2026", DateRole::Finish, &c),
        Ok(moment("2026-10-20T00:00"))
    );
    assert_eq!(
        parse_date("31/12/2149 24:00", DateRole::Start, &c),
        Ok(Moment::MAX)
    );
}

#[test]
fn r143_refusals() {
    let fr = ctx(Language::Fr);
    assert_eq!(
        parse_date("31/02/2026", DateRole::Start, &fr),
        Err(TextError::BadDate)
    );
    assert_eq!(
        parse_date("19/13/2026", DateRole::Start, &fr),
        Err(TextError::BadDate)
    );
    assert_eq!(
        parse_date("19/10/2026 25:00", DateRole::Start, &fr),
        Err(TextError::BadDate)
    );
    assert_eq!(
        parse_date("19/10/2026 08:60", DateRole::Start, &fr),
        Err(TextError::BadDate)
    );
    assert_eq!(
        parse_date("19/10-2026", DateRole::Start, &fr),
        Err(TextError::BadDate)
    );
    assert_eq!(
        parse_date("19/10/202", DateRole::Start, &fr),
        Err(TextError::BadDate)
    );
    assert_eq!(
        parse_date("jour 19/10/2026", DateRole::Start, &fr),
        Err(TextError::BadDate)
    );
    assert_eq!(
        parse_date("19/10/2026 13:00 PM", DateRole::Start, &fr),
        Err(TextError::BadDate)
    );
    assert_eq!(
        parse_date("19/10/2026 x", DateRole::Start, &fr),
        Err(TextError::BadDate)
    );
    assert_eq!(parse_date("", DateRole::Start, &fr), Err(TextError::Empty));
    // Hors du domaine (1984 à 2149, R-003).
    assert_eq!(
        parse_date("31/12/1983", DateRole::Start, &fr),
        Err(TextError::OutOfRange)
    );
    assert_eq!(
        parse_date("2150-01-02", DateRole::Start, &fr),
        Err(TextError::OutOfRange)
    );
}

#[test]
fn r143_writing() {
    let m = moment("2026-10-19T08:00");
    assert_eq!(format_date(m, &ctx(Language::Fr)), "19/10/2026 08:00");
    assert_eq!(format_date(m, &ctx(Language::En)), "10/19/2026 08:00");
    assert_eq!(
        format_date(moment("1984-01-01T00:00"), &ctx(Language::Fr)),
        "01/01/1984 00:00"
    );
    // Fin du domaine : « 31/12/2149 24:00 », relue à l'identique.
    assert_eq!(format_date(Moment::MAX, &ctx(Language::Fr)), "31/12/2149 24:00");
    for language in [Language::Fr, Language::En] {
        let c = ctx(language);
        for m in [
            m,
            Moment::MAX,
            moment("1984-01-01T00:00"),
            moment("2149-12-31T23:59"),
        ] {
            assert_eq!(parse_date(&format_date(m, &c), DateRole::Start, &c), Ok(m));
        }
    }
}
