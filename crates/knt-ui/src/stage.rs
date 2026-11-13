//! The stage: a picture of the music, made to be watched and streamed (16:9, or 9:16 for
//! Shorts). Each layer has its own living shape, colored by the recipe of the Menu; a landscape
//! around them follows the recipe, the time of day and the season; captions say what plays and
//! what the director does.
//!
//! - the sky: the local time's colors (dawn, day, dusk, night) with the sun or the moon, stars,
//!   the seed's constellation, a shooting star now and then, rays of light and an aurora;
//! - the landscape: mountains in three ridges over a lake that reflects the light, and each
//!   recipe's scenery (fog, a forest, a planet, arches of light, a far city);
//! - the weather: rain in the rainy recipes, else the season's (snow, leaves, petals,
//!   fireflies);
//! - the drone: a slow shimmer of waves on the lake, as bright as its filter is open;
//! - the pad: the harmony wheel, the twelve notes on a slowly turning circle, with the chord's
//!   notes as orbs on it, joined into a constellation, and a ring that moves with the layers;
//! - the texture: a mist of rising particles, and fireflies;
//! - the bells: rings, sparks and falling gold where a bell rings;
//! - the modular: sixteen steps on an arc under the wheel;
//! - the percussion: ripples across the lake, with the pulse's tempo;
//! - the choir: soft veils across the sky;
//! - a new section: a wave of light; the notes played on a keyboard: bursts on the wheel;
//! - the audience (from the chat): stars for the people listening, greetings, polls,
//!   milestones; and the focus timer;
//! - optionally the window (rain on the glass, a cup, a lamp) and Lumo, the lantern spirit.
//!
//! [`draw`] works on any [`Canvas`]: the window's painter, or the live stream's pixmap.

use eframe::egui::{Align2, Color32, Pos2, Vec2, pos2, vec2};
use knt_engine::{CARTA, Snapshot, note_name};

use crate::app::event_text;
use crate::canvas::Canvas;
use crate::scene::{Overlay, Season};
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
    /// When (`t`) the section and the key last changed.
    pub section_at: f64,
    last_section: u32,
    pub key_at: f64,
    last_key: (i32, Option<knt_engine::Scale>),
    /// The light, 0..1: it dims with the fade and the end of the sleep timer.
    pub light: f32,
}

