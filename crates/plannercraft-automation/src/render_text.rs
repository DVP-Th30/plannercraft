//! Textes du planning pour le rendu (règle 3, ADR-0016 D7) : implémentation
//! de [`BarTextSource`] sur une session. Les noms, ressources, dates,
//! durées et pourcentages autour des barres, dans l'en-tête et le pied de
//! page sont formatés ici, à partir du modèle et du planning du moteur ;
//! `plannercraft-render` et `plannercraft-pdf` ne font que les placer.
//!
//! Formes (hypothèses H-R, `docs/features/render.md`) : dates sans heure
//! (`JJ/MM/AAAA`, `MM/DD/YYYY` en anglais, R-143) ; durées en toutes lettres
//! comme la table (« 5 jours », « 1,5 day ») ; récapitulative et marge en
//! jours ; % entier (« 50 % ») ; ressources comme la colonne « Noms
//! ressources » (« Maçon[50 %];Électricien ») ; « début – fin ».

use plannercraft_core::{
    ConversionFactors, Decimal, DurationUnit, LinkType, Minutes, Moment, TaskOutcome, TaskRole, TaskUid,
    TimeBasis,
};
use plannercraft_render::geometry::{BarTextSource, Token};
use plannercraft_render::style::BarField;

use crate::session::Session;
use crate::text::{
    DisplayLabels, Language, PluralRule, TextContext, display_duration, display_minutes_as, display_percent,
    format_date, format_resource_names,
};

/// Source des textes d'une session, dans une langue.
#[derive(Clone, Copy, Debug)]
pub struct SessionTexts<'a> {
    /// Session (projet et planning du moteur).
    pub session: &'a Session,
    /// Langue des textes.
    pub lang: Language,
}

/// Mots des unités et marques d'une langue (mêmes formes que les `.ftl` de
/// l'interface, J7 lot L6) ; aussi les valeurs du tableau avant/après
/// (`diff::DiffView`, H-UI-189).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Labels(pub(crate) Language);

impl DisplayLabels for Labels {
    fn decimal_sep(&self) -> char {
        match self.0 {
            Language::Fr => ',',
            Language::En => '.',
        }
    }

    fn list_sep(&self) -> &str {
        match self.0 {
            Language::Fr => ";",
            Language::En => ",",
        }
    }

    fn plural(&self) -> PluralRule {
        match self.0 {
            Language::Fr => PluralRule::BelowTwo,
            Language::En => PluralRule::ExactlyOne,
        }
    }

