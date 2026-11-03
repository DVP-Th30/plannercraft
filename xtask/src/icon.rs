//! `cargo xtask icon [--check]` : icône de l'application (J8, lot L1),
//! **à valider par le propriétaire** (ADR-0016, actions du propriétaire).
//!
//! Dessin : carré arrondi sombre (gabarit des icônes macOS : marge de
//! 100/1024, rayon 185/1024) portant trois barres de Gantt décalées, aux
//! couleurs de la palette Okabe-Ito (lisibles par les daltoniens). Rendu
//! en entiers seulement, sur-échantillonné 4 × 4 par pixel (bords lissés),
//! encodé en PNG par le crate `image` : mêmes octets à chaque exécution.
//! Écrit `assets/icon/plannercraft-<n>.png` pour n = 16, 32, 64, 128, 256, 512,
//! 1024 ; la CI en tire le `.icns` macOS (`iconutil`) et l'icône Linux.
//!
//! `--check` échoue si un fichier du dépôt diffère du dessin (même contrôle
//! par le test `icon_files_fresh`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use image::ImageEncoder as _;
use image::codecs::png::PngEncoder;

/// Côtés produits, en pixels.
pub const SIZES: [u32; 7] = [16, 32, 64, 128, 256, 512, 1024];

/// Dossier écrit, relatif à la racine du dépôt.
const DIR: &str = "assets/icon";

/// Côté de la grille de dessin (unités des formes ci-dessous).
const GRID: i64 = 1024;

/// Sous-échantillons par côté de pixel.
const SUB: i64 = 4;

/// Couleur opaque `(r, g, b)`.
type Rgb = (u8, u8, u8);

/// Rectangle arrondi de la grille : gauche, haut, droite, bas, rayon.
#[derive(Clone, Copy)]
struct Shape {
    left: i64,
    top: i64,
    right: i64,
    bottom: i64,
    radius: i64,
    color: Rgb,
}

/// Formes, de l'arrière vers l'avant.
const SHAPES: [Shape; 4] = [
    // Fond : ardoise sombre.
    Shape {
        left: 100,
        top: 100,
        right: 924,
        bottom: 924,
        radius: 185,
        color: (0x1F, 0x29, 0x33),
    },
    // Barres de Gantt, extrémités en demi-cercle (Okabe-Ito : bleu ciel,
    // orange, vert bleuté).
    Shape {
        left: 230,
        top: 290,
        right: 560,
        bottom: 400,
        radius: 55,
        color: (0x56, 0xB4, 0xE9),
    },
    Shape {
        left: 420,
        top: 457,
        right: 770,
        bottom: 567,
        radius: 55,
        color: (0xE6, 0x9F, 0x00),
    },
    Shape {
        left: 320,
        top: 624,
        right: 800,
        bottom: 734,
        radius: 55,
        color: (0x00, 0x9E, 0x73),
    },
];

impl Shape {
    /// Le point `(x, y)`, en grille × `scale`, est dans la forme.
    fn contains(&self, x: i64, y: i64, scale: i64) -> bool {
        let s = |v: i64| v.saturating_mul(scale);
        let r = s(self.radius);
        let (left, top, right, bottom) = (s(self.left), s(self.top), s(self.right), s(self.bottom));
        if x < left || x > right || y < top || y > bottom {
            return false;
        }
        // Distance au rectangle intérieur (coins ôtés du rayon).
        let dx = left
            .saturating_add(r)
            .saturating_sub(x)
            .max(x.saturating_sub(right.saturating_sub(r)))
            .max(0);
        let dy = top
            .saturating_add(r)
            .saturating_sub(y)
            .max(y.saturating_sub(bottom.saturating_sub(r)))
            .max(0);
        dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy)) <= r.saturating_mul(r)
    }
}