impl Visual {
    /// Moves toward `s`; `now` is the view's clock (seconds).
    pub fn update(&mut self, s: &Snapshot, now: f64) {
        let first = self.last.is_none();
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
        if s.section != self.last_section {
            if self.last_section != 0 {
                self.section_at = self.t;
            } else {
                self.section_at = -100.0;
            }
            self.last_section = s.section;
        }
        let key = (s.root, s.scale);
        if key != self.last_key {
            self.key_at = if self.last_key.1.is_some() {
                self.t
            } else {
                -100.0
            };
            self.last_key = key;
        }
        // the sleep timer's last two minutes dim the picture with the music
        let sleepy = s.sleep_left.map_or(1.0, |l| (l / 120.0).clamp(0.0, 1.0));
        let target = (0.25 + 0.75 * s.fade.clamp(0.0, 1.0)) * (0.15 + 0.85 * sleepy);
        let target = if s.playing { target } else { 1.0 };
        if first {
            self.light = target;
        }
        self.light += (target - self.light) * (1.0 - (-dt / 1.5).exp());
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

/// The sky's tint at an hour of the day: (color, how much of it).
fn sky_tint(hour: f32) -> (Color32, f32) {
    // (hour, color, amount), around the clock
    const KEYS: [(f32, [u8; 3], f32); 8] = [
        (0.0, [18, 26, 64], 0.3),
        (5.0, [24, 30, 70], 0.28),
        (6.5, [235, 130, 125], 0.3),
        (8.5, [150, 190, 225], 0.18),
        (16.5, [150, 190, 225], 0.18),
        (19.0, [245, 135, 70], 0.32),
        (21.0, [40, 36, 90], 0.28),
        (24.0, [18, 26, 64], 0.3),
    ];
    let h = hour.rem_euclid(24.0);
    let k = KEYS
        .windows(2)
        .find(|w| h >= w[0].0 && h <= w[1].0)
        .unwrap_or(&KEYS[..2]);
    let f = ((h - k[0].0) / (k[1].0 - k[0].0)).clamp(0.0, 1.0);
    let c = |x: [u8; 3]| Color32::from_rgb(x[0], x[1], x[2]);
    (
        lerp(c(k[0].1), c(k[1].1), f),
        k[0].2 + (k[1].2 - k[0].2) * f,
    )
}

/// How dark it is at an hour: 1 at night, 0 by day.
fn night_of(hour: f32) -> f32 {
    let h = hour.rem_euclid(24.0);
    let day = ((h - 6.0) / 1.5).clamp(0.0, 1.0) * ((20.5 - h) / 1.5).clamp(0.0, 1.0);
    1.0 - day
}

/// A ridge of the mountains: its height (a fraction of the picture) at `x` (0..1).
fn ridge(k: usize, x: f32, t: f32, top: f32, step: f32) -> f32 {
    let kf = k as f32;
    let xx = x * (2.0 + kf) + t * 0.004 * (kf + 1.0);
    let r = (xx * 3.1 + kf * 4.0).sin() * 0.5
        + (xx * 7.3 + kf).sin() * 0.25
        + (xx * 15.7 + kf * 2.0).sin() * 0.1;
    top + step * kf - 0.05 * r
}

/// Draws the stage over the whole canvas. `live`: the streaming badge; `ov`: the options and
/// the audience.
pub fn draw(cv: &mut dyn Canvas, s: &Snapshot, vis: &Visual, live: Option<&str>, ov: &Overlay) {
    let (w, h) = cv.size();
    let portrait = h > w;
    let u = w.min(h) / 1080.0;
    let t = vis.t as f32;
    let tau = std::f32::consts::TAU;
    let o = ov.opts;
    let id = s.recipe as usize % CARTA.len();
    let recipe = CARTA[id].1;
    let [bg, main, accent] = recipe.palette.map(rgb);
    let lv = |i: usize| vis.levels[i];
    let col = |i: usize| layer_color(&recipe, i, vis.hue);
    let cream = Color32::from_rgb(0xef, 0xe3, 0xc8);
    let bright = vis.drone_bright;
    let (hour, month) = if o.day_night || o.seasons {
        crate::scene::local_time()
    } else {
        (21.0, 1)
    };
    let hour = if o.day_night { hour } else { 21.0 };
    let season = o.seasons.then(|| Season::of_month(month));
    let night = if o.day_night {
        night_of(hour)
    } else {
        1.0 - recipe.brightness
    };
    // where things are
    let horizon = if o.lake {
        h * if portrait { 0.74 } else { 0.8 }
    } else {
        h
    };
    let ridge_top = if portrait { 0.56 } else { 0.62 };
    let ridge_step = 0.06;
    let center = pos2(w * 0.5, h * if portrait { 0.36 } else { 0.42 });
    let big_r = (h * 0.27).min(w * 0.4);
    let rainy = matches!(id, 3 | 4);

    // the camera: a slow drift and breath
    let (zoom, pan) = if o.camera {
        (
            1.03 + 0.02 * (t * 0.011).sin(),
            vec2((t * 0.007).sin() * 0.012 * w, (t * 0.009).cos() * 0.008 * h),
        )
    } else {
        (1.0, Vec2::ZERO)
    };

    // the backdrop: the sky, the sun or the moon, the aurora, the rays, the scenery and the
    // mountains (it moves slowly, so the stream's canvas keeps it a few frames)
    if !cv.backdrop_kept() {
        cv.set_view(1.0, Vec2::ZERO);
        let (tint, amt) = if o.day_night {
            sky_tint(hour)
        } else {
            (bg, 0.0)
        };
        let top = lerp(bg, tint, amt);
        cv.background(top, lerp(lerp(bg, main, 0.25), tint, amt * 0.5));
        // the aurora: three soft clouds of the drone's, the pad's and the choir's colors
        let mut clouds = [(Pos2::ZERO, 0.0f32, Color32::TRANSPARENT); 3];
        for (k, &i) in [0usize, 1, 5].iter().enumerate() {
            let a = 0.04 + 0.12 * lv(i);
            let kf = k as f32;
            let c = pos2(
                w * (0.5 + 0.35 * (t * (0.013 + 0.004 * kf) + kf * 2.1).sin()),
                h * (0.45 + 0.2 * (t * (0.017 + 0.003 * kf) + kf * 1.3).cos()),
            );
            let r = h.max(w) * 0.25 * (1.0 + 0.2 * (t * 0.05 + kf).sin());
            clouds[k] = (c, r, col(i).gamma_multiply(a * 1.6));
        }
        cv.clouds(&clouds);
        cv.set_view(zoom, pan);
        // the sun by day, the moon by night, on an arc over the hills
        if o.day_night {
            let (f, sun) = if (6.5..19.5).contains(&hour) {
                ((hour - 6.5) / 13.0, true)
            } else {
                ((hour - 19.5).rem_euclid(24.0) / 11.0, false)
            };
            let q = pos2(
                w * (0.08 + 0.84 * f),
                horizon - (std::f32::consts::PI * f).sin() * (horizon - h * 0.12),
            );
            if sun {
                let warm = lerp(Color32::from_rgb(255, 214, 150), accent, 0.3);
                for g in 0..4 {
                    cv.circle(
                        q,
                        (90.0 - 18.0 * g as f32) * u,
                        warm.gamma_multiply(0.05 + 0.03 * g as f32),
                    );
                }
                cv.circle(q, 28.0 * u, warm.gamma_multiply(0.9));
            } else {
                for g in 0..3 {
                    cv.circle(
                        q,
                        (60.0 - 15.0 * g as f32) * u,
                        cream.gamma_multiply(0.03 + 0.02 * g as f32),
                    );
                }
                cv.circle(q, 22.0 * u, cream.gamma_multiply(0.85));
                // a crescent: the night's color over one side
                cv.circle(q + vec2(9.0, -5.0) * u, 19.0 * u, top.gamma_multiply(0.92));
            }
        }
        // light rays from above, swaying with the drone's filter
        for k in 0..7 {
            let kf = k as f32;
            let sway = (t * 0.03 + kf * 1.7).sin() * 0.08;
            let a0 = pos2(w * (0.5 + (kf - 3.0) * 0.05), -h * 0.1);
            let a1 = pos2(w * (0.5 + (kf - 3.0) * 0.16 + sway), horizon);
            let a =
                (0.012 + 0.03 * bright) * (0.6 + 0.4 * (t * 0.07 + kf).sin()) * (1.2 - night * 0.5);
            cv.line(
                &[a0, a1],
                (60.0 + 50.0 * hash(k + 40)) * u,
                main.gamma_multiply(a),
            );
        }
        // the far scenery
        if o.scenery {
            match id {
                // Cathedral: arches of light
                2 => {
                    for k in 0..5 {
                        let x = w * (0.18 + 0.16 * k as f32);
                        let half = w * 0.05;
                        let base = h * (ridge_top + 0.02);
                        let top = h * (0.12 + 0.05 * (k as f32 - 2.0).abs());
                        let pts: Vec<Pos2> = (0..=24)
                            .map(|j| {
                                let f = j as f32 / 24.0;
                                // up the left side, a pointed arch, down the right
                                let (x0, y0) = if f < 0.35 {
                                    (x - half, base - (base - top - half) * f / 0.35)
                                } else if f < 0.65 {
                                    let a = (f - 0.35) / 0.3;
                                    let th = std::f32::consts::PI * a;
                                    (x - half * th.cos(), top + half * (1.0 - th.sin()) * 0.9)
                                } else {
                                    (
                                        x + half,
                                        top + half + (base - top - half) * (f - 0.65) / 0.35,
                                    )
                                };
                                pos2(x0, y0)
                            })
                            .collect();
                        cv.line(&pts, 26.0 * u, col(5).gamma_multiply(0.05));
                        cv.line(&pts, 1.5 * u, col(5).gamma_multiply(0.22));
                    }
                }
                // Orbit: a ringed planet and two moons
                5 => {
                    let q = pos2(w * if portrait { 0.75 } else { 0.8 }, h * 0.22);
                    let r = h.min(w) * 0.09;
                    let body = lerp(bg, col(1), 0.45);
                    let ring_pts = |front: bool| -> Vec<Pos2> {
                        (0..=40)
                            .map(|j| {
                                let a = if front { 0.0 } else { std::f32::consts::PI }
                                    + std::f32::consts::PI * j as f32 / 40.0;
                                let (x, y) = (a.cos() * r * 1.9, a.sin() * r * 0.45);
                                let tilt = -0.35f32;
                                pos2(
                                    q.x + x * tilt.cos() - y * tilt.sin(),
                                    q.y + x * tilt.sin() + y * tilt.cos(),
                                )
                            })
                            .collect()
                    };
                    cv.line(&ring_pts(false), 3.0 * u, col(3).gamma_multiply(0.45));
                    cv.circle(q, r * 1.15, body.gamma_multiply(0.15));
                    cv.circle(q, r, body);
                    cv.circle(q + vec2(r * 0.15, r * 0.12), r * 0.8, lerp(body, bg, 0.3));
                    cv.line(&ring_pts(true), 3.0 * u, col(3).gamma_multiply(0.7));
                    for m in 0..2 {
                        let a = t * (0.05 + 0.03 * m as f32) + m as f32 * 2.5;
                        let d = r * (2.6 + 0.8 * m as f32);
                        cv.circle(
                            pos2(q.x + a.cos() * d, q.y + a.sin() * d * 0.4),
                            (5.0 - 1.5 * m as f32) * u,
                            cream.gamma_multiply(0.6),
                        );
                    }
                }
                _ => {}
            }
        }
        // the mountains: three ridges, the farthest the palest, drifting very slowly
        for k in 0..3 {
            let kf = k as f32;
            let mut pts: Vec<Pos2> = (0..=96)
                .map(|j| {
                    let x = j as f32 / 96.0;
                    pos2(x * w, h * ridge(k, x, t, ridge_top, ridge_step))
                })
                .collect();
            // Wet Forest: pines on the nearest ridge
            if o.scenery && id == 4 && k == 2 {
                pts = (0..=240)
                    .map(|j| {
                        let x = j as f32 / 240.0;
                        let tree = (j / 4) as u32;
                        let within = (j % 4) as f32;
                        let tall = 0.03 + 0.05 * hash(tree * 13 + 7);
                        let spike = if within == 2.0 { tall } else { tall * 0.15 };
                        pos2(x * w, h * (ridge(k, x, t, ridge_top, ridge_step) - spike))
                    })
                    .collect();
            }
            let far = lerp(bg, col(0), 0.22 - 0.07 * kf);
            let near = lerp(bg, Color32::BLACK, 0.25 + 0.15 * kf);
            let far = lerp(far, Color32::BLACK, night * 0.12);
            cv.fill_below(
                &pts,
                horizon,
                far.gamma_multiply(0.85),
                near.gamma_multiply(0.95),
            );
            cv.line(
                &pts,
                1.0 * u,
                col(0).gamma_multiply((0.12 - 0.03 * kf) * (0.4 + bright)),
            );
            // Early Mist: fog between the ridges
            if o.scenery && id == 0 {
                let y = h * (ridge_top + ridge_step * kf + 0.03);
                cv.line(
                    &[pos2(-50.0, y), pos2(w + 50.0, y)],
                    40.0 * u,
                    cream.gamma_multiply(0.06),
                );
            }
        }
        // the lake
        if o.lake {
            let water = lerp(lerp(bg, main, 0.12), Color32::BLACK, 0.2 + 0.2 * night);
            cv.fill_below(
                &[pos2(-w, horizon), pos2(2.0 * w, horizon)],
                h * 1.2,
                water,
                lerp(bg, Color32::BLACK, 0.5),
            );
            cv.line(
                &[pos2(-w, horizon), pos2(2.0 * w, horizon)],
                1.5 * u,
                main.gamma_multiply(0.25),
            );
            // Deep Focus: a quiet grid on the water
            if o.scenery && id == 1 {
                let vp = pos2(w * 0.5, horizon);
                for k in -12..=12 {
                    let x = w * 0.5 + k as f32 * w * 0.12;
                    cv.line(
                        &[vp, pos2(x, h * 1.1)],
                        1.0 * u,
                        accent.gamma_multiply(0.07),
                    );
                }
                for k in 1..8 {
                    let y = horizon + (h - horizon) * (k as f32 / 8.0).powi(2) * 1.1;
                    cv.line(
                        &[pos2(0.0, y), pos2(w, y)],
                        1.0 * u,
                        accent.gamma_multiply(0.06),
                    );
                }
            }
        }
        cv.backdrop_done();
    }
    cv.set_view(zoom, pan);

    // the stars: twinkling, more of them by night
    for i in 0..(70.0 + 180.0 * night) as u32 {
        let x = (hash(i * 7 + 1) + t * 0.0015).fract();
        let y = hash(i * 7 + 2) * (ridge_top - 0.02);
        let tw = (t * (0.4 + 1.6 * hash(i * 7 + 3)) + hash(i * 7 + 4) * 30.0).sin();
        let a = (0.12 + 0.5 * tw * tw) * (0.15 + 0.85 * night);
        let big = hash(i * 7 + 5) > 0.93;
        let c = if hash(i * 7 + 6) < 0.7 { cream } else { accent };
        let q = pos2(x * w, y * h);
        cv.circle(q, if big { 2.2 } else { 1.0 } * u, c.gamma_multiply(a));
        if big && tw > 0.8 {
            let l = 7.0 * u * (tw - 0.8) * 5.0;
            let sc = c.gamma_multiply(0.35 * a);
            cv.line(&[pos2(q.x - l, q.y), pos2(q.x + l, q.y)], 1.0 * u, sc);
            cv.line(&[pos2(q.x, q.y - l), pos2(q.x, q.y + l)], 1.0 * u, sc);
        }
    }
    // the seed's constellation
    if o.constellation {
        let seed = s.seed as u32;
        let pts: Vec<Pos2> = (0..7)
            .map(|k| {
                pos2(
                    w * (0.06 + 0.24 * hash(seed.wrapping_mul(31) + k * 2)),
                    h * (0.13 + 0.2 * hash(seed.wrapping_mul(17) + k * 2 + 1)),
                )
            })
            .collect();
        let a = 0.25 + 0.35 * night;
        cv.line(&pts, 1.0 * u, accent.gamma_multiply(0.18 * a));
        for (k, q) in pts.iter().enumerate() {
            let tw = 0.7 + 0.3 * (t * 0.8 + k as f32).sin();
            cv.circle(*q, 5.0 * u, accent.gamma_multiply(0.12 * a * tw));
            cv.circle(*q, 2.0 * u, cream.gamma_multiply(0.8 * a * tw));
        }
        let low = pts.iter().map(|q| q.y).fold(0.0, f32::max);
        let mid = pts.iter().map(|q| q.x).sum::<f32>() / pts.len() as f32;
        cv.text(
            pos2(mid, low + 20.0 * u),
            Align2::CENTER_CENTER,
            &format!("seed {:04}", s.seed),
            14.0 * u,
            cream.gamma_multiply(0.3 * a),
        );
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
        let dir = vec2(0.9, 0.35);
        let head = p0 + dir * (ph / 0.05) * w * 0.25;
        let fade = (1.0 - ph / 0.05) * 0.9;
        for j in 0..6 {
            let a0 = head - dir * j as f32 * 22.0 * u;
            let a1 = head - dir * (j as f32 + 1.0) * 22.0 * u;
            cv.line(
                &[a0, a1],
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
    // Dusk: the lights of a far city on the farthest ridge
    if o.scenery && id == 6 {
        for i in 0..70u32 {
            let x = 0.25 + 0.5 * hash(i * 5 + 300);
            let y = h * (ridge(0, x, t, ridge_top, ridge_step) + 0.004 + 0.02 * hash(i * 5 + 301));
            let blink = 0.6 + 0.4 * (t * (0.3 + hash(i * 5 + 302)) + i as f32).sin();
            let c = if hash(i * 5 + 303) < 0.8 {
                Color32::from_rgb(255, 196, 120)
            } else {
                cream
            };
            cv.circle(
                pos2(x * w, y),
                1.3 * u,
                c.gamma_multiply(0.7 * blink * (0.4 + 0.6 * night)),
            );
        }
    }

    // the harmony wheel: the twelve notes on a slowly turning circle; the chord's notes are
    // orbs on it (the lower, the closer to the center), joined into a constellation
    let pad = lv(1);
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
    for k in 0..36 {
        let a0 = -rot * 2.0 + k as f32 * tau / 36.0;
        let pts: Vec<Pos2> = (0..=4)
            .map(|j| on_wheel(a0 + j as f32 * tau / 36.0 * 0.5 / 4.0, big_r * 1.18))
            .collect();
        cv.line(&pts, 1.2 * u, ring_c.gamma_multiply(wheel_a * 0.7));
    }
    // the layers' ring: it swells where each layer sounds
    for k in 0..72 {
        let a = rot * 0.5 + k as f32 * tau / 72.0;
        let mut amp = 0.0;
        for i in 0..knt_engine::LAYERS {
            let center_a = i as f32 * tau / knt_engine::LAYERS as f32;
            let d = (a - center_a)
                .rem_euclid(tau)
                .min((center_a - a).rem_euclid(tau));
            amp += lv(i) * (-d * d * 6.0).exp();
        }
        let wob = 0.6 + 0.4 * (t * 2.3 + k as f32 * 0.9).sin() * (t * 0.7 + k as f32 * 0.31).cos();
        let len = big_r * 0.16 * (amp * wob).min(1.2);
        if len > 0.5 {
            cv.line(
                &[on_wheel(a, big_r * 0.66), on_wheel(a, big_r * 0.66 + len)],
                2.0 * u,
                ring_c.gamma_multiply(0.15 + 0.35 * amp.min(1.0)),
            );
        }
    }
    let notes = s.chord.notes();
    let pcs: Vec<i32> = notes.iter().map(|n| n.rem_euclid(12)).collect();
    for pc in 0..12 {
        let a = angle(pc);
        let lit = pcs.contains(&pc);
        let r1 = big_r * if lit { 1.1 } else { 1.04 };
        cv.line(
            &[on_wheel(a, big_r * 0.95), on_wheel(a, r1)],
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
    // a new section: a wave of light from the wheel; a new key: a second, warmer one
    for (at, c) in [(vis.section_at, cream), (vis.key_at, accent)] {
        let age = (vis.t - at) as f32;
        if (0.0..4.0).contains(&age) {
            let a = (1.0 - age / 4.0).powi(2);
            cv.ring(
                center,
                big_r * (1.0 + age * 0.9),
                (3.0 + 6.0 * a) * u,
                c.gamma_multiply(0.45 * a),
            );
            cv.ring(
                center,
                big_r * (0.8 + age * 0.6),
                1.5 * u,
                c.gamma_multiply(0.3 * a),
            );
        }
    }

    // the texture: mist, rising slowly
    let mist = lv(2);
    let n = (100.0 + 360.0 * mist) as u32;
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
    // fireflies: they gather toward the wheel as the texture grows
    let summer = season == Some(Season::Summer);
    let flies = 18 + if summer { 22 } else { 0 };
    for i in 0..flies {
        let fi = i as f32;
        let home = pos2(
            w * (0.1 + 0.8 * hash(i * 3 + 500)),
            h * (ridge_top - 0.1 + 0.25 * hash(i * 3 + 501)),
        );
        let wander = vec2(
            (t * (0.11 + 0.05 * hash(i + 502)) + fi).sin() * 60.0 * u,
            (t * (0.13 + 0.04 * hash(i + 503)) + fi * 1.7).cos() * 30.0 * u,
        );
        let gather = 0.35 * mist;
        let q = home + (center - home) * gather + wander;
        let glow = (0.5 + 0.5 * (t * (0.9 + hash(i + 504)) + fi * 2.0).sin()).powi(3);
        let c = lerp(Color32::from_rgb(230, 240, 130), accent, 0.4);
        cv.circle(q, 7.0 * u, c.gamma_multiply(0.1 * glow));
        cv.circle(q, 1.8 * u, c.gamma_multiply(0.9 * glow));
    }

    // the lake: the drone's shimmer, and the reflections of the light above
    let drone = lv(0);
    let wave_top = if o.lake {
        horizon + (h - horizon) * 0.1
    } else {
        h * 0.72
    };
    let wave_bottom = if o.lake { h * 0.96 } else { h * 0.88 };
    for k in 0..5 {
        let base = wave_bottom - (wave_bottom - wave_top) * k as f32 / 4.0;
        let pts: Vec<Pos2> = (0..=120)
            .map(|j| {
                let x = w * j as f32 / 120.0;
                let ph = j as f32 / 120.0 * tau * (1.5 + k as f32 * 0.6);
                let y =
                    base + (ph + t * (0.12 + 0.04 * k as f32)).sin() * h * 0.012 * (0.3 + drone);
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
    if o.lake {
        // the orbs' and the sun's or moon's light on the water: broken, shimmering strokes
        let mut lights: Vec<(f32, Color32)> = Vec::new();
        if pad > 0.01 {
            for (k, &note) in notes.iter().enumerate() {
                lights.push((orb_at(k, note).x, accent.gamma_multiply(0.3 * pad)));
            }
        }
        if o.day_night {
            let f = if (6.5..19.5).contains(&hour) {
                (hour - 6.5) / 13.0
            } else {
                (hour - 19.5).rem_euclid(24.0) / 11.0
            };
            lights.push((w * (0.08 + 0.84 * f), cream.gamma_multiply(0.35)));
        }
        for (li, (x, c)) in lights.iter().enumerate() {
            for j in 0..7 {
                let y = horizon + (h - horizon) * (0.08 + 0.12 * j as f32);
                let half = (14.0 + 5.0 * j as f32)
                    * u
                    * (0.6 + 0.4 * (t * 1.3 + j as f32 + li as f32).sin());
                let dx = (t * 0.9 + j as f32 * 1.7).sin() * 4.0 * u;
                cv.line(
                    &[pos2(x - half + dx, y), pos2(x + half + dx, y)],
                    2.0 * u,
                    c.gamma_multiply(1.0 - j as f32 / 8.0),
                );
            }
        }
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
    // the notes played on a keyboard: bursts on the wheel
    let now = crate::scene::clock();
    for &(note, at) in &ov.notes {
        let age = (now - at) as f32;
        if !(0.0..3.0).contains(&age) {
            continue;
        }
        let q = orb_at(0, note);
        let a = (1.0 - age / 3.0).powi(2);
        cv.ring(
            q,
            (10.0 + age * 80.0) * u,
            2.5 * u,
            col(1).gamma_multiply(0.7 * a),
        );
        cv.circle(q, (8.0 + 6.0 * a) * u, cream.gamma_multiply(a));
        for k in 0..8 {
            let sa = k as f32 * tau / 8.0 + note as f32;
            let d = (12.0 + age * 110.0) * u;
            cv.circle(
                q + vec2(sa.cos(), sa.sin()) * d,
                2.0 * u,
                accent.gamma_multiply(a),
            );
        }
    }
    // the bells: rings, sparks and falling gold where a bell rings
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
            for k in 0..6 {
                let dx = (hash(note as u32 * 7 + k) - 0.5) * 60.0 * u
                    + (age * 2.0 + k as f32).sin() * 6.0 * u;
                let dy = age * age * (12.0 + 10.0 * hash(note as u32 + k * 3)) * u;
                cv.circle(
                    pos2(x + dx, y + dy),
                    1.5 * u,
                    Color32::from_rgb(255, 214, 120).gamma_multiply(a * 0.8),
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
    // the percussion: ripples across the water, with the pulse's tempo
    let perc = lv(4);
    if perc > 0.02 {
        let c = pos2(
            w * 0.5,
            (horizon + h) * 0.5 + if o.lake { 0.0 } else { -h * 0.06 },
        );
        let beat = 60.0 / recipe.bpm.max(40.0) * 2.0;
        for k in 0..4 {
            let ph = ((t / beat) + k as f32 / 4.0).fract();
            let rx = (30.0 + ph * 700.0) * u;
            let pts: Vec<Pos2> = (0..=64)
                .map(|j| {
                    let a = j as f32 / 64.0 * tau;
                    pos2(c.x + a.cos() * rx, c.y + a.sin() * rx * 0.14)
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

    // the weather: rain in the rainy recipes, else the season's
    if o.weather {
        if rainy {
            for i in 0..140u32 {
                let speed = 0.5 + 0.3 * hash(i + 700);
                let y = (hash(i + 701) + t * speed).fract() * horizon.max(h * 0.8);
                let x = (hash(i + 702) * 1.1 - 0.05) * w - y * 0.12;
                let len = (14.0 + 10.0 * hash(i + 703)) * u;
                cv.line(
                    &[pos2(x, y), pos2(x + len * 0.12, y + len)],
                    1.0 * u,
                    cream.gamma_multiply(0.16),
                );
            }
            if o.lake {
                for i in 0..10u32 {
                    let ph = (t * 0.7 + hash(i + 720) * 10.0).fract();
                    let k = (t * 0.7 + hash(i + 720) * 10.0).floor() as u32;
                    let x = w * hash(i * 31 + k * 7 + 721);
                    let y = horizon + (h - horizon) * (0.1 + 0.8 * hash(i * 17 + k * 3 + 722));
                    let r = (3.0 + ph * 22.0) * u;
                    let pts: Vec<Pos2> = (0..=16)
                        .map(|j| {
                            let a = j as f32 / 16.0 * tau;
                            pos2(x + a.cos() * r, y + a.sin() * r * 0.25)
                        })
                        .collect();
                    cv.line(&pts, 1.0 * u, cream.gamma_multiply(0.25 * (1.0 - ph)));
                }
            }
        } else {
            match season {
                Some(Season::Winter) => {
                    for i in 0..110u32 {
                        let y = (hash(i + 800) + t * (0.02 + 0.02 * hash(i + 801))).fract() * h;
                        let x = (hash(i + 802) + (t * 0.3 + i as f32).sin() * 0.01).fract() * w;
                        cv.circle(
                            pos2(x, y),
                            (1.2 + 1.8 * hash(i + 803)) * u,
                            cream.gamma_multiply(0.55),
                        );
                    }
                }
                Some(Season::Autumn) => {
                    for i in 0..30u32 {
                        let y = (hash(i + 850) + t * (0.025 + 0.015 * hash(i + 851))).fract() * h;
                        let x = (hash(i + 852) + (t * 0.4 + i as f32).sin() * 0.03).fract() * w;
                        let a = t * (1.0 + hash(i + 853)) + i as f32;
                        let d = vec2(a.cos(), a.sin()) * 5.0 * u;
                        let c = if hash(i + 854) < 0.5 {
                            Color32::from_rgb(214, 110, 50)
                        } else {
                            Color32::from_rgb(200, 150, 60)
                        };
                        cv.line(
                            &[pos2(x, y) - d, pos2(x, y) + d],
                            3.5 * u,
                            c.gamma_multiply(0.7),
                        );
                    }
                }
                Some(Season::Spring) => {
                    for i in 0..36u32 {
                        let y = (hash(i + 900) + t * (0.015 + 0.01 * hash(i + 901))).fract() * h;
                        let x = (hash(i + 902) + t * 0.01 + (t * 0.5 + i as f32).sin() * 0.02)
                            .fract()
                            * w;
                        cv.circle(
                            pos2(x, y),
                            2.4 * u,
                            Color32::from_rgb(245, 180, 200).gamma_multiply(0.6),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    // Lumo, the little lantern spirit, on the nearest hill
    if o.mascot {
        let x = 0.82;
        let ground = h * ridge(2, x, t, ridge_top, ridge_step);
        let bob = (t * 1.1).sin() * 2.0 * u;
        let u = u * 1.5;
        let body = pos2(x * w, ground - 16.0 * u + bob);
        let dark = lerp(bg, Color32::BLACK, 0.55);
        cv.circle(body, 17.0 * u, accent.gamma_multiply(0.12));
        cv.circle(body, 15.0 * u, dark);
        cv.circle(body + vec2(-8.0, -13.0) * u, 5.0 * u, dark);
        cv.circle(body + vec2(8.0, -13.0) * u, 5.0 * u, dark);
        // the eyes blink every few seconds
        let blink = (t * 0.23).fract() < 0.03;
        for dx in [-5.0, 5.0] {
            let e = body + vec2(dx, -2.0) * u;
            if blink {
                cv.line(
                    &[e - vec2(2.0, 0.0) * u, e + vec2(2.0, 0.0) * u],
                    1.2 * u,
                    cream.gamma_multiply(0.8),
                );
            } else {
                cv.circle(e, 2.0 * u, cream.gamma_multiply(0.9));
            }
        }
        // its lantern, glowing with the music
        let stick_top = body + vec2(16.0, -10.0) * u;
        let lamp = stick_top + vec2(10.0 + (t * 0.8).sin() * 2.0, 8.0) * u;
        cv.line(
            &[
                body + vec2(10.0, 4.0) * u,
                stick_top,
                lamp - vec2(0.0, 5.0) * u,
            ],
            1.5 * u,
            dark,
        );
        let glow = 0.5 + 0.5 * (lv(1) + lv(0)).min(1.0);
        cv.circle(lamp, 16.0 * u, accent.gamma_multiply(0.12 * glow));
        cv.circle(lamp, 5.0 * u, accent.gamma_multiply(0.9 * glow));
    }

    // the window: the stage seen from inside, on a rainy night
    if o.window {
        let wood = lerp(Color32::from_rgb(52, 34, 24), bg, 0.3);
        let frame = |cv: &mut dyn Canvas, a: Pos2, b: Pos2, top: bool| {
            if top {
                cv.fill_below(
                    &[pos2(a.x, a.y), pos2(b.x, a.y)],
                    b.y,
                    wood,
                    lerp(wood, Color32::BLACK, 0.3),
                );
            }
        };
        frame(cv, pos2(-w, -h), pos2(2.0 * w, h * 0.05), true);
        frame(cv, pos2(-w, h * 0.9), pos2(2.0 * w, h * 1.2), true);
        cv.line(
            &[pos2(w * 0.025, -h), pos2(w * 0.025, h * 2.0)],
            w * 0.05,
            wood,
        );
        cv.line(
            &[pos2(w * 0.975, -h), pos2(w * 0.975, h * 2.0)],
            w * 0.05,
            wood,
        );
        cv.line(
            &[pos2(w * 0.5, 0.0), pos2(w * 0.5, h * 0.9)],
            w * 0.016,
            wood,
        );
        cv.line(&[pos2(0.0, h * 0.46), pos2(w, h * 0.46)], w * 0.012, wood);
        // drops on the glass, some sliding down
        for i in 0..70u32 {
            let x = w * (0.06 + 0.88 * hash(i + 1000));
            let mut y = h * (0.07 + 0.8 * hash(i + 1001));
            let slide = i % 9 == 0;
            if slide {
                y = h * 0.07 + ((hash(i + 1002) + t * 0.03).fract()) * h * 0.82;
                cv.line(
                    &[pos2(x, y - 40.0 * u), pos2(x, y)],
                    1.0 * u,
                    cream.gamma_multiply(0.12),
                );
            }
            cv.circle(
                pos2(x, y),
                (1.5 + 2.5 * hash(i + 1003)) * u,
                cream.gamma_multiply(0.22),
            );
        }
        // the sill: a cup with steam, a small plant, a candle
        let sill = h * 0.9;
        let cup = pos2(w * 0.14, sill - 22.0 * u);
        cv.line(
            &[
                cup - vec2(16.0, 18.0) * u,
                cup - vec2(16.0, -18.0) * u,
                cup + vec2(16.0, 18.0) * u,
                cup + vec2(16.0, -18.0) * u,
            ],
            3.0 * u,
            cream.gamma_multiply(0.7),
        );
        cv.ring(
            cup + vec2(24.0, 0.0) * u,
            8.0 * u,
            3.0 * u,
            cream.gamma_multiply(0.6),
        );
        for k in 0..2 {
            let pts: Vec<Pos2> = (0..12)
                .map(|j| {
                    let f = j as f32 / 11.0;
                    cup + vec2(
                        (f * 6.0 + t * 1.5 + k as f32).sin() * 5.0 + (k as f32 - 0.5) * 10.0,
                        -24.0 - f * 50.0,
                    ) * u
                })
                .collect();
            cv.line(&pts, 2.0 * u, cream.gamma_multiply(0.15));
        }
        let pot = pos2(w * 0.24, sill - 14.0 * u);
        cv.circle(
            pot,
            14.0 * u,
            Color32::from_rgb(150, 80, 50).gamma_multiply(0.8),
        );
        for k in 0..5 {
            let a = -tau / 4.0 + (k as f32 - 2.0) * 0.4 + (t * 0.5 + k as f32).sin() * 0.04;
            cv.line(
                &[pot, pot + vec2(a.cos(), a.sin()) * 40.0 * u],
                4.0 * u,
                Color32::from_rgb(70, 120, 70).gamma_multiply(0.85),
            );
        }
        let candle = pos2(w * 0.86, sill - 24.0 * u);
        cv.line(
            &[candle + vec2(0.0, 24.0) * u, candle],
            12.0 * u,
            cream.gamma_multiply(0.75),
        );
        let flick = 0.8 + 0.2 * (t * 9.0).sin() * (t * 5.3).cos();
        cv.circle(
            candle - vec2(0.0, 10.0) * u,
            40.0 * u,
            Color32::from_rgb(255, 190, 110).gamma_multiply(0.08 * flick),
        );
        cv.circle(
            candle - vec2(0.0, 8.0) * u,
            5.0 * u,
            Color32::from_rgb(255, 210, 130).gamma_multiply(0.95 * flick),
        );
    }

    // the light: the fade and the sleep timer dim everything drawn so far
    cv.set_view(1.0, Vec2::ZERO);
    if vis.light < 0.99 {
        let dark = Color32::from_black_alpha(((1.0 - vis.light) * 255.0) as u8);
        cv.fill_below(&[pos2(0.0, 0.0), pos2(w, 0.0)], h, dark, dark);
    }

    // a bare picture (a thumbnail): no captions
    if ov.bare {
        return;
    }
    // the captions
    let dish = CARTA[id].0;
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
    // what the director (or the chat) just did, fading
    let director = s
        .events
        .iter()
        .flatten()
        .next()
        .map(|(e, age)| (event_text(e), *age as f64));
    let chat = ov.chat_event.as_ref().map(|(e, at)| (e.clone(), now - at));
    let shown = match (director, chat) {
        (Some(d), Some(c)) => Some(if c.1 < d.1 { c } else { d }),
        (d, c) => d.or(c),
    };
    if let Some((text, age)) = shown {
        let a = (1.0 - age as f32 / 12.0).clamp(0.0, 1.0);
        if a > 0.0 {
            cv.text(
                pos2(60.0 * u, h - 60.0 * u),
                Align2::LEFT_BOTTOM,
                &text,
                30.0 * u,
                cream.gamma_multiply(0.85 * a),
            );
        }
    }
    // the chat's newcomers, welcomed
    for (k, (name, at)) in ov.greets.iter().rev().take(3).enumerate() {
        let age = (now - at) as f32;
        let a = (age / 0.8).min(1.0) * (1.0 - (age - 7.0).max(0.0) / 3.0).clamp(0.0, 1.0);
        if a > 0.0 {
            cv.text(
                pos2(60.0 * u, h - 110.0 * u - k as f32 * 30.0 * u),
                Align2::LEFT_BOTTOM,
                &format!("welcome, {name}"),
                22.0 * u,
                accent.gamma_multiply(0.85 * a),
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
    // the people listening: one star each, and how many
    if let Some(v) = ov.viewers {
        for i in 0..v.min(150) {
            let q = pos2(
                w * (0.35 + 0.6 * hash(i * 11 + 2000)),
                h * (0.03 + 0.1 * hash(i * 11 + 2001)),
            );
            let tw = 0.6 + 0.4 * (t * (0.5 + hash(i + 2002)) + i as f32).sin();
            cv.circle(q, 4.0 * u, accent.gamma_multiply(0.1 * tw));
            cv.circle(q, 1.6 * u, cream.gamma_multiply(0.85 * tw));
        }
        cv.text(
            pos2(w - 60.0 * u, h - 90.0 * u),
            Align2::RIGHT_BOTTOM,
            &format!("{v} listening  ·  one star each"),
            18.0 * u,
            cream.gamma_multiply(0.5),
        );
    }
    // the focus timer
    if let Some(f) = &ov.focus {
        let c = pos2(w * 0.5, 70.0 * u);
        let left = f.left.max(0.0) as u64;
        let frac = (1.0 - f.left / f.total.max(1.0)).clamp(0.0, 1.0) as f32;
        cv.ring(c, 34.0 * u, 3.0 * u, cream.gamma_multiply(0.15));
        let arc: Vec<Pos2> = (0..=48)
            .map(|j| {
                let a = -tau / 4.0 + tau * frac * j as f32 / 48.0;
                c + vec2(a.cos(), a.sin()) * 34.0 * u
            })
            .collect();
        cv.line(&arc, 3.5 * u, accent.gamma_multiply(0.85));
        cv.text(
            c,
            Align2::CENTER_CENTER,
            &format!("{:02}:{:02}", left / 60, left % 60),
            20.0 * u,
            cream.gamma_multiply(0.9),
        );
        cv.text(
            c + vec2(0.0, 52.0) * u,
            Align2::CENTER_CENTER,
            &f.label,
            16.0 * u,
            accent.gamma_multiply(0.8),
        );
    }
    // the poll
    if let Some(p) = &ov.poll {
        let x = 60.0 * u;
        let mut y = 120.0 * u;
        let total: u32 = p.options.iter().map(|o| o.1).sum();
        cv.text(
            pos2(x, y),
            Align2::LEFT_TOP,
            &p.question,
            24.0 * u,
            cream.gamma_multiply(0.9),
        );
        y += 38.0 * u;
        for (k, (name, votes)) in p.options.iter().enumerate() {
            let f = if total > 0 {
                *votes as f32 / total as f32
            } else {
                0.0
            };
            cv.line(
                &[pos2(x, y + 12.0 * u), pos2(x + 260.0 * u, y + 12.0 * u)],
                18.0 * u,
                cream.gamma_multiply(0.08),
            );
            if f > 0.0 {
                cv.line(
                    &[pos2(x, y + 12.0 * u), pos2(x + 260.0 * u * f, y + 12.0 * u)],
                    18.0 * u,
                    col(k % 7).gamma_multiply(0.55),
                );
            }
            cv.text(
                pos2(x + 8.0 * u, y + 12.0 * u),
                Align2::LEFT_CENTER,
                &format!("!{}  {name}", k + 1),
                16.0 * u,
                cream.gamma_multiply(0.9),
            );
            cv.text(
                pos2(x + 270.0 * u, y + 12.0 * u),
                Align2::LEFT_CENTER,
                &votes.to_string(),
                16.0 * u,
                cream.gamma_multiply(0.7),
            );
            y += 28.0 * u;
        }
        let left = (p.ends - now).max(0.0) as u64;
        cv.text(
            pos2(x, y + 4.0 * u),
            Align2::LEFT_TOP,
            &format!("vote in the chat  ·  {left} s"),
            14.0 * u,
            cream.gamma_multiply(0.5),
        );
    }
    // a milestone: big, in the middle, with a burst
    if let Some((text, at)) = &ov.milestone {
        let age = (now - at) as f32;
        let a = (age / 0.6).min(1.0) * (1.0 - (age - 6.0).max(0.0) / 3.0).clamp(0.0, 1.0);
        if a > 0.0 {
            cv.ring(
                center,
                big_r * (0.3 + age * 0.5),
                4.0 * u,
                accent.gamma_multiply(0.4 * a),
            );
            cv.text(
                center + vec2(0.0, -big_r * 1.45),
                Align2::CENTER_CENTER,
                text,
                44.0 * u,
                cream.gamma_multiply(a),
            );
        }
    }
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
