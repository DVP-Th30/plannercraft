//! « Modifier le temps de travail » (J7, lot L12 ; brief § 8 « éditeur de
//! calendrier », S63 à S65, S71) : fenêtre modale sur les calendriers du
//! projet.
//!
//! - **Liste** : calendrier du projet d'abord, puis les autres calendriers
//!   de base, puis les calendriers dérivés (ceux des ressources), dans
//!   l'ordre du projet.
//! - **Mois** : chaque jour lu sur le calendrier compilé du moteur
//!   (`CompiledCalendar::day_ranges`) ; un jour couvert par une exception du
//!   calendrier ou de sa base (dépliée par `expand_exception`) est marqué.
//!   L'interface ne calcule rien : elle place les jours du mois dans une
//!   grille. Le mois montre le calendrier **enregistré** : un brouillon
//!   apparaît après « OK ».
//! - **Exceptions** (R-165, nom O44) : nom, du, au, ouvré (plages) ou chômé ;
//!   `calendar.set_exception`, `calendar.remove_exception`. Une exception
//!   récurrente (dépliage différent de sa période) est montrée en lecture
//!   seule.
//! - **Semaine type** (R-247) : hérité (calendrier dérivé seulement), chômé
//!   ou ouvré sur des plages ; `calendar.set_week` des seuls jours changés.
//! - **Nom** (R-248) : `calendar.update { name }` dans le lot de « OK ».
//! - **Nouveau** et **Supprimer** (R-248) : lots immédiats, une entrée
//!   d'annulation chacun (le registre ne sait pas désigner, dans un même
//!   lot, un calendrier créé plus tôt) ; `calendar.in_use` s'affiche avec
//!   ses utilisateurs.
//!
//! Brouillons par calendrier, créés à la première sélection ; « OK »
//! produit un seul lot pour tous les calendriers modifiés (ordre des UID ;
//! pour chacun : nom, semaine, retraits d'exceptions, ajouts et
//! remplacements).

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};
use plannercraft_automation::command::{
    CalendarAdd, CalendarDelete, CalendarException as ExceptionOp, CalendarExceptionKey, CalendarRef,
    CalendarSetWeek, CalendarUpdate,
};
use plannercraft_automation::json::{DaySettingJson, RangeJson, WeekSettingJson, WeekdayJson};
use plannercraft_automation::text::{DateOrder, DateRole, Language, TextContext, parse_date};
use plannercraft_automation::{Batch, Command, Patch, Session};
use plannercraft_core::calendar::{Calendar, CalendarKind, DayRule, DaySetting, TimeRange, expand_exception};
use plannercraft_core::{CalendarSet, CalendarUid, Day, DayRange, Project, TimeOfDay, Weekday};

use super::{batch_of, error_line, text_field};
use crate::edit::EditError;
use crate::i18n::{Arg, TextId, Texts};

/// Onglet de la fenêtre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WtTab {
    /// Exceptions datées.
    #[default]
    Exceptions,
    /// Semaine type.
    Week,
}

/// Calendrier proposé dans la liste.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarEntry {
    /// UID.
    pub uid: CalendarUid,
    /// Nom.
    pub name: String,
    /// Calendrier du projet.
    pub project: bool,
    /// Base d'un calendrier dérivé (`None` : calendrier de base).
    pub base: Option<CalendarUid>,
    /// Un calendrier au moins d'une ressource.
    pub resource: bool,
}

/// Réglage d'un jour de la semaine type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayChoice {
    /// Hérité de la base (calendrier dérivé seulement).
    Inherit,
    /// Chômé.
    NonWorking,
    /// Ouvré sur les plages saisies.
    Working,
}

/// Choix d'un jour proposés pour un calendrier de base ou dérivé.
#[must_use]
pub fn day_choices(base: bool) -> &'static [(DayChoice, TextId)] {
    const BASE: [(DayChoice, TextId); 2] = [
        (DayChoice::NonWorking, TextId::WtWeekNonWorking),
        (DayChoice::Working, TextId::WtWeekWorking),
    ];
    const DERIVED: [(DayChoice, TextId); 3] = [
        (DayChoice::Inherit, TextId::WtWeekInherit),
        (DayChoice::NonWorking, TextId::WtWeekNonWorking),
        (DayChoice::Working, TextId::WtWeekWorking),
    ];
    if base { &BASE } else { &DERIVED }
}

/// Jour de la semaine type (brouillon).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayDraft {
    /// Jour.
    pub day: Weekday,
    /// Réglage saisi.
    pub choice: DayChoice,
    /// Plages saisies (« 08:00-12:00, 13:00-17:00 »).
    pub ranges: String,
    /// Réglage relu à l'ouverture.
    original_choice: DayChoice,
    /// Plages relues à l'ouverture.
    original_ranges: String,
    /// Plages de la base pour ce jour (calendrier dérivé ; `None` : chômé
    /// ou sans base), affichées pour un jour hérité.
    pub inherited: Option<String>,
}

/// Champs saisis d'une exception.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExceptionFields {
    /// Nom.
    pub name: String,
    /// Premier jour (texte, R-143).
    pub from: String,
    /// Dernier jour, inclus (texte).
    pub to: String,
    /// Jours ouvrés (sur `ranges`) ; sinon chômés.
    pub working: bool,
    /// Plages des jours ouvrés.
    pub ranges: String,
}

/// Ligne de l'onglet Exceptions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExceptionRow {
    /// Champs saisis.
    pub fields: ExceptionFields,
    /// Exception récurrente du fichier : lecture seule (O44).
    pub recurring: bool,
    /// Exception relue : premier jour stocké et champs à l'ouverture
    /// (`None` : ligne ajoutée).
    original: Option<(Day, ExceptionFields)>,
}

impl ExceptionRow {
    /// Ligne neuve, vide, chômée.
    #[must_use]
    pub fn blank() -> Self {
        Self {
            fields: ExceptionFields::default(),
            recurring: false,
            original: None,
        }
    }
}

