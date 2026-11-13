//! Octets MSPDI → session d'édition prête à afficher (lecture + calcul),
//! sans egui.
//!
//! Le document est une [`Session`] de `plannercraft_automation` (ADR-0012 D11) :
//! toute édition passe par un lot de commandes du registre, recalculé par
//! le moteur. Toute date affichée vient de `plannercraft_core::schedule`
//! (règle 3).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use plannercraft_automation::text::Language as TextLanguage;
use plannercraft_automation::{DocOrigin as SessionOrigin, Session};
use plannercraft_core::{CalendarSet, CompiledCalendar, Limits, Moment, Project};
use plannercraft_mspdi::{ImportDiagnostic, MspdiError};

use crate::timescale::{ViewSpan, default_range};

/// Résultat de [`load`] : devient un `OpenDocument` quand l'application lui
/// attribue une origine et un numéro d'ouverture.
#[derive(Clone, Debug)]
pub struct LoadedDocument {
    /// Session d'édition : modèle lu, planning calculé à l'ouverture,
    /// historique vide.
    pub session: Session,
    /// Diagnostics d'import (ceux du calcul sont dans `schedule`).
    pub import_diagnostics: Vec<ImportDiagnostic>,
    /// Plage de jours affichée par l'échelle de temps.
    pub range: ViewSpan,
    /// La plage a été tronquée à 20 ans (projet trop long) : l'application
    /// le signale par un message traduit.
    pub range_truncated: bool,
    /// Fichier du navigateur d'où vient le document (version web, File
    /// System Access, lot W1, H-UI-204) : « Enregistrer » le réécrit.
    /// `None` ailleurs.
    pub web_file: Option<crate::io::WebFile>,
}

/// Échec d'ouverture d'un document.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// Lecture des octets impossible (fichier, téléversement).
    #[error("lecture impossible : {0}")]
    Read(String),
    /// Document MSPDI structurellement inexploitable.
    #[error(transparent)]
    Mspdi(#[from] MspdiError),
}

/// Taille maximale d'un document ouvert : la borne du lecteur MSPDI
/// (`plannercraft_mspdi::Limits::max_bytes`, 256 Mio), contrôlée **avant** la
/// lecture des octets (relecture finale de J8 : un fichier déposé de
/// plusieurs Go ne doit pas être alloué en entier, wasm compris).
#[must_use]
pub fn max_open_bytes() -> u64 {
    plannercraft_mspdi::Limits::default().max_bytes
}

/// Refus d'un document de `size` octets au-delà de [`max_open_bytes`]
/// (`mspdi.error.too_large`, comme le lecteur).
///
/// # Errors
///
/// [`OpenError::Mspdi`] (`MspdiError::TooLarge`).
pub fn check_size(size: u64) -> Result<(), OpenError> {
    let max = max_open_bytes();
    if size > max {
        return Err(OpenError::Mspdi(MspdiError::TooLarge { size, max }));
    }
    Ok(())
}

/// Comme [`check_size`] pour la taille d'un `Blob` du navigateur (`f64`,
/// entière) ; une valeur non finie ou négative est refusée.
///
/// # Errors
///
/// [`OpenError::Mspdi`] (`MspdiError::TooLarge`).
pub fn check_blob_size(size: f64) -> Result<(), OpenError> {
    let size = if size.is_finite() && size >= 0.0 {
        format!("{size:.0}").parse::<u64>().unwrap_or(u64::MAX)
    } else {
        u64::MAX
    };
    check_size(size)
}

/// Lit le fichier `path` sans dépasser [`max_open_bytes`] : taille connue
/// refusée avant toute lecture, lecture bornée sinon (fichier qui grandit,
/// taille inconnue).
///
/// # Errors
///
/// [`OpenError::Read`] (fichier illisible) ; [`OpenError::Mspdi`] au-delà
/// de la borne.
pub fn read_path(path: &std::path::Path) -> Result<Vec<u8>, OpenError> {
    use std::io::Read as _;
    // Taille lue sur les métadonnées, avant même l'ouverture.
    if let Ok(meta) = std::fs::metadata(path) {
        check_size(meta.len())?;
    }
    let file = std::fs::File::open(path).map_err(|e| OpenError::Read(e.to_string()))?;
    let max = max_open_bytes();
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| OpenError::Read(e.to_string()))?;
    check_size(u64::try_from(bytes.len()).unwrap_or(u64::MAX))?;
    Ok(bytes)
}

