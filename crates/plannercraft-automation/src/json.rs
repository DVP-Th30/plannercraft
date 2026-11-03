//! Formes JSON des valeurs du modèle dans les commandes et les vues
//! (contrat J4, ADR-0012 D6).
//!
//! Une seule forme de durée en entrée et en sortie :
//! `{"minutes":2400,"unit":"day","text":"5j"}` ; en entrée, `text` seul ou
//! `minutes` seul suffit, les deux doivent concorder (partie « lecture » de
//! R-152/R-158, lot L3). Une sortie ([`DurationJson::from_duration`]) est
//! une entrée valide qui redonne la même durée.
//!
//! Dates : ISO local sans fuseau (`"2026-11-02T08:00"`, R-075), jours
//! `"2026-11-11"`, heures `"08:00"` ; leurs schémas sont [`MomentSchema`],
//! [`DaySchema`] et [`TimeOfDaySchema`].

use std::borrow::Cow;

use plannercraft_core::resource::Granularity;
use plannercraft_core::{
    ConstraintKind, Decimal, Duration, DurationError, DurationUnit, Lag, LagValue, LinkType, Minutes,
    SignedPercent, TaskType, TimeBasis, TimeOfDay, Units,
};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::error::{CommandError, CommandErrorKind, codes};
use crate::text::lex::{Cursor, bounded_chars};
use crate::text::{TextContext, TextError, format_duration, format_lag, parse_duration, parse_lag};

/// Schéma JSON d'un instant (`plannercraft_core::Moment`) : ISO local à la minute,
/// sans fuseau (`"2026-11-02T08:00"`, `":00"` final toléré).
pub struct MomentSchema;

impl JsonSchema for MomentSchema {
    fn schema_name() -> Cow<'static, str> {
        "Moment".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}(:00)?$",
            "description": "Date et heure locales, sans fuseau : 2026-11-02T08:00."
        })
    }
}

/// Schéma JSON d'un jour (`plannercraft_core::Day`) : `"2026-11-11"`.
pub struct DaySchema;

impl JsonSchema for DaySchema {
    fn schema_name() -> Cow<'static, str> {
        "Day".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$",
            "description": "Jour : 2026-11-11."
        })
    }
}

/// Schéma JSON d'une heure du jour (`plannercraft_core::TimeOfDay`) : `"08:00"`,
/// de `"00:00"` à `"24:00"`.
pub struct TimeOfDaySchema;

impl JsonSchema for TimeOfDaySchema {
    fn schema_name() -> Cow<'static, str> {
        "TimeOfDay".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^[0-9]{2}:[0-9]{2}$",
            "description": "Heure : 08:00 (24:00 = fin du jour)."
        })
    }
}

/// Erreur `param.invalid` avec son explication.
fn invalid(why: &str) -> CommandError {
    CommandError::new(
        codes::PARAM_INVALID,
        CommandErrorKind::InvalidParam(why.to_owned()),
    )
}

/// Erreur `param.mismatch`.
const fn mismatch() -> CommandError {
    CommandError::new(codes::PARAM_MISMATCH, CommandErrorKind::ParamMismatch)
}

/// Erreur du modèle sur une durée ou un décalage → `param.invalid`.
fn from_duration_error(e: &DurationError) -> CommandError {
    match e {
        DurationError::Negative => invalid("durée négative"),
        _ => invalid("durée hors des bornes du modèle"),
    }
}

/// Base de temps d'un drapeau `elapsed`.
const fn basis_of(elapsed: bool) -> TimeBasis {
    if elapsed {
        TimeBasis::Elapsed
    } else {
        TimeBasis::Working
    }
}

/// Vrai si le champ facultatif `given` est absent ou égal à `actual`.
fn agrees<T: PartialEq + Copy>(given: Option<T>, actual: T) -> bool {
    given.is_none_or(|g| g == actual)
}

