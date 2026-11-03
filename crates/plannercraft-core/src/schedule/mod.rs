//! Moteur de planification : chemin critique (ADR-0009, spécification J2
//! § 3, règles R-035 à R-069), contraintes et planification à rebours
//! (ADR-0010, spécification J3 § 3, règles R-090 à R-112).
//!
//! [`schedule`] est une fonction pure du projet : aucune horloge, aucune
//! entrée/sortie, aucun état gardé entre deux appels. Elle n'échoue jamais et
//! ne panique jamais : toute anomalie devient un diagnostic, et une tâche
//! qu'on ne sait pas placer est exclue avec sa raison (règles 3 et 10).
//!
//! Étapes :
//! 0. calendriers compilés, replis (R-066), table des échelles ;
//! 1. à 4. réseau ([`network`]) : hiérarchie (R-052), classification
//!    (manuelles R-063, inactives R-062), nœuds canoniques, liens effectifs
//!    (R-059), adjacence CSR ;
//! 5. ordre topologique et cycles ([`order`], R-035, R-065) ;
//! 6. depuis le début : passe avant principale ([`forward`], R-036 à
//!    R-045, R-069), passe arrière secondaire ([`backward`], R-046, R-047,
//!    R-051), épingle des tâches ALAP et repasse avant (R-092) ; à rebours :
//!    passe arrière principale depuis l'ancre `F` (R-106, R-111), début
//!    calculé `S*` (R-107), passe avant secondaire, épingle des tâches ASAP
//!    et repasse arrière (R-108) ;
//! 7. marges et critique ([`slack`], R-048 à R-050, R-101), contraintes non
//!    tenues et échéances dépassées (R-099, R-102) ;
//! 8. agrégats des récapitulatives et sortie ([`rollup`], R-112).
//!
//! Les décalages passent tous par [`transfer`] (transfert avant `g`,
//! adjoint `h` en passe arrière, R-047).

mod backward;
mod constraint;
pub mod explain;
mod forward;
mod image;
pub(crate) mod load;
mod network;
mod order;
mod placement;
mod rollup;
mod slack;
mod transfer;
mod types;

use std::collections::BTreeMap;

pub use crate::diagnostic::Pass;
pub use image::{assigned_leaf_duration, placed_by_assignments, sync_assigned_durations};
pub use types::{
    AssignmentRole, AssignmentSchedule, Driver, EffectiveCalendar, Exclusion, LateDriver, MAX_OVERLOADS,
    Overload, ProgressSchedule, ResourceLoad, Schedule, TaskOutcome, TaskRole, TaskSchedule,
};

use crate::arena::Arena;
use crate::calendar::{CalendarKind, CalendarSet, CompiledCalendar, Span, builtin};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::duration::Minutes;
use crate::ids::{CalendarUid, TaskUid};
use crate::model::{Project, ScheduleAnchor, Task};
use crate::time::Moment;
use constraint::{Mode, PassRole};
use network::{Fail, Network, NodeIx, pos};

/// Cause de `link.unschedulable` pour un lien à décalage en % depuis une
/// récapitulative automatique, à rebours (R-110) : sa durée (R-054) dépend
/// des dates au plus tard de ses sous-tâches, que ce lien plafonne.
const PERCENT_LAG_FROM_SUMMARY: &str = "lag.percent_of_summary_from_finish";

/// État calculé d'un nœud canonique.
#[derive(Clone, Copy, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "drapeaux indépendants du calcul d'un nœud (échecs par passe, repli, prédécesseur effectif), lus séparément par les passes"
)]
struct NodeState {
    /// Dates au plus tôt ; porte : `(G, G)` ; agrégat : (min ES, max EF),
    /// ou les dates saisies d'une récapitulative manuelle datée (R-064).
    /// Feuille à retard de nivellement : dates retardées (R-178).
    early: Option<Span>,
    /// Feuille à retard de nivellement : début au plus tôt non retardé,
    /// affiché comme `early.start` (R-178, O27).
    undelayed: Option<Moment>,
    /// Feuille à retard de nivellement : placement sans le retard depuis
    /// toutes ses bornes ; plafonne les dates au plus tard que la feuille
    /// impose à ses prédécesseurs (R-178, A6 UID 21 → 36).
    held: Option<Span>,
    /// Agrégat : (min ES, max EF) des enfants, même pour une récapitulative
    /// manuelle datée (début réel des sous-tâches, lu par la marge libre
    /// d'un lien vers sa porte).
    children: Option<Span>,
    /// Dates au plus tard ; porte : `(LG, LG)` ; agrégat : (min LS, max LF).
    late: Option<Span>,
    /// Cause de l'ES (R-044).
    driver: Driver,
    /// Dates de repli (R-069).
    degraded: bool,
    /// Conversion impossible en passe avant depuis le début (R-069).
    failed: bool,
    /// Conversion impossible en passe arrière principale à rebours
    /// (R-111) : la tâche est exclue, ses liens ignorés.
    back_failed: bool,
    /// Passe avant : le nœud a un prédécesseur effectif (lien appliqué,
    /// propre ou hérité de la porte d'une récapitulative ancêtre) ; miroir
    /// de R-051 à rebours (R-110).
    linked: bool,
    /// Dates au plus tard et marges de repli : elles dépendent d'un
    /// successeur dégradé ou non planifiable (R-069, propagé vers l'amont).
    late_degraded: bool,
    /// Cause de la fin au plus tard (R-047).
    late_driver: LateDriver,
    /// Agrégat : plafond de fin hérité `C(S)` (R-057) et la récapitulative
    /// qui l'a posé.
    cap_finish: Option<(Moment, TaskUid)>,
    /// Agrégat : plafond de début hérité `K(S)` (R-057, A11) et la
    /// récapitulative qui l'a posé.
    cap_start: Option<(Moment, TaskUid)>,
    /// Épingle d'une tâche à placement opposé (ALAP depuis le début,
    /// R-092) : dates fixes de la repasse.
    pin: Option<Span>,
    /// Marge totale (R-048).
    total_slack: Minutes,
    /// Marge libre (R-049).
    free_slack: Minutes,
    /// Feuille achevée (R-203) : dates réelles retenues (début implicite
    /// résolu une fois, par la passe principale).
    actual: Option<Span>,
    /// Borne brute retenue pour le pilote (R-230, partie moteur).
    driver_bound: Option<Moment>,
    /// Feuille en cours (R-204 à R-207) : partie réelle, reprise et fin
    /// placées par la passe principale (début réel implicite résolu une
    /// fois), début au plus tard du reste posé par la passe arrière.
    run: Option<Run>,
}