/// Lit un document MSPDI et ouvre sa session (planning calculé).
///
/// `now` est la date courante injectée (`ReadOptions::current_date`) : aucune
/// horloge n'est lue ici. L'origine de la session est posée par
/// l'application à l'installation ; sa langue (français) ne sert pas à
/// l'interface, qui passe toujours la sienne (messages d'erreur, lecture des
/// saisies).
///
/// # Errors
///
/// [`OpenError::Mspdi`] si le lecteur MSPDI refuse le document ; tout le
/// reste devient un diagnostic.
pub fn load(bytes: &[u8], now: Moment) -> Result<LoadedDocument, OpenError> {
    let origin = SessionOrigin {
        name: String::new(),
        path: None,
    };
    let (session, import_diagnostics) =
        Session::open_bytes(bytes, origin, now, TextLanguage::Fr).map_err(|error| match error {
            plannercraft_automation::OpenError::Mspdi(error) => OpenError::Mspdi(error),
        })?;
    let range = default_range(session.project(), session.schedule());
    Ok(LoadedDocument {
        session,
        import_diagnostics,
        range: range.span,
        range_truncated: range.truncated,
        web_file: None,
    })
}

/// Ouvre la session d'un projet déjà construit (N8, J7 lot L14) : planning
/// calculé par le moteur, historique vide, aucun diagnostic d'import. Même
/// plage affichée que [`load`].
#[must_use]
pub fn load_project(project: Project, now: Moment) -> LoadedDocument {
    let origin = SessionOrigin {
        name: String::new(),
        path: None,
    };
    let session = Session::from_project(project, origin, now, TextLanguage::Fr);
    let range = default_range(session.project(), session.schedule());
    LoadedDocument {
        session,
        import_diagnostics: Vec::new(),
        range: range.span,
        range_truncated: range.truncated,
        web_file: None,
    }
}

/// Nom affiché d'un fichier : dernier composant de `path` (le chemin entier
/// s'il n'en a pas).
#[must_use]
pub fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |name| name.to_string_lossy().into_owned())
}

/// Calendriers compilés d'un document (N3, J7 lot L7) : compilés à la
/// première demande, **une fois par génération** de la session, avec le
/// budget du moteur (`Limits::compiled_segments`, R-172 amendée) ; partagés
/// par `Arc` (aucune copie par image).
///
/// Le cache vit dans l'`OpenDocument` et se lit par `&self` (le contexte
/// d'image ne prête le document qu'en lecture) : verrou court, jamais tenu
/// pendant le dessin ; un verrou empoisonné est repris tel quel (règle 10).
#[derive(Debug)]
pub struct CalendarCache {
    /// Ensemble compilé et génération qu'il reflète.
    slot: Mutex<Option<(u64, Arc<CalendarSet>)>>,
    /// Nombre de compilations faites (tests : une par génération).
    compiles: AtomicU64,
    /// Budget de compilation, en segments gardés.
    limit: usize,
}

impl CalendarCache {
    /// Cache vide au budget du moteur.
    #[must_use]
    pub fn new() -> Self {
        Self::with_limit(Limits::default().compiled_segments)
    }

    /// Cache vide au budget `limit` (tests du refus).
    #[must_use]
    pub const fn with_limit(limit: usize) -> Self {
        Self {
            slot: Mutex::new(None),
            compiles: AtomicU64::new(0),
            limit,
        }
    }

    /// Calendriers de `project` à la génération `generation` : ceux du
    /// cache s'il reflète cette génération, sinon compilés maintenant (une
    /// fois) et gardés. Ne panique jamais : un calendrier en échec ou hors
    /// budget est dans l'ensemble comme une erreur (`CalendarSet::get`).
    pub fn get(&self, project: &Project, generation: u64) -> Arc<CalendarSet> {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((cached, set)) = slot.as_ref()
            && *cached == generation
        {
            return Arc::clone(set);
        }
        let (set, _) = CalendarSet::compile_with(project, self.limit);
        self.compiles.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = set.get(project.calendar) {
            // Une fois par génération (le résultat est gardé) ; le moteur, de
            // même budget, a déjà émis son diagnostic (panneau des
            // diagnostics).
            log::warn!("jours chômés non grisés : calendrier du projet indisponible ({error})");
        }
        let set = Arc::new(set);
        *slot = Some((generation, Arc::clone(&set)));
        set
    }

