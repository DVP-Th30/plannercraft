//! Texte d'affichage des colonnes, à la manière de la table de MS Project
//! (J7, lot L6 : besoins N1, N2, N11 de `docs/ui-needs.md`) : durées en
//! toutes lettres (« 5 jours », « 1,5 day »), décalages et prédécesseurs
//! (« 3FD+2 jours »), % achevé (« 50 % »), noms des ressources
//! (« Maçon[50 %];Électricien »).
//!
//! Les mots viennent de l'appelant ([`DisplayLabels`] : l'interface les lit
//! dans ses `.ftl`) ; ce module écrit les nombres (arithmétique entière sur
//! le point fixe du cœur, jamais de flottant), l'accord des unités et
//! l'assemblage. Il n'est pas relu : la saisie passe par
//! [`parse_duration`](super::parse_duration),
//! [`parse_predecessors`](super::parse_predecessors) et
//! [`parse_percent`](super::parse_percent), qui acceptent ces textes.
//!
//! Conventions d'affichage (hypothèses H-UI de `docs/features/ui.md`) :
//! pluriel par [`PluralRule`], suffixe `?` d'une durée estimée, ID seul pour
//! un lien fin à début sans décalage, `%` entier tronqué (⌊ ⌋) dans la
//! colonne % achevé, unités d'affectation omises à 100 %.

use std::fmt::Write as _;

use super::lex::{Cursor, bounded_chars};
use super::{TextContext, TextError};

use plannercraft_core::{
    Assignment, ConversionFactors, Decimal, Duration, DurationUnit, Lag, LagValue, Link, LinkType, Minutes,
    Percent, Project, ResourceKind, TimeBasis, Units,
};

/// Règle de pluriel des unités (« 1,5 jour » en français, « 1.5 days » en
/// anglais).
///
/// Hypothèse H-UI : en français, singulier si |valeur| < 2 (« 0 jour »,
/// « 1,5 jour », « -1 jour ») ; en anglais, singulier si |valeur| = 1
/// exactement (« 1 day », « 0 days », « 1.5 days »).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PluralRule {
    /// Singulier si |v| < 2 (clé `below-two`, français).
    BelowTwo,
    /// Singulier si |v| = 1 (clé `exactly-one`, anglais ; repli).
    #[default]
    ExactlyOne,
}

impl PluralRule {
    /// Règle désignée par une clé (`below-two`, `exactly-one`, espaces
    /// ignorés) ; `None` pour une clé inconnue.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "below-two" => Some(Self::BelowTwo),
            "exactly-one" => Some(Self::ExactlyOne),
            _ => None,
        }
    }

    /// Vrai si la valeur prend la forme singulière.
    #[must_use]
    pub fn is_one(self, d: Decimal) -> bool {
        let d = d.normalized();
        let abs = d.mantissa().unsigned_abs();
        match self {
            Self::ExactlyOne => d.scale() == 0 && abs == 1,
            Self::BelowTwo => 10_u64
                .checked_pow(u32::from(d.scale()))
                .and_then(|den| den.checked_mul(2))
                .is_some_and(|two| abs < two),
        }
    }
}

/// Mots et marques d'une langue d'affichage, fournis par l'appelant.
pub trait DisplayLabels {
    /// Séparateur décimal (français `,`, anglais `.`).
    fn decimal_sep(&self) -> char;
    /// Séparateur de liste (prédécesseurs, ressources : `;` ou `,`).
    fn list_sep(&self) -> &str;
    /// Règle de pluriel des unités.
    fn plural(&self) -> PluralRule;
    /// Mot d'une unité dans une base de temps, au singulier ou non ; vide :
    /// la valeur est écrite seule.
    fn unit_word(&self, unit: DurationUnit, basis: TimeBasis, singular: bool) -> &str;
    /// Code d'un type de lien (français `FD`, anglais `FS`…).
    fn link_code(&self, kind: LinkType) -> &str;
    /// Texte d'une valeur qui ne peut pas être écrite (« ? »).
    fn unknown(&self) -> &str;
    /// Suffixe d'un pourcentage (français « \u{a0}% », anglais « % »).
    fn percent_suffix(&self) -> &str;
}

/// Suffixe d'une durée estimée (« 1 jour? »), le même dans les deux langues.
pub const ESTIMATED_SUFFIX: char = '?';

