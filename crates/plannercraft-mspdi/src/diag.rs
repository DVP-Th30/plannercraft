//! Diagnostics d'import MSPDI (ADR-0009, A20).
//!
//! Le cœur ignore les chemins XML : l'import a son propre type,
//! [`ImportDiagnostic`], qui porte l'élément en cause, sa position et la
//! valeur brute. Les diagnostics de `Project::validate` y sont enveloppés
//! ([`ImportCode::Core`]). Gravité déduite de la variante : avertissement
//! pour une valeur corrigée (défaut appliqué, arrondi, doublon ignoré),
//! erreur pour une information manquante ou une référence rompue.

use std::fmt;

use plannercraft_core::{DiagArg, Diagnostic, Severity, Subject};

use crate::error::{LineIndex, Location};

/// Longueur maximale (en caractères) de la valeur brute citée.
const MAX_VALUE_CHARS: usize = 200;

/// Nombre maximal de diagnostics d'import gardés (règle 10 : un document
/// hostile ne fait pas croître la mémoire sans borne) ; au-delà, seul leur
/// nombre est gardé ([`ImportCode::Truncated`]).
pub const MAX_DIAGNOSTICS: usize = 10_000;

/// Nombre maximal de diagnostics gardés par code : une avalanche d'un code
/// n'en masque pas d'autres.
pub const MAX_DIAGNOSTICS_PER_CODE: u32 = 1_000;

/// Raison pour laquelle un lien est ignoré à l'import (R-074).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LinkIgnoredReason {
    /// Lien vers un autre projet (`CrossProject = 1`).
    CrossProject,
    /// Lien d'une tâche vers elle-même.
    SelfLink,
    /// `Type` de lien inconnu.
    UnknownType,
}

impl LinkIgnoredReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::CrossProject => "cross_project",
            Self::SelfLink => "self_link",
            Self::UnknownType => "unknown_type",
        }
    }
}

/// Nature d'un diagnostic d'import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportCode {
    /// Valeur illisible ou hors bornes : défaut appliqué.
    InvalidValue,
    /// Code numérique inconnu : défaut appliqué.
    UnknownCode {
        /// Code lu.
        code: i64,
    },
    /// Élément obligatoire absent (UID…).
    MissingRequired,
    /// Élément, UID ou lien en double : le premier gagne.
    Duplicate,
    /// Référence vers un UID inexistant.
    DanglingReference,
    /// Lien ignoré.
    LinkIgnored {
        /// Raison.
        reason: LinkIgnoredReason,
    },
    /// Secondes, durée ou décalage arrondis à la minute.
    Rounded,
    /// Fonction non prise en charge (`LevelingDelay`…). La planification à
    /// rebours (`schedule_from_finish`) ne l'est plus depuis J3 (R-089).
    Unsupported {
        /// Fonction, en code stable (`leveling_delay`…).
        what: String,
    },
    /// Incohérence tolérée (drapeau `Summary` contraire à la hiérarchie,
    /// `ID` ≠ position, encodage déclaré, entité inconnue, fuseau ignoré…).
    Inconsistent,
    /// Diagnostics supprimés au-delà des plafonds ([`MAX_DIAGNOSTICS`],
    /// [`MAX_DIAGNOSTICS_PER_CODE`]) : un seul, en fin de liste.
    Truncated {
        /// Nombre de diagnostics supprimés.
        dropped: u64,
    },
    /// Diagnostic de `Project::validate`, enveloppé.
    Core(Diagnostic),
    /// Enfant non lu d'un élément qui n'a pas d'`Extras` (jour de
    /// calendrier, exception, semaine datée, plage, lien) : perdu à
    /// l'aller-retour, compté (`SourceInfo::not_preserved`, R-087).
    NotPreserved,
    /// `HonorConstraints` à 0 alors qu'une tâche automatique active a une
    /// contrainte datée : les liens l'emportent sur les contraintes (R-089,
    /// R-099). Info, une fois.
    HonorConstraintsOff,
    /// Restes relus (`RemainingDuration`, `RemainingWork`) différents de la
    /// dérivation (R-200, R-211) : remplacés (J6, R-214, lot L5). Info, une
    /// fois par fichier.
    ProgressInconsistent {
        /// Valeurs relues remplacées.
        count: u32,
    },
    /// Instruction `<?plannercraft-style?>` ignorée (H-R12, J8) : trop grande,
    /// seconde, après la racine, ou contenu invalide ; le document s'ouvre
    /// avec le modèle par défaut.
    StyleIgnored {
        /// Raison.
        reason: crate::style_pi::StyleIgnoredReason,
    },
    /// Valeur illisible d'un champ personnalisé modélisé (nombre, date,
    /// indicateur, texte de plus de 255 caractères, `Value` absente) :
    /// gardée telle quelle en `Extras` (R-265). Info.
    CustomUnreadable,
}