/// Dates calculées d'une feuille en cours (R-204 à R-207, ADR-0014 D2) :
/// une seule coupure, partie réelle `[actual_start, stop]`, reste placé
/// depuis `resume`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Run {
    /// Début réel (saisi, ou implicite, R-200).
    actual_start: Moment,
    /// Fin de la partie réelle (R-204), jamais stockée dans le modèle.
    stop: Moment,
    /// Début du reste (R-204, R-205).
    resume: Moment,
    /// Fin du reste, fin de la tâche.
    finish: Moment,
    /// Début au plus tard du reste (R-206) : ce que voient ses
    /// prédécesseurs (FS, SS) ; à rebours, la reprise (R-207).
    late_resume: Option<Moment>,
}

impl NodeState {
    const NEW: Self = Self {
        early: None,
        undelayed: None,
        held: None,
        children: None,
        late: None,
        driver: Driver::ProjectStart,
        degraded: false,
        failed: false,
        back_failed: false,
        linked: false,
        late_degraded: false,
        late_driver: LateDriver::ProjectFinish,
        cap_finish: None,
        cap_start: None,
        pin: None,
        total_slack: Minutes::ZERO,
        free_slack: Minutes::ZERO,
        actual: None,
        driver_bound: None,
        run: None,
    };
}

/// Calcul en cours : réseau, états des nœuds, décalages convertis.
struct Calc<'n, 'a> {
    net: &'n Network<'a>,
    project: &'n Project,
    /// Sens de planification.
    mode: Mode,
    /// Implémentation des marges et des % achevés (lot L15).
    strategy: Strategy,
    /// Rôle de la passe en cours.
    role: PassRole,
    /// Repasse (R-092) : aucun diagnostic, sauf `schedule.internal`.
    quiet: bool,
    /// Début du projet : depuis le début, l'ancre brute (R-036) ; à rebours,
    /// le début calculé `S*` (R-107), plancher de la passe avant secondaire.
    start: Moment,
    /// Plafond commun de la passe arrière (R-046) : depuis le début, la fin
    /// du projet (R-045), connue après la passe avant ; à rebours, l'ancre
    /// brute `F` (R-106).
    finish: Moment,
    /// États, par nœud.
    st: Vec<NodeState>,
    /// Décalage converti de chaque arc de lien, calculé une fois (R-043).
    lag: Vec<Option<Minutes>>,
    /// Arcs de lien vers une porte dont la borne n'a pas pu se calculer
    /// (`link.unschedulable`, R-069) : non appliqués, ils ne plafonnent rien
    /// et ne comptent ni pour la marge libre ni pour R-051.
    dropped: Vec<bool>,
    diags: Diagnostics,
}

impl Calc<'_, '_> {
    /// État du nœud `v`.
    fn state(&self, v: NodeIx) -> Option<&NodeState> {
        self.st.get(pos(v)?)
    }

    /// Le nœud `v` peut servir de borne à ses voisins dans la passe
    /// arrière (ADR-0010) : depuis le début (passe secondaire), il a des
    /// dates au plus tôt ; à rebours (passe principale ou repasse), sa passe
    /// arrière principale n'a pas échoué (R-111). Remplace les tests directs
    /// sur `early` de la passe arrière.
    fn reached(&self, v: NodeIx) -> bool {
        match self.mode {
            Mode::FromStart => self.state(v).is_some_and(|s| s.early.is_some()),
            Mode::FromFinish => self.state(v).is_some_and(|s| !s.back_failed),
        }
    }