/// Écrit un décimal du cœur avec le séparateur `sep` (« -2,5 », « 31 ») ;
/// zéros finaux retirés.
pub fn write_decimal(out: &mut String, d: Decimal, sep: char) {
    let d = d.normalized();
    let abs = d.mantissa().unsigned_abs();
    if d.mantissa() < 0 {
        out.push('-');
    }
    let Some(den) = 10_u64.checked_pow(u32::from(d.scale())) else {
        // Échelle bornée à 6 par le cœur : branche inatteignable en pratique.
        let _ = write!(out, "{abs}");
        return;
    };
    let int_part = abs.checked_div(den).unwrap_or(abs);
    let frac_part = abs.checked_rem(den).unwrap_or(0);
    let _ = write!(out, "{int_part}");
    if d.scale() > 0 {
        let width = usize::from(d.scale());
        out.push(sep);
        let _ = write!(out, "{frac_part:0width$}");
    }
}

/// Écrit une valeur suivie du mot de son unité, accordé.
fn write_value_unit(
    out: &mut String,
    value: Decimal,
    unit: DurationUnit,
    basis: TimeBasis,
    l: &impl DisplayLabels,
) {
    write_decimal(out, value, l.decimal_sep());
    let word = l.unit_word(unit, basis, l.plural().is_one(value));
    if !word.is_empty() {
        out.push(' ');
        out.push_str(word);
    }
}

/// Écrit des minutes dans une unité et une base (« 31 jours », deux
/// décimales au plus, R-009), avec le suffixe d'estimation si demandé ;
/// `l.unknown()` si la conversion échoue (règle 10 : jamais de panique).
pub fn write_display_duration(
    out: &mut String,
    minutes: Minutes,
    unit: DurationUnit,
    basis: TimeBasis,
    estimated: bool,
    f: &ConversionFactors,
    l: &impl DisplayLabels,
) {
    match f.to_unit_value(minutes, unit, basis) {
        Ok(value) => {
            write_value_unit(out, value, unit, basis, l);
            if estimated {
                out.push(ESTIMATED_SUFFIX);
            }
        }
        Err(_) => out.push_str(l.unknown()),
    }
}

/// Durée d'une tâche dans son unité (« 5 jours », « 2,5 jours »,
/// « 1 jour? », « 5 jours écoulés »).
#[must_use]
pub fn display_duration(d: &Duration, f: &ConversionFactors, l: &impl DisplayLabels) -> String {
    let mut out = String::new();
    write_display_duration(&mut out, d.minutes(), d.unit(), d.basis(), d.is_estimated(), f, l);
    out
}

/// Minutes dans une unité et une base données (« 31 jours ») : durée d'une
/// récapitulative, marge, variation.
#[must_use]
pub fn display_minutes_as(
    m: Minutes,
    unit: DurationUnit,
    basis: TimeBasis,
    f: &ConversionFactors,
    l: &impl DisplayLabels,
) -> String {
    let mut out = String::new();
    write_display_duration(&mut out, m, unit, basis, false, f, l);
    out
}

/// Écrit un décalage signé (« +2 jours », « -1 jour », « +50% ») ; rien pour
/// un décalage nul.
pub fn write_display_lag(out: &mut String, lag: &Lag, f: &ConversionFactors, l: &impl DisplayLabels) {
    match lag.value {
        LagValue::Time { minutes, unit } => {
            if minutes == Minutes::ZERO {
                return;
            }
            if !minutes.is_negative() {
                out.push('+');
            }
            write_display_duration(out, minutes, unit, lag.basis, false, f, l);
        }
        LagValue::Percent { percent } => {
            let raw = percent.get();
            if raw == 0 {
                return;
            }
            if raw > 0 {
                out.push('+');
            }
            // 1/10 000 → pour cent avec deux décimales.
            match Decimal::new(raw, 2) {
                Ok(value) => {
                    write_decimal(out, value, l.decimal_sep());
                    out.push('%');
                }
                Err(_) => out.push_str(l.unknown()),
            }
        }
    }
}

/// Vrai si le décalage est nul (temps ou pourcentage).
const fn lag_is_zero(lag: &Lag) -> bool {
    match lag.value {
        LagValue::Time { minutes, .. } => minutes.get() == 0,
        LagValue::Percent { percent } => percent.get() == 0,
    }
}

