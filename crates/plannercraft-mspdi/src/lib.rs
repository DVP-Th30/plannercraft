//! Import et export du format MSPDI XML (Microsoft Project Data Interchange).
//!
//! MSPDI est le format natif de PlannerCraft en V1 : il garantit l'échange avec
//! MS Project. L'implémentation suit le schéma XML public de Microsoft
//! (`mspdi_pj15.xsd`, Project 2013 SDK, cité par URL et empreinte, jamais
//! commité) et le comportement observé sur des fichiers enregistrés par
//! MS Project (clean-room, règle 1). Aucun flottant : les décimales des
//! durées sont lues en entiers et arrondies par la règle unique R-001.
//!
//! État (J3, ADR-0010) : lecture ([`read`]) et écriture ([`write::write`]).
//! Lecture événementielle
//! quick-xml, arbre borné par sous-arbre (pile explicite, limites de taille,
//! de profondeur, d'éléments et de texte ; DOCTYPE refusé), correspondance
//! typée indulgente : une valeur illisible devient un diagnostic et reçoit sa
//! valeur par défaut, jamais un échec global. Les valeurs calculées par
//! MS Project vont dans [`RecordedSchedule`], jamais dans le `Project` :
//! l'effacement exigé par l'ADR-0004 est structurel.
//!
//! Modules :
//! - [`scalar`] : booléens, entiers, dates, heures, durées ISO 8601,
//!   dixièmes de minute (R-070, R-072, R-073, R-075) ;
//! - [`codes`] : tables des codes numériques MSPDI vers les types du cœur ;
//! - [`error`] : erreurs structurelles typées, positions ;
//! - [`diag`] : diagnostics d'import ;
//! - [`extras`] : éléments non modélisés conservés (clés, ordre canonique) ;
//! - [`schema`] : table unique des éléments du schéma (séquences pj15,
//!   classes, ordre observé ; R-087, R-121) ;
//! - [`style_pi`] : style du document dans le fichier
//!   (`<?plannercraft-style?>`, H-R12, J8) ;
//! - [`write`] : écriture MSPDI, profils d'ordre, diagnostics d'écriture
//!   (R-120 à R-132).

pub mod codes;
pub mod custom;
pub mod diag;
pub mod error;
pub mod extras;
mod normalize;
pub mod scalar;
pub mod schema;
pub mod style_pi;
pub mod write;

mod assignment;
mod baseline;
mod calendar;
mod fields;
mod project;
mod recorded;
mod resource;
mod task;
mod xml;

use std::collections::BTreeMap;

use plannercraft_core::{Moment, Project};

pub use diag::{ImportCode, ImportDiagnostic, LinkIgnoredReason};
pub use error::{Location, MspdiError};
pub use normalize::normalize;
pub use recorded::{RecordedAssignment, RecordedResource, RecordedSchedule, RecordedTask, Tenths};

/// Plafond de la profondeur d'imbrication, quelle que soit
/// [`Limits::max_depth`] : la pile des éléments ouverts reste petite, et
/// aucune valeur réglée par l'appelant ne la rend dangereuse (règle 10).
pub const MAX_DEPTH: u16 = 256;

/// Limites de lecture : protègent contre un document hostile ou corrompu.
///
/// Mémoire : les éléments sautés ne coûtent qu'un compteur ; seuls les
/// éléments retenus (champs lus des tâches, calendriers, affectations et du
/// projet) sont construits, environ 100 octets chacun, bornés par
/// `max_retained` ; les diagnostics d'import sont plafonnés (10 000 au
/// total, 1 000 par code). Avec les valeurs par défaut, la mémoire de la
/// lecture reste de l'ordre de quelques centaines de Mio au pire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Taille maximale de l'entrée, en octets.
    pub max_bytes: u64,
    /// Profondeur maximale d'imbrication, plafonnée à [`MAX_DEPTH`].
    pub max_depth: u16,
    /// Nombre maximal d'éléments (retenus ou sautés).
    pub max_elements: u64,
    /// Nombre maximal d'éléments retenus (construits en mémoire).
    pub max_retained: u64,
    /// Longueur maximale du texte d'un élément, en octets.
    pub max_text_bytes: usize,
    /// Total maximal des octets capturés en `Extras` (R-087).
    pub max_extras_bytes: u64,
}

impl Default for Limits {
    /// 256 Mio, 64 niveaux, 20 000 000 éléments dont 3 000 000 retenus
    /// (environ 60 000 tâches de 50 champs), 16 Mio de texte, captures
    /// bornées par la taille de l'entrée.
    fn default() -> Self {
        let max_bytes = 256 * 1024 * 1024;
        Self {
            max_bytes,
            max_depth: 64,
            max_elements: 20_000_000,
            max_retained: 3_000_000,
            max_text_bytes: 16 * 1024 * 1024,
            max_extras_bytes: max_bytes,
        }
    }
}

