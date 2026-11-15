//! Planning ouvert depuis un fichier MS Project (`.mpp`, `.mpt` ; lot M3,
//! ADR-0038), sans egui : ce que le bandeau de compte rendu et la fenêtre
//! « Différences avec MS Project » affichent.
//!
//! Le fichier est lu par `plannercraft-mpp` ; les dates enregistrées par MS
//! Project restent à part (jamais dans le projet, règle 3) et sont
//! comparées au planning calculé par le moteur (`plannercraft_mpp::compare`,
//! R-291, R-298). L'interface ne calcule aucune date : elle lit la
//! comparaison et le planning de la session.
//!
//! La comparaison n'a de sens que pour le planning tel qu'ouvert : elle est
//! tenue à jour tant que le projet est celui du fichier, ou celui qu'a
//! laissé « Garder les dates de MS Project » (R-299) ; après toute autre
//! modification, le compte rendu des différences se retire (il revient si
//! l'utilisateur annule jusqu'au planning ouvert).

use std::collections::BTreeSet;
use std::sync::Arc;

use plannercraft_automation::Session;
use plannercraft_automation::helpers::{self, HelperError};
use plannercraft_core::{Moment, Project, TaskOutcome, TaskRole, TaskUid};
use plannercraft_mpp::compare::{Comparison, GapCause, GapClass, compare};
use plannercraft_mpp::{MppCode, MppDiagnostic, Recorded, SourceInfo};

/// Extension d'un nom de fichier, en minuscules (`None` sans point).
fn extension(name: &str) -> Option<String> {
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

/// Le nom désigne un fichier MS Project (`.mpp`, `.mpt`), casse ignorée.
#[must_use]
pub fn has_project_extension(name: &str) -> bool {
    matches!(extension(name).as_deref(), Some("mpp" | "mpt"))
}

/// Le nom désigne un gabarit MS Project (`.mpt`), casse ignorée.
#[must_use]
pub fn is_template_name(name: &str) -> bool {
    extension(name).as_deref() == Some("mpt")
}

/// Nom du planning ouvert depuis le fichier `name` : sans chemin ni
/// extension `.mpp` ou `.mpt` (« Planning.mpp » ⇒ « Planning »), pour que
/// « Enregistrer » propose « Planning.xml » et jamais le fichier d'origine.
#[must_use]
pub fn document_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if has_project_extension(base)
        && let Some((stem, _)) = base.rsplit_once('.')
    {
        return stem.to_owned();
    }
    base.to_owned()
}

/// Les octets sont-ils à lire comme un fichier MS Project ? Reconnaissance
/// par le **contenu** : signature d'un fichier composé ; un fichier nommé
/// `.mpp` ou `.mpt` qui n'est ni composé ni XML aussi (le lecteur dira
/// clairement pourquoi il le refuse) ; un XML reste un XML, quel que soit
/// son nom.
#[must_use]
pub fn is_project_file(bytes: &[u8], name: &str) -> bool {
    if plannercraft_mpp::cfb::is_compound(bytes) {
        return true;
    }
    if !has_project_extension(name) {
        return false;
    }
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let first = text.iter().find(|b| !b.is_ascii_whitespace());
    // XML en UTF-8 (« < ») ou en UTF-16 (BOM) : lecteur MSPDI.
    !matches!(first, Some(b'<' | 0xFF | 0xFE))
}

/// Ce que le fichier contient et qui n'est pas repris, quand ce lecteur sait
/// le constater pour ce fichier (comptes de la lecture, diagnostics).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NotTaken {
    /// Affectations dont la répartition du travail a été modifiée à la
    /// main (`mpp.assignment_contoured`).
    pub contoured: usize,
    /// Liens à décalage en pourcentage, gardés sans décalage
    /// (`mpp.lag_percent_unread`).
    pub lag_percent: usize,
    /// Calendriers de ressource dont la base manque.
    pub resource_calendars: u32,
}

impl NotTaken {
    /// Rien à signaler.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.contoured == 0 && self.lag_percent == 0 && self.resource_calendars == 0
    }
}

