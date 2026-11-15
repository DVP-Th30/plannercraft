//! Fichiers `.mpp` synthétiques pour les tests, construits d'après les
//! structures consignées dans `docs/formats/mpp.md` ; le conteneur est
//! écrit par la crate `cfb` (écrivain indépendant du lecteur maison).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    dead_code,
    reason = "outils de test"
)]

use std::io::{Cursor, Write as _};

use plannercraft_core::Moment;
use plannercraft_mpp::{Layout, ReadOptions};

/// « Maintenant » des tests.
pub fn now() -> Moment {
    "2026-11-14T08:00".parse().unwrap()
}

/// Options de lecture des tests.
pub fn options() -> ReadOptions {
    ReadOptions::new("synth".to_owned(), now())
}

/// Horodatage : jour (1 = 1984-01-01) et heure en dixièmes de minute.
pub fn stamp(s: &str) -> u32 {
    let m: Moment = s.parse().unwrap();
    let minutes = m.minutes();
    let day = (minutes / 1_440 + 1) as u32;
    let tenths = ((minutes % 1_440) * 10) as u32;
    (day << 16) | tenths
}

/// Aucune date.
pub const NA: u32 = 0xFFFF_FFFF;

fn put16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

/// Valeur UTF-16 terminée par un nul.
pub fn utf16(s: &str) -> Vec<u8> {
    let mut v: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
    v.extend_from_slice(&[0, 0]);
    v
}

/// Table fixe : éléments de méta et enregistrements.
#[derive(Default, Clone)]
pub struct Fixed {
    items: Vec<(Vec<u8>, Vec<u8>)>,
    item_len: usize,
}

impl Fixed {
    pub fn new(item_len: usize) -> Self {
        Self {
            items: Vec::new(),
            item_len,
        }
    }

    /// Ajoute un enregistrement ; `meta_bits` : (octet, masque) posés dans
    /// l'élément de méta ; `flags` à l'octet 0.
    pub fn push(&mut self, flags: u16, meta_bits: &[(usize, u8)], record: Vec<u8>) {
        let mut meta = vec![0_u8; self.item_len];
        put16(&mut meta, 0, flags);
        for (byte, mask) in meta_bits {
            meta[*byte] |= mask;
        }
        self.items.push((meta, record));
    }

    /// (`FixedMeta`, `FixedData`).
    pub fn build(&self) -> (Vec<u8>, Vec<u8>) {
        let mut data = Vec::new();
        let mut meta = vec![0_u8; 16];
        meta[..4].copy_from_slice(&[0xBA, 0xAD, 0xDF, 0xFA]);
        put32(&mut meta, 8, self.items.len() as u32);
        for (m, rec) in &self.items {
            let mut m = m.clone();
            put32(&mut m, 4, data.len() as u32);
            meta.extend_from_slice(&m);
            data.extend_from_slice(rec);
        }
        put32(&mut meta, 12, data.len() as u32);
        (meta, data)
    }

    /// Trois enregistrements réservés (« n 00 ff ff »), comme en tête de
    /// chaque table observée.
    pub fn reserved(&mut self) {
        for n in 0..3_u16 {
            let mut rec = vec![0_u8; 16];
            put16(&mut rec, 0, n);
            put16(&mut rec, 2, 0xFFFF);
            self.push(4, &[], rec);
        }
    }
}

/// Valeurs variables : (UID, clé, octets).
pub fn var(entries: &[(u32, u32, Vec<u8>)]) -> (Vec<u8>, Vec<u8>) {
    let mut data = Vec::new();
    let mut meta = vec![0_u8; 24];
    meta[..4].copy_from_slice(&[0xBA, 0xAD, 0xDF, 0xFA]);
    put32(&mut meta, 8, entries.len() as u32);
    for (uid, key, value) in entries {
        let mut e = vec![0_u8; 12];
        put32(&mut e, 0, *uid);
        put32(&mut e, 4, data.len() as u32);
        put32(&mut e, 8, *key);
        meta.extend_from_slice(&e);
        data.extend_from_slice(&(value.len() as u32).to_le_bytes());
        data.extend_from_slice(value);
    }
    put32(&mut meta, 20, data.len() as u32);
    (meta, data)
}

