//! C-REL-03 (J8, lot L1) : `plannercraft --version` imprime `PlannerCraft <version>`
//! (version du workspace) et sort aussitôt, sans fenêtre. Le binaire est
//! lancé par `CARGO_BIN_EXE_plannercraft` ; un processus encore vivant après le
//! délai (fenêtre ouverte, attente) fait échouer le test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Délai au-delà duquel le binaire est réputé avoir ouvert une fenêtre.
const LIMIT: Duration = Duration::from_secs(30);

/// Lance le binaire avec `arg`, attend sa fin (tué au-delà de [`LIMIT`])
/// et rend son code de sortie et sa sortie standard.
fn run(arg: &str) -> (bool, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_plannercraft"))
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("`plannercraft {arg}` encore vivant après {LIMIT:?} : fenêtre ouverte ?");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    (status.success(), out)
}

#[test]
fn version_flag_no_window() {
    let expected = format!("PlannerCraft {}\n", plannercraft_core::PRODUCT_VERSION);
    for arg in ["--version", "-V"] {
        let (success, out) = run(arg);
        assert!(success, "`plannercraft {arg}` : code d'échec");
        assert_eq!(out, expected, "`plannercraft {arg}`");
    }
    assert_eq!(plannercraft_core::PRODUCT_VERSION, env!("CARGO_PKG_VERSION"));
}
