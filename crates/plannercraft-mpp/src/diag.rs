//! Diagnostics de la lecture : codes stables `mpp.*`, une valeur au plus
//! par diagnostic, nombre borné (règle 10).

use plannercraft_core::{Diagnostic, Severity};
use serde::Serialize;

/// Nature d'un diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MppCode {
    /// Champ d'une tâche hors de l'enregistrement : défaut appliqué.
    FieldMissing,
    /// Date hors du domaine du moteur (1984-2149) ou heure invalide :
    /// ignorée.
    DateInvalid,
    /// Durée négative ou trop grande : 0.
    DurationInvalid,
    /// Code de format de durée inconnu : unité par défaut du projet.
    FormatUnknown,
    /// Valeur hors bornes (niveau, priorité, % achevé, type) : défaut.
    ValueInvalid,
    /// UID en double : le premier enregistrement gagne.
    DuplicateUid,
    /// ID enregistrés non contigus ou en double : tâches renumérotées dans
    /// l'ordre des ID enregistrés (R-284).
    IdRenumbered,
    /// Lien vers une tâche absente, vide ou vers elle-même : ignoré.
    LinkIgnored,
    /// Lien en double : le premier gagne.
    LinkDuplicate,
    /// Décalage en pourcentage : lien gardé, décalage nul (format non lu).
    LagPercentUnread,
    /// Type de lien ou de contrainte inconnu : défaut.
    CodeUnknown,
    /// Contrainte datée sans date : « Dès que possible ».
    ConstraintWithoutDate,
    /// Plage horaire d'un calendrier invalide : ignorée.
    CalendarRangeInvalid,
    /// Exception d'un type de récurrence inconnu ou aux paramètres hors
    /// bornes : ignorée.
    CalendarExceptionUnknownType,
    /// Exception invalide ou en chevauchement : ignorée.
    CalendarExceptionInvalid,
    /// Calendrier du projet introuvable par son nom : premier calendrier
    /// de base.
    ProjectCalendarFallback,
    /// Calendrier d'une tâche absent : calendrier du projet.
    TaskCalendarUnknown,
    /// Propriété du projet absente ou invalide : défaut.
    PropertyMissing,
    /// Notes tronquées (64 Kio).
    NotesTruncated,
    /// Le mode de planification (manuel ou automatique) n'est pas lu : les
    /// tâches sont importées en automatique (R-290). Information, une fois.
    ModeNotRead,
    /// Affectation à une tâche absente, ou ressource inconnue : ignorée
    /// (tâche) ou gardée sans ressource.
    AssignmentIgnored,
    /// Valeur d'une ressource ou d'une affectation illisible (unités,
    /// travail) : défaut.
    ResourceValueInvalid,
    /// Calendrier d'une ressource absent : calendrier du projet.
    ResourceCalendarUnknown,
    /// Calendrier dérivé dont la base est absente : ignoré.
    CalendarBaseUnknown,
    /// Profil d'affectation modifié à la main : la répartition saisie
    /// n'est pas reprise (le moteur place le travail comme un profil
    /// uniforme) ; dates non recalculables à l'identique.
    AssignmentContoured,
    /// Retard d'affectation non lu dans cette disposition (Project 2010) ;
    /// information, une fois.
    AssignmentDelayNotRead,
    /// Diagnostic de `Project::validate`.
    Core,
}

