//! The stage: a full-window 16:9 picture of the music, made to be watched (and streamed). Each
//! layer has its own living shape, colored by the dish of the Carta; captions say what plays
//! and what the director does.
//!
//! - the drone: a slow horizon of waves at the bottom, as bright as its filter is open;
//! - the pad: one glowing orb per note of the chord, high or low with the pitch;
//! - the texture: a mist of drifting particles;
//! - the bells: rings that open where a bell rings;
//! - the percussion: a pulse that beats from the center;
//! - the choir: soft veils across the sky.

use eframe::egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use knt_engine::{CARTA, Recipe, Snapshot, note_name};

use crate::app::event_text;

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn hash(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Draws the stage into `full` (letterboxed to 16:9).
pub fn draw(p: &Painter, full: Rect, s: &Snapshot, now: f64, recipe: Recipe) {
    p.rect_filled(full, 0.0, Color32::BLACK);
    let w = full.width().min(full.height() * 16.0 / 9.0);
    let h = w * 9.0 / 16.0;
    let r = Rect::from_center_size(full.center(), vec2(w, h));
    let u = w / 1920.0;
    let t = now as f32;
    let [bg, main, accent] = recipe.palette.map(rgb);
    // the sky: a vertical gradient from the dish's background
    let top = bg;
    let bottom = crate::theme::lerp(bg, main, 0.25);
    let mesh = {
        let mut m = eframe::egui::Mesh::default();
        m.colored_vertex(r.left_top(), top);
        m.colored_vertex(r.right_top(), top);
        m.colored_vertex(r.right_bottom(), bottom);
        m.colored_vertex(r.left_bottom(), bottom);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        m
    };
    p.add(Shape::mesh(mesh));
    let lv = |i: usize| (s.levels[i] * s.gains[i].max(0.0) * 12.0).sqrt().min(1.0);
    // the choir: veils
    let choir = lv(5);
    if choir > 0.01 {
        for k in 0..3 {
            let pts: Vec<Pos2> = (0..=80)
                .map(|j| {
                    let x = r.left() + r.width() * j as f32 / 80.0;
                    let y = r.top()
                        + h * (0.22 + 0.07 * k as f32)
                        + (j as f32 * 0.12 + t * (0.15 + 0.05 * k as f32)).sin() * 40.0 * u;
                    pos2(x, y)
                })
                .collect();
            p.add(Shape::line(
                pts,
                Stroke::new(30.0 * u, accent.gamma_multiply(0.05 * choir)),
            ));
        }
    }
    // the texture: mist
    let mist = lv(2);
    let n = (120.0 + 400.0 * mist) as u32;
    for i in 0..n {
        let x = (hash(i) + t * 0.004 * (0.5 + hash(i + 7))).fract();
        let y = (hash(i + 99) + (t * 0.3 + i as f32).sin() * 0.01).fract();
        let a = 0.08 + 0.35 * mist * hash(i + 3);
        p.circle_filled(
            pos2(r.left() + x * w, r.top() + y * h),
            (1.0 + 2.5 * hash(i + 5)) * u,
            accent.gamma_multiply(a),
        );
    }
    // the drone: the horizon
    let drone = lv(0);
    let bright = (s.drone_cutoff / 2500.0).clamp(0.15, 1.0);
    for k in 0..5 {
        let base = r.bottom() - h * (0.12 + 0.04 * k as f32);
        let pts: Vec<Pos2> = (0..=120)
            .map(|j| {
                let x = r.left() + w * j as f32 / 120.0;
                let ph = j as f32 / 120.0 * std::f32::consts::TAU * (1.5 + k as f32 * 0.6);
                let y = base + (ph + t * (0.12 + 0.04 * k as f32)).sin() * h * 0.03 * (0.3 + drone);
                pos2(x, y)
            })
            .collect();
        p.add(Shape::line(
            pts,
            Stroke::new(
                (2.5 - k as f32 * 0.3) * u,
                main.gamma_multiply(bright * (0.9 - 0.15 * k as f32)),
            ),
        ));
    }
    // the pad: orbs
    let pad = lv(1);
    let notes = s.chord.notes();
    for (k, &note) in notes.iter().enumerate() {
        let x = r.left() + w * (0.2 + 0.6 * (k as f32 + 0.5) / notes.len().max(1) as f32);
        let y = r.top()
            + h * (0.75 - ((note - 36) as f32 / 40.0).clamp(0.0, 1.0) * 0.45)
            + (t * 0.3 + k as f32).sin() * 10.0 * u;
        let rad = (28.0 + 40.0 * pad) * u * (1.0 + (t * 0.5 + k as f32 * 1.3).sin() * 0.08);
        for g in 0..4 {
            p.circle_filled(
                pos2(x, y),
                rad * (2.2 - g as f32 * 0.35),
                main.gamma_multiply(0.05 + 0.04 * g as f32 * (0.4 + pad)),
            );
        }
        p.circle_filled(
            pos2(x, y),
            rad * 0.55,
            accent.gamma_multiply(0.5 + 0.5 * pad),
        );
        p.text(
            pos2(x, y + rad * 1.6),
            Align2::CENTER_CENTER,
            note_name(note),
            FontId::proportional(18.0 * u),
            accent.gamma_multiply(0.55),
        );
    }
    // the bells: rings
    for &(note, age) in &s.bells {
        if age.is_finite() && age < 8.0 {
            let x = r.left() + w * (0.1 + 0.8 * hash(note as u32 * 31));
            let y = r.top() + h * (0.15 + 0.3 * hash(note as u32 * 17 + 3));
            let a = (1.0 - age / 8.0).powi(2);
            for k in 0..3 {
                let rad = (8.0 + age * 45.0 - k as f32 * 18.0).max(0.0) * u;
                p.circle_stroke(
                    pos2(x, y),
                    rad,
                    Stroke::new(2.0 * u, accent.gamma_multiply(a * 0.8)),
                );
            }
            p.circle_filled(pos2(x, y), 4.0 * u, accent.gamma_multiply(a));
        }
    }
    // the modular: a row of steps that light as it plays
    let modular = lv(6);
    if modular > 0.02 {
        let step = ((t * 4.0) as usize) % 16;
        let y = r.top() + h * 0.58;
        for k in 0..16 {
            let x = r.left() + w * (0.25 + 0.5 * k as f32 / 15.0);
            let on = k == step;
            let a = if on { modular } else { 0.15 * modular };
            p.circle_filled(
                pos2(x, y),
                (if on { 7.0 } else { 3.0 }) * u,
                accent.gamma_multiply(a),
            );
        }
    }
    // the percussion: a pulse from the center
    let perc = lv(4);
    if perc > 0.02 {
        let c = pos2(r.center().x, r.bottom() - h * 0.12);
        p.circle_stroke(
            c,
            (40.0 + 200.0 * perc) * u,
            Stroke::new(3.0 * u, main.gamma_multiply(0.5 * perc)),
        );
        p.circle_filled(c, (6.0 + 14.0 * perc) * u, accent.gamma_multiply(perc));
    }
    // the captions
    let cream = Color32::from_rgb(0xef, 0xe3, 0xc8);
    let dish = CARTA[s.recipe as usize % CARTA.len()].0;
    crate::theme::wordmark(
        p,
        pos2(r.left() + 60.0 * u, r.top() + 50.0 * u),
        30.0 * u,
        cream.gamma_multiply(0.8),
    );
    p.text(
        pos2(r.right() - 60.0 * u, r.top() + 50.0 * u),
        Align2::RIGHT_TOP,
        dish,
        FontId::proportional(40.0 * u),
        cream.gamma_multiply(0.9),
    );
    let key = match s.scale {
        Some(sc) if s.section > 0 => format!("{} {}", note_name(s.root), sc.name()),
        _ => String::new(),
    };
    p.text(
        pos2(r.right() - 60.0 * u, r.top() + 100.0 * u),
        Align2::RIGHT_TOP,
        key,
        FontId::proportional(24.0 * u),
        cream.gamma_multiply(0.6),
    );
    // what the director just did, fading
    if let Some((e, age)) = s.events.iter().flatten().next() {
        let a = (1.0 - age / 12.0).clamp(0.0, 1.0);
        if a > 0.0 {
            p.text(
                pos2(r.left() + 60.0 * u, r.bottom() - 60.0 * u),
                Align2::LEFT_BOTTOM,
                event_text(e),
                FontId::proportional(30.0 * u),
                cream.gamma_multiply(0.85 * a),
            );
        }
    }
    let tm = s.time as u64;
    let who = if s.user_active {
        "playing live"
    } else {
        "the director composes"
    };
    p.text(
        pos2(r.right() - 60.0 * u, r.bottom() - 60.0 * u),
        Align2::RIGHT_BOTTOM,
        format!(
            "{who}  ·  section {}  ·  {:02}:{:02}:{:02}",
            s.section,
            tm / 3600,
            tm / 60 % 60,
            tm % 60
        ),
        FontId::proportional(20.0 * u),
        cream.gamma_multiply(0.55),
    );
}
