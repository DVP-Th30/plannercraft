//! Passe arrière (spécification J2 § 3.8, R-046, R-047, R-051, R-057,
//! R-061, R-069 ; spécification J3 § 3.8, R-095 à R-111), dans l'ordre
//! topologique inverse. Depuis le début, c'est la passe secondaire (dates au
//! plus tard sous la fin du projet) ; à rebours, la passe principale (dates
//! planifiées sous l'ancre `F`, R-106), puis éventuellement sa repasse
//! (R-108).
//!
//! Les plafonds passent par le transfert adjoint `h` (R-047). Un agrégat est
//! traité avant ses enfants (il les suit dans l'ordre topologique) : il leur
//! transmet ses plafonds hérités `C(S)` (fin, liens FS/FF sortants) et
//! `K(S)` (début, liens SS/SF sortants, appliqué aux seules feuilles de même
//! début, A11). Une porte est traitée après ses enfants : `LG(S)` = min des
//! débuts au plus tard de ses enfants actifs, lu par les liens FS/SS qui la
//! visent.

use super::constraint::{ConstraintRule, Mode, Side};
use super::network::{Fail, NodeIx, NodeKind, pos};
use super::{Calc, LateDriver, Pass, PassRole, transfer};
use crate::calendar::{Span, TimeScale};
use crate::diagnostic::{Diagnostic, OverriddenBound};
use crate::ids::TaskUid;
use crate::model::{ConstraintKind, LinkType};
use crate::time::Moment;

/// Plafonds lus sur les arcs de lien sortants d'un nœud, avec leur cause
/// (R-047), dans l'ordre des arcs.
#[derive(Default)]
struct Caps {
    /// Plafonds de fin (FS, FF).
    finishes: Vec<(Moment, LateDriver)>,
    /// Plafonds de début (SS, SF), à convertir pour une feuille.
    starts: Vec<(Moment, LateDriver)>,
    /// Un successeur est peu sûr (dégradé, en échec) : dates au plus tard
    /// de repli (R-069).
    unreliable: bool,
}

impl Caps {
    /// Plus petit plafond de fin ; à égalité, le premier arc.
    fn finish(&self) -> Option<(Moment, LateDriver)> {
        self.finishes.iter().fold(None, |acc, &c| Some(min_cap(acc, c)))
    }
}

/// Plafond de fin d'une feuille, avec sa cause (R-047).
#[derive(Clone, Copy)]
struct Cap {
    /// Plafond de fin.
    at: Moment,
    /// Cause, si c'est le plus petit plafond.
    driver: LateDriver,
}

/// Minimum d'un plafond optionnel et d'un plafond.
fn min_opt(a: Option<Moment>, b: Moment) -> Moment {
    a.map_or(b, |a| a.min(b))
}

/// Plus petit de deux plafonds avec leur cause ; à égalité, le premier
/// garde la sienne.
fn min_cap<T: Copy>(a: Option<(Moment, T)>, b: (Moment, T)) -> (Moment, T) {
    match a {
        Some(a) if a.0 <= b.0 => a,
        _ => b,
    }
}

/// Plafond de fin en cours de calcul et sa cause (R-047) : le premier
/// minimum strict garde sa cause.
struct LateCap {
    finish: Moment,
    driver: LateDriver,
}

impl LateCap {
    /// Applique le plafond `m` de cause `driver` s'il est strictement plus
    /// petit.
    fn apply(&mut self, m: Moment, driver: LateDriver) {
        if m < self.finish {
            self.finish = m;
            self.driver = driver;
        }
    }
}

impl Calc<'_, '_> {
    /// Passe arrière complète. Depuis le début, c'est la passe secondaire
    /// (`role` = `Secondary`) : elle ne visite que les nœuds atteints par la
    /// passe avant (`reached`). À rebours, c'est la passe principale
    /// (`Primary`, tous les nœuds actifs, sous l'ancre `F`, R-106) ou sa
    /// repasse (`Replay`, après l'épingle des tâches ASAP, R-108 : aucun
    /// diagnostic sauf un invariant violé).
    pub(super) fn backward(&mut self, order: &[NodeIx], role: PassRole) {
        self.role = role;
        self.quiet = role == PassRole::Replay;
        // R-051 : seulement depuis le début (son miroir à rebours est en
        // passe avant, R-110).
        let has_succ = if self.mode == Mode::FromStart {
            self.has_successors()
        } else {
            Vec::new()
        };
        for &v in order.iter().rev() {
            let Some(node) = self.net.node(v) else { continue };
            if node.inactive || !self.reached(v) {
                continue;
            }
            let (kind, uid) = (node.kind, node.uid);
            let early = self.state(v).and_then(|s| s.early);
            // Depuis le début, un nœud sans dates au plus tôt n'est pas
            // atteint (`reached`) ; à rebours, la passe principale n'en a
            // pas encore.
            if self.mode == Mode::FromStart && early.is_none() {
                continue;
            }
            // R-064 : une récapitulative manuelle datée se traite comme une
            // feuille manuelle ; ses enfants n'héritent d'aucun plafond.
            let leaf_like = kind == NodeKind::Leaf || node.fixed.is_some();
            if leaf_like {
                let own_succ = has_succ.get(node.task).copied().unwrap_or(false);
                match self.leaf_late(v, early, own_succ) {
                    Ok((late, driver)) => {
                        if let Some(s) = self.state_mut(v) {
                            s.late = Some(late);
                            s.late_driver = driver;
                        }
                    }
                    Err(fail) => self.late_failed(v, uid, early, fail),
                }
            } else if kind == NodeKind::Rollup {
                if let Err(fail) = self.rollup_caps(v, early) {
                    let from_finish = self.mode == Mode::FromFinish;
                    if let Some(s) = self.state_mut(v) {
                        s.degraded = true;
                        // À rebours, ses sous-tâches n'ont pas tous leurs
                        // plafonds hérités : dates de repli (R-111).
                        s.late_degraded |= from_finish;
                    }
                    self.report(uid, Pass::Backward, fail);
                }
            } else {
                self.gate_late(v);
            }
        }
        self.quiet = false;
    }

