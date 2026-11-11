//! Cœur de PlannerCraft : modèle de données et moteur de planification.
//!
//! Cette crate ne fait aucune entrée/sortie : elle ne connaît ni le système de
//! fichiers, ni l'heure système, ni MCP, ni egui (règles 5 et 6 de `CLAUDE.md`).
//! « Maintenant » est toujours injecté par l'appelant. Les durées sont en
//! minutes entières (`i64`) et aucun flottant n'est admis dans le moteur.
//! Elle compile en `wasm32-unknown-unknown` (vérifié en CI).
//!
//! Gardes, en plus des lints du workspace :
//! - les attributs ci-dessous (calcul flottant, sorties console) ;
//! - `clippy.toml` de cette crate : horloges (`chrono::Utc::now`,
//!   `SystemTime::now`…), système de fichiers (`std::fs`), variables
//!   d'environnement, types `f32`/`f64`, `HashMap`/`HashSet`. Il s'applique
//!   aussi dans un build du workspace, où chrono reçoit la feature `now` par
//!   rmcp.
//!
//! État : jalon J2 en cours (ADR-0009). Modules :
//! - [`fixed`] : arrondi unique (R-001) et décimal exact ;
//! - [`quantity`] : unités, pourcentages et coûts en point fixe (R-002) ;
//! - [`time`] : instants à la minute, jours, heures, semaines (R-003 à R-005) ;
//! - [`duration`] : minutes, durées, travail, décalages, facteurs de
//!   conversion des unités (R-006 à R-010) ;
//! - [`ids`] : UID, ID, GUID, numéros de ligne de base ;
//! - [`arena`] : collections ordonnées indexées par UID ;
//! - [`calendar`] : modèle stocké des calendriers, calendriers fournis,
//!   résolution et récurrences, forme compilée et conversions ouvrées,
//!   placement des durées et des jalons (R-011 à R-031) ;
//! - [`model`] : modèle V1 (projet, tâches, liens, ressources, affectations,
//!   lignes de base), sérialisation validée (R-032 à R-034) ;
//! - [`schedule`] : moteur de planification (chemin critique, liens,
//!   décalages, marges ; R-035 à R-069, J2) ;
//! - [`resource`] : équilibrage durée, travail, unités à l'édition et
//!   utilisation par période (R-188 à R-194, J5) ;
//! - [`progress`] : faits d'avancement, restes dérivés et équilibrage d'une
//!   saisie (R-200 à R-213, J6) ;
//! - [`baseline`] : capture des lignes de base et écarts (R-220 à R-225,
//!   J6) ;
//! - [`diagnostic`] : diagnostics typés, gravité et sujet déduits ;
//! - [`diagnostic_args`] : arguments neutres des diagnostics, pour leur
//!   traduction par code (N5, J8) ;
//! - [`error`] : erreurs typées, codes stables.

#![deny(
    clippy::float_arithmetic,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::dbg_macro
)]

pub mod arena;
pub mod baseline;
pub mod calendar;
pub mod diagnostic;
pub mod diagnostic_args;
pub mod duration;
pub mod error;
pub mod fixed;
pub mod ids;
pub mod model;
pub mod parity;
pub mod progress;
pub mod quantity;
pub mod resource;
pub mod schedule;
pub mod time;

pub use arena::{Arena, HasUid};
pub use calendar::{BuiltinCalendarNames, CalendarSet, CompiledCalendar, Direction, Span, TimeScale};
pub use diagnostic::{Diagnostic, Diagnostics, OverriddenBound, Pass, Severity, Subject};
pub use diagnostic_args::DiagArg;
pub use duration::{
    ConversionFactors, Duration, DurationUnit, Lag, LagValue, Minutes, TimeBasis, Work, percent_of,
};
pub use error::{
    CalendarError, CalendarModelError, CoreError, DurationError, EntityKind, FixedError, ModelError,
    TimeError,
};
pub use fixed::{Decimal, round_div};
pub use ids::{AssignmentUid, BaselineNumber, CalendarUid, EntityUid, Guid, ResourceUid, TaskId, TaskUid};
pub use model::{
    Assignment, AssignmentBaseline, Constraint, ConstraintKind, CustomFieldDef, CustomFieldId, CustomKind,
    CustomValue, CustomValueError, ExtraEntry, Extras, LevelingDelay, Link, LinkType, OutlineLevel, Priority,
    Progress, ProgressOption, ProgressOptions, Project, ProjectInfo, ProjectInit, ProjectOptions, Resource,
    ResourceBaseline, ResourceKind, ScheduleAnchor, SchedulingMode, Task, TaskBaseline, TaskFields, TaskType,
    WorkContour, resource_id,
};
pub use progress::{ProgressFacts, ProgressState};
pub use quantity::{Money, Percent, Rate, RateUnit, SignedPercent, Units};
pub use schedule::{
    AssignmentRole, AssignmentSchedule, Driver, EffectiveCalendar, Exclusion, LateDriver, Limits, Overload,
    ProgressSchedule, ResourceLoad, Schedule, TaskOutcome, TaskRole, TaskSchedule, schedule, schedule_with,
};
pub use time::{Day, DayRange, Moment, Month, TimeOfDay, Week, Weekday};

/// Nom affiché du produit.
///
/// Seul endroit qui définit ce nom dans le code : la CLI, la fenêtre desktop
/// et la version web le lisent ici. Les autres endroits où il apparaît
/// (noms de crates, binaires, fichiers) sont listés dans `docs/rename.md`.
pub const PRODUCT_NAME: &str = "PlannerCraft";

/// Nom technique du produit, en minuscules : binaire de la CLI, nom du
/// serveur MCP, répertoire d'état utilisateur, préfixe des clés de
/// persistance.
pub const PRODUCT_ID: &str = "plannercraft";

/// Ancien nom technique (`planrs`, nom provisoire jusqu'au 2026-11-03).
/// Lu seulement pour reprendre sans perte l'état laissé par une version
/// antérieure (préférences d'interface, répertoire d'état, instruction de
/// style des fichiers) ; jamais écrit.
pub const LEGACY_PRODUCT_ID: &str = "planrs";

/// Version du produit : celle du workspace (`[workspace.package] version`).
///
/// Une seule version pour tout le dépôt : `plannercraft --version` et
/// `plannercraft-desktop --version` l'impriment (`PlannerCraft <version>`, C-REL-03) et
/// `cargo xtask release-check` exige qu'elle égale le tag publié et la
/// première section de `CHANGELOG.md` (C-REL-01, `docs/features/release.md`).
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");