    /// Le plafond de début hérité `K(parent)` s'applique au nœud de début
    /// au plus tôt `es` (R-057, A11) : depuis le début, aux seuls nœuds de
    /// même début que la récapitulative ; à rebours, à tous, les dates au
    /// plus tôt n'existant pas encore (R-110, ADR-0010).
    fn k_applies(&self, parent_start: Option<Moment>, es: Option<Moment>) -> bool {
        match self.mode {
            Mode::FromStart => es.is_some() && parent_start == es,
            Mode::FromFinish => true,
        }
    }

    /// État modifiable du nœud `v`.
    fn state_mut(&mut self, v: NodeIx) -> Option<&mut NodeState> {
        self.st.get_mut(pos(v)?)
    }

    /// Cible d'arc peu sûre pour la passe arrière et la marge libre (R-069) :
    /// nœud dégradé, en échec ou aux dates au plus tard de repli ; pour une
    /// porte, aussi l'agrégat de ses sous-tâches.
    fn unreliable(&self, v: NodeIx) -> bool {
        let bad = |s: &NodeState| s.degraded || s.failed || s.back_failed || s.late_degraded;
        let Some(s) = self.state(v) else { return true };
        let twin = self
            .net
            .node(v)
            .filter(|n| n.kind == network::NodeKind::Gate)
            .and_then(|n| n.twin)
            .and_then(|r| self.state(r));
        bad(s) || twin.is_some_and(bad)
    }

    /// Le nœud `v` est une feuille achevée (R-203) : ses liens entrants ne
    /// plafonnent pas ses prédécesseurs (passe arrière, marge libre, R-051),
    /// S20 : « it can no longer affect the completion of successor tasks ».
    /// Même lecture pour une feuille en cours sans fractionnement (R-205) :
    /// ses liens entrants sont sans effet.
    fn actual_target(&self, v: NodeIx) -> bool {
        self.net
            .node(v)
            .is_some_and(|n| n.actual().is_some() || n.in_progress().is_some_and(|ip| !ip.split))
    }

    /// Dates au plus tôt de la cible `v` vues par ses prédécesseurs (marge
    /// libre, R-049) : celles du nœud ; feuille en cours fractionnable
    /// (R-206) : début = reprise (seule la partie restante dépend d'eux).
    fn early_view(&self, v: NodeIx) -> Option<Span> {
        let s = self.state(v)?;
        let early = s.early?;
        Some(match s.run {
            Some(run) if self.net.node(v).is_some_and(|n| n.in_progress().is_some()) => Span {
                start: run.resume,
                finish: early.finish,
            },
            _ => early,
        })
    }

    /// Arc `a` non appliqué en passe avant (R-069).
    fn is_dropped(&self, a: u32) -> bool {
        pos(a).and_then(|i| self.dropped.get(i)).copied().unwrap_or(false)
    }

    /// Ajoute un diagnostic ; en repasse (`quiet`), seul un invariant violé
    /// (`schedule.internal`) est gardé : la passe principale a déjà tout
    /// signalé.
    fn emit(&mut self, d: Diagnostic) {
        if !self.quiet || matches!(d, Diagnostic::ScheduleInternal { .. }) {
            self.diags.push(d);
        }
    }

    /// Signale l'échec du calcul de la tâche `uid` dans la passe `pass`. En
    /// repasse, un échec est un invariant violé (la passe principale a
    /// réussi sur ce nœud).
    fn report(&mut self, uid: TaskUid, pass: Pass, fail: Fail) {
        if self.quiet {
            let detail = match fail {
                Fail::Conversion(cause) => cause,
                Fail::Internal(detail) => detail,
            };
            self.diags.push(Diagnostic::ScheduleInternal {
                detail: format!("tâche {uid} : échec en repasse ({detail})"),
            });
            return;
        }
        match fail {
            Fail::Conversion(cause) => self.diags.push(Diagnostic::TaskUnschedulable {
                task: uid,
                pass,
                cause: cause.to_owned(),
            }),
            Fail::Internal(detail) => self.diags.push(Diagnostic::ScheduleInternal {
                detail: format!("tâche {uid} : {detail}"),
            }),
        }
    }

    /// Minutes du décalage de l'arc `a` (R-043 : pourcentage de la durée du
    /// prédécesseur, durée d'une récapitulative par R-054), mémorisées.
    fn lag_minutes(&mut self, a: u32) -> Result<Minutes, Fail> {
        let k = pos(a).ok_or(Fail::Internal("arc hors bornes"))?;
        if let Some(Some(m)) = self.lag.get(k) {
            return Ok(*m);
        }
        let arc = self.net.arc(a).ok_or(Fail::Internal("arc hors bornes"))?;
        let link = arc
            .link
            .ok_or(Fail::Internal("arc de hiérarchie sans décalage"))?;
        let m = match link.lag {
            network::LagSpec::Time(m) => m,
            network::LagSpec::Percent(pct) => crate::duration::percent_of(self.duration_of(arc.from)?, pct)?,
        };
        if let Some(slot) = self.lag.get_mut(k) {
            *slot = Some(m);
        }
        Ok(m)
    }