    /// Échec du calcul des dates au plus tard du nœud `v`. Depuis le début
    /// (R-069) : dates au plus tard de repli, égales aux dates au plus tôt,
    /// `degraded`. À rebours, en passe principale (R-111) : la tâche est
    /// exclue (`back_failed`, aucune date), ses prédécesseurs l'ignorent
    /// (`link.excluded_endpoint`) et sont dégradés ; en repasse, un échec
    /// est un invariant violé (`schedule.internal`, par [`Calc::report`]),
    /// les dates de la passe principale sont gardées.
    fn late_failed(&mut self, v: NodeIx, uid: TaskUid, early: Option<Span>, fail: Fail) {
        match (self.mode, self.role) {
            (Mode::FromFinish, PassRole::Primary) => {
                if let Some(s) = self.state_mut(v) {
                    s.late = None;
                    s.back_failed = true;
                }
            }
            (Mode::FromFinish, _) => {}
            (Mode::FromStart, _) => {
                if let Some(s) = self.state_mut(v) {
                    s.late = early;
                    s.late_driver = LateDriver::Fallback;
                    s.degraded = true;
                }
            }
        }
        self.report(uid, Pass::Backward, fail);
    }

    /// R-107 : début calculé `S*` à rebours, plancher de la passe avant
    /// secondaire : min des débuts au plus tard des feuilles actives
    /// planifiées (manuelles comprises) et des récapitulatives manuelles
    /// datées (R-064) ; l'ancre `F` s'il n'y en a aucune.
    pub(super) fn earliest_late_start(&self) -> Moment {
        self.net
            .nodes
            .iter()
            .zip(&self.st)
            .filter(|(node, s)| {
                (node.kind == NodeKind::Leaf || node.fixed.is_some()) && !node.inactive && !s.back_failed
            })
            .filter_map(|(_, s)| s.late.map(|l| l.start))
            .min()
            .unwrap_or(self.finish)
    }

    /// R-051 : vrai, par position, si la tâche a un successeur effectif
    /// (lien non coupé vers une tâche active et planifiée), en propre ou
    /// hérité d'une récapitulative ancêtre (sauf à travers une
    /// récapitulative manuelle datée, R-064). Parents avant enfants : un seul
    /// passage, après la passe avant. Un lien vers une cible sans dates au
    /// plus tôt (non planifiable, récapitulative vide dont l'agrégat n'a pas
    /// de dates, porte en échec) a été ignoré (`link.excluded_endpoint`) :
    /// ce n'est pas un successeur, même filtre que [`Self::out_caps`].
    fn has_successors(&self) -> Vec<bool> {
        let mut out = vec![false; self.net.tasks.len()];
        for p in 0..self.net.tasks.len() {
            let Some(info) = self.net.task(p) else { continue };
            let inherited = info
                .parent
                .filter(|&par| {
                    !self
                        .net
                        .task(par)
                        .and_then(|t| t.exit)
                        .is_some_and(|r| self.net.is_fixed_rollup(r))
                })
                .and_then(|par| out.get(par).copied())
                .unwrap_or(false);
            let own = info
                .exit
                .and_then(|v| self.net.outgoing(v))
                .is_some_and(|out_arcs| {
                    out_arcs.iter().any(|&a| {
                        !self.net.is_cut(a)
                            && !self.is_dropped(a)
                            && self.net.arc(a).is_some_and(|arc| {
                                arc.link.is_some_and(|l| !l.to_inactive)
                                    && !self.actual_target(arc.to)
                                    && self.applied_target(arc.to)
                            })
                    })
                });
            if let Some(slot) = out.get_mut(p) {
                *slot = own || inherited;
            }
        }
        out
    }

    /// Cible d'un arc de lien effectivement appliqué en passe avant : elle a
    /// des dates au plus tôt et, pour une porte, l'agrégat jumeau aussi (une
    /// récapitulative vide n'en a pas, R-053).
    fn applied_target(&self, target: NodeIx) -> bool {
        let Some(node) = self.net.node(target) else {
            return false;
        };
        self.reached(target) && (node.kind != NodeKind::Gate || node.twin.is_some_and(|r| self.reached(r)))
    }

