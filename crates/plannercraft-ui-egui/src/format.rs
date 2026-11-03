//! Formateurs d'affichage purs (durée, prédécesseurs, date, marge, %), sans
//! egui ni fluent.
//!
//! Les libellés sont résolus une fois par langue depuis les `.ftl`
//! (`Texts::format_labels`) : les formateurs ne voient que des chaînes, ce qui
//! les rend testables sans fluent. Durées, décalages, prédécesseurs, % et
//! noms des ressources sont écrits par `plannercraft_automation::text` (besoins N1,
//! N2, N11 clos au lot L6 de J7, `docs/ui-needs.md`) à partir de ces
//! libellés ([`DisplayLabels`]) ; restent ici les dates et la marge, et la
//! traduction des mots. Hypothèses H-UI.

use std::fmt::Write as _;

use chrono::Datelike as _;
use plannercraft_automation::text::{self, DisplayLabels};
use plannercraft_core::{
    ConstraintKind, ConversionFactors, Duration, DurationUnit, Lag, Link, LinkType, Minutes, Moment, Percent,
    Priority, Project, TaskType, TimeBasis, Weekday,
};

/// Libellé d'une unité au singulier et au pluriel (« jour » / « jours »).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitLabel {
    /// Forme singulière (règle de pluriel : [`FormatLabels::plural`]).
    pub one: String,
    /// Forme employée sinon.
    pub other: String,
}

/// Libellés de toutes les unités de durée, ouvrées et écoulées (champs
/// nommés : aucune indexation).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitLabels {
    /// Minute ouvrée.
    pub minute: UnitLabel,
    /// Heure ouvrée.
    pub hour: UnitLabel,
    /// Jour ouvré.
    pub day: UnitLabel,
    /// Semaine ouvrée.
    pub week: UnitLabel,
    /// Mois ouvré.
    pub month: UnitLabel,
    /// Minute écoulée.
    pub elapsed_minute: UnitLabel,
    /// Heure écoulée.
    pub elapsed_hour: UnitLabel,
    /// Jour écoulé.
    pub elapsed_day: UnitLabel,
    /// Semaine écoulée.
    pub elapsed_week: UnitLabel,
    /// Mois écoulé.
    pub elapsed_month: UnitLabel,
}

impl UnitLabels {
    /// Libellé d'une unité dans une base de temps.
    #[must_use]
    pub const fn get(&self, unit: DurationUnit, basis: TimeBasis) -> &UnitLabel {
        match (basis, unit) {
            (TimeBasis::Working, DurationUnit::Minute) => &self.minute,
            (TimeBasis::Working, DurationUnit::Hour) => &self.hour,
            (TimeBasis::Working, DurationUnit::Day) => &self.day,
            (TimeBasis::Working, DurationUnit::Week) => &self.week,
            (TimeBasis::Working, DurationUnit::Month) => &self.month,
            (TimeBasis::Elapsed, DurationUnit::Minute) => &self.elapsed_minute,
            (TimeBasis::Elapsed, DurationUnit::Hour) => &self.elapsed_hour,
            (TimeBasis::Elapsed, DurationUnit::Day) => &self.elapsed_day,
            (TimeBasis::Elapsed, DurationUnit::Week) => &self.elapsed_week,
            (TimeBasis::Elapsed, DurationUnit::Month) => &self.elapsed_month,
        }
    }
}

/// Ordre des composantes d'une date courte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateOrder {
    /// Jour / mois / année (français).
    #[default]
    DayMonthYear,
    /// Mois / jour / année (anglais).
    MonthDayYear,
}

/// Libellés et conventions d'affichage d'une langue.
///
/// Les tableaux se lisent par `.get()` seulement. Mois : janvier d'abord ;
/// jours : dans l'ordre de `plannercraft_core::Weekday::ALL` (dimanche d'abord).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatLabels {
    /// Séparateur décimal (FR `,` ; EN `.`).
    pub decimal_sep: char,
    /// Séparateur de liste des prédécesseurs (FR `;` ; EN `,`).
    pub list_sep: String,
    /// Libellés des unités de durée.
    pub units: UnitLabels,
    /// Code du lien fin à début (FR `FD` ; EN `FS`).
    pub link_fs: String,
    /// Code du lien début à début (FR `DD` ; EN `SS`).
    pub link_ss: String,
    /// Code du lien fin à fin (`FF`).
    pub link_ff: String,
    /// Code du lien début à fin (FR `DF` ; EN `SF`).
    pub link_sf: String,
    /// Ordre des composantes d'une date.
    pub date_order: DateOrder,
    /// Règle de pluriel des unités (clé `fmt-plural-rule`).
    pub plural: PluralRule,
    /// « Oui ».
    pub yes: String,
    /// « Non ».
    pub no: String,
    /// Texte affiché quand une valeur ne peut pas être formatée.
    pub unknown: String,
    /// Noms longs des mois, janvier d'abord.
    pub months_long: [String; 12],
    /// Noms courts des mois, janvier d'abord.
    pub months_short: [String; 12],
    /// Noms courts des jours, dimanche d'abord.
    pub weekdays_short: [String; 7],
    /// Initiales des jours, dimanche d'abord.
    pub weekday_initials: [String; 7],
    /// Préfixe de trimestre (FR `T` ; EN `Q`).
    pub quarter_prefix: String,
    /// Préfixe d'une semaine numérotée (clé `tier-week` : FR `S` ; EN `W`).
    pub week_prefix: String,
    /// Suffixe d'un pourcentage (clé `fmt-percent-suffix` : FR « \u{a0}% » ;
    /// EN « % » ; J7 lot L6).
    pub percent_suffix: String,
    /// Mode de planification manuel (colonne « Mode », J8 lot L11).
    pub mode_manual: String,
    /// Mode de planification automatique.
    pub mode_auto: String,
}

