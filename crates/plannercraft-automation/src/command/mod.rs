//! Commandes sérialisées (contrat J4, ADR-0012 D5, D6).
//!
//! Format versionné unique, partagé par `plannercraft apply`, le MCP, le canal de
//! contrôle et le journal :
//! `{"format_version":1,"lang":"fr","ops":[{"op":"task.add",…}]}`, champs
//! inconnus refusés. Les tâches sont désignées par UID (entier JSON) ou par
//! une étiquette `"$x"` créée par un `task.add` antérieur du même lot
//! (R-150, lot L3). Dates en ISO local (`"2026-11-02T08:00"`).
//!
//! Propriétaires : types et schémas, L3 ; exécution, L4 (`task.rs`) et L5
//! (`link.rs`, `constraint.rs`, `project.rs`, `calendar.rs`) ; contexte
//! d'exécution (`exec.rs`), L2.

mod assignment;
mod baseline;
mod calendar;
mod constraint;
mod custom;
pub(crate) mod exec;
mod extras_invalidation;
mod link;
pub(crate) mod outline;
pub mod params;
mod progress;
mod project;
mod refs;
mod resource;
pub mod shift;
pub mod style;
mod task;

pub use outline::is_summary_at;

use std::borrow::Cow;
use std::collections::BTreeMap;

use plannercraft_core::{CalendarUid, Day, Moment, ResourceUid, TaskUid, TimeOfDay};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use self::baseline::{BaselineClear, BaselineSet};
pub use self::calendar::{CalendarAdd, CalendarDelete, CalendarSetWeek, CalendarUpdate};
pub use self::custom::{CustomFieldRename, parse_custom_number};
pub use self::progress::{ProgressReschedule, ProgressUpdate};
use self::refs::label_refusal;
pub use self::refs::{LABEL_PATTERN, define_label, is_valid_label, resolve_task};
pub use self::shift::TaskShift;
use crate::error::{CommandError, CommandErrorKind, codes};
pub use crate::json::{
    ConstraintKindJson, DaySettingJson, DurationJson, DurationUnitJson, GranularityJson, LagJson,
    LinkTypeJson, PercentJson, RangeJson, RateJson, RateUnitJson, ResourceKindJson, RollupJson, SideJson,
    TaskTypeJson, UnitsJson, WeekSettingJson, WeekdayJson,
};
use crate::json::{DaySchema, MomentSchema, TimeOfDaySchema};
use crate::text::Language;

/// Lot de commandes : une transaction, un recalcul, une entrée d'historique
/// (ADR-0012 D2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    /// Version du format (1).
    pub format_version: u32,
    /// Langue des textes (durées) ; à défaut, celle de la session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<Language>,
    /// Opérations, dans l'ordre.
    pub ops: Vec<Command>,
    /// Libellé de l'entrée d'historique.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Longueur maximale de l'explication d'une erreur de lecture JSON, en
/// caractères.
const MAX_PARSE_DETAIL_CHARS: usize = 500;

/// Erreur de lecture JSON → `param.invalid`, explication de serde bornée.
fn parse_error(e: &serde_json::Error) -> CommandError {
    let detail: String = e.to_string().chars().take(MAX_PARSE_DETAIL_CHARS).collect();
    CommandError::new(codes::PARAM_INVALID, CommandErrorKind::InvalidParam(detail))
}

/// Lit les paramètres JSON d'une commande du registre.
///
/// # Errors
///
/// `param.invalid` (champ inconnu, type faux, étiquette mal formée…), avec
/// l'explication de serde.
pub fn parse_params<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, CommandError> {
    serde_json::from_value(value).map_err(|e| parse_error(&e))
}

impl Batch {
    /// Version du format comprise par cette version de PlannerCraft.
    pub const FORMAT_VERSION: u32 = 1;