    /// Plafonds des arcs de lien sortants du nœud `v` (R-047) : FS
    /// `h(LS_s)`, FF `h(LF_s)` en fin ; SS `h(LS_s)`, SF `h(LF_s)` en début.
    /// Les arcs coupés, vers une tâche inactive ou vers un nœud sans dates
    /// (au plus tôt ou au plus tard) sont ignorés. À rebours, un lien vers
    /// une tâche exclue par la passe arrière principale est signalé
    /// (`link.excluded_endpoint`, R-111). `drop` : un plafond qui ne se
    /// calcule pas ignore son lien (`link.unschedulable`, la cible
    /// dégradée) au lieu de faire échouer le nœud — agrégat d'une
    /// récapitulative à rebours, miroir de la porte en passe avant (R-069,
    /// R-110).
    fn out_caps(&mut self, v: NodeIx, drop: bool) -> Result<Caps, Fail> {
        let mut caps = Caps::default();
        // Référence au réseau copiée : aucune copie des arcs sortants.
        let net = self.net;
        for &a in net
            .outgoing(v)
            .ok_or(Fail::Internal("arcs sortants hors bornes"))?
        {
            if net.is_cut(a) || self.is_dropped(a) {
                continue;
            }
            let arc = *net.arc(a).ok_or(Fail::Internal("arc hors bornes"))?;
            let Some(l) = arc.link else { continue };
            // R-203 : une feuille achevée ne plafonne pas ses prédécesseurs.
            if l.to_inactive || self.actual_target(arc.to) {
                continue;
            }
            caps.unreliable |= self.unreliable(arc.to);
            // R-111 : cible exclue par la passe arrière principale.
            if self.mode == Mode::FromFinish && !self.reached(arc.to) {
                self.emit(Diagnostic::LinkExcludedEndpoint {
                    predecessor: l.pred,
                    successor: l.succ,
                });
                continue;
            }
            // Cible sans dates au plus tard, ou porte dont la borne n'a pas pu
            // se calculer en passe avant (R-069) : le lien n'a pas été
            // appliqué, il ne plafonne rien.
            let Some(tl) = self.late_view(arc.to) else {
                continue;
            };
            let y = match self.link_cap(a, l, arc.to, tl) {
                Ok(y) => y,
                Err(Fail::Conversion(cause)) if drop => {
                    self.drop_cap(a, l, cause);
                    caps.unreliable = true;
                    continue;
                }
                Err(fail) => return Err(fail),
            };
            let driver = LateDriver::Successor {
                successor: l.succ,
                link: l.ty,
            };
            match l.ty {
                LinkType::FinishToStart | LinkType::FinishToFinish => caps.finishes.push((y, driver)),
                LinkType::StartToStart | LinkType::StartToFinish => caps.starts.push((y, driver)),
            }
        }
        Ok(caps)
    }

    /// Dates au plus tard de la cible `target` vues par ses prédécesseurs,
    /// `None` si elle n'est pas atteinte ou n'en a pas : retard de
    /// nivellement (R-178, [`Self::held_late`]) ; feuille en cours (R-206,
    /// R-207) : début = début au plus tard du reste, seule partie qui dépend
    /// d'eux.
    fn late_view(&self, target: NodeIx) -> Option<Span> {
        let s = self.state(target).filter(|_| self.reached(target))?;
        let late = Self::held_late(s.late?, s.held, self.mode);
        let running = self.net.node(target).is_some_and(|n| n.in_progress().is_some());
        Some(match s.run.and_then(|r| r.late_resume) {
            Some(start) if running => Span {
                start,
                finish: late.finish,
            },
            _ => late,
        })
    }

    /// R-178 : dates au plus tard d'une cible à retard de nivellement, vues
    /// de ses prédécesseurs : au plus tard son placement sans le retard
    /// (`held`), depuis le début seulement (le retard a absorbé leur marge,
    /// A6 UID 21 → 36). Hypothèse.
    fn held_late(late: Span, held: Option<Span>, mode: Mode) -> Span {
        match held {
            Some(h) if mode == Mode::FromStart => Span {
                start: late.start.min(h.start),
                finish: late.finish.min(h.finish),
            },
            _ => late,
        }
    }

    /// Plafond `h(ancre au plus tard de la cible)` de l'arc de lien `a`
    /// (R-047).
    fn link_cap(
        &mut self,
        a: u32,
        l: super::network::LinkArc,
        target: NodeIx,
        tl: Span,
    ) -> Result<Moment, Fail> {
        let lag = self.lag_minutes(a)?;
        let sigma = *self
            .net
            .scale(l.sigma)
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        let anchor = match l.ty {
            LinkType::FinishToStart | LinkType::StartToStart => tl.start,
            LinkType::FinishToFinish | LinkType::StartToFinish => self.finish_anchor(target, tl.finish),
        };
        Ok(transfer::backward(
            sigma,
            anchor,
            lag,
            self.reads_raw(target, l.sigma),
        )?)
    }

    /// R-069, R-110 : arc de lien `a` depuis l'agrégat d'une récapitulative
    /// non appliqué à rebours (`link.unschedulable`) ; ignoré ensuite par la
    /// passe avant secondaire et par la marge libre.
    fn drop_cap(&mut self, a: u32, l: super::network::LinkArc, cause: &str) {
        if let Some(slot) = pos(a).and_then(|i| self.dropped.get_mut(i)) {
            *slot = true;
        }
        self.emit(Diagnostic::LinkUnschedulable {
            predecessor: l.pred,
            successor: l.succ,
            cause: cause.to_owned(),
        });
    }