/// Liste des prédécesseurs (« 3;5DD+2 jours ») : ID (et non UID), seul pour
/// un lien fin à début sans décalage ; `l.unknown()` à la place de l'ID d'un
/// UID absent du projet ; ordre des liens conservé.
#[must_use]
pub fn display_predecessors(
    links: &[Link],
    project: &Project,
    f: &ConversionFactors,
    l: &impl DisplayLabels,
) -> String {
    let mut out = String::new();
    for (k, link) in links.iter().enumerate() {
        if k > 0 {
            out.push_str(l.list_sep());
        }
        match project.task_id(link.predecessor) {
            Some(id) => {
                let _ = write!(out, "{}", id.get());
            }
            None => out.push_str(l.unknown()),
        }
        if link.kind == LinkType::FinishToStart && lag_is_zero(&link.lag) {
            continue;
        }
        out.push_str(l.link_code(link.kind));
        write_display_lag(&mut out, &link.lag, f, l);
    }
    out
}

/// % achevé en pourcentage entier tronqué (« 33 % » pour 3 333/10 000 ;
/// hypothèse H-UI : MS Project affiche des pourcentages entiers).
#[must_use]
pub fn display_percent(p: Percent, l: &impl DisplayLabels) -> String {
    // 0 ≤ p ≤ 10 000 : la division entière ne peut pas échouer.
    let whole = p.get().checked_div(100).unwrap_or(0);
    let mut out = whole.to_string();
    out.push_str(l.percent_suffix());
    out
}

/// Écrit des unités d'affectation en pourcentage (10 000 → « 100 »,
/// 1 250 → « 12,5 »).
fn write_units_percent(out: &mut String, units: Units, l: &impl DisplayLabels) {
    match Decimal::new(units.get(), 2) {
        Ok(value) => write_decimal(out, value, l.decimal_sep()),
        Err(_) => out.push_str(l.unknown()),
    }
}

/// N11 : noms des ressources de `assignments` (affectations d'une même
/// tâche, dans l'ordre du projet), séparés par `l.list_sep()` :
///
/// - ressource travail : son nom, suivi de « [50 %] » si ses unités ne
///   valent pas 100 % (« Maçon[50 %];Électricien ») ;
/// - ressource matérielle : son nom suivi de la quantité et de l'étiquette
///   (« Ciment[5 m³] », toujours écrite : une quantité n'a pas de valeur
///   implicite) ;
/// - ressource coût : son nom seul ;
/// - affectation sans ressource (fictive, O24) : omise ; ressource absente
///   du projet : `l.unknown()`.
///
/// Hypothèse (champ « Noms ressources » de MS Project, lecture seule en
/// J7) : crochets accolés au nom, suffixe de pourcentage de la langue.
#[must_use]
pub fn format_resource_names<'a>(
    assignments: impl IntoIterator<Item = &'a Assignment>,
    project: &Project,
    l: &impl DisplayLabels,
) -> String {
    let mut out = String::new();
    let mut first = true;
    for assignment in assignments {
        let Some(uid) = assignment.resource else {
            continue;
        };
        if !first {
            out.push_str(l.list_sep());
        }
        first = false;
        let Some(resource) = project.resources.get(uid) else {
            out.push_str(l.unknown());
            continue;
        };
        out.push_str(&resource.name);
        match &resource.kind {
            ResourceKind::Work => {
                if assignment.units != Units::FULL {
                    out.push('[');
                    write_units_percent(&mut out, assignment.units, l);
                    out.push_str(l.percent_suffix());
                    out.push(']');
                }
            }
            ResourceKind::Material { label } => {
                out.push('[');
                match Decimal::new(assignment.units.get(), 4) {
                    Ok(value) => write_decimal(&mut out, value, l.decimal_sep()),
                    Err(_) => out.push_str(l.unknown()),
                }
                if !label.is_empty() {
                    out.push(' ');
                    out.push_str(label);
                }
                out.push(']');
            }
            ResourceKind::Cost => {}
        }
    }
    out
}