    /// Lit un lot JSON. La version est contrôlée **avant** les opérations :
    /// un lot d'une version future est refusé comme tel, même si ses
    /// opérations sont inconnues ici.
    ///
    /// # Errors
    ///
    /// `batch.unsupported_version` si `format_version` n'est pas 1 ;
    /// `param.invalid` (champ `format_version`) s'il manque ou n'est pas un
    /// entier, ou pour tout autre défaut de forme.
    pub fn from_json(value: serde_json::Value) -> Result<Self, CommandError> {
        let version = value.get("format_version").and_then(serde_json::Value::as_u64);
        match version {
            None => {
                return Err(CommandError::new(
                    codes::PARAM_INVALID,
                    CommandErrorKind::InvalidParam("« format_version » (entier) attendu".to_owned()),
                )
                .with_field("format_version"));
            }
            Some(v) if v != u64::from(Self::FORMAT_VERSION) => {
                let shown = u32::try_from(v).unwrap_or(u32::MAX);
                return Err(CommandError::new(
                    codes::BATCH_UNSUPPORTED_VERSION,
                    CommandErrorKind::UnsupportedVersion(shown),
                )
                .with_field("format_version"));
            }
            Some(_) => {}
        }
        parse_params(value)
    }

    /// Lit un lot depuis son texte JSON (voir [`Batch::from_json`]).
    ///
    /// # Errors
    ///
    /// Comme [`Batch::from_json`] ; `param.invalid` si le texte n'est pas du
    /// JSON.
    pub fn from_json_str(s: &str) -> Result<Self, CommandError> {
        let value: serde_json::Value = serde_json::from_str(s).map_err(|e| parse_error(&e))?;
        Self::from_json(value)
    }
}

/// Commande d'écriture. Le nom JSON (`"op"`) est l'id du registre.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op")]
#[non_exhaustive]
pub enum Command {
    /// Ajoute une tâche (R-152).
    #[serde(rename = "task.add")]
    TaskAdd(TaskAdd),
    /// Modifie une tâche (R-157, R-158).
    #[serde(rename = "task.update")]
    TaskUpdate(TaskUpdate),
    /// Supprime une tâche (R-155).
    #[serde(rename = "task.delete")]
    TaskDelete(TaskDelete),
    /// Déplace un sous-arbre (R-154).
    #[serde(rename = "task.move")]
    TaskMove(TaskMove),
    /// Indente (R-153).
    #[serde(rename = "task.indent")]
    TaskIndent(TaskOne),
    /// Désindente (R-153).
    #[serde(rename = "task.outdent")]
    TaskOutdent(TaskOne),
    /// Ajoute un lien (R-156).
    #[serde(rename = "link.add")]
    LinkAdd(LinkSpec),
    /// Modifie un lien (R-156).
    #[serde(rename = "link.update")]
    LinkUpdate(LinkSpec),
    /// Retire un lien (R-156).
    #[serde(rename = "link.remove")]
    LinkRemove(LinkKey),
    /// Pose une contrainte (R-159).
    #[serde(rename = "task.set_constraint")]
    TaskSetConstraint(SetConstraint),
    /// Pose ou retire une échéance (R-159).
    #[serde(rename = "task.set_deadline")]
    TaskSetDeadline(SetDeadline),
    /// Modifie les informations du projet.
    #[serde(rename = "project.set_info")]
    ProjectSetInfo(ProjectInfoPatch),
    /// Change le sens et la date pilote du projet.
    #[serde(rename = "project.set_anchor")]
    ProjectSetAnchor(AnchorSpec),
    /// Pose ou retire la date d'état.
    #[serde(rename = "project.set_status_date")]
    ProjectSetStatusDate(SetStatusDate),
    /// Crée ou remplace une exception de calendrier (R-165).
    #[serde(rename = "calendar.set_exception")]
    CalendarSetException(CalendarException),
    /// Retire une exception de calendrier (R-165).
    #[serde(rename = "calendar.remove_exception")]
    CalendarRemoveException(CalendarExceptionKey),
    /// Ajoute une ressource (R-195).
    #[serde(rename = "resource.add")]
    ResourceAdd(ResourceAdd),
    /// Modifie une ressource (R-195).
    #[serde(rename = "resource.update")]
    ResourceUpdate(ResourceUpdate),
    /// Supprime une ressource (R-195).
    #[serde(rename = "resource.delete")]
    ResourceDelete(ResourceDelete),
    /// Affecte une ressource à une tâche (R-196).
    #[serde(rename = "assignment.add")]
    AssignmentAdd(AssignmentAdd),
    /// Modifie une affectation (R-196).
    #[serde(rename = "assignment.update")]
    AssignmentUpdate(AssignmentUpdate),
    /// Retire une affectation (R-196).
    #[serde(rename = "assignment.remove")]
    AssignmentRemove(AssignmentKey),
    /// Met à jour l'avancement d'une tâche (R-216).
    #[serde(rename = "progress.update")]
    ProgressUpdate(ProgressUpdate),
    /// Replanifie le travail restant (R-217).
    #[serde(rename = "progress.reschedule")]
    ProgressReschedule(ProgressReschedule),
    /// Enregistre une ligne de base (R-227).
    #[serde(rename = "baseline.set")]
    BaselineSet(BaselineSet),
    /// Crée un calendrier (R-248 ; J7).
    #[serde(rename = "calendar.add")]
    CalendarAdd(CalendarAdd),
    /// Renomme un calendrier ou change sa base (R-248 ; J7).
    #[serde(rename = "calendar.update")]
    CalendarUpdate(CalendarUpdate),
    /// Règle la semaine type d'un calendrier (R-247 ; J7).
    #[serde(rename = "calendar.set_week")]
    CalendarSetWeek(CalendarSetWeek),
    /// Supprime un calendrier (R-248 ; J7).
    #[serde(rename = "calendar.delete")]
    CalendarDelete(CalendarDelete),
    /// Efface une ligne de base (R-250 ; J7).
    #[serde(rename = "baseline.clear")]
    BaselineClear(BaselineClear),
    /// Applique un modèle, une surcharge, ou retire le style du document
    /// (J8, ADR-0016 D7, H-R12).
    #[serde(rename = "style.apply")]
    StyleApply(style::StyleApply),
    /// Mise en forme directe d'un type de barre (J8, H-UI-59, H-UI-60).
    #[serde(rename = "style.set_bar_role")]
    StyleSetBarRole(style::StyleSetBarRole),
    /// Retire les surcharges du style (« Revenir au modèle », J8, H-UI-61).
    #[serde(rename = "style.reset_to_template")]
    StyleResetToTemplate(style::StyleResetToTemplate),
    /// Renomme un champ personnalisé (alias) ou lui rend son nom (R-264,
    /// seconde passe UX).
    #[serde(rename = "custom_field.rename")]
    CustomFieldRename(CustomFieldRename),
    /// Décale des tâches d'une durée signée (R-267, finitions P1).
    #[serde(rename = "task.shift")]
    TaskShift(TaskShift),
}

