//! Contraintes de date (ADR-0010, spécification J3 § 3.4 à 3.6, R-090 à
//! R-105).
//!
//! Une table `ConstraintRule` est dérivée une fois par nœud à la
//! construction du réseau, selon le type de la contrainte, le sens de
//! planification et l'option « les tâches respectent toujours leurs dates
//! de contrainte » ([`derive`], R-099). Les passes restent génériques : elles
//! ne lisent que la règle et l'ancrage normalisé de la date ([`anchor`],
//! R-090), jamais le type.

use crate::calendar::{Span, TimeScale};
use crate::duration::Minutes;
use crate::error::CalendarError;
use crate::model::ConstraintKind;
use crate::time::Moment;

use super::Calc;
use super::network::{Fail, NodeKind, ix};

/// Sens de planification (ADR-0010) : depuis le début, passe avant
/// principale ; à rebours, passe arrière principale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// Depuis la date de début du projet.
    FromStart,
    /// À rebours depuis la date de fin du projet.
    FromFinish,
}

/// Rôle d'une passe (ADR-0010) : la passe principale place les tâches
/// (passe avant depuis le début), la passe secondaire calcule les dates
/// opposées et les marges (passe arrière depuis le début).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PassRole {
    /// Passe principale.
    Primary,
    /// Passe secondaire.
    Secondary,
    /// Repasse de la passe principale après l'épingle des tâches à
    /// placement opposé (R-092) : épingles tenues pour fixes, aucun
    /// diagnostic sauf un invariant violé.
    Replay,
}

/// Côté de la date d'une contrainte : début (MSO, SNET, SNLT) ou fin (MFO,
/// FNET, FNLT).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    /// Date de début.
    Start,
    /// Date de fin.
    Finish,
}

/// Effet d'une contrainte dans les passes (ADR-0010, table de la
/// spécification J3 § 3.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConstraintRule {
    /// Aucun effet (ASAP depuis le début).
    None,
    /// ALAP depuis le début, ASAP à rebours : épinglée après la passe
    /// secondaire (R-092, R-108).
    Opposite,
    /// Plancher de la passe avant (SNET, FNET).
    Floor(Side),
    /// Plafond de la passe arrière (SNLT, FNLT).
    Cap(Side),
    /// MSO, MFO, option inactive : plancher et plafond.
    Window(Side),
    /// Option active : la borne du côté secondaire s'impose aussi dans la
    /// passe principale (SNLT, FNLT depuis le début ; SNET, FNET à rebours).
    Clamped(Side),
    /// MSO, MFO, option active : au plus tôt = au plus tard = ancrage.
    Pin(Side),
    /// Épingle réelle d'une feuille achevée (J6, R-203, ADR-0014 D1) : dates
    /// planifiées, au plus tôt et au plus tard = [début réel, fin réelle]
    /// bruts ; liens entrants, porte, contrainte, retard de nivellement et
    /// ALAP ignorés ; aucun plafond vers ses prédécesseurs. `start` : début
    /// réel saisi, `None` s'il est implicite (R-200 : début au plus tôt du
    /// réseau depuis le début, `backward_span(fin réelle, D)` à rebours).
    Actual {
        /// Début réel saisi.
        start: Option<Moment>,
        /// Fin réelle (au moins le début réel saisi).
        finish: Moment,
    },
    /// Feuille automatique en cours (J6, R-204 à R-207, ADR-0014 D2) :
    /// partie réelle fixe depuis le début réel, reste replacé depuis la
    /// reprise ; la contrainte saisie ne joue plus que comme plancher de la
    /// reprise (SNET, MSO, fractionnement permis), les autres sont ignorées
    /// (`constraint.ignored_in_progress`).
    Progress(InProgress),
}

/// Avancement d'une feuille en cours, figé à la construction du réseau
/// (R-200, R-204, R-205).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InProgress {
    /// Début réel saisi ; `None` : implicite (début au plus tôt du réseau,
    /// R-200).
    pub start: Option<Moment>,
    /// Durée réelle effective (R-200).
    pub actual: Minutes,
    /// Durée restante D − réel (R-200).
    pub remaining: Minutes,
    /// Reprise saisie (`Progress::resume`, R-204).
    pub resume: Option<Moment>,
    /// Fractionnement des tâches en cours permis (`split_in_progress`,
    /// R-205).
    pub split: bool,
}

