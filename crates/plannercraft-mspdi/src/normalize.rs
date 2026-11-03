//! Image d'un projet par l'aller-retour MSPDI (R-133) :
//! `read(write(p)).project == normalize(p)` pour tout projet sans erreur de
//! `Project::validate` ; `normalize` est idempotente.
//!
//! Elle décrit exactement ce que l'aller-retour change (la liste « perdu » de
//! la documentation) : pourcentages au pour-cent, restes d'une feuille et
//! d'une affectation dérivés (R-214), reprise saisie gardée pour une
//! feuille en cours seulement, décalages en pourcentage
//! au pas de 0,1 %, durée d'une récapitulative automatique (valeur
//! calculée), chaînes (U+FFFD, troncature au `maxLength`), `Period` réécrite
//! quotidienne (mêmes jours), `Extras` (invalides et origines étrangères
//! retirés, ancres recalculées, ordre canonique, `CurrencyCode` ajouté ;
//! style du document gardé seulement s'il est écrit, H-R12),
//! taux relus à travers l'écriture par heure ; champs personnalisés (R-266 :
//! valeurs et définitions des champs étrangers retirées, textes tronqués à
//! 255 et alias à 50 caractères, vides retirés, définitions et valeurs
//! simples des `Extras` remontées dans le modèle) ; profil strict : `GUID`,
//! éléments hors schéma et plages des semaines datées perdus.

use std::collections::BTreeSet;

use plannercraft_core::calendar::{
    Calendar, CalendarException, CalendarKind, DayHours, DayRule, Recurrence, TimeRange,
};
use plannercraft_core::{
    Duration, ExtraEntry, Extras, LagValue, LevelingDelay, OutlineLevel, Percent, Project, Rate, Resource,
    SchedulingMode, SignedPercent, Task, TaskType, TimeOfDay, Weekday, round_div,
};

use crate::assignment::ASSIGNMENT_DELAY_ELAPSED;
use crate::extras::{self, Anchor, ExtraKey, ORIGIN};
use crate::resource::rate_amount;
use crate::scalar::parse_decimal;
use crate::schema::{Class, Parent, Profile, sequence};
use crate::task::{TASK_DELAY_ELAPSED, computed_zero, delay_value, keeps_resume};
use crate::write::{
    OrderProfile, WriteOptions, escape_text, hourly_rate_text, leveling_payload_int, remaining, sanitize_text,
};