/// Suffixe d'une durée estimée (« 1 jour? »), identique en français et en
/// anglais dans MS Project (hypothèse H-UI : symbole, pas une chaîne
/// traduite). Défini par `plannercraft_automation::text` (N1).
pub use plannercraft_automation::text::ESTIMATED_SUFFIX;
/// Règle de pluriel des unités, lue dans la clé `.ftl` `fmt-plural-rule`
/// (`Texts::format_labels`) ; définie par `plannercraft_automation::text` (N1).
pub use plannercraft_automation::text::PluralRule;
/// Écriture d'un décimal du cœur en arithmétique entière (N1).
pub use plannercraft_automation::text::write_decimal;

/// Les libellés d'une langue alimentent les formateurs d'affichage de
/// `plannercraft_automation::text` (N1, N2, N11 : J7 lot L6) : l'interface ne
/// garde que la traduction des mots.
impl DisplayLabels for FormatLabels {
    fn decimal_sep(&self) -> char {
        self.decimal_sep
    }

    fn list_sep(&self) -> &str {
        &self.list_sep
    }

    fn plural(&self) -> PluralRule {
        self.plural
    }

    fn unit_word(&self, unit: DurationUnit, basis: TimeBasis, singular: bool) -> &str {
        let label = self.units.get(unit, basis);
        if singular { &label.one } else { &label.other }
    }

    fn link_code(&self, kind: LinkType) -> &str {
        match kind {
            LinkType::FinishToStart => &self.link_fs,
            LinkType::StartToStart => &self.link_ss,
            LinkType::FinishToFinish => &self.link_ff,
            LinkType::StartToFinish => &self.link_sf,
        }
    }

    fn unknown(&self) -> &str {
        &self.unknown
    }

    fn percent_suffix(&self) -> &str {
        &self.percent_suffix
    }
}

/// Écrit des minutes dans une unité et une base (« 31 jours »), avec le
/// suffixe d'estimation si demandé ; `l.unknown` si la conversion échoue.
/// Délégué à `plannercraft_automation::text` (N1).
pub fn write_duration(
    out: &mut String,
    minutes: Minutes,
    unit: DurationUnit,
    basis: TimeBasis,
    estimated: bool,
    f: &ConversionFactors,
    l: &FormatLabels,
) {
    text::write_display_duration(out, minutes, unit, basis, estimated, f, l);
}

/// Durée d'une tâche dans son unité (« 5 jours », « 2,5 jours », « 1 jour? »,
/// « 5 jours écoulés ») ; délégué à `plannercraft_automation::text` (N1).
#[must_use]
pub fn format_duration(d: &Duration, f: &ConversionFactors, l: &FormatLabels) -> String {
    text::display_duration(d, f, l)
}

/// Minutes exprimées dans une unité et une base données (« 31 jours ») :
/// durée d'une récapitulative (`TaskSchedule::duration`), marge ; délégué à
/// `plannercraft_automation::text` (N1).
#[must_use]
pub fn format_minutes_as(
    m: Minutes,
    unit: DurationUnit,
    basis: TimeBasis,
    f: &ConversionFactors,
    l: &FormatLabels,
) -> String {
    text::display_minutes_as(m, unit, basis, f, l)
}

/// Écrit un décalage signé (« +2 jours », « -1 jour », « +50% ») ; rien pour
/// un décalage nul. Délégué à `plannercraft_automation::text` (N2).
pub fn write_lag(out: &mut String, lag: &Lag, f: &ConversionFactors, l: &FormatLabels) {
    text::write_display_lag(out, lag, f, l);
}

/// Liste des prédécesseurs (« 3;5DD+2 jours ») : ID (et non UID) seul pour un
/// lien fin à début sans décalage, `l.unknown` à la place de l'ID d'un UID
/// inconnu ; ordre des liens conservé, séparateur `l.list_sep`. Délégué à
/// `plannercraft_automation::text` (N2).
#[must_use]
pub fn format_predecessors(
    links: &[Link],
    project: &Project,
    f: &ConversionFactors,
    l: &FormatLabels,
) -> String {
    text::display_predecessors(links, project, f, l)
}