/// Propriétés : (clé, octets).
pub fn props(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![0_u8; 16];
    for (key, value) in entries {
        out.extend_from_slice(&(value.len() as u32).to_le_bytes());
        out.extend_from_slice(&key.to_le_bytes());
        out.extend_from_slice(&(value.len() as u32).to_le_bytes());
        out.extend_from_slice(value);
        // Valeur de longueur impaire : un octet de bourrage.
        if value.len() % 2 == 1 {
            out.push(0);
        }
    }
    out
}

/// Tâche synthétique.
#[derive(Clone)]
pub struct T {
    pub uid: u32,
    pub id: u32,
    pub name: &'static str,
    pub level: u16,
    /// Dixièmes de minute.
    pub duration: i32,
    pub format: u16,
    pub start: u32,
    pub finish: u32,
    pub constraint: (u16, u32),
    pub percent: u16,
    pub calendar: i32,
    pub deadline: u32,
    pub milestone: bool,
    pub manual: Option<(u32, u32, i32)>,
    pub active: bool,
    pub notes: Option<&'static [u8]>,
    /// Ligne vide (enregistrement court).
    pub null: bool,
    /// Enregistrement supprimé.
    pub deleted: bool,
    /// Début réel, fin réelle (horodatages), durée réelle (dixièmes).
    pub actual: (u32, u32, i32),
    /// Reprise (horodatage).
    pub resume: u32,
    /// Retard de nivellement (dixièmes) et son format.
    pub leveling: (i32, u16),
    /// Durée de l'enregistrement fixe si elle diffère de `duration`.
    pub fixed_duration: Option<i32>,
}

impl T {
    pub fn new(uid: u32, id: u32, name: &'static str) -> Self {
        Self {
            uid,
            id,
            name,
            level: 1,
            duration: 4_800,
            format: 7,
            start: stamp("2026-01-05T08:00"),
            finish: stamp("2026-01-05T17:00"),
            constraint: (0, NA),
            percent: 0,
            calendar: -1,
            deadline: NA,
            milestone: false,
            manual: None,
            active: true,
            notes: None,
            null: false,
            deleted: false,
            actual: (NA, NA, 0),
            resume: NA,
            leveling: (0, 7),
            fixed_duration: None,
        }
    }
}

/// Décalages par disposition (copie de `docs/formats/mpp.md` § 7).
struct Off {
    len: usize,
    uid: usize,
    id: usize,
    start: usize,
    finish: usize,
    duration: usize,
    format: usize,
    level: usize,
    ctype: usize,
    cdate: usize,
    priority: usize,
    percent: usize,
    calendar: usize,
    deadline: usize,
    milestone: (usize, u8),
    manual: (usize, u8),
    active: (usize, u8),
    /// Autres dates de l'enregistrement (création, réels, reprise) : NA.
    other_dates: [usize; 4],
    actual: (usize, usize, usize),
    resume: usize,
    leveling: (usize, usize),
}

fn off(layout: Layout) -> Off {
    match layout {
        Layout::Project2010 => Off {
            len: 206,
            uid: 0,
            id: 4,
            start: 106,
            finish: 8,
            duration: 42,
            format: 46,
            level: 40,
            ctype: 56,
            cdate: 80,
            priority: 88,
            percent: 90,
            calendar: 118,
            deadline: 122,
            milestone: (8, 0x20),
            manual: (8, 0x08),
            active: (8, 0x04),
            other_dates: [98, 72, 76, 20],
            actual: (72, 76, 48),
            resume: 20,
            leveling: (58, 62),
        },
        Layout::Project2013 => Off {
            len: 202,
            uid: 4,
            id: 0,
            start: 104,
            finish: 108,
            duration: 84,
            format: 164,
            level: 172,
            ctype: 64,
            cdate: 66,
            priority: 78,
            percent: 92,
            calendar: 178,
            deadline: 182,
            milestone: (10, 0x02),
            manual: (8, 0x80),
            active: (8, 0x40),
            other_dates: [128, 120, 124, 132],
            actual: (120, 124, 80),
            resume: 132,
            leveling: (70, 162),
        },
    }
}

