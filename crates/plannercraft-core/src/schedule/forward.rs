//! Passe avant (spécification J2 § 3.7, R-036 à R-045, R-062, R-063,
//! R-069 ; spécification J3 § 3.7, R-090 à R-110), dans l'ordre
//! topologique. Depuis le début, c'est la passe principale (puis sa repasse,
//! R-092) ; à rebours, la passe secondaire, depuis le début calculé `S*`
//! (R-107).
//!
//! Chaque arc entrant donne une borne candidate ; la plus grande en
//! `Moment` brut gagne, et à égalité la première atteinte reste pilote
//! (R-044 : début du projet, porte du parent, liens dans l'ordre de
//! stockage).

use super::constraint::{ConstraintRule, InProgress, Mode, Side};
use super::network::{Fail, NodeIx, NodeKind};
use super::{Calc, Driver, Pass, PassRole, transfer};
use crate::calendar::{Span, TimeScale};
use crate::diagnostic::{Diagnostic, OverriddenBound};
use crate::duration::Minutes;
use crate::ids::TaskUid;
use crate::model::{ConstraintKind, LinkType};
use crate::time::Moment;

/// Bornes d'un nœud : plus grande borne de début et son pilote, drapeau
/// `degraded` transmis par les arcs entrants, présence d'un prédécesseur
/// effectif (lien appliqué, propre ou hérité d'une porte, R-110).
struct Bounds {
    start: Moment,
    driver: Driver,
    degraded: bool,
    linked: bool,
    /// Plus grande borne de début seule (début du projet, porte, FS, SS) et
    /// son pilote : origine du retard de nivellement (R-178).
    start_side: (Moment, Driver),
    /// Plus grande borne de fin (FF, SF) convertie en début, et son pilote :
    /// elle porte sur le début retardé (R-178).
    finish_side: Option<(Moment, Driver)>,
}

/// Une borne candidate.
#[derive(Clone, Copy)]
enum Candidate {
    /// Borne de début (porte, FS, SS).
    Start(Moment),
    /// Borne de fin (FF, SF), à convertir en début (R-039).
    Finish(Moment),
}

impl Calc<'_, '_> {
    /// Passe avant complète, puis fin du projet (R-045). Depuis le début,
    /// c'est la passe principale (`role` = `Primary`) ou sa repasse
    /// (`Replay`, R-092) ; à rebours, la passe secondaire (`Secondary`,
    /// R-107) : les tâches exclues par la passe arrière principale (R-111)
    /// sont sautées, une conversion impossible donne des dates au plus tôt
    /// de repli égales aux dates au plus tard (miroir de R-069), et la fin
    /// du projet reste l'ancre `F`.
    pub(super) fn forward(&mut self, order: &[NodeIx], role: PassRole) {
        self.role = role;
        self.quiet = role == PassRole::Replay;
        for &v in order {
            let Some(node) = self.net.node(v) else {
                self.emit(Diagnostic::ScheduleInternal {
                    detail: "nœud hors bornes en passe avant".to_owned(),
                });
                continue;
            };
            let (kind, uid) = (node.kind, node.uid);
            // Repasse : un nœud en échec dans la passe principale le reste
            // (ses voisins l'ont déjà ignoré) ; à rebours, une tâche exclue
            // par la passe arrière principale n'a pas de dates (R-111).
            if self
                .state(v)
                .is_some_and(|s| (self.quiet && s.failed) || s.back_failed)
            {
                continue;
            }
            let result = match kind {
                NodeKind::Gate => self.gate(v),
                NodeKind::Leaf => self.leaf(v),
                NodeKind::Rollup => self.rollup(v),
            };
            if let Err(fail) = result {
                let secondary = role == PassRole::Secondary;
                if let Some(s) = self.state_mut(v) {
                    if secondary {
                        // Miroir de R-069 à rebours : dates au plus tôt de
                        // repli, égales aux dates au plus tard.
                        s.early = s.late;
                        s.degraded = true;
                    } else {
                        s.early = None;
                        s.failed = true;
                    }
                }
                self.report(uid, Pass::Forward, fail);
            }
        }
        // Repasse : la fin du projet de la passe principale est gardée (les
        // épingles ne la déplacent pas, R-092) ; à rebours, la fin du projet
        // est l'ancre `F` (R-106).
        self.quiet = false;
        if role != PassRole::Primary {
            return;
        }
        // R-045 : fin du projet = max des fins au plus tôt des feuilles
        // actives planifiées (manuelles comprises, inactives exclues) et des
        // récapitulatives manuelles datées (R-064) ; sans tâche planifiée, le
        // début du projet. Ce max peut précéder le début du projet (tâche
        // manuelle ou contrainte respectée avant lui, R-105).
        let mut finish: Option<Moment> = None;
        for (node, s) in self.net.nodes.iter().zip(&self.st) {
            if (node.kind == NodeKind::Leaf || node.fixed.is_some())
                && !node.inactive
                && let Some(e) = s.early
            {
                finish = Some(finish.map_or(e.finish, |f| f.max(e.finish)));
            }
        }
        self.finish = finish.unwrap_or(self.start);
    }