impl DurationJson {
    /// Forme de sortie complète d'une durée : minutes, unité, texte (R-141),
    /// `elapsed` et `estimated`. Relue par [`DurationJson::to_duration`],
    /// elle redonne exactement `d`.
    #[must_use]
    pub fn from_duration(d: &Duration, ctx: &TextContext) -> Self {
        Self {
            minutes: Some(d.minutes().get()),
            unit: Some(d.unit().into()),
            text: Some(format_duration(d, ctx)),
            elapsed: Some(d.basis() == TimeBasis::Elapsed),
            estimated: Some(d.is_estimated()),
        }
    }

    /// Durée du modèle (forme des durées, ADR-0012 D6) :
    /// - `text` seul : lu par [`parse_duration`] (R-140) dans `ctx` ;
    /// - `minutes` seul : `unit` (à défaut l'unité par défaut du projet),
    ///   `elapsed` et `estimated` (à défaut faux) ;
    /// - `text` et d'autres champs : chaque champ fourni doit concorder avec
    ///   le texte lu (minutes, unité, base, estimation).
    ///
    /// # Errors
    ///
    /// `text.*` si le texte est illisible ; `param.mismatch` si un champ
    /// contredit le texte ; `param.invalid` sans `text` ni `minutes`, ou
    /// pour des minutes négatives ou hors bornes (R-008). Le champ fautif
    /// est posé par l'appelant.
    pub fn to_duration(&self, ctx: &TextContext) -> Result<Duration, CommandError> {
        if let Some(text) = &self.text {
            let d = parse_duration(text, ctx).map_err(CommandError::text)?;
            let consistent = agrees(self.minutes, d.minutes().get())
                && agrees(self.unit, d.unit().into())
                && agrees(self.elapsed, d.basis() == TimeBasis::Elapsed)
                && agrees(self.estimated, d.is_estimated());
            return if consistent { Ok(d) } else { Err(mismatch()) };
        }
        let Some(minutes) = self.minutes else {
            return Err(invalid("« minutes » ou « text » attendu"));
        };
        let unit = self.unit.map_or(ctx.default_duration_unit, DurationUnit::from);
        Duration::new(
            Minutes::new(minutes),
            unit,
            basis_of(self.elapsed.unwrap_or(false)),
            self.estimated.unwrap_or(false),
        )
        .map_err(|e| from_duration_error(&e))
    }
}

impl LagJson {
    /// Forme de sortie complète d'un décalage : minutes et unité, ou
    /// pourcentage, plus le texte (R-141) et `elapsed`. Relue par
    /// [`LagJson::to_lag`], elle redonne exactement `l`.
    #[must_use]
    pub fn from_lag(l: &Lag, ctx: &TextContext) -> Self {
        let (minutes, percent, unit) = match l.value {
            LagValue::Time { minutes, unit } => (Some(minutes.get()), None, Some(unit.into())),
            LagValue::Percent { percent } => (None, Some(percent.get()), None),
        };
        Self {
            minutes,
            percent,
            unit,
            text: Some(format_lag(l, ctx)),
            elapsed: Some(l.basis == TimeBasis::Elapsed),
        }
    }

    /// Décalage du modèle (R-156, lecture) : `text` seul (« +2j », « 50% »,
    /// [`parse_lag`]) ; `minutes` (signées) et `unit` (à défaut l'unité par
    /// défaut du projet) ; ou `percent` en 1/100 de % (5 000 = 50 %). Avec
    /// `text`, chaque autre champ fourni doit concorder avec le texte lu.
    ///
    /// # Errors
    ///
    /// `text.*` si le texte est illisible ; `param.mismatch` si un champ
    /// contredit le texte, ou une unité accompagne un pourcentage ;
    /// `param.invalid` sans aucune valeur, avec `minutes` et `percent`
    /// ensemble, ou hors bornes (R-002, R-008).
    pub fn to_lag(&self, ctx: &TextContext) -> Result<Lag, CommandError> {
        if let Some(text) = &self.text {
            let l = parse_lag(text, ctx).map_err(CommandError::text)?;
            let value_ok = match l.value {
                LagValue::Time { minutes, unit } => {
                    self.percent.is_none()
                        && agrees(self.minutes, minutes.get())
                        && agrees(self.unit, unit.into())
                }
                LagValue::Percent { percent } => {
                    self.minutes.is_none() && self.unit.is_none() && agrees(self.percent, percent.get())
                }
            };
            let consistent = value_ok && agrees(self.elapsed, l.basis == TimeBasis::Elapsed);
            return if consistent { Ok(l) } else { Err(mismatch()) };
        }
        let basis = basis_of(self.elapsed.unwrap_or(false));
        let value = match (self.minutes, self.percent) {
            (Some(_), Some(_)) => return Err(invalid("« minutes » et « percent » exclusifs")),
            (None, None) => return Err(invalid("« minutes », « percent » ou « text » attendu")),
            (Some(m), None) => LagValue::Time {
                minutes: Minutes::new(m),
                unit: self.unit.map_or(ctx.default_duration_unit, DurationUnit::from),
            },
            (None, Some(p)) => {
                if self.unit.is_some() {
                    return Err(mismatch());
                }
                let percent =
                    SignedPercent::new(p).map_err(|_| invalid("pourcentage hors des bornes (±10 000 %)"))?;
                LagValue::Percent { percent }
            }
        };
        Lag::new(basis, value).map_err(|e| from_duration_error(&e))
    }
}

