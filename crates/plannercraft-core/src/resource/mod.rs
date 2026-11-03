//! Ressources et affectations : règles d'édition et utilisation par période
//! (ADR-0013 D3, D6 ; contrat J5, lot L0).
//!
//! - [`edit`] : types de tâche et pilotage par l'effort, fonctions pures
//!   appelées par les seules commandes (`plannercraft-automation`) ; le recalcul
//!   ne fait que placer (R-189 à R-194, lot L2) ;
//! - [`usage`] : travail par période et par ressource, calculé à la demande
//!   depuis un `Schedule` (R-188, lot L5).
//!
//! Le placement des affectations vit dans le moteur (`schedule`), pas ici.

pub mod edit;
pub mod usage;

pub use edit::{Edit, EditError, Part, Rebalanced, Recalculated, TaskSide, apply};
pub use usage::{
    AssignmentUsageReport, AssignmentUsageRow, Granularity, MAX_ASSIGNMENT_ROWS, UsageError, UsageReport,
    UsageRow, usage, usage_by_assignment, usage_with,
};