/// Pixels RGBA d'une icône de `size` pixels de côté.
fn render(size: u32) -> Vec<u8> {
    let edge = i64::from(size);
    let subs = edge.saturating_mul(SUB);
    // Centre du sous-échantillon `i` : (2i + 1) / (2 × subs) du côté, soit,
    // en grille × (2 × subs) : (2i + 1) × GRID.
    let scale = subs.saturating_mul(2);
    let center = |i: i64| i.saturating_mul(2).saturating_add(1).saturating_mul(GRID);
    let samples = SUB.saturating_mul(SUB);
    let mut out =
        Vec::with_capacity(usize::try_from(edge.saturating_mul(edge).saturating_mul(4)).unwrap_or(0));
    for py in 0..edge {
        for px in 0..edge {
            let (mut red, mut green, mut blue, mut covered) = (0_i64, 0_i64, 0_i64, 0_i64);
            for sy in 0..SUB {
                for sx in 0..SUB {
                    let x = center(px.saturating_mul(SUB).saturating_add(sx));
                    let y = center(py.saturating_mul(SUB).saturating_add(sy));
                    if let Some(shape) = SHAPES.iter().rev().find(|shape| shape.contains(x, y, scale)) {
                        red = red.saturating_add(i64::from(shape.color.0));
                        green = green.saturating_add(i64::from(shape.color.1));
                        blue = blue.saturating_add(i64::from(shape.color.2));
                        covered = covered.saturating_add(1);
                    }
                }
            }
            // Couleur moyenne des échantillons couverts (arrondie), opacité
            // au prorata de la couverture.
            let rounded_div = |sum: i64, by: i64| {
                sum.saturating_add(by.checked_div(2).unwrap_or(0))
                    .checked_div(by)
                    .unwrap_or(0)
            };
            let mean = |sum: i64| rounded_div(sum, covered);
            let alpha = rounded_div(covered.saturating_mul(255), samples);
            for channel in [mean(red), mean(green), mean(blue), alpha] {
                out.push(u8::try_from(channel.clamp(0, 255)).unwrap_or(u8::MAX));
            }
        }
    }
    out
}

/// PNG d'une icône de `size` pixels de côté.
fn png(size: u32) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(&render(size), size, size, image::ExtendedColorType::Rgba8)
        .map_err(|error| format!("PNG {size} px : {error}"))?;
    Ok(bytes)
}

/// Nom du fichier d'un côté.
fn file_name(size: u32) -> String {
    format!("plannercraft-{size}.png")
}

pub fn run(args: &[String]) -> ExitCode {
    let check = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => {
            eprintln!("usage : cargo xtask icon [--check]");
            return ExitCode::FAILURE;
        }
    };
    let dir = repo_root().join(DIR);
    if !check && let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("xtask icon : dossier {DIR} impossible à créer : {error}");
        return ExitCode::FAILURE;
    }
    let mut failed = false;
    for size in SIZES {
        let name = file_name(size);
        let bytes = match png(size) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("xtask icon : {error}");
                failed = true;
                continue;
            }
        };
        let path = dir.join(&name);
        if check {
            if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
                eprintln!("xtask icon : {DIR}/{name} périmé, relancer `cargo xtask icon`");
                failed = true;
            }
        } else if let Err(error) = std::fs::write(&path, bytes) {
            eprintln!("xtask icon : écriture de {DIR}/{name} impossible : {error}");
            failed = true;
        }
    }
    if failed {
        return ExitCode::FAILURE;
    }
    println!(
        "xtask icon : OK ({} fichiers {})",
        SIZES.len(),
        if check { "à jour" } else { "écrits" }
    );
    ExitCode::SUCCESS
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        reason = "tests"
    )]

    use super::*;

    /// Pixel RGBA `(x, y)` d'un rendu de côté `size`.
    fn pixel(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let at = usize::try_from((y * size + x) * 4).unwrap();
        [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
    }

    /// Deux rendus donnent les mêmes octets ; coin transparent, fond opaque,
    /// barre orange au centre.
    #[test]
    fn icon_deterministic_render() {
        assert_eq!(png(64).unwrap(), png(64).unwrap());
        let pixels = render(1024);
        assert_eq!(pixels.len(), 1024 * 1024 * 4);
        assert_eq!(pixel(&pixels, 1024, 2, 2)[3], 0, "coin transparent");
        assert_eq!(pixel(&pixels, 1024, 150, 512), [0x1F, 0x29, 0x33, 255], "fond");
        assert_eq!(
            pixel(&pixels, 1024, 600, 512),
            [0xE6, 0x9F, 0x00, 255],
            "barre orange"
        );
        // Bord lissé : opacité intermédiaire sur l'arrondi du fond.
        let small = render(16);
        assert!((0..16).any(|x| (1..255).contains(&pixel(&small, 16, x, 1)[3])));
    }

    /// Les fichiers du dépôt sont ceux du dessin.
    #[test]
    fn icon_files_fresh() {
        let dir = repo_root().join(DIR);
        for size in SIZES {
            let current = std::fs::read(dir.join(file_name(size))).unwrap();
            assert!(
                current == png(size).unwrap(),
                "{DIR}/{} périmé : relancer `cargo xtask icon`",
                file_name(size)
            );
        }
    }
}