    #[allow(clippy::match_same_arms, reason = "une ligne par langue, lue comme une table")]
    fn unit_word(&self, unit: DurationUnit, basis: TimeBasis, singular: bool) -> &str {
        let elapsed = basis == TimeBasis::Elapsed;
        match (self.0, unit, elapsed, singular) {
            (Language::Fr, DurationUnit::Minute, false, true) => "minute",
            (Language::Fr, DurationUnit::Minute, false, false) => "minutes",
            (Language::Fr, DurationUnit::Hour, false, true) => "heure",
            (Language::Fr, DurationUnit::Hour, false, false) => "heures",
            (Language::Fr, DurationUnit::Day, false, true) => "jour",
            (Language::Fr, DurationUnit::Day, false, false) => "jours",
            (Language::Fr, DurationUnit::Week, false, true) => "semaine",
            (Language::Fr, DurationUnit::Week, false, false) => "semaines",
            (Language::Fr, DurationUnit::Month, false, _) => "mois",
            (Language::Fr, DurationUnit::Minute, true, true) => "minute écoulée",
            (Language::Fr, DurationUnit::Minute, true, false) => "minutes écoulées",
            (Language::Fr, DurationUnit::Hour, true, true) => "heure écoulée",
            (Language::Fr, DurationUnit::Hour, true, false) => "heures écoulées",
            (Language::Fr, DurationUnit::Day, true, true) => "jour écoulé",
            (Language::Fr, DurationUnit::Day, true, false) => "jours écoulés",
            (Language::Fr, DurationUnit::Week, true, true) => "semaine écoulée",
            (Language::Fr, DurationUnit::Week, true, false) => "semaines écoulées",
            (Language::Fr, DurationUnit::Month, true, true) => "mois écoulé",
            (Language::Fr, DurationUnit::Month, true, false) => "mois écoulés",
            (Language::En, DurationUnit::Minute, false, true) => "minute",
            (Language::En, DurationUnit::Minute, false, false) => "minutes",
            (Language::En, DurationUnit::Hour, false, true) => "hour",
            (Language::En, DurationUnit::Hour, false, false) => "hours",
            (Language::En, DurationUnit::Day, false, true) => "day",
            (Language::En, DurationUnit::Day, false, false) => "days",
            (Language::En, DurationUnit::Week, false, true) => "week",
            (Language::En, DurationUnit::Week, false, false) => "weeks",
            (Language::En, DurationUnit::Month, false, true) => "month",
            (Language::En, DurationUnit::Month, false, false) => "months",
            (Language::En, DurationUnit::Minute, true, true) => "eminute",
            (Language::En, DurationUnit::Minute, true, false) => "eminutes",
            (Language::En, DurationUnit::Hour, true, true) => "ehour",
            (Language::En, DurationUnit::Hour, true, false) => "ehours",
            (Language::En, DurationUnit::Day, true, true) => "eday",
            (Language::En, DurationUnit::Day, true, false) => "edays",
            (Language::En, DurationUnit::Week, true, true) => "eweek",
            (Language::En, DurationUnit::Week, true, false) => "eweeks",
            (Language::En, DurationUnit::Month, true, true) => "emonth",
            (Language::En, DurationUnit::Month, true, false) => "emonths",
        }
    }

    #[allow(clippy::match_same_arms, reason = "une ligne par langue, lue comme une table")]
    fn link_code(&self, kind: LinkType) -> &str {
        match (self.0, kind) {
            (Language::Fr, LinkType::FinishToStart) => "FD",
            (Language::Fr, LinkType::StartToStart) => "DD",
            (Language::Fr, LinkType::FinishToFinish) => "FF",
            (Language::Fr, LinkType::StartToFinish) => "DF",
            (Language::En, LinkType::FinishToStart) => "FS",
            (Language::En, LinkType::StartToStart) => "SS",
            (Language::En, LinkType::FinishToFinish) => "FF",
            (Language::En, LinkType::StartToFinish) => "SF",
        }
    }

    fn unknown(&self) -> &'static str {
        "?"
    }

    fn percent_suffix(&self) -> &str {
        match self.0 {
            Language::Fr => "\u{a0}%",
            Language::En => "%",
        }
    }
}

impl SessionTexts<'_> {
    /// Contexte d'écriture des dates.
    fn ctx(&self) -> TextContext {
        TextContext::for_project(self.session.project(), self.lang)
    }

    /// Date sans heure (R-143, partie date).
    fn day(&self, m: Moment) -> String {
        let full = format_date(m, &self.ctx());
        full.split_once(' ').map_or(full.clone(), |(d, _)| d.to_owned())
    }

    /// Date courte `JJ/MM/AA` (`MM/JJ/AA` en anglais), comme la table de
    /// l'interface sans le jour de la semaine (H-UI-63).
    fn day_short(&self, m: Moment) -> String {
        let day = self.day(m);
        // « 19/10/2026 » → « 19/10/26 » : les deux derniers chiffres de
        // l'année.
        match day.rsplit_once('/') {
            Some((head, year)) if year.len() == 4 => {
                format!("{head}/{}", year.get(2..).unwrap_or(year))
            }
            _ => day,
        }
    }
}

