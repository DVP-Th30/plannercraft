//! Équilibrage d'une saisie d'avancement (ADR-0014 D3, D4 ; règles R-200
//! (équation), R-201, R-202, R-211 à R-213 ; lot L2).
//!
//! Fonction pure, sans planning ni projet complet : la tâche, ses
//! affectations de travail, la saisie et un contexte (dates planifiées
//! courantes, calendrier de mesure, date d'état, options). La commande
//! `progress.update` (R-216) l'appelle puis écrit le résultat ; le recalcul
//! ne rééquilibre jamais.
//!
//! Vérité = réels (ADR-0014 D4) : la saisie produit une durée D, une durée
//! réelle, des dates réelles et le travail réel de chaque affectation ; le
//! reste (D − réel, W − réel) et les % en sont dérivés. Minutes ouvrées sur
//! le calendrier de mesure (temps civil pour une durée écoulée), divisions
//! par `round_div` (R-001) en `i128`, aucune panique (règle 10).
//!
//! La date d'état (`Ctx::status_date`) ne sert qu'aux options `Move*`, que
//! le calcul ne suit pas (R-218) : elle n'entre dans aucune valeur ici.

use crate::calendar::{CompiledCalendar, Span, TimeScale};
use crate::duration::{Duration, Minutes, TimeBasis, Work, percent_of};
use crate::fixed::round_div;
use crate::ids::AssignmentUid;
use crate::model::{Progress, ProgressOption, ProgressOptions, Task};
use crate::progress::facts;
use crate::quantity::{Percent, SignedPercent};
use crate::time::Moment;

/// Une saisie d'avancement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// % achevé (durée), 1/10 000 (R-201).
    Percent(Percent),
    /// Début réel (R-202).
    ActualStart(Moment),
    /// Fin réelle (R-202).
    ActualFinish(Moment),
    /// Durée réelle, minutes ouvrées (R-202).
    ActualDuration(Minutes),
    /// Durée restante, minutes ouvrées (R-202 : D = réel + reste).
    RemainingDuration(Minutes),
    /// % travail achevé, 1/10 000 (R-211).
    PercentWork(Percent),
    /// Travail réel de la tâche, réparti sur ses affectations (R-211).
    ActualWork(Work),
    /// % physique, 1/10 000 (R-212, sans effet sur les dates).
    Physical(Percent),
}

/// Contexte d'une saisie.
#[derive(Clone, Copy, Debug)]
pub struct Ctx<'a> {
    /// Dates planifiées courantes de la tâche (début réel implicite, fin
    /// réelle à 100 %, R-201).
    pub scheduled: Span,
    /// Calendrier de mesure de la tâche (R-113) : durée entre le début et
    /// la fin réels (R-202).
    pub measure: &'a CompiledCalendar,
    /// Date d'état effective (R-034, R-219).
    pub status_date: Moment,
    /// Options de l'avancement (R-218).
    pub options: ProgressOptions,
    /// Tâche récapitulative (refusée, R-213) : la hiérarchie se lit sur le
    /// projet, pas sur la tâche seule (ajout au contrat de `j6.md`).
    pub summary: bool,
}

/// Affectation de travail d'une tâche (les ressources matérielles et de
/// coût n'ont pas de travail réel).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Part {
    /// UID de l'affectation.
    pub uid: AssignmentUid,
    /// Travail.
    pub work: Work,
    /// Travail réel.
    pub actual_work: Work,
}

/// Remarque d'une saisie acceptée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EditNote {
    /// Jalon à un % strictement entre 0 et 100 : ramené à 100 %
    /// (`progress.milestone_rounded`, R-213).
    MilestoneRounded,
    /// Option que la saisie ne suit pas (`progress.option_unsupported`,
    /// R-218).
    OptionUnsupported(ProgressOption),
}

/// Saisie équilibrée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edited {
    /// Avancement à écrire.
    pub progress: Progress,
    /// Durée de la tâche (minutes ouvrées) ; change si la durée réelle
    /// dépasse D, si un reste est saisi ou si la fin réelle mesure D
    /// (R-202).
    pub duration: Minutes,
    /// Affectations de travail, triées par UID (R-211).
    pub parts: Vec<Part>,
    /// Remarques, dans l'ordre.
    pub notes: Vec<EditNote>,
}

