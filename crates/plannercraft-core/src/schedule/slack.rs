//! Marges et critique (spécification J2 § 3.9, R-048 à R-050, R-060,
//! R-067).
//!
//! - Marge totale : `min(M.between(ES, LS), M.between(EF, LF))`, `M` étant
//!   l'échelle de marge (placement de la feuille, temps civil pour une durée
//!   écoulée ; calendrier d'une récapitulative).
//! - Marge libre : `max(0, min(TF, écarts aux successeurs))`, arcs sortants
//!   propres et hérités des récapitulatives ancêtres (R-101 : sans
//!   contrainte ni échéance, la borne à 0 est sans effet).
//! - Critique : `TF ≤ seuil × minutes par jour`.

use super::constraint::{ConstraintRule, Mode, Side};
use super::network::{Fail, LinkArc, NodeIx, NodeKind, pos};
use super::placement::Placer;
use super::{Calc, NodeState, Pass, transfer};
use crate::calendar::{Span, TimeScale};
use crate::diagnostic::Diagnostic;
use crate::duration::Minutes;
use crate::model::LinkType;
use crate::time::Moment;

/// Drapeaux lus par [`Calc::unreliable`] (R-069) : un nœud dont l'un change
/// pendant la passe des marges rend la suite de la passe dépendante de
/// l'ordre de traitement, comme dans la référence.
fn unsafe_flags(s: &NodeState) -> (bool, bool, bool, bool) {
    (s.degraded, s.failed, s.back_failed, s.late_degraded)
}

/// Arc de lien évalué pour la marge libre, indépendamment du nœud mesuré
/// (R-049) : tout sauf l'ancre propre et l'échelle de mesure.
#[derive(Clone, Copy, Debug)]
struct ArcEval {
    /// Lien SS ou SF : l'ancre propre est le début.
    start_kind: bool,
    /// Cible peu sûre à l'entrée de la passe (R-069).
    unreliable: bool,
    /// Borne `h(ancre du successeur)` ; `None` : cible sans dates (l'arc est
    /// lu, sans écart).
    bound: Option<Moment>,
    /// Décalage ou transfert impossible : le nœud passe par la référence.
    failed: bool,
}

/// Arcs FS/FF d'un nœud (propres et hérités des agrégats ancêtres),
/// réduits : plus petite borne, drapeaux.
#[derive(Clone, Copy, Debug, Default)]
struct Inherited {
    /// Plus petite borne.
    bound: Option<Moment>,
    /// Un arc vers une cible peu sûre.
    unreliable: bool,
    /// Un arc qui ne se calcule pas.
    failed: bool,
}

impl Inherited {
    /// Ajoute un arc.
    fn add(&mut self, e: &ArcEval) {
        self.merge(Self {
            bound: e.bound,
            unreliable: e.unreliable,
            failed: e.failed,
        });
    }

    /// Réunit deux réductions.
    fn merge(&mut self, o: Self) {
        self.bound = match (self.bound, o.bound) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.unreliable |= o.unreliable;
        self.failed |= o.failed;
    }
}

/// Index de la passe des marges, stratégie rapide (lot L15).
struct SlackIndex {
    /// Évaluation de chaque arc, par indice d'arc.
    evals: Vec<Option<ArcEval>>,
    /// Par nœud : arcs FS/FF propres et hérités, réduits.
    finish_chain: Vec<Inherited>,
    /// Par nœud : plus proche ancêtre strict (dans la chaîne d'héritage)
    /// porteur d'au moins un arc SS/SF lu.
    start_up: Vec<Option<NodeIx>>,
}

impl SlackIndex {
    /// Évaluation de l'arc `a`.
    fn eval(&self, a: u32) -> Option<ArcEval> {
        pos(a).and_then(|i| self.evals.get(i)).copied().flatten()
    }
}

impl Calc<'_, '_> {
    /// Seuil de criticité en minutes (R-050), saturé.
    pub(super) fn critical_limit(&self) -> Minutes {
        let o = &self.project.options;
        let days = i64::from(o.critical_slack_limit_days);
        let per_day = i64::from(o.factors.minutes_per_day());
        Minutes::new(days.checked_mul(per_day).unwrap_or(i64::MAX))
    }