/// R-099 : règle d'une contrainte de type `kind` sur une feuille automatique
/// active, selon le sens et l'option « respecter les contraintes ».
pub(super) const fn derive(kind: ConstraintKind, mode: Mode, honor: bool) -> ConstraintRule {
    use ConstraintKind as K;
    let side = if kind.is_start_side() {
        Side::Start
    } else {
        Side::Finish
    };
    let from_start = matches!(mode, Mode::FromStart);
    match kind {
        K::AsSoonAsPossible if from_start => ConstraintRule::None,
        K::AsLateAsPossible if !from_start => ConstraintRule::None,
        K::AsSoonAsPossible | K::AsLateAsPossible => ConstraintRule::Opposite,
        // Planchers ; serrés dans la passe principale à rebours si l'option
        // est active.
        K::StartNoEarlierThan | K::FinishNoEarlierThan if honor && !from_start => {
            ConstraintRule::Clamped(side)
        }
        K::StartNoEarlierThan | K::FinishNoEarlierThan => ConstraintRule::Floor(side),
        // Plafonds ; serrés dans la passe principale depuis le début si
        // l'option est active.
        K::StartNoLaterThan | K::FinishNoLaterThan if honor && from_start => ConstraintRule::Clamped(side),
        K::StartNoLaterThan | K::FinishNoLaterThan => ConstraintRule::Cap(side),
        K::MustStartOn | K::MustFinishOn if honor => ConstraintRule::Pin(side),
        K::MustStartOn | K::MustFinishOn => ConstraintRule::Window(side),
    }
}

/// R-090 : ancrage `A` d'une contrainte datée sur l'échelle de placement
/// `P` d'une tâche de durée `d`.
/// - type début (MSO, SNET, SNLT) : `A = P.forward_span(date, d)` (S9 : un
///   début saisi à 07:00 sur un calendrier à 08:00 commence à 08:00) ;
/// - type fin (MFO, FNET, FNLT) : `f* = P.backward_span(date, 0).finish`
///   (R-031 en passe arrière : la date si elle commence une minute ouvrée
///   ou tombe dans une plage, sinon l'instant ouvré précédent), puis
///   `A = (P.add(f*, −d), f*)` (R-022) ;
/// - temps écoulé : `(date, date + d)` ou `(date − d, date)`, sans recalage.
///
/// # Errors
///
/// Erreurs de conversion de l'échelle (hors domaine, aucun temps ouvré).
pub(super) fn anchor(
    scale: TimeScale<'_>,
    kind: ConstraintKind,
    date: Moment,
    d: Minutes,
) -> Result<Span, CalendarError> {
    if kind.is_start_side() {
        return scale.forward_span(date, d);
    }
    let finish = scale.backward_span(date, Minutes::ZERO)?.finish;
    let back = d.checked_neg().ok_or(CalendarError::NegativeSpan)?;
    Ok(Span {
        start: scale.add(finish, back)?,
        finish,
    })
}

/// Contrainte d'un nœud, prête pour les passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NodeConstraint {
    /// Type saisi.
    pub kind: ConstraintKind,
    /// Règle dérivée (R-099).
    pub rule: ConstraintRule,
    /// Date saisie, brute ; `None` pour ASAP et ALAP.
    pub date: Option<Moment>,
    /// Ancrage R-090 d'une feuille ; `Err` : conversion impossible (R-069).
    /// `None` : pas d'ancrage (type non daté, porte d'une récapitulative).
    pub anchor: Option<Result<Span, Fail>>,
}

impl NodeConstraint {
    /// Ancrage R-090, ou l'échec de sa conversion.
    pub fn span(&self) -> Result<Span, Fail> {
        self.anchor
            .unwrap_or(Err(Fail::Internal("contrainte sans ancrage")))
    }
}