/// R-133 : image de `project` par l'écriture (profil de `options`) suivie
/// de la lecture.
#[must_use]
pub fn normalize(project: &Project, options: &WriteOptions) -> Project {
    let strict = options.profile == OrderProfile::Strict;
    let mut p = project.clone();
    let text = |s: &str, parent: Parent, name: &str| string(s, parent, name);
    p.info.name = text(&p.info.name, Parent::Project, "Name");
    p.info.title = text(&p.info.title, Parent::Project, "Title");
    p.info.subject = text(&p.info.subject, Parent::Project, "Subject");
    p.info.category = text(&p.info.category, Parent::Project, "Category");
    p.info.company = text(&p.info.company, Parent::Project, "Company");
    p.info.manager = text(&p.info.manager, Parent::Project, "Manager");
    p.info.author = text(&p.info.author, Parent::Project, "Author");
    if strict {
        p.info.guid = None;
    }

    // Tâches : récapitulative (R-052) déduite de la hiérarchie.
    let flags = summary_flags(&p);
    let uids: Vec<_> = p.tasks.iter().map(Task::uid).collect();
    for (uid, summary) in uids.into_iter().zip(flags) {
        if let Some(t) = p.tasks.get_mut(uid) {
            task(t, summary, options);
        }
    }
    if let Some(t) = &mut p.summary_task {
        task(t, true, options);
        // UID 0 : premier niveau, sans lien, durée calculée (R-084, R-071).
        t.outline_level = OutlineLevel::TOP;
        t.predecessors.clear();
        t.duration = computed_zero(t.duration);
    }

    // Calendriers reconstruits (période → quotidienne, D15).
    let calendars: Vec<Calendar> = p.calendars.iter().map(|c| calendar(c, options)).collect();
    p.calendars = plannercraft_core::Arena::try_from(calendars).unwrap_or_default();

    let factors = p.options.factors;
    let ruids: Vec<_> = p.resources.iter().map(Resource::uid).collect();
    for uid in ruids {
        if let Some(r) = p.resources.get_mut(uid) {
            resource(r, &factors, options);
        }
    }
    let auids: Vec<_> = p
        .assignments
        .iter()
        .map(plannercraft_core::Assignment::uid)
        .collect();
    for uid in auids {
        if let Some(a) = p.assignments.get_mut(uid) {
            if strict {
                a.guid = None;
            }
            if a.delay.is_negative() {
                a.delay = plannercraft_core::Minutes::ZERO;
            }
            // R-199 : reste dérivé d'une affectation sans travail réel.
            a.remaining_work = crate::write::assignment_remaining_work(a);
            leveling(&mut a.leveling_delay, &mut a.extras, ASSIGNMENT_DELAY_ELAPSED);
            let numbers: Vec<u8> = a.baselines.keys().map(|n| n.get()).collect();
            a.extras = extras_image(&a.extras, Parent::Assignment, &numbers, options.profile, &|_| {
                false
            });
        }
    }

    // Extras du projet : conteneurs non vides réécrits (ancres), puis
    // `CurrencyCode` obligatoire (R-130).
    let containers = (
        !p.calendars.is_empty(),
        !p.resources.is_empty(),
        !p.assignments.is_empty(),
    );
    let written = move |name: &str| match name {
        "Calendars" => containers.0,
        "Resources" => containers.1,
        "Assignments" => containers.2,
        _ => false,
    };
    let mut ex = extras_image(&p.extras, Parent::Project, &[], options.profile, &written);
    let has_currency = ex.0.iter().any(|e| {
        ExtraKey::parse(&e.key).is_some_and(|k| k.name == "CurrencyCode" && k.anchor == Anchor::None)
    });
    if !has_currency {
        let code = string(&options.currency_code, Parent::Project, "CurrencyCode");
        let escaped = escape_text(&code);
        let payload = if escaped.is_empty() {
            "<CurrencyCode/>".to_owned()
        } else {
            format!("<CurrencyCode>{escaped}</CurrencyCode>")
        };
        ex.0.push(ExtraEntry {
            origin: ORIGIN.to_owned(),
            key: "CurrencyCode".to_owned(),
            payload,
        });
        ex = extras_image(&ex, Parent::Project, &[], options.profile, &written);
    }
    // H-R12 : le style du document se relit tel quel s'il est écrit.
    let style = crate::style_pi::entry(&p.extras)
        .map(|e| e.payload.clone())
        .filter(|payload| options.style && crate::style_pi::writable(payload));
    crate::style_pi::set(&mut ex, style);
    p.extras = ex;
    // P62 (R-266) : champs personnalisés, après les `Extras`.
    crate::custom::normalize(&mut p, |s, max| sanitize_text(s).chars().take(max).collect());
    p
}

/// Chaîne écrite puis relue : caractères interdits → U+FFFD, troncature au
/// `maxLength` de l'élément (R-131).
fn string(s: &str, parent: Parent, name: &str) -> String {
    let clean = sanitize_text(s);
    match sequence(parent).slot(name).and_then(|slot| slot.max_len) {
        Some(max) => clean.chars().take(usize::from(max)).collect(),
        None => clean,
    }
}

/// Récapitulatives (R-052).
fn summary_flags(p: &Project) -> Vec<bool> {
    let mut out = vec![false; p.tasks.len()];
    let mut next: Option<u16> = None;
    for (slot, t) in out.iter_mut().zip(p.tasks.iter()).rev() {
        if t.is_null {
            continue;
        }
        let level = t.outline_level.get();
        *slot = next.is_some_and(|n| n > level);
        next = Some(level);
    }
    out
}

