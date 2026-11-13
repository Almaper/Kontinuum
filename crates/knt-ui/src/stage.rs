//! The stage: a 16:9 picture of the music, made to be watched and streamed. Each layer has its
//! own living shape, colored by the recipe of the Menu; captions say what plays and what the
//! director does.
//!
//! - the sky: twinkling stars, a shooting star now and then, rays of light, and an aurora in
//!   the layers' colors;
//! - the drone: mountains in three ridges, and a slow horizon of waves over them, as bright as
//!   its filter is open;
//! - the pad: the harmony wheel, the twelve notes on a slowly turning circle, with the chord's
//!   notes as orbs on it, joined into a constellation;
//! - the texture: a mist of rising particles;
//! - the bells: rings and sparks that open where a bell rings;
//! - the modular: sixteen steps on an arc under the wheel;
//! - the percussion: ripples across the ground, with the pulse's tempo;
//! - the choir: soft veils across the sky.
//!
//! [`draw`] works on any [`Canvas`]: the window's painter, or the live stream's pixmap.

use eframe::egui::{Align2, Color32, Pos2, pos2};
use knt_engine::{CARTA, Snapshot, note_name};

use crate::app::event_text;
use crate::canvas::Canvas;
use crate::theme::{hsv, hue, lerp};

/// What the picture shows, smoothed frame by frame: the snapshot comes some 40 times a second
/// and jumps; the picture glides. One per view (the window's stage, the stream).
#[derive(Debug, Clone, Copy, Default)]
pub struct Visual {
    /// The picture's own clock, seconds.
    pub t: f64,
    pub levels: [f32; knt_engine::LAYERS],
    pub drone_bright: f32,
    /// The color wheel's turn, degrees: the key's root and the chord move it slowly.
    pub hue: f32,
    last: Option<f64>,
}

impl Visual {
    /// Moves toward `s`; `now` is the view's clock (seconds).
    pub fn update(&mut self, s: &Snapshot, now: f64) {
        let dt = self.last.map_or(0.0, |l| (now - l).clamp(0.0, 0.25)) as f32;
        self.last = Some(now);
        self.t += dt as f64;
        let k = 1.0 - (-dt / 0.35).exp();
        for (i, v) in self.levels.iter_mut().enumerate() {
            let target = (s.levels[i] * s.gains[i].max(0.0) * 12.0).sqrt().min(1.0);
            *v += (target - *v) * k;
        }
        let b = (s.drone_cutoff / 2500.0).clamp(0.15, 1.0);
        self.drone_bright += (b - self.drone_bright) * k;
        // the chord's root turns the wheel (a fifth up: 30 degrees), very slowly
        let root = s.chord.bass().unwrap_or(s.root);
        // within about ±25 degrees of the recipe's colors
        let target = (root.rem_euclid(12) as f32 * 7.0 * 7.0).rem_euclid(84.0) * 0.6 - 25.0;
        let mut d = (target - self.hue).rem_euclid(360.0);
        if d > 180.0 {
            d -= 360.0;
        }
        self.hue += d * (1.0 - (-dt / 6.0).exp());
    }
}

/// The color of a layer: around the recipe's main hue, each layer its own, all turned by the
/// chord (`vis.hue`).
pub fn layer_color(recipe: &knt_engine::Recipe, layer: usize, turn: f32) -> Color32 {
    let base = hue(rgb(recipe.palette[1]));
    let (dh, s, v) = [
        (0.0, 0.6, 0.95),   // drone
        (25.0, 0.5, 1.0),   // pad
        (-30.0, 0.4, 0.95), // texture
        (55.0, 0.45, 1.0),  // bells
        (-15.0, 0.7, 0.95), // percussion
        (-60.0, 0.35, 1.0), // choir
        (80.0, 0.55, 1.0),  // modular
    ][layer.min(6)];
    hsv(base + dh + turn, s, v)
}

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

