//! Lignes de base calculées (J6, ADR-0014 D6, D7 ; règles R-220 à R-225).
//! Contrat J6 (lot L0) ; règles remplies par le lot L4.
//!
//! Les lignes stockées (11 par entité, champs optionnels) vivent dans le
//! modèle (`model::baseline`) ; ce module les calcule à partir du moteur :
//! - [`capture`] : copie des valeurs du `Schedule` (début, fin, durée
//!   calculée, travail) sur une portée, avec cumul vers les récapitulatives
//!   (R-220 à R-223) ; [`apply`] l'écrit dans le projet ;
//! - [`variance`] : écarts signés en minutes ouvrées sur l'échelle de
//!   mesure de chaque tâche (R-224, R-225).

mod capture;
mod measure;
mod variance;

pub use capture::{BaselineEdit, BaselineError, BaselineSpec, Rollup, Scope, apply, capture};
pub use variance::{TaskVariance, Variance, Variances, variance};