    /// Bornes de début du nœud `v` (R-036, R-044). Pour une feuille
    /// manuelle (`fixed`), chaque borne violée donne
    /// `link.manual_not_honored` (R-063) et aucune conversion n'est faite.
    fn bounds(&mut self, v: NodeIx, fixed: Option<Span>) -> Result<Bounds, Fail> {
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let (place, kind) = (node.place, node.kind);
        let mut b = Bounds {
            start: self.start,
            driver: Driver::ProjectStart,
            degraded: false,
            linked: false,
            start_side: (self.start, Driver::ProjectStart),
            finish_side: None,
        };
        let incoming = self
            .net
            .incoming(v)
            .ok_or(Fail::Internal("arcs entrants hors bornes"))?;
        for a in incoming {
            // Arc coupé (R-065) ; lien non appliqué par la passe arrière
            // principale à rebours (R-110).
            if self.net.is_cut(a) || (self.role == PassRole::Secondary && self.is_dropped(a)) {
                continue;
            }
            let arc = *self.net.arc(a).ok_or(Fail::Internal("arc hors bornes"))?;
            let src = *self.state(arc.from).ok_or(Fail::Internal("nœud hors bornes"))?;
            let Some(se) = src.early else {
                // Origine non planifiée (R-069, R-111) ou récapitulative vide.
                if let Some(l) = arc.link {
                    self.emit(Diagnostic::LinkExcludedEndpoint {
                        predecessor: l.pred,
                        successor: l.succ,
                    });
                }
                b.degraded |= src.failed || src.back_failed || src.degraded;
                continue;
            };
            b.degraded |= src.degraded;
            let (cand, driver) = match arc.link {
                None => {
                    let summary = self
                        .net
                        .node(arc.from)
                        .ok_or(Fail::Internal("nœud hors bornes"))?
                        .uid;
                    (Candidate::Start(se.start), Driver::SummaryGate { summary })
                }
                Some(l) => {
                    let x = match self.link_bound(a, l, se) {
                        Ok(x) => x,
                        // R-069 : la borne d'un lien vers une récapitulative
                        // ne se calcule pas : le lien est ignoré, la porte
                        // garde ses autres bornes, dégradée.
                        Err(Fail::Conversion(cause)) if kind == NodeKind::Gate => {
                            self.drop_link(a, l, cause);
                            b.degraded = true;
                            continue;
                        }
                        Err(fail) => return Err(fail),
                    };
                    let cand = match l.ty {
                        LinkType::FinishToStart | LinkType::StartToStart => Candidate::Start(x),
                        LinkType::FinishToFinish | LinkType::StartToFinish => Candidate::Finish(x),
                    };
                    (
                        cand,
                        Driver::Link {
                            predecessor: l.pred,
                            link: l.ty,
                        },
                    )
                }
            };
            // R-110 : prédécesseur effectif (lien appliqué), propre ou hérité
            // de la porte du parent.
            b.linked |= arc.link.is_some() || src.linked;
            let scale = self
                .net
                .scale(place)
                .ok_or(Fail::Internal("échelle hors bornes"))?;
            let start = match (cand, fixed) {
                // Feuille manuelle : la borne ne la déplace pas (R-063).
                (_, Some(f)) => {
                    self.manual_bound(arc.link, cand, f, *scale)?;
                    continue;
                }
                // R-039, R-040 : borne de fin convertie en borne de début
                // (feuille affectée : min des débuts de ses parts, R-174) ;
                // feuille en cours : début du reste qui finit en `x` (R-205).
                (Candidate::Finish(x), None) if kind == NodeKind::Leaf => {
                    self.remaining_placer(v)?.backward_span(x)?.start
                }
                (Candidate::Start(x) | Candidate::Finish(x), None) => x,
            };
            match cand {
                Candidate::Start(_) if start > b.start_side.0 => b.start_side = (start, driver),
                Candidate::Finish(_) if b.finish_side.is_none_or(|(f, _)| start > f) => {
                    b.finish_side = Some((start, driver));
                }
                Candidate::Start(_) | Candidate::Finish(_) => {}
            }
            if start > b.start {
                b.start = start;
                b.driver = driver;
            }
        }
        Ok(b)
    }

