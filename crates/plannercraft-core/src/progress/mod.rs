//! Avancement (J6, ADR-0014 D2 à D4 ; règles R-200 à R-213).
//!
//! Fonctions pures, sans planning ni projet complet :
//! - [`facts`] : lecture de l'avancement saisi d'une tâche (état, durée
//!   réelle effective, restes et % dérivés ; R-200, R-211). Une seule
//!   dérivation, partagée par le moteur, l'écrivain MSPDI, `normalize` et les
//!   vues (ADR-0014 D4 : vérité = réels, restes dérivés) ;
//! - [`edit`] : équilibrage d'une saisie (% ↔ durée réelle ↔ reste, début
//!   réel implicite, fin réelle à 100 %), appelé par `progress.update`
//!   (D3 : le recalcul place, l'édition équilibre ; lot L2).
//!
//! Pourcentages en 1/10 000 (10 000 = 100 %), durées en minutes ouvrées,
//! divisions par `round_div` (R-001) en `i128`.

pub mod edit;
mod facts;

pub use facts::{
    ProgressFacts, ProgressState, facts, percent_of_duration, remaining_duration, remaining_work,
};