/// Lien synthétique : (prédécesseur, successeur, type, décalage en
/// dixièmes, format).
pub type L = (u32, u32, u16, i32, u16);

/// Calendrier synthétique : UID, base (0 : de base), nom, bloc semaine.
pub struct C {
    pub uid: u32,
    pub base: i32,
    pub name: &'static str,
    pub data: Option<Vec<u8>>,
}

/// Jour de 60 octets : `None` = jour par défaut ; sinon plages (début,
/// fin) en minutes.
pub fn day(ranges: Option<&[(u32, u32)]>) -> Vec<u8> {
    let mut d = vec![0_u8; 60];
    match ranges {
        None => put16(&mut d, 0, 1),
        Some(r) => {
            put16(&mut d, 2, r.len() as u16);
            let mut total = 0;
            let mut cum = 0;
            for (k, (a, b)) in r.iter().enumerate() {
                put16(&mut d, 8 + 2 * k, (*a * 10) as u16);
                put32(&mut d, 20 + 4 * k, (b - a) * 10);
                cum += (b - a) * 10;
                put32(&mut d, 40 + 4 * k, cum);
                total += (b - a) * 10;
            }
            put32(&mut d, 4, total);
        }
    }
    d
}

/// Exception simple : premier et dernier jour (`AAAA-MM-JJ`), plages, nom.
pub fn exception(first: &str, last: &str, ranges: &[(u32, u32)], name: &str) -> Vec<u8> {
    exception_rec(first, last, ranges, name, 1, [0; 4], None)
}

/// Exception de type `kind` (1 période, 2 annuelle par jour, 3 annuelle par
/// rang, 4 mensuelle par jour, 5 mensuelle par rang, 6 hebdomadaire, 7
/// quotidienne), paramètres des octets 76 à 79, occurrences saisies.
pub fn exception_rec(
    first: &str,
    last: &str,
    ranges: &[(u32, u32)],
    name: &str,
    kind: u32,
    params: [u8; 4],
    occurrences: Option<u32>,
) -> Vec<u8> {
    let dn = |s: &str| {
        let m: Moment = format!("{s}T00:00").parse().unwrap();
        (m.minutes() / 1_440 + 1) as u16
    };
    let mut e = vec![0_u8; 12];
    put16(&mut e, 0, dn(first));
    put16(&mut e, 2, dn(last));
    put32(&mut e, 4, occurrences.unwrap_or(1));
    put32(&mut e, 8, u32::from(occurrences.is_some()));
    e.extend_from_slice(&day(Some(ranges)));
    let n = utf16(name);
    let mut tail = vec![0_u8; 20];
    put32(&mut tail, 0, kind);
    tail[4..8].copy_from_slice(&params);
    put32(&mut tail, 16, n.len() as u32);
    e.extend_from_slice(&tail);
    e.extend_from_slice(&n);
    e
}

/// Bloc semaine (7 jours, dimanche d'abord) et exceptions.
pub fn week(days: [Vec<u8>; 7], exceptions: &[Vec<u8>]) -> Vec<u8> {
    let mut b: Vec<u8> = days.concat();
    b.extend_from_slice(&(exceptions.len() as u32).to_le_bytes());
    for e in exceptions {
        b.extend_from_slice(e);
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
    }
    b
}

/// Ressource synthétique : UID, nom, nature (0 travail, 1 matérielle,
/// 2 coût), unités maximales en 1/10 000, calendrier (UID ou 0).
#[derive(Clone)]
pub struct R {
    pub uid: u32,
    pub name: &'static str,
    pub kind: u8,
    pub max_units: f64,
    pub calendar: i32,
}