/// Saisie refusée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// Récapitulative, automatique ou manuelle (R-213,
    /// `progress.summary_not_editable`).
    #[error("l'avancement d'une récapitulative ne se saisit pas")]
    SummaryNotEditable,
    /// Tâche inactive (R-213, `progress.inactive`).
    #[error("l'avancement d'une tâche inactive ne se saisit pas")]
    Inactive,
    /// Fin réelle avant le début réel (R-202, `progress.invalid_order`).
    #[error("la fin réelle précède le début réel")]
    InvalidOrder,
    /// Deux champs qui s'excluent (R-202, `param.conflict`), nommés comme
    /// dans `progress.update`.
    #[error("« {0} » et « {1} » ne se saisissent pas ensemble")]
    Conflict(&'static str, &'static str),
    /// Résultat hors des bornes (durée, travail).
    #[error("résultat hors des bornes")]
    Overflow,
}

impl EditError {
    /// Code stable (celui de l'erreur de commande, R-216).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SummaryNotEditable => "progress.summary_not_editable",
            Self::Inactive => "progress.inactive",
            Self::InvalidOrder => "progress.invalid_order",
            Self::Conflict(..) => "param.conflict",
            Self::Overflow => "limit.exceeded",
        }
    }
}

/// Noms des champs, comme dans `progress.update` (R-216).
const PERCENT: &str = "percent_complete";
const ACTUAL_START: &str = "actual_start";
const ACTUAL_FINISH: &str = "actual_finish";
const ACTUAL_DURATION: &str = "actual_duration";
const REMAINING_DURATION: &str = "remaining_duration";
const PERCENT_WORK: &str = "percent_work_complete";
const ACTUAL_WORK: &str = "actual_work";
const PHYSICAL: &str = "physical_percent_complete";
/// Second nom d'un travail saisi sur une tâche sans affectation de travail.
const ASSIGNMENTS: &str = "assignments";

/// Saisie regroupée par champ ; un champ saisi deux fois avec deux valeurs
/// différentes est un conflit.
#[derive(Clone, Copy, Debug, Default)]
struct Entered {
    percent: Option<Percent>,
    start: Option<Moment>,
    finish: Option<Moment>,
    actual_duration: Option<Minutes>,
    remaining: Option<Minutes>,
    percent_work: Option<Percent>,
    actual_work: Option<Work>,
    physical: Option<Percent>,
}

/// Range `v` dans `slot` ; une seconde valeur différente est refusée.
fn put<T: PartialEq + Copy>(slot: &mut Option<T>, v: T, name: &'static str) -> Result<(), EditError> {
    match slot {
        Some(old) if *old != v => Err(EditError::Conflict(name, name)),
        _ => {
            *slot = Some(v);
            Ok(())
        }
    }
}

impl Entered {
    /// Regroupe la saisie et refuse les conflits de R-202.
    fn collect(inputs: &[Input]) -> Result<Self, EditError> {
        let mut e = Self::default();
        for input in inputs {
            match *input {
                Input::Percent(p) => put(&mut e.percent, p, PERCENT)?,
                Input::ActualStart(t) => put(&mut e.start, t, ACTUAL_START)?,
                Input::ActualFinish(t) => put(&mut e.finish, t, ACTUAL_FINISH)?,
                Input::ActualDuration(m) => put(&mut e.actual_duration, m, ACTUAL_DURATION)?,
                Input::RemainingDuration(m) => put(&mut e.remaining, m, REMAINING_DURATION)?,
                Input::PercentWork(p) => put(&mut e.percent_work, p, PERCENT_WORK)?,
                Input::ActualWork(w) => put(&mut e.actual_work, w, ACTUAL_WORK)?,
                Input::Physical(p) => put(&mut e.physical, p, PHYSICAL)?,
            }
        }
        if e.actual_duration.is_some_and(Minutes::is_negative)
            || e.remaining.is_some_and(Minutes::is_negative)
        {
            return Err(EditError::Overflow);
        }
        if e.percent.is_some() && e.actual_duration.is_some() {
            return Err(EditError::Conflict(PERCENT, ACTUAL_DURATION));
        }
        // Le % fixe réel / D, le reste fixe D − réel : ensemble, l'un
        // contredit l'autre (ajout au contrat, hypothèse).
        if e.percent.is_some() && e.remaining.is_some() {
            return Err(EditError::Conflict(PERCENT, REMAINING_DURATION));
        }
        if e.finish.is_some() && e.percent.is_some_and(|p| p < Percent::FULL) {
            return Err(EditError::Conflict(PERCENT, ACTUAL_FINISH));
        }
        if e.finish.is_some() && e.remaining.is_some_and(|q| q > Minutes::ZERO) {
            return Err(EditError::Conflict(ACTUAL_FINISH, REMAINING_DURATION));
        }
        if e.percent_work.is_some() && e.actual_work.is_some() {
            return Err(EditError::Conflict(PERCENT_WORK, ACTUAL_WORK));
        }
        Ok(e)
    }

