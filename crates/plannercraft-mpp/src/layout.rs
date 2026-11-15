//! Décalages des champs dans les enregistrements, par disposition (R-282).
//!
//! Chaque décalage est un fait **déduit** de `docs/formats/mpp.md` (§ 7 à
//! § 9) : constant sur tous les fichiers MPP14 du corpus de la disposition,
//! avec le nombre de tâches où il a été vérifié contre le corrigé. Un champ
//! absent de cette table n'est pas lu.

use crate::detect::Layout;

/// Décalages d'un enregistrement de tâche (`TBkndTask/FixedData`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskLayout {
    /// Longueur de l'enregistrement complet.
    pub len: usize,
    /// UID (`u32`).
    pub uid: usize,
    /// ID (`u32`).
    pub id: usize,
    /// Début enregistré (horodatage).
    pub start: usize,
    /// Fin enregistrée (horodatage).
    pub finish: usize,
    /// Durée (`i32`, dixièmes de minute).
    pub duration: usize,
    /// Format de la durée (`u16`, codes MSPDI `DurationFormat`).
    pub duration_format: usize,
    /// Niveau hiérarchique (`u16`).
    pub outline_level: usize,
    /// Type de contrainte (`u16`, codes MSPDI `ConstraintType`).
    pub constraint_type: usize,
    /// Date de contrainte (horodatage).
    pub constraint_date: usize,
    /// Priorité (`u16`).
    pub priority: usize,
    /// % achevé (`u16`).
    pub percent_complete: usize,
    /// Type de tâche (`u16`, codes MSPDI `Type`).
    pub task_type: usize,
    /// Calendrier de la tâche (`i32`, −1 = aucun).
    pub calendar: usize,
    /// Date de création (horodatage).
    pub created: usize,
    /// Échéance (horodatage).
    pub deadline: usize,
    /// Début réel (horodatage) ; `None` : non établi pour cette disposition.
    pub actual_start: Option<usize>,
    /// Fin réelle (horodatage).
    pub actual_finish: Option<usize>,
    /// Durée réelle (`i32`, dixièmes de minute).
    pub actual_duration: Option<usize>,
    /// Reprise du reste d'une tâche commencée (horodatage).
    pub resume: usize,
    /// Retard de nivellement (`i32`, dixièmes de minute) et son format
    /// (`u16`, codes MSPDI).
    pub leveling_delay: (usize, usize),
    /// Jalon : octet de l'élément de `FixedMeta` et masque.
    pub milestone: (usize, u8),
    /// Planification manuelle : octet de l'élément de `Fixed2Meta` et
    /// masque.
    pub manual: (usize, u8),
    /// Active : octet de l'élément de `Fixed2Meta` et masque.
    pub active: (usize, u8),
}

/// Décalages de l'enregistrement `TBkndTask/Fixed2Data` (64 octets, même
/// rang que l'enregistrement fixe ; les deux dispositions).
pub mod task2 {
    /// Début saisi d'une tâche manuelle (horodatage).
    pub const MANUAL_START: usize = 50;
    /// Fin saisie d'une tâche manuelle (horodatage).
    pub const MANUAL_FINISH: usize = 54;
    /// Durée saisie d'une tâche manuelle (`i32`, dixièmes de minute).
    pub const MANUAL_DURATION: usize = 58;
}

/// Disposition des tâches.
#[must_use]
pub const fn task(layout: Layout) -> TaskLayout {
    match layout {
        Layout::Project2010 => TaskLayout {
            len: 206,
            uid: 0,
            id: 4,
            start: 106,
            finish: 8,
            duration: 42,
            duration_format: 46,
            outline_level: 40,
            constraint_type: 56,
            constraint_date: 80,
            priority: 88,
            percent_complete: 90,
            task_type: 94,
            calendar: 118,
            created: 98,
            deadline: 122,
            actual_start: Some(72),
            actual_finish: Some(76),
            actual_duration: Some(48),
            leveling_delay: (58, 62),
            resume: 20,
            milestone: (8, 0x20),
            manual: (8, 0x08),
            active: (8, 0x04),
        },
        Layout::Project2013 => TaskLayout {
            len: 202,
            uid: 4,
            id: 0,
            start: 104,
            finish: 108,
            duration: 84,
            duration_format: 164,
            outline_level: 172,
            constraint_type: 64,
            constraint_date: 66,
            priority: 78,
            percent_complete: 92,
            task_type: 140,
            calendar: 178,
            created: 128,
            deadline: 182,
            actual_start: Some(120),
            actual_finish: Some(124),
            actual_duration: Some(80),
            leveling_delay: (70, 162),
            resume: 132,
            milestone: (10, 0x02),
            manual: (8, 0x80),
            active: (8, 0x40),
        },
    }
}