/// Abréviation d'une unité dans les étiquettes courtes (H-UI-63) :
/// « j », « sem. », « mois » en français, « d », « wk », « mo » en
/// anglais ; « e » devant pour une durée écoulée.
fn short_unit(lang: Language, unit: DurationUnit, basis: TimeBasis) -> String {
    let word = match (lang, unit) {
        (_, DurationUnit::Minute) => "min",
        (_, DurationUnit::Hour) => "h",
        (Language::Fr, DurationUnit::Day) => "j",
        (Language::En, DurationUnit::Day) => "d",
        (Language::Fr, DurationUnit::Week) => "sem.",
        (Language::En, DurationUnit::Week) => "wk",
        (Language::Fr, DurationUnit::Month) => "mois",
        (Language::En, DurationUnit::Month) => "mo",
    };
    if basis == TimeBasis::Elapsed {
        format!("e{word}")
    } else {
        word.to_owned()
    }
}

/// Valeur à une décimale au plus (arrondi au plus proche, demi vers le
/// haut en valeur absolue), marque décimale de la langue : « 17,4 »,
/// « 13 ».
fn one_decimal(v: Decimal, sep: char) -> String {
    // Dixièmes : mantisse ramenée à l'échelle 1.
    let tenths = match v.scale() {
        0 => v.mantissa().saturating_mul(10),
        1 => v.mantissa(),
        s => {
            let den = 10_i64.saturating_pow(u32::from(s).saturating_sub(1));
            let m = v.mantissa();
            let half = den / 2;
            let adj = if m < 0 {
                m.saturating_sub(half)
            } else {
                m.saturating_add(half)
            };
            adj.checked_div(den).unwrap_or(0)
        }
    };
    let sign = if tenths < 0 { "-" } else { "" };
    let abs = tenths.unsigned_abs();
    let (int, frac) = (abs / 10, abs % 10);
    if frac == 0 {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}{sep}{frac}")
    }
}

/// Durée courte (H-UI-63) : `m` minutes dans l'unité donnée, une décimale
/// au plus (« 13 j », « 4 sem. », « 1,5 mois ») ; « ? » si la conversion
/// échoue (règle 10).
fn short_duration(
    m: Minutes,
    unit: DurationUnit,
    basis: TimeBasis,
    f: &ConversionFactors,
    lang: Language,
) -> String {
    let sep = match lang {
        Language::Fr => ',',
        Language::En => '.',
    };
    match f.to_unit_value(m, unit, basis) {
        Ok(v) => format!("{}\u{a0}{}", one_decimal(v, sep), short_unit(lang, unit, basis)),
        Err(_) => "?".to_owned(),
    }
}

/// Durée courte d'une récapitulative (H-UI-63) : jours ouvrés, ou mois
/// (une décimale) au-delà de deux mois (`2 × jours par mois`) : « 45 j »,
/// « 17,4 mois ».
fn short_summary_duration(m: Minutes, f: &ConversionFactors, lang: Language) -> String {
    let per_day = i64::from(f.minutes_per_day());
    let two_months = per_day
        .saturating_mul(i64::from(f.days_per_month()))
        .saturating_mul(2);
    let unit = if m.get() > two_months {
        DurationUnit::Month
    } else {
        DurationUnit::Day
    };
    short_duration(m, unit, TimeBasis::Working, f, lang)
}