    /// Nombre de compilations faites depuis la création du cache.
    #[must_use]
    pub fn compiles(&self) -> u64 {
        self.compiles.load(Ordering::Relaxed)
    }
}

impl Default for CalendarCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CalendarCache {
    /// Copie vide, au même budget : la copie recompilera à sa première
    /// demande (un document cloné peut ensuite diverger).
    fn clone(&self) -> Self {
        Self::with_limit(self.limit)
    }
}

/// Calendrier du projet compilé, si sa compilation a réussi (repli : aucun
/// grisé).
#[must_use]
pub fn project_calendar<'a>(set: &'a CalendarSet, project: &Project) -> Option<&'a CompiledCalendar> {
    set.get(project.calendar).ok()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "tests : un échec doit s'arrêter net et lisiblement"
    )]

    use super::{CalendarCache, OpenError, file_name, load, load_project, project_calendar};

    fn now() -> plannercraft_core::Moment {
        "2026-10-17T08:00".parse().unwrap()
    }

    #[test]
    fn document_load_demo() {
        let doc = load(crate::DEMO_MSPDI, now()).unwrap();
        let (project, schedule) = (doc.session.project(), doc.session.schedule());
        assert_eq!(project.tasks.len(), 49);
        assert!(doc.import_diagnostics.is_empty(), "{:?}", doc.import_diagnostics);
        assert!(schedule.diagnostics.is_empty(), "{:?}", schedule.diagnostics);
        assert_eq!(schedule.project_finish, "2027-05-17T17:00".parse().unwrap());
        let links: usize = project.tasks.iter().map(|t| t.predecessors.len()).sum();
        assert_eq!(links, 46);
        assert!(doc.range.start <= schedule.project_start.day());
        assert!(doc.range.end > schedule.project_finish.day());
        assert!(!doc.range_truncated);
        assert!(!doc.session.is_dirty());
        assert_eq!(doc.session.generation(), 0);
    }

    #[test]
    fn document_load_garbage_is_error() {
        let Err(err) = load(b"pas du xml", now()) else {
            panic!("un document illisible doit être refusé");
        };
        assert!(matches!(err, OpenError::Mspdi(_)));
        assert_ne!(err.to_string(), "");
        let read = OpenError::Read("accès refusé".to_owned());
        assert_eq!(read.to_string(), "lecture impossible : accès refusé");
    }

    /// N3 : une compilation par génération ; le refus du budget laisse
    /// l'ensemble sans calendrier du projet, sans panique.
    #[test]
    fn n3_calendar_cache_once_per_generation() {
        let doc = load(crate::DEMO_MSPDI, now()).unwrap();
        let project = doc.session.project();
        let cache = CalendarCache::new();
        let first = cache.get(project, 0);
        let again = cache.get(project, 0);
        assert!(std::sync::Arc::ptr_eq(&first, &again));
        assert_eq!(cache.compiles(), 1);
        assert!(project_calendar(&first, project).is_some());
        let next = cache.get(project, 1);
        assert_eq!(cache.compiles(), 2);
        assert!(!std::sync::Arc::ptr_eq(&first, &next));
        // Le clone repart vide.
        assert_eq!(cache.clone().compiles(), 0);
        let tiny = CalendarCache::with_limit(1);
        let set = tiny.get(project, 0);
        assert!(project_calendar(&set, project).is_none());
        tiny.get(project, 0);
        assert_eq!(tiny.compiles(), 1);
    }

    /// N8 : un projet construit s'ouvre sans MSPDI, même planning que la
    /// session du moteur, rien de modifié.
    #[test]
    fn n8_load_project_direct() {
        let read = load(crate::DEMO_MSPDI, now()).unwrap();
        let project = read.session.project().clone();
        let doc = load_project(project, now());
        assert_eq!(doc.session.project().tasks.len(), 49);
        assert_eq!(
            doc.session.schedule().project_finish,
            read.session.schedule().project_finish
        );
        assert_eq!(doc.import_diagnostics.len(), 0);
        assert!(!doc.session.is_dirty());
        assert_eq!(doc.session.generation(), 0);
        assert_eq!(doc.range, read.range);
    }

    #[test]
    fn file_name_of_path() {
        assert_eq!(file_name("/tmp/plans/maison.xml"), "maison.xml");
        assert_eq!(file_name("maison.xml"), "maison.xml");
        assert_eq!(file_name("/"), "/");
    }
}