/// Draws the stage over the whole canvas (which should be 16:9). `live`: the streaming badge.
pub fn draw(cv: &mut dyn Canvas, s: &Snapshot, vis: &Visual, live: Option<&str>) {
    let (w, h) = cv.size();
    let u = w / 1920.0;
    let t = vis.t as f32;
    let recipe = CARTA[s.recipe as usize % CARTA.len()].1;
    let [bg, main, accent] = recipe.palette.map(rgb);
    let lv = |i: usize| vis.levels[i];
    let col = |i: usize| layer_color(&recipe, i, vis.hue);
    let tau = std::f32::consts::TAU;
    let cream = Color32::from_rgb(0xef, 0xe3, 0xc8);
    let bright = vis.drone_bright;
    // the backdrop: the sky, the aurora, the rays and the mountains (it moves slowly, so the
    // stream's canvas keeps it a few frames)
    if !cv.backdrop_kept() {
        cv.background(bg, lerp(bg, main, 0.25));
        // the aurora: three soft clouds of the drone's, the pad's and the choir's colors, drifting
        let mut clouds = [(Pos2::ZERO, 0.0f32, Color32::TRANSPARENT); 3];
        for (k, &i) in [0usize, 1, 5].iter().enumerate() {
            let a = 0.04 + 0.12 * lv(i);
            let kf = k as f32;
            let c = pos2(
                w * (0.5 + 0.35 * (t * (0.013 + 0.004 * kf) + kf * 2.1).sin()),
                h * (0.45 + 0.2 * (t * (0.017 + 0.003 * kf) + kf * 1.3).cos()),
            );
            let r = h * (0.45 + 0.1 * (t * 0.05 + kf).sin());
            clouds[k] = (c, r, col(i).gamma_multiply(a * 1.6));
        }
        cv.clouds(&clouds);
        // light rays from above, swaying with the drone's filter
        for k in 0..7 {
            let kf = k as f32;
            let sway = (t * 0.03 + kf * 1.7).sin() * 0.08;
            let top = pos2(w * (0.5 + (kf - 3.0) * 0.05), -h * 0.1);
            let bottom = pos2(w * (0.5 + (kf - 3.0) * 0.16 + sway), h * 0.95);
            let a = (0.012 + 0.03 * bright) * (0.6 + 0.4 * (t * 0.07 + kf).sin());
            cv.line(
                &[top, bottom],
                (60.0 + 50.0 * hash(k + 40)) * u,
                main.gamma_multiply(a),
            );
        }
        // the mountains: three ridges, the farthest the palest, drifting very slowly
        for k in 0..3 {
            let kf = k as f32;
            let speed = 0.004 * (kf + 1.0);
            let pts: Vec<Pos2> = (0..=96)
                .map(|j| {
                    let x = j as f32 / 96.0;
                    let xx = x * (2.0 + kf) + t * speed;
                    let ridge = (xx * 3.1 + kf * 4.0).sin() * 0.5
                        + (xx * 7.3 + kf).sin() * 0.25
                        + (xx * 15.7 + kf * 2.0).sin() * 0.1;
                    pos2(x * w, h * (0.66 + 0.07 * kf - 0.05 * ridge))
                })
                .collect();
            let far = lerp(bg, col(0), 0.22 - 0.07 * kf);
            let near = lerp(bg, Color32::BLACK, 0.25 + 0.15 * kf);
            cv.fill_below(&pts, h, far.gamma_multiply(0.85), near.gamma_multiply(0.95));
            // the ridge's rim catches the light
            cv.line(
                &pts,
                1.0 * u,
                col(0).gamma_multiply((0.12 - 0.03 * kf) * (0.4 + bright)),
            );
        }
        cv.backdrop_done();
    }
    // the stars: twinkling, more of them in the darker recipes
    let night = 1.0 - recipe.brightness;
    for i in 0..(90.0 + 160.0 * night) as u32 {
        let x = (hash(i * 7 + 1) + t * 0.0015).fract();
        let y = hash(i * 7 + 2) * 0.62;
        let tw = (t * (0.4 + 1.6 * hash(i * 7 + 3)) + hash(i * 7 + 4) * 30.0).sin();
        let a = (0.12 + 0.5 * tw * tw) * (0.4 + 0.6 * night);
        let big = hash(i * 7 + 5) > 0.93;
        let c = if hash(i * 7 + 6) < 0.7 { cream } else { accent };
        cv.circle(
            pos2(x * w, y * h),
            if big { 2.2 } else { 1.0 } * u,
            c.gamma_multiply(a),
        );
        if big && tw > 0.8 {
            // a sparkle: a small cross
            let q = pos2(x * w, y * h);
            let l = 7.0 * u * (tw - 0.8) * 5.0;
            let sc = c.gamma_multiply(0.35 * a);
            cv.line(&[pos2(q.x - l, q.y), pos2(q.x + l, q.y)], 1.0 * u, sc);
            cv.line(&[pos2(q.x, q.y - l), pos2(q.x, q.y + l)], 1.0 * u, sc);
        }
    }
    // a shooting star, now and then
    let period = 26.0;
    let n_star = (t / period).floor();
    let ph = t / period - n_star;
    if ph < 0.05 {
        let k = n_star as u32;
        let p0 = pos2(
            w * (0.15 + 0.6 * hash(k * 3 + 900)),
            h * (0.05 + 0.2 * hash(k * 3 + 901)),
        );
        let dir = Pos2::new(0.9, 0.35).to_vec2();
        let head = p0 + dir * (ph / 0.05) * w * 0.25;
        let fade = (1.0 - ph / 0.05) * 0.9;
        for j in 0..6 {
            let tail = head - dir * (j as f32 + 1.0) * 22.0 * u;
            let tail0 = head - dir * j as f32 * 22.0 * u;
            cv.line(
                &[tail0, tail],
                (2.4 - j as f32 * 0.35) * u,
                cream.gamma_multiply(fade * (1.0 - j as f32 / 6.0)),
            );
        }
    }
    // the choir: veils
    let choir = lv(5);
    if choir > 0.01 {
        for k in 0..4 {
            let pts: Vec<Pos2> = (0..=80)
                .map(|j| {
                    let x = w * j as f32 / 80.0;
                    let y = h * (0.18 + 0.06 * k as f32)
                        + (j as f32 * 0.12 + t * (0.15 + 0.05 * k as f32)).sin() * 40.0 * u
                        + (j as f32 * 0.05 - t * 0.07).sin() * 25.0 * u;
                    pos2(x, y)
                })
                .collect();
            cv.line(
                &pts,
                (24.0 + 10.0 * k as f32) * u,
                col(5).gamma_multiply(0.07 * choir),
            );
            cv.line(&pts, 1.2 * u, col(5).gamma_multiply(0.25 * choir));
        }
    }
    // the harmony wheel: the twelve notes on a slowly turning circle; the chord's notes are
    // orbs on it (the lower, the closer to the center), joined into a constellation
    let pad = lv(1);
    let center = pos2(w * 0.5, h * 0.43);
    let big_r = h * 0.27;
    let rot = t * 0.015;
    let angle = |pc: i32| rot + pc as f32 * tau / 12.0 - tau / 4.0;
    let on_wheel = |a: f32, r: f32| pos2(center.x + a.cos() * r, center.y + a.sin() * r);
    let wheel_a = 0.1 + 0.25 * pad.max(lv(0));
    let ring_c = lerp(main, col(1), 0.5);
    cv.ring(center, big_r, 1.2 * u, ring_c.gamma_multiply(wheel_a));
    cv.ring(
        center,
        big_r * 0.62,
        1.0 * u,
        ring_c.gamma_multiply(wheel_a * 0.6),
    );
    // the outer dashed ring turns the other way
    for k in 0..36 {
        let a0 = -rot * 2.0 + k as f32 * tau / 36.0;
        let pts: Vec<Pos2> = (0..=4)
            .map(|j| on_wheel(a0 + j as f32 * tau / 36.0 * 0.5 / 4.0, big_r * 1.18))
            .collect();
        cv.line(&pts, 1.2 * u, ring_c.gamma_multiply(wheel_a * 0.7));
    }
    let notes = s.chord.notes();
    let pcs: Vec<i32> = notes.iter().map(|n| n.rem_euclid(12)).collect();
    for pc in 0..12 {
        let a = angle(pc);
        let lit = pcs.contains(&pc);
        let r0 = big_r * 0.95;
        let r1 = big_r * if lit { 1.1 } else { 1.04 };
        cv.line(
            &[on_wheel(a, r0), on_wheel(a, r1)],
            if lit { 2.5 } else { 1.2 } * u,
            ring_c.gamma_multiply(if lit { 0.4 + 0.5 * pad } else { wheel_a }),
        );
        if pc == s.root.rem_euclid(12) && s.section > 0 {
            cv.circle(
                on_wheel(a, big_r * 1.18),
                4.0 * u,
                accent.gamma_multiply(0.7),
            );
        }
    }
    let orb_at = |k: usize, note: i32| {
        let oct = ((note - 36) as f32 / 36.0).clamp(0.0, 1.0);
        let wob = (t * 0.3 + k as f32 * 1.9).sin() * 6.0 * u;
        on_wheel(angle(note.rem_euclid(12)), big_r * (0.35 + 0.6 * oct) + wob)
    };
    if pad > 0.01 && !notes.is_empty() {
        // the constellation
        let mut pts: Vec<Pos2> = notes
            .iter()
            .enumerate()
            .map(|(k, &n)| orb_at(k, n))
            .collect();
        pts.push(pts[0]);
        cv.line(&pts, 1.5 * u, col(1).gamma_multiply(0.35 * pad));
        for q in &pts {
            cv.line(&[center, *q], 1.0 * u, col(1).gamma_multiply(0.12 * pad));
        }
        cv.circle(
            center,
            (5.0 + 6.0 * pad) * u,
            accent.gamma_multiply(0.5 * pad),
        );
    }
    // the texture: mist, rising slowly
    let mist = lv(2);
    let n = (120.0 + 400.0 * mist) as u32;
    for i in 0..n {
        let x = (hash(i) + t * 0.004 * (0.5 + hash(i + 7)) + (t * 0.2 + i as f32).sin() * 0.003)
            .rem_euclid(1.0);
        let y = (hash(i + 99) - t * 0.006 * (0.3 + hash(i + 13))).rem_euclid(1.0);
        let a = (0.08 + 0.35 * mist * hash(i + 3)) * (y * 1.3).min(1.0);
        let c = if hash(i + 11) < 0.5 { accent } else { col(2) };
        cv.circle(
            pos2(x * w, y * h),
            (1.0 + 2.5 * hash(i + 5)) * u,
            c.gamma_multiply(a),
        );
    }
    // the drone: the horizon, glowing over the mountains
    let drone = lv(0);
    for k in 0..5 {
        let base = h * (0.88 - 0.04 * k as f32);
        let pts: Vec<Pos2> = (0..=120)
            .map(|j| {
                let x = w * j as f32 / 120.0;
                let ph = j as f32 / 120.0 * tau * (1.5 + k as f32 * 0.6);
                let y = base + (ph + t * (0.12 + 0.04 * k as f32)).sin() * h * 0.03 * (0.3 + drone);
                pos2(x, y)
            })
            .collect();
        let c = lerp(main, col(0), 0.6);
        let a = bright * (0.9 - 0.15 * k as f32) * (0.3 + 0.7 * drone);
        if k < 2 {
            cv.line(
                &pts,
                (10.0 - k as f32 * 1.5) * u,
                c.gamma_multiply(a * 0.12),
            );
        }
        cv.line(&pts, (2.5 - k as f32 * 0.3) * u, c.gamma_multiply(a));
    }
    // the pad: the chord's orbs on the wheel, each with a halo and a small moon
    if pad > 0.01 {
        for (k, &note) in notes.iter().enumerate() {
            let q = orb_at(k, note);
            let rad = (16.0 + 26.0 * pad) * u * (1.0 + (t * 0.5 + k as f32 * 1.3).sin() * 0.08);
            for g in 1..4 {
                let gf = g as f32;
                cv.circle(
                    q,
                    rad * (2.6 - gf * 0.45),
                    col(1).gamma_multiply(0.035 + 0.04 * gf * (0.4 + pad)),
                );
            }
            cv.circle(q, rad * 0.55, accent.gamma_multiply(0.5 + 0.5 * pad));
            cv.circle(q, rad * 0.25, cream.gamma_multiply(0.5 * pad));
            let ma = t * (0.6 + 0.15 * k as f32) + k as f32 * 2.0;
            cv.ring(q, rad * 1.5, 0.8 * u, col(1).gamma_multiply(0.18 * pad));
            cv.circle(
                pos2(q.x + ma.cos() * rad * 1.5, q.y + ma.sin() * rad * 1.5),
                2.5 * u,
                cream.gamma_multiply(0.7 * pad),
            );
            cv.text(
                pos2(q.x, q.y + rad * 2.0),
                Align2::CENTER_CENTER,
                note_name(note),
                17.0 * u,
                accent.gamma_multiply(0.55),
            );
        }
    }
    // the bells: rings and sparks where a bell rings
    for &(note, age) in &s.bells {
        if age.is_finite() && age < 8.0 {
            let x = w * (0.1 + 0.8 * hash(note as u32 * 31));
            let y = h * (0.12 + 0.3 * hash(note as u32 * 17 + 3));
            let a = (1.0 - age / 8.0).powi(2);
            for k in 0..3 {
                let rad = (8.0 + age * 45.0 - k as f32 * 18.0).max(0.0) * u;
                cv.ring(pos2(x, y), rad, 2.0 * u, col(3).gamma_multiply(a * 0.8));
            }
            for k in 0..10 {
                let sa = k as f32 * tau / 10.0 + hash(note as u32 + k) * 0.6;
                let d = (10.0 + age * 70.0 * (0.6 + 0.4 * hash(note as u32 * 3 + k))) * u;
                cv.circle(
                    pos2(x + sa.cos() * d, y + sa.sin() * d + age * age * 4.0 * u),
                    1.8 * u,
                    col(3).gamma_multiply(a * 0.9),
                );
            }
            cv.circle(pos2(x, y), (4.0 + 6.0 * a) * u, col(3).gamma_multiply(a));
        }
    }
    // the modular: sixteen steps on an arc under the wheel, the playing one lit and tied to it
    let modular = lv(6);
    if modular > 0.02 {
        let step = ((t * 4.0) as usize) % 16;
        for k in 0..16 {
            let a = tau * (0.12 + 0.26 * k as f32 / 15.0);
            let q = on_wheel(a, big_r * 1.32);
            let on = k == step;
            let al = if on { modular } else { 0.18 * modular };
            if on {
                cv.line(
                    &[q, on_wheel(a, big_r * 1.18)],
                    1.5 * u,
                    col(6).gamma_multiply(0.6 * modular),
                );
                cv.circle(q, 14.0 * u, col(6).gamma_multiply(0.15 * modular));
            }
            cv.circle(
                q,
                (if on { 6.0 } else { 3.0 }) * u,
                col(6).gamma_multiply(al),
            );
        }
    }
    // the percussion: ripples across the ground, with the pulse's tempo
    let perc = lv(4);
    if perc > 0.02 {
        let c = pos2(w * 0.5, h * 0.9);
        let beat = 60.0 / recipe.bpm.max(40.0) * 2.0;
        for k in 0..4 {
            let ph = ((t / beat) + k as f32 / 4.0).fract();
            let rx = (30.0 + ph * 700.0) * u;
            let pts: Vec<Pos2> = (0..=64)
                .map(|j| {
                    let a = j as f32 / 64.0 * tau;
                    pos2(c.x + a.cos() * rx, c.y + a.sin() * rx * 0.18)
                })
                .collect();
            cv.line(
                &pts,
                2.0 * u,
                col(4).gamma_multiply(perc * 0.6 * (1.0 - ph).powi(2)),
            );
        }
        cv.circle(c, (6.0 + 14.0 * perc) * u, col(4).gamma_multiply(perc));
    }
    // the captions
    let dish = CARTA[s.recipe as usize % CARTA.len()].0;
    let (lines, _, width) = crate::theme::wordmark_lines(pos2(60.0 * u, 50.0 * u), 30.0 * u);
    for l in lines {
        cv.line(&l, width, cream.gamma_multiply(0.8));
    }
    cv.text(
        pos2(w - 60.0 * u, 50.0 * u),
        Align2::RIGHT_TOP,
        dish,
        40.0 * u,
        cream.gamma_multiply(0.9),
    );
    if let Some(sc) = s.scale.filter(|_| s.section > 0) {
        cv.text(
            pos2(w - 60.0 * u, 100.0 * u),
            Align2::RIGHT_TOP,
            &format!("{} {}", note_name(s.root), sc.name()),
            24.0 * u,
            cream.gamma_multiply(0.6),
        );
    }
    // the timbres that sound now
    let mut y = 140.0 * u;
    for (i, l) in knt_engine::Layer::ALL.iter().enumerate() {
        if s.active[i] && lv(i) > 0.02 && !s.patches[i].is_empty() {
            cv.text(
                pos2(w - 60.0 * u, y),
                Align2::RIGHT_TOP,
                &format!("{} · {}", l.name(), s.patches[i]),
                17.0 * u,
                cream.gamma_multiply(0.45),
            );
            cv.circle(
                pos2(w - 48.0 * u, y + 10.0 * u),
                4.0 * u,
                col(i).gamma_multiply(0.4 + 0.6 * lv(i)),
            );
            y += 24.0 * u;
        }
    }
    // what the director just did, fading
    if let Some((e, age)) = s.events.iter().flatten().next() {
        let a = (1.0 - age / 12.0).clamp(0.0, 1.0);
        if a > 0.0 {
            cv.text(
                pos2(60.0 * u, h - 60.0 * u),
                Align2::LEFT_BOTTOM,
                &event_text(e),
                30.0 * u,
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
    cv.text(
        pos2(w - 60.0 * u, h - 60.0 * u),
        Align2::RIGHT_BOTTOM,
        &format!(
            "{who}  ·  section {}  ·  {:02}:{:02}:{:02}",
            s.section,
            tm / 3600,
            tm / 60 % 60,
            tm % 60
        ),
        20.0 * u,
        cream.gamma_multiply(0.55),
    );
    if let Some(l) = live {
        let c = pos2(68.0 * u, 110.0 * u);
        cv.circle(c, 8.0 * u, Color32::from_rgb(0xe0, 0x3a, 0x2a));
        cv.text(
            pos2(c.x + 18.0 * u, c.y),
            Align2::LEFT_CENTER,
            l,
            20.0 * u,
            cream.gamma_multiply(0.8),
        );
    }
}