    /// Marges de toutes les feuilles et récapitulatives actives planifiées.
    ///
    /// Stratégie rapide (lot L15) : les arcs de lien sont évalués une fois
    /// ([`ArcEval`]), les arcs FS/FF propres et hérités réduits par une
    /// propagation descendante unique ([`SlackIndex`]), et seule la remontée
    /// vers les ancêtres porteurs d'arcs SS/SF reste par nœud. Le résultat
    /// est celui de la référence ([`Self::node_slack`]) arc par arc : un
    /// nœud dont un arc ne se calcule pas passe par la référence, et dès
    /// qu'un nœud change de drapeau « peu sûr » (R-069 : la référence le lit
    /// sur les nœuds déjà traités), la suite de la passe aussi.
    pub(super) fn slack(&mut self) {
        let index = match self.strategy {
            super::Strategy::Fast => Some(self.slack_index()),
            super::Strategy::Reference => None,
        };
        let mut dirty = false;
        for k in 0..self.net.nodes.len() {
            let Some(v) = super::network::ix(k) else { continue };
            let Some(node) = self.net.node(v) else { continue };
            if node.inactive || node.kind == NodeKind::Gate {
                continue;
            }
            let uid = node.uid;
            let actual = node.actual().is_some();
            let Some(s) = self.state(v).copied() else { continue };
            let (Some(early), Some(late)) = (s.early, s.late) else {
                continue;
            };
            // R-203 : feuille achevée, TS = FS = 0.
            if actual {
                if let Some(st) = self.state_mut(v) {
                    st.total_slack = Minutes::ZERO;
                    st.free_slack = Minutes::ZERO;
                }
                continue;
            }
            // R-206 : feuille en cours, marge du reste (reprise contre début
            // au plus tard du reste, fin contre fin au plus tard) ; le début
            // réel, fixe, ne compte pas. Seul le premier terme de la marge
            // totale change : la stratégie rapide la calcule aussi (J6-PERF-1).
            let running = node.in_progress().is_some().then_some(s.run).flatten();
            let fast = match &index {
                Some(index) if !dirty => self.node_slack_fast(index, v, early, late, running),
                _ => None,
            };
            let result = match fast {
                Some((tf, fs, unreliable)) => {
                    if unreliable && let Some(st) = self.state_mut(v) {
                        st.late_degraded = true;
                    }
                    Ok((tf, fs))
                }
                None => self.node_slack(v, early, late, running),
            };
            match result {
                Ok((tf, fs)) => {
                    if let Some(st) = self.state_mut(v) {
                        st.total_slack = tf;
                        st.free_slack = fs;
                    }
                }
                Err(fail) => {
                    if let Some(st) = self.state_mut(v) {
                        st.degraded = true;
                    }
                    self.report(uid, Pass::Backward, fail);
                }
            }
            if self.state(v).map(unsafe_flags) != Some(unsafe_flags(&s)) {
                dirty = true;
            }
        }
    }

    /// Évaluation de l'arc `a` indépendante du nœud mesuré (miroir de
    /// [`Self::gap`] sans l'ancre propre) ; `None` pour un arc ignoré
    /// (hiérarchie, coupé, abandonné, vers une tâche inactive).
    fn arc_eval(&mut self, a: u32) -> Option<ArcEval> {
        if self.net.is_cut(a) || self.is_dropped(a) {
            return None;
        }
        let arc = *self.net.arc(a)?;
        let l = arc.link?;
        // R-203 : un lien vers une feuille achevée ne compte pas.
        if l.to_inactive || self.actual_target(arc.to) {
            return None;
        }
        let start_kind = matches!(l.ty, LinkType::StartToStart | LinkType::StartToFinish);
        let unreliable = self.unreliable(arc.to);
        let (bound, failed) = match self.arc_bound(a, arc.to, l) {
            Ok(b) => (b, false),
            Err(_) => (None, true),
        };
        Some(ArcEval {
            start_kind,
            unreliable,
            bound,
            failed,
        })
    }