/// % achevé, entier tronqué (« 33 % ») ; délégué à
/// `plannercraft_automation::text` (J7 lot L6).
#[must_use]
pub fn format_percent(p: Percent, l: &FormatLabels) -> String {
    text::display_percent(p, l)
}

/// Nom court d'un jour de la semaine (`weekdays_short`, dimanche d'abord).
fn weekday_short(w: Weekday, l: &FormatLabels) -> &str {
    let rank = match w {
        Weekday::Sunday => 0,
        Weekday::Monday => 1,
        Weekday::Tuesday => 2,
        Weekday::Wednesday => 3,
        Weekday::Thursday => 4,
        Weekday::Friday => 5,
        Weekday::Saturday => 6,
    };
    l.weekdays_short.get(rank).map_or("", String::as_str)
}

/// Date courte (« lun 02/11/26 » / « Mon 11/2/26 ») : jour de la semaine,
/// puis jour et mois sur deux chiffres en français, sans zéro initial en
/// anglais, année sur deux chiffres. Heure non affichée (hypothèse H-UI :
/// format de date par défaut de MS Project).
#[must_use]
pub fn format_date(m: Moment, l: &FormatLabels) -> String {
    let date = m.to_naive().date();
    let year = date.year().rem_euclid(100);
    let (month, day) = (date.month(), date.day());
    let weekday = weekday_short(m.weekday(), l);
    let mut out = String::new();
    if !weekday.is_empty() {
        out.push_str(weekday);
        out.push(' ');
    }
    let _ = match l.date_order {
        DateOrder::DayMonthYear => write!(out, "{day:02}/{month:02}/{year:02}"),
        DateOrder::MonthDayYear => write!(out, "{month}/{day}/{year:02}"),
    };
    out
}

/// Marge en jours ouvrés, signée (« 0 jour », « -2 jours »), hypothèse
/// H-UI-09.
#[must_use]
pub fn format_slack(m: Minutes, f: &ConversionFactors, l: &FormatLabels) -> String {
    format_minutes_as(m, DurationUnit::Day, TimeBasis::Working, f, l)
}

/// Libellés des champs à choix de la seconde passe UX (lot L7, H-UI-117) :
/// types de contrainte et de tâche, résolus une fois par langue
/// (`field::field_labels`) ; Oui, Non et les modes viennent de
/// [`FormatLabels`]. Champs nommés : aucune indexation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldLabels {
    /// « Dès que possible ».
    pub asap: String,
    /// « Le plus tard possible ».
    pub alap: String,
    /// « Doit commencer le ».
    pub mso: String,
    /// « Doit finir le ».
    pub mfo: String,
    /// « Début au plus tôt le ».
    pub snet: String,
    /// « Début au plus tard le ».
    pub snlt: String,
    /// « Fin au plus tôt le ».
    pub fnet: String,
    /// « Fin au plus tard le ».
    pub fnlt: String,
    /// « Capacité fixe ».
    pub fixed_units: String,
    /// « Durée fixe ».
    pub fixed_duration: String,
    /// « Travail fixe ».
    pub fixed_work: String,
}

impl FieldLabels {
    /// Nom d'un type de contrainte.
    #[must_use]
    pub fn constraint(&self, kind: ConstraintKind) -> &str {
        match kind {
            ConstraintKind::AsSoonAsPossible => &self.asap,
            ConstraintKind::AsLateAsPossible => &self.alap,
            ConstraintKind::MustStartOn => &self.mso,
            ConstraintKind::MustFinishOn => &self.mfo,
            ConstraintKind::StartNoEarlierThan => &self.snet,
            ConstraintKind::StartNoLaterThan => &self.snlt,
            ConstraintKind::FinishNoEarlierThan => &self.fnet,
            ConstraintKind::FinishNoLaterThan => &self.fnlt,
        }
    }

    /// Nom d'un type de tâche.
    #[must_use]
    pub fn task_type(&self, task_type: TaskType) -> &str {
        match task_type {
            TaskType::FixedUnits => &self.fixed_units,
            TaskType::FixedDuration => &self.fixed_duration,
            TaskType::FixedWork => &self.fixed_work,
        }
    }
}

/// Les huit types de contrainte, dans l'ordre de la fenêtre « Informations
/// sur la tâche » (choix de la saisie, H-UI-117).
pub const CONSTRAINT_KINDS: [ConstraintKind; 8] = [
    ConstraintKind::AsSoonAsPossible,
    ConstraintKind::AsLateAsPossible,
    ConstraintKind::MustStartOn,
    ConstraintKind::MustFinishOn,
    ConstraintKind::StartNoEarlierThan,
    ConstraintKind::StartNoLaterThan,
    ConstraintKind::FinishNoEarlierThan,
    ConstraintKind::FinishNoLaterThan,
];

/// Les trois types de tâche.
pub const TASK_TYPES: [TaskType; 3] = [TaskType::FixedUnits, TaskType::FixedDuration, TaskType::FixedWork];