/// Lit un % achevé saisi dans la colonne (« 50 », « 50 % », « 12,5% ») :
/// nombre dans la langue du contexte (R-140 : virgule ou point), `%`
/// facultatif (la colonne est en pourcentage, comme MS Project), espaces
/// admis autour ; arrondi au 1/10 000 par R-001.
///
/// # Errors
///
/// [`TextError::Empty`], [`TextError::BadNumber`], [`TextError::UnknownUnit`]
/// (autre suffixe que `%`), [`TextError::Negative`] (« -5 »),
/// [`TextError::OutOfRange`] au-delà de 100 %, [`TextError::TooLong`].
pub fn parse_percent(s: &str, ctx: &TextContext) -> Result<Percent, TextError> {
    let chars = bounded_chars(s)?;
    let mut cur = Cursor::new(&chars);
    cur.skip_spaces();
    if cur.at_end() {
        return Err(TextError::Empty);
    }
    if cur.peek() == Some('-') {
        return Err(TextError::Negative);
    }
    let value = cur.decimal_in(ctx.language)?;
    cur.skip_spaces();
    let rest: String = cur.rest().iter().collect();
    let rest = rest.trim_end();
    if !(rest.is_empty() || rest == "%") {
        return Err(TextError::UnknownUnit {
            unit: rest.to_owned(),
        });
    }
    Percent::from_percent(value).map_err(|_| TextError::OutOfRange)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::arithmetic_side_effects,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::{
        Assignment, AssignmentUid, BuiltinCalendarNames, ConversionFactors, Decimal, DurationUnit, LinkType,
        Percent, Project, ProjectInit, Resource, ResourceKind, ResourceUid, ScheduleAnchor, TaskUid,
        TimeBasis, Units,
    };

    use super::{DisplayLabels, PluralRule, display_minutes_as, display_percent, format_resource_names};

    /// Libellés minimaux, français ou anglais.
    struct Labels {
        fr: bool,
    }

    impl DisplayLabels for Labels {
        fn decimal_sep(&self) -> char {
            if self.fr { ',' } else { '.' }
        }
        fn list_sep(&self) -> &str {
            if self.fr { ";" } else { "," }
        }
        fn plural(&self) -> PluralRule {
            if self.fr {
                PluralRule::BelowTwo
            } else {
                PluralRule::ExactlyOne
            }
        }
        fn unit_word(&self, _: DurationUnit, _: TimeBasis, singular: bool) -> &str {
            match (self.fr, singular) {
                (true, true) => "jour",
                (true, false) => "jours",
                (false, true) => "day",
                (false, false) => "days",
            }
        }
        fn link_code(&self, kind: LinkType) -> &str {
            super::super::link_code(
                kind,
                if self.fr {
                    super::super::Language::Fr
                } else {
                    super::super::Language::En
                },
            )
        }
        fn unknown(&self) -> &'static str {
            "?"
        }
        fn percent_suffix(&self) -> &str {
            if self.fr { "\u{a0}%" } else { "%" }
        }
    }

    const FR: Labels = Labels { fr: true };
    const EN: Labels = Labels { fr: false };

    fn project() -> Project {
        let now = "2026-10-19T08:00".parse().unwrap();
        let mut p = Project::new(ProjectInit {
            name: "T".to_owned(),
            anchor: ScheduleAnchor::FromStart(now),
            current_date: now,
            calendar_names: BuiltinCalendarNames::default(),
        });
        for (uid, name, kind) in [
            (1, "Maçon", ResourceKind::Work),
            (2, "Électricien", ResourceKind::Work),
            (
                3,
                "Ciment",
                ResourceKind::Material {
                    label: "m³".to_owned(),
                },
            ),
            (4, "Frais", ResourceKind::Cost),
        ] {
            let mut r = Resource::new(ResourceUid::new(uid), name.to_owned());
            r.kind = kind;
            p.resources.push(r).unwrap();
        }
        p
    }

    fn assignment(uid: u32, resource: Option<u32>, units: i64) -> Assignment {
        let mut a = Assignment::new(
            AssignmentUid::new(uid),
            TaskUid::new(1),
            resource.map(ResourceUid::new),
        );
        a.units = Units::new(units).unwrap();
        a
    }

    #[test]
    fn display_resource_names_fr_en() {
        let p = project();
        let list = [
            assignment(1, Some(1), 5_000),
            assignment(2, Some(2), 10_000),
            assignment(3, Some(3), 50_000),
            assignment(4, Some(4), 0),
        ];
        assert_eq!(
            format_resource_names(&list, &p, &FR),
            "Maçon[50\u{a0}%];Électricien;Ciment[5 m³];Frais"
        );
        assert_eq!(
            format_resource_names(&list, &p, &EN),
            "Maçon[50%],Électricien,Ciment[5 m³],Frais"
        );
        // Unités fractionnaires, fictive omise, ressource inconnue.
        let odd = [
            assignment(5, None, 10_000),
            assignment(6, Some(1), 1_250),
            assignment(7, Some(99), 10_000),
            assignment(8, Some(1), 0),
        ];
        assert_eq!(
            format_resource_names(&odd, &p, &FR),
            "Maçon[12,5\u{a0}%];?;Maçon[0\u{a0}%]"
        );
        assert_eq!(format_resource_names(&[], &p, &FR), "");
        // Quantité matérielle fractionnaire, étiquette vide.
        let mut q = project();
        q.resources.get_mut(ResourceUid::new(3)).unwrap().kind =
            ResourceKind::Material { label: String::new() };
        assert_eq!(
            format_resource_names(&[assignment(9, Some(3), 15_000)], &q, &EN),
            "Ciment[1.5]"
        );
    }

    #[test]
    fn display_parse_percent() {
        use super::super::{Language, TextContext, TextError};
        use super::parse_percent;
        let p = project();
        let fr = TextContext::for_project(&p, Language::Fr);
        let en = TextContext::for_project(&p, Language::En);
        let get = |s: &str, ctx: &TextContext| parse_percent(s, ctx).map(Percent::get);
        assert_eq!(get("50", &fr), Ok(5_000));
        assert_eq!(get(" 50 % ", &fr), Ok(5_000));
        assert_eq!(get("50\u{a0}%", &fr), Ok(5_000));
        assert_eq!(get("12,5%", &fr), Ok(1_250));
        assert_eq!(get("12.5", &en), Ok(1_250));
        assert_eq!(get("100", &en), Ok(10_000));
        assert_eq!(get("0", &en), Ok(0));
        assert_eq!(get("", &fr), Err(TextError::Empty));
        assert_eq!(get("101", &fr), Err(TextError::OutOfRange));
        assert_eq!(get("-5", &fr), Err(TextError::Negative));
        assert!(matches!(get("abc", &fr), Err(TextError::BadNumber { .. })));
        assert!(matches!(get("50j", &fr), Err(TextError::UnknownUnit { .. })));
        // Le texte affiché se relit.
        assert_eq!(
            get(&display_percent(Percent::new(3_300).unwrap(), &FR), &fr),
            Ok(3_300)
        );
    }

    #[test]
    fn display_percent_floor() {
        assert_eq!(display_percent(Percent::new(3_333).unwrap(), &FR), "33\u{a0}%");
        assert_eq!(display_percent(Percent::new(10_000).unwrap(), &EN), "100%");
        assert_eq!(display_percent(Percent::new(99).unwrap(), &EN), "0%");
        assert_eq!(display_percent(Percent::ZERO, &FR), "0\u{a0}%");
    }

    #[test]
    fn display_minutes_and_plural() {
        let f = ConversionFactors::new(480, 2_400, 20).unwrap();
        let m = |v| plannercraft_core::Minutes::new(v);
        assert_eq!(
            display_minutes_as(m(720), DurationUnit::Day, TimeBasis::Working, &f, &FR),
            "1,5 jour"
        );
        assert_eq!(
            display_minutes_as(m(720), DurationUnit::Day, TimeBasis::Working, &f, &EN),
            "1.5 days"
        );
        assert_eq!(
            display_minutes_as(m(-960), DurationUnit::Day, TimeBasis::Working, &f, &FR),
            "-2 jours"
        );
        assert_eq!(
            display_minutes_as(m(i64::MAX), DurationUnit::Minute, TimeBasis::Working, &f, &FR),
            "?"
        );
        assert!(PluralRule::BelowTwo.is_one(Decimal::new(-150, 2).unwrap()));
        assert!(!PluralRule::ExactlyOne.is_one(Decimal::new(150, 2).unwrap()));
        assert_eq!(PluralRule::from_key(" below-two "), Some(PluralRule::BelowTwo));
        assert_eq!(PluralRule::from_key("dual"), None);
    }
}