/// Pourcentage au pour-cent (R-127).
fn whole_percent(p: Percent) -> Percent {
    round_div(i128::from(p.get()), 100)
        .and_then(|v| v.checked_mul(100))
        .and_then(|v| Percent::new(v).ok())
        .unwrap_or(p)
}

/// Une tâche.
fn task(t: &mut Task, summary: bool, options: &WriteOptions) {
    t.name = string(&t.name, Parent::Task, "Name");
    t.notes = string(&t.notes, Parent::Task, "Notes");
    t.wbs = t.wbs.as_deref().map(|w| string(w, Parent::Task, "WBS"));
    if options.profile == OrderProfile::Strict {
        t.guid = None;
    }
    let pr = &mut t.progress;
    pr.percent_complete = whole_percent(pr.percent_complete);
    pr.percent_work_complete = whole_percent(pr.percent_work_complete);
    pr.physical_percent_complete = whole_percent(pr.physical_percent_complete);
    // R-127, R-214 : restes d'une feuille dérivés (sur le % relu, au
    // pour-cent) ; reprise gardée pour une feuille en cours seulement.
    (t.progress.remaining_duration, t.progress.remaining_work) = remaining(t, summary);
    if !keeps_resume(t, summary) {
        t.progress.resume = None;
    }
    let duration = t.duration;
    match &mut t.mode {
        SchedulingMode::Manual { duration: d, .. } => {
            // Relue avec l'unité, la base et l'estimation de la tâche (R-085).
            *d = d.and_then(|d| {
                Duration::new(
                    d.minutes(),
                    duration.unit(),
                    duration.basis(),
                    duration.is_estimated(),
                )
                .ok()
            });
        }
        SchedulingMode::Auto => {
            if summary {
                // R-071 : valeur calculée, pas une entrée.
                t.duration = computed_zero(duration);
            }
        }
    }
    for link in &mut t.predecessors {
        if let LagValue::Percent { percent } = link.lag.value
            && let Some(v) = round_div(i128::from(percent.get()), 10)
                .and_then(|v| v.checked_mul(10))
                .and_then(|v| SignedPercent::new(v).ok())
        {
            link.lag.value = LagValue::Percent { percent: v };
        }
    }
    // R-193 : une feuille à travail fixe est relue pilotée par l'effort.
    if !summary && !t.is_null && t.task_type == TaskType::FixedWork {
        t.effort_driven = true;
    }
    leveling(&mut t.leveling_delay, &mut t.extras, TASK_DELAY_ELAPSED);
    let numbers: Vec<u8> = t.baselines.keys().map(|n| n.get()).collect();
    t.extras = extras_image(&t.extras, Parent::Task, &numbers, options.profile, &|_| false);
}

/// R-198 : retard de nivellement relu. Celui du modèle est écrit seulement
/// s'il est non nul après l'arrondi et dans le domaine des durées. Un
/// `Extras` hérité (`LevelingDelay`, `LevelingDelayFormat`, enregistré
/// avant J5) est jeté si le modèle a un retard, sinon réécrit puis relu
/// dans le modèle : il quitte les `Extras` dans les deux cas.
fn leveling(delay: &mut Option<LevelingDelay>, extras: &mut Extras, default_elapsed: bool) {
    *delay = delay.and_then(|d| {
        let tenths = d.minutes.get().checked_mul(10)?;
        delay_value(
            tenths,
            Some(crate::codes::leveling_delay_format_code(d.unit, d.elapsed)),
            default_elapsed,
        )
    });
    let legacy = |e: &ExtraEntry, name: &str| {
        e.origin == ORIGIN
            && ExtraKey::parse(&e.key)
                .is_some_and(|k| k.scope.is_none() && k.anchor == Anchor::None && k.name == name)
            && extras::valid_payload(name, &e.payload)
    };
    let first = |name: &str| {
        extras
            .0
            .iter()
            .find(|e| legacy(e, name))
            .map(|e| leveling_payload_int(&e.payload))
    };
    let (value, format) = (first("LevelingDelay"), first("LevelingDelayFormat"));
    if delay.is_none() {
        *delay = value
            .flatten()
            .and_then(|tenths| delay_value(tenths, format.flatten(), default_elapsed));
    }
    extras
        .0
        .retain(|e| !(legacy(e, "LevelingDelay") || legacy(e, "LevelingDelayFormat")));
}