impl ImportCode {
    /// Code stable : `mspdi.invalid_value`… ou `core.<code du cœur>`.
    #[must_use]
    pub fn code(&self) -> String {
        match self {
            Self::InvalidValue => "mspdi.invalid_value".to_owned(),
            Self::UnknownCode { .. } => "mspdi.unknown_code".to_owned(),
            Self::MissingRequired => "mspdi.missing_required".to_owned(),
            Self::Duplicate => "mspdi.duplicate".to_owned(),
            Self::DanglingReference => "mspdi.dangling_reference".to_owned(),
            Self::LinkIgnored { .. } => "mspdi.link_ignored".to_owned(),
            Self::Rounded => "mspdi.rounded".to_owned(),
            Self::Unsupported { .. } => "mspdi.unsupported".to_owned(),
            Self::Inconsistent => "mspdi.inconsistent".to_owned(),
            Self::Truncated { .. } => "mspdi.truncated".to_owned(),
            Self::Core(d) => format!("core.{}", d.code()),
            Self::NotPreserved => "mspdi.not_preserved".to_owned(),
            Self::HonorConstraintsOff => "mspdi.honor_constraints_off".to_owned(),
            Self::ProgressInconsistent { .. } => "mspdi.progress_inconsistent".to_owned(),
            Self::StyleIgnored { .. } => "style.ignored".to_owned(),
            Self::CustomUnreadable => "mspdi.custom_unreadable".to_owned(),
        }
    }

    /// Rang de la variante, pour le plafond par code.
    const fn rank(&self) -> usize {
        match self {
            Self::InvalidValue => 0,
            Self::UnknownCode { .. } => 1,
            Self::MissingRequired => 2,
            Self::Duplicate => 3,
            Self::DanglingReference => 4,
            Self::LinkIgnored { .. } => 5,
            Self::Rounded => 6,
            Self::Unsupported { .. } => 7,
            Self::Inconsistent => 8,
            Self::Truncated { .. } => 9,
            Self::Core(_) => 10,
            Self::NotPreserved => 11,
            Self::HonorConstraintsOff => 12,
            Self::ProgressInconsistent { .. } => 13,
            Self::StyleIgnored { .. } => 14,
            Self::CustomUnreadable => 15,
        }
    }

    /// Gravité, déduite de la variante.
    #[must_use]
    pub fn severity(&self) -> Severity {
        match self {
            Self::InvalidValue
            | Self::UnknownCode { .. }
            | Self::Duplicate
            | Self::Rounded
            | Self::Inconsistent
            | Self::Truncated { .. }
            | Self::LinkIgnored { .. }
            | Self::Unsupported { .. }
            | Self::StyleIgnored { .. } => Severity::Warning,
            Self::MissingRequired | Self::DanglingReference => Severity::Error,
            Self::NotPreserved
            | Self::HonorConstraintsOff
            | Self::ProgressInconsistent { .. }
            | Self::CustomUnreadable => Severity::Info,
            Self::Core(d) => d.severity(),
        }
    }
}

/// Diagnostic d'import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDiagnostic {
    /// Nature.
    pub code: ImportCode,
    /// Gravité (déduite de `code`).
    pub severity: Severity,
    /// Élément du projet concerné, s'il est connu.
    pub subject: Option<Subject>,
    /// Position dans l'entrée ; `None` pour un diagnostic du cœur enveloppé.
    pub at: Option<Location>,
    /// Chemin de l'élément XML (« Tasks/Task/PredecessorLink/LinkLag ») ;
    /// vide pour un diagnostic du cœur.
    pub element: String,
    /// Valeur brute, tronquée à 200 caractères.
    pub value: Option<String>,
}

impl ImportDiagnostic {
    /// Diagnostic d'import ; gravité déduite du code, valeur tronquée.
    #[must_use]
    pub fn new(
        code: ImportCode,
        subject: Option<Subject>,
        at: Option<Location>,
        element: &str,
        value: Option<&str>,
    ) -> Self {
        Self {
            severity: code.severity(),
            code,
            subject,
            at,
            element: element.to_owned(),
            value: value.map(|v| v.chars().take(MAX_VALUE_CHARS).collect()),
        }
    }

    /// Diagnostic du cœur enveloppé.
    #[must_use]
    pub fn core(d: Diagnostic) -> Self {
        Self {
            severity: d.severity(),
            subject: Some(d.subject()),
            code: ImportCode::Core(d),
            at: None,
            element: String::new(),
            value: None,
        }
    }
}