/// Affectation synthétique.
#[derive(Clone)]
pub struct A {
    pub uid: u32,
    pub task: u32,
    /// −65 535 : aucune ressource.
    pub resource: i32,
    /// En 1/10 000.
    pub units: f64,
    /// Millièmes de minute.
    pub work: f64,
    pub actual_work: f64,
    /// Dixièmes de minute (disposition Project 2013 seulement).
    pub delay: i32,
    /// Code du profil (0 à 7) dans la répartition.
    pub contour: u8,
    /// Profil modifié à la main (bit de l'élément de méta).
    pub contoured: bool,
}

impl A {
    pub fn new(uid: u32, task: u32, resource: i32, units: f64, work_minutes: f64) -> Self {
        Self {
            uid,
            task,
            resource,
            units,
            work: work_minutes * 1_000.0,
            actual_work: 0.0,
            delay: 0,
            contour: 0,
            contoured: false,
        }
    }
}

/// Fichier `.mpp` synthétique.
pub struct Mpp {
    pub resources: Vec<R>,
    pub assignments: Vec<A>,
    pub layout: Layout,
    pub tasks: Vec<T>,
    pub links: Vec<L>,
    pub calendars: Vec<C>,
    pub props: Vec<(u32, Vec<u8>)>,
    /// Nom du dossier des données (« `   114` »).
    pub dir: &'static str,
}

impl Mpp {
    pub fn new(layout: Layout) -> Self {
        Self {
            resources: Vec::new(),
            assignments: Vec::new(),
            layout,
            tasks: vec![T::new(0, 0, "Projet")],
            links: Vec::new(),
            calendars: vec![C {
                uid: 1,
                base: 0,
                name: "Standard",
                data: None,
            }],
            props: vec![
                (0x0240_0002, stamp("2026-01-05T08:00").to_le_bytes().to_vec()),
                (0x0240_0003, stamp("2026-01-30T17:00").to_le_bytes().to_vec()),
                (0x0240_0004, 1_u16.to_le_bytes().to_vec()),
                (0x0240_0019, 1_u16.to_le_bytes().to_vec()),
                (0x0240_0008, utf16("Projet synthétique")),
                (0x0240_000E, utf16("Standard")),
                (0x0240_001D, 480_u32.to_le_bytes().to_vec()),
                (0x0240_001E, 2_400_u32.to_le_bytes().to_vec()),
                (0x0240_138F, 20_u16.to_le_bytes().to_vec()),
            ],
            dir: "   114",
        }
    }