/// Décalages d'un enregistrement de lien (`TBkndCons/FixedData`, 20
/// octets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkLayout {
    /// UID du prédécesseur (`u32`).
    pub predecessor: usize,
    /// UID du successeur (`u32`).
    pub successor: usize,
    /// Type (`u16`, codes MSPDI).
    pub kind: usize,
    /// Décalage (`i32`, dixièmes de minute).
    pub lag: usize,
    /// Format du décalage (`u16`, codes MSPDI).
    pub lag_format: usize,
}

/// Longueur d'un enregistrement de lien.
pub const LINK_LEN: usize = 20;

/// Disposition des liens.
#[must_use]
pub const fn link(layout: Layout) -> LinkLayout {
    match layout {
        Layout::Project2010 => LinkLayout {
            predecessor: 4,
            successor: 8,
            kind: 12,
            lag: 16,
            lag_format: 14,
        },
        Layout::Project2013 => LinkLayout {
            predecessor: 4,
            successor: 8,
            kind: 12,
            lag: 14,
            lag_format: 18,
        },
    }
}

/// Décalages d'un enregistrement de calendrier (`TBkndCal/FixedData`, 12
/// octets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarLayout {
    /// UID (`u32`).
    pub uid: usize,
    /// UID du calendrier de base (`i32`) ; nul ou négatif : calendrier de
    /// base.
    pub base: usize,
}

/// Longueur d'un enregistrement de calendrier.
pub const CALENDAR_LEN: usize = 12;

/// Disposition des calendriers.
#[must_use]
pub const fn calendar(layout: Layout) -> CalendarLayout {
    match layout {
        Layout::Project2010 => CalendarLayout { uid: 0, base: 4 },
        Layout::Project2013 => CalendarLayout { uid: 8, base: 0 },
    }
}

/// Clés des valeurs variables d'une tâche (`TBkndTask/VarMeta`).
pub mod task_var {
    /// Nom (UTF-16, terminé par un nul).
    pub const NAME: u32 = 0x0B40_000E;
    /// Notes (RTF).
    pub const NOTES: u32 = 0x0B40_000F;
}

/// Clés des valeurs variables d'un calendrier (`TBkndCal/VarMeta`).
pub mod calendar_var {
    /// Nom (UTF-16).
    pub const NAME: u32 = 0x0D40_0001;
    /// Semaine et exceptions.
    pub const DATA: u32 = 0x0D40_0008;
}

/// Clés des propriétés du projet (`   114/Props`).
pub mod project_prop {
    /// Début du projet (horodatage).
    pub const START: u32 = 0x0240_0002;
    /// Fin du projet (horodatage).
    pub const FINISH: u32 = 0x0240_0003;
    /// Planifier depuis le début (`u16`, 1 = oui, 0 = depuis la fin) ;
    /// `0x02400019`, qui lui ressemblait sur le corpus de test, porte « la
    /// tâche met à jour la ressource » (discriminé par un fichier réel).
    pub const FROM_START: u32 = 0x0240_0004;
    /// Titre (UTF-16).
    pub const TITLE: u32 = 0x0240_0008;
    /// Nom du calendrier du projet (UTF-16).
    pub const CALENDAR_NAME: u32 = 0x0240_000E;
    /// Format de durée par défaut (`u16`, codes MSPDI).
    pub const DURATION_FORMAT: u32 = 0x0240_0015;
    /// Heure de début par défaut (`u16`, dixièmes de minute).
    pub const DEFAULT_START: u32 = 0x0240_001C;
    /// Heure de fin par défaut (`u16`, dixièmes de minute).
    pub const DEFAULT_FINISH: u32 = 0x0240_0021;
    /// Minutes par jour (`u32`).
    pub const MINUTES_PER_DAY: u32 = 0x0240_001D;
    /// Minutes par semaine (`u32`).
    pub const MINUTES_PER_WEEK: u32 = 0x0240_001E;
    /// Semaine par défaut (7 jours de 60 octets, dimanche d'abord) : jours
    /// « par défaut » des calendriers.
    pub const DEFAULT_WEEK: u32 = 0x0240_1388;
    /// Date d'état (horodatage).
    pub const STATUS_DATE: u32 = 0x0240_0045;
    /// Premier jour de la semaine (`u16`, codes MSPDI `WeekStartDay`).
    pub const WEEK_START: u32 = 0x0240_0025;
    /// Jours par mois (`u16`).
    pub const DAYS_PER_MONTH: u32 = 0x0240_138F;
}
