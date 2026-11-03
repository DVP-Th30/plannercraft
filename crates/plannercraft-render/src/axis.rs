//! Axe du temps : minutes → abscisse en 1/16 px, entière (lot L7).
//!
//! Toute abscisse est un entier `i64` en seizièmes de pixel, calculé dans le
//! sens direct (instant → abscisse) par `round_div` du cœur : aucun flottant,
//! aucune date née d'un pixel. Hypothèses d'affichage H-R
//! (`docs/features/render.md`). La projection de l'interface
//! (`plannercraft-ui-egui::timescale`) calcule la même chose en `f32` : duplication
//! consignée (ADR-0012, D10), à résorber quand l'axe descendra dans ce crate.

use plannercraft_core::{Day, Moment, Project, Schedule, Weekday, round_div};

use crate::metrics::{FontRef, InterMetrics, TextMeasure};
use crate::request::{Lang, RangeSpec, Zoom};

/// Minutes d'un jour civil.
pub(crate) const MINUTES_PER_DAY: i64 = 1_440;

/// Seizièmes de pixel par pixel.
pub(crate) const SUB: i64 = 16;

/// Marge avant le début du projet, en jours (avant recul au début de semaine).
const LEAD_DAYS: i32 = 7;

/// Marge après la fin du projet, en jours.
const TRAIL_DAYS: i32 = 21;

/// Distance maximale (jours) de la date d'état à la plage du projet pour que
/// la plage automatique s'étende jusqu'à elle.
const STATUS_REACH_DAYS: i64 = 90;

/// Garde-fou : cellules au plus par niveau de l'en-tête.
const MAX_CELLS: usize = 8_192;

/// Échelle résolue (après `Zoom::Auto`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scale {
    /// 36 px par jour.
    Day,
    /// 12 px par jour.
    Week,
    /// 3,5 px par jour.
    Month,
    /// 1,2 px par jour (19,2/16, exact : 96/5 seizièmes ; J8 lot L11, comme
    /// l'écran J7).
    Quarter,
    /// Échelle continue de `Zoom::Fill` : `num / den` seizièmes de pixel
    /// par jour (largeur remplie / jours, exacte), niveaux de l'en-tête
    /// `tiers` (ceux de l'échelle discrète atteinte).
    Fill {
        /// Niveaux de l'en-tête (haut, bas).
        tiers: (TierUnit, TierUnit),
        /// Seizièmes de pixel par jour : numérateur (≥ 1).
        num: i64,
        /// Seizièmes de pixel par jour : dénominateur (≥ 1).
        den: i64,
    },
}

impl Scale {
    /// Seizièmes de pixel par jour civil, en fraction `(numérateur,
    /// dénominateur)` : exacte pour les quatre échelles de l'interface
    /// (36, 12, 3,5 et 1,2 px par jour), pour que l'écran et la scène
    /// tombent au même 1/16 px (P55).
    pub(crate) const fn rate(self) -> (i64, i64) {
        match self {
            Self::Day => (576, 1),
            Self::Week => (192, 1),
            Self::Month => (56, 1),
            Self::Quarter => (96, 5),
            Self::Fill { num, den, .. } => (num, den),
        }
    }

    /// Seizièmes de pixel par jour civil, arrondis à l'entier inférieur
    /// (seuils de la scène).
    pub(crate) const fn px16_per_day(self) -> i64 {
        let (num, den) = self.rate();
        match num.checked_div(den) {
            Some(v) => v,
            None => num,
        }
    }

    /// Niveaux de l'en-tête (haut, bas), comme l'interface.
    pub(crate) const fn tiers(self) -> (TierUnit, TierUnit) {
        match self {
            Self::Day => (TierUnit::Week, TierUnit::Day),
            Self::Week => (TierUnit::Month, TierUnit::Week),
            Self::Month => (TierUnit::Quarter, TierUnit::Month),
            Self::Quarter => (TierUnit::Year, TierUnit::Quarter),
            Self::Fill { tiers, .. } => tiers,
        }
    }
}

