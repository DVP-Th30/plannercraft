//! Benchmark informatif du moteur (spécification J2 § 6), exécuté en CI
//! (job `perf`) sans seuil : le seuil bloquant est le test `tests/perf`.
//! `cargo bench -p plannercraft-tests --bench schedule`. Comprend l'utilisation
//! des ressources par blocs de 64 semaines (J7, lot L4).

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "benchmark : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use plannercraft_core::resource::{Granularity, usage_by_assignment, usage_with};
use plannercraft_core::{CalendarSet, HasUid, Moment, ResourceUid, schedule};
use plannercraft_tests::generate::{
    assigned_network, assigned_progress_network, chain, fan_in, large_network, progress_network,
};

fn bench(c: &mut Criterion) {
    let large = large_network(2026);
    c.bench_function("schedule/5000_tasks_10000_links", |b| {
        b.iter(|| schedule(black_box(&large)));
    });
    let long = chain(5_000);
    c.bench_function("schedule/chain_5000", |b| b.iter(|| schedule(black_box(&long))));
    let wide = fan_in(10_000);
    c.bench_function("schedule/10000_links_into_one", |b| {
        b.iter(|| schedule(black_box(&wide)));
    });
    // J5 : budget bloquant de 50 ms dans `tests/perf` (`perf_5000_assigned`).
    let assigned = assigned_network(2026);
    c.bench_function("schedule/5000_assigned", |b| {
        b.iter(|| schedule(black_box(&assigned)));
    });
    // J6 : budget bloquant de 50 ms dans `tests/perf` (`perf_5000_progress`).
    let progress = progress_network(2026);
    c.bench_function("schedule/5000_progress", |b| {
        b.iter(|| schedule(black_box(&progress)));
    });
    // J7 (lot L4, ADR-0015 D7) : un bloc de 64 semaines de l'utilisation,
    // toutes ressources, sur des calendriers déjà compilés ; journalisé
    // (cible < 10 ms), sans seuil.
    let sched = schedule(&assigned);
    let (set, _) = CalendarSet::compile(&assigned);
    let resources: Vec<ResourceUid> = assigned.resources.iter().map(HasUid::uid).collect();
    let from: Moment = sched.project_start;
    let to = from
        .checked_add(plannercraft_core::Minutes::new(64 * 7 * 1_440))
        .unwrap();
    c.bench_function("usage/5000_assigned_64_weeks", |b| {
        b.iter(|| {
            usage_with(
                black_box(&assigned),
                &sched,
                &set,
                from,
                to,
                Granularity::Week,
                &resources,
            )
            .unwrap()
        });
    });
    c.bench_function("usage/5000_assigned_64_weeks_by_assignment", |b| {
        b.iter(|| {
            usage_by_assignment(
                black_box(&assigned),
                &sched,
                &set,
                from,
                to,
                Granularity::Week,
                &resources,
            )
            .unwrap()
        });
    });
    let assigned_progress = assigned_progress_network(2026);
    c.bench_function("schedule/5000_assigned_progress", |b| {
        b.iter(|| schedule(black_box(&assigned_progress)));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