impl Command {
    /// Id du registre de la commande (valeur de `"op"`).
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
            Self::TaskAdd(_) => "task.add",
            Self::TaskUpdate(_) => "task.update",
            Self::TaskDelete(_) => "task.delete",
            Self::TaskMove(_) => "task.move",
            Self::TaskIndent(_) => "task.indent",
            Self::TaskOutdent(_) => "task.outdent",
            Self::LinkAdd(_) => "link.add",
            Self::LinkUpdate(_) => "link.update",
            Self::LinkRemove(_) => "link.remove",
            Self::TaskSetConstraint(_) => "task.set_constraint",
            Self::TaskSetDeadline(_) => "task.set_deadline",
            Self::ProjectSetInfo(_) => "project.set_info",
            Self::ProjectSetAnchor(_) => "project.set_anchor",
            Self::ProjectSetStatusDate(_) => "project.set_status_date",
            Self::CalendarSetException(_) => "calendar.set_exception",
            Self::CalendarRemoveException(_) => "calendar.remove_exception",
            Self::ResourceAdd(_) => "resource.add",
            Self::ResourceUpdate(_) => "resource.update",
            Self::ResourceDelete(_) => "resource.delete",
            Self::AssignmentAdd(_) => "assignment.add",
            Self::AssignmentUpdate(_) => "assignment.update",
            Self::AssignmentRemove(_) => "assignment.remove",
            Self::ProgressUpdate(_) => "progress.update",
            Self::ProgressReschedule(_) => "progress.reschedule",
            Self::BaselineSet(_) => "baseline.set",
            Self::CalendarAdd(_) => "calendar.add",
            Self::CalendarUpdate(_) => "calendar.update",
            Self::CalendarSetWeek(_) => "calendar.set_week",
            Self::CalendarDelete(_) => "calendar.delete",
            Self::BaselineClear(_) => "baseline.clear",
            Self::StyleApply(_) => "style.apply",
            Self::StyleSetBarRole(_) => "style.set_bar_role",
            Self::StyleResetToTemplate(_) => "style.reset_to_template",
            Self::CustomFieldRename(_) => "custom_field.rename",
            Self::TaskShift(_) => "task.shift",
        }
    }
}