/// Échelle d'une requête pour une plage de `days` jours.
///
/// `Auto { width_px }` (H-R, comme « Ajuster » de l'interface) : Semaine si
/// `jours × 12 ≤ largeur` ; sinon Trimestre si `jours × 3,5 > 1,5 × largeur`
/// (écrit `jours × 7 > 3 × largeur`) ; sinon Mois. Largeur nulle : Mois.
///
/// `Fill { width_px }` (J8 lot L16) : `largeur × 16 / jours` seizièmes de
/// pixel par jour, exactement (la plage remplit la largeur) ; au-delà de
/// Jour (36 px), Jour ; niveaux de l'en-tête (mois et semaines) de Semaine
/// à partir de 3,5 px par jour, de Mois à partir de 1,2 px, de Trimestre en
/// dessous (hypothèse de design, H-R4 : un cran plus fin que l'échelle
/// discrète atteinte, les libellés suivant H-R5). Largeur nulle : Mois.
pub(crate) fn resolve_scale(zoom: Zoom, days: i64) -> Scale {
    match zoom {
        Zoom::Day => Scale::Day,
        Zoom::Week => Scale::Week,
        Zoom::Month => Scale::Month,
        Zoom::Quarter => Scale::Quarter,
        Zoom::Auto { width_px } => {
            let width = i64::from(width_px);
            if width == 0 {
                Scale::Month
            } else if days.saturating_mul(12) <= width {
                Scale::Week
            } else if days.saturating_mul(7) > width.saturating_mul(3) {
                Scale::Quarter
            } else {
                Scale::Month
            }
        }
        Zoom::Fill { width_px } => fill_scale(width_px, days),
    }
}

/// Échelle continue de `Zoom::Fill` (voir [`resolve_scale`]).
fn fill_scale(width_px: u32, days: i64) -> Scale {
    if width_px == 0 {
        return Scale::Month;
    }
    let width16 = i64::from(width_px).saturating_mul(SUB);
    let days = days.max(1);
    let reaches = |s: Scale| width16 >= days.saturating_mul(s.px16_per_day());
    if reaches(Scale::Day) {
        return Scale::Day;
    }
    // Niveaux d'un cran plus fins que l'échelle discrète atteinte : à
    // 3,5 px par jour une semaine fait 24,5 px, assez pour son numéro ou son
    // jour (H-R5 choisit la variante), comme les plannings de chantier de
    // référence (mois et semaines même sur deux ans en A3).
    let base = if reaches(Scale::Month) {
        Scale::Week
    } else if reaches(Scale::Quarter) {
        Scale::Month
    } else {
        Scale::Quarter
    };
    Scale::Fill {
        tiers: base.tiers(),
        num: width16,
        den: days,
    }
}

/// Plage de jours affichée, fin exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DaySpan {
    /// Premier jour (abscisse 0 au minuit de ce jour).
    pub start: Day,
    /// Jour suivant le dernier jour affiché.
    pub end: Day,
    /// La plage demandée dépassait `max_days` : fin coupée.
    pub truncated: bool,
}

impl DaySpan {
    /// Nombre de jours (≥ 0).
    pub(crate) fn days(self) -> i64 {
        days_between(self.start, self.end).max(0)
    }
}

/// Jours de `from` à `to` (négatif si `to` précède `from`).
fn days_between(from: Day, to: Day) -> i64 {
    i64::from(to.index()).saturating_sub(i64::from(from.index()))
}

/// Recule `d` jusqu'au jour `week_start` (au plus 6 pas, arrêt au bord du
/// domaine).
pub(crate) fn align_to_week_start(d: Day, week_start: Weekday) -> Day {
    let mut day = d;
    for _ in 0..6 {
        if day.weekday() == week_start {
            break;
        }
        match day.checked_add_days(-1) {
            Ok(prev) => day = prev,
            Err(_) => break,
        }
    }
    day
}