    /// Premier champ de durée saisi (% , durée réelle, reste, fin réelle),
    /// dans l'ordre d'application.
    fn duration_field(&self) -> Option<&'static str> {
        [
            (self.actual_duration.is_some(), ACTUAL_DURATION),
            (self.remaining.is_some(), REMAINING_DURATION),
            (self.percent.is_some(), PERCENT),
            (self.finish.is_some(), ACTUAL_FINISH),
        ]
        .into_iter()
        .find_map(|(set, name)| set.then_some(name))
    }

    /// Champ de travail saisi.
    fn work_field(&self) -> Option<&'static str> {
        if self.percent_work.is_some() {
            Some(PERCENT_WORK)
        } else if self.actual_work.is_some() {
            Some(ACTUAL_WORK)
        } else {
            None
        }
    }

    /// Vrai si la saisie touche autre chose que le % physique.
    const fn beyond_physical(&self) -> bool {
        self.percent.is_some()
            || self.start.is_some()
            || self.finish.is_some()
            || self.actual_duration.is_some()
            || self.remaining.is_some()
            || self.percent_work.is_some()
            || self.actual_work.is_some()
    }
}

/// Avancement de durée en cours d'équilibrage.
#[derive(Clone, Copy, Debug)]
struct State {
    /// Début réel (`None` : non commencée, ou début implicite à poser).
    start: Option<Moment>,
    /// Fin réelle (`Some` ⇔ achevée).
    finish: Option<Moment>,
    /// Durée réelle, `0 ≤ act ≤ d` sauf donnée relue achevée.
    act: Minutes,
    /// Durée D.
    d: Minutes,
}

/// Erreur de calendrier ou de domaine : résultat hors des bornes (la saisie
/// est refusée, jamais de panique).
fn out_of_bounds<E>(_: E) -> EditError {
    EditError::Overflow
}

/// Durée bornée au domaine des durées de tâche (R-008).
const fn bounded(m: Minutes) -> Result<Minutes, EditError> {
    if m.is_negative() || m.get() > Duration::MAX_MINUTES {
        Err(EditError::Overflow)
    } else {
        Ok(m)
    }
}

/// `percent_of(m, p)` (R-010) pour un % de 0 à 10 000 : `0 ≤ résultat ≤ m`.
fn share(m: Minutes, p: Percent) -> Result<Minutes, EditError> {
    let pct = SignedPercent::new(p.get()).map_err(out_of_bounds)?;
    percent_of(m, pct).map_err(out_of_bounds)
}

/// Contexte de calcul d'une saisie.
struct Calc<'c, 'a> {
    ctx: &'c Ctx<'a>,
    scale: TimeScale<'a>,
    /// Durée de la tâche avant la saisie : les dates planifiées du contexte
    /// ne valent que pour elle.
    planned_d: Minutes,
}

impl Calc<'_, '_> {
    /// Fin d'une partie réelle de `m` minutes depuis `start` : `start` pour
    /// `m = 0` (jalon, R-213), sinon `forward_span(start, m).finish`.
    fn end_from(&self, start: Moment, m: Minutes) -> Result<Moment, EditError> {
        if m == Minutes::ZERO {
            return Ok(start);
        }
        self.scale
            .forward_span(start, m)
            .map(|s| s.finish)
            .map_err(out_of_bounds)
    }

    /// Début réel, implicite au début planifié s'il manque (R-201).
    fn ensure_start(&self, st: &mut State) -> Moment {
        *st.start.get_or_insert(self.ctx.scheduled.start)
    }

    /// Achève la tâche à 100 % (R-201) : réel = D ; fin réelle = fin
    /// planifiée quand le début réel est le début planifié et que la saisie
    /// n'a pas changé D, sinon `forward_span(début réel, D)` ; jalon : fin =
    /// début. Une tâche déjà achevée garde sa fin.
    fn complete(&self, st: &mut State) -> Result<(), EditError> {
        let start = self.ensure_start(st);
        st.act = st.d;
        if st.finish.is_some() {
            return Ok(());
        }
        let planned = self.ctx.scheduled;
        let finish = if st.d == Minutes::ZERO {
            start
        } else if start == planned.start && st.d == self.planned_d && planned.finish >= start {
            planned.finish
        } else {
            self.end_from(start, st.d)?
        };
        st.finish = Some(finish);
        Ok(())
    }