/// Plages d'un jour ouvré sans `WorkingTimes` à la lecture (R-078).
fn default_hours() -> DayRule {
    let range = |a: u16, b: u16| {
        TimeOfDay::new(a)
            .ok()
            .zip(TimeOfDay::new(b).ok())
            .and_then(|(a, b)| TimeRange::new(a, b).ok())
    };
    let ranges: Vec<TimeRange> = [range(480, 720), range(780, 1_020)]
        .into_iter()
        .flatten()
        .collect();
    DayHours::new(ranges).map_or(DayRule::NonWorking, DayRule::Working)
}

/// Un calendrier, reconstruit.
fn calendar(c: &Calendar, options: &WriteOptions) -> Calendar {
    let strict = options.profile == OrderProfile::Strict;
    let name = string(&c.name, Parent::Calendar, "Name");
    let mut out = match c.kind() {
        CalendarKind::Base { week } => Calendar::new_base(c.uid(), name, week.clone()),
        CalendarKind::Derived { base, week } => {
            let mut d = Calendar::new_derived(c.uid(), name, *base);
            for day in Weekday::ALL {
                let _ = d.set_day(day, week.get(day).clone());
            }
            d
        }
    };
    for w in c.work_weeks() {
        let mut w = w.clone();
        w.name = string(&w.name, Parent::WorkWeek, "Name");
        if strict {
            // Forme pj15 : pas de plages sous `WorkWeek/WeekDay` (R-128).
            w.days = w.days.map(|r| match r {
                DayRule::Working(_) => default_hours(),
                other => other.clone(),
            });
        }
        let _ = out.add_work_week(w);
    }
    for e in c.exceptions() {
        let mut e: CalendarException = e.clone();
        e.name = string(&e.name, Parent::Exception, "Name");
        if e.recurrence == Recurrence::Period {
            e.recurrence = Recurrence::Daily { every: 1 };
        }
        let _ = out.add_exception(e);
    }
    out.guid = if strict { None } else { c.guid };
    out.extras = extras_image(&c.extras, Parent::Calendar, &[], options.profile, &|_| false);
    out
}

/// Taux relu à travers l'écriture par heure (R-124, garde-fou).
fn rate(r: Rate, factors: &plannercraft_core::ConversionFactors) -> Rate {
    let text = hourly_rate_text(r, factors);
    let amount = parse_decimal(&text).and_then(|d| rate_amount(d.value, r.per, factors));
    Rate {
        amount: amount.unwrap_or(r.amount),
        per: r.per,
    }
}

/// Une ressource.
fn resource(r: &mut Resource, factors: &plannercraft_core::ConversionFactors, options: &WriteOptions) {
    r.name = string(&r.name, Parent::Resource, "Name");
    r.initials = string(&r.initials, Parent::Resource, "Initials");
    r.group = string(&r.group, Parent::Resource, "Group");
    r.notes = string(&r.notes, Parent::Resource, "Notes");
    if let plannercraft_core::ResourceKind::Material { label } = &mut r.kind {
        *label = string(label, Parent::Resource, "MaterialLabel");
    }
    if options.profile == OrderProfile::Strict {
        r.guid = None;
    }
    r.standard_rate = rate(r.standard_rate, factors);
    r.overtime_rate = rate(r.overtime_rate, factors);
    let numbers: Vec<u8> = r.baselines.keys().map(|n| n.get()).collect();
    r.extras = extras_image(&r.extras, Parent::Resource, &numbers, options.profile, &|_| false);
}