/// Désignation d'une tâche (R-150). JSON : entier → UID ; chaîne `"$x"`
/// (motif [`LABEL_PATTERN`]) → étiquette ; toute autre chaîne est refusée à
/// la lecture (et par [`resolve_task`] si la valeur est construite en Rust).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "TaskRefRepr", into = "TaskRefRepr")]
pub enum TaskRef {
    /// UID.
    Uid(TaskUid),
    /// Étiquette `$x` d'une tâche créée plus tôt dans le lot.
    Label(String),
}

/// Forme JSON de [`TaskRef`].
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum TaskRefRepr {
    Uid(u32),
    Label(String),
}

impl TryFrom<TaskRefRepr> for TaskRef {
    type Error = String;

    fn try_from(r: TaskRefRepr) -> Result<Self, Self::Error> {
        match r {
            TaskRefRepr::Uid(u) => Ok(Self::Uid(TaskUid::new(u))),
            TaskRefRepr::Label(s) if is_valid_label(&s) => Ok(Self::Label(s)),
            TaskRefRepr::Label(s) => Err(label_refusal(&s)),
        }
    }
}

impl From<TaskRef> for TaskRefRepr {
    fn from(r: TaskRef) -> Self {
        match r {
            TaskRef::Uid(u) => Self::Uid(u.get()),
            TaskRef::Label(s) => Self::Label(s),
        }
    }
}

impl JsonSchema for TaskRef {
    fn schema_name() -> Cow<'static, str> {
        "TaskRef".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "oneOf": [
                { "type": "integer", "minimum": 1, "maximum": 4_294_967_295_u32 },
                { "type": "string", "pattern": LABEL_PATTERN }
            ],
            "description": "Tâche : UID (entier) ou étiquette \"$x\" d'une tâche créée plus tôt dans le même lot."
        })
    }
}

/// Placement d'une tâche ajoutée ou déplacée (R-152, R-154).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// En dernière position, niveau 1.
    #[default]
    End,
    /// Après le sous-arbre de la tâche, au même niveau.
    After(TaskRef),
    /// Juste avant la tâche, au même niveau.
    Before(TaskRef),
    /// Premier enfant de la tâche.
    FirstChildOf(TaskRef),
    /// Dernier enfant de la tâche.
    LastChildOf(TaskRef),
}

/// Modification d'un champ : absent (garder), valeur (poser), `null`
/// (effacer). Les champs `Patch` portent `#[serde(default)]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Patch<T> {
    /// Champ absent : valeur gardée.
    #[default]
    Keep,
    /// Nouvelle valeur.
    Set(T),
    /// `null` : valeur effacée.
    Clear,
}

impl<T> Patch<T> {
    /// Vrai pour [`Patch::Keep`] (champ omis à l'écriture).
    #[must_use]
    pub const fn is_keep(&self) -> bool {
        matches!(self, Self::Keep)
    }
}

impl<T: Serialize> Serialize for Patch<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Set(v) => v.serialize(s),
            Self::Keep | Self::Clear => s.serialize_none(),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(d).map(|v| v.map_or(Self::Clear, Self::Set))
    }
}

impl<T: JsonSchema> JsonSchema for Patch<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        format!("Patch_{}", T::schema_name()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        Option::<T>::json_schema(generator)
    }
}

/// `task.add`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskAdd {
    /// Nom.
    pub name: String,
    /// Étiquette `$x` de la tâche créée, pour la suite du lot (R-150).
    #[serde(rename = "as", default, skip_serializing_if = "Option::is_none")]
    #[schemars(pattern(LABEL_PATTERN))]
    pub label: Option<String>,
    /// Placement (défaut : fin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Placement>,
    /// Durée (défaut : celle du projet, estimée).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<DurationJson>,
    /// Jalon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone: Option<bool>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Début saisi (→ contrainte, R-158).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub start: Option<Moment>,
    /// Fin saisie (→ contrainte, R-158).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub finish: Option<Moment>,
}