impl MppCode {
    /// Code stable (`mpp.field_missing`…).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::FieldMissing => "mpp.field_missing",
            Self::DateInvalid => "mpp.date_invalid",
            Self::DurationInvalid => "mpp.duration_invalid",
            Self::FormatUnknown => "mpp.format_unknown",
            Self::ValueInvalid => "mpp.value_invalid",
            Self::DuplicateUid => "mpp.duplicate_uid",
            Self::IdRenumbered => "mpp.id_renumbered",
            Self::LinkIgnored => "mpp.link_ignored",
            Self::LinkDuplicate => "mpp.link_duplicate",
            Self::LagPercentUnread => "mpp.lag_percent_unread",
            Self::CodeUnknown => "mpp.code_unknown",
            Self::ConstraintWithoutDate => "mpp.constraint_without_date",
            Self::CalendarRangeInvalid => "mpp.calendar_range_invalid",
            Self::CalendarExceptionUnknownType => "mpp.calendar_exception_unknown_type",
            Self::CalendarExceptionInvalid => "mpp.calendar_exception_invalid",
            Self::ProjectCalendarFallback => "mpp.project_calendar_fallback",
            Self::TaskCalendarUnknown => "mpp.task_calendar_unknown",
            Self::PropertyMissing => "mpp.property_missing",
            Self::NotesTruncated => "mpp.notes_truncated",
            Self::ModeNotRead => "mpp.mode_not_read",
            Self::AssignmentIgnored => "mpp.assignment_ignored",
            Self::ResourceValueInvalid => "mpp.resource_value_invalid",
            Self::ResourceCalendarUnknown => "mpp.resource_calendar_unknown",
            Self::CalendarBaseUnknown => "mpp.calendar_base_unknown",
            Self::AssignmentContoured => "mpp.assignment_contoured",
            Self::AssignmentDelayNotRead => "mpp.assignment_delay_not_read",
            Self::Core => "mpp.core",
        }
    }

    /// Gravité.
    #[must_use]
    pub const fn severity(self) -> Severity {
        match self {
            Self::IdRenumbered
            | Self::ModeNotRead
            | Self::NotesTruncated
            | Self::AssignmentDelayNotRead
            | Self::Core => Severity::Info,
            _ => Severity::Warning,
        }
    }
}

/// Diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MppDiagnostic {
    /// Nature.
    pub code: MppCode,
    /// Gravité.
    pub severity: Severity,
    /// UID de la tâche ou du calendrier concerné.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    /// Valeur ou détail (borné à 200 caractères).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Diagnostic du cœur enveloppé (code du cœur).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core: Option<&'static str>,
}

impl MppDiagnostic {
    /// Diagnostic du cœur enveloppé ; sa gravité est gardée.
    #[must_use]
    pub fn core(d: &Diagnostic) -> Self {
        Self {
            code: MppCode::Core,
            severity: d.severity(),
            uid: None,
            value: None,
            core: Some(d.code()),
        }
    }
}

/// Collecteur borné.
#[derive(Debug)]
pub struct Sink {
    /// Diagnostics gardés.
    pub items: Vec<MppDiagnostic>,
    /// Diagnostics supprimés au-delà de la borne.
    pub dropped: u64,
    /// Diagnostics d'incohérence d'un enregistrement (date ou durée
    /// invalide, code inconnu, valeur hors bornes, UID en double), comptés
    /// même au-delà de la borne : garde de R-292.
    pub inconsistencies: u64,
    max: usize,
}

impl Sink {
    /// Collecteur de `max` diagnostics au plus.
    #[must_use]
    pub const fn new(max: usize) -> Self {
        Self {
            items: Vec::new(),
            dropped: 0,
            inconsistencies: 0,
            max,
        }
    }

    /// Ajoute un diagnostic.
    pub fn push(&mut self, code: MppCode, uid: Option<u32>, value: Option<String>) {
        if matches!(
            code,
            MppCode::DateInvalid
                | MppCode::DurationInvalid
                | MppCode::FormatUnknown
                | MppCode::ValueInvalid
                | MppCode::CodeUnknown
                | MppCode::DuplicateUid
                | MppCode::FieldMissing
        ) {
            self.inconsistencies = self.inconsistencies.saturating_add(1);
        }
        if self.items.len() >= self.max {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        let value = value.map(|v| v.chars().take(200).collect());
        self.items.push(MppDiagnostic {
            code,
            severity: code.severity(),
            uid,
            value,
            core: None,
        });
    }
}