impl fmt::Display for ImportDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let ImportCode::Core(d) = &self.code {
            return write!(f, "{d}");
        }
        f.write_str(&self.code.code())?;
        match &self.code {
            ImportCode::UnknownCode { code } => write!(f, " ({code})")?,
            ImportCode::LinkIgnored { reason } => write!(f, " ({})", reason.as_str())?,
            ImportCode::Unsupported { what } => write!(f, " ({what})")?,
            ImportCode::Truncated { dropped } => write!(f, " ({dropped} diagnostics supprimés)")?,
            ImportCode::ProgressInconsistent { count } => write!(f, " ({count} restes relus remplacés)")?,
            ImportCode::StyleIgnored { reason } => write!(f, " ({})", reason.as_str())?,
            _ => {}
        }
        if !self.element.is_empty() {
            write!(f, " : {}", self.element)?;
        }
        if let Some(v) = &self.value {
            write!(f, " = « {v} »")?;
        }
        if let Some(s) = &self.subject {
            write!(f, ", {s}")?;
        }
        if let Some(at) = &self.at {
            write!(f, ", {at}")?;
        }
        Ok(())
    }
}

/// Nombre de rangs de [`ImportCode`] (plafond par code).
const RANKS: usize = 16;

/// Collecteur de diagnostics, avec l'index des lignes de l'entrée ; borné
/// par [`MAX_DIAGNOSTICS`] et [`MAX_DIAGNOSTICS_PER_CODE`].
pub(crate) struct Sink<'a> {
    lines: &'a LineIndex<'a>,
    pub(crate) out: Vec<ImportDiagnostic>,
    /// Diagnostics gardés, par rang de code.
    per_code: [u32; RANKS],
    /// Diagnostics supprimés.
    pub(crate) dropped: u64,
}

impl<'a> Sink<'a> {
    pub(crate) const fn new(lines: &'a LineIndex<'a>) -> Self {
        Self {
            lines,
            out: Vec::new(),
            per_code: [0; RANKS],
            dropped: 0,
        }
    }

    /// Ajoute un diagnostic sur l'élément commençant à l'octet `at` ; au-delà
    /// des plafonds, il est seulement compté.
    pub(crate) fn push(
        &mut self,
        code: ImportCode,
        subject: Option<Subject>,
        at: u64,
        element: &str,
        value: Option<&str>,
    ) {
        let Some(kept) = self.per_code.get_mut(code.rank()) else {
            return;
        };
        if self.out.len() >= MAX_DIAGNOSTICS || *kept >= MAX_DIAGNOSTICS_PER_CODE {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        *kept = kept.saturating_add(1);
        let at = self.lines.locate(at);
        self.out
            .push(ImportDiagnostic::new(code, subject, Some(at), element, value));
    }

    /// Diagnostics gardés, suivis de [`ImportCode::Truncated`] si des
    /// diagnostics ont été supprimés.
    pub(crate) fn finish(mut self) -> Vec<ImportDiagnostic> {
        if self.dropped > 0 {
            self.out.push(ImportDiagnostic::new(
                ImportCode::Truncated {
                    dropped: self.dropped,
                },
                None,
                None,
                "",
                None,
            ));
        }
        self.out
    }
}

/// Traduction par code (N5, J8 lot L14) : arguments neutres, en plus du
/// `Display` français qui ne change pas.
impl ImportCode {
    /// Arguments nommés propres au code (sans les champs communs de
    /// [`ImportDiagnostic`] : élément, valeur, sujet, position). Pour
    /// [`ImportCode::Core`], ceux du diagnostic du cœur.
    #[must_use]
    pub fn args(&self) -> Vec<(&'static str, DiagArg)> {
        match self {
            Self::UnknownCode { code } => vec![("code", DiagArg::Int(*code))],
            Self::LinkIgnored { reason } => vec![("reason", DiagArg::Text(reason.as_str().to_owned()))],
            Self::Unsupported { what } => vec![("what", DiagArg::Text(what.clone()))],
            Self::Truncated { dropped } => vec![("dropped", DiagArg::count64(*dropped))],
            Self::ProgressInconsistent { count } => vec![("count", DiagArg::Int(i64::from(*count)))],
            Self::StyleIgnored { reason } => vec![("reason", DiagArg::Text(reason.as_str().to_owned()))],
            Self::Core(d) => d.args(),
            Self::InvalidValue
            | Self::MissingRequired
            | Self::Duplicate
            | Self::DanglingReference
            | Self::Rounded
            | Self::Inconsistent
            | Self::NotPreserved
            | Self::HonorConstraintsOff
            | Self::CustomUnreadable => Vec::new(),
        }
    }