/// `task.update`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskUpdate {
    /// Tâche.
    pub uid: TaskRef,
    /// Nom (`null` refusé).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub name: Patch<String>,
    /// Durée.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub duration: Patch<DurationJson>,
    /// Jalon.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub milestone: Patch<bool>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub notes: Patch<String>,
    /// Début saisi (R-158).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    #[schemars(with = "Patch<MomentSchema>")]
    pub start: Patch<Moment>,
    /// Fin saisie (R-158).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    #[schemars(with = "Patch<MomentSchema>")]
    pub finish: Patch<Moment>,
    /// Tâche active.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub active: Patch<bool>,
    /// Type de tâche (R-189, R-197 ; J5, refusé `command.not_yet_available`
    /// jusqu'au lot L7).
    #[serde(rename = "type", default, skip_serializing_if = "Patch::is_keep")]
    pub task_type: Patch<TaskTypeJson>,
    /// Pilotée par l'effort (R-190 à R-193, R-197 ; J5, refusé jusqu'au lot
    /// L7).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub effort_driven: Patch<bool>,
    /// Travail total (R-189, R-197 ; `param.conflict` avec `duration` ; J5,
    /// refusé jusqu'au lot L7).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub work: Patch<DurationJson>,
    /// Mode de planification (J8, R-253) : vers manuel, dates planifiées
    /// courantes figées ; vers automatique, valeurs manuelles retirées.
    /// Sur une feuille manuelle, `start`, `finish` et `duration` écrivent
    /// les valeurs manuelles, sans contrainte (R-254).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub mode: Patch<ModeJson>,
    /// Champs personnalisés (R-264, seconde passe UX) : clé `text1` …
    /// `text30`, `number1` … `number20`, `date1` … `date10`, `flag1` …
    /// `flag20` ; valeur : texte (≤ 255 caractères) ; nombre en texte dans
    /// la langue du lot (« 12,5 » ou « 12.5 ») ou nombre JSON ; date ISO
    /// locale comme `start` (« 2026-11-02T08:00 ») ; booléen ; `null`
    /// efface. Aucune date calculée.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<String, serde_json::Value>,
}

/// Mode de planification d'une tâche en JSON (J8, R-253).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModeJson {
    /// Planification automatique.
    Auto,
    /// Planification manuelle (dates figées, S73).
    Manual,
}

impl TaskUpdate {
    /// Champs J5 posés (`type`, `effort_driven`, `work`), dans l'ordre du
    /// schéma : refusés tant que le lot L7 ne les exécute pas.
    #[must_use]
    pub fn j5_fields(&self) -> Vec<&'static str> {
        [
            ("type", self.task_type.is_keep()),
            ("effort_driven", self.effort_driven.is_keep()),
            ("work", self.work.is_keep()),
        ]
        .into_iter()
        .filter_map(|(name, keep)| (!keep).then_some(name))
        .collect()
    }
}

/// `task.delete`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskDelete {
    /// Tâche.
    pub uid: TaskRef,
    /// Supprimer aussi les descendants (obligatoire pour une récapitulative).
    #[serde(default)]
    pub cascade: bool,
}

/// `task.move`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskMove {
    /// Tâche (avec son sous-arbre).
    pub uid: TaskRef,
    /// Destination.
    pub to: Placement,
}

/// Commande à une seule tâche (`task.indent`, `task.outdent`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskOne {
    /// Tâche.
    pub uid: TaskRef,
}

/// `link.add`, `link.update`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkSpec {
    /// Prédécesseur.
    pub pred: TaskRef,
    /// Successeur.
    pub succ: TaskRef,
    /// Type (défaut FS à l'ajout ; gardé à la modification).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<LinkTypeJson>,
    /// Décalage (défaut 0 à l'ajout ; gardé à la modification).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lag: Option<LagJson>,
}

/// `link.remove`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkKey {
    /// Prédécesseur.
    pub pred: TaskRef,
    /// Successeur.
    pub succ: TaskRef,
}

/// `task.set_constraint`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetConstraint {
    /// Tâche.
    pub uid: TaskRef,
    /// Type.
    pub kind: ConstraintKindJson,
    /// Date (obligatoire pour les types datés, interdite sinon).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    pub date: Option<Moment>,
}

/// `task.set_deadline`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetDeadline {
    /// Tâche.
    pub uid: TaskRef,
    /// Échéance (`null` : retirée).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    #[schemars(with = "Patch<MomentSchema>")]
    pub deadline: Patch<Moment>,
}

/// `project.set_status_date`. Struct plutôt que `Patch<Moment>` nu : une
/// variante d'enum à étiquette interne doit contenir un objet JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetStatusDate {
    /// Date d'état (`null` : NA).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    #[schemars(with = "Patch<MomentSchema>")]
    pub date: Patch<Moment>,
}