/// Brouillon d'un calendrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarDraft {
    /// Calendrier.
    pub uid: CalendarUid,
    /// Calendrier de base (sinon dérivé).
    pub base: bool,
    /// Nom saisi.
    pub name: String,
    /// Nom relu.
    original_name: String,
    /// Jours de la semaine type, dans l'ordre d'affichage (premier jour de
    /// la semaine du projet d'abord).
    pub week: Vec<DayDraft>,
    /// Exceptions, dans l'ordre de stockage, puis les lignes ajoutées.
    pub exceptions: Vec<ExceptionRow>,
    /// Premier jour des exceptions relues retirées.
    pub removed: Vec<Day>,
}

/// Refus d'un champ du brouillon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
    /// Calendrier.
    pub calendar: CalendarUid,
    /// Onglet.
    pub tab: WtTab,
    /// Champ (`"from"`, `"to"`, `"ranges"`, `"week"`).
    pub field: &'static str,
    /// Ligne (exception) ou rang du jour (semaine).
    pub row: usize,
    /// Message traduit.
    pub message: String,
}

/// Jours de la semaine dans l'ordre d'affichage : `start` d'abord.
#[must_use]
pub fn week_order(start: Weekday) -> [Weekday; 7] {
    let offset = weekday_index(start);
    std::array::from_fn(|i| {
        let k = i.checked_add(offset).and_then(|v| v.checked_rem(7)).unwrap_or(0);
        Weekday::ALL.get(k).copied().unwrap_or(Weekday::Monday)
    })
}

/// Rang d'un jour dans `Weekday::ALL` (dimanche = 0).
fn weekday_index(d: Weekday) -> usize {
    Weekday::ALL.iter().position(|w| *w == d).unwrap_or(0)
}

/// Jour JSON d'un jour du modèle.
const fn weekday_json(d: Weekday) -> WeekdayJson {
    match d {
        Weekday::Monday => WeekdayJson::Monday,
        Weekday::Tuesday => WeekdayJson::Tuesday,
        Weekday::Wednesday => WeekdayJson::Wednesday,
        Weekday::Thursday => WeekdayJson::Thursday,
        Weekday::Friday => WeekdayJson::Friday,
        Weekday::Saturday => WeekdayJson::Saturday,
        Weekday::Sunday => WeekdayJson::Sunday,
    }
}