    /// Saisie du début réel (R-202) : une tâche achevée garde sa durée, sa
    /// fin suit le début (hypothèse), sauf fin réelle saisie avec.
    fn actual_start(&self, st: &mut State, s: Moment, finish_entered: bool) -> Result<(), EditError> {
        let moved = st.start != Some(s);
        st.start = Some(s);
        if moved && st.finish.is_some() && !finish_entered {
            st.finish = Some(self.end_from(s, st.act)?);
        }
        Ok(())
    }

    /// Saisie de la durée réelle `r` (R-202) : `r ≥ D` (r > 0) ⇒ D = r,
    /// achevée, fin réelle = `forward_span(début réel, r)` ; sinon réel = r,
    /// non achevée.
    fn actual_duration(&self, st: &mut State, r: Minutes) -> Result<(), EditError> {
        let r = bounded(r)?;
        if r > Minutes::ZERO && r >= st.d {
            if st.finish.is_some() && r == st.act && r == st.d {
                return Ok(());
            }
            let start = self.ensure_start(st);
            st.d = r;
            st.act = r;
            st.finish = Some(self.end_from(start, r)?);
        } else if r == Minutes::ZERO && st.d == Minutes::ZERO {
            // Jalon : une durée réelle nulle ne change rien.
        } else {
            st.act = r;
            st.finish = None;
            if r > Minutes::ZERO {
                self.ensure_start(st);
            }
        }
        Ok(())
    }

    /// Saisie du reste `q` (R-202, S50) : D = réel + reste ; reste nul ⇒
    /// achevée.
    fn remaining(&self, st: &mut State, q: Minutes) -> Result<(), EditError> {
        let d = st.act.checked_add(q).ok_or(EditError::Overflow)?;
        st.d = bounded(d)?;
        if q == Minutes::ZERO {
            if st.finish.is_none() {
                let start = self.ensure_start(st);
                st.finish = Some(self.end_from(start, st.act)?);
            }
        } else {
            st.finish = None;
        }
        Ok(())
    }

    /// Saisie du % `p` (R-201, R-213). Rend vrai si un jalon a été arrondi.
    fn percent(&self, st: &mut State, p: Percent) -> Result<bool, EditError> {
        if p == Percent::ZERO {
            st.start = None;
            st.finish = None;
            st.act = Minutes::ZERO;
            return Ok(false);
        }
        if st.d == Minutes::ZERO || p == Percent::FULL {
            self.complete(st)?;
            return Ok(st.d == Minutes::ZERO && p < Percent::FULL);
        }
        self.ensure_start(st);
        // Non achevée : réel < D (un arrondi à D donnerait 100 % sans fin
        // réelle, contraire à « 100 % ⇔ fin réelle », P39).
        let below = Minutes::new(st.d.get().saturating_sub(1).max(0));
        st.act = share(st.d, p)?.min(below);
        st.finish = None;
        Ok(false)
    }

    /// Saisie de la fin réelle `f` (R-202) : 100 %, D = réel =
    /// `between(début réel, f)` ; `f` avant le début réel : refus.
    fn actual_finish(
        &self,
        st: &mut State,
        f: Moment,
        entered_actual: Option<Minutes>,
    ) -> Result<(), EditError> {
        let start = self.ensure_start(st);
        if f < start {
            return Err(EditError::InvalidOrder);
        }
        let m = bounded(self.scale.between(start, f).map_err(out_of_bounds)?)?;
        if entered_actual.is_some_and(|r| r != m) {
            return Err(EditError::Conflict(ACTUAL_DURATION, ACTUAL_FINISH));
        }
        st.d = m;
        st.act = m;
        st.finish = Some(f);
        Ok(())
    }
}

/// % de durée dérivé (R-200) d'un état : 10 000 si achevée ; D = 0 : 0 ;
/// sinon `round_div(réel × 10 000, D)` borné à 9 999 (non achevée).
fn derived_percent(st: &State) -> Percent {
    if st.finish.is_some() {
        return Percent::FULL;
    }
    let d = st.d.get();
    if d <= 0 {
        return Percent::ZERO;
    }
    let num = i128::from(st.act.get()).saturating_mul(10_000);
    round_div(num, i128::from(d))
        .map(|v| v.clamp(0, Percent::FULL.get().saturating_sub(1)))
        .and_then(|v| Percent::new(v).ok())
        .unwrap_or(Percent::ZERO)
}