/// `project.set_info`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfoPatch {
    /// Nom.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub name: Patch<String>,
    /// Titre.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub title: Patch<String>,
    /// Objet.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub subject: Patch<String>,
    /// Auteur.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub author: Patch<String>,
    /// Responsable.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub manager: Patch<String>,
    /// Société.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub company: Patch<String>,
    /// Catégorie.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub category: Patch<String>,
    /// Type des nouvelles tâches (S2 `DefaultTaskType`, R-197 ; J5, refusé
    /// `command.not_yet_available` jusqu'au lot L7).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub default_task_type: Patch<TaskTypeJson>,
    /// Nouvelles tâches pilotées par l'effort (S2 `NewTasksEffortDriven`,
    /// R-197 ; J5, refusé jusqu'au lot L7).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub new_tasks_effort_driven: Patch<bool>,
    /// Calendrier du projet (J7, R-249 ; UID d'un calendrier existant, de
    /// base ou dérivé, `null` refusé).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub calendar: Patch<u32>,
}

impl ProjectInfoPatch {
    /// Champs J5 posés (`default_task_type`, `new_tasks_effort_driven`) :
    /// refusés tant que le lot L7 ne les exécute pas.
    #[must_use]
    pub fn j5_fields(&self) -> Vec<&'static str> {
        [
            ("default_task_type", self.default_task_type.is_keep()),
            ("new_tasks_effort_driven", self.new_tasks_effort_driven.is_keep()),
        ]
        .into_iter()
        .filter_map(|(name, keep)| (!keep).then_some(name))
        .collect()
    }
}

/// Sens de planification et date pilote. JSON : `{"start":"…"}` ou
/// `{"finish":"…"}` (un objet, pour l'étiquette interne de [`Command`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "AnchorRepr", into = "AnchorRepr")]
pub enum AnchorSpec {
    /// Depuis la date de début.
    Start(Moment),
    /// À rebours depuis la date de fin.
    Finish(Moment),
}

/// Forme JSON de [`AnchorSpec`] : exactement un des deux champs.
#[derive(Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AnchorRepr {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    start: Option<Moment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<MomentSchema>")]
    finish: Option<Moment>,
}

impl TryFrom<AnchorRepr> for AnchorSpec {
    type Error = &'static str;

    fn try_from(r: AnchorRepr) -> Result<Self, Self::Error> {
        match (r.start, r.finish) {
            (Some(s), None) => Ok(Self::Start(s)),
            (None, Some(f)) => Ok(Self::Finish(f)),
            _ => Err("exactement un de « start » ou « finish » attendu"),
        }
    }
}

impl From<AnchorSpec> for AnchorRepr {
    fn from(a: AnchorSpec) -> Self {
        match a {
            AnchorSpec::Start(s) => Self {
                start: Some(s),
                finish: None,
            },
            AnchorSpec::Finish(f) => Self {
                start: None,
                finish: Some(f),
            },
        }
    }
}

/// Calendrier désigné : UID (entier JSON) ou `"project"` (calendrier du
/// projet).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CalendarRefRepr", into = "CalendarRefRepr")]
pub enum CalendarRef {
    /// Calendrier du projet.
    Project,
    /// UID.
    Uid(CalendarUid),
}

/// Forme JSON de [`CalendarRef`].
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum CalendarRefRepr {
    Uid(u32),
    Name(String),
}

impl TryFrom<CalendarRefRepr> for CalendarRef {
    type Error = &'static str;

    fn try_from(r: CalendarRefRepr) -> Result<Self, Self::Error> {
        match r {
            CalendarRefRepr::Uid(u) => Ok(Self::Uid(CalendarUid::new(u))),
            CalendarRefRepr::Name(s) if s == "project" => Ok(Self::Project),
            CalendarRefRepr::Name(_) => Err("UID de calendrier ou « project » attendu"),
        }
    }
}

impl From<CalendarRef> for CalendarRefRepr {
    fn from(c: CalendarRef) -> Self {
        match c {
            CalendarRef::Project => Self::Name("project".to_owned()),
            CalendarRef::Uid(u) => Self::Uid(u.get()),
        }
    }
}

impl JsonSchema for CalendarRef {
    fn schema_name() -> Cow<'static, str> {
        "CalendarRef".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "oneOf": [
                { "type": "integer", "minimum": 0, "maximum": 4_294_967_295_u32 },
                { "const": "project" }
            ]
        })
    }
}