impl BarTextSource for SessionTexts<'_> {
    fn bar_text(&self, uid: TaskUid, f: BarField) -> Option<String> {
        let project = self.session.project();
        let task = project.tasks.get(uid)?;
        let labels = Labels(self.lang);
        let factors = &project.options.factors;
        let scheduled = match self.session.schedule().tasks.get(&uid) {
            Some(TaskOutcome::Scheduled(s)) => Some(s),
            _ => None,
        };
        let text = match f {
            BarField::Name => task.name.clone(),
            BarField::Resources => format_resource_names(
                project.assignments.iter().filter(|a| a.task == uid),
                project,
                &labels,
            ),
            BarField::Start => self.day(scheduled?.start),
            BarField::Finish => self.day(scheduled?.finish),
            BarField::StartFinish => {
                let s = scheduled?;
                format!("{} – {}", self.day(s.start), self.day(s.finish))
            }
            BarField::Duration => {
                let s = scheduled?;
                if s.role == TaskRole::Summary {
                    display_minutes_as(
                        s.duration,
                        DurationUnit::Day,
                        TimeBasis::Working,
                        factors,
                        &labels,
                    )
                } else {
                    display_duration(&task.duration, factors, &labels)
                }
            }
            BarField::DurationShort => {
                let s = scheduled?;
                if s.role == TaskRole::Summary {
                    short_summary_duration(s.duration, factors, self.lang)
                } else {
                    let d = &task.duration;
                    short_duration(d.minutes(), d.unit(), d.basis(), factors, self.lang)
                }
            }
            BarField::NameDuration => {
                let d = self.bar_text(uid, BarField::DurationShort);
                match d {
                    Some(d) if !task.name.is_empty() => format!("{} ({d})", task.name),
                    _ => task.name.clone(),
                }
            }
            BarField::StartShort => self.day_short(scheduled?.start),
            BarField::FinishShort => self.day_short(scheduled?.finish),
            BarField::Percent => display_percent(scheduled?.percent_complete, &labels),
            BarField::TotalSlack => display_minutes_as(
                scheduled?.total_slack,
                DurationUnit::Day,
                TimeBasis::Working,
                factors,
                &labels,
            ),
        };
        (!text.is_empty()).then_some(text)
    }

    fn token(&self, t: Token) -> String {
        let project = self.session.project();
        match t {
            Token::Project => {
                if project.info.name.is_empty() {
                    self.session.origin().name.clone()
                } else {
                    project.info.name.clone()
                }
            }
            Token::StatusDate => self.day(project.effective_status_date()),
            Token::PrintDate => self.day(self.session.now()),
            Token::File => {
                let origin = self.session.origin();
                origin
                    .path
                    .as_deref()
                    .and_then(|p| p.rsplit(['/', '\\']).next())
                    .filter(|n| !n.is_empty())
                    .map_or_else(|| origin.name.clone(), str::to_owned)
            }
            Token::ScheduleTitle => match self.lang {
                Language::Fr => "Planning prévisionnel".to_owned(),
                Language::En => "Provisional schedule".to_owned(),
            },
            // Numéros de page : remplacés par la pagination (L7).
            Token::Page | Token::Pages => String::new(),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::*;
    use crate::session::DocOrigin;
    use plannercraft_core::calendar::BuiltinCalendarNames;
    use plannercraft_core::{Duration, Minutes, OutlineLevel, Project, ProjectInit, ScheduleAnchor, Task};

    fn session() -> Session {
        let start: Moment = "2026-10-19T08:00".parse().unwrap();
        let mut p = Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(start),
            current_date: start,
            calendar_names: BuiltinCalendarNames::default(),
        });
        let options = p.options;
        let mut t = Task::new(TaskUid::new(1), "Murs".to_owned(), &options);
        t.outline_level = OutlineLevel::new(1).unwrap();
        t.duration =
            Duration::new(Minutes::new(4_800), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        p.tasks.push(t).unwrap();
        Session::from_project(
            p,
            DocOrigin {
                name: "P".to_owned(),
                path: Some("/tmp/chantier/p.xml".to_owned()),
            },
            "2026-10-25T09:00".parse().unwrap(),
            Language::Fr,
        )
    }

    #[test]
    fn bar_texts_come_from_engine() {
        let s = session();
        let fr = SessionTexts {
            session: &s,
            lang: Language::Fr,
        };
        let uid = TaskUid::new(1);
        assert_eq!(fr.bar_text(uid, BarField::Name).as_deref(), Some("Murs"));
        assert_eq!(fr.bar_text(uid, BarField::Start).as_deref(), Some("19/10/2026"));
        assert_eq!(fr.bar_text(uid, BarField::Finish).as_deref(), Some("30/10/2026"));
        assert_eq!(
            fr.bar_text(uid, BarField::StartFinish).as_deref(),
            Some("19/10/2026 – 30/10/2026")
        );
        assert_eq!(fr.bar_text(uid, BarField::Duration).as_deref(), Some("10 jours"));
        assert_eq!(fr.bar_text(uid, BarField::Percent).as_deref(), Some("0\u{a0}%"));
        assert_eq!(fr.bar_text(uid, BarField::TotalSlack).as_deref(), Some("0 jour"));
        assert_eq!(fr.bar_text(uid, BarField::Resources), None);
        assert_eq!(fr.bar_text(TaskUid::new(9), BarField::Name), None);
        let en = SessionTexts {
            session: &s,
            lang: Language::En,
        };
        assert_eq!(en.bar_text(uid, BarField::Finish).as_deref(), Some("10/30/2026"));
        assert_eq!(en.bar_text(uid, BarField::Duration).as_deref(), Some("10 days"));
        assert_eq!(fr.token(Token::Project), "P");
        assert_eq!(fr.token(Token::File), "p.xml");
        assert_eq!(fr.token(Token::PrintDate), "25/10/2026");
        assert_eq!(fr.token(Token::StatusDate), "19/10/2026");
        assert_eq!(fr.token(Token::Page), "");
        // Relecture finale de J8 : titre neutre traduit (cartouche).
        assert_eq!(fr.token(Token::ScheduleTitle), "Planning prévisionnel");
        assert_eq!(en.token(Token::ScheduleTitle), "Provisional schedule");
    }

    /// H-UI-63 : étiquettes courtes (durée après le nom, durée courte,
    /// dates courtes), toujours venues du moteur (règle 3).
    #[test]
    fn h_ui_63_short_labels() {
        let s = session();
        let fr = SessionTexts {
            session: &s,
            lang: Language::Fr,
        };
        let uid = TaskUid::new(1);
        assert_eq!(
            fr.bar_text(uid, BarField::NameDuration).as_deref(),
            Some("Murs (10\u{a0}j)")
        );
        assert_eq!(
            fr.bar_text(uid, BarField::DurationShort).as_deref(),
            Some("10\u{a0}j")
        );
        assert_eq!(
            fr.bar_text(uid, BarField::StartShort).as_deref(),
            Some("19/10/26")
        );
        assert_eq!(
            fr.bar_text(uid, BarField::FinishShort).as_deref(),
            Some("30/10/26")
        );
        let en = SessionTexts {
            session: &s,
            lang: Language::En,
        };
        assert_eq!(
            en.bar_text(uid, BarField::FinishShort).as_deref(),
            Some("10/30/26")
        );
        assert_eq!(
            en.bar_text(uid, BarField::NameDuration).as_deref(),
            Some("Murs (10\u{a0}d)")
        );
    }

    #[test]
    fn h_ui_63_short_durations() {
        let f = ConversionFactors::default();
        let fr = Language::Fr;
        let m = |v: i64| Minutes::new(v);
        // 4 semaines.
        assert_eq!(
            short_duration(m(4 * 2_400), DurationUnit::Week, TimeBasis::Working, &f, fr),
            "4\u{a0}sem."
        );
        // 1,5 mois (20 j/mois).
        assert_eq!(
            short_duration(m(15 * 480), DurationUnit::Month, TimeBasis::Working, &f, fr),
            "0,8\u{a0}mois"
        );
        // Récapitulative : jusqu'à 40 j (2 mois) en jours, au-delà en mois.
        assert_eq!(short_summary_duration(m(45 * 480), &f, fr), "2,3\u{a0}mois");
        assert_eq!(short_summary_duration(m(40 * 480), &f, fr), "40\u{a0}j");
        assert_eq!(short_summary_duration(m(348 * 480), &f, fr), "17,4\u{a0}mois");
        assert_eq!(
            short_summary_duration(m(348 * 480), &f, Language::En),
            "17.4\u{a0}mo"
        );
        assert_eq!(
            short_duration(m(3 * 1_440), DurationUnit::Day, TimeBasis::Elapsed, &f, fr),
            "3\u{a0}ej"
        );
        assert_eq!(one_decimal(Decimal::new(1_745, 2).unwrap(), ','), "17,5");
        assert_eq!(one_decimal(Decimal::new(-1_745, 2).unwrap(), ','), "-17,5");
        assert_eq!(one_decimal(Decimal::integer(13), ','), "13");
    }
}