/// « Oui » ou « Non ».
#[must_use]
pub fn format_yes_no(value: bool, l: &FormatLabels) -> String {
    if value { l.yes.clone() } else { l.no.clone() }
}

/// Travail en heures ouvrées (« 40 heures », « 7,5 heures »), comme la
/// colonne Travail de MS Project (hypothèse H-UI-117 : unité par défaut du
/// travail) ; signé pour un écart de travail.
#[must_use]
pub fn format_work(m: Minutes, f: &ConversionFactors, l: &FormatLabels) -> String {
    format_minutes_as(m, DurationUnit::Hour, TimeBasis::Working, f, l)
}

/// Priorité (« 500 »).
#[must_use]
pub fn format_priority(p: Priority) -> String {
    p.get().to_string()
}

/// Notes sur une ligne (colonne Notes, H-UI-117) : fins de ligne et
/// tabulations remplacées par une espace, espaces de bord retirées ; au
/// plus [`NOTES_LINE_CHARS`] caractères, puis « … ».
#[must_use]
pub fn format_notes_line(notes: &str) -> String {
    let mut out = String::with_capacity(notes.len().min(NOTES_LINE_CHARS));
    let mut count = 0_usize;
    for c in notes.trim().chars() {
        if count == NOTES_LINE_CHARS {
            out.push('…');
            break;
        }
        out.push(if c.is_control() { ' ' } else { c });
        count = count.saturating_add(1);
    }
    out
}