/// Plage d'une requête, coupée à `max_days` jours (au moins un jour).
///
/// `Auto` (H-R, même règle que la plage par défaut de l'interface) : jour de
/// début du projet − 7, jour de fin + 21 (exclusif), étendue jusqu'à la date
/// d'état effective si elle est à moins de 90 jours et que l'extension tient
/// dans `max_days`, début reculé au premier jour de semaine du projet.
/// `Span { from, to }` : jours inclus, bornes remises dans l'ordre.
pub(crate) fn resolve_range(
    range: RangeSpec,
    project: &Project,
    schedule: &Schedule,
    max_days: u32,
) -> DaySpan {
    let max_days = i32::try_from(max_days.max(1)).unwrap_or(i32::MAX);
    let (start, end) = match range {
        RangeSpec::Span { from, to } => {
            let (a, b) = if to < from { (to, from) } else { (from, to) };
            (a, b.checked_add_days(1).unwrap_or(Day::MAX))
        }
        RangeSpec::Auto => auto_range(project, schedule, i64::from(max_days)),
    };
    let max_end = start.checked_add_days(max_days).unwrap_or(Day::MAX);
    let min_end = start.checked_add_days(1).unwrap_or(Day::MAX);
    let truncated = end > max_end;
    DaySpan {
        start,
        end: end.min(max_end).max(min_end),
        truncated,
    }
}

/// Plage automatique (avant coupe).
fn auto_range(project: &Project, schedule: &Schedule, max_days: i64) -> (Day, Day) {
    let week_start = project.options.week_start_day;
    let first = schedule.project_start.day();
    let last = schedule.project_finish.day().max(first);
    let start = first.checked_add_days(-LEAD_DAYS).unwrap_or(Day::MIN);
    let end = last.checked_add_days(TRAIL_DAYS).unwrap_or(Day::MAX);
    let status = project.effective_status_date().day();
    let status_end = status.checked_add_days(1).unwrap_or(Day::MAX);
    let mut ext_start = start;
    let mut ext_end = end;
    if status < start && days_between(status, start) <= STATUS_REACH_DAYS {
        ext_start = status;
    }
    if status_end > end && days_between(end, status_end) <= STATUS_REACH_DAYS {
        ext_end = status_end;
    }
    let (start, end) = if days_between(align_to_week_start(ext_start, week_start), ext_end) <= max_days {
        (ext_start, ext_end)
    } else {
        (start, end)
    };
    (align_to_week_start(start, week_start), end)
}

/// Projection temps → abscisse en 1/16 px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Axis {
    /// Minute du minuit du premier jour.
    origin: i64,
    /// Seizièmes de pixel par jour : numérateur.
    px16: i64,
    /// Seizièmes de pixel par jour : dénominateur (≥ 1).
    den: i64,
    /// Abscisse (1/16 px) du début de la plage.
    left16: i64,
    /// Largeur du contenu (1/16 px).
    width16: i64,
}

impl Axis {
    /// Axe de `span` à l'échelle `scale`, le début de plage étant en `left16`.
    pub(crate) fn new(span: DaySpan, scale: Scale, left16: i64) -> Self {
        let (px16, den) = scale.rate();
        let width16 = round_div(
            i128::from(span.days()).saturating_mul(i128::from(px16)),
            i128::from(den),
        )
        .unwrap_or(0);
        Self {
            origin: day_minutes(span.start),
            px16,
            den,
            left16,
            width16,
        }
    }

    /// Abscisse d'une minute absolue :
    /// `left16 + round_div((minutes − origine) × px16, 1 440 × den)`.
    pub(crate) fn x16_minutes(&self, minutes: i64) -> i64 {
        let delta = i128::from(minutes.saturating_sub(self.origin));
        let px16 = i128::from(self.px16);
        let den = i128::from(MINUTES_PER_DAY).saturating_mul(i128::from(self.den.max(1)));
        let x = round_div(delta.saturating_mul(px16), den).unwrap_or(0);
        self.left16.saturating_add(x)
    }

    /// Seizièmes de pixel par jour, en fraction `(numérateur,
    /// dénominateur)`.
    pub(crate) const fn rate(&self) -> (i64, i64) {
        (self.px16, self.den)
    }

    /// Abscisse d'un instant.
    pub(crate) fn x16(&self, m: Moment) -> i64 {
        self.x16_minutes(m.minutes())
    }

    /// Abscisse du minuit d'un jour.
    pub(crate) fn x16_day(&self, d: Day) -> i64 {
        self.x16_minutes(day_minutes(d))
    }

    /// Abscisse du début de plage.
    pub(crate) const fn left16(&self) -> i64 {
        self.left16
    }

    /// Abscisse de la fin de plage.
    pub(crate) const fn right16(&self) -> i64 {
        self.left16.saturating_add(self.width16)
    }