/// Durée en JSON.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DurationJson {
    /// Minutes (ouvrées, ou écoulées si `elapsed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes: Option<i64>,
    /// Unité d'affichage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<DurationUnitJson>,
    /// Texte à la manière de MS Project (« 5j », « 2d? »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Durée écoulée (« 5ej »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<bool>,
    /// Durée estimée (« 5j? »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated: Option<bool>,
}

/// Décalage d'un lien en JSON : minutes et unité, ou pourcentage de la durée
/// du prédécesseur, ou texte (« +2j », « 50% »).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LagJson {
    /// Minutes (signées).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes: Option<i64>,
    /// Pourcentage en 1/100 de % (5 000 = 50 %).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<i64>,
    /// Unité d'affichage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<DurationUnitJson>,
    /// Texte (« +2j », « -1d », « 50% »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Décalage écoulé.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed: Option<bool>,
}

/// Unité d'une durée en JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DurationUnitJson {
    /// Minute.
    Minute,
    /// Heure.
    Hour,
    /// Jour.
    Day,
    /// Semaine.
    Week,
    /// Mois.
    Month,
}

impl From<DurationUnit> for DurationUnitJson {
    fn from(u: DurationUnit) -> Self {
        match u {
            DurationUnit::Minute => Self::Minute,
            DurationUnit::Hour => Self::Hour,
            DurationUnit::Day => Self::Day,
            DurationUnit::Week => Self::Week,
            DurationUnit::Month => Self::Month,
        }
    }
}

impl From<DurationUnitJson> for DurationUnit {
    fn from(u: DurationUnitJson) -> Self {
        match u {
            DurationUnitJson::Minute => Self::Minute,
            DurationUnitJson::Hour => Self::Hour,
            DurationUnitJson::Day => Self::Day,
            DurationUnitJson::Week => Self::Week,
            DurationUnitJson::Month => Self::Month,
        }
    }
}

/// Type de lien en JSON (codes anglais de MS Project).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkTypeJson {
    /// Fin à début.
    #[default]
    Fs,
    /// Début à début.
    Ss,
    /// Fin à fin.
    Ff,
    /// Début à fin.
    Sf,
}

impl From<LinkType> for LinkTypeJson {
    fn from(k: LinkType) -> Self {
        match k {
            LinkType::FinishToStart => Self::Fs,
            LinkType::StartToStart => Self::Ss,
            LinkType::FinishToFinish => Self::Ff,
            LinkType::StartToFinish => Self::Sf,
        }
    }
}

impl From<LinkTypeJson> for LinkType {
    fn from(k: LinkTypeJson) -> Self {
        match k {
            LinkTypeJson::Fs => Self::FinishToStart,
            LinkTypeJson::Ss => Self::StartToStart,
            LinkTypeJson::Ff => Self::FinishToFinish,
            LinkTypeJson::Sf => Self::StartToFinish,
        }
    }
}