    /// Borne `h(ancre au plus tôt du successeur)` de l'arc `a` (R-049),
    /// `None` si la cible n'a pas de dates (porte en échec, agrégat vide).
    fn arc_bound(&mut self, a: u32, to: NodeIx, l: LinkArc) -> Result<Option<Moment>, Fail> {
        let target = self.net.node(to).ok_or(Fail::Internal("nœud hors bornes"))?;
        let te = match target.kind {
            NodeKind::Gate if self.state(to).is_some_and(|s| s.early.is_none()) => None,
            NodeKind::Gate => target.twin.and_then(|r| self.state(r)).and_then(|s| s.children),
            _ => self.early_view(to),
        };
        let Some(te) = te else { return Ok(None) };
        let anchor = match l.ty {
            LinkType::FinishToStart | LinkType::StartToStart => te.start,
            LinkType::FinishToFinish | LinkType::StartToFinish => self.finish_anchor(to, te.finish),
        };
        let lag = self.lag_minutes(a)?;
        let sigma = self
            .net
            .scale(l.sigma)
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        Ok(Some(transfer::backward(
            *sigma,
            anchor,
            lag,
            self.reads_raw(to, l.sigma),
        )?))
    }

    /// Agrégat de la récapitulative parente dont le nœud `v` hérite les
    /// arcs (comme la référence : pas au-delà d'une récapitulative manuelle
    /// datée, R-064).
    fn chain_parent(&self, v: NodeIx) -> Option<NodeIx> {
        self.net
            .node(v)
            .and_then(|n| n.parent_rollup)
            .filter(|&r| !self.net.is_fixed_rollup(r))
    }

    /// Index de la stratégie rapide : évaluation de chaque arc, réduction
    /// des arcs FS/FF propres et hérités de chaque nœud, plus proche ancêtre
    /// porteur d'arcs SS/SF. Coût linéaire en nœuds et en arcs.
    fn slack_index(&mut self) -> SlackIndex {
        let net = self.net;
        let evals: Vec<Option<ArcEval>> = (0..net.arcs.len())
            .map(|a| super::network::ix(a).and_then(|a| self.arc_eval(a)))
            .collect();
        let eval = |a: u32| pos(a).and_then(|i| evals.get(i)).copied().flatten();
        let n = net.nodes.len();
        let mut own = vec![Inherited::default(); n];
        let mut has_start = vec![false; n];
        for (k, (o, h)) in own.iter_mut().zip(has_start.iter_mut()).enumerate() {
            let Some(v) = super::network::ix(k) else { continue };
            for &a in net.outgoing(v).unwrap_or_default() {
                match eval(a) {
                    Some(e) if e.start_kind => *h = true,
                    Some(e) => o.add(&e),
                    None => {}
                }
            }
        }
        // Propagation descendante, ancêtres d'abord (le chemin vers le
        // premier nœud déjà résolu est empilé) ; une chaîne plus longue que
        // le nombre de nœuds (impossible : la hiérarchie est un arbre) est
        // coupée.
        let mut finish_chain = vec![Inherited::default(); n];
        let mut start_up: Vec<Option<NodeIx>> = vec![None; n];
        let mut done = vec![false; n];
        let mut path: Vec<usize> = Vec::new();
        for k in 0..n {
            let mut cur = Some(k);
            while let Some(c) = cur {
                if done.get(c).copied().unwrap_or(true) || path.len() > n {
                    break;
                }
                path.push(c);
                cur = super::network::ix(c)
                    .and_then(|c| self.chain_parent(c))
                    .and_then(pos);
            }
            while let Some(c) = path.pop() {
                let parent = super::network::ix(c).and_then(|c| self.chain_parent(c));
                let resolved = parent
                    .and_then(|p| pos(p).map(|i| (p, i)))
                    .filter(|&(_, i)| done.get(i).copied().unwrap_or(false));
                let (inherited, up) = match resolved {
                    Some((p, i)) => (
                        finish_chain.get(i).copied().unwrap_or_default(),
                        if has_start.get(i).copied().unwrap_or(false) {
                            Some(p)
                        } else {
                            start_up.get(i).copied().flatten()
                        },
                    ),
                    None => (Inherited::default(), None),
                };
                let mut chain = own.get(c).copied().unwrap_or_default();
                chain.merge(inherited);
                if let Some(slot) = finish_chain.get_mut(c) {
                    *slot = chain;
                }
                if let Some(slot) = start_up.get_mut(c) {
                    *slot = up;
                }
                if let Some(slot) = done.get_mut(c) {
                    *slot = true;
                }
            }
        }
        SlackIndex {
            evals,
            finish_chain,
            start_up,
        }
    }