/// `percent_of(W, p)` sur du travail (R-211) : `0 ≤ résultat ≤ W`.
fn work_share(w: Work, p: Percent) -> Result<Work, EditError> {
    Work::new(share(w.minutes(), p)?).map_err(out_of_bounds)
}

/// R-211 : répartit `total` sur les affectations au prorata de leur
/// travail (parts égales si le travail total est nul) ; reste de l'arrondi
/// par plus grand reste, puis UID croissant (`parts` triées par UID) ; la
/// somme vaut `total`.
fn distribute(parts: &[Part], total: Work) -> Result<Vec<Work>, EditError> {
    let x = i128::from(total.minutes().get());
    let all_zero = parts.iter().all(|p| p.work == Work::ZERO);
    let weight = |p: &Part| {
        if all_zero {
            1_i128
        } else {
            i128::from(p.work.minutes().get())
        }
    };
    let sum = parts
        .iter()
        .try_fold(0_i128, |acc, p| acc.checked_add(weight(p)))
        .ok_or(EditError::Overflow)?;
    if sum <= 0 {
        return Ok(Vec::new());
    }
    let mut floors = Vec::with_capacity(parts.len());
    let mut remainders = Vec::with_capacity(parts.len());
    for (rank, p) in parts.iter().enumerate() {
        let num = x.checked_mul(weight(p)).ok_or(EditError::Overflow)?;
        let floor = num.checked_div(sum).ok_or(EditError::Overflow)?;
        let rem = num.checked_rem(sum).ok_or(EditError::Overflow)?;
        floors.push(floor);
        remainders.push((rem, rank));
    }
    let given = floors
        .iter()
        .try_fold(0_i128, |acc, f| acc.checked_add(*f))
        .ok_or(EditError::Overflow)?;
    let left = x.checked_sub(given).ok_or(EditError::Overflow)?;
    // Plus grand reste d'abord, puis rang (UID croissant).
    remainders.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let extra = usize::try_from(left).map_err(out_of_bounds)?;
    for &(_, rank) in remainders.iter().take(extra) {
        if let Some(f) = floors.get_mut(rank) {
            *f = f.checked_add(1).ok_or(EditError::Overflow)?;
        }
    }
    floors
        .into_iter()
        .map(|f| {
            i64::try_from(f)
                .map_err(out_of_bounds)
                .and_then(|m| Work::new(Minutes::new(m)).map_err(out_of_bounds))
        })
        .collect()
}

/// Somme du travail (ou du travail réel) des affectations.
fn total(parts: &[Part], f: impl Fn(&Part) -> Work) -> Result<i64, EditError> {
    parts
        .iter()
        .try_fold(0_i64, |acc, p| acc.checked_add(f(p).minutes().get()))
        .ok_or(EditError::Overflow)
}