/// Longueur affichée d'une note dans la table (le reste est tronqué).
pub const NOTES_LINE_CHARS: usize = 255;

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        reason = "tests : un échec doit s'arrêter net et lisiblement"
    )]

    use plannercraft_core::{
        BuiltinCalendarNames, ConversionFactors, Decimal, Duration, DurationUnit, Lag, LagValue, Link,
        LinkType, Minutes, Moment, Project, ProjectInit, ScheduleAnchor, SignedPercent, Task, TaskUid,
        TimeBasis,
    };

    use super::{
        DateOrder, FormatLabels, PluralRule, UnitLabel, UnitLabels, format_date, format_duration,
        format_minutes_as, format_predecessors, format_slack, write_decimal,
    };

    fn unit(one: &str, other: &str) -> UnitLabel {
        UnitLabel {
            one: one.to_owned(),
            other: other.to_owned(),
        }
    }

    fn strings<const N: usize>(items: [&str; N]) -> [String; N] {
        items.map(str::to_owned)
    }

    /// Libellés français construits à la main (sans fluent).
    fn fr() -> FormatLabels {
        FormatLabels {
            decimal_sep: ',',
            list_sep: ";".to_owned(),
            units: UnitLabels {
                minute: unit("minute", "minutes"),
                hour: unit("heure", "heures"),
                day: unit("jour", "jours"),
                week: unit("semaine", "semaines"),
                month: unit("mois", "mois"),
                elapsed_minute: unit("minute écoulée", "minutes écoulées"),
                elapsed_hour: unit("heure écoulée", "heures écoulées"),
                elapsed_day: unit("jour écoulé", "jours écoulés"),
                elapsed_week: unit("semaine écoulée", "semaines écoulées"),
                elapsed_month: unit("mois écoulé", "mois écoulés"),
            },
            link_fs: "FD".to_owned(),
            link_ss: "DD".to_owned(),
            link_ff: "FF".to_owned(),
            link_sf: "DF".to_owned(),
            date_order: DateOrder::DayMonthYear,
            plural: PluralRule::BelowTwo,
            yes: "Oui".to_owned(),
            no: "Non".to_owned(),
            unknown: "?".to_owned(),
            weekdays_short: strings(["dim", "lun", "mar", "mer", "jeu", "ven", "sam"]),
            ..FormatLabels::default()
        }
    }

    /// Libellés anglais construits à la main (sans fluent).
    fn en() -> FormatLabels {
        FormatLabels {
            decimal_sep: '.',
            list_sep: ",".to_owned(),
            units: UnitLabels {
                minute: unit("min", "mins"),
                hour: unit("hr", "hrs"),
                day: unit("day", "days"),
                week: unit("wk", "wks"),
                month: unit("mon", "mons"),
                elapsed_minute: unit("emin", "emins"),
                elapsed_hour: unit("ehr", "ehrs"),
                elapsed_day: unit("eday", "edays"),
                elapsed_week: unit("ewk", "ewks"),
                elapsed_month: unit("emon", "emons"),
            },
            link_fs: "FS".to_owned(),
            link_ss: "SS".to_owned(),
            link_ff: "FF".to_owned(),
            link_sf: "SF".to_owned(),
            date_order: DateOrder::MonthDayYear,
            plural: PluralRule::ExactlyOne,
            yes: "Yes".to_owned(),
            no: "No".to_owned(),
            unknown: "?".to_owned(),
            weekdays_short: strings(["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]),
            ..FormatLabels::default()
        }
    }

    /// Facteurs par défaut de MS Project : 480 min/jour, 2 400 min/semaine,
    /// 20 jours/mois.
    fn factors() -> ConversionFactors {
        ConversionFactors::new(480, 2_400, 20).unwrap()
    }

    fn days(minutes: i64, basis: TimeBasis, estimated: bool) -> Duration {
        Duration::new(Minutes::new(minutes), DurationUnit::Day, basis, estimated).unwrap()
    }

    fn moment(s: &str) -> Moment {
        s.parse().unwrap()
    }

    #[test]
    fn fmt_duration_fr_days() {
        let f = factors();
        assert_eq!(
            format_duration(&days(2_400, TimeBasis::Working, false), &f, &fr()),
            "5 jours"
        );
        assert_eq!(
            format_duration(&days(480, TimeBasis::Working, false), &f, &fr()),
            "1 jour"
        );
        assert_eq!(
            format_duration(&days(0, TimeBasis::Working, false), &f, &fr()),
            "0 jour"
        );
        assert_eq!(
            format_duration(&days(960, TimeBasis::Working, false), &f, &fr()),
            "2 jours"
        );
    }

    #[test]
    fn fmt_duration_en_days() {
        let f = factors();
        assert_eq!(
            format_duration(&days(2_400, TimeBasis::Working, false), &f, &en()),
            "5 days"
        );
        assert_eq!(
            format_duration(&days(480, TimeBasis::Working, false), &f, &en()),
            "1 day"
        );
        assert_eq!(
            format_duration(&days(0, TimeBasis::Working, false), &f, &en()),
            "0 days"
        );
        assert_eq!(
            format_duration(&days(720, TimeBasis::Working, false), &f, &en()),
            "1.5 days"
        );
    }

    #[test]
    fn fmt_duration_fr_fraction_comma() {
        let f = factors();
        assert_eq!(
            format_duration(&days(1_200, TimeBasis::Working, false), &f, &fr()),
            "2,5 jours"
        );
        assert_eq!(
            format_duration(&days(720, TimeBasis::Working, false), &f, &fr()),
            "1,5 jour"
        );
        // 1/3 de jour : deux décimales (R-009).
        assert_eq!(
            format_duration(&days(160, TimeBasis::Working, false), &f, &fr()),
            "0,33 jour"
        );
        assert_eq!(
            format_duration(&days(504, TimeBasis::Working, false), &f, &fr()),
            "1,05 jour"
        );
    }

    #[test]
    fn fmt_duration_estimated() {
        let f = factors();
        assert_eq!(
            format_duration(&days(480, TimeBasis::Working, true), &f, &fr()),
            "1 jour?"
        );
        assert_eq!(
            format_duration(&days(960, TimeBasis::Working, true), &f, &en()),
            "2 days?"
        );
    }

    #[test]
    fn fmt_duration_elapsed() {
        let f = factors();
        // Jour écoulé : 1 440 min.
        assert_eq!(
            format_duration(&days(7_200, TimeBasis::Elapsed, false), &f, &fr()),
            "5 jours écoulés"
        );
        assert_eq!(
            format_duration(&days(1_440, TimeBasis::Elapsed, false), &f, &fr()),
            "1 jour écoulé"
        );
        assert_eq!(
            format_duration(&days(7_200, TimeBasis::Elapsed, false), &f, &en()),
            "5 edays"
        );
    }

    #[test]
    fn fmt_duration_other_units() {
        let f = factors();
        let hours = Duration::new(Minutes::new(180), DurationUnit::Hour, TimeBasis::Working, false).unwrap();
        assert_eq!(format_duration(&hours, &f, &fr()), "3 heures");
        let weeks =
            Duration::new(Minutes::new(4_800), DurationUnit::Week, TimeBasis::Working, false).unwrap();
        assert_eq!(format_duration(&weeks, &f, &en()), "2 wks");
        let month = Duration::new(
            Minutes::new(9_600),
            DurationUnit::Month,
            TimeBasis::Working,
            false,
        )
        .unwrap();
        assert_eq!(format_duration(&month, &f, &fr()), "1 mois");
    }

    #[test]
    fn fmt_duration_extremes_no_panic() {
        let f = factors();
        let max = Duration::new(
            Minutes::new(Duration::MAX_MINUTES),
            DurationUnit::Minute,
            TimeBasis::Elapsed,
            true,
        )
        .unwrap();
        assert_eq!(format_duration(&max, &f, &fr()), "87308640 minutes écoulées?");
        // Minutes brutes hors du domaine : débordement de `to_unit_value`.
        assert_eq!(
            format_minutes_as(
                Minutes::new(i64::MAX),
                DurationUnit::Minute,
                TimeBasis::Working,
                &f,
                &fr()
            ),
            "?"
        );
        assert_eq!(
            format_minutes_as(
                Minutes::new(i64::MAX),
                DurationUnit::Day,
                TimeBasis::Working,
                &f,
                &fr()
            ),
            "19215358410114116,26 jours"
        );
        assert_eq!(
            format_minutes_as(
                Minutes::new(i64::MIN),
                DurationUnit::Minute,
                TimeBasis::Working,
                &f,
                &en()
            ),
            "?"
        );
        let big = format_minutes_as(
            Minutes::new(i64::MAX / 200),
            DurationUnit::Day,
            TimeBasis::Working,
            &f,
            &fr(),
        );
        assert!(big.ends_with(" jours"), "{big}");
        // Libellés vides : pas de panique, pas d'espace pendante.
        assert_eq!(
            format_duration(
                &days(480, TimeBasis::Working, false),
                &f,
                &FormatLabels::default()
            ),
            "1"
        );
    }

    #[test]
    fn fmt_decimal_integer_arithmetic() {
        let mut out = String::new();
        write_decimal(&mut out, Decimal::new(-250, 2).unwrap(), ',');
        assert_eq!(out, "-2,5");
        out.clear();
        write_decimal(&mut out, Decimal::new(i64::MIN, 6).unwrap(), '.');
        assert_eq!(out, "-9223372036854.775808");
        out.clear();
        write_decimal(&mut out, Decimal::new(-5, 2).unwrap(), ',');
        assert_eq!(out, "-0,05");
    }

    #[test]
    fn fmt_slack_signed_days() {
        let f = factors();
        assert_eq!(format_slack(Minutes::new(0), &f, &fr()), "0 jour");
        assert_eq!(format_slack(Minutes::new(-960), &f, &fr()), "-2 jours");
        assert_eq!(format_slack(Minutes::new(-480), &f, &fr()), "-1 jour");
        assert_eq!(format_slack(Minutes::new(2_400), &f, &en()), "5 days");
        assert_eq!(format_slack(Minutes::new(-480), &f, &en()), "-1 day");
        assert_eq!(format_slack(Minutes::new(240), &f, &en()), "0.5 days");
    }

    /// Projet de quatre tâches (UID 10, 20, 30, 40 ; ID 1 à 4).
    fn small_project() -> Project {
        let now = moment("2026-10-19T08:00");
        let mut p = Project::new(ProjectInit {
            name: "T".to_owned(),
            anchor: ScheduleAnchor::FromStart(now),
            current_date: now,
            calendar_names: BuiltinCalendarNames::default(),
        });
        let options = p.options;
        for uid in [10, 20, 30, 40] {
            p.tasks
                .push(Task::new(TaskUid::new(uid), String::new(), &options))
                .unwrap();
        }
        p
    }

    fn link(pred: u32, kind: LinkType, basis: TimeBasis, value: LagValue) -> Link {
        Link {
            predecessor: TaskUid::new(pred),
            kind,
            lag: Lag::new(basis, value).unwrap(),
        }
    }

    fn lag_days(minutes: i64) -> LagValue {
        LagValue::Time {
            minutes: Minutes::new(minutes),
            unit: DurationUnit::Day,
        }
    }

    #[test]
    fn fmt_predecessors_fs_omits_type() {
        let p = small_project();
        let f = factors();
        let links = [Link::finish_to_start(TaskUid::new(30))];
        assert_eq!(format_predecessors(&links, &p, &f, &fr()), "3");
        assert_eq!(format_predecessors(&links, &p, &f, &en()), "3");
        let two = [
            Link::finish_to_start(TaskUid::new(10)),
            Link::finish_to_start(TaskUid::new(20)),
        ];
        assert_eq!(format_predecessors(&two, &p, &f, &fr()), "1;2");
        assert_eq!(format_predecessors(&two, &p, &f, &en()), "1,2");
        assert_eq!(format_predecessors(&[], &p, &f, &fr()), "");
    }

    #[test]
    fn fmt_predecessors_ss_lag_fr() {
        let p = small_project();
        let f = factors();
        let ss = [link(
            30,
            LinkType::StartToStart,
            TimeBasis::Working,
            lag_days(960),
        )];
        assert_eq!(format_predecessors(&ss, &p, &f, &fr()), "3DD+2 jours");
        assert_eq!(format_predecessors(&ss, &p, &f, &en()), "3SS+2 days");
        let fs = [link(
            30,
            LinkType::FinishToStart,
            TimeBasis::Working,
            lag_days(960),
        )];
        assert_eq!(format_predecessors(&fs, &p, &f, &fr()), "3FD+2 jours");
        let kinds = [
            link(10, LinkType::StartToStart, TimeBasis::Working, lag_days(0)),
            link(20, LinkType::FinishToFinish, TimeBasis::Working, lag_days(0)),
            link(40, LinkType::StartToFinish, TimeBasis::Working, lag_days(0)),
        ];
        assert_eq!(format_predecessors(&kinds, &p, &f, &fr()), "1DD;2FF;4DF");
        assert_eq!(format_predecessors(&kinds, &p, &f, &en()), "1SS,2FF,4SF");
    }

    #[test]
    fn fmt_predecessors_negative_lag() {
        let p = small_project();
        let f = factors();
        let lead = [link(
            20,
            LinkType::FinishToStart,
            TimeBasis::Working,
            lag_days(-480),
        )];
        assert_eq!(format_predecessors(&lead, &p, &f, &fr()), "2FD-1 jour");
        assert_eq!(format_predecessors(&lead, &p, &f, &en()), "2FS-1 day");
    }

    #[test]
    fn fmt_predecessors_percent_and_elapsed() {
        let p = small_project();
        let f = factors();
        let percent = |v| LagValue::Percent {
            percent: SignedPercent::new(v).unwrap(),
        };
        let links = [
            link(10, LinkType::FinishToStart, TimeBasis::Working, percent(5_000)),
            link(20, LinkType::StartToStart, TimeBasis::Working, percent(-2_550)),
            link(30, LinkType::FinishToStart, TimeBasis::Elapsed, lag_days(2_880)),
        ];
        assert_eq!(
            format_predecessors(&links, &p, &f, &fr()),
            "1FD+50%;2DD-25,5%;3FD+2 jours écoulés"
        );
        assert_eq!(
            format_predecessors(&links, &p, &f, &en()),
            "1FS+50%,2SS-25.5%,3FS+2 edays"
        );
    }

    #[test]
    fn fmt_predecessors_unknown_uid() {
        let p = small_project();
        let f = factors();
        let links = [Link::finish_to_start(TaskUid::new(99))];
        assert_eq!(format_predecessors(&links, &p, &f, &fr()), "?");
        let mixed = [
            Link::finish_to_start(TaskUid::new(10)),
            link(77, LinkType::StartToStart, TimeBasis::Working, lag_days(0)),
        ];
        assert_eq!(format_predecessors(&mixed, &p, &f, &fr()), "1;?DD");
    }

    #[test]
    fn fmt_date_fr() {
        assert_eq!(format_date(moment("2026-11-02T08:00"), &fr()), "lun 02/11/26");
        assert_eq!(format_date(moment("2027-05-17T17:00"), &fr()), "lun 17/05/27");
        assert_eq!(format_date(moment("2026-10-18T08:00"), &fr()), "dim 18/10/26");
    }

    #[test]
    fn fmt_date_en() {
        assert_eq!(format_date(moment("2026-11-02T08:00"), &en()), "Mon 11/2/26");
        assert_eq!(format_date(moment("2027-12-31T17:00"), &en()), "Fri 12/31/27");
        assert_eq!(format_date(moment("2030-01-05T08:00"), &en()), "Sat 1/5/30");
    }

    #[test]
    fn fmt_date_extremes() {
        // Bornes du domaine des dates : aucune panique.
        let first = Moment::from_minutes(0).unwrap();
        assert_ne!(format_date(first, &fr()), "");
        // Libellés vides : date seule.
        assert_eq!(
            format_date(moment("2026-11-02T08:00"), &FormatLabels::default()),
            "02/11/26"
        );
    }

    #[test]
    fn fmt_labels_from_ftl_match_hand_built() {
        let fr_texts = crate::i18n::Texts::new(crate::i18n::Language::Fr);
        let fr_ftl = fr_texts.format_labels();
        let f = factors();
        assert_eq!(
            format_duration(&days(2_400, TimeBasis::Working, false), &f, fr_ftl),
            "5 jours"
        );
        assert_eq!(format_date(moment("2026-11-02T08:00"), fr_ftl), "lun 02/11/26");
        let en_texts = crate::i18n::Texts::new(crate::i18n::Language::En);
        let en_ftl = en_texts.format_labels();
        assert_eq!(format_date(moment("2026-11-02T08:00"), en_ftl), "Mon 11/2/26");
        assert_eq!(
            format_duration(&days(2_400, TimeBasis::Working, false), &f, en_ftl),
            "5 days"
        );
    }

    /// N1, N2 (J7, lot L6) : les formateurs de l'interface délèguent à
    /// `plannercraft_automation::text` et rendent les textes J4 à l'identique
    /// (corpus des tests ci-dessus, dans les deux langues).
    #[test]
    fn format_delegates_to_automation_same_text() {
        use plannercraft_automation::text;
        let f = factors();
        let p = small_project();
        let durations = [
            (days(2_400, TimeBasis::Working, false), "5 jours", "5 days"),
            (days(720, TimeBasis::Working, false), "1,5 jour", "1.5 days"),
            (days(0, TimeBasis::Working, false), "0 jour", "0 days"),
            (days(160, TimeBasis::Working, false), "0,33 jour", "0.33 days"),
            (days(480, TimeBasis::Working, true), "1 jour?", "1 day?"),
            (
                days(7_200, TimeBasis::Elapsed, false),
                "5 jours écoulés",
                "5 edays",
            ),
        ];
        for (d, want_fr, want_en) in &durations {
            assert_eq!(format_duration(d, &f, &fr()), *want_fr);
            assert_eq!(text::display_duration(d, &f, &fr()), *want_fr);
            assert_eq!(format_duration(d, &f, &en()), *want_en);
            assert_eq!(text::display_duration(d, &f, &en()), *want_en);
        }
        let percent = |v| LagValue::Percent {
            percent: SignedPercent::new(v).unwrap(),
        };
        let links = [
            Link::finish_to_start(TaskUid::new(30)),
            link(30, LinkType::StartToStart, TimeBasis::Working, lag_days(960)),
            link(20, LinkType::FinishToStart, TimeBasis::Working, lag_days(-480)),
            link(10, LinkType::FinishToStart, TimeBasis::Working, percent(5_000)),
            link(30, LinkType::FinishToStart, TimeBasis::Elapsed, lag_days(2_880)),
            link(77, LinkType::StartToFinish, TimeBasis::Working, lag_days(0)),
        ];
        let want_fr = "3;3DD+2 jours;2FD-1 jour;1FD+50%;3FD+2 jours écoulés;?DF";
        let want_en = "3,3SS+2 days,2FS-1 day,1FS+50%,3FS+2 edays,?SF";
        assert_eq!(format_predecessors(&links, &p, &f, &fr()), want_fr);
        assert_eq!(text::display_predecessors(&links, &p, &f, &fr()), want_fr);
        assert_eq!(format_predecessors(&links, &p, &f, &en()), want_en);
        assert_eq!(text::display_predecessors(&links, &p, &f, &en()), want_en);
        assert_eq!(format_slack(Minutes::new(-960), &f, &fr()), "-2 jours");
        assert_eq!(
            text::display_minutes_as(
                Minutes::new(-960),
                DurationUnit::Day,
                TimeBasis::Working,
                &f,
                &fr()
            ),
            "-2 jours"
        );
        // Libellés lus dans les `.ftl` : mêmes textes.
        let fr_texts = crate::i18n::Texts::new(crate::i18n::Language::Fr);
        let en_texts = crate::i18n::Texts::new(crate::i18n::Language::En);
        for (d, want_fr, want_en) in &durations {
            assert_eq!(format_duration(d, &f, fr_texts.format_labels()), *want_fr);
            let en_text = format_duration(d, &f, en_texts.format_labels());
            if d.basis() == TimeBasis::Working {
                assert_eq!(en_text, *want_en);
            }
        }
        assert_eq!(
            format_predecessors(&links, &p, &f, fr_texts.format_labels()),
            want_fr
        );
        assert_eq!(
            super::format_percent(
                plannercraft_core::Percent::new(5_000).unwrap(),
                fr_texts.format_labels()
            ),
            "50\u{a0}%"
        );
        assert_eq!(
            super::format_percent(
                plannercraft_core::Percent::new(5_000).unwrap(),
                en_texts.format_labels()
            ),
            "50%"
        );
    }

    /// Pluriel piloté par `FormatLabels::plural` (clé `fmt-plural-rule`),
    /// indépendamment de l'ordre des dates.
    #[test]
    fn fmt_plural_rule_from_labels_not_date_order() {
        let f = factors();
        let one_and_half = days(720, TimeBasis::Working, false);
        // Règle française avec un ordre de dates anglais.
        let below_two = FormatLabels {
            date_order: DateOrder::MonthDayYear,
            plural: PluralRule::BelowTwo,
            ..fr()
        };
        assert_eq!(format_duration(&one_and_half, &f, &below_two), "1,5 jour");
        // Règle anglaise avec un ordre de dates français.
        let exactly_one = FormatLabels {
            date_order: DateOrder::DayMonthYear,
            plural: PluralRule::ExactlyOne,
            ..fr()
        };
        assert_eq!(format_duration(&one_and_half, &f, &exactly_one), "1,5 jours");
        assert_eq!(
            format_duration(&days(0, TimeBasis::Working, false), &f, &exactly_one),
            "0 jours"
        );
        assert_eq!(PluralRule::from_key(" below-two "), Some(PluralRule::BelowTwo));
        assert_eq!(PluralRule::from_key("exactly-one"), Some(PluralRule::ExactlyOne));
        assert_eq!(PluralRule::from_key("dual"), None);
    }

    /// Pluriel lu dans les `.ftl` réels : « 1,5 jour » en français,
    /// « 1.5 days » en anglais.
    #[test]
    fn fmt_plural_rule_via_ftl_key() {
        let f = factors();
        let one_and_half = days(720, TimeBasis::Working, false);
        let fr_texts = crate::i18n::Texts::new(crate::i18n::Language::Fr);
        assert_eq!(fr_texts.format_labels().plural, PluralRule::BelowTwo);
        assert_eq!(
            format_duration(&one_and_half, &f, fr_texts.format_labels()),
            "1,5 jour"
        );
        let en_texts = crate::i18n::Texts::new(crate::i18n::Language::En);
        assert_eq!(en_texts.format_labels().plural, PluralRule::ExactlyOne);
        assert_eq!(
            format_duration(&one_and_half, &f, en_texts.format_labels()),
            "1.5 days"
        );
    }
}