/// Type de contrainte en JSON (sigles anglais de MS Project).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKindJson {
    /// Le plus tôt possible.
    Asap,
    /// Le plus tard possible.
    Alap,
    /// Doit commencer le.
    Mso,
    /// Doit finir le.
    Mfo,
    /// Ne pas commencer avant le.
    Snet,
    /// Ne pas commencer après le.
    Snlt,
    /// Ne pas finir avant le.
    Fnet,
    /// Ne pas finir après le.
    Fnlt,
}

impl From<ConstraintKind> for ConstraintKindJson {
    fn from(k: ConstraintKind) -> Self {
        match k {
            ConstraintKind::AsSoonAsPossible => Self::Asap,
            ConstraintKind::AsLateAsPossible => Self::Alap,
            ConstraintKind::MustStartOn => Self::Mso,
            ConstraintKind::MustFinishOn => Self::Mfo,
            ConstraintKind::StartNoEarlierThan => Self::Snet,
            ConstraintKind::StartNoLaterThan => Self::Snlt,
            ConstraintKind::FinishNoEarlierThan => Self::Fnet,
            ConstraintKind::FinishNoLaterThan => Self::Fnlt,
        }
    }
}

impl From<ConstraintKindJson> for ConstraintKind {
    fn from(k: ConstraintKindJson) -> Self {
        match k {
            ConstraintKindJson::Asap => Self::AsSoonAsPossible,
            ConstraintKindJson::Alap => Self::AsLateAsPossible,
            ConstraintKindJson::Mso => Self::MustStartOn,
            ConstraintKindJson::Mfo => Self::MustFinishOn,
            ConstraintKindJson::Snet => Self::StartNoEarlierThan,
            ConstraintKindJson::Snlt => Self::StartNoLaterThan,
            ConstraintKindJson::Fnet => Self::FinishNoEarlierThan,
            ConstraintKindJson::Fnlt => Self::FinishNoLaterThan,
        }
    }
}

/// Unités (affectation, capacité, quantité d'une ressource matérielle) en
/// JSON (contrat J5) : `value` en 1/10 000 (10 000 = 100 % = 1,0 en MSPDI),
/// `text` en pourcentage (`"50%"`). En entrée, l'un ou l'autre suffit ; les
/// deux doivent concorder (`param.mismatch`), comme [`DurationJson`]. La
/// lecture est écrite par le lot L7.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitsJson {
    /// Valeur en 1/10 000 (10 000 = 100 %).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub value: Option<i64>,
    /// Pourcentage en texte (`"50%"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Lecture du texte d'un [`UnitsJson`] : nombre (langue du contexte), puis
/// `%` si `percent`, rien d'autre ; espaces admis autour.
fn parse_units_text(text: &str, percent: bool, ctx: &TextContext) -> Result<Units, TextError> {
    let chars = bounded_chars(text)?;
    let mut cur = Cursor::new(&chars);
    cur.skip_spaces();
    if cur.at_end() {
        return Err(TextError::Empty);
    }
    let value: Decimal = cur.decimal_in(ctx.language)?;
    cur.skip_spaces();
    let unit: String = cur.rest().iter().collect();
    let unit = unit.trim_end();
    let read = match (percent, unit) {
        (true, "%") => Units::from_percent(value),
        (false, "") => Units::from_fraction(value),
        _ => {
            return Err(TextError::UnknownUnit {
                unit: unit.to_owned(),
            });
        }
    };
    read.map_err(|_| TextError::OutOfRange)
}

impl UnitsJson {
    /// Unités d'affectation ou capacité (R-195, R-196) : `value` en
    /// 1/10 000 (10 000 = 100 %) ou `text` en pourcentage (« 50% »,
    /// « 150 % », « 12,5% »), arrondi par R-001 ; les deux fournis doivent
    /// concorder (comme [`DurationJson::to_duration`]).
    ///
    /// # Errors
    ///
    /// `text.*` si le texte est illisible ; `param.mismatch` si `value`
    /// contredit le texte ; `param.invalid` sans valeur, ou hors de
    /// `0..=600 000 000 000` (R-002). Le champ fautif est posé par
    /// l'appelant.
    pub fn to_units(&self, ctx: &TextContext) -> Result<Units, CommandError> {
        self.read(true, ctx)
    }