    /// Marge totale, marge libre et drapeau « successeur peu sûr » du nœud
    /// `v` par l'index (stratégie rapide), égaux à ceux de
    /// [`Self::node_slack`] ; `None` si un arc ou une conversion échoue :
    /// l'appelant passe alors par la référence, qui signale l'échec.
    ///
    /// L'écart d'un arc FS/FF est `M.between(EF, borne)`, croissant en la
    /// borne (temps ouvré cumulé) : le minimum des écarts est l'écart à la
    /// plus petite borne, propre ou héritée. Un arc SS/SF dépend du nœud
    /// (dernier début plaçable de sa durée) et ne compte, hérité, que pour
    /// un nœud de même début que l'ancêtre : seuls les ancêtres porteurs
    /// d'arcs SS/SF sont visités.
    fn node_slack_fast(
        &self,
        index: &SlackIndex,
        v: NodeIx,
        early: Span,
        late: Span,
        running: Option<super::Run>,
    ) -> Option<(Minutes, Minutes, bool)> {
        let node = self.net.node(v)?;
        let m = *self.net.scale(node.measure)?;
        let place = self.placer(v).ok()?;
        // R-206 : feuille en cours, reprise contre début au plus tard du
        // reste (même terme que la référence) ; les arcs restent ceux de
        // l'index, mesurés depuis les dates au plus tôt.
        let (es, ls) = match running {
            Some(run) => (run.resume, run.late_resume.unwrap_or(run.resume)),
            None => (early.start, late.start),
        };
        let tf = m
            .between(es, ls)
            .ok()?
            .min(m.between(early.finish, late.finish).ok()?);
        let chain = index.finish_chain.get(pos(v)?)?;
        if chain.failed {
            return None;
        }
        let mut fs = tf;
        let mut unreliable = chain.unreliable;
        if let Some(b) = chain.bound {
            fs = fs.min(m.between(early.finish, b).ok()?);
        }
        let converts = node.kind == NodeKind::Leaf || node.fixed.is_some();
        let starts = |o: NodeIx, fs: &mut Minutes, unreliable: &mut bool| -> Option<()> {
            for &a in self.net.outgoing(o).unwrap_or_default() {
                let Some(e) = index.eval(a) else { continue };
                if !e.start_kind {
                    continue;
                }
                if e.failed {
                    return None;
                }
                *unreliable |= e.unreliable;
                if let Some(b) = e.bound {
                    let b = if converts { place.latest_start(b).ok()? } else { b };
                    *fs = (*fs).min(m.between(early.start, b).ok()?);
                }
            }
            Some(())
        };
        starts(v, &mut fs, &mut unreliable)?;
        let mut up = index.start_up.get(pos(v)?).copied().flatten();
        let mut steps = 0usize;
        while let Some(o) = up {
            let anc_start = self.state(o).and_then(|s| s.early).map(|e| e.start);
            if anc_start == Some(early.start) {
                starts(o, &mut fs, &mut unreliable)?;
            }
            up = index.start_up.get(pos(o)?).copied().flatten();
            steps = steps.checked_add(1)?;
            if steps > self.net.nodes.len() {
                return None;
            }
        }
        Some((tf, fs.min(tf).max(Minutes::ZERO), unreliable))
    }