/// Texte de plages (« 08:00-12:00, 13:00-17:00 »).
#[must_use]
pub fn ranges_text(ranges: &[TimeRange]) -> String {
    ranges
        .iter()
        .map(|r| format!("{}-{}", r.from(), r.to()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Heure `HH:MM` ou `H:MM`.
fn time_of_day(s: &str) -> Option<TimeOfDay> {
    let s = s.trim();
    if s.len() == 4 && s.as_bytes().get(1) == Some(&b':') {
        return format!("0{s}").parse().ok();
    }
    s.parse().ok()
}

/// Lit des plages « 08:00-12:00, 13:00-17:00 » (séparateurs `,` ou `;`) ;
/// `None` si une plage est illisible ou s'il n'y en a aucune. L'ordre, les
/// chevauchements et le nombre de plages sont contrôlés par le registre
/// (R-247, R-165).
#[must_use]
pub fn parse_ranges(text: &str) -> Option<Vec<RangeJson>> {
    let mut out = Vec::new();
    for part in text.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = part.split_once('-')?;
        out.push(RangeJson {
            from: time_of_day(a)?,
            to: time_of_day(b)?,
        });
    }
    (!out.is_empty()).then_some(out)
}

/// Date d'un jour (`JJ/MM/AAAA` ou `MM/JJ/AAAA`, sans heure), relisible
/// par `parse_date`.
#[must_use]
pub fn day_text(day: Day, ctx: &TextContext) -> String {
    let d = day.to_naive();
    let (a, b) = match ctx.date_order {
        DateOrder::DayMonthYear => (d.day(), d.month()),
        DateOrder::MonthDayYear => (d.month(), d.day()),
    };
    format!("{a:02}/{b:02}/{:04}", d.year())
}

/// Lit un jour (R-143 ; l'heure éventuelle est ignorée).
fn parse_day(text: &str, ctx: &TextContext) -> Result<Day, EditError> {
    parse_date(text, DateRole::Start, ctx)
        .map(plannercraft_core::Moment::day)
        .map_err(EditError::from)
}

/// Champs d'une exception relue.
fn exception_fields(
    e: &plannercraft_core::calendar::CalendarException,
    ctx: &TextContext,
) -> ExceptionFields {
    let (working, ranges) = match &e.setting {
        DaySetting::NonWorking => (false, String::new()),
        DaySetting::Working(h) => (true, ranges_text(h.ranges())),
    };
    ExceptionFields {
        name: e.name.clone(),
        from: day_text(e.period.first(), ctx),
        to: day_text(e.period.last(), ctx),
        working,
        ranges,
    }
}

/// Exception récurrente : son dépliage n'est pas sa période d'un seul
/// tenant (récurrence, nombre d'occurrences, ou récurrence invalide).
fn is_recurring(e: &plannercraft_core::calendar::CalendarException) -> bool {
    !expand_exception(e).is_ok_and(|runs| runs == [e.period])
}

/// Réglage et plages d'une règle de jour.
fn rule_choice(rule: &DayRule) -> (DayChoice, String) {
    match rule {
        DayRule::Inherit => (DayChoice::Inherit, String::new()),
        DayRule::NonWorking => (DayChoice::NonWorking, String::new()),
        DayRule::Working(h) => (DayChoice::Working, ranges_text(h.ranges())),
    }
}

/// Semaine de la base d'un calendrier dérivé.
fn base_week(p: &Project, c: &Calendar) -> Option<plannercraft_core::Week<DaySetting>> {
    let CalendarKind::Derived { base, .. } = c.kind() else {
        return None;
    };
    match p.calendars.get(*base)?.kind() {
        CalendarKind::Base { week } => Some(week.clone()),
        CalendarKind::Derived { .. } => None,
    }
}

impl CalendarDraft {
    /// Brouillon du calendrier `uid` relu sur `p` (`None` : absent).
    #[must_use]
    pub fn from_project(p: &Project, uid: CalendarUid, ctx: &TextContext) -> Option<Self> {
        let c = p.calendars.get(uid)?;
        let base_week = base_week(p, c);
        let week = week_order(p.options.week_start_day)
            .into_iter()
            .map(|day| {
                let (choice, ranges) = match c.kind() {
                    CalendarKind::Base { week } => rule_choice(&DayRule::from(week.get(day).clone())),
                    CalendarKind::Derived { week, .. } => rule_choice(week.get(day)),
                };
                let inherited = base_week.as_ref().and_then(|w| match w.get(day) {
                    DaySetting::NonWorking => None,
                    DaySetting::Working(h) => Some(ranges_text(h.ranges())),
                });
                DayDraft {
                    day,
                    choice,
                    original_choice: choice,
                    original_ranges: ranges.clone(),
                    ranges,
                    inherited,
                }
            })
            .collect();
        let exceptions = c
            .exceptions()
            .iter()
            .map(|e| {
                let fields = exception_fields(e, ctx);
                ExceptionRow {
                    original: Some((e.period.first(), fields.clone())),
                    recurring: is_recurring(e),
                    fields,
                }
            })
            .collect();
        Some(Self {
            uid,
            base: c.is_base(),
            name: c.name.clone(),
            original_name: c.name.clone(),
            week,
            exceptions,
            removed: Vec::new(),
        })
    }

    /// Ajoute une ligne d'exception vide ; rend son rang.
    pub fn add_row(&mut self) -> usize {
        self.exceptions.push(ExceptionRow::blank());
        self.exceptions.len().saturating_sub(1)
    }

    /// Retire la ligne `i` (une exception relue est retirée à « OK » ; une
    /// récurrente n'est jamais retirée).
    pub fn remove_row(&mut self, i: usize) {
        if self.exceptions.get(i).is_none_or(|row| row.recurring) {
            return;
        }
        let row = self.exceptions.remove(i);
        if let Some((from, _)) = row.original {
            self.removed.push(from);
        }
    }

    /// Opérations du brouillon (vide : rien de changé) ; refus des champs
    /// illisibles ajoutés à `errors`.
    fn ops(&self, ctx: &TextContext, texts: &Texts, errors: &mut Vec<FieldError>) -> Vec<Command> {
        let mut ops = Vec::new();
        let cal = CalendarRef::Uid(self.uid);
        if self.name != self.original_name {
            ops.push(Command::CalendarUpdate(CalendarUpdate {
                uid: self.uid.get(),
                name: Patch::Set(self.name.clone()),
                base: Patch::Keep,
            }));
        }
        let mut days = Vec::new();
        for (i, d) in self.week.iter().enumerate() {
            let same_ranges = d.choice != DayChoice::Working || d.ranges.trim() == d.original_ranges.trim();
            if d.choice == d.original_choice && same_ranges {
                continue;
            }
            let setting = match d.choice {
                DayChoice::Inherit => WeekSettingJson::Inherit,
                DayChoice::NonWorking => WeekSettingJson::NonWorking,
                DayChoice::Working => {
                    if let Some(r) = parse_ranges(&d.ranges) {
                        WeekSettingJson::Ranges(r)
                    } else {
                        errors.push(self.error(WtTab::Week, "week", i, texts.get(TextId::WtBadRanges)));
                        continue;
                    }
                }
            };
            days.push(DaySettingJson {
                day: weekday_json(d.day),
                setting,
            });
        }
        if !days.is_empty() {
            ops.push(Command::CalendarSetWeek(CalendarSetWeek { calendar: cal, days }));
        }
        let mut removals: Vec<Day> = self.removed.clone();
        let mut sets = Vec::new();
        for (i, row) in self.exceptions.iter().enumerate() {
            if row.recurring {
                continue;
            }
            let original = row.original.as_ref();
            if original.is_some_and(|(_, f)| *f == row.fields) {
                continue;
            }
            let Some((from, to, ranges)) = self.parse_row(i, &row.fields, ctx, texts, errors) else {
                continue;
            };
            if let Some((stored, f)) = original
                && (f.from.trim() != row.fields.from.trim() || f.to.trim() != row.fields.to.trim())
            {
                removals.push(*stored);
            }
            sets.push(Command::CalendarSetException(ExceptionOp {
                cal,
                from,
                to,
                ranges,
                name: Some(row.fields.name.clone()),
            }));
        }
        ops.extend(
            removals
                .into_iter()
                .map(|from| Command::CalendarRemoveException(CalendarExceptionKey { cal, from })),
        );
        ops.extend(sets);
        ops
    }

    /// Période et plages lues d'une ligne ; refus ajoutés à `errors`.
    fn parse_row(
        &self,
        i: usize,
        f: &ExceptionFields,
        ctx: &TextContext,
        texts: &Texts,
        errors: &mut Vec<FieldError>,
    ) -> Option<(Day, Day, Vec<[TimeOfDay; 2]>)> {
        let from = parse_day(&f.from, ctx)
            .map_err(|e| errors.push(self.error(WtTab::Exceptions, "from", i, e.message(texts))))
            .ok();
        // Fin vide : un seul jour.
        let to = if f.to.trim().is_empty() {
            from
        } else {
            parse_day(&f.to, ctx)
                .map_err(|e| errors.push(self.error(WtTab::Exceptions, "to", i, e.message(texts))))
                .ok()
        };
        let ranges = if f.working {
            let parsed = parse_ranges(&f.ranges).map(|r| r.into_iter().map(|r| [r.from, r.to]).collect());
            if parsed.is_none() {
                errors.push(self.error(WtTab::Exceptions, "ranges", i, texts.get(TextId::WtBadRanges)));
            }
            parsed
        } else {
            Some(Vec::new())
        };
        Some((from?, to?, ranges?))
    }

    /// Refus d'un champ de ce calendrier.
    const fn error(&self, tab: WtTab, field: &'static str, row: usize, message: String) -> FieldError {
        FieldError {
            calendar: self.uid,
            tab,
            field,
            row,
            message,
        }
    }
}

/// Calendriers du projet dans l'ordre de la liste.
#[must_use]
pub fn calendar_entries(p: &Project) -> Vec<CalendarEntry> {
    let entry = |c: &Calendar| CalendarEntry {
        uid: c.uid(),
        name: c.name.clone(),
        project: c.uid() == p.calendar,
        base: match c.kind() {
            CalendarKind::Base { .. } => None,
            CalendarKind::Derived { base, .. } => Some(*base),
        },
        resource: p.resources.iter().any(|r| r.calendar == Some(c.uid())),
    };
    let mut out: Vec<CalendarEntry> = p.calendars.get(p.calendar).map(entry).into_iter().collect();
    out.extend(
        p.calendars
            .iter()
            .filter(|c| c.uid() != p.calendar && c.is_base())
            .map(entry),
    );
    out.extend(
        p.calendars
            .iter()
            .filter(|c| c.uid() != p.calendar && !c.is_base())
            .map(entry),
    );
    out
}

/// Marque d'un jour du mois.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayMark {
    /// Ouvré (semaine type).
    Working,
    /// Chômé (semaine type).
    NonWorking,
    /// Exception ouvrée.
    ExceptionWorking,
    /// Exception chômée.
    ExceptionNonWorking,
}

/// Jour du mois affiché.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonthDay {
    /// Jour.
    pub day: Day,
    /// Numéro dans le mois (1 à 31).
    pub number: u32,
    /// Marque.
    pub mark: DayMark,
    /// Plages ouvrées lues sur le moteur (vide : chômé).
    pub ranges: String,
}