    /// Un exemple de chaque variante hors [`ImportCode::Core`] (couverte par
    /// `Diagnostic::examples`), pour les contrôles de couverture des
    /// traductions.
    #[must_use]
    pub fn examples() -> Vec<Self> {
        vec![
            Self::InvalidValue,
            Self::UnknownCode { code: 7 },
            Self::MissingRequired,
            Self::Duplicate,
            Self::DanglingReference,
            Self::LinkIgnored {
                reason: LinkIgnoredReason::CrossProject,
            },
            Self::Rounded,
            Self::Unsupported {
                what: "leveling_delay".to_owned(),
            },
            Self::Inconsistent,
            Self::Truncated { dropped: 3 },
            Self::NotPreserved,
            Self::HonorConstraintsOff,
            Self::ProgressInconsistent { count: 2 },
            Self::StyleIgnored {
                reason: crate::style_pi::StyleIgnoredReason::TooLarge,
            },
            Self::CustomUnreadable,
        ]
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::{Diagnostic, Severity, Subject, TaskUid};

    use super::{DiagArg, ImportCode, ImportDiagnostic, LinkIgnoredReason};

    #[test]
    fn codes_and_severities() {
        let cases = [
            (ImportCode::InvalidValue, "mspdi.invalid_value", Severity::Warning),
            (
                ImportCode::UnknownCode { code: 99 },
                "mspdi.unknown_code",
                Severity::Warning,
            ),
            (
                ImportCode::MissingRequired,
                "mspdi.missing_required",
                Severity::Error,
            ),
            (ImportCode::Duplicate, "mspdi.duplicate", Severity::Warning),
            (
                ImportCode::DanglingReference,
                "mspdi.dangling_reference",
                Severity::Error,
            ),
            (
                ImportCode::LinkIgnored {
                    reason: LinkIgnoredReason::CrossProject,
                },
                "mspdi.link_ignored",
                Severity::Warning,
            ),
            (ImportCode::Rounded, "mspdi.rounded", Severity::Warning),
            (
                ImportCode::Unsupported {
                    what: "leveling_delay".to_owned(),
                },
                "mspdi.unsupported",
                Severity::Warning,
            ),
            (ImportCode::Inconsistent, "mspdi.inconsistent", Severity::Warning),
            (
                ImportCode::Truncated { dropped: 5 },
                "mspdi.truncated",
                Severity::Warning,
            ),
            (ImportCode::NotPreserved, "mspdi.not_preserved", Severity::Info),
            (
                ImportCode::ProgressInconsistent { count: 2 },
                "mspdi.progress_inconsistent",
                Severity::Info,
            ),
        ];
        for (code, text, severity) in cases {
            assert_eq!(code.code(), text);
            assert_eq!(code.severity(), severity);
        }
        let core = Diagnostic::LinkSelf {
            task: TaskUid::new(3),
        };
        let d = ImportDiagnostic::core(core.clone());
        assert_eq!(d.code.code(), "core.link.self");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(
            d.subject,
            Some(Subject::Link {
                predecessor: TaskUid::new(3),
                successor: TaskUid::new(3)
            })
        );
        assert_eq!(d.to_string(), core.to_string());
    }

    #[test]
    fn value_truncated_and_display() {
        let long = "x".repeat(500);
        let d = ImportDiagnostic::new(
            ImportCode::UnknownCode { code: 42 },
            Some(Subject::Task(TaskUid::new(7))),
            None,
            "Tasks/Task/Type",
            Some(&long),
        );
        assert_eq!(d.value.as_ref().unwrap().chars().count(), 200);
        let text = d.to_string();
        assert!(
            text.starts_with("mspdi.unknown_code (42) : Tasks/Task/Type"),
            "{text}"
        );
    }

    /// Un exemple par variante non enveloppée, codes distincts, arguments
    /// nommés distincts (N5).
    #[test]
    fn n5_import_examples_cover_every_variant() {
        let examples = ImportCode::examples();
        let ranks: Vec<usize> = examples.iter().map(ImportCode::rank).collect();
        let mut expected: Vec<usize> = (0..16).collect();
        expected.retain(|r| *r != ImportCode::Core(plannercraft_core::Diagnostic::TaskReservedUid).rank());
        assert_eq!(ranks, expected);
        let codes: std::collections::BTreeSet<String> = examples.iter().map(ImportCode::code).collect();
        assert_eq!(codes.len(), examples.len());
        for code in &examples {
            let args = code.args();
            let names: std::collections::BTreeSet<&str> = args.iter().map(|(n, _)| *n).collect();
            assert_eq!(names.len(), args.len());
        }
        let core = ImportCode::Core(plannercraft_core::Diagnostic::LinkSelf {
            task: plannercraft_core::TaskUid::new(3),
        });
        assert_eq!(core.args(), vec![("task", DiagArg::Uid(3))]);
    }
}