    /// Dates au plus tard d'une feuille active (automatique ou manuelle) ou
    /// d'une récapitulative manuelle datée (R-064), et leur cause :
    /// plafond de fin = min(plafond commun : fin du projet, ou ancre `F` à
    /// rebours ; EF si plusieurs chemins critiques et aucun successeur,
    /// depuis le début ; `C(parent)` ; liens FS/FF) ; plafonds de début
    /// (`K(parent)` si `k_applies`, liens SS/SF) convertis par
    /// `forward_span(y, d).finish` ; contrainte ; échéance ;
    /// `late = backward_span(plafond, d)` (R-030, R-031 pour un jalon,
    /// R-061). Le premier minimum strict, dans cet ordre, donne la cause
    /// (`LateDriver`, R-047).
    ///
    /// À rebours (passe principale, R-106) : une tâche manuelle garde ses
    /// dates saisies (R-063, R-064, `link.manual_not_honored` pour chaque
    /// lien sortant violé) ; une épingle (MSO, MFO, option active, R-097,
    /// R-098, R-109 ; tâche ASAP épinglée en repasse, R-108) et un serrage
    /// (SNET, FNET, option active, R-109) donnent `constraint.override` par
    /// plafond violé (R-100).
    fn leaf_late(
        &mut self,
        v: NodeIx,
        early: Option<Span>,
        has_succ: bool,
    ) -> Result<(Span, LateDriver), Fail> {
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let (uid, place, constraint, deadline, fixed) =
            (node.uid, node.place, node.constraint, node.deadline, node.fixed);
        let from_finish = self.mode == Mode::FromFinish;
        // R-203, R-207 : feuille achevée, dates au plus tard = dates réelles.
        if let Some((start, finish)) = node.actual() {
            return self.actual_late(v, early, start, finish);
        }
        if let Some(ip) = node.in_progress() {
            return self.running_late(v, early, has_succ, ip);
        }
        // R-097, R-098 : une feuille épinglée (MSO, MFO, option active) a
        // ses dates au plus tard sur l'ancrage, sans plafond : une borne
        // violée s'exprime par la marge négative de l'amont.
        if !from_finish
            && let Some(c) = constraint
            && let ConstraintRule::Pin(_) = c.rule
        {
            return Ok((c.span()?, LateDriver::Constraint { kind: c.kind }));
        }
        let scale = *self
            .net
            .scale(place)
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        if from_finish {
            // R-106 (R-063, R-064) : dates saisies, prises comme dates au
            // plus tard ; elles plafonnent ses prédécesseurs.
            if let Some(f) = fixed {
                let f = f?;
                self.manual_successors(v, uid, scale, f);
                return Ok((f, LateDriver::Manual));
            }
            let pin = self.state(v).and_then(|s| s.pin);
            let forced = match (constraint, pin) {
                (Some(c), _) if matches!(c.rule, ConstraintRule::Pin(_)) => Some((c.span()?, c.kind, false)),
                (Some(c), Some(p)) => Some((p, c.kind, true)),
                _ => None,
            };
            if let Some((a, kind, opposite)) = forced {
                // Les liens sortants ne plafonnent pas une épingle, mais
                // sont lus comme pour toute feuille : `link.excluded_endpoint`
                // vers une cible exclue et drapeau « peu sûr » (R-111) ; un
                // plafond qui ne se calcule pas ne fait pas échouer
                // l'épingle.
                if let Ok(caps) = self.out_caps(v, false)
                    && caps.unreliable
                    && let Some(s) = self.state_mut(v)
                {
                    s.late_degraded = true;
                }
                self.late_overrides(v, scale, a, kind, opposite);
                return Ok((a, LateDriver::Constraint { kind }));
            }
        }
        let mut lf = LateCap {
            finish: self.finish,
            driver: LateDriver::ProjectFinish,
        };
        if !from_finish
            && self.project.options.multiple_critical_paths
            && !has_succ
            && let Some(e) = early
        {
            lf.apply(e.finish, LateDriver::MultipleCriticalPaths);
        }
        let (caps, unreliable) = self.leaf_caps(v, early)?;
        if unreliable && let Some(s) = self.state_mut(v) {
            s.late_degraded = true;
        }
        for cap in &caps {
            lf.apply(cap.at, cap.driver);
        }
        // Contrainte côté plafond (R-095, R-096) : `A.finish`, où `A` est
        // l'ancrage R-090 (pour SNLT, `A.finish = forward_span(s*, d).finish`,
        // le plafond de fin d'un début au plus tard en `s*`).
        if let Some(c) = constraint
            && self.caps_late(c.rule)
        {
            lf.apply(c.span()?.finish, LateDriver::Constraint { kind: c.kind });
        }
        // R-102 : échéance, plafond brut (comme les autres plafonds, avant
        // `backward_span`), propagé à l'amont par les liens.
        if let Some(d) = deadline {
            lf.apply(d, LateDriver::Deadline);
        }
        // R-174 : feuille affectée, min des débuts de ses parts.
        let natural = self.placer(v)?.backward_span(lf.finish)?;
        // R-109 : à rebours, option active, SNET et FNET serrent la passe
        // principale : si le placement sous les plafonds commence (finit)
        // avant l'ancrage, la feuille est placée sur l'ancrage et chaque
        // plafond violé est signalé (R-100).
        if from_finish
            && let Some(c) = constraint
            && let ConstraintRule::Clamped(side) = c.rule
        {
            let a = c.span()?;
            let early_side = match side {
                Side::Start => natural.start < a.start,
                Side::Finish => scale.between(natural.finish, a.finish)?.get() > 0,
            };
            if early_side {
                self.late_overrides(v, scale, a, c.kind, false);
                return Ok((a, LateDriver::Constraint { kind: c.kind }));
            }
        }
        Ok((natural, lf.driver))
    }

