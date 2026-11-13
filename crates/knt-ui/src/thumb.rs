//! The video's thumbnail, made from the stage as it is now: the picture, a dark band at the
//! left, a LIVE badge, the words viewers search for in big letters, and the hook.

use std::path::PathBuf;

use eframe::egui::{Align2, Color32, pos2};
use knt_engine::{CARTA, Snapshot};

use crate::canvas::{Canvas, SkiaCanvas};
use crate::scene::Overlay;
use crate::stage::{self, Visual};

/// The bold font of the thumbnails (Inter Black, SIL Open Font License 1.1).
const BOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-Black.otf");

/// The width of `text` at `size` in `font`.
fn width(font: &fontdue::Font, text: &str, size: f32) -> f32 {
    text.chars()
        .map(|c| font.metrics(c, size).advance_width)
        .sum()
}

/// Makes the thumbnail of what plays (1280×720 PNG) and saves it; returns where.
pub fn make(snap: &Snapshot, vis: &Visual, ov: &Overlay, spanish: bool) -> Result<PathBuf, String> {
    let (w, h) = (1280u32, 720u32);
    let mut cv = SkiaCanvas::new(w, h);
    let mut ov = ov.clone();
    ov.opts.camera = false;
    ov.opts.window = false;
    ov.viewers = None;
    ov.greets.clear();
    ov.poll = None;
    ov.focus = None;
    ov.milestone = None;
    ov.bare = true;
    stage::draw(&mut cv, snap, vis, None, &ov);
    let (wf, hf) = (w as f32, h as f32);
    // the dark band at the left, for the words
    for k in 0..96 {
        let x = k as f32 * 8.0;
        let a = (1.0 - x / 760.0).max(0.0).powf(1.4) * 0.85;
        cv.line(
            &[pos2(x + 4.0, -10.0), pos2(x + 4.0, hf + 10.0)],
            8.5,
            Color32::from_rgba_unmultiplied(8, 12, 16, (a * 255.0) as u8),
        );
    }
    let id = snap.recipe as usize % CARTA.len();
    let [_, main, accent] = CARTA[id]
        .1
        .palette
        .map(|c| Color32::from_rgb(c[0], c[1], c[2]));
    let cream = Color32::from_rgb(246, 236, 214);
    let gold = crate::theme::lerp(accent, Color32::from_rgb(247, 196, 104), 0.5);
    let teal = crate::theme::lerp(main, Color32::from_rgb(120, 220, 205), 0.6);
    let bold = fontdue::Font::from_bytes(BOLD, fontdue::FontSettings::default())
        .map_err(|e| format!("the bold font: {e}"))?;
    // the words: the search words, uppercase, at most two lines
    let lead = crate::promo::lead(id, spanish).to_uppercase();
    let words: Vec<&str> = lead.split_whitespace().collect();
    let lines: Vec<String> = match words.len() {
        0 => vec![],
        1 => vec![words[0].into()],
        2 => vec![words[0].into(), words[1].into()],
        _ => {
            // the last word ("MUSIC", "AMBIENT") is dropped when it makes a third line
            let first = words[0].to_string();
            let rest = words[1..words.len() - 1].join(" ");
            vec![first, rest]
        }
    };
    let size = lines
        .iter()
        .map(|l| (640.0 / width(&bold, l, 1.0)).min(132.0))
        .fold(132.0f32, f32::min);
    let old = cv.swap_font(bold);
    // the badge
    cv.line(
        &[pos2(84.0, 83.0), pos2(214.0, 83.0)],
        56.0,
        Color32::from_rgb(214, 38, 38),
    );
    cv.circle(pos2(88.0, 83.0), 11.0, Color32::WHITE);
    cv.text(
        pos2(108.0, 83.0),
        Align2::LEFT_CENTER,
        if spanish { "EN VIVO" } else { "LIVE" },
        if spanish { 34.0 } else { 46.0 },
        Color32::WHITE,
    );
    let mut y = 160.0;
    for (k, l) in lines.iter().enumerate() {
        let c = if k == 0 { cream } else { gold };
        // a glow: the words again around themselves, faint
        for g in 0..12 {
            let a = g as f32 / 12.0 * std::f32::consts::TAU;
            cv.text(
                pos2(56.0 + a.cos() * 6.0, y + a.sin() * 6.0),
                Align2::LEFT_TOP,
                l,
                size,
                c.gamma_multiply(0.022),
            );
        }
        cv.text(pos2(56.0, y), Align2::LEFT_TOP, l, size, c);
        y += size * 1.0;
    }
    let (h1, h2, sub) = if spanish {
        (
            "Música que",
            "NUNCA se repite",
            "Compuesta en directo, nota a nota",
        )
    } else {
        ("Music that", "NEVER repeats", "Composed live, note by note")
    };
    y += 18.0;
    cv.text(pos2(62.0, y), Align2::LEFT_TOP, h1, 50.0, cream);
    cv.text(pos2(62.0, y + 58.0), Align2::LEFT_TOP, h2, 50.0, teal);
    let bold = cv.swap_font(old);
    drop(bold);
    cv.line(&[pos2(64.0, y + 134.0), pos2(420.0, y + 134.0)], 3.0, gold);
    cv.text(
        pos2(64.0, y + 150.0),
        Align2::LEFT_TOP,
        sub,
        32.0,
        cream.gamma_multiply(0.9),
    );
    let _ = (wf, hf);
    // saved next to the recordings
    let dir = crate::stream::recordings_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = format!(
        "Kontinuum thumbnail {} {}.png",
        CARTA[id].0,
        chrono::Local::now().format("%Y-%m-%d %H-%M")
    );
    let path = dir.join(name);
    image::save_buffer(&path, cv.rgba(), w, h, image::ColorType::Rgba8)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn makes_a_thumbnail() {
        let snap = knt_engine::Snapshot {
            recipe: 1,
            levels: [0.3; knt_engine::LAYERS],
            gains: [1.0; knt_engine::LAYERS],
            chord: knt_engine::Chord::from_notes(&[53, 60, 65, 67], 0),
            ..Default::default()
        };
        let mut vis = crate::stage::Visual::default();
        for k in 0..30 {
            vis.update(&snap, k as f64 * 0.1);
        }
        let p = super::make(&snap, &vis, &crate::scene::Overlay::default(), false).unwrap();
        if let Ok(to) = std::env::var("KNT_THUMB") {
            std::fs::copy(&p, to).unwrap();
        }
        std::fs::remove_file(p).unwrap();
    }
}