/// `calendar.set_exception` (R-165) : jours `[from, to]` chômés
/// (`ranges` vide) ou ouvrés sur les plages données.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalendarException {
    /// Calendrier.
    pub cal: CalendarRef,
    /// Premier jour.
    #[schemars(with = "DaySchema")]
    pub from: Day,
    /// Dernier jour (inclus).
    #[schemars(with = "DaySchema")]
    pub to: Day,
    /// Plages ouvrées `["08:00","12:00"]` ; vide : jours chômés.
    #[serde(default)]
    #[schemars(with = "Vec<[TimeOfDaySchema; 2]>")]
    pub ranges: Vec<[TimeOfDay; 2]>,
    /// Nom de l'exception (J7, R-165 amendée, ≤ 512 caractères, O44 ;
    /// absent : nom gardé à un remplacement, vide à une création).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// `calendar.remove_exception` (R-165).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CalendarExceptionKey {
    /// Calendrier.
    pub cal: CalendarRef,
    /// Premier jour de l'exception.
    #[schemars(with = "DaySchema")]
    pub from: Day,
}

/// Désignation d'une ressource (R-195, même motif que [`TaskRef`]). JSON :
/// entier → UID ; chaîne `"$x"` → étiquette d'une ressource créée plus tôt
/// dans le lot (`resource.add`, `"as"`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "ResourceRefRepr", into = "ResourceRefRepr")]
pub enum ResourceRef {
    /// UID.
    Uid(ResourceUid),
    /// Étiquette `$x` d'une ressource créée plus tôt dans le lot.
    Label(String),
}

/// Forme JSON de [`ResourceRef`].
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum ResourceRefRepr {
    Uid(u32),
    Label(String),
}

impl TryFrom<ResourceRefRepr> for ResourceRef {
    type Error = String;

    fn try_from(r: ResourceRefRepr) -> Result<Self, Self::Error> {
        match r {
            ResourceRefRepr::Uid(u) => Ok(Self::Uid(ResourceUid::new(u))),
            ResourceRefRepr::Label(s) if is_valid_label(&s) => Ok(Self::Label(s)),
            ResourceRefRepr::Label(s) => Err(label_refusal(&s)),
        }
    }
}

impl From<ResourceRef> for ResourceRefRepr {
    fn from(r: ResourceRef) -> Self {
        match r {
            ResourceRef::Uid(u) => Self::Uid(u.get()),
            ResourceRef::Label(s) => Self::Label(s),
        }
    }
}

impl JsonSchema for ResourceRef {
    fn schema_name() -> Cow<'static, str> {
        "ResourceRef".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "oneOf": [
                { "type": "integer", "minimum": 0, "maximum": 4_294_967_295_u32 },
                { "type": "string", "pattern": LABEL_PATTERN }
            ],
            "description": "Ressource : UID (entier) ou étiquette \"$x\" d'une ressource créée plus tôt dans le même lot."
        })
    }
}

/// Calendrier d'une ressource créée : `"own"` (calendrier propre dérivé du
/// calendrier du projet, défaut, O34), `"project"` (calendrier du projet
/// partagé) ou UID d'un calendrier existant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ResourceCalendarRepr", into = "ResourceCalendarRepr")]
pub enum ResourceCalendarJson {
    /// Calendrier propre, créé et dérivé du calendrier du projet.
    #[default]
    Own,
    /// Calendrier du projet.
    Project,
    /// Calendrier existant.
    Uid(CalendarUid),
}

/// Forme JSON de [`ResourceCalendarJson`].
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum ResourceCalendarRepr {
    Uid(u32),
    Name(String),
}

impl TryFrom<ResourceCalendarRepr> for ResourceCalendarJson {
    type Error = &'static str;

    fn try_from(r: ResourceCalendarRepr) -> Result<Self, Self::Error> {
        match r {
            ResourceCalendarRepr::Uid(u) => Ok(Self::Uid(CalendarUid::new(u))),
            ResourceCalendarRepr::Name(s) if s == "own" => Ok(Self::Own),
            ResourceCalendarRepr::Name(s) if s == "project" => Ok(Self::Project),
            ResourceCalendarRepr::Name(_) => Err("« own », « project » ou UID de calendrier attendu"),
        }
    }
}