    /// R-063 : borne `cand` d'un lien entrant d'une feuille manuelle de
    /// dates saisies `f` : elle ne la déplace pas ; si elle est violée (début
    /// en brut, fin en temps ouvré), `link.manual_not_honored`, signalé par
    /// la passe principale seulement (à rebours, par la passe arrière
    /// principale, sur les liens sortants, R-106).
    fn manual_bound(
        &mut self,
        link: Option<super::network::LinkArc>,
        cand: Candidate,
        f: Span,
        scale: TimeScale<'_>,
    ) -> Result<(), Fail> {
        let Some(l) = link.filter(|_| self.role == PassRole::Primary) else {
            return Ok(());
        };
        let violated = match cand {
            Candidate::Start(x) => x > f.start,
            Candidate::Finish(x) => scale.between(f.finish, x)?.get() > 0,
        };
        if violated {
            self.emit(Diagnostic::LinkManualNotHonored {
                predecessor: l.pred,
                successor: l.succ,
            });
        }
        Ok(())
    }

    /// Borne `g(ancre)` de l'arc de lien `a` (R-037 à R-043) : fin ou début
    /// du prédécesseur selon le type, transférée par le décalage.
    fn link_bound(&mut self, a: u32, l: super::network::LinkArc, se: Span) -> Result<Moment, Fail> {
        let lag = self.lag_minutes(a)?;
        let sigma = self
            .net
            .scale(l.sigma)
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        let anchor = match l.ty {
            LinkType::FinishToStart | LinkType::FinishToFinish => se.finish,
            LinkType::StartToStart | LinkType::StartToFinish => se.start,
        };
        Ok(transfer::forward(*sigma, anchor, lag)?)
    }

    /// R-069 : arc de lien `a` vers une porte non appliqué
    /// (`link.unschedulable`).
    fn drop_link(&mut self, a: u32, l: super::network::LinkArc, cause: &str) {
        if let Some(slot) = super::network::pos(a).and_then(|i| self.dropped.get_mut(i)) {
            *slot = true;
        }
        self.emit(Diagnostic::LinkUnschedulable {
            predecessor: l.pred,
            successor: l.succ,
            cause: cause.to_owned(),
        });
    }