    /// R-203 (R-207 à rebours) : dates au plus tard d'une feuille achevée =
    /// ses dates réelles, cause `LateDriver::Actual`, sans aucun plafond (ni
    /// fin du projet, ni successeur, ni contrainte, ni échéance). Début réel
    /// implicite (R-200) : depuis le début, celui de la passe avant ; à
    /// rebours, la passe arrière principale le résout une fois,
    /// `backward_span(fin réelle, D).start` sur l'échelle de placement
    /// (hypothèse), `progress.actual_start_missing`.
    fn actual_late(
        &mut self,
        v: NodeIx,
        early: Option<Span>,
        start: Option<Moment>,
        finish: Moment,
    ) -> Result<(Span, LateDriver), Fail> {
        let stored = self.state(v).and_then(|s| s.actual).or(early);
        let span = match (stored, start) {
            (Some(span), _) => span,
            (None, Some(s)) => Span { start: s, finish },
            (None, None) => {
                let back = self.placer(v)?.backward_span(finish)?;
                Span {
                    start: back.start.min(finish),
                    finish,
                }
            }
        };
        if start.is_none() {
            let uid = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?.uid;
            self.emit(Diagnostic::ProgressActualStartMissing { task: uid });
        }
        if let Some(s) = self.state_mut(v) {
            s.actual = Some(span);
        }
        Ok((span, LateDriver::Actual))
    }

    /// R-206 (depuis le début) : dates au plus tard d'une feuille en cours.
    /// Partie réelle fixe : début au plus tard = début réel ; fin au plus
    /// tard par les plafonds de fin normaux (fin du projet, EF si plusieurs
    /// chemins critiques et aucun successeur, `C(parent)`, liens FS/FF,
    /// échéance) ; les plafonds de début (`K(parent)`, SS, SF) portent sur
    /// le début réel, déjà passé, et ne jouent pas ; la contrainte ne plafonne
    /// rien (R-205). Début au plus tard du reste = `backward_span(LF, reste)`
    /// sur le placement du reste, gardé pour les prédécesseurs (FS, SS).
    ///
    /// R-207 (à rebours) : le reste est placé au plus tôt depuis la reprise
    /// (`max(Stop, reprise saisie)`, aucune borne : les prédécesseurs ne sont
    /// pas encore placés), dates au plus tard = ces dates, cause
    /// `LateDriver::Actual`, `progress.from_finish_in_progress`
    /// (avertissement, passe principale). Début réel implicite : début du
    /// placement de toute la tâche sous ses plafonds de fin (hypothèse,
    /// comme R-203 à rebours), résolu une fois.
    fn running_late(
        &mut self,
        v: NodeIx,
        early: Option<Span>,
        has_succ: bool,
        ip: super::constraint::InProgress,
    ) -> Result<(Span, LateDriver), Fail> {
        let stored = self.state(v).and_then(|s| s.run);
        let from_finish = self.mode == Mode::FromFinish;
        if from_finish && let Some(run) = stored {
            let span = Span {
                start: run.actual_start,
                finish: run.finish,
            };
            return Ok((span, LateDriver::Actual));
        }
        let mut lf = LateCap {
            finish: self.finish,
            driver: LateDriver::ProjectFinish,
        };
        if !from_finish
            && self.project.options.multiple_critical_paths
            && !has_succ
            && let Some(e) = early
        {
            lf.apply(e.finish, LateDriver::MultipleCriticalPaths);
        }
        let links = self.out_caps(v, false)?;
        let mut unreliable = links.unreliable;
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let (uid, parent, deadline) = (node.uid, node.parent_rollup, node.deadline);
        if let Some(pr) = parent {
            let ps = *self.state(pr).ok_or(Fail::Internal("nœud hors bornes"))?;
            if let Some((c, summary)) = ps.cap_finish {
                lf.apply(c, LateDriver::SummaryCap { summary });
            }
            unreliable |= ps.late_degraded && !self.net.is_fixed_rollup(pr);
        }
        for &(at, driver) in &links.finishes {
            lf.apply(at, driver);
        }
        if let Some(d) = deadline {
            lf.apply(d, LateDriver::Deadline);
        }
        if unreliable && let Some(s) = self.state_mut(v) {
            s.late_degraded = true;
        }
        if !from_finish {
            let run = stored.ok_or(Fail::Internal("tâche en cours sans dates au plus tôt"))?;
            let rest = self.remaining_placer(v)?.backward_span(lf.finish)?;
            if let Some(s) = self.state_mut(v) {
                s.run = Some(super::Run {
                    late_resume: Some(rest.start),
                    ..run
                });
            }
            let late = Span {
                start: run.actual_start.min(rest.finish),
                finish: rest.finish,
            };
            return Ok((late, lf.driver));
        }
        // R-207 : à rebours, passe principale.
        let actual_start = if let Some(start) = ip.start {
            start
        } else {
            self.emit(Diagnostic::ProgressActualStartMissing { task: uid });
            self.placer(v)?.backward_span(lf.finish)?.start
        };
        let stop = self.stop_of(v, actual_start, ip.actual)?;
        let from = ip.resume.map_or(stop, |r| r.max(stop));
        let rest = self.remaining_placer(v)?.forward_span(from)?;
        let run = super::Run {
            actual_start,
            stop,
            resume: rest.start.max(stop),
            finish: rest.finish.max(stop),
            late_resume: Some(rest.start.max(stop)),
        };
        self.emit(Diagnostic::ProgressFromFinishInProgress { task: uid });
        if let Some(s) = self.state_mut(v) {
            s.run = Some(run);
        }
        Ok((
            Span {
                start: actual_start,
                finish: run.finish,
            },
            LateDriver::Actual,
        ))
    }