/// Une tâche en écart réel avec MS Project (R-298), telle qu'affichée par la
/// fenêtre « Différences avec MS Project ».
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffRow {
    /// Tâche.
    pub uid: TaskUid,
    /// Numéro (position dans le planning).
    pub id: u32,
    /// Nom.
    pub name: String,
    /// Récapitulative (ses dates suivent ses sous-tâches : jamais gardées).
    pub summary: bool,
    /// Début et fin enregistrés par MS Project.
    pub recorded: (Option<Moment>, Option<Moment>),
    /// Début et fin calculés par le moteur.
    pub computed: (Option<Moment>, Option<Moment>),
    /// Cause non recalculable constatée, s'il y en a une.
    pub cause: Option<GapCause>,
}

/// Planning ouvert depuis un fichier MS Project : dates enregistrées,
/// comparaison au calcul, état du compte rendu.
#[derive(Clone, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "drapeaux indépendants du compte rendu"
)]
pub struct MppOpened {
    /// Dates enregistrées par MS Project (jamais dans le projet).
    recorded: Arc<Recorded>,
    /// Projet tel qu'ouvert.
    original: Arc<Project>,
    /// Projet laissé par « Garder les dates de MS Project ».
    kept: Option<Arc<Project>>,
    /// Comptes du compte rendu : tâches (lignes vides exclues) et liens.
    pub tasks: usize,
    /// Liens.
    pub links: usize,
    /// Non repris, constaté pour ce fichier.
    pub not_taken: NotTaken,
    /// Gabarit (`.mpt`) : ouvert comme un nouveau planning.
    pub template: bool,
    /// Le planning avait des tâches en écart réel à l'ouverture.
    had_gaps: bool,
    /// Comparaison du planning courant (tant qu'il est comparable).
    comparison: Comparison,
    /// Génération de la session comparée.
    generation: u64,
    /// Le planning est celui du fichier, ou celui laissé par « Garder les
    /// dates » : la comparaison vaut.
    pristine: bool,
    /// Bandeau fermé par l'utilisateur (il ne revient pas).
    pub banner_closed: bool,
}

impl MppOpened {
    /// État d'un fichier lu : dates enregistrées `recorded`, provenance
    /// `source` et diagnostics de la lecture ; son projet a été remis à
    /// `session`.
    #[must_use]
    pub fn new(
        recorded: Recorded,
        source: &SourceInfo,
        diagnostics: &[MppDiagnostic],
        session: &Session,
        template: bool,
    ) -> Self {
        let project = session.project();
        let comparison = compare(project, session.schedule(), &recorded);
        let count = |code: MppCode| diagnostics.iter().filter(|d| d.code == code).count();
        Self {
            recorded: Arc::new(recorded),
            original: Arc::new(project.clone()),
            kept: None,
            tasks: project.tasks.iter().filter(|t| !t.is_null).count(),
            links: project.tasks.iter().map(|t| t.predecessors.len()).sum(),
            not_taken: NotTaken {
                contoured: count(MppCode::AssignmentContoured),
                lag_percent: count(MppCode::LagPercentUnread),
                resource_calendars: source.not_imported.resource_calendars,
            },
            template,
            had_gaps: comparison.tasks_off > 0,
            comparison,
            generation: session.generation(),
            pristine: true,
            banner_closed: false,
        }
    }

    /// Suit la session après un changement (édition, annulation,
    /// rétablissement) : comparaison relue si le planning est encore
    /// comparable.
    pub fn refresh(&mut self, session: &Session) {
        let project = session.project();
        self.pristine =
            project == self.original.as_ref() || self.kept.as_deref().is_some_and(|kept| kept == project);
        if self.pristine && self.generation != session.generation() {
            self.comparison = compare(project, session.schedule(), &self.recorded);
            self.generation = session.generation();
        }
    }

    /// Note le planning laissé par « Garder les dates de MS Project ».
    pub fn mark_kept(&mut self, session: &Session) {
        self.kept = Some(Arc::new(session.project().clone()));
        self.refresh(session);
    }

    /// La comparaison vaut pour le planning courant.
    #[must_use]
    pub const fn comparable(&self) -> bool {
        self.pristine
    }