    /// Largeur du contenu.
    pub(crate) const fn width16(&self) -> i64 {
        self.width16
    }
}

/// Minute du minuit d'un jour (même origine que `Moment::minutes`).
fn day_minutes(d: Day) -> i64 {
    i64::from(d.index()).saturating_mul(MINUTES_PER_DAY)
}

/// Écrit `v / 16` avec quatre décimales fixes (`{}.{:04}`, reste × 625).
pub(crate) fn push_fixed16(out: &mut String, v: i64) {
    use std::fmt::Write as _;
    let neg = v < 0;
    let abs = v.unsigned_abs();
    let int = abs / 16;
    let frac = (abs % 16).saturating_mul(625);
    let sign = if neg { "-" } else { "" };
    let _ = write!(out, "{sign}{int}.{frac:04}");
}

/// Unité d'un niveau de l'en-tête.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TierUnit {
    /// Jour.
    Day,
    /// Semaine (commence au premier jour de semaine du projet).
    Week,
    /// Mois civil.
    Month,
    /// Trimestre civil.
    Quarter,
    /// Année civile.
    Year,
}

impl TierUnit {
    /// Durée minimale de l'unité, en jours.
    const fn min_days(self) -> i64 {
        match self {
            Self::Day => 1,
            Self::Week => 7,
            Self::Month => 28,
            Self::Quarter => 90,
            Self::Year => 365,
        }
    }
}

/// Date civile d'un jour : (année, mois 1–12, jour 1–31), lue sur l'écriture
/// ISO du cœur (`AAAA-MM-JJ`) pour ne dépendre que de `plannercraft-core`.
pub(crate) fn ymd(d: Day) -> (i32, u32, u32) {
    let s = d.to_string();
    let mut parts = s.split('-');
    let y = parts.next().and_then(|p| p.parse().ok()).unwrap_or(1984);
    let m = parts.next().and_then(|p| p.parse().ok()).unwrap_or(1);
    let dd = parts.next().and_then(|p| p.parse().ok()).unwrap_or(1);
    (y, m, dd)
}

/// Premier jour d'un mois, s'il est dans le domaine.
fn first_of_month(year: i32, month: u32) -> Option<Day> {
    format!("{year:04}-{month:02}-01").parse().ok()
}

/// Début de l'unité qui contient `d` (repli sur `Day::MIN`).
fn unit_start(d: Day, unit: TierUnit, week_start: Weekday) -> Day {
    let (year, month, _) = ymd(d);
    let start = match unit {
        TierUnit::Day => Some(d),
        TierUnit::Week => Some(align_to_week_start(d, week_start)),
        TierUnit::Month => first_of_month(year, month),
        TierUnit::Quarter => first_of_month(
            year,
            (month.saturating_sub(1) / 3).saturating_mul(3).saturating_add(1),
        ),
        TierUnit::Year => first_of_month(year, 1),
    };
    start.unwrap_or(Day::MIN)
}

/// Début de l'unité suivante ; `None` hors du domaine.
fn next_unit_start(cur: Day, unit: TierUnit) -> Option<Day> {
    let (year, month, _) = ymd(cur);
    let add_months = |n: u32| {
        let zero_based = month.saturating_sub(1).saturating_add(n);
        let y = year.checked_add(i32::try_from(zero_based / 12).ok()?)?;
        first_of_month(y, (zero_based % 12).saturating_add(1))
    };
    match unit {
        TierUnit::Day => cur.checked_add_days(1).ok(),
        TierUnit::Week => cur.checked_add_days(7).ok(),
        TierUnit::Month => add_months(1),
        TierUnit::Quarter => add_months(3),
        TierUnit::Year => first_of_month(year.checked_add(1)?, 1),
    }
}

/// Cellule d'un niveau de l'en-tête.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    /// Bord gauche (1/16 px), rogné au début de la plage.
    pub x0: i64,
    /// Bord droit (1/16 px), rogné à la fin de la plage.
    pub x1: i64,
    /// Le bord gauche est une vraie frontière d'unité (pas le bord de plage).
    pub boundary: bool,
    /// Étiquette (vide si aucune variante ne tient).
    pub label: String,
}