    /// Plafonds d'une feuille, dans l'ordre de R-047 (`C(parent)`,
    /// `K(parent)` si `k_applies`, liens FS/FF puis SS/SF dans l'ordre des
    /// arcs), chacun converti en plafond de fin : un plafond de début `y`
    /// devient `P.forward_span(P.latest_start(y, d), d).finish` (dernier début
    /// plaçable, R-047). Rend aussi le drapeau « successeur ou plafond
    /// hérité peu sûr » (R-069).
    fn leaf_caps(&mut self, v: NodeIx, early: Option<Span>) -> Result<(Vec<Cap>, bool), Fail> {
        let placer = self.placer(v)?;
        // Plafond de début `y` : la tâche doit commencer au plus tard en `y`
        // en brut ; `y` peut tomber hors du temps ouvré de P(t) (décalage
        // écoulé, successeur sur un autre calendrier, K(S)) : dernier début
        // plaçable avant la conversion (R-047).
        let start_cap = |y: Moment| -> Result<Moment, Fail> {
            let x = placer.latest_start(y)?;
            Ok(placer.forward_span(x)?.finish)
        };
        let parent = self
            .net
            .node(v)
            .ok_or(Fail::Internal("nœud hors bornes"))?
            .parent_rollup;
        let mut out = Vec::new();
        let links = self.out_caps(v, false)?;
        let mut unreliable = links.unreliable;
        if let Some(pr) = parent {
            let ps = *self.state(pr).ok_or(Fail::Internal("nœud hors bornes"))?;
            if let Some((c, summary)) = ps.cap_finish {
                out.push(Cap {
                    at: c,
                    driver: LateDriver::SummaryCap { summary },
                });
            }
            if let Some((k, summary)) = ps.cap_start
                && self.k_applies(ps.early.map(|e| e.start), early.map(|e| e.start))
            {
                out.push(Cap {
                    at: start_cap(k)?,
                    driver: LateDriver::SummaryCap { summary },
                });
            }
            unreliable |= ps.late_degraded && !self.net.is_fixed_rollup(pr);
        }
        for &(at, driver) in &links.finishes {
            out.push(Cap { at, driver });
        }
        for &(y, driver) in &links.starts {
            out.push(Cap {
                at: start_cap(y)?,
                driver,
            });
        }
        Ok((out, unreliable))
    }