    /// Quantité d'une ressource matérielle (R-196) : `value` en 1/10 000
    /// (30 000 = 3 unités de son étiquette) ou `text` en nombre (« 3 »,
    /// « 1,5 »), sans `%`.
    ///
    /// # Errors
    ///
    /// Comme [`UnitsJson::to_units`].
    pub fn to_quantity(&self, ctx: &TextContext) -> Result<Units, CommandError> {
        self.read(false, ctx)
    }

    fn read(&self, percent: bool, ctx: &TextContext) -> Result<Units, CommandError> {
        if let Some(text) = &self.text {
            let u = parse_units_text(text, percent, ctx).map_err(CommandError::text)?;
            return if agrees(self.value, u.get()) {
                Ok(u)
            } else {
                Err(mismatch())
            };
        }
        let Some(value) = self.value else {
            return Err(invalid("« value » ou « text » attendu"));
        };
        Units::new(value).map_err(|_| invalid("unités hors des bornes (0 à 600 000 000 000, R-002)"))
    }
}

/// Pourcentage d'avancement en entrée (J6, `progress.update`, R-216) :
/// `value` en 1/100 de % (10 000 = 100 %) ou `text` (`"40%"`, `"12,5 %"`) ;
/// les deux fournis doivent concorder strictement (comme [`UnitsJson`]).
/// Lecture et contrôle : lot L6.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PercentJson {
    /// Valeur en 1/100 de % (0 à 10 000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 10_000))]
    pub value: Option<i64>,
    /// Pourcentage en texte (`"40%"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl PercentJson {
    /// % d'avancement (R-216) : `value` en 1/100 de % (10 000 = 100 %) ou
    /// `text` (« 40% », « 12,5 % », nombre dans la langue du lot, `%`
    /// obligatoire), arrondi par R-001 ; les deux fournis doivent concorder
    /// strictement (comme [`UnitsJson::to_units`]).
    ///
    /// # Errors
    ///
    /// `text.*` si le texte est illisible ; `param.mismatch` si `value`
    /// contredit le texte ; `param.invalid` sans valeur, ou hors de
    /// `0..=10 000`. Le champ fautif est posé par l'appelant.
    pub fn to_percent(&self, ctx: &TextContext) -> Result<plannercraft_core::Percent, CommandError> {
        let out_of_range = || invalid("pourcentage hors des bornes (0 à 100 %, 0 à 10 000)");
        let value = if let Some(text) = &self.text {
            let u = parse_units_text(text, true, ctx).map_err(CommandError::text)?;
            if !agrees(self.value, u.get()) {
                return Err(mismatch());
            }
            u.get()
        } else {
            self.value
                .ok_or_else(|| invalid("« value » ou « text » attendu"))?
        };
        plannercraft_core::Percent::new(value).map_err(|_| out_of_range())
    }
}

/// Cumul d'une ligne de base vers les récapitulatives (J6, R-222 ; S58
/// « Roll up baselines »).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RollupJson {
    /// Aucun cumul.
    #[default]
    None,
    /// « To all summary tasks ».
    ToAllSummaries,
    /// « From subtasks into selected summary task(s) ».
    FromSubtasks,
}

impl From<RollupJson> for plannercraft_core::baseline::Rollup {
    fn from(r: RollupJson) -> Self {
        match r {
            RollupJson::None => Self::None,
            RollupJson::ToAllSummaries => Self::ToAllSummaries,
            RollupJson::FromSubtasks => Self::FromSubtasks,
        }
    }
}

/// Date expliquée par `explain_date` (J6, R-230, R-231).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SideJson {
    /// Début.
    #[default]
    Start,
    /// Fin.
    Finish,
}

impl From<SideJson> for plannercraft_core::schedule::explain::Side {
    fn from(s: SideJson) -> Self {
        match s {
            SideJson::Start => Self::Start,
            SideJson::Finish => Self::Finish,
        }
    }
}

/// Type de tâche en JSON (S41).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskTypeJson {
    /// Unités fixes.
    FixedUnits,
    /// Durée fixe.
    FixedDuration,
    /// Travail fixe (toujours piloté par l'effort).
    FixedWork,
}