/// Applique la saisie `inputs` à `task` et à ses affectations de travail
/// `parts` (ordre quelconque, UID uniques), dans l'ordre de R-202 : début
/// réel, durée réelle, reste, %, fin réelle, puis travail ; le % physique
/// est rangé tel quel (R-212).
///
/// - R-201 : % > 0 sans début réel ⇒ début réel = début planifié ; réel =
///   `percent_of(D, p)` ; 100 % ⇒ fin réelle = fin planifiée ; 0 % ⇒ dates
///   réelles effacées, réel 0, travail réel 0 ;
/// - R-202 : début réel seul ⇒ commencée à 0 % ; durée réelle r ≥ D ⇒ D = r,
///   achevée ; reste q ⇒ D = réel + q (q = 0 : achevée) ; fin réelle f ⇒
///   D = réel = temps de mesure entre le début réel et f ;
/// - R-211 : un % de durée saisi donne `AW_a = percent_of(W_a, p)` ; % travail
///   saisi : `AW_a = percent_of(W_a, p)` ; travail réel saisi : réparti au
///   prorata ; travail réel au-delà de W ⇒ W = AW ; travail achevé ⇒ tâche
///   achevée (S52) ; travail saisi sans champ de durée (tâche non achevée) ⇒
///   durée réelle = `round_div(D × Σ AW, Σ W)` (amendement du 2026-10-20) ;
/// - R-213 : récapitulative, inactive refusées ; jalon à un % entre 0 et
///   100 ⇒ 100 % (`MilestoneRounded`).
///
/// Le résultat est cohérent (R-200) : 0 ≤ réel ≤ D, reste = D − réel, % dérivé,
/// 100 % ⇔ fin réelle ; appliquer deux fois la même saisie donne le même
/// résultat (P39).
///
/// # Errors
///
/// [`EditError`] : récapitulative, inactive, fin avant le début, champs en
/// conflit (R-202, et travail achevé avec une durée non achevée, travail
/// non achevé sur une tâche achevée, travail sur une tâche sans affectation
/// de travail), durée ou travail hors des bornes.
pub fn apply(task: &Task, parts: &[Part], inputs: &[Input], ctx: &Ctx<'_>) -> Result<Edited, EditError> {
    if ctx.summary {
        return Err(EditError::SummaryNotEditable);
    }
    if !task.active {
        return Err(EditError::Inactive);
    }
    let e = Entered::collect(inputs)?;
    let calc = Calc {
        ctx,
        scale: match task.duration.basis() {
            TimeBasis::Working => TimeScale::Working(ctx.measure),
            TimeBasis::Elapsed => TimeScale::Elapsed,
        },
        planned_d: task.duration.minutes().max(Minutes::ZERO),
    };
    let f = facts(task);
    let mut st = State {
        start: f.actual_start,
        finish: f.actual_finish,
        act: f.actual_duration,
        d: task.duration.minutes().max(Minutes::ZERO),
    };
    let mut notes = Vec::new();
    if e.beyond_physical() {
        notes.extend(
            ctx.options
                .unsupported()
                .into_iter()
                .map(EditNote::OptionUnsupported),
        );
    }

    // Avancement de durée, dans l'ordre de R-202.
    if let Some(s) = e.start {
        calc.actual_start(&mut st, s, e.finish.is_some())?;
    }
    if let Some(r) = e.actual_duration {
        calc.actual_duration(&mut st, r)?;
    }
    if let Some(q) = e.remaining {
        calc.remaining(&mut st, q)?;
    }
    let mut percent_used = None;
    if let Some(p) = e.percent {
        if calc.percent(&mut st, p)? {
            notes.push(EditNote::MilestoneRounded);
        }
        percent_used = Some(p);
    }
    if let Some(fin) = e.finish {
        calc.actual_finish(&mut st, fin, e.actual_duration)?;
    }

    // Travail (R-211).
    let mut parts = parts.to_vec();
    parts.sort_unstable();
    apply_work(&calc, &e, &mut st, &mut parts, percent_used)?;

    // Début réel implicite d'une tâche commencée (R-200), posé à la saisie.
    let started = st.finish.is_some() || st.act > Minutes::ZERO || st.start.is_some();
    if started && st.start.is_none() {
        let planned = ctx.scheduled.start;
        st.start = Some(st.finish.map_or(planned, |fin| planned.min(fin)));
    }
    if let (Some(s), Some(fin), true) = (st.start, st.finish, e.start.is_some() || e.finish.is_some())
        && fin < s
    {
        return Err(EditError::InvalidOrder);
    }

    Ok(Edited {
        progress: progress_of(task, &st, &parts, e.physical)?,
        duration: st.d,
        parts,
        notes,
    })
}