    /// Tâches en écart réel (au-delà de deux minutes), 0 si le planning
    /// n'est plus comparable.
    #[must_use]
    pub const fn real_gaps(&self) -> u32 {
        if self.pristine {
            self.comparison.tasks_off
        } else {
            0
        }
    }

    /// Tâches à une ou deux minutes près (arrondi, non comptées).
    #[must_use]
    pub const fn rounding(&self) -> u32 {
        if self.pristine {
            self.comparison.tasks_rounding
        } else {
            0
        }
    }

    /// La fenêtre « Différences avec MS Project » peut s'ouvrir : le
    /// planning avait des écarts à l'ouverture et n'a pas été modifié
    /// autrement que par « Garder les dates ».
    #[must_use]
    pub const fn differences_available(&self) -> bool {
        self.pristine && self.had_gaps
    }

    /// « Garder les dates de MS Project » a été appliqué au planning
    /// courant.
    #[must_use]
    pub fn kept_applied(&self) -> bool {
        self.pristine && self.kept.is_some() && self.comparison.tasks_off == 0
    }

    /// Lignes de la fenêtre : tâches en écart réel, dans l'ordre du
    /// planning ; vide si le planning n'est plus comparable.
    #[must_use]
    pub fn rows(&self, session: &Session) -> Vec<DiffRow> {
        if !self.pristine {
            return Vec::new();
        }
        let project = session.project();
        let schedule = session.schedule();
        let mut seen = BTreeSet::new();
        let mut rows = Vec::new();
        for gap in self.comparison.gaps.iter().filter(|g| g.class == GapClass::Real) {
            let uid = TaskUid::new(gap.uid);
            if !seen.insert(uid) {
                continue;
            }
            let Some(task) = project.tasks.get(uid) else {
                continue;
            };
            let (computed, summary) = match schedule.get(uid) {
                Some(TaskOutcome::Scheduled(s)) => {
                    ((Some(s.start), Some(s.finish)), s.role == TaskRole::Summary)
                }
                _ => ((None, None), false),
            };
            let recorded = self
                .recorded
                .tasks
                .get(&uid)
                .map_or((None, None), |r| (r.start, r.finish));
            rows.push(DiffRow {
                uid,
                id: gap.id,
                name: task.name.clone(),
                summary,
                recorded,
                computed,
                cause: gap.cause,
            });
        }
        rows
    }

    /// Lot de « Garder les dates de MS Project » (R-299) : tâches choisies
    /// par `plannercraft_mpp::kept_dates`, posées par
    /// `helpers::keep_dates` ; `None` s'il n'y a rien à garder.
    ///
    /// # Errors
    ///
    /// [`HelperError`] : tâche inconnue (jamais attendu : les tâches
    /// viennent du projet).
    pub fn keep_batch(
        &self,
        session: &Session,
    ) -> Result<Option<plannercraft_automation::Batch>, HelperError> {
        if !self.pristine {
            return Ok(None);
        }
        let kept = plannercraft_mpp::kept_dates(session.project(), &self.recorded);
        let dates: Vec<(TaskUid, Moment, Moment)> = kept.iter().map(|k| (k.uid, k.start, k.finish)).collect();
        helpers::keep_dates(session, &dates)
    }
}

#[cfg(test)]
mod tests {
    use super::{document_name, has_project_extension, is_project_file, is_template_name};

    #[test]
    fn names_and_detection() {
        assert_eq!(document_name("Planning.mpp"), "Planning");
        assert_eq!(document_name("/plans/Chantier B.MPT"), "Chantier B");
        assert_eq!(document_name("C:\\plans\\a.mpp"), "a");
        assert_eq!(document_name("maison.xml"), "maison.xml");
        assert!(has_project_extension("a.MPP") && is_template_name("b.mpt") && !is_template_name("b.mpp"));
        let compound = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0];
        assert!(is_project_file(&compound, "sans-extension"));
        assert!(is_project_file(b"PK\x03\x04", "faux.mpp"));
        assert!(!is_project_file(b"\xEF\xBB\xBF <Project/>", "renomme.mpp"));
        assert!(!is_project_file(b"<Project/>", "maison.xml"));
        assert!(!is_project_file(b"pas du xml", "maison.xml"));
    }
}
