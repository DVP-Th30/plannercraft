//! Verdict de parité contre les planchers (ADR-0004, ADR-0009 § 9.7).
//!
//! Fonction pure [`decide`], testée sur des résultats synthétiques :
//! 1. `CORPUS_FLOOR` = somme des `floor` du manifeste, sinon échec ;
//! 2. tout fichier présent : identiques ≥ `floor`, sinon échec (régression
//!    nommée) ; erreur de lecture d'un fichier présent avec `floor > 0` :
//!    échec ;
//! 3. global : Σ identiques (présents) ≥ `CORPUS_FLOOR` − Σ `floor`
//!    (absents) : un absent ne compte ni pour ni contre ;
//! 4. `PLANNERCRAFT_CORPUS=required`, manifeste non vide et tous les fichiers
//!    absents : échec ; sinon un absent n'est qu'un avertissement ;
//! 5. identiques > `floor` : « plancher relevable », succès ;
//! 6. fichier enregistré par MS Project (`producer = msproject`) : tout cas
//!    compare les champs de base (neuf, dix pour une récapitulative), sinon
//!    échec (comparaison incomplète : le plancher serait tenu à vide).

use serde::Serialize;

/// Raison de l'absence d'un fichier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum AbsentReason {
    /// Pas dans `tests/corpus/files/` (`cargo xtask corpus fetch`).
    NotDownloaded,
    /// Empreinte différente du manifeste : jamais comparé.
    HashMismatch {
        /// Empreinte constatée.
        actual: String,
    },
}

/// État d'un fichier pour le verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FileStatus {
    /// Absent (ni pour ni contre).
    Absent(AbsentReason),
    /// Présent mais illisible : 0 identique.
    ReadError(String),
    /// Lu, recalculé, comparé.
    Compared {
        /// Cas comparés.
        cases: u32,
        /// Cas identiques.
        identical: u32,
        /// Comparaisons incomplètes (champ de base absent, ou aucune valeur
        /// comparable).
        incomplete: u32,
    },
}

/// Résultat d'un fichier, réduit à ce que le verdict lit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileResult {
    /// Nom du manifeste.
    pub name: String,
    /// Plancher du fichier.
    pub floor: u32,
    /// Fichier enregistré par MS Project : la comparaison doit être complète.
    pub complete_required: bool,
    /// État.
    pub status: FileStatus,
}

impl FileResult {
    /// Présent (lu ou en erreur de lecture).
    #[must_use]
    pub fn is_present(&self) -> bool {
        !matches!(self.status, FileStatus::Absent(_))
    }

    /// Cas identiques (0 si absent ou illisible).
    #[must_use]
    pub fn identical(&self) -> u32 {
        match self.status {
            FileStatus::Compared { identical, .. } => identical,
            FileStatus::Absent(_) | FileStatus::ReadError(_) => 0,
        }
    }
}

/// Verdict.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Verdict {
    /// Échecs : le test échoue s'il y en a un.
    pub failures: Vec<String>,
    /// Avertissements (fichier absent, erreur de lecture sans plancher).
    pub warnings: Vec<String>,
    /// Planchers relevables.
    pub raisable: Vec<String>,
}

impl Verdict {
    /// Aucun échec.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Applique les règles du verdict.
#[must_use]
pub fn decide(results: &[FileResult], corpus_floor: u32, required: bool) -> Verdict {
    let mut v = Verdict::default();
    let floor_sum = results
        .iter()
        .fold(0_u64, |acc, r| acc.saturating_add(u64::from(r.floor)));
    if floor_sum != u64::from(corpus_floor) {
        v.failures.push(format!(
            "CORPUS_FLOOR = {corpus_floor}, mais la somme des `floor` du manifeste vaut {floor_sum}"
        ));
    }
    let mut identical_present = 0_u64;
    let mut floor_absent = 0_u64;
    for r in results {
        match &r.status {
            FileStatus::Absent(reason) => {
                floor_absent = floor_absent.saturating_add(u64::from(r.floor));
                let why = match reason {
                    AbsentReason::NotDownloaded => "non téléchargé (cargo xtask corpus fetch)".to_owned(),
                    AbsentReason::HashMismatch { actual } => {
                        format!("empreinte différente du manifeste ({actual})")
                    }
                };
                v.warnings.push(format!("{} : absent, {why}", r.name));
            }
            FileStatus::ReadError(e) => {
                let msg = format!("{} : erreur de lecture ({e})", r.name);
                if r.floor > 0 {
                    v.failures
                        .push(format!("{msg}, plancher {} non atteint", r.floor));
                } else {
                    v.warnings.push(msg);
                }
            }
            FileStatus::Compared {
                cases,
                identical,
                incomplete,
            } => {
                if r.complete_required && *incomplete > 0 {
                    v.failures.push(format!(
                        "{} : comparaison incomplète, {incomplete} cas sans tous les champs de base \
                         (régression du lecteur ?)",
                        r.name
                    ));
                }
                identical_present = identical_present.saturating_add(u64::from(*identical));
                if *identical < r.floor {
                    v.failures.push(format!(
                        "{} : régression, {identical} cas identiques sur {cases} < plancher {}",
                        r.name, r.floor
                    ));
                } else if *identical > r.floor {
                    v.raisable.push(format!(
                        "{} : plancher relevable, {identical} identiques > plancher {}",
                        r.name, r.floor
                    ));
                }
            }
        }
    }
    let required_global = u64::from(corpus_floor).saturating_sub(floor_absent);
    if identical_present < required_global {
        v.failures.push(format!(
            "parité globale : {identical_present} cas identiques < {required_global} (CORPUS_FLOOR {corpus_floor} − planchers des absents {floor_absent})"
        ));
    }
    let all_absent = !results.is_empty() && results.iter().all(|r| !r.is_present());
    if all_absent {
        let msg = "aucun fichier du corpus n'est présent (cargo xtask corpus fetch)".to_owned();
        if required {
            v.failures.push(format!("PLANNERCRAFT_CORPUS=required : {msg}"));
        } else {
            v.warnings.push(msg);
        }
    }
    v
}