    /// Ancre de fin d'un successeur, avant le transfert arrière d'un lien FF
    /// ou SF (R-047) : le dernier instant de même temps ouvré sur l'échelle
    /// de placement du successeur (`next_working_moment`, identité en temps
    /// écoulé). La contrainte FF/SF se lit en temps ouvré de cette échelle
    /// (R-039) : sans ce report, un décalage écoulé vers une feuille ouvrée
    /// donnerait une marge négative sans contrainte (fin du successeur
    /// ramenée à 17:00 alors que la borne tombait à 17:06). Plus aucun temps
    /// ouvré après `y` (fin du domaine) : tout instant jusqu'à la fin du
    /// domaine a le même temps ouvré, l'ancre est `Moment::MAX`.
    ///
    /// Jalon (R-031) : une borne de fin `b` le place en `b` si `b` commence
    /// une minute ouvrée, sinon à l'instant ouvré précédent. Il ne dépasse
    /// donc pas `y` tant que `b < next_working_moment(y)` : l'ancre est `y`
    /// s'il commence une minute ouvrée, sinon `next_working_moment(y) − 1
    /// min` (et non `next_working_moment(y)`, qui le placerait au début de la
    /// plage suivante).
    fn finish_anchor(&self, target: NodeIx, y: Moment) -> Moment {
        let Some(node) = self.net.node(target) else {
            return y;
        };
        let milestone = node.duration == Minutes::ZERO;
        match self.net.scale(node.place) {
            Some(crate::calendar::TimeScale::Working(c)) => match c.next_working_moment(y) {
                Ok(next) if milestone && next != y => next.checked_add(Minutes::new(-1)).unwrap_or(y),
                Ok(next) => next,
                Err(_) => Moment::MAX,
            },
            _ => y,
        }
    }

    /// R-047 : la cible `target` d'un lien d'échelle `sigma` lit sa borne en
    /// `Moment` brut et non en temps ouvré de `σ` : porte d'une
    /// récapitulative (borne non recalée), jalon (R-031 : il reste sur une
    /// fin de minute ouvrée mais avance au début de la minute ouvrée
    /// suivante sinon), feuille placée sur une autre échelle (calendrier
    /// propre différent, durée écoulée). Le transfert arrière doit alors
    /// être l'adjoint en brut ([`transfer::backward`], `raw`). Une feuille de
    /// durée > 0 placée sur `σ` commence à `next_working(borne)`, qui ne
    /// dépend que du temps ouvré de la borne.
    fn reads_raw(&self, target: NodeIx, sigma: u32) -> bool {
        let Some(node) = self.net.node(target) else {
            return true;
        };
        // Feuille en cours sans reste (R-204) : son reste se lit comme un
        // jalon.
        let no_rest = node.in_progress().is_some_and(|ip| ip.remaining == Minutes::ZERO);
        if node.kind == network::NodeKind::Gate || node.duration == Minutes::ZERO || no_rest {
            return true;
        }
        if node.place == sigma {
            return false;
        }
        match (self.net.scale(node.place), self.net.scale(sigma)) {
            (Some(crate::calendar::TimeScale::Working(a)), Some(crate::calendar::TimeScale::Working(b))) => {
                !std::ptr::eq(*a, *b)
            }
            _ => true,
        }
    }

    /// Durée d'un nœud prédécesseur : feuille (manuelle : durée utilisée) ;
    /// agrégat : temps ouvré entre son début et sa fin sur le calendrier de
    /// la récapitulative, sinon du projet (R-054, R-113) ; récapitulative manuelle datée : durée
    /// utilisée (R-064).
    fn duration_of(&self, v: NodeIx) -> Result<Minutes, Fail> {
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        if node.fixed.is_some() {
            return Ok(node.duration);
        }
        match node.kind {
            network::NodeKind::Leaf => Ok(node.duration),
            network::NodeKind::Rollup => {
                // À rebours, la durée d'une récapitulative dépend des dates de
                // ses sous-tâches, inconnues quand la passe arrière principale
                // lit ses liens sortants (R-110) : le lien est ignoré.
                let missing = match self.mode {
                    Mode::FromStart => Fail::Internal("agrégat sans dates"),
                    Mode::FromFinish => Fail::Conversion(PERCENT_LAG_FROM_SUMMARY),
                };
                let e = self.state(v).and_then(|s| s.early).ok_or(missing)?;
                let scale = self
                    .net
                    .scale(node.measure)
                    .ok_or(Fail::Internal("échelle hors bornes"))?;
                Ok(scale.between(e.start, e.finish)?)
            }
            network::NodeKind::Gate => Err(Fail::Internal("porte prise comme prédécesseur")),
        }
    }
}