/// Mois affiché d'un calendrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonthView {
    /// Année.
    pub year: i32,
    /// Mois (1 à 12).
    pub month: u32,
    /// Cases vides avant le premier jour (premier jour de la semaine du
    /// projet en première colonne).
    pub lead: usize,
    /// Jours du mois.
    pub days: Vec<MonthDay>,
}

/// Mois `year`/`month` du calendrier `uid` : jours ouvrés ou chômés lus sur
/// le calendrier compilé du moteur, exceptions (du calendrier ou de sa
/// base) marquées.
///
/// # Errors
///
/// Message du moteur si le calendrier n'est pas compilé (absent, hors
/// budget, incohérent) ; mois hors du domaine.
pub fn month_view(
    p: &Project,
    set: &CalendarSet,
    uid: CalendarUid,
    year: i32,
    month: u32,
) -> Result<MonthView, String> {
    let compiled = set.get(uid).map_err(|e| e.to_string())?;
    let first = NaiveDate::from_ymd_opt(year, month, 1)
        .and_then(|d| Day::from_naive(d).ok())
        .ok_or_else(|| format!("{month:02}/{year}"))?;
    let mut runs: Vec<DayRange> = Vec::new();
    if let Some(c) = p.calendars.get(uid) {
        let base = match c.kind() {
            CalendarKind::Derived { base, .. } => p.calendars.get(*base),
            CalendarKind::Base { .. } => None,
        };
        for e in c
            .exceptions()
            .iter()
            .chain(base.into_iter().flat_map(Calendar::exceptions))
        {
            runs.extend(expand_exception(e).unwrap_or_default());
        }
    }
    let lead = weekday_index(first.weekday())
        .checked_add(7)
        .and_then(|v| v.checked_sub(weekday_index(p.options.week_start_day)))
        .and_then(|v| v.checked_rem(7))
        .unwrap_or(0);
    let mut days = Vec::new();
    for n in 0..31 {
        let Ok(day) = first.checked_add_days(n) else {
            break;
        };
        let date = day.to_naive();
        if date.month() != month {
            break;
        }
        let ranges = compiled.day_ranges(day).map_err(|e| e.to_string())?;
        let working = !ranges.is_empty();
        let exception = runs.iter().any(|r| r.contains(day));
        let mark = match (exception, working) {
            (false, true) => DayMark::Working,
            (false, false) => DayMark::NonWorking,
            (true, true) => DayMark::ExceptionWorking,
            (true, false) => DayMark::ExceptionNonWorking,
        };
        days.push(MonthDay {
            day,
            number: date.day(),
            mark,
            ranges: ranges_text(ranges),
        });
    }
    Ok(MonthView {
        year,
        month,
        lead,
        days,
    })
}

/// Mois décalé de `delta` mois, borné au domaine (1984 à 2149).
#[must_use]
pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let index = year
        .checked_mul(12)
        .and_then(|v| v.checked_add(i32::try_from(month).unwrap_or(1)))
        .and_then(|v| v.checked_sub(1))
        .and_then(|v| v.checked_add(delta))
        .unwrap_or(1984 * 12)
        .clamp(1984 * 12, 2149 * 12 + 11);
    let y = index.checked_div(12).unwrap_or(1984);
    let m = index.checked_rem(12).and_then(|v| v.checked_add(1)).unwrap_or(1);
    (y, u32::try_from(m).unwrap_or(1))
}

/// Mois lu sur le moteur, avec ce qu'il reflète.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonthCache {
    /// Clé du document.
    pub key: (u64, u64),
    /// Calendrier.
    pub calendar: CalendarUid,
    /// Année.
    pub year: i32,
    /// Mois (1 à 12).
    pub month: u32,
    /// Mois, ou message du moteur.
    pub view: Result<MonthView, String>,
}

impl MonthCache {
    /// Le cache reflète ce calendrier et ce mois (clé quelconque si `None`).
    #[must_use]
    pub fn shows(&self, key: Option<(u64, u64)>, calendar: CalendarUid, year: i32, month: u32) -> bool {
        key.is_none_or(|k| k == self.key)
            && self.calendar == calendar
            && self.year == year
            && self.month == month
    }
}

/// Formulaire « Nouveau calendrier ».
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewCalendarForm {
    /// Nom.
    pub name: String,
    /// Copie du calendrier sélectionné (sinon nouveau calendrier de base,
    /// semaine du Standard intégré).
    pub copy: bool,
}