/// Travail réel des affectations (R-211) et ses effets sur l'état de durée :
/// travail achevé ⇒ tâche achevée (S52) ; travail réel > 0 ⇒ commencée.
fn apply_work(
    calc: &Calc<'_, '_>,
    e: &Entered,
    st: &mut State,
    parts: &mut [Part],
    percent_used: Option<Percent>,
) -> Result<(), EditError> {
    let work_field = e.work_field();
    if parts.is_empty() {
        let nonzero = e.percent_work.is_some_and(|p| p > Percent::ZERO)
            || e.actual_work.is_some_and(|w| w > Work::ZERO);
        if let (true, Some(field)) = (nonzero, work_field) {
            return Err(EditError::Conflict(field, ASSIGNMENTS));
        }
    } else if let Some(pw) = e.percent_work {
        for p in parts.iter_mut() {
            p.actual_work = work_share(p.work, pw)?;
        }
    } else if let Some(x) = e.actual_work {
        let shares = distribute(parts, x)?;
        for (p, aw) in parts.iter_mut().zip(shares) {
            p.actual_work = aw;
            p.work = p.work.max(aw);
        }
    } else if e.duration_field().is_some() {
        // Le % de durée met à jour le travail réel (`TaskUpdatesResource`).
        let p = match percent_used {
            Some(p) if st.finish.is_none() && p > Percent::ZERO => p,
            _ => derived_percent(st),
        };
        for part in parts.iter_mut() {
            part.actual_work = work_share(part.work, p)?;
        }
    }
    if let (Some(field), false) = (work_field, parts.is_empty()) {
        let planned_work = total(parts, |p| p.work)?;
        let done_work = total(parts, |p| p.actual_work)?;
        let work_done = e.percent_work == Some(Percent::FULL) || (done_work > 0 && done_work >= planned_work);
        if work_done && st.finish.is_none() {
            if let Some(dur) = e.duration_field() {
                return Err(EditError::Conflict(dur, field));
            }
            calc.complete(st)?;
        } else if !work_done && st.finish.is_some() && done_work < planned_work {
            return Err(EditError::Conflict(
                field,
                e.duration_field().unwrap_or(ACTUAL_FINISH),
            ));
        }
        // R-211 amendée (arbitrage du 2026-10-20) : travail saisi seul sur
        // une tâche affectée non achevée ⇒ durée réelle tirée du travail,
        // `round_div(D × Σ AW, Σ W)` bornée à D − 1 : la partie réelle de
        // chaque affectation (R-209, AW_a / U_a) finit au plus tard à `Stop`
        // et son reste ne la chevauche plus.
        if e.duration_field().is_none() && st.finish.is_none() && planned_work > 0 {
            let d = i128::from(st.d.get());
            let act = d
                .checked_mul(i128::from(done_work))
                .and_then(|x| round_div(x, i128::from(planned_work)))
                .ok_or(EditError::Overflow)?;
            let below = st.d.get().saturating_sub(1).max(0);
            st.act = Minutes::new(act.clamp(0, below));
        }
        if done_work > 0 {
            calc.ensure_start(st);
        }
    }
    Ok(())
}

/// Avancement écrit (R-200, R-211, R-212) : réels, restes et % dérivés.
fn progress_of(
    task: &Task,
    st: &State,
    parts: &[Part],
    physical: Option<Percent>,
) -> Result<Progress, EditError> {
    let mut out = task.progress;
    out.actual_start = st.start;
    out.actual_finish = st.finish;
    out.actual_duration = st.act;
    out.remaining_duration = if st.finish.is_some() {
        Minutes::ZERO
    } else {
        Minutes::new(st.d.get().saturating_sub(st.act.get()).max(0))
    };
    out.percent_complete = derived_percent(st);
    if let Some(p) = physical {
        out.physical_percent_complete = p;
    }
    // La reprise ne vaut que pour une tâche en cours (R-204).
    if st.finish.is_some() || st.start.is_none() {
        out.resume = None;
    }
    if !parts.is_empty() {
        let planned_work = total(parts, |p| p.work)?;
        let done_work = total(parts, |p| p.actual_work)?;
        out.actual_work = Work::new(Minutes::new(done_work)).map_err(out_of_bounds)?;
        out.remaining_work =
            Work::new(Minutes::new(planned_work.saturating_sub(done_work).max(0))).map_err(out_of_bounds)?;
        out.percent_work_complete = if planned_work <= 0 {
            Percent::ZERO
        } else if done_work >= planned_work {
            Percent::FULL
        } else {
            let num = i128::from(done_work).saturating_mul(10_000);
            round_div(num, i128::from(planned_work))
                .map(|v| v.clamp(0, Percent::FULL.get().saturating_sub(1)))
                .and_then(|v| Percent::new(v).ok())
                .unwrap_or(Percent::ZERO)
        };
    }
    Ok(out)
}