/// Plafonds de taille du calcul (règle 10 : un fichier hostile ne doit pas
/// épuiser la mémoire).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Arcs dépliés par R-058 (lien FF ou SF vers une récapitulative, un
    /// arc par feuille), tous liens confondus ; au-delà, les liens restants
    /// sont ignorés avec `link.expansion_limit`.
    pub expanded_arcs: usize,
    /// Segments des calendriers combinés (intersections et unions, R-172)
    /// gardés par recalcul, tous confondus ; le travail des combinaisons
    /// (segments lus) est borné à 8 fois cette valeur. Au-delà,
    /// `calendar.too_many_segments` et repli (R-171, R-173).
    pub combined_segments: usize,
    /// Segments des calendriers du modèle compilés par recalcul
    /// (`CalendarSet::compile_with`, R-172 amendée), tous confondus ; le
    /// travail de compilation est borné à 8 fois cette valeur. Au-delà,
    /// `calendar.too_many_segments`, calendrier non compilé et repli de ses
    /// tâches et ressources (R-066, R-171).
    pub compiled_segments: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            expanded_arcs: network::DEFAULT_EXPANDED_ARC_BUDGET,
            combined_segments: placement::DEFAULT_COMBINED_SEGMENTS,
            compiled_segments: crate::calendar::compiled::DEFAULT_COMPILED_SEGMENTS,
        }
    }
}

/// Calcule le planning complet du projet.
///
/// Fonction pure : aucune horloge, aucune entrée/sortie. N'échoue jamais, ne
/// panique jamais : toute anomalie devient un diagnostic. Les calendriers
/// sont compilés à chaque appel.
#[must_use]
pub fn schedule(project: &Project) -> Schedule {
    schedule_with(project, Limits::default())
}

/// [`schedule`] avec des plafonds explicites (tests des plafonds, outils).
#[must_use]
pub fn schedule_with(project: &Project, limits: Limits) -> Schedule {
    run(project, limits, Strategy::Fast)
}

/// [`schedule`] par l'implémentation de référence des marges et du %
/// achevé des récapitulatives (celle de J3, en O(N × profondeur × liens)) :
/// oracle des tests d'égalité de l'implémentation rapide (ADR-0012, lot
/// L15). Résultat identique à [`schedule`] ; ne sert pas en production.
#[doc(hidden)]
#[must_use]
pub fn schedule_reference(project: &Project) -> Schedule {
    run(project, Limits::default(), Strategy::Reference)
}

/// Implémentation des marges (R-049) et du % achevé des récapitulatives
/// (R-055).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Strategy {
    /// Réduction des arcs hérités et sommes préfixes (lot L15), repli sur la
    /// référence pour un nœud en échec ou après un drapeau changé.
    Fast,
    /// Remontée des ancêtres pour chaque nœud, sous-arbre parcouru pour
    /// chaque récapitulative (J3), oracle des tests.
    Reference,
}

/// Corps de [`schedule_with`] et de [`schedule_reference`].
fn run(project: &Project, limits: Limits, strategy: Strategy) -> Schedule {
    let (mode, anchor) = match project.anchor {
        ScheduleAnchor::FromStart(m) => (Mode::FromStart, m),
        ScheduleAnchor::FromFinish(m) => (Mode::FromFinish, m),
    };
    let start = anchor;
    let mut diags = Diagnostics::new();
    // Étape 0 : calendriers et replis (R-066).
    let (set, compile_diags) = CalendarSet::compile_with(project, limits.compiled_segments);
    diags.extend(compile_diags);
    let fallback = match set.get(project.calendar) {
        Ok(c) if c.has_working_time() => None,
        _ => {
            diags.push(Diagnostic::ProjectCalendarFallback {
                calendar: project.calendar,
            });
            Some(compile_builtin_standard(project))
        }
    };
    let project_cal: Option<&CompiledCalendar> = match &fallback {
        Some(c) => c.as_ref(),
        None => set.get(project.calendar).ok(),
    };
    let Some(project_cal) = project_cal else {
        return internal_failure(
            project,
            start,
            diags,
            "calendrier Standard intégré non compilable",
        );
    };
    // R-113 : le calendrier du projet est un calendrier de base. S'il en
    // désigne un dérivé (calendrier de ressource), sa base mesure durées des
    // récapitulatives, marges et décalages, et place les tâches sans
    // affectation de travail ni calendrier propre (R-175) ; le calendrier
    // désigné ne place que les tâches affectées à ses ressources (R-171).
    let project_cal = match (&fallback, derived_base(project, &set)) {
        (None, Some(base)) => base,
        _ => project_cal,
    };
    let sc = network::scales(project, &set, project_cal, &mut diags);
    // J5 : affectations résolues (calendriers effectifs, R-171 ; `d_a`,
    // R-170), avant le réseau qui les place (R-173).
    let resolved = placement::resolve(project, &set, project_cal, limits.combined_segments, &mut diags);
    // Étapes 1 à 5.
    let Some(mut net) = network::build(
        project,
        sc,
        (&resolved, &set),
        mode,
        limits.expanded_arcs,
        &mut diags,
    ) else {
        return internal_failure(project, start, diags, "construction du réseau");
    };
    let Some(order) = order::order(&mut net, &mut diags) else {
        return internal_failure(project, start, diags, "ordre topologique");
    };
    let mut calc = Calc {
        net: &net,
        project,
        mode,
        strategy,
        role: PassRole::Primary,
        quiet: false,
        start: anchor,
        finish: anchor,
        st: vec![NodeState::NEW; net.nodes.len()],
        lag: vec![None; net.arcs.len()],
        dropped: vec![false; net.arcs.len()],
        diags,
    };
    // Étape 6.
    match mode {
        Mode::FromStart => {
            calc.forward(&order, PassRole::Primary);
            calc.backward(&order, PassRole::Secondary);
            // R-092 : tâches ALAP épinglées sur leurs dates au plus tard,
            // puis une seule repasse de la passe avant, seulement s'il y a
            // une épingle.
            if calc.pin_opposite() {
                calc.forward(&order, PassRole::Replay);
            }
        }
        Mode::FromFinish => {
            // R-106 : passe arrière principale sous le plafond commun `F`.
            calc.backward(&order, PassRole::Primary);
            // R-107 : début calculé `S*`, plancher de la passe avant
            // secondaire.
            calc.start = calc.earliest_late_start();
            calc.forward(&order, PassRole::Secondary);
            // R-108 : tâches ASAP épinglées sur leurs dates au plus tôt, puis
            // une seule repasse de la passe arrière.
            if calc.pin_opposite() {
                calc.backward(&order, PassRole::Replay);
            }
        }
    }
    // Étapes 7 et 8.
    calc.summary_late();
    calc.slack();
    calc.check_constraints();
    let mut out = calc.output(&order);
    // J5 (lot L5) : travail, débits, charges et surutilisation (R-184 à
    // R-187).
    // Intersections reprises de `resolved` (jamais recompilées).
    load::apply(project, &net, (&set, project_cal, &resolved), &mut out);
    out
}