/// `Extras` d'une entité après l'aller-retour (R-123, R-087) : invalides et
/// origines étrangères retirés, hors schéma retirés en profil strict, ancres
/// recalculées sur les éléments effectivement réécrits, ordre canonique.
/// `written` : éléments M? que l'on sait réécrits (conteneurs du projet).
fn extras_image(
    extras: &Extras,
    parent: Parent,
    baselines: &[u8],
    profile: Profile,
    written: &dyn Fn(&str) -> bool,
) -> Extras {
    let mut kept: Vec<(ExtraKey, String)> = Vec::new();
    for e in extras.0.iter().filter(|e| e.origin == ORIGIN) {
        let Some(key) = ExtraKey::parse(&e.key) else {
            continue;
        };
        if key.is_attribute() {
            if parent == Parent::Project {
                kept.push((key, e.payload.clone()));
            }
            continue;
        }
        if key.scope.is_some_and(|n| !baselines.contains(&n))
            || extras::placement(parent, &e.key).is_none()
            || !extras::valid_payload(&key.name, &e.payload)
        {
            continue;
        }
        let inner = match key.scope {
            Some(_) => extras::baseline_parent(parent).unwrap_or(parent),
            None => parent,
        };
        if profile == OrderProfile::Strict && sequence(inner).class(&key.name).is_none() {
            continue;
        }
        kept.push((key, e.payload.clone()));
    }
    // Ordre d'écriture (clés d'origine), puis ancres recalculées : un
    // élément hors schéma se relit ancré sur le dernier élément réécrit qui
    // le précède (M, D, F, P présent, conteneur non vide) ; un tri stable
    // garde l'ordre d'écriture entre clés de même emplacement.
    let mut written_order = Extras(
        kept.iter()
            .map(|(k, payload)| ExtraEntry {
                origin: ORIGIN.to_owned(),
                key: k.to_string(),
                payload: payload.clone(),
            })
            .collect(),
    );
    extras::canonical_order(&mut written_order, parent);
    let keys: Vec<(ExtraKey, String)> = written_order
        .0
        .into_iter()
        .filter_map(|e| Some((ExtraKey::parse(&e.key)?, e.payload)))
        .collect();
    let mut out: Vec<ExtraEntry> = Vec::with_capacity(keys.len());
    for (key, payload) in &keys {
        let mut key = key.clone();
        if let Anchor::After(a) = &key.anchor {
            let inner = match key.scope {
                Some(_) => extras::baseline_parent(parent).unwrap_or(parent),
                None => parent,
            };
            let present: BTreeSet<&str> = keys
                .iter()
                .filter(|(k, _)| k.scope == key.scope && k.anchor == Anchor::None)
                .map(|(k, _)| k.name.as_str())
                .collect();
            let eligible = |n: &str| {
                extras::is_static_anchor(inner, n)
                    || (sequence(inner).class(n) == Some(Class::Preserved) && present.contains(n))
                    || (key.scope.is_none() && written(n))
            };
            if !eligible(a) {
                let names: Vec<&str> = sequence(inner).names(Profile::Observed).collect();
                let at = names.iter().position(|n| n == a).unwrap_or(0);
                let before = names.get(..at).unwrap_or_default();
                key.anchor = before
                    .iter()
                    .rev()
                    .find(|n| eligible(n))
                    .map_or(Anchor::Start, |n| Anchor::After((*n).to_owned()));
            }
        }
        out.push(ExtraEntry {
            origin: ORIGIN.to_owned(),
            key: key.to_string(),
            payload: payload.clone(),
        });
    }
    let mut extras = Extras(out);
    extras::canonical_order(&mut extras, parent);
    extras
}