/// Cellules d'un niveau sur toute la plage (au plus [`MAX_CELLS`]) ;
/// étiquettes mesurées dans `font` (H-R11), numéros de semaine ISO si
/// `week_numbers` (niveau Semaine).
pub(crate) fn tier_cells(
    axis: &Axis,
    span: DaySpan,
    unit: TierUnit,
    week_start: Weekday,
    lang: Lang,
    font: FontRef,
    week_numbers: bool,
) -> Vec<Cell> {
    let level = label_level(unit, axis.rate(), lang, font, week_numbers);
    let mut cells = Vec::new();
    let mut cur = unit_start(span.start, unit, week_start);
    while cells.len() < MAX_CELLS && cur < span.end {
        let next = match next_unit_start(cur, unit) {
            Some(next) if next > cur => next,
            _ => span.end,
        };
        let boundary = cur >= span.start;
        let x0 = axis.x16_day(cur.max(span.start));
        let x1 = axis.x16_day(next.min(span.end));
        let label = level
            .and_then(|k| label_variants(cur, unit, lang, week_numbers).into_iter().nth(k))
            .unwrap_or_default();
        cells.push(Cell {
            x0,
            x1,
            boundary,
            label,
        });
        cur = next;
    }
    cells
}

/// Cellules du niveau des mois relatifs au démarrage `start` (H-UI-67) :
/// une par mois de la plage, étiquette « M1 », « M-3 » (vide si elle ne
/// tient pas dans la cellule, mesurée dans `font`).
pub(crate) fn relative_month_cells(axis: &Axis, span: DaySpan, start: Day, font: FontRef) -> Vec<Cell> {
    let mut cells = Vec::new();
    let mut cur = unit_start(span.start, TierUnit::Month, Weekday::Monday);
    while cells.len() < MAX_CELLS && cur < span.end {
        let next = match next_unit_start(cur, TierUnit::Month) {
            Some(next) if next > cur => next,
            _ => span.end,
        };
        let x0 = axis.x16_day(cur.max(span.start));
        let x1 = axis.x16_day(next.min(span.end));
        let label = crate::geometry::relative_month_label(crate::geometry::relative_month(cur, start));
        let fits = label_width16(&label, font).saturating_add(4 * SUB) <= x1.saturating_sub(x0);
        cells.push(Cell {
            x0,
            x1,
            boundary: cur >= span.start,
            label: if fits { label } else { String::new() },
        });
        cur = next;
    }
    cells
}

/// Noms longs des mois.
const MONTHS_LONG_FR: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];
const MONTHS_LONG_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
/// Noms courts des mois.
const MONTHS_SHORT_FR: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc.",
];
const MONTHS_SHORT_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
/// Initiales des jours, dimanche d'abord.
const WEEKDAY_INITIALS_FR: [&str; 7] = ["D", "L", "M", "M", "J", "V", "S"];
const WEEKDAY_INITIALS_EN: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];

/// Élément d'une table de libellés (`""` hors bornes).
fn pick<'a>(names: &[&'a str], i: usize) -> &'a str {
    names.get(i).copied().unwrap_or("")
}