/// Réponse de la fenêtre pour une image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WtOutcome {
    /// Rien : la fenêtre reste ouverte.
    Open,
    /// « OK ».
    Ok,
    /// « Annuler » ou Échap.
    Cancel,
    /// « Créer » du formulaire « Nouveau » (lot immédiat).
    Create,
    /// « Supprimer » (lot immédiat).
    Delete,
}

/// État de la fenêtre ouverte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkingTimeState {
    /// Clé du document à l'ouverture (ou après le dernier lot immédiat).
    pub key: (u64, u64),
    /// Calendriers de la liste.
    pub calendars: Vec<CalendarEntry>,
    /// Calendrier affiché.
    pub selected: CalendarUid,
    /// Brouillons des calendriers déjà affichés.
    pub drafts: BTreeMap<CalendarUid, CalendarDraft>,
    /// Année du mois affiché.
    pub year: i32,
    /// Mois affiché (1 à 12).
    pub month: u32,
    /// Mois lu sur le moteur (clé du document, calendrier, année, mois),
    /// rempli par l'hôte avant le dessin.
    pub month_view: Option<MonthCache>,
    /// Onglet.
    pub tab: WtTab,
    /// Formulaire « Nouveau », ouvert.
    pub new_form: Option<NewCalendarForm>,
    /// Refus des champs.
    pub errors: Vec<FieldError>,
    /// Refus du registre.
    pub error: Option<String>,
    /// Le document avait changé : brouillons relus, rien appliqué.
    pub stale: bool,
    /// Premier jour de la semaine du projet.
    pub week_start: Weekday,
}

impl WorkingTimeState {
    /// Fenêtre sur le calendrier `selected` (défaut : celui du projet) ; le
    /// mois affiché est celui de la date d'état effective.
    #[must_use]
    pub fn open(s: &Session, key: (u64, u64), lang: Language, selected: Option<CalendarUid>) -> Self {
        let p = s.project();
        let calendars = calendar_entries(p);
        let selected = selected
            .filter(|uid| calendars.iter().any(|c| c.uid == *uid))
            .unwrap_or(p.calendar);
        let date = p.effective_status_date().day().to_naive();
        let mut state = Self {
            key,
            calendars,
            selected,
            drafts: BTreeMap::new(),
            year: date.year(),
            month: date.month(),
            month_view: None,
            tab: WtTab::Exceptions,
            new_form: None,
            errors: Vec::new(),
            error: None,
            stale: false,
            week_start: p.options.week_start_day,
        };
        state.select(s, selected, lang);
        state
    }

    /// Affiche le calendrier `uid` (brouillon créé à la première fois).
    pub fn select(&mut self, s: &Session, uid: CalendarUid, lang: Language) {
        let ctx = TextContext::for_project(s.project(), lang);
        if !self.drafts.contains_key(&uid)
            && let Some(d) = CalendarDraft::from_project(s.project(), uid, &ctx)
        {
            self.drafts.insert(uid, d);
        }
        self.selected = uid;
    }

    /// Brouillon du calendrier affiché.
    #[must_use]
    pub fn draft(&self) -> Option<&CalendarDraft> {
        self.drafts.get(&self.selected)
    }

    /// Brouillon du calendrier affiché, modifiable.
    pub fn draft_mut(&mut self) -> Option<&mut CalendarDraft> {
        self.drafts.get_mut(&self.selected)
    }

    /// Lot de « OK » : un seul lot pour tous les brouillons modifiés ;
    /// `Ok(None)` si rien n'a changé.
    ///
    /// # Errors
    ///
    /// Refus des champs illisibles (tous).
    pub fn to_batch(
        &self,
        s: &Session,
        texts: &Texts,
        lang: Language,
    ) -> Result<Option<Batch>, Vec<FieldError>> {
        let ctx = TextContext::for_project(s.project(), lang);
        let mut errors = Vec::new();
        let ops: Vec<Command> = self
            .drafts
            .values()
            .flat_map(|d| d.ops(&ctx, texts, &mut errors))
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok((!ops.is_empty()).then(|| batch_of(ops, lang)))
    }

    /// Lot immédiat de « Créer » : `calendar.add` (copie du calendrier
    /// affiché si demandé) ; `None` sans formulaire ouvert.
    #[must_use]
    pub fn create_batch(&self, lang: Language) -> Option<Batch> {
        let form = self.new_form.as_ref()?;
        Some(batch_of(
            vec![Command::CalendarAdd(CalendarAdd {
                name: form.name.trim().to_owned(),
                base: None,
                copy_of: form.copy.then(|| self.selected.get()),
            })],
            lang,
        ))
    }

    /// Lot immédiat de « Supprimer » : `calendar.delete` du calendrier
    /// affiché.
    #[must_use]
    pub fn delete_batch(&self, lang: Language) -> Batch {
        batch_of(
            vec![Command::CalendarDelete(CalendarDelete {
                uid: self.selected.get(),
            })],
            lang,
        )
    }

    /// Après un lot immédiat appliqué : liste relue, brouillons des
    /// calendriers disparus retirés, nouvelle clé ; `select` affiché s'il
    /// existe (sinon le calendrier affiché, sinon celui du projet).
    pub fn after_own_change(
        &mut self,
        s: &Session,
        key: (u64, u64),
        lang: Language,
        select: Option<CalendarUid>,
    ) {
        let p = s.project();
        self.calendars = calendar_entries(p);
        self.drafts.retain(|uid, _| p.calendars.contains(*uid));
        self.key = key;
        self.errors.clear();
        self.error = None;
        let exists = |uid: &CalendarUid| p.calendars.contains(*uid);
        let target = select
            .filter(exists)
            .or_else(|| Some(self.selected).filter(exists))
            .unwrap_or(p.calendar);
        self.select(s, target, lang);
    }

    /// Refus du champ `field`, ligne `row`, du calendrier affiché.
    fn error_for(&self, tab: WtTab, field: &str, row: usize) -> Option<&str> {
        self.errors
            .iter()
            .find(|e| e.calendar == self.selected && e.tab == tab && e.field == field && e.row == row)
            .map(|e| e.message.as_str())
    }