    /// R-100 à rebours : la feuille `v`, placée de force sur `a` par sa
    /// contrainte de type `kind` (épingle, serrage ; `opposite` : épingle
    /// d'une tâche ASAP en repasse, R-108), viole ces bornes : l'ancre `F`
    /// (`ProjectFinish`, fin en temps ouvré de son échelle), les plafonds
    /// hérités de sa récapitulative (`SummaryCap` : `C(S)` en fin, en temps
    /// ouvré ; `K(S)` en début, en brut sur son dernier début plaçable), et
    /// chaque lien sortant propre (`Successor`) que ses dates violent face
    /// aux dates au plus tard du successeur, lues comme en passe avant
    /// (borne `g` de son ancre ; FS, SS : début du successeur en brut ; FF,
    /// SF : fin en temps ouvré de son échelle). Un `constraint.override` par
    /// borne violée, en passe principale seulement ; en repasse, seule une
    /// épingle de tâche ASAP est contrôlée, et une violation est un invariant
    /// violé. Une conversion impossible ne signale rien.
    fn late_overrides(
        &mut self,
        v: NodeIx,
        scale: TimeScale<'_>,
        a: Span,
        kind: ConstraintKind,
        opposite: bool,
    ) {
        if self.mode != Mode::FromFinish
            || self.role == PassRole::Secondary
            || (self.role == PassRole::Replay && !opposite)
        {
            return;
        }
        let Some(node) = self.net.node(v) else { return };
        let (task, parent) = (node.uid, node.parent_rollup);
        let Ok(placer) = self.placer(v) else { return };
        let exceeds = |cap: Moment| scale.between(cap, a.finish).is_ok_and(|m| m.get() > 0);
        let mut violated: Vec<OverriddenBound> = Vec::new();
        if exceeds(self.finish) {
            violated.push(OverriddenBound::ProjectFinish);
        }
        if let Some(ps) = parent.and_then(|pr| self.state(pr)).copied() {
            if let Some((c, summary)) = ps.cap_finish
                && exceeds(c)
            {
                violated.push(OverriddenBound::SummaryCap { summary });
            }
            if let Some((k, summary)) = ps.cap_start
                && placer.latest_start(k).is_ok_and(|x| a.start > x)
            {
                violated.push(OverriddenBound::SummaryCap { summary });
            }
        }
        let net = self.net;
        for &arc_ix in net.outgoing(v).unwrap_or_default() {
            if net.is_cut(arc_ix) || self.is_dropped(arc_ix) {
                continue;
            }
            let Some(arc) = net.arc(arc_ix).copied() else {
                continue;
            };
            let Some(l) = arc.link.filter(|l| !l.to_inactive && !self.actual_target(arc.to)) else {
                continue;
            };
            let Some(tl) = self.late_view(arc.to) else {
                continue;
            };
            let Some(target_scale) = net.node(arc.to).and_then(|n| net.scale(n.place)).copied() else {
                continue;
            };
            let (Ok(lag), Some(sigma)) = (self.lag_minutes(arc_ix), net.scale(l.sigma).copied()) else {
                continue;
            };
            let anchor = match l.ty {
                LinkType::FinishToStart | LinkType::FinishToFinish => a.finish,
                LinkType::StartToStart | LinkType::StartToFinish => a.start,
            };
            let Ok(x) = transfer::forward(sigma, anchor, lag) else {
                continue;
            };
            let broken = match l.ty {
                LinkType::FinishToStart | LinkType::StartToStart => x > tl.start,
                LinkType::FinishToFinish | LinkType::StartToFinish => {
                    target_scale.between(tl.finish, x).is_ok_and(|m| m.get() > 0)
                }
            };
            if broken {
                violated.push(OverriddenBound::Successor {
                    successor: l.succ,
                    link: l.ty,
                });
            }
        }
        for bound in violated {
            if self.role == PassRole::Replay {
                self.emit(Diagnostic::ScheduleInternal {
                    detail: format!("tâche {task} : épingle en repasse, {bound} est violé(e)"),
                });
            } else {
                self.emit(Diagnostic::ConstraintOverride { task, kind, bound });
            }
        }
    }

    /// R-106 (miroir de R-063) : tâche manuelle à rebours, dates saisies
    /// `f` ; `link.manual_not_honored` pour chaque lien sortant propre dont
    /// le plafond est violé (fin en temps ouvré pour FS/FF, début en brut
    /// pour SS/SF), en passe principale seulement.
    fn manual_successors(&mut self, v: NodeIx, uid: TaskUid, scale: TimeScale<'_>, f: Span) {
        if self.role != PassRole::Primary {
            return;
        }
        let Ok(caps) = self.out_caps(v, false) else {
            return;
        };
        let finish_late = |y: Moment| scale.between(y, f.finish).is_ok_and(|m| m.get() > 0);
        let broken = caps
            .finishes
            .iter()
            .filter(|(y, _)| finish_late(*y))
            .chain(caps.starts.iter().filter(|(y, _)| f.start > *y))
            .filter_map(|(_, d)| match d {
                LateDriver::Successor { successor, .. } => Some(*successor),
                _ => None,
            })
            .collect::<Vec<_>>();
        for successor in broken {
            self.emit(Diagnostic::LinkManualNotHonored {
                predecessor: uid,
                successor,
            });
        }
    }

    /// La règle `rule` plafonne la fin au plus tard dans la passe arrière
    /// secondaire depuis le début : SNLT, FNLT (plafond, ou serrage si
    /// l'option est active) ; MSO, MFO option inactive (fenêtre).
    fn caps_late(&self, rule: ConstraintRule) -> bool {
        match rule {
            ConstraintRule::Cap(_) | ConstraintRule::Window(_) => true,
            ConstraintRule::Clamped(_) => self.mode == Mode::FromStart,
            ConstraintRule::None
            | ConstraintRule::Opposite
            | ConstraintRule::Floor(_)
            | ConstraintRule::Pin(_)
            | ConstraintRule::Actual { .. }
            | ConstraintRule::Progress(_) => false,
        }
    }