/// R-113 : calendrier de base du calendrier du projet quand celui-ci est
/// dérivé, compilé et doté de temps ouvré ; `None` sinon (calendrier du
/// projet de base, base inutilisable : le calendrier désigné sert à tout).
fn derived_base<'s>(project: &Project, set: &'s CalendarSet) -> Option<&'s CompiledCalendar> {
    let calendar = project.calendars.get(project.calendar)?;
    let CalendarKind::Derived { base, .. } = calendar.kind() else {
        return None;
    };
    set.get(*base).ok().filter(|c| c.has_working_time())
}

/// R-113 (exposée pour l'écrivain MSPDI, R-122) : calendrier qui **mesure**
/// les marges de `task` (R-048, R-049) et, pour une récapitulative, sa
/// durée (R-054) — le même que celui du moteur.
///
/// - Tâche de calendrier propre utilisable (compilé, avec du temps ouvré) :
///   ce calendrier, même s'il est le calendrier dérivé désigné par le
///   projet (il place et mesure).
/// - Sinon (aucun calendrier propre, calendrier propre inutilisable,
///   R-066, ligne vide) et pour la récapitulative du projet (UID 0, dont le
///   calendrier propre n'est pas lu, R-068) : le calendrier du projet, qui
///   est un calendrier de base ; s'il désigne un calendrier dérivé dont la
///   base est utilisable, cette base.
/// - `None` : le calendrier du projet est inutilisable, le moteur mesure sur
///   le calendrier Standard intégré (repli de R-066).
///
/// Une durée écoulée (R-067) se mesure en temps civil : c'est à l'appelant
/// d'en décider pour une feuille (la fonction ne regarde pas la base de la
/// durée ; une récapitulative se mesure toujours sur ce calendrier).
/// Compile les calendriers à chaque appel : pour plusieurs tâches, voir
/// [`measure_calendar_in`].
#[must_use]
pub fn measure_calendar(project: &Project, task: &Task) -> Option<CalendarUid> {
    let (set, _) = CalendarSet::compile(project);
    measure_calendar_in(project, &set, task)
}

/// [`measure_calendar`] sur des calendriers déjà compilés
/// (`CalendarSet::compile(project)`).
#[must_use]
pub fn measure_calendar_in(project: &Project, set: &CalendarSet, task: &Task) -> Option<CalendarUid> {
    let usable = |uid: CalendarUid| set.get(uid).is_ok_and(CompiledCalendar::has_working_time);
    if task.uid() != TaskUid::PROJECT_SUMMARY
        && let Some(own) = task.calendar.filter(|&c| !task.is_null && usable(c))
    {
        return Some(own);
    }
    if !usable(project.calendar) {
        return None;
    }
    match project
        .calendars
        .get(project.calendar)
        .map(crate::calendar::Calendar::kind)
    {
        Some(CalendarKind::Derived { base, .. }) if usable(*base) => Some(*base),
        _ => Some(project.calendar),
    }
}