    /// Libellé d'un calendrier de la liste.
    fn entry_label(texts: &Texts, entry: &CalendarEntry) -> String {
        let name = Arg::Str(entry.name.clone().into());
        if entry.project {
            texts.get_args(TextId::WtProjectCalendar, &[("name", name)])
        } else if entry.resource {
            texts.get_args(TextId::WtResourceCalendar, &[("name", name)])
        } else {
            entry.name.clone()
        }
    }
}

// --- Fenêtre (egui) -------------------------------------------------------

/// Identifiant de la fenêtre (couche modale d'egui).
#[must_use]
pub fn modal_id() -> egui::Id {
    egui::Id::unique("plannercraft.working-time")
}

/// Dessine la fenêtre (une image) ; rend la réponse de l'utilisateur.
/// Le mois (`state.month_view`) est rempli par l'hôte avant l'appel.
pub fn show(
    ctx: &egui::Context,
    texts: &Texts,
    s: &Session,
    lang: Language,
    state: &mut WorkingTimeState,
) -> WtOutcome {
    let mut outcome = WtOutcome::Open;
    let armed = super::enter_armed(ctx, None);
    let response = egui::Modal::new(modal_id()).show(ctx, |ui| {
        ui.set_width(660.0);
        ui.heading(texts.get(TextId::WtTitle));
        if state.stale {
            ui.colored_label(ui.visuals().warn_fg_color, texts.get(TextId::TaskInfoStale));
        }
        calendar_row(ui, texts, s, lang, state, &mut outcome);
        if state.new_form.is_some() {
            new_form(ui, texts, state, &mut outcome);
        }
        ui.separator();
        month_grid(ui, texts, state);
        ui.separator();
        ui.horizontal(|ui| {
            for (tab, title) in [
                (WtTab::Exceptions, TextId::WtTabExceptions),
                (WtTab::Week, TextId::WtTabWeek),
            ] {
                let failed = state
                    .errors
                    .iter()
                    .any(|e| e.calendar == state.selected && e.tab == tab);
                let mut text = egui::RichText::new(texts.get(title));
                if failed {
                    text = text.color(ui.visuals().error_fg_color);
                }
                if ui.selectable_label(state.tab == tab, text).clicked() {
                    state.tab = tab;
                }
            }
        });
        // Hauteur fixe (`min_scrolled_height` aussi : sans lui, la zone
        // grandit sur plusieurs images et la fenêtre, recentrée, glisse) ;
        // changer d'onglet ne redimensionne pas la fenêtre.
        egui::ScrollArea::vertical()
            .min_scrolled_height(220.0)
            .max_height(220.0)
            .auto_shrink([false, false])
            .show(ui, |ui| match state.tab {
                WtTab::Exceptions => exceptions_tab(ui, texts, state),
                WtTab::Week => week_tab(ui, texts, state),
            });
        // Refus sur un autre calendrier que celui affiché, puis du registre.
        for e in state.errors.iter().filter(|e| e.calendar != state.selected) {
            ui.colored_label(ui.visuals().error_fg_color, e.message.as_str());
        }
        error_line(ui, state.error.as_deref());
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(texts.get(TextId::TaskInfoOk)).clicked() {
                outcome = WtOutcome::Ok;
            }
            if ui.button(texts.get(TextId::TaskInfoCancel)).clicked() {
                outcome = WtOutcome::Cancel;
            }
        });
    });
    let escape = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        outcome = WtOutcome::Cancel;
    }
    if outcome == WtOutcome::Open
        && state.new_form.is_none()
        && super::enter_pressed(ctx, armed, response.is_top_modal, response.any_popup_open)
    {
        outcome = WtOutcome::Ok;
    }
    outcome
}

/// Liste des calendriers, Nouveau, Supprimer, nom et base.
fn calendar_row(
    ui: &mut egui::Ui,
    texts: &Texts,
    s: &Session,
    lang: Language,
    state: &mut WorkingTimeState,
    outcome: &mut WtOutcome,
) {
    let label = texts.get(TextId::WtCalendar);
    let mut chosen = state.selected;
    ui.horizontal(|ui| {
        ui.label(&label);
        let selected_text = state
            .calendars
            .iter()
            .find(|c| c.uid == state.selected)
            .map(|c| WorkingTimeState::entry_label(texts, c))
            .unwrap_or_default();
        let response = egui::ComboBox::from_id_salt("plannercraft.working-time.calendar")
            .selected_text(selected_text)
            .width(280.0)
            .show_ui(ui, |ui| {
                for entry in &state.calendars {
                    ui.selectable_value(
                        &mut chosen,
                        entry.uid,
                        WorkingTimeState::entry_label(texts, entry),
                    );
                }
            })
            .response;
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
        if ui.button(texts.get(TextId::WtNew)).clicked() {
            state.new_form = Some(NewCalendarForm::default());
        }
        if ui.button(texts.get(TextId::WtDelete)).clicked() {
            *outcome = WtOutcome::Delete;
        }
    });
    if chosen != state.selected {
        state.select(s, chosen, lang);
    }
    let base_name = state
        .calendars
        .iter()
        .find(|c| c.uid == state.selected)
        .and_then(|c| c.base)
        .and_then(|b| state.calendars.iter().find(|c| c.uid == b))
        .map(|c| c.name.clone());
    if let Some(draft) = state.drafts.get_mut(&state.selected) {
        ui.horizontal(|ui| {
            let name = texts.get(TextId::WtName);
            ui.label(&name);
            text_field(ui, &name, &mut draft.name, true, 260.0);
            if let Some(base) = base_name {
                ui.label(texts.get_args(TextId::WtBaseOf, &[("name", Arg::Str(base.into()))]));
            }
        });
    }
}