impl From<ResourceCalendarJson> for ResourceCalendarRepr {
    fn from(c: ResourceCalendarJson) -> Self {
        match c {
            ResourceCalendarJson::Own => Self::Name("own".to_owned()),
            ResourceCalendarJson::Project => Self::Name("project".to_owned()),
            ResourceCalendarJson::Uid(u) => Self::Uid(u.get()),
        }
    }
}

impl JsonSchema for ResourceCalendarJson {
    fn schema_name() -> Cow<'static, str> {
        "ResourceCalendar".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "oneOf": [
                { "type": "integer", "minimum": 0, "maximum": 4_294_967_295_u32 },
                { "enum": ["own", "project"] }
            ]
        })
    }
}

/// `resource.add` (R-195).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceAdd {
    /// Nom.
    pub name: String,
    /// Étiquette `$x` de la ressource créée, pour la suite du lot.
    #[serde(rename = "as", default, skip_serializing_if = "Option::is_none")]
    #[schemars(pattern(LABEL_PATTERN))]
    pub label: Option<String>,
    /// Type (défaut : travail).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ResourceKindJson>,
    /// Étiquette d'unité d'une ressource matérielle (« m³ »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_label: Option<String>,
    /// Capacité maximale (défaut 100 %, S47).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_units: Option<UnitsJson>,
    /// Calendrier (défaut : calendrier propre, O34).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<ResourceCalendarJson>,
    /// Initiales.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    /// Groupe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

/// `resource.update` (R-195).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceUpdate {
    /// Ressource.
    pub uid: ResourceRef,
    /// Nom (`null` refusé).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub name: Patch<String>,
    /// Type (`null` refusé ; refusé `resource.has_assignments` si la
    /// ressource est affectée ; ajout du lot L7 au contrat L0).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub kind: Patch<ResourceKindJson>,
    /// Étiquette d'unité d'une ressource matérielle (ajout du lot L7).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub material_label: Patch<String>,
    /// Capacité maximale (`null` refusé).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub max_units: Patch<UnitsJson>,
    /// Calendrier (`null` : aucun, calendrier de la tâche ou du projet).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    #[schemars(with = "Patch<u32>")]
    pub calendar: Patch<CalendarUid>,
    /// Initiales.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub initials: Patch<String>,
    /// Groupe.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub group: Patch<String>,
    /// Notes.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub notes: Patch<String>,
    /// Ressource inactive.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub inactive: Patch<bool>,
    /// Taux standard (J7, R-195 amendée : stocké tel quel, jamais
    /// calculé ; `null` : 0).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub standard_rate: Patch<RateJson>,
    /// Taux des heures supplémentaires (J7, R-195 amendée ; ressource de
    /// travail).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub overtime_rate: Patch<RateJson>,
    /// Coût par utilisation (J7, R-195 amendée ; montant seul, `per`
    /// refusé).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub cost_per_use: Patch<RateJson>,
}

/// `resource.delete` (R-195).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceDelete {
    /// Ressource.
    pub uid: ResourceRef,
    /// Retirer aussi ses affectations (obligatoire si elle en a :
    /// `resource.has_assignments`).
    #[serde(default)]
    pub cascade: bool,
}

/// `assignment.add` (R-196).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignmentAdd {
    /// Tâche.
    pub task: TaskRef,
    /// Ressource.
    pub resource: ResourceRef,
    /// Unités (ressource de travail ; défaut 100 %).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<UnitsJson>,
    /// Travail de l'affectation (ressource de travail).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<DurationJson>,
    /// Quantité (ressource matérielle).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<UnitsJson>,
    /// Retard d'affectation (`Delay`, R-177).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<DurationJson>,
}

/// `assignment.update` (R-196).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignmentUpdate {
    /// Tâche.
    pub task: TaskRef,
    /// Ressource.
    pub resource: ResourceRef,
    /// Unités.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub units: Patch<UnitsJson>,
    /// Travail.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub work: Patch<DurationJson>,
    /// Quantité (ressource matérielle).
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub quantity: Patch<UnitsJson>,
    /// Retard d'affectation.
    #[serde(default, skip_serializing_if = "Patch::is_keep")]
    pub delay: Patch<DurationJson>,
}

/// `assignment.remove` (R-196) : l'affectation de `resource` à `task`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignmentKey {
    /// Tâche.
    pub task: TaskRef,
    /// Ressource.
    pub resource: ResourceRef,
}