impl From<TaskType> for TaskTypeJson {
    fn from(t: TaskType) -> Self {
        match t {
            TaskType::FixedUnits => Self::FixedUnits,
            TaskType::FixedDuration => Self::FixedDuration,
            TaskType::FixedWork => Self::FixedWork,
        }
    }
}

impl From<TaskTypeJson> for TaskType {
    fn from(t: TaskTypeJson) -> Self {
        match t {
            TaskTypeJson::FixedUnits => Self::FixedUnits,
            TaskTypeJson::FixedDuration => Self::FixedDuration,
            TaskTypeJson::FixedWork => Self::FixedWork,
        }
    }
}

/// Type de ressource en JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKindJson {
    /// Travail (personne, machine).
    #[default]
    Work,
    /// Matérielle (consommée en quantité, étiquette d'unité).
    Material,
    /// Coût.
    Cost,
}

/// Granularité d'un rapport d'utilisation en JSON (R-188).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GranularityJson {
    /// Jour.
    Day,
    /// Semaine (défaut).
    #[default]
    Week,
    /// Mois.
    Month,
    /// Trimestre civil (1er janvier, avril, juillet, octobre ; J7, R-246).
    Quarter,
}

impl From<GranularityJson> for Granularity {
    fn from(g: GranularityJson) -> Self {
        match g {
            GranularityJson::Day => Self::Day,
            GranularityJson::Week => Self::Week,
            GranularityJson::Month => Self::Month,
            GranularityJson::Quarter => Self::Quarter,
        }
    }
}

impl From<Granularity> for GranularityJson {
    fn from(g: Granularity) -> Self {
        match g {
            Granularity::Day => Self::Day,
            Granularity::Week => Self::Week,
            Granularity::Month => Self::Month,
            Granularity::Quarter => Self::Quarter,
        }
    }
}

/// Unité d'un taux en JSON (J7, R-195 amendée ; `plannercraft_core::RateUnit`,
/// format d'origine `StandardRateFormat` gardé, O46).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RateUnitJson {
    /// Par minute.
    Minute,
    /// Par heure.
    Hour,
    /// Par jour.
    Day,
    /// Par semaine.
    Week,
    /// Par mois.
    Month,
    /// Par an.
    Year,
    /// Par unité de matériau (ressource matérielle).
    MaterialUnit,
}

/// Taux en JSON (J7, R-195 amendée) : montant en centièmes d'unité
/// monétaire et unité, stocké tel quel, jamais converti ni calculé
/// (ADR-0013 D7). Lecture et contrôle : lot L5.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RateJson {
    /// Montant en centièmes (`plannercraft_core::Money`).
    pub amount: i64,
    /// Unité du taux (défaut : heure ; unité de matériau pour une ressource
    /// matérielle).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per: Option<RateUnitJson>,
}

/// Jour de la semaine en JSON (J7, `calendar.set_week`, R-247).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WeekdayJson {
    /// Lundi.
    Monday,
    /// Mardi.
    Tuesday,
    /// Mercredi.
    Wednesday,
    /// Jeudi.
    Thursday,
    /// Vendredi.
    Friday,
    /// Samedi.
    Saturday,
    /// Dimanche.
    Sunday,
}

/// Plage ouvrée d'un jour (J7, R-247) : `[from, to)`, `to` peut valoir
/// `"24:00"` (contrôle : lot L5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RangeJson {
    /// Début (`"08:00"`).
    #[schemars(with = "TimeOfDaySchema")]
    pub from: TimeOfDay,
    /// Fin (`"12:00"`, `"24:00"`).
    #[schemars(with = "TimeOfDaySchema")]
    pub to: TimeOfDay,
}

/// Réglage d'un jour de la semaine type (J7, R-247).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WeekSettingJson {
    /// Hérité de la base (calendrier dérivé seulement).
    Inherit,
    /// Chômé.
    NonWorking,
    /// Ouvré sur ces plages (triées, disjointes, au plus 5).
    Ranges(Vec<RangeJson>),
}

/// Jour de la semaine type et son réglage (J7, R-247).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DaySettingJson {
    /// Jour.
    pub day: WeekdayJson,
    /// Réglage.
    pub setting: WeekSettingJson,
}