/// Calendrier Standard intégré, compilé seul, sous l'UID du calendrier du
/// projet (repli de R-066).
fn compile_builtin_standard(project: &Project) -> Option<CompiledCalendar> {
    let standard = builtin::standard(project.calendar, "Standard".to_owned());
    let arena = Arena::try_from(vec![standard]).ok()?;
    CompiledCalendar::compile(&arena, project.calendar).ok()
}

/// Invariant interne violé avant les passes : `schedule.internal`, toutes
/// les tâches non vides exclues comme non planifiables.
fn internal_failure(project: &Project, start: Moment, mut diags: Diagnostics, detail: &str) -> Schedule {
    diags.push(Diagnostic::ScheduleInternal {
        detail: detail.to_owned(),
    });
    let tasks: BTreeMap<_, _> = project
        .tasks
        .iter()
        .map(|t| {
            let reason = if t.is_null {
                Exclusion::NullRow
            } else {
                Exclusion::Unschedulable
            };
            (
                t.uid(),
                TaskOutcome::Excluded {
                    reason,
                    display: None,
                },
            )
        })
        .collect();
    Schedule {
        project_start: start,
        project_finish: start,
        project_summary: None,
        tasks,
        order: Vec::new(),
        diagnostics: diags.finish(),
        assignments: BTreeMap::new(),
        resources: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    //! Mécanique des portes et des agrégats (cas de la spécification J2 ;
    //! les fiches R-053 à R-060 ont leurs tests dans `tests/rules/`).
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Driver, Schedule, TaskRole, TaskSchedule, schedule};
    use crate::calendar::BuiltinCalendarNames;
    use crate::duration::{Duration, DurationUnit, Lag, LagValue, Minutes, TimeBasis};
    use crate::ids::TaskUid;
    use crate::model::{Link, LinkType, OutlineLevel, Project, ProjectInit, ScheduleAnchor, Task};
    use crate::quantity::Percent;
    use crate::time::Moment;

    fn m(s: &str) -> Moment {
        s.parse().unwrap()
    }

    /// Projet : (uid, niveau, minutes), liens (pred, succ, type, décalage).
    fn project(tasks: &[(u32, u16, i64)], links: &[(u32, u32, LinkType, i64)]) -> Project {
        let mut p = Project::new(ProjectInit {
            name: "T".to_owned(),
            anchor: ScheduleAnchor::FromStart(m("2026-10-19T08:00")),
            current_date: m("2026-10-19T08:00"),
            calendar_names: BuiltinCalendarNames::default(),
        });
        let o = p.options;
        for &(uid, level, minutes) in tasks {
            let mut t = Task::new(TaskUid::new(uid), String::new(), &o);
            t.outline_level = OutlineLevel::new(level).unwrap();
            t.duration = Duration::new(
                Minutes::new(minutes),
                DurationUnit::Day,
                TimeBasis::Working,
                false,
            )
            .unwrap();
            p.tasks.push(t).unwrap();
        }
        for &(pred, succ, kind, lag) in links {
            let lag = Lag::new(
                TimeBasis::Working,
                LagValue::Time {
                    minutes: Minutes::new(lag),
                    unit: DurationUnit::Day,
                },
            )
            .unwrap();
            p.tasks
                .get_mut(TaskUid::new(succ))
                .unwrap()
                .predecessors
                .push(Link {
                    predecessor: TaskUid::new(pred),
                    kind,
                    lag,
                });
        }
        p
    }

    fn at(s: &Schedule, uid: u32) -> TaskSchedule {
        *s.scheduled(TaskUid::new(uid)).unwrap()
    }

    #[test]
    fn gate_bounds_all_subtasks() {
        // X (1) 2 j FS → S (2) {A (3) 1 j, B (4) 1 j} : A, B mer 08:00 –
        // 17:00, pilote : la porte de S.
        let s = schedule(&project(
            &[(1, 1, 960), (2, 1, 0), (3, 2, 480), (4, 2, 480)],
            &[(1, 2, LinkType::FinishToStart, 0)],
        ));
        for uid in [3, 4] {
            assert_eq!(at(&s, uid).early.start, m("2026-10-21T08:00"));
            assert_eq!(
                at(&s, uid).driver,
                Driver::SummaryGate {
                    summary: TaskUid::new(2)
                }
            );
        }
        let sum = at(&s, 2);
        assert_eq!(sum.role, TaskRole::Summary);
        assert_eq!(sum.driver, Driver::Rollup);
        assert_eq!(sum.early.start, m("2026-10-21T08:00"));
        assert_eq!(sum.duration, Minutes::new(480));
    }

    #[test]
    fn summary_successor_caps_all_leaves() {
        // S (1) {A (2) 1 j, B (3) 2 j} FS → Y (4) 1 j : Y mer ; A LF mar
        // 17:00, TF 480, FS 480 ; B TF 0.
        let s = schedule(&project(
            &[(1, 1, 0), (2, 2, 480), (3, 2, 960), (4, 1, 480)],
            &[(1, 4, LinkType::FinishToStart, 0)],
        ));
        assert_eq!(at(&s, 4).early.start, m("2026-10-21T08:00"));
        assert_eq!(at(&s, 2).late.finish, m("2026-10-20T17:00"));
        assert_eq!(at(&s, 2).total_slack, Minutes::new(480));
        assert_eq!(at(&s, 2).free_slack, Minutes::new(480));
        assert_eq!(at(&s, 3).total_slack, Minutes::ZERO);
    }

    #[test]
    fn summary_start_link_caps_same_start_leaves_only() {
        // S (1) {A (2) 1 j ; B (3) FS de A 1 j} SS+1 j → Y (4) 1 j :
        // Y mar 08:00 – 17:00 ; plafond lun 08:00 sur A seule →
        // TF(A) = TF(B) = TF(Y) = 0.
        let s = schedule(&project(
            &[(1, 1, 0), (2, 2, 480), (3, 2, 480), (4, 1, 480)],
            &[
                (2, 3, LinkType::FinishToStart, 0),
                (1, 4, LinkType::StartToStart, 480),
            ],
        ));
        assert_eq!(at(&s, 4).early.start, m("2026-10-20T08:00"));
        for uid in [2, 3, 4] {
            assert_eq!(at(&s, uid).total_slack, Minutes::ZERO, "{uid}");
            assert!(at(&s, uid).free_slack >= Minutes::ZERO, "{uid}");
        }
    }

    #[test]
    fn summary_slack_on_aggregated_dates() {
        // P (1) 1 j ; S (2) {A (3) 1 j sans lien, B (4) 1 j FS de P} ;
        // Q (5) 1 j FS de B. S : ES lun 08:00, EF mar 17:00, LS = mar 08:00,
        // LF = mer 17:00 → TF(S) = FS(S) = 480, non critique malgré B.
        let s = schedule(&project(
            &[(1, 1, 480), (2, 1, 0), (3, 2, 480), (4, 2, 480), (5, 1, 480)],
            &[
                (1, 4, LinkType::FinishToStart, 0),
                (4, 5, LinkType::FinishToStart, 0),
            ],
        ));
        let sum = at(&s, 2);
        assert_eq!(sum.late.start, m("2026-10-20T08:00"));
        assert_eq!(sum.late.finish, m("2026-10-21T17:00"));
        assert_eq!(sum.total_slack, Minutes::new(480));
        assert_eq!(sum.free_slack, Minutes::new(480));
        assert!(!sum.critical);
        assert!(at(&s, 4).critical);
        assert_eq!(at(&s, 3).total_slack, Minutes::new(960));
    }

    #[test]
    fn summary_percent_weighted_by_leaf_durations() {
        // S1 (1) {S2 (2) {A (3) 960 à 50 %, B (4) 480 à 0 %}, C (5) 240 à
        // 100 %} : (480 + 0 + 240) / 1 680 → 4 286.
        let mut p = project(
            &[(1, 1, 0), (2, 2, 0), (3, 3, 960), (4, 3, 480), (5, 2, 240)],
            &[],
        );
        p.tasks
            .get_mut(TaskUid::new(3))
            .unwrap()
            .progress
            .percent_complete = Percent::new(5_000).unwrap();
        p.tasks
            .get_mut(TaskUid::new(5))
            .unwrap()
            .progress
            .percent_complete = Percent::FULL;
        let s = schedule(&p);
        assert_eq!(at(&s, 1).percent_complete, Percent::new(4_286).unwrap());
    }

    #[test]
    fn from_finish_schedules_backward_from_the_anchor() {
        // R-106 : A 1 j ALAP, F ven 23/10 17:00 → A ven 08:00 – 17:00 ;
        // début calculé (R-107) = ven 08:00 ; fin du projet = F.
        let mut p = project(&[(1, 1, 480)], &[]);
        p.anchor = ScheduleAnchor::FromFinish(m("2026-10-23T17:00"));
        p.tasks.get_mut(TaskUid::new(1)).unwrap().constraint = crate::model::Constraint::AsLateAsPossible;
        let s = schedule(&p);
        assert_eq!(s.project_finish, m("2026-10-23T17:00"));
        assert_eq!(s.project_start, m("2026-10-23T08:00"));
        assert_eq!(at(&s, 1).start, m("2026-10-23T08:00"));
        assert_eq!(at(&s, 1).late_driver, super::LateDriver::ProjectFinish);
        assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    }

    #[test]
    fn schedule_json_round_trip() {
        let s = schedule(&project(
            &[(1, 1, 0), (2, 2, 480), (3, 1, 480)],
            &[(2, 3, LinkType::FinishToStart, 0)],
        ));
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""status":"scheduled""#), "{json}");
        assert!(json.contains(r#""kind":"rollup""#), "{json}");
        assert!(
            json.contains(r#""late_driver":{"kind":"project_finish"}"#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Schedule>(&json).unwrap(), s);
    }
}