/// Variantes d'étiquette d'une cellule, de la plus longue à la plus courte
/// (H-R5, mêmes formes que l'en-tête de l'interface) ; semaine numérotée
/// « S43 » / « W43 » si `week_numbers` (H-R5, modèle Chantier).
fn label_variants(day: Day, unit: TierUnit, lang: Lang, week_numbers: bool) -> Vec<String> {
    let (year, month, dom) = ymd(day);
    let month_i = usize::try_from(month.saturating_sub(1)).unwrap_or(0);
    let (month_long, short) = match lang {
        Lang::Fr => (pick(&MONTHS_LONG_FR, month_i), pick(&MONTHS_SHORT_FR, month_i)),
        Lang::En => (pick(&MONTHS_LONG_EN, month_i), pick(&MONTHS_SHORT_EN, month_i)),
    };
    match unit {
        TierUnit::Day => {
            let wd = Weekday::ALL.iter().position(|w| *w == day.weekday()).unwrap_or(0);
            let initial = match lang {
                Lang::Fr => pick(&WEEKDAY_INITIALS_FR, wd),
                Lang::En => pick(&WEEKDAY_INITIALS_EN, wd),
            };
            vec![format!("{initial} {dom}"), dom.to_string()]
        }
        TierUnit::Week if week_numbers => {
            let n = crate::scene::iso_week(day.checked_add_days(3).unwrap_or(day));
            let prefix = match lang {
                Lang::Fr => "S",
                Lang::En => "W",
            };
            vec![format!("{prefix}{n} · {dom}"), format!("{prefix}{n}")]
        }
        TierUnit::Week => {
            let full = match lang {
                Lang::Fr => format!("{dom} {short}"),
                Lang::En => format!("{short} {dom}"),
            };
            vec![full, dom.to_string()]
        }
        TierUnit::Month => {
            let yy = year.rem_euclid(100);
            let medium = match lang {
                Lang::Fr => format!("{short} {yy:02}"),
                Lang::En => format!("{short} '{yy:02}"),
            };
            let initial: String = month_long.chars().take(1).flat_map(char::to_uppercase).collect();
            vec![
                format!("{month_long} {year}"),
                month_long.to_owned(),
                medium,
                short.to_owned(),
                initial,
            ]
        }
        TierUnit::Quarter => {
            let q = (month.saturating_sub(1) / 3).saturating_add(1);
            let prefix = match lang {
                Lang::Fr => "T",
                Lang::En => "Q",
            };
            vec![format!("{prefix}{q} {year}"), format!("{prefix}{q}")]
        }
        TierUnit::Year => vec![year.to_string()],
    }
}

/// Largeur d'une étiquette en 1/16 px : mesure d'Inter (H-R11) plus 4 px
/// de marge.
pub(crate) fn label_width16(text: &str, font: FontRef) -> i64 {
    InterMetrics.width16(text, font).saturating_add(4 * SUB)
}