/// Formulaire « Nouveau calendrier ».
fn new_form(ui: &mut egui::Ui, texts: &Texts, state: &mut WorkingTimeState, outcome: &mut WtOutcome) {
    let current = state
        .calendars
        .iter()
        .find(|c| c.uid == state.selected)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let mut close = false;
    if let Some(form) = &mut state.new_form {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                let name = texts.get(TextId::WtNewName);
                ui.label(&name);
                text_field(ui, &name, &mut form.name, true, 220.0);
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut form.copy, false, texts.get(TextId::WtNewBase));
                ui.radio_value(
                    &mut form.copy,
                    true,
                    texts.get_args(TextId::WtNewCopy, &[("name", Arg::Str(current.into()))]),
                );
            });
            ui.horizontal(|ui| {
                if ui.button(texts.get(TextId::WtCreate)).clicked() {
                    *outcome = WtOutcome::Create;
                }
                if ui.button(texts.get(TextId::WtNewCancel)).clicked() {
                    close = true;
                }
            });
        });
    }
    if close {
        state.new_form = None;
    }
}

/// Mois affiché : en-têtes des jours, cases colorées (lecture du moteur).
fn month_grid(ui: &mut egui::Ui, texts: &Texts, state: &mut WorkingTimeState) {
    let labels = texts.format_labels();
    ui.horizontal(|ui| {
        if ui.button(texts.get(TextId::WtPrevMonth)).clicked() {
            (state.year, state.month) = shift_month(state.year, state.month, -1);
        }
        let month_name = usize::try_from(state.month)
            .ok()
            .and_then(|m| m.checked_sub(1))
            .and_then(|m| labels.months_long.get(m))
            .cloned()
            .unwrap_or_default();
        ui.strong(texts.get_args(
            TextId::WtMonthTitle,
            &[
                ("name", Arg::Str(month_name.into())),
                ("value", Arg::Str(state.year.to_string().into())),
            ],
        ));
        if ui.button(texts.get(TextId::WtNextMonth)).clicked() {
            (state.year, state.month) = shift_month(state.year, state.month, 1);
        }
    });
    let view = match state.month_view.as_ref().map(|c| &c.view) {
        Some(Ok(view)) => view,
        Some(Err(detail)) => {
            error_line(
                ui,
                Some(&texts.get_args(
                    TextId::WtMonthUnavailable,
                    &[("detail", Arg::Str(detail.clone().into()))],
                )),
            );
            return;
        }
        None => return,
    };
    let visuals = ui.visuals().clone();
    let fill = |mark: DayMark| match mark {
        DayMark::Working => visuals.extreme_bg_color,
        DayMark::NonWorking => visuals.widgets.noninteractive.bg_fill,
        DayMark::ExceptionWorking | DayMark::ExceptionNonWorking => visuals.selection.bg_fill,
    };
    egui::Grid::new("plannercraft.working-time.month")
        .spacing(egui::vec2(2.0, 2.0))
        .show(ui, |ui| {
            for day in week_order(state.week_start) {
                ui.label(
                    labels
                        .weekdays_short
                        .get(weekday_index(day))
                        .cloned()
                        .unwrap_or_default(),
                );
            }
            ui.end_row();
            let mut column = 0_usize;
            for _ in 0..view.lead {
                ui.label("");
                column = column.saturating_add(1);
            }
            // Six semaines toujours : la hauteur du mois ne change pas.
            let padding = 42_usize.saturating_sub(view.lead.saturating_add(view.days.len()));
            let cells = view
                .days
                .iter()
                .map(Some)
                .chain(std::iter::repeat_n(None, padding));
            for d in cells {
                if column == 7 {
                    ui.end_row();
                    column = 0;
                }
                day_cell(ui, texts, d, &fill);
                column = column.saturating_add(1);
            }
            ui.end_row();
        });
    legend(ui, texts, fill);
}

/// Case d'un jour du mois (`None` : case vide de bourrage).
fn day_cell(
    ui: &mut egui::Ui,
    texts: &Texts,
    d: Option<&MonthDay>,
    fill: &impl Fn(DayMark) -> egui::Color32,
) {
    let mut frame = egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 2));
    if let Some(d) = d {
        frame = frame.fill(fill(d.mark));
    }
    frame.show(ui, |ui| {
        ui.set_min_width(28.0);
        let Some(d) = d else {
            ui.label(egui::RichText::new("  ").monospace());
            return;
        };
        let text = egui::RichText::new(d.number.to_string()).monospace();
        let text = if matches!(d.mark, DayMark::NonWorking | DayMark::ExceptionNonWorking) {
            text.weak()
        } else {
            text
        };
        let state_text = if d.ranges.is_empty() {
            texts.get(TextId::WtStateNonWorking)
        } else {
            texts.get_args(
                TextId::WtStateWorking,
                &[("value", Arg::Str(d.ranges.clone().into()))],
            )
        };
        ui.label(text).on_hover_text(state_text);
    });
}

/// Légende des couleurs du mois.
fn legend(ui: &mut egui::Ui, texts: &Texts, fill: impl Fn(DayMark) -> egui::Color32) {
    ui.horizontal(|ui| {
        for (mark, id) in [
            (DayMark::Working, TextId::WtLegendWorking),
            (DayMark::NonWorking, TextId::WtLegendNonWorking),
            (DayMark::ExceptionNonWorking, TextId::WtLegendException),
        ] {
            egui::Frame::new()
                .fill(fill(mark))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(texts.get(id));
                });
        }
    });
}

/// Libellé d'accessibilité « champ, ligne n ».
fn row_label(texts: &Texts, field: TextId, row: usize) -> String {
    texts.get_args(
        TextId::TaskInfoRowField,
        &[
            ("column", Arg::Str(texts.get(field).into())),
            (
                "count",
                Arg::Count(i64::try_from(row.saturating_add(1)).unwrap_or(i64::MAX)),
            ),
        ],
    )
}