    fn saved_by(&self) -> &'static str {
        match self.layout {
            Layout::Project2010 => "14,0,7011,1000",
            Layout::Project2013 => "16,0,10730,20102",
        }
    }

    /// Octets du fichier.
    pub fn bytes(&self) -> Vec<u8> {
        let o = off(self.layout);
        let mut fixed = Fixed::new(47);
        let mut fixed2 = Fixed::new(93);
        fixed.reserved();
        fixed2.reserved();
        let mut vars = Vec::new();
        for t in &self.tasks {
            let deleted = if t.deleted { 2 } else { 0 };
            if t.null {
                let mut rec = vec![0_u8; 16];
                put32(&mut rec, 0, t.uid);
                put32(&mut rec, 4, t.id);
                fixed.push(4 | deleted, &[], rec);
                fixed2.push(4 | deleted, &[], vec![0_u8; 64]);
                continue;
            }
            let mut rec = vec![0_u8; o.len];
            for off in o.other_dates {
                put32(&mut rec, off, NA);
            }
            put32(&mut rec, o.uid, t.uid);
            put32(&mut rec, o.id, t.id);
            put32(&mut rec, o.start, t.start);
            put32(&mut rec, o.finish, t.finish);
            put32(
                &mut rec,
                o.duration,
                t.fixed_duration.unwrap_or(t.duration) as u32,
            );
            put32(&mut rec, o.actual.0, t.actual.0);
            put32(&mut rec, o.actual.1, t.actual.1);
            put32(&mut rec, o.actual.2, t.actual.2 as u32);
            put32(&mut rec, o.resume, t.resume);
            put32(&mut rec, o.leveling.0, t.leveling.0 as u32);
            put16(&mut rec, o.leveling.1, t.leveling.1);
            put16(&mut rec, o.format, t.format);
            put16(&mut rec, o.level, t.level);
            put16(&mut rec, o.ctype, t.constraint.0);
            put32(&mut rec, o.cdate, t.constraint.1);
            put16(&mut rec, o.priority, 500);
            put16(&mut rec, o.percent, t.percent);
            put32(&mut rec, o.calendar, t.calendar as u32);
            put32(&mut rec, o.deadline, t.deadline);
            let mut bits = Vec::new();
            if t.milestone {
                bits.push(o.milestone);
            }
            fixed.push(deleted, &bits, rec);
            let mut rec2 = vec![0xFF_u8; 64];
            let mut bits2 = Vec::new();
            if let Some((s, f, d)) = t.manual {
                bits2.push(o.manual);
                put32(&mut rec2, 50, s);
                put32(&mut rec2, 54, f);
                put32(&mut rec2, 58, d as u32);
            }
            if t.active {
                bits2.push(o.active);
            }
            fixed2.push(deleted, &bits2, rec2);
            vars.push((t.uid, 0x0B40_000E, utf16(t.name)));
            if let Some(n) = t.notes {
                vars.push((t.uid, 0x0B40_000F, n.to_vec()));
            }
        }
        let mut cons = Fixed::new(10);
        for (k, (p, s, kind, lag, fmt)) in self.links.iter().enumerate() {
            let mut rec = vec![0_u8; 20];
            put32(&mut rec, 0, k as u32 + 1);
            put32(&mut rec, 4, *p);
            put32(&mut rec, 8, *s);
            put16(&mut rec, 12, *kind);
            match self.layout {
                Layout::Project2010 => {
                    put16(&mut rec, 14, *fmt);
                    put32(&mut rec, 16, *lag as u32);
                }
                Layout::Project2013 => {
                    put32(&mut rec, 14, *lag as u32);
                    put16(&mut rec, 18, *fmt);
                }
            }
            cons.push(0, &[], rec);
        }
        let mut cal = Fixed::new(10);
        cal.reserved();
        let mut cal_vars = Vec::new();
        for c in &self.calendars {
            let mut rec = vec![0_u8; 12];
            match self.layout {
                Layout::Project2010 => {
                    put32(&mut rec, 0, c.uid);
                    put32(&mut rec, 4, if c.base == 0 { u32::MAX } else { c.base as u32 });
                    put32(&mut rec, 8, if c.base == 0 { u32::MAX } else { 0 });
                }
                Layout::Project2013 => {
                    put32(&mut rec, 0, c.base as u32);
                    put32(&mut rec, 4, if c.base == 0 { u32::MAX } else { 0 });
                    put32(&mut rec, 8, c.uid);
                }
            }
            cal.push(0, &[], rec);
            cal_vars.push((c.uid, 0x0D40_0001, utf16(c.name)));
            if let Some(d) = &c.data {
                cal_vars.push((c.uid, 0x0D40_0008, d.clone()));
            }
        }
        let (tm, td) = fixed.build();
        let (t2m, t2d) = fixed2.build();
        let (tvm, tvd) = var(&vars);
        let (cm, cd) = cons.build();
        let (calm, cald) = cal.build();
        let (calvm, calvd) = var(&cal_vars);
        // Ressources (docs/formats/mpp.md § 14) et affectations (§ 15).
        let mut rsc = Fixed::new(37);
        let mut rsc2 = Fixed::new(50);
        rsc.reserved();
        rsc2.reserved();
        let mut rsc_vars = Vec::new();
        for r in &self.resources {
            let (len, uid, max, cal, kind) = match self.layout {
                Layout::Project2010 => (188, 0, 44, 16, 14),
                Layout::Project2013 => (172, 4, 8, 154, 170),
            };
            let mut rec = vec![0_u8; len];
            put32(&mut rec, uid, r.uid);
            rec[max..max + 8].copy_from_slice(&r.max_units.to_le_bytes());
            put32(&mut rec, cal, r.calendar as u32);
            put16(&mut rec, kind, u16::from(r.kind != 0));
            rsc.push(0, &[], rec);
            let bits: Vec<(usize, u8)> = if r.kind == 2 { vec![(8, 0x10)] } else { Vec::new() };
            rsc2.push(0, &bits, vec![0_u8; 64]);
            rsc_vars.push((r.uid, 0x0C40_0001, utf16(r.name)));
        }
        let mut assn = Fixed::new(34);
        let mut assn_vars = Vec::new();
        for a in &self.assignments {
            let mut rec = vec![0_u8; 110];
            put32(&mut rec, 0, a.uid);
            put32(&mut rec, 4, a.task);
            put32(&mut rec, 8, a.resource as u32);
            let (units, work, actual, bit) = match self.layout {
                Layout::Project2010 => (46, 54, 62, (8, 0x10)),
                Layout::Project2013 => (12, 20, 28, (10, 0x04)),
            };
            rec[units..units + 8].copy_from_slice(&a.units.to_le_bytes());
            rec[work..work + 8].copy_from_slice(&a.work.to_le_bytes());
            rec[actual..actual + 8].copy_from_slice(&a.actual_work.to_le_bytes());
            if self.layout == Layout::Project2013 {
                put32(&mut rec, 64, a.delay as u32);
            }
            let bits: Vec<(usize, u8)> = if a.contoured { vec![bit] } else { Vec::new() };
            assn.push(0, &bits, rec);
            let mut dist = vec![0_u8; 68];
            dist[28] = a.contour;
            assn_vars.push((a.uid, 0x0F40_0031, dist));
        }
        let (rm, rd) = rsc.build();
        let (r2m, r2d) = rsc2.build();
        let (rvm, rvd) = var(&rsc_vars);
        let (am, ad) = assn.build();
        let (avm, avd) = var(&assn_vars);
        let mut streams: Vec<(String, Vec<u8>)> = vec![
            ("/Props14".to_owned(), utf16(self.saved_by())),
            (format!("/{}/Props", self.dir), props(&self.props)),
        ];
        for (name, data) in [
            ("TBkndTask/FixedMeta", tm),
            ("TBkndTask/FixedData", td),
            ("TBkndTask/Fixed2Meta", t2m),
            ("TBkndTask/Fixed2Data", t2d),
            ("TBkndTask/VarMeta", tvm),
            ("TBkndTask/Var2Data", tvd),
            ("TBkndCons/FixedMeta", cm),
            ("TBkndCons/FixedData", cd),
            ("TBkndCal/FixedMeta", calm),
            ("TBkndCal/FixedData", cald),
            ("TBkndCal/VarMeta", calvm),
            ("TBkndCal/Var2Data", calvd),
            ("TBkndRsc/FixedMeta", rm),
            ("TBkndRsc/FixedData", rd),
            ("TBkndRsc/Fixed2Meta", r2m),
            ("TBkndRsc/Fixed2Data", r2d),
            ("TBkndRsc/VarMeta", rvm),
            ("TBkndRsc/Var2Data", rvd),
            ("TBkndAssn/FixedMeta", am),
            ("TBkndAssn/FixedData", ad),
            ("TBkndAssn/VarMeta", avm),
            ("TBkndAssn/Var2Data", avd),
        ] {
            streams.push((format!("/{}/{name}", self.dir), data));
        }
        container(&streams)
    }
}

/// Fichier composé avec ces flux (dossiers créés au besoin).
pub fn container(streams: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut comp = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    for (path, data) in streams {
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let mut dir = String::new();
        for p in &parts[..parts.len() - 1] {
            dir.push('/');
            dir.push_str(p);
            if !comp.exists(&dir) {
                comp.create_storage(&dir).unwrap();
            }
        }
        let mut s = comp.create_stream(path).unwrap();
        s.write_all(data).unwrap();
    }
    comp.flush().unwrap();
    comp.into_inner().into_inner()
}