/// Rang de variante commun à un niveau : la plus longue dont toutes les
/// formes (les douze mois de 2026) tiennent dans la plus petite cellule de
/// l'unité ; `None` si aucune ne tient.
fn label_level(
    unit: TierUnit,
    rate: (i64, i64),
    lang: Lang,
    font: FontRef,
    week_numbers: bool,
) -> Option<usize> {
    let width = unit
        .min_days()
        .saturating_mul(rate.0)
        .checked_div(rate.1)
        .unwrap_or(0);
    let dom = match unit {
        TierUnit::Day | TierUnit::Week => 28,
        TierUnit::Month | TierUnit::Quarter | TierUnit::Year => 1,
    };
    let samples: Vec<Vec<String>> = (1..=12_u32)
        .filter_map(|m| format!("2026-{m:02}-{dom:02}").parse::<Day>().ok())
        .map(|d| label_variants(d, unit, lang, week_numbers))
        .collect();
    let levels = samples.iter().map(Vec::len).max().unwrap_or(0);
    (0..levels).find(|&k| {
        samples
            .iter()
            .all(|variants| variants.get(k).is_none_or(|v| label_width16(v, font) <= width))
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::*;

    #[test]
    fn fixed16_formats() {
        let f = |v| {
            let mut s = String::new();
            push_fixed16(&mut s, v);
            s
        };
        assert_eq!(f(448), "28.0000");
        assert_eq!(f(1), "0.0625");
        assert_eq!(f(15), "0.9375");
        assert_eq!(f(-1), "-0.0625");
        assert_eq!(f(-33), "-2.0625");
        assert_eq!(f(i64::MIN), "-576460752303423488.0000");
    }

    #[test]
    fn ymd_and_months() {
        let d: Day = "2026-11-02".parse().unwrap();
        assert_eq!(ymd(d), (2026, 11, 2));
        assert_eq!(ymd(Day::MIN), (1984, 1, 1));
        assert_eq!(ymd(Day::MAX), (2149, 12, 31));
        assert_eq!(next_unit_start(d, TierUnit::Month), "2026-12-01".parse().ok());
        assert_eq!(
            next_unit_start("2026-12-01".parse().unwrap(), TierUnit::Month),
            "2027-01-01".parse().ok()
        );
        assert_eq!(
            next_unit_start("2149-12-01".parse().unwrap(), TierUnit::Month),
            None
        );
        assert_eq!(
            unit_start(d, TierUnit::Quarter, Weekday::Monday),
            "2026-10-01".parse().unwrap()
        );
    }

    #[test]
    fn auto_scale() {
        assert_eq!(resolve_scale(Zoom::Auto { width_px: 0 }, 10), Scale::Month);
        assert_eq!(resolve_scale(Zoom::Auto { width_px: 1200 }, 100), Scale::Week);
        assert_eq!(resolve_scale(Zoom::Auto { width_px: 1200 }, 300), Scale::Month);
        assert_eq!(resolve_scale(Zoom::Auto { width_px: 1200 }, 600), Scale::Quarter);
    }

    /// `Zoom::Fill` : la plage remplit exactement la largeur ; niveaux de
    /// l'échelle discrète atteinte ; plafond Jour ; largeur nulle : Mois.
    #[test]
    fn fill_scale_fills_width() {
        assert_eq!(resolve_scale(Zoom::Fill { width_px: 0 }, 10), Scale::Month);
        assert_eq!(resolve_scale(Zoom::Fill { width_px: 1_000 }, 10), Scale::Day);
        let scale = resolve_scale(Zoom::Fill { width_px: 800 }, 70);
        assert_eq!(scale.rate(), (800 * 16, 70));
        assert_eq!(scale.tiers(), Scale::Week.tiers(), "11,4 px par jour ≥ 3,5");
        assert_eq!(
            resolve_scale(Zoom::Fill { width_px: 800 }, 400).tiers(),
            Scale::Month.tiers(),
            "2 px par jour : 1,2 ≤ … < 3,5"
        );
        assert_eq!(
            resolve_scale(Zoom::Fill { width_px: 800 }, 1_000).tiers(),
            Scale::Quarter.tiers(),
            "0,8 px par jour < 1,2"
        );
        let start: Day = "2026-10-19".parse().unwrap();
        let span = DaySpan {
            start,
            end: start.checked_add_days(70).unwrap(),
            truncated: false,
        };
        let axis = Axis::new(span, resolve_scale(Zoom::Fill { width_px: 800 }, 70), 0);
        assert_eq!(axis.width16(), 800 * 16);
    }

    /// Trimestre exact (J8, lot L11) : 1,2 px par jour, comme l'écran J7 ;
    /// cinq jours font 6 px pile, un jour 19,2 arrondi à 19/16 px.
    #[test]
    fn quarter_scale_is_exactly_1_2_px_per_day() {
        let start: Day = "2026-10-19".parse().unwrap();
        let span = DaySpan {
            start,
            end: start.checked_add_days(100).unwrap(),
            truncated: false,
        };
        let axis = Axis::new(span, Scale::Quarter, 0);
        assert_eq!(axis.width16(), 1_920);
        assert_eq!(axis.x16_day(start.checked_add_days(5).unwrap()), 96);
        assert_eq!(axis.x16_day(start.checked_add_days(1).unwrap()), 19);
        assert_eq!(Scale::Quarter.px16_per_day(), 19);
        for scale in [Scale::Day, Scale::Week, Scale::Month] {
            assert_eq!(scale.rate().1, 1);
        }
    }

    const BODY: FontRef = FontRef {
        weight: crate::metrics::FontWeight::Regular,
        size16: 192,
    };

    #[test]
    fn labels_fit_their_cells() {
        assert_eq!(
            label_level(TierUnit::Month, (192, 1), Lang::Fr, BODY, false),
            Some(0)
        );
        assert_eq!(
            label_level(TierUnit::Week, (192, 1), Lang::Fr, BODY, false),
            Some(0)
        );
        let d: Day = "2026-11-02".parse().unwrap();
        assert_eq!(label_variants(d, TierUnit::Week, Lang::En, false)[0], "Nov 2");
        assert_eq!(
            label_variants(d, TierUnit::Quarter, Lang::Fr, false)[0],
            "T4 2026"
        );
        // Chantier : semaine ISO (lun 19/10/2026 = S43).
        let w: Day = "2026-10-19".parse().unwrap();
        assert_eq!(
            label_variants(w, TierUnit::Week, Lang::Fr, true),
            ["S43 · 19", "S43"]
        );
        assert_eq!(label_variants(w, TierUnit::Week, Lang::En, true)[1], "W43");
    }
}
