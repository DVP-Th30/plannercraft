//! Entrée hostile (règle 10) : aucune entrée ne fait paniquer ni boucler la
//! lecture ; elle rend un projet ou une erreur. Octets synthétiques (vide,
//! en-tête seul, conteneur sans les flux attendus, chaînes de secteurs
//! bouclées, tailles fausses), puis fichiers synthétiques et fichiers du
//! corpus (s'il est présent) tronqués et altérés au hasard, graine fixe.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "tests : un échec doit paniquer"
)]

mod common;

use std::path::Path;

use common::{C, Mpp, T, day, exception, week};
use plannercraft_mpp::{Layout, MppError, read};

/// Générateur déterministe (xorshift64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % (n.max(1) as u64)) as usize
    }
}

/// Lit `bytes` : le seul résultat attendu est de rendre la main.
fn survive(bytes: &[u8]) -> bool {
    match read(bytes, &common::options()) {
        Ok(doc) => {
            // Le projet rendu se calcule sans paniquer non plus.
            let s = plannercraft_core::schedule(&doc.project);
            let _ = plannercraft_mpp::compare(&doc.project, &s, &doc.recorded);
            let _ = plannercraft_mpp::report::text(&doc, None, true);
            true
        }
        Err(_) => false,
    }
}

/// Tronque et altère `orig` `rounds` fois.
fn mutate_all(orig: &[u8], rounds: usize, seed: u64) -> (usize, usize) {
    let mut rng = Rng(seed);
    let (mut ok, mut err) = (0, 0);
    for k in 0..rounds {
        let mut b = orig.to_vec();
        match k % 3 {
            0 => b.truncate(rng.below(orig.len())),
            1 => {
                for _ in 0..=rng.below(16) {
                    let i = rng.below(b.len());
                    b[i] = rng.next() as u8;
                }
            }
            _ => {
                // Un entier de 32 bits remplacé par une valeur extrême
                // (taille, décalage, nombre d'éléments annoncés faux).
                let i = rng.below(b.len().saturating_sub(4));
                let v: u32 = match rng.below(4) {
                    0 => u32::MAX,
                    1 => 0x7FFF_FFFF,
                    2 => 0xFFFF_FFFE,
                    _ => rng.next() as u32,
                };
                b[i..i + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        if survive(&b) {
            ok += 1;
        } else {
            err += 1;
        }
    }
    (ok, err)
}

fn rich(layout: Layout) -> Vec<u8> {
    let mut m = Mpp::new(layout);
    for u in 1..=6 {
        let mut t = T::new(u, u, "tâche");
        t.level = if u % 2 == 0 { 2 } else { 1 };
        t.notes = Some(br"{\rtf1 note\par}");
        m.tasks.push(t);
    }
    m.links = vec![(1, 3, 1, 0, 7), (3, 5, 3, 4_800, 7)];
    m.calendars.push(C {
        uid: 3,
        base: 0,
        name: "Chantier",
        data: Some(week(
            [
                day(Some(&[])),
                day(None),
                day(None),
                day(None),
                day(None),
                day(None),
                day(Some(&[])),
            ],
            &[exception("2026-12-25", "2026-12-25", &[], "Noël")],
        )),
    });
    m.bytes()
}

#[test]
fn hostile_synthetic_bytes() {
    // Vide, signature seule, en-tête seul (512 octets).
    assert_eq!(read(&[], &common::options()).unwrap_err(), MppError::NotCompound);
    let sig = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    assert_eq!(read(&sig, &common::options()).unwrap_err(), MppError::Corrupt);
    let mut header = vec![0_u8; 512];
    header[..8].copy_from_slice(&sig);
    assert_eq!(read(&header, &common::options()).unwrap_err(), MppError::Corrupt);
    // Conteneur valide sans les flux attendus.
    let empty = common::container(&[("/   114/Rien".to_owned(), vec![0; 10])]);
    assert_eq!(
        read(&empty, &common::options()).unwrap_err(),
        MppError::UnknownLayout
    );
    let no_tasks = common::container(&[
        ("/Props14".to_owned(), common::utf16("16,0,1,1")),
        ("/   114/Props".to_owned(), vec![0; 16]),
    ]);
    assert_eq!(
        read(&no_tasks, &common::options()).unwrap_err(),
        MppError::MissingStream("TBkndTask/FixedMeta")
    );
    // Un fichier valide dont la FAT pointe sur elle-même : erreur, pas de
    // boucle.
    let mut b = rich(Layout::Project2013);
    let first_fat = u32::from_le_bytes(b[0x4C..0x50].try_into().unwrap()) as usize;
    let shift = u16::from_le_bytes(b[0x1E..0x20].try_into().unwrap());
    let fat_at = (first_fat + 1) << shift;
    let first_dir = u32::from_le_bytes(b[0x30..0x34].try_into().unwrap());
    let entry = fat_at + 4 * first_dir as usize;
    b[entry..entry + 4].copy_from_slice(&first_dir.to_le_bytes());
    assert!(read(&b, &common::options()).is_err());
    // Nombre de secteurs de FAT démesuré.
    let mut b = rich(Layout::Project2013);
    b[0x2C..0x30].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(read(&b, &common::options()).unwrap_err(), MppError::Corrupt);
}

#[test]
fn mutated_synthetic_files_never_panic() {
    for (layout, seed) in [
        (Layout::Project2010, 0x5EED_0001),
        (Layout::Project2013, 0x5EED_0002),
    ] {
        let orig = rich(layout);
        assert!(survive(&orig));
        let (ok, err) = mutate_all(&orig, 600, seed);
        assert_eq!(ok + err, 600);
    }
}

/// Fichiers du corpus présents (`cargo xtask corpus fetch-mpp`), sinon
/// rien : le test ne dépend pas du réseau. `MPP_ROBUST_DIR` (dossier) et
/// `MPP_ROBUST_FILES` (nombre de fichiers, 8 par défaut) élargissent la
/// mesure en local.
#[test]
fn mutated_corpus_files_never_panic() {
    let dir = std::env::var("MPP_ROBUST_DIR").map_or_else(
        |_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus-mpp/files"),
        std::path::PathBuf::from,
    );
    let take = std::env::var("MPP_ROBUST_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    files.sort();
    for (k, path) in files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "mpp"))
        .take(take)
        .enumerate()
    {
        let orig = std::fs::read(path).unwrap();
        let (ok, err) = mutate_all(&orig, 150, 0xC0DE_0000 + k as u64);
        assert_eq!(ok + err, 150);
    }
}

/// Valeurs variables : des milliers d'entrées qui désignent la même grande
/// valeur ne la recopient pas (aucune amplification d'allocation).
#[test]
fn shared_variable_values_are_not_copied() {
    let m = Mpp::new(Layout::Project2013);
    let mut streams = vec![
        ("/Props14".to_owned(), common::utf16("16,0,1,1")),
        ("/   114/Props".to_owned(), common::props(&m.props)),
    ];
    let mut fixed = common::Fixed::new(47);
    let mut rec = vec![0_u8; 202];
    rec[4] = 1;
    rec[0] = 1;
    fixed.push(0, &[], rec);
    let (fm, fd) = fixed.build();
    let big = 1_usize << 20;
    let mut data = (big as u32).to_le_bytes().to_vec();
    data.resize(4 + big, 0x41);
    let n = 200_000_u32;
    let mut meta = vec![0_u8; 24];
    meta[8..12].copy_from_slice(&n.to_le_bytes());
    for k in 0..n {
        meta.extend_from_slice(&1_u32.to_le_bytes());
        meta.extend_from_slice(&0_u32.to_le_bytes());
        meta.extend_from_slice(&(0x0B40_0000 + k).to_le_bytes());
    }
    streams.push(("/   114/TBkndTask/FixedMeta".to_owned(), fm));
    streams.push(("/   114/TBkndTask/FixedData".to_owned(), fd));
    streams.push(("/   114/TBkndTask/VarMeta".to_owned(), meta));
    streams.push(("/   114/TBkndTask/Var2Data".to_owned(), data));
    let bytes = common::container(&streams);
    // 200 000 × 1 Mio recopiés feraient 200 Gio : la lecture doit rendre la
    // main sans les allouer.
    let doc = read(&bytes, &common::options()).unwrap();
    assert_eq!(doc.project.tasks.len(), 1);
}