/// Options de lecture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadOptions {
    /// « Maintenant » injecté (règle 5) : date courante du projet si
    /// `CurrentDate` est absent, début du projet si `StartDate` l'est.
    pub current_date: Moment,
    /// Limites.
    pub limits: Limits,
    /// Conserver les éléments non modélisés en `Extras` (R-087) ; faux :
    /// ils sont sautés (mémoire minimale, aller-retour avec pertes).
    pub keep_extras: bool,
}

impl ReadOptions {
    /// Options avec les limites par défaut, `Extras` conservés.
    #[must_use]
    pub fn new(current_date: Moment) -> Self {
        Self {
            current_date,
            limits: Limits::default(),
            keep_extras: true,
        }
    }
}

/// Provenance du document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceInfo {
    /// `SaveVersion` (14 pour MS Project 2010 et suivants).
    pub save_version: Option<i64>,
    /// Espace de noms déclaré par la racine.
    pub namespace: Option<String>,
    /// Sections d'interface présentes (`Views`, `Tables`, `Filters`, `Groups`,
    /// `Maps`, `Reports`, `Drawings`, `DataLinks`, `VBAProjects`) : signature d'un
    /// enregistrement par MS Project.
    pub ui_sections: Vec<String>,
    /// Éléments sautés (contenu d'un champ scalaire, conteneurs inconnus,
    /// éléments non conservés avec `keep_extras` faux, calculés non
    /// recalculés et perdus ci-dessous compris…).
    pub skipped_elements: u64,
    /// Éléments calculés non recalculés par PlannerCraft (classe X : coûts,
    /// valeur acquise, écarts, surutilisation, travail des ressources), par
    /// nom : ni lus ni conservés, MS Project les recalcule (R-087, D13).
    pub dropped_computed: BTreeMap<String, u64>,
    /// Enfants non lus d'un élément sans `Extras` (jour de calendrier,
    /// exception, semaine datée, plage, lien) : perdus (`mspdi.not_preserved`).
    pub not_preserved: u64,
    /// Diagnostics d'import supprimés au-delà des plafonds
    /// ([`diag::MAX_DIAGNOSTICS`], [`diag::MAX_DIAGNOSTICS_PER_CODE`]) ;
    /// un diagnostic [`ImportCode::Truncated`] les signale aussi.
    pub dropped_diagnostics: u64,
}

/// Document MSPDI lu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MspdiDocument {
    /// Entrées seules (ADR-0008), options CPM comprises.
    pub project: Project,
    /// Valeurs calculées telles qu'enregistrées : jamais dans `project`.
    pub recorded: RecordedSchedule,
    /// Provenance.
    pub source: SourceInfo,
    /// Diagnostics d'import, puis ceux de `Project::validate` (enveloppés).
    pub diagnostics: Vec<ImportDiagnostic>,
}

/// BOM UTF-8.
const BOM_UTF8: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Lit un document MSPDI. Aucune E/S : l'entrée est en octets (testable,
/// wasm).
///
/// # Errors
///
/// [`MspdiError`] seulement si le document est structurellement
/// inexploitable ; tout le reste devient un diagnostic d'import.
pub fn read(input: &[u8], options: &ReadOptions) -> Result<MspdiDocument, MspdiError> {
    let limits = options.limits;
    let size = u64::try_from(input.len()).unwrap_or(u64::MAX);
    if size > limits.max_bytes {
        return Err(MspdiError::TooLarge {
            size,
            max: limits.max_bytes,
        });
    }
    if matches!(input, [0xFE, 0xFF, ..] | [0xFF, 0xFE, ..]) {
        return Err(MspdiError::Utf16NotSupported);
    }
    let body = input.strip_prefix(BOM_UTF8).unwrap_or(input);
    if body.is_empty() {
        return Err(MspdiError::Empty);
    }
    let text = std::str::from_utf8(body).map_err(|e| MspdiError::InvalidUtf8 {
        at: error::locate_in_bytes(body, e.valid_up_to()),
    })?;
    let lines = error::LineIndex::new(text);
    let mut sink = diag::Sink::new(&lines);
    let raw = xml::read_document(text, limits, options.keep_extras, &lines, &mut sink)?;
    let project::Built {
        project,
        recorded,
        save_version,
        dropped_computed,
    } = project::build(&raw, options, &mut sink);
    let mut all_dropped = raw.dropped_computed.clone();
    for (name, n) in dropped_computed {
        let total = all_dropped.entry(name).or_insert(0);
        *total = total.saturating_add(n);
    }
    let source = SourceInfo {
        save_version,
        namespace: raw.namespace.clone(),
        ui_sections: raw.ui_sections.clone(),
        skipped_elements: raw.skipped_elements,
        dropped_computed: all_dropped,
        not_preserved: raw.not_preserved,
        dropped_diagnostics: sink.dropped,
    };
    let mut diagnostics = sink.finish();
    diagnostics.extend(
        project
            .validate()
            .finish()
            .into_iter()
            .map(ImportDiagnostic::core),
    );
    Ok(MspdiDocument {
        project,
        recorded,
        source,
        diagnostics,
    })
}