    /// Marge totale et marge libre du nœud `v` (référence, J3 : remontée
    /// des ancêtres et de leurs arcs sortants) ; oracle de la stratégie
    /// rapide et son repli.
    fn node_slack(
        &mut self,
        v: NodeIx,
        early: Span,
        late: Span,
        running: Option<super::Run>,
    ) -> Result<(Minutes, Minutes), Fail> {
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        // Échelle de mesure (R-048, R-113) ; le dernier début plaçable d'un
        // lien SS/SF reste sur l'échelle de placement.
        let m = *self
            .net
            .scale(node.measure)
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        let place = self.placer(v)?;
        let (es, ls) = match running {
            Some(run) => (run.resume, run.late_resume.unwrap_or(run.resume)),
            None => (early.start, late.start),
        };
        let tf = m.between(es, ls)?.min(m.between(early.finish, late.finish)?);
        let mut fs = tf;
        // Arcs propres, puis ceux des agrégats ancêtres (§ 3.9) ; un arc SS
        // ou SF hérité ne compte que pour un nœud de même début que
        // l'ancêtre (comme son plafond en passe arrière, A11).
        let net = self.net;
        let mut owner = Some(v);
        let mut first = true;
        while let Some(o) = owner {
            let anc_start = self.state(o).and_then(|s| s.early).map(|e| e.start);
            for &a in net.outgoing(o).unwrap_or_default() {
                if let Some(gap) =
                    self.gap(v, a, m, place, early, !first && anc_start != Some(early.start))?
                {
                    fs = fs.min(gap);
                }
            }
            // Une récapitulative manuelle datée (R-064) ne dépend pas de ses
            // enfants : ses liens et ceux de ses ancêtres ne les concernent pas.
            owner = self
                .net
                .node(o)
                .and_then(|n| n.parent_rollup)
                .filter(|&r| !self.net.is_fixed_rollup(r));
            first = false;
        }
        // R-101 : marge libre bornée à 0 (Q10 : MS Project affiche 0 quand
        // la marge totale est négative) ; sans contrainte ni échéance, la
        // borne est sans effet (FS ≥ 0 reste une propriété, P13).
        Ok((tf, fs.min(tf).max(Minutes::ZERO)))
    }

    /// Échéances dépassées (R-102) et contraintes non tenues (R-099, R-103,
    /// R-109), après les marges, sur les dates planifiées (au plus tôt depuis
    /// le début, au plus tard à rebours ; une porte lit les dates de sa
    /// récapitulative). Une échéance est dépassée si la fin planifiée la
    /// dépasse en temps ouvré de l'échelle. Une contrainte n'est pas tenue
    /// (`constraint.not_honored`, avec la date saisie) : depuis le début, un
    /// plafond ou une fenêtre (option inactive) dont la date planifiée
    /// dépasse l'ancrage ; à rebours, un plancher ou une fenêtre dont la date
    /// planifiée le précède, ou un plafond dépassé (FNLT d'une
    /// récapitulative). Début comparé en brut, fin en temps ouvré ; la porte
    /// d'une récapitulative (SNET, R-103) compare son début à la date brute.
    pub(super) fn check_constraints(&mut self) {
        let from_finish = self.mode == Mode::FromFinish;
        let planned_of = |s: &super::NodeState| if from_finish { s.late } else { s.early };
        let mut found = Vec::new();
        for (node, s) in self.net.nodes.iter().zip(&self.st) {
            let planned = if node.kind == NodeKind::Gate {
                node.twin.and_then(|r| self.state(r)).and_then(planned_of)
            } else {
                planned_of(s)
            };
            let (Some(planned), Some(scale)) = (planned, self.net.scale(node.place)) else {
                continue;
            };
            let after = |a: crate::time::Moment, b: crate::time::Moment| {
                scale.between(a, b).is_ok_and(|m| m.get() > 0)
            };
            // R-102 : échéance dépassée, jugée sur la fin planifiée, en temps
            // ouvré de l'échelle.
            if let Some(deadline) = node.deadline
                && after(deadline, planned.finish)
            {
                found.push(Diagnostic::TaskDeadlineMissed {
                    task: node.uid,
                    deadline,
                    finish: planned.finish,
                });
            }
            let Some(c) = node.constraint else { continue };
            let Some(date) = c.date else { continue };
            let a = match c.anchor {
                Some(Ok(a)) => a,
                // Porte d'une récapitulative : date brute (R-103).
                None => Span {
                    start: date,
                    finish: date,
                },
                Some(Err(_)) => continue,
            };
            let exceeds = |side: Side| match side {
                Side::Start => planned.start > a.start,
                Side::Finish => after(a.finish, planned.finish),
            };
            let precedes = |side: Side| match side {
                Side::Start => planned.start < a.start,
                Side::Finish => after(planned.finish, a.finish),
            };
            let violated = match (c.rule, from_finish) {
                (ConstraintRule::Cap(side) | ConstraintRule::Window(side), false)
                | (ConstraintRule::Cap(side), true) => exceeds(side),
                (ConstraintRule::Floor(side) | ConstraintRule::Window(side), true) => precedes(side),
                _ => false,
            };
            if violated {
                found.push(Diagnostic::ConstraintNotHonored {
                    task: node.uid,
                    kind: c.kind,
                    date,
                });
            }
        }
        for d in found {
            self.diags.push(d);
        }
    }