    /// Porte d'une récapitulative : plus grande borne de début (début du
    /// projet, porte du parent, liens FS/SS entrants, SNET de la
    /// récapitulative), non recalée.
    fn gate(&mut self, v: NodeIx) -> Result<(), Fail> {
        let mut b = self.bounds(v, None)?;
        // R-103 : SNET d'une récapitulative, plancher brut de sa porte (non
        // recalé, comme une borne de lien, R-056), candidat en dernier.
        if let Some(c) = self.net.node(v).and_then(|n| n.constraint)
            && let (ConstraintRule::Floor(Side::Start), Some(date)) = (c.rule, c.date)
            && date > b.start
        {
            b.start = date;
            b.driver = Driver::Constraint { kind: c.kind };
        }
        let replay = self.role == PassRole::Replay;
        let s = self.state_mut(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        s.early = Some(Span {
            start: b.start,
            finish: b.start,
        });
        s.driver = b.driver;
        s.driver_bound = Some(b.start);
        s.degraded = b.degraded || (replay && s.degraded);
        s.linked = b.linked;
        Ok(())
    }

    /// Feuille : automatique (R-030, R-031 sur la borne), manuelle (dates
    /// saisies, R-063) ou inactive (même calcul qu'une automatique, pour
    /// l'affichage, R-062).
    fn leaf(&mut self, v: NodeIx) -> Result<(), Fail> {
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        if let Some((start, finish)) = node.actual() {
            return self.actual_leaf(v, start, finish);
        }
        if let Some(ip) = node.in_progress() {
            return self.running_leaf(v, ip);
        }
        let (place, constraint) = (node.place, node.constraint);
        let fixed = match node.fixed {
            Some(r) => Some(r?),
            None => None,
        };
        let mut b = self.bounds(v, fixed)?;
        let (pin, late) = self.state(v).map_or((None, None), |s| (s.pin, s.late));
        // R-178 : retard de nivellement de la tâche (feuille automatique non
        // épinglée).
        let undelayed = if fixed.is_none() && pin.is_none() {
            self.level(v, &mut b)?
        } else {
            None
        };
        // R-230 : borne brute du pilote, avant le retard de nivellement.
        let mut bound = undelayed.map_or(b.start, |(_, _, raw)| raw);
        // R-110 (miroir de R-051) : à rebours, option « plusieurs chemins
        // critiques » active, une feuille automatique sans prédécesseur
        // effectif commence au plus tôt à son début au plus tard.
        if self.mode == Mode::FromFinish
            && self.project.options.multiple_critical_paths
            && fixed.is_none()
            && !b.linked
            && let Some(l) = late
            && l.start > b.start
        {
            b.start = l.start;
            b.driver = Driver::ProjectStart;
            bound = l.start;
        }
        let (early, driver) = if let Some(f) = fixed {
            (f, Driver::Manual)
        } else {
            let scale = *self
                .net
                .scale(place)
                .ok_or(Fail::Internal("échelle hors bornes"))?;
            // R-092 : épingle d'une tâche ALAP, tenue pour fixe en repasse ;
            // une borne violée est un invariant violé.
            if let (Some(p), Some(c)) = (pin, constraint) {
                self.overrides(v, scale, p, c.kind, true)?;
                (p, Driver::Constraint { kind: c.kind })
            } else {
                // R-173 : feuille affectée placée par ses parts.
                let natural = self.placer(v)?.forward_span(b.start)?;
                self.constrain(v, scale, natural, b.driver)?
            }
        };
        // R-230 : pas de borne pour une manuelle ; une contrainte qui pilote
        // donne sa date (type début) ou le début de son ancrage (type fin,
        // converti comme une borne FF/SF) ; sinon la plus grande borne.
        let driver_bound = match (driver, constraint) {
            (Driver::Manual, _) => None,
            (Driver::Constraint { .. }, Some(c)) => match c.date {
                Some(date) if c.kind.is_start_side() && pin.is_none() => Some(date),
                _ => Some(early.start),
            },
            _ => Some(bound),
        };
        let replay = self.role == PassRole::Replay;
        let s = self.state_mut(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        s.early = Some(early);
        s.undelayed = undelayed.map(|(u, _, _)| u.min(early.start));
        s.held = undelayed.map(|(_, h, _)| h);
        s.driver = driver;
        s.driver_bound = driver_bound;
        // Repasse : les drapeaux posés par la passe arrière sont gardés.
        s.degraded = b.degraded || (replay && s.degraded);
        s.linked = b.linked;
        Ok(())
    }

    /// R-178 : retard de nivellement enregistré de la feuille `v`. Début
    /// non retardé `ES` = plus grande borne de début (début du projet, porte,
    /// FS, SS) ; début retardé `ES'` = `ES` + retard, écoulé ou ouvré sur
    /// l'échelle de placement ; une borne de fin (FF, SF), convertie en
    /// début, porte sur `ES'` (A6 UID 20, O27). Rend `ES` recalé sur le
    /// placement (début au plus tôt affiché) et le placement sans retard
    /// depuis toutes les bornes (plafond des prédécesseurs en passe arrière)
    /// et la borne brute du pilote retenu (R-230 : `ES` brut, ou la borne de
    /// fin convertie si elle l'emporte), `None` sans retard.
    fn level(&self, v: NodeIx, b: &mut Bounds) -> Result<Option<(Moment, Span, Moment)>, Fail> {
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let Some(delay) = self
            .project
            .tasks
            .at(node.task)
            .and_then(|t| t.leveling_delay)
            .filter(|l| l.minutes > crate::duration::Minutes::ZERO)
        else {
            return Ok(None);
        };
        let scale = if delay.elapsed {
            TimeScale::Elapsed
        } else {
            *self
                .net
                .scale(node.place)
                .ok_or(Fail::Internal("échelle hors bornes"))?
        };
        let placer = self.placer(v)?;
        let held = placer.forward_span(b.start)?;
        let (es, es_driver) = b.start_side;
        let delayed = scale.add(es, delay.minutes)?;
        let raw;
        (b.start, b.driver, raw) = match b.finish_side {
            Some((f, d)) if f > delayed => (f, d, f),
            _ => (delayed, es_driver, es),
        };
        Ok(Some((placer.forward_span(es)?.start, held, raw)))
    }

    /// R-203 : feuille achevée, épinglée sur ses dates réelles brutes
    /// (`Driver::Actual`, aucune borne de pilote) ; liens entrants, porte,
    /// contrainte et retard de nivellement ignorés. Début réel implicite
    /// (R-200) : début placé depuis les bornes du réseau, résolu une fois
    /// (passe principale) puis gardé, `progress.actual_start_missing`.
    /// Chaque lien entrant dont la borne est violée donne
    /// `progress.link_overridden` (hors repasse), jamais
    /// `link.manual_not_honored`.
    fn actual_leaf(&mut self, v: NodeIx, start: Option<Moment>, finish: Moment) -> Result<(), Fail> {
        let stored = self.state(v).and_then(|s| s.actual);
        // Bornes : lues pour le début implicite et pour les liens violés ;
        // une borne qui ne se calcule pas ne fait échouer qu'un début
        // implicite.
        let bounds = if start.is_none() && stored.is_none() {
            Some(self.bounds(v, None)?)
        } else {
            self.bounds(v, None).ok()
        };
        let span = match (stored, start) {
            (Some(span), _) => span,
            (None, Some(s)) => Span { start: s, finish },
            (None, None) => {
                let from = bounds.as_ref().map_or(self.start, |b| b.start);
                let natural = self.placer(v)?.forward_span(from)?;
                Span {
                    start: natural.start.min(finish),
                    finish,
                }
            }
        };
        let uid = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?.uid;
        if start.is_none() {
            self.emit(Diagnostic::ProgressActualStartMissing { task: uid });
        }
        if self.role != PassRole::Replay {
            self.links_overridden(v, span)?;
        }
        let degraded = start.is_none() && bounds.as_ref().is_some_and(|b| b.degraded);
        let linked = bounds.as_ref().is_some_and(|b| b.linked);
        let s = self.state_mut(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        s.early = Some(span);
        s.actual = Some(span);
        s.undelayed = None;
        s.held = None;
        s.driver = Driver::Actual;
        s.driver_bound = None;
        s.degraded = degraded;
        s.linked = linked;
        Ok(())
    }

    /// R-204, R-205 : feuille automatique en cours, une seule coupure
    /// (ADR-0014 D2). Début réel saisi, ou implicite (R-200 : placement
    /// depuis les bornes de début du réseau, résolu une fois par la passe
    /// principale puis gardé, `progress.actual_start_missing`) ; `Stop` = fin
    /// de la partie réelle placée depuis le début réel sur l'échelle de
    /// placement (début réel si la durée réelle est nulle) ; reprise =
    /// premier instant de placement du reste depuis
    /// `max(Stop, reprise saisie, bornes)` ; fin = fin du reste (feuille
    /// affectée : plus grande fin des restes de ses parts, R-209). Bornes :
    /// fractionnement permis, toutes celles du réseau (début du projet,
    /// porte, FS, SS, et FF/SF converties sur le reste) et le plancher SNET,
    /// MSO ; sinon aucune, et chaque lien entrant dont la borne est violée
    /// donne `progress.link_ignored` (info, hors repasse). Pilote
    /// `Driver::Actual`, sans borne de pilote ; retard de nivellement ignoré
    /// (il n'agit qu'avant le début). À rebours (passe avant secondaire,
    /// R-207) : dates de la passe arrière principale, gardées.
    fn running_leaf(&mut self, v: NodeIx, ip: InProgress) -> Result<(), Fail> {
        let stored = self.state(v).and_then(|s| s.run);
        if self.mode == Mode::FromFinish {
            let run = stored.ok_or(Fail::Internal("tâche en cours sans dates à rebours"))?;
            let s = self.state_mut(v).ok_or(Fail::Internal("nœud hors bornes"))?;
            s.early = Some(Span {
                start: run.actual_start,
                finish: run.finish,
            });
            s.driver = Driver::Actual;
            s.driver_bound = None;
            s.undelayed = None;
            s.held = None;
            return Ok(());
        }
        let b = self.bounds(v, None)?;
        let uid = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?.uid;
        let actual_start = match (ip.start, stored) {
            (Some(start), _) => start,
            (None, Some(run)) => run.actual_start,
            (None, None) => {
                self.emit(Diagnostic::ProgressActualStartMissing { task: uid });
                self.placer(v)?.forward_span(b.start_side.0)?.start
            }
        };
        let stop = self.stop_of(v, actual_start, ip.actual)?;
        let mut from = ip.resume.map_or(stop, |r| r.max(stop));
        if ip.split {
            from = from.max(b.start);
            if let Some(date) = self.net.node(v).and_then(|n| n.constraint).and_then(|c| c.date) {
                from = from.max(date);
            }
        }
        let rest = self.remaining_placer(v)?.forward_span(from)?;
        let run = super::Run {
            actual_start,
            stop,
            resume: rest.start.max(stop),
            finish: rest.finish.max(stop),
            late_resume: stored.and_then(|r| r.late_resume),
        };
        if !ip.split && self.role != PassRole::Replay {
            self.links_ignored(v, run)?;
        }
        let s = self.state_mut(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        s.early = Some(Span {
            start: actual_start,
            finish: run.finish,
        });
        s.run = Some(run);
        s.undelayed = None;
        s.held = None;
        s.driver = Driver::Actual;
        s.driver_bound = None;
        s.degraded = b.degraded;
        s.linked = b.linked;
        Ok(())
    }

    /// R-204 : `Stop` d'une feuille en cours `v` de début réel
    /// `actual_start` et de durée réelle `actual` : fin de la partie réelle
    /// placée depuis le début réel sur l'échelle de placement ; le début réel
    /// (brut) si la durée réelle est nulle.
    pub(super) fn stop_of(&self, v: NodeIx, actual_start: Moment, actual: Minutes) -> Result<Moment, Fail> {
        if actual <= Minutes::ZERO {
            return Ok(actual_start);
        }
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let scale = self
            .net
            .scale(node.place)
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        Ok(scale.forward_span(actual_start, actual)?.finish.max(actual_start))
    }

    /// R-205 : fractionnement non permis, `progress.link_ignored` pour
    /// chaque lien entrant de la feuille en cours `v` (dates `run`) dont la
    /// borne est violée : FS, SS si la borne dépasse la reprise en brut ; FF,
    /// SF si la fin la précède en temps ouvré de l'échelle de placement. Une
    /// borne qui ne se calcule pas ne signale rien.
    fn links_ignored(&mut self, v: NodeIx, run: super::Run) -> Result<(), Fail> {
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let scale = *self
            .net
            .scale(node.place)
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        let incoming = self
            .net
            .incoming(v)
            .ok_or(Fail::Internal("arcs entrants hors bornes"))?;
        for i in incoming {
            if self.net.is_cut(i) || self.is_dropped(i) {
                continue;
            }
            let arc = *self.net.arc(i).ok_or(Fail::Internal("arc hors bornes"))?;
            let Some(l) = arc.link else { continue };
            let Some(se) = self.state(arc.from).and_then(|s| s.early) else {
                continue;
            };
            let Ok(x) = self.link_bound(i, l, se) else {
                continue;
            };
            let violated = match l.ty {
                LinkType::FinishToStart | LinkType::StartToStart => x > run.resume,
                LinkType::FinishToFinish | LinkType::StartToFinish => {
                    scale.between(run.finish, x).is_ok_and(|m| m.get() > 0)
                }
            };
            if violated {
                self.emit(Diagnostic::ProgressLinkIgnored {
                    predecessor: l.pred,
                    successor: l.succ,
                });
            }
        }
        Ok(())
    }

    /// R-203 : `progress.link_overridden` pour chaque lien entrant de la
    /// feuille achevée `v` (dates réelles `a`) dont la borne est violée :
    /// FS, SS si la borne dépasse le début réel en brut ; FF, SF si la fin
    /// réelle la précède en temps ouvré de l'échelle de placement. Une borne
    /// qui ne se calcule pas ne signale rien.
    fn links_overridden(&mut self, v: NodeIx, a: Span) -> Result<(), Fail> {
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let scale = *self
            .net
            .scale(node.place)
            .ok_or(Fail::Internal("échelle hors bornes"))?;
        let incoming = self
            .net
            .incoming(v)
            .ok_or(Fail::Internal("arcs entrants hors bornes"))?;
        for i in incoming {
            if self.net.is_cut(i) || self.is_dropped(i) {
                continue;
            }
            let arc = *self.net.arc(i).ok_or(Fail::Internal("arc hors bornes"))?;
            let Some(l) = arc.link else { continue };
            let Some(se) = self.state(arc.from).and_then(|s| s.early) else {
                continue;
            };
            let Ok(x) = self.link_bound(i, l, se) else {
                continue;
            };
            let violated = match l.ty {
                LinkType::FinishToStart | LinkType::StartToStart => x > a.start,
                LinkType::FinishToFinish | LinkType::StartToFinish => {
                    scale.between(a.finish, x).is_ok_and(|m| m.get() > 0)
                }
            };
            if violated {
                self.emit(Diagnostic::ProgressLinkOverridden {
                    predecessor: l.pred,
                    successor: l.succ,
                });
            }
        }
        Ok(())
    }

    /// Contrainte d'une feuille automatique en passe avant (ADR-0010), sur
    /// le placement `natural` de ses bornes (R-044) : un plancher (R-093,
    /// R-094) ne pilote que s'il déplace le début placé ; une date de
    /// contrainte plus grande en brut mais de même temps ouvré ne change
    /// rien.
    /// - plancher de début : `s* > natural.start` (deux débuts placés,
    ///   comparés en brut) → `early = A` ;
    /// - plancher de fin : `P.between(natural.finish, f*) > 0` → `early = A`
    ///   (fin = `f*`, O14, et non `forward_span(A.start, d)`).
    fn constrain(
        &mut self,
        v: NodeIx,
        scale: TimeScale<'_>,
        natural: Span,
        driver: Driver,
    ) -> Result<(Span, Driver), Fail> {
        let node = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        let Some(c) = node.constraint else {
            return Ok((natural, driver));
        };
        let pinned = Driver::Constraint { kind: c.kind };
        // Passe principale depuis le début, repasse comprise (R-092 : la
        // repasse rejoue la passe principale, contraintes comprises).
        let primary_from_start = self.role != PassRole::Secondary && self.mode == Mode::FromStart;
        match c.rule {
            // R-097, R-098 : option active, MSO et MFO épinglent la feuille
            // sur l'ancrage, quelles que soient ses bornes (R-100).
            ConstraintRule::Pin(_) => {
                let a = c.span()?;
                self.overrides(v, scale, a, c.kind, false)?;
                return Ok((a, pinned));
            }
            // R-095, R-096, R-099 : option active, le plafond (SNLT, FNLT)
            // s'impose en passe principale ; chaque borne violée est
            // signalée (R-100).
            ConstraintRule::Clamped(side) if primary_from_start => {
                let a = c.span()?;
                let late = match side {
                    Side::Start => natural.start > a.start,
                    Side::Finish => scale.between(a.finish, natural.finish)?.get() > 0,
                };
                if late {
                    self.overrides(v, scale, a, c.kind, false)?;
                    return Ok((a, pinned));
                }
            }
            // Plancher : SNET, FNET ; MSO, MFO option inactive (fenêtre,
            // R-097, R-098) ; à rebours, SNET et FNET serrés en passe
            // principale (option active) restent des planchers de la passe
            // avant secondaire (R-109).
            ConstraintRule::Floor(Side::Start)
            | ConstraintRule::Window(Side::Start)
            | ConstraintRule::Clamped(Side::Start) => {
                let a = c.span()?;
                if a.start > natural.start {
                    return Ok((a, pinned));
                }
            }
            ConstraintRule::Floor(Side::Finish)
            | ConstraintRule::Window(Side::Finish)
            | ConstraintRule::Clamped(Side::Finish) => {
                let a = c.span()?;
                if scale.between(natural.finish, a.finish)?.get() > 0 {
                    return Ok((a, pinned));
                }
            }
            ConstraintRule::None
            | ConstraintRule::Opposite
            | ConstraintRule::Cap(_)
            | ConstraintRule::Actual { .. }
            | ConstraintRule::Progress(_) => {}
        }
        Ok((natural, driver))
    }

    /// R-100 : la feuille `v`, placée de force sur `a` par sa contrainte de
    /// type `kind` (`opposite` : épingle d'une tâche ALAP en repasse, R-092),
    /// viole ces bornes ; un `constraint.override` par borne
    /// violée, en passe principale seulement : début du projet si
    /// `a.start <` début (brut) ; porte du parent si `a.start < G` ; lien
    /// FS/SS si `a.start <` borne (brut) ; lien FF/SF si
    /// `P.between(a.finish, borne) > 0` (R-063, même lecture que pour une
    /// tâche manuelle).
    fn overrides(
        &mut self,
        v: NodeIx,
        scale: TimeScale<'_>,
        a: Span,
        kind: ConstraintKind,
        opposite: bool,
    ) -> Result<(), Fail> {
        // Passe secondaire : jamais ; repasse : seulement pour contrôler les
        // épingles des tâches à placement opposé (les contraintes dures ont
        // déjà été signalées par la passe principale).
        if self.role == PassRole::Secondary || (self.role == PassRole::Replay && !opposite) {
            return Ok(());
        }
        let task = self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?.uid;
        if a.start < self.start {
            self.violated(task, kind, OverriddenBound::ProjectStart);
        }
        let incoming = self
            .net
            .incoming(v)
            .ok_or(Fail::Internal("arcs entrants hors bornes"))?;
        for i in incoming {
            if self.net.is_cut(i) || self.is_dropped(i) {
                continue;
            }
            let arc = *self.net.arc(i).ok_or(Fail::Internal("arc hors bornes"))?;
            let Some(se) = self.state(arc.from).and_then(|s| s.early) else {
                continue;
            };
            let bound = match arc.link {
                None => {
                    let summary = self
                        .net
                        .node(arc.from)
                        .ok_or(Fail::Internal("nœud hors bornes"))?
                        .uid;
                    (se.start > a.start).then_some(OverriddenBound::SummaryGate { summary })
                }
                Some(l) => {
                    let x = self.link_bound(i, l, se)?;
                    let violated = match l.ty {
                        LinkType::FinishToStart | LinkType::StartToStart => x > a.start,
                        LinkType::FinishToFinish | LinkType::StartToFinish => {
                            scale.between(a.finish, x)?.get() > 0
                        }
                    };
                    violated.then_some(OverriddenBound::Predecessor {
                        predecessor: l.pred,
                        link: l.ty,
                    })
                }
            };
            if let Some(bound) = bound {
                self.violated(task, kind, bound);
            }
        }
        Ok(())
    }

    /// Borne `bound` violée par la tâche `task` placée de force : en passe
    /// principale, `constraint.override` (R-100) ; en repasse, une épingle
    /// (R-092) ne viole jamais un lien : invariant violé.
    fn violated(&mut self, task: TaskUid, kind: ConstraintKind, bound: OverriddenBound) {
        if self.role == PassRole::Replay {
            self.emit(Diagnostic::ScheduleInternal {
                detail: format!("tâche {task} : épingle en repasse, {bound} est violé(e)"),
            });
        } else {
            self.emit(Diagnostic::ConstraintOverride { task, kind, bound });
        }
    }

    /// Agrégat d'une récapitulative (R-053) : min des débuts, max des fins
    /// des enfants planifiés reliés (actifs, ou tous inactifs sous une
    /// récapitulative inactive). Aucun : pas de dates (`EmptySummary`).
    /// Récapitulative manuelle datée (R-064) : ses dates saisies, pilote
    /// `Manual`, quelles que soient celles de ses enfants.
    fn rollup(&mut self, v: NodeIx) -> Result<(), Fail> {
        let fixed = match self.net.node(v).ok_or(Fail::Internal("nœud hors bornes"))?.fixed {
            Some(r) => Some(r?),
            None => None,
        };
        let incoming = self
            .net
            .incoming(v)
            .ok_or(Fail::Internal("arcs entrants hors bornes"))?;
        let mut span: Option<Span> = None;
        let (mut degraded, mut failed) = (false, false);
        let replay = self.role == PassRole::Replay;
        for a in incoming {
            let arc = self.net.arc(a).ok_or(Fail::Internal("arc hors bornes"))?;
            let child = self.state(arc.from).ok_or(Fail::Internal("nœud hors bornes"))?;
            degraded |= child.degraded || child.failed;
            failed |= child.failed;
            if let Some(e) = child.early {
                span = Some(match span {
                    None => e,
                    Some(s) => Span {
                        start: s.start.min(e.start),
                        finish: s.finish.max(e.finish),
                    },
                });
            }
        }
        let s = self.state_mut(v).ok_or(Fail::Internal("nœud hors bornes"))?;
        s.children = span;
        s.driver_bound = None;
        if let Some(f) = fixed {
            s.early = Some(f);
            s.driver = Driver::Manual;
            return Ok(());
        }
        s.early = span;
        s.driver = Driver::Rollup;
        s.degraded = degraded || (replay && s.degraded);
        // Tous les enfants en échec : l'agrégat compte comme non planifiable
        // pour ses successeurs (dégradés, R-069).
        s.failed = span.is_none() && failed;
        Ok(())
    }
}