impl Calc<'_, '_> {
    /// R-092 (R-108 à rebours) : après la passe secondaire, chaque feuille
    /// automatique active de règle `Opposite` (ALAP depuis le début, ASAP à
    /// rebours), non dégradée, de marge totale ≥ 0 (R-048, sur son échelle de
    /// mesure, R-113) et dont les dates au plus tard ne précèdent pas les
    /// dates au plus tôt en brut, est épinglée sur ses dates secondaires (`pin` : au plus tard
    /// depuis le début, au plus tôt à rebours) ; la repasse de la passe
    /// principale les tient pour fixes. Rend vrai si au moins une épingle
    /// déplace une tâche. Une tâche à marge négative n'est pas épinglée
    /// (S8 : une ALAP avec échéance « could still finish after its deadline
    /// if its predecessors slipped »).
    ///
    /// Preuve (depuis le début) : une feuille placée sur ses dates au plus
    /// tard vérifie `g(LF) ≤ LS_s` pour tout successeur (adjoint R-047, P13
    /// (g)) ; les successeurs replacés restent donc au plus tard à leurs
    /// dates au plus tard, inchangées ; la fin du projet ne bouge pas (la
    /// tâche qui la fixe a TF = 0 ; avec R-051, une feuille terminale a
    /// LS = ES). Miroir à rebours.
    pub(super) fn pin_opposite(&mut self) -> bool {
        let mut any = false;
        for k in 0..self.net.nodes.len() {
            let Some(v) = ix(k) else { continue };
            let Some(node) = self.net.node(v) else { continue };
            // Marge mesurée comme R-048 : échelle de mesure (calendrier de
            // base du projet pour une tâche sans calendrier propre, R-113).
            let measure = node.measure;
            let (Some(c), Some(scale)) = (node.constraint, self.net.scale(measure).copied()) else {
                continue;
            };
            if node.kind != NodeKind::Leaf || node.inactive || c.rule != ConstraintRule::Opposite {
                continue;
            }
            let Some(state) = self.state(v).copied() else {
                continue;
            };
            let (Some(early), Some(late)) = (state.early, state.late) else {
                continue;
            };
            if state.degraded || state.late_degraded || state.failed || state.back_failed || early == late {
                continue;
            }
            let tf = match (
                scale.between(early.start, late.start),
                scale.between(early.finish, late.finish),
            ) {
                (Ok(start_slack), Ok(finish_slack)) => start_slack.min(finish_slack),
                _ => continue,
            };
            if tf.is_negative() || late.start < early.start || late.finish < early.finish {
                continue;
            }
            let pin = match self.mode {
                Mode::FromStart => late,
                Mode::FromFinish => early,
            };
            if let Some(st) = self.state_mut(v) {
                st.pin = Some(pin);
                any = true;
            }
        }
        any
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{ConstraintRule as R, Mode, Side, anchor, derive};
    use crate::arena::Arena;
    use crate::calendar::builtin::standard;
    use crate::calendar::compiled::CompiledCalendar;
    use crate::calendar::{Span, TimeScale};
    use crate::duration::Minutes;
    use crate::ids::CalendarUid;
    use crate::model::ConstraintKind as K;
    use crate::time::Moment;

    fn m(s: &str) -> Moment {
        s.parse().unwrap()
    }

    #[test]
    fn derive_table() {
        // Table de la spécification J3 § 3.5 : (type, début actif, début
        // inactif, rebours actif, rebours inactif).
        let table = [
            (K::AsSoonAsPossible, R::None, R::None, R::Opposite, R::Opposite),
            (K::AsLateAsPossible, R::Opposite, R::Opposite, R::None, R::None),
            (
                K::StartNoEarlierThan,
                R::Floor(Side::Start),
                R::Floor(Side::Start),
                R::Clamped(Side::Start),
                R::Floor(Side::Start),
            ),
            (
                K::FinishNoEarlierThan,
                R::Floor(Side::Finish),
                R::Floor(Side::Finish),
                R::Clamped(Side::Finish),
                R::Floor(Side::Finish),
            ),
            (
                K::StartNoLaterThan,
                R::Clamped(Side::Start),
                R::Cap(Side::Start),
                R::Cap(Side::Start),
                R::Cap(Side::Start),
            ),
            (
                K::FinishNoLaterThan,
                R::Clamped(Side::Finish),
                R::Cap(Side::Finish),
                R::Cap(Side::Finish),
                R::Cap(Side::Finish),
            ),
            (
                K::MustStartOn,
                R::Pin(Side::Start),
                R::Window(Side::Start),
                R::Pin(Side::Start),
                R::Window(Side::Start),
            ),
            (
                K::MustFinishOn,
                R::Pin(Side::Finish),
                R::Window(Side::Finish),
                R::Pin(Side::Finish),
                R::Window(Side::Finish),
            ),
        ];
        for (kind, a, b, c, d) in table {
            assert_eq!(derive(kind, Mode::FromStart, true), a, "{kind:?}");
            assert_eq!(derive(kind, Mode::FromStart, false), b, "{kind:?}");
            assert_eq!(derive(kind, Mode::FromFinish, true), c, "{kind:?}");
            assert_eq!(derive(kind, Mode::FromFinish, false), d, "{kind:?}");
        }
    }

    #[test]
    fn anchor_start_and_finish_sides() {
        let a = Arena::try_from(vec![standard(CalendarUid::new(1), "Standard".to_owned())]).unwrap();
        let c = CompiledCalendar::compile(&a, CalendarUid::new(1)).unwrap();
        let w = TimeScale::Working(&c);
        let span = |s: &str, f: &str| Span {
            start: m(s),
            finish: m(f),
        };
        // Début : recalé vers l'avant (S9).
        assert_eq!(
            anchor(w, K::StartNoEarlierThan, m("2026-10-19T07:00"), Minutes::new(480)),
            Ok(span("2026-10-19T08:00", "2026-10-19T17:00"))
        );
        // Fin : début d'une plage gardé (R-031 arrière), puis recul de d.
        assert_eq!(
            anchor(
                w,
                K::FinishNoEarlierThan,
                m("2026-10-26T08:00"),
                Minutes::new(480)
            ),
            Ok(span("2026-10-23T08:00", "2026-10-26T08:00"))
        );
        // Fin en temps chômé : instant ouvré précédent.
        assert_eq!(
            anchor(w, K::MustFinishOn, m("2026-10-24T10:00"), Minutes::new(480)),
            Ok(span("2026-10-23T08:00", "2026-10-23T17:00"))
        );
        // Temps écoulé : aucun recalage.
        assert_eq!(
            anchor(
                TimeScale::Elapsed,
                K::FinishNoEarlierThan,
                m("2026-10-21T10:00"),
                Minutes::new(1_440)
            ),
            Ok(span("2026-10-20T10:00", "2026-10-21T10:00"))
        );
        // Hors domaine : erreur, jamais de panique.
        assert!(anchor(w, K::FinishNoLaterThan, Moment::MIN, Minutes::new(480)).is_err());
        assert!(anchor(w, K::StartNoLaterThan, Moment::MAX, Minutes::new(480)).is_err());
    }
}