    /// Écart de l'arc de lien `a` pour le nœud `v` (R-049) : `M.between(ancre propre,
    /// h(ancre au plus tôt du successeur))`, `M` échelle de mesure, `place`
    /// échelle de placement (dernier début plaçable, R-113). `None` pour un arc ignoré
    /// (hiérarchie, coupé, vers une tâche inactive, cible sans dates, ou arc
    /// SS/SF hérité écarté).
    fn gap(
        &mut self,
        v: NodeIx,
        a: u32,
        m: TimeScale<'_>,
        place: Placer<'_, '_>,
        own: Span,
        skip_start_kind: bool,
    ) -> Result<Option<Minutes>, Fail> {
        if self.net.is_cut(a) || self.is_dropped(a) {
            return Ok(None);
        }
        let arc = *self.net.arc(a).ok_or(Fail::Internal("arc hors bornes"))?;
        let Some(l) = arc.link else { return Ok(None) };
        let start_kind = matches!(l.ty, LinkType::StartToStart | LinkType::StartToFinish);
        if l.to_inactive || (start_kind && skip_start_kind) || self.actual_target(arc.to) {
            return Ok(None);
        }
        // Successeur peu sûr : marge libre de repli (R-069).
        if self.unreliable(arc.to)
            && let Some(s) = self.state_mut(v)
        {
            s.late_degraded = true;
        }
        let target = self.net.node(arc.to).ok_or(Fail::Internal("nœud hors bornes"))?;
        // Cible porte : début agrégé des sous-tâches (min ES des enfants, même
        // sous une récapitulative manuelle datée, R-064).
        // Porte en échec (R-069) : le lien n'a pas été appliqué.
        let te = match target.kind {
            NodeKind::Gate if self.state(arc.to).is_some_and(|s| s.early.is_none()) => None,
            NodeKind::Gate => target.twin.and_then(|r| self.state(r)).and_then(|s| s.children),
            _ => self.early_view(arc.to),
        };
        let Some(te) = te else { return Ok(None) };
        let anchor = match l.ty {
            LinkType::FinishToStart | LinkType::StartToStart => te.start,
            LinkType::FinishToFinish | LinkType::StartToFinish => self.finish_anchor(arc.to, te.finish),
        };
        let lag = self.lag_minutes(a)?;
        let sigma = self
            .net
            .scale(l.sigma)
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        let bound = transfer::backward(*sigma, anchor, lag, self.reads_raw(arc.to, l.sigma))?;
        let (mine, bound) = match l.ty {
            LinkType::FinishToStart | LinkType::FinishToFinish => (own.finish, bound),
            // Borne de début d'une feuille : dernier début plaçable (R-047,
            // R-049), comme son plafond en passe arrière.
            LinkType::StartToStart | LinkType::StartToFinish => {
                let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
                let bound = if node.kind == NodeKind::Leaf || node.fixed.is_some() {
                    place.latest_start(bound)?
                } else {
                    bound
                };
                (own.start, bound)
            }
        };
        Ok(Some(m.between(mine, bound)?))
    }
}