    /// Plafonds hérités d'un agrégat (R-057) : `C(S) = min(C(parent), FS/FF
    /// sortants, FNLT et échéance de la récapitulative (R-103))` ;
    /// `K(S) = min(SS/SF sortants, K(parent) si ES_S == ES_parent)` (à
    /// rebours : toujours, R-110).
    fn rollup_caps(&mut self, v: NodeIx, early: Option<Span>) -> Result<(), Fail> {
        let parent = self
            .net
            .node(v)
            .ok_or(Fail::Internal("nœud hors bornes"))?
            .parent_rollup;
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let (uid, constraint, deadline) = (node.uid, node.constraint, node.deadline);
        // À rebours, un lien dont le plafond ne se calcule pas est ignoré
        // (`link.unschedulable`), miroir de la porte en passe avant (R-069,
        // R-110).
        let caps = self.out_caps(v, self.mode == Mode::FromFinish)?;
        let mut c = caps.finish().map(|(m, _)| (m, uid));
        // R-103 : FNLT de la récapitulative (`f*` sur son calendrier) et son
        // échéance (brute) contribuent à `C(S)`.
        // Un FNLT dont l'ancrage ne se convertit pas (hors du domaine des
        // dates) est signalé (`task.unschedulable`, R-069, R-111) et seul
        // ignoré : les plafonds des liens et `C(parent)` restent posés.
        if let Some(nc) = constraint
            && let ConstraintRule::Cap(_) = nc.rule
        {
            match nc.span() {
                Ok(a) => c = Some(min_cap(c, (a.finish, uid))),
                // En repasse (R-108), l'échec a déjà été signalé par la
                // passe principale.
                Err(fail) if !self.quiet => self.report(uid, Pass::Backward, fail),
                Err(_) => {}
            }
        }
        if let Some(d) = deadline {
            c = Some(min_cap(c, (d, uid)));
        }
        let mut k: Option<(Moment, TaskUid)> = None;
        for &(y, _) in &caps.starts {
            k = Some(min_cap(k, (y, uid)));
        }
        let mut unreliable = caps.unreliable;
        if let Some(pr) = parent {
            let ps = *self.state(pr).ok_or(Fail::Internal("nœud hors bornes"))?;
            unreliable |= ps.late_degraded && !self.net.is_fixed_rollup(pr);
            if let Some(pc) = ps.cap_finish {
                c = Some(min_cap(c, pc));
            }
            if let Some(pk) = ps.cap_start
                && self.k_applies(ps.early.map(|e| e.start), early.map(|e| e.start))
            {
                k = Some(min_cap(k, pk));
            }
        }
        // R-108 : la repasse arrière fige `K(S)` sur la passe principale.
        // Une tâche ASAP épinglée sur ses dates au plus tôt ne respecte le
        // lien SS/SF depuis la récapitulative que pour son début agrégé (le
        // plus petit début de ses feuilles) : recalculé, `K(S)` plafonnerait
        // aussi les feuilles qui commencent plus tard, au-delà de leurs dates
        // au plus tôt (marge négative sans contrainte).
        let freeze = self.role == PassRole::Replay;
        let s = self.state_mut(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        s.cap_finish = c;
        if !freeze {
            s.cap_start = k;
        }
        s.late_degraded |= unreliable;
        Ok(())
    }

    /// Porte : `LG(S)` = min des débuts au plus tard des enfants actifs
    /// (feuille : LS ; récapitulative : sa porte).
    fn gate_late(&mut self, v: NodeIx) {
        let Some(out) = self.net.outgoing(v) else { return };
        let mut lg: Option<Moment> = None;
        let mut unreliable = false;
        for &a in out {
            let Some(arc) = self.net.arc(a) else { continue };
            if arc.link.is_some() {
                continue;
            }
            let Some(child) = self.net.node(arc.to) else {
                continue;
            };
            // R-203 : une sous-tâche achevée ne plafonne pas la porte (ni,
            // par elle, les prédécesseurs de la récapitulative) ; R-205 :
            // une sous-tâche en cours sans fractionnement non plus.
            if child.inactive || self.actual_target(arc.to) {
                continue;
            }
            unreliable |= self.unreliable(arc.to);
            // R-206 : sous-tâche en cours, début au plus tard du reste.
            let late = if child.in_progress().is_some() {
                self.late_view(arc.to)
            } else {
                self.state(arc.to).and_then(|s| s.late)
            };
            if let Some(l) = late {
                lg = Some(min_opt(lg, l.start));
            }
        }
        if let Some(s) = self.state_mut(v) {
            s.late_degraded |= unreliable;
            if let Some(lg) = lg {
                s.late = Some(Span {
                    start: lg,
                    finish: lg,
                });
            }
        }
    }

    /// Dates au plus tard des récapitulatives actives (R-060) : LS = min LS,
    /// LF = max LF des enfants actifs, des plus intérieures aux plus
    /// extérieures (positions décroissantes). Une récapitulative manuelle
    /// datée garde les siennes, calculées comme pour une feuille (R-064).
    pub(super) fn summary_late(&mut self) {
        for p in (0..self.net.tasks.len()).rev() {
            let Some(info) = self.net.task(p) else { continue };
            if info.role != super::network::Role::Summary || info.inactive() {
                continue;
            }
            let Some(rollup) = info.exit else { continue };
            if self.net.is_fixed_rollup(rollup) {
                continue;
            }
            let mut late: Option<Span> = None;
            let mut unreliable = false;
            let Some(incoming) = self.net.incoming(rollup) else {
                continue;
            };
            for a in incoming {
                let Some(arc) = self.net.arc(a) else { continue };
                unreliable |= self.state(arc.from).is_some_and(|s| s.late_degraded);
                if let Some(l) = self.state(arc.from).and_then(|s| s.late) {
                    late = Some(match late {
                        None => l,
                        Some(s) => Span {
                            start: s.start.min(l.start),
                            finish: s.finish.max(l.finish),
                        },
                    });
                }
            }
            if let Some(s) = pos(rollup).and_then(|i| self.st.get_mut(i)) {
                s.late = late;
                s.late_degraded |= unreliable;
            }
        }
    }
}