/// R-211 : reste saisi sur une affectation ⇒ `W_a = AW_a + reste` (ADR-0014
/// D4) ; l'appelant applique ensuite R-189 « travail » (`resource::edit`).
///
/// Exemple : AW 480, reste 720 ⇒ W 1 200.
///
/// # Errors
///
/// [`EditError::Overflow`] au-delà du travail maximal (S11).
pub fn remaining_work_entered(part: Part, remaining: Work) -> Result<Part, EditError> {
    let w = part
        .actual_work
        .minutes()
        .checked_add(remaining.minutes())
        .ok_or(EditError::Overflow)?;
    Ok(Part {
        work: Work::new(w).map_err(out_of_bounds)?,
        ..part
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Ctx, EditError, EditNote, Input, Part, apply, distribute, remaining_work_entered};
    use crate::arena::Arena;
    use crate::calendar::{CompiledCalendar, Span, standard};
    use crate::duration::{Minutes, Work};
    use crate::ids::{AssignmentUid, CalendarUid, TaskUid};
    use crate::model::{ProgressOption, ProgressOptions, ProjectOptions, Task};
    use crate::quantity::Percent;

    fn work(m: i64) -> Work {
        Work::new(Minutes::new(m)).unwrap()
    }

    fn part(uid: u32, w: i64) -> Part {
        Part {
            uid: AssignmentUid::new(uid),
            work: work(w),
            actual_work: Work::ZERO,
        }
    }

    #[test]
    fn refuses_summary_and_inactive_and_sorts_parts() {
        let u = CalendarUid::new(1);
        let cals = Arena::try_from(vec![standard(u, "S".to_owned())]).unwrap();
        let cal = CompiledCalendar::compile(&cals, u).unwrap();
        let at = |s: &str| s.parse().unwrap();
        let mut ctx = Ctx {
            scheduled: Span {
                start: at("2026-10-19T08:00"),
                finish: at("2026-10-19T17:00"),
            },
            measure: &cal,
            status_date: at("2026-10-19T08:00"),
            options: ProgressOptions::default(),
            summary: false,
        };
        let mut t = Task::new(TaskUid::new(1), "T".to_owned(), &ProjectOptions::default());
        let parts = [part(2, 0), part(1, 0)];
        let e = apply(&t, &parts, &[Input::Percent(Percent::FULL)], &ctx).unwrap();
        assert_eq!(e.progress.actual_finish, Some(at("2026-10-19T17:00")));
        assert_eq!(e.duration, Minutes::new(480));
        assert_eq!(e.parts.first().map(|p| p.uid), Some(AssignmentUid::new(1)));
        t.active = false;
        assert_eq!(apply(&t, &parts, &[], &ctx), Err(EditError::Inactive));
        ctx.summary = true;
        assert_eq!(apply(&t, &parts, &[], &ctx), Err(EditError::SummaryNotEditable));
        assert_eq!(EditError::Conflict("a", "b").code(), "param.conflict");
    }

    #[test]
    fn unsupported_options_noted_except_for_physical_only() {
        let u = CalendarUid::new(1);
        let cals = Arena::try_from(vec![standard(u, "S".to_owned())]).unwrap();
        let cal = CompiledCalendar::compile(&cals, u).unwrap();
        let at = |s: &str| s.parse().unwrap();
        let options = ProgressOptions {
            task_updates_resource: false,
            move_completed_ends_back: true,
            ..ProgressOptions::default()
        };
        let ctx = Ctx {
            scheduled: Span {
                start: at("2026-10-19T08:00"),
                finish: at("2026-10-19T17:00"),
            },
            measure: &cal,
            status_date: at("2026-10-19T08:00"),
            options,
            summary: false,
        };
        let t = Task::new(TaskUid::new(1), "T".to_owned(), &ProjectOptions::default());
        let parts = [part(1, 480)];
        let e = apply(&t, &parts, &[Input::Percent(Percent::new(5_000).unwrap())], &ctx).unwrap();
        assert_eq!(
            e.notes,
            [
                EditNote::OptionUnsupported(ProgressOption::TaskUpdatesResource),
                EditNote::OptionUnsupported(ProgressOption::MoveCompletedEndsBack),
            ]
        );
        // Calcul identique aux défauts : le % met à jour le travail réel.
        assert_eq!(e.parts.first().map(|p| p.actual_work), Some(work(240)));
        let e = apply(&t, &parts, &[Input::Physical(Percent::FULL)], &ctx).unwrap();
        assert_eq!(e.notes, []);
    }

    #[test]
    fn distribute_sums_and_breaks_ties_by_uid() {
        let shares = distribute(&[part(1, 0), part(2, 0), part(3, 0)], work(7)).unwrap();
        assert_eq!(shares, vec![work(3), work(2), work(2)]);
        let shares = distribute(&[part(1, 1), part(2, 2)], work(1)).unwrap();
        // 1/3 et 2/3 : le plus grand reste (2/3) prend la minute.
        assert_eq!(shares, vec![work(0), work(1)]);
        assert_eq!(distribute(&[], work(5)).unwrap(), Vec::<Work>::new());
    }

    #[test]
    fn remaining_work_bounds() {
        let p = Part {
            actual_work: work(480),
            ..part(1, 960)
        };
        assert_eq!(remaining_work_entered(p, work(720)).unwrap().work, work(1_200));
        let max = Part {
            actual_work: work(Work::MAX_MINUTES),
            ..part(1, Work::MAX_MINUTES)
        };
        assert_eq!(remaining_work_entered(max, work(1)), Err(EditError::Overflow));
    }
}