/// Onglet Exceptions.
fn exceptions_tab(ui: &mut egui::Ui, texts: &Texts, state: &mut WorkingTimeState) {
    let errors: Vec<(usize, &'static str, String)> = state
        .errors
        .iter()
        .filter(|e| e.calendar == state.selected && e.tab == WtTab::Exceptions)
        .map(|e| (e.row, e.field, e.message.clone()))
        .collect();
    let Some(draft) = state.drafts.get_mut(&state.selected) else {
        return;
    };
    let mut remove = None;
    egui::Grid::new("plannercraft.working-time.exceptions")
        .num_columns(6)
        .striped(true)
        .show(ui, |ui| {
            for id in [
                TextId::WtExcName,
                TextId::WtExcFrom,
                TextId::WtExcTo,
                TextId::WtExcWorking,
                TextId::WtExcRanges,
            ] {
                ui.strong(texts.get(id));
            }
            ui.label("");
            ui.end_row();
            for (i, row) in draft.exceptions.iter_mut().enumerate() {
                if row.recurring {
                    ui.label(&row.fields.name);
                    ui.label(&row.fields.from);
                    ui.label(&row.fields.to);
                    ui.label(if row.fields.working {
                        texts.get(TextId::WtLegendWorking)
                    } else {
                        texts.get(TextId::WtLegendNonWorking)
                    });
                    ui.label(&row.fields.ranges);
                    ui.weak(texts.get(TextId::WtExcRecurring));
                    ui.end_row();
                    continue;
                }
                text_field(
                    ui,
                    &row_label(texts, TextId::WtExcName, i),
                    &mut row.fields.name,
                    true,
                    150.0,
                );
                text_field(
                    ui,
                    &row_label(texts, TextId::WtExcFrom, i),
                    &mut row.fields.from,
                    true,
                    90.0,
                );
                text_field(
                    ui,
                    &row_label(texts, TextId::WtExcTo, i),
                    &mut row.fields.to,
                    true,
                    90.0,
                );
                let working = row_label(texts, TextId::WtExcWorking, i);
                let check = ui.add(egui::Checkbox::without_text(&mut row.fields.working));
                check.widget_info(|| egui::WidgetInfo::labeled(egui::Role::CheckBox, true, working.clone()));
                let enabled = row.fields.working;
                text_field(
                    ui,
                    &row_label(texts, TextId::WtExcRanges, i),
                    &mut row.fields.ranges,
                    enabled,
                    150.0,
                );
                if ui.button(texts.get(TextId::TaskInfoRemoveRow)).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
                for (_, _, message) in errors.iter().filter(|(r, _, _)| *r == i) {
                    ui.label("");
                    ui.colored_label(ui.visuals().error_fg_color, message.as_str());
                    ui.end_row();
                }
            }
        });
    if let Some(i) = remove {
        draft.remove_row(i);
    }
    if ui.button(texts.get(TextId::WtExcAdd)).clicked() {
        draft.add_row();
    }
}

/// Onglet Semaine type.
fn week_tab(ui: &mut egui::Ui, texts: &Texts, state: &mut WorkingTimeState) {
    let labels = texts.format_labels().weekdays_short.clone();
    let errors: Vec<(usize, String)> = (0..7)
        .filter_map(|i| state.error_for(WtTab::Week, "week", i).map(|m| (i, m.to_owned())))
        .collect();
    let Some(draft) = state.drafts.get_mut(&state.selected) else {
        return;
    };
    let choices = day_choices(draft.base);
    egui::Grid::new("plannercraft.working-time.week")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for (i, d) in draft.week.iter_mut().enumerate() {
                let name = labels.get(weekday_index(d.day)).cloned().unwrap_or_default();
                ui.label(&name);
                let setting_label =
                    texts.get_args(TextId::WtWeekSetting, &[("name", Arg::Str(name.clone().into()))]);
                let selected = choices
                    .iter()
                    .find(|(c, _)| *c == d.choice)
                    .map(|(_, t)| texts.get(*t))
                    .unwrap_or_default();
                let response = egui::ComboBox::from_id_salt(("plannercraft.working-time.day", i))
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (c, t) in choices {
                            ui.selectable_value(&mut d.choice, *c, texts.get(*t));
                        }
                    })
                    .response;
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::ComboBox, true, setting_label.clone())
                });
                let ranges_label = texts.get_args(TextId::WtWeekRanges, &[("name", Arg::Str(name.into()))]);
                match d.choice {
                    DayChoice::Working => {
                        if d.ranges.is_empty() {
                            d.ranges = d
                                .inherited
                                .clone()
                                .unwrap_or_else(|| "08:00-12:00, 13:00-17:00".to_owned());
                        }
                        text_field(ui, &ranges_label, &mut d.ranges, true, 220.0);
                    }
                    DayChoice::Inherit => {
                        ui.weak(match &d.inherited {
                            Some(r) => texts
                                .get_args(TextId::WtWeekInherited, &[("value", Arg::Str(r.clone().into()))]),
                            None => texts.get(TextId::WtStateNonWorking),
                        });
                    }
                    DayChoice::NonWorking => {
                        ui.label("");
                    }
                }
                ui.end_row();
                if let Some((_, message)) = errors.iter().find(|(r, _)| *r == i) {
                    ui.label("");
                    ui.colored_label(ui.visuals().error_fg_color, message.as_str());
                    ui.end_row();
                }
            }
        });
}

/// Calendrier et onglet d'un refus du registre sur le lot `batch`
/// (opération fautive `op_index`) ; `None` si l'opération n'est pas d'un
/// calendrier désigné par UID.
#[must_use]
pub fn locate(
    batch: &Batch,
    error: &plannercraft_automation::CommandError,
) -> Option<(CalendarUid, Option<WtTab>)> {
    let op = error.op_index.and_then(|i| batch.ops.get(i))?;
    let uid_of = |c: CalendarRef| match c {
        CalendarRef::Uid(u) => Some(u),
        CalendarRef::Project => None,
    };
    match op {
        Command::CalendarUpdate(u) => Some((CalendarUid::new(u.uid), None)),
        Command::CalendarSetWeek(w) => uid_of(w.calendar).map(|u| (u, Some(WtTab::Week))),
        Command::CalendarSetException(e) => uid_of(e.cal).map(|u| (u, Some(WtTab::Exceptions))),
        Command::CalendarRemoveException(k) => uid_of(k.cal).map(|u| (u, Some(WtTab::Exceptions))),
        _ => None,
    }
}
