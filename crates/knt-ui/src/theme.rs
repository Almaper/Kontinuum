//! The warm analog look: walnut, cream faceplates, bakelite knobs, amber lamps and an amber
//! display. Everything is drawn with egui's painter: no image files.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2, pos2, vec2,
};

pub const WALNUT: Color32 = Color32::from_rgb(0x2b, 0x1b, 0x12);
pub const WALNUT_DARK: Color32 = Color32::from_rgb(0x1c, 0x11, 0x0b);
pub const CREAM: Color32 = Color32::from_rgb(0xef, 0xe3, 0xc8);
pub const CREAM_DARK: Color32 = Color32::from_rgb(0xd9, 0xca, 0xa8);
pub const INK: Color32 = Color32::from_rgb(0x3b, 0x24, 0x16);
pub const INK_SOFT: Color32 = Color32::from_rgb(0x7a, 0x5c, 0x44);
pub const AMBER: Color32 = Color32::from_rgb(0xf2, 0xa0, 0x3a);
pub const AMBER_DIM: Color32 = Color32::from_rgb(0x6a, 0x3e, 0x14);
pub const RUST: Color32 = Color32::from_rgb(0xc4, 0x51, 0x2a);
pub const BAKELITE: Color32 = Color32::from_rgb(0x1f, 0x1a, 0x17);
pub const DISPLAY: Color32 = Color32::from_rgb(0x17, 0x0e, 0x08);

/// The style of the widgets egui draws itself (menus, combo boxes, windows).
pub fn install(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.global_style_mut(|s| {
        let v = &mut s.visuals;
        v.panel_fill = WALNUT;
        v.window_fill = CREAM;
        v.extreme_bg_color = CREAM_DARK;
        v.override_text_color = Some(INK);
        v.selection.bg_fill = AMBER;
        v.selection.stroke = Stroke::new(1.0, INK);
        v.widgets.inactive.weak_bg_fill = CREAM_DARK;
        v.widgets.inactive.bg_fill = CREAM_DARK;
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xe8, 0xd6, 0xb0);
        v.widgets.active.weak_bg_fill = AMBER;
        v.window_stroke = Stroke::new(1.0, INK_SOFT);
        s.spacing.item_spacing = vec2(8.0, 6.0);
    });
}

/// The wood behind everything: a dark gradient with fine grain lines.
pub fn wood(p: &Painter, r: Rect) {
    p.rect_filled(r, 0.0, WALNUT);
    let mut seed = 0x9E37_79B9u32;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    let lines = (r.height() / 5.0) as usize;
    for i in 0..lines {
        let y = r.top() + i as f32 * 5.0 + rnd() * 3.0;
        let a = (rnd() * 18.0) as u8;
        let c = if rnd() < 0.5 {
            Color32::from_rgba_unmultiplied(0x6a, 0x42, 0x28, a)
        } else {
            Color32::from_rgba_unmultiplied(0x0c, 0x06, 0x03, a + 10)
        };
        let wave = rnd() * 6.0;
        let pts: Vec<Pos2> = (0..=24)
            .map(|k| {
                let x = r.left() + r.width() * k as f32 / 24.0;
                pos2(x, y + (k as f32 * 0.6 + wave).sin() * 1.5)
            })
            .collect();
        p.add(Shape::line(pts, Stroke::new(1.0 + rnd() * 1.5, c)));
    }
}

/// A cream faceplate with screws in its corners and an engraved title.
pub fn plate(p: &Painter, r: Rect, title: Option<&str>) {
    p.rect_filled(
        r.translate(vec2(0.0, 3.0)),
        10.0,
        Color32::from_black_alpha(90),
    );
    p.rect_filled(r, 10.0, CREAM);
    p.rect_stroke(r, 10.0, Stroke::new(1.0, CREAM_DARK), StrokeKind::Inside);
    for c in [
        r.left_top() + vec2(9.0, 9.0),
        r.right_top() + vec2(-9.0, 9.0),
        r.left_bottom() + vec2(9.0, -9.0),
        r.right_bottom() + vec2(-9.0, -9.0),
    ] {
        p.circle_filled(c, 3.2, CREAM_DARK);
        p.line_segment(
            [c - vec2(2.0, 0.0), c + vec2(2.0, 0.0)],
            Stroke::new(1.0, INK_SOFT),
        );
    }
    if let Some(t) = title {
        p.text(
            pos2(r.center().x, r.top() + 16.0),
            Align2::CENTER_CENTER,
            spaced(t),
            FontId::proportional(12.0),
            INK,
        );
    }
}

/// Letter-spaced capitals, the engraved labels' look.
pub fn spaced(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.to_uppercase().chars().enumerate() {
        if i > 0 {
            out.push('\u{2009}');
        }
        out.push(c);
    }
    out
}

/// The amber display: a dark recessed glass.
pub fn display(p: &Painter, r: Rect) {
    p.rect_filled(r, 6.0, DISPLAY);
    p.rect_stroke(r, 6.0, Stroke::new(2.0, WALNUT_DARK), StrokeKind::Inside);
    // a faint glass reflection
    let g = Rect::from_min_size(
        r.min + vec2(4.0, 4.0),
        vec2(r.width() - 8.0, r.height() * 0.3),
    );
    p.rect_filled(g, 4.0, Color32::from_white_alpha(6));
}

/// A lamp: amber when on, dark when off, with a glow.
pub fn lamp(p: &Painter, c: Pos2, on: f32) {
    let on = on.clamp(0.0, 1.0);
    if on > 0.05 {
        p.circle_filled(c, 9.0, AMBER.gamma_multiply(0.18 * on));
    }
    p.circle_filled(c, 5.0, lerp(AMBER_DIM, AMBER, on));
    p.circle_stroke(c, 5.0, Stroke::new(1.0, WALNUT_DARK));
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// A bakelite knob with an amber arc for its value; drag up or down to turn it. `value` 0..1.
/// A double click sets `default`. Returns whether it changed.
pub fn knob(ui: &mut Ui, value: &mut f32, label: &str, default: f32) -> bool {
    let size = vec2(64.0, 78.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let before = *value;
    if resp.dragged() {
        let d = -resp.drag_delta().y + resp.drag_delta().x * 0.5;
        *value = (*value + d / 160.0).clamp(0.0, 1.0);
    }
    if resp.double_clicked() {
        *value = default;
    }
    let p = ui.painter_at(rect.expand(4.0));
    let c = pos2(rect.center().x, rect.top() + 30.0);
    let r = 21.0;
    // the scale: 270 degrees from bottom-left
    let a0 = std::f32::consts::PI * 0.75;
    let span = std::f32::consts::PI * 1.5;
    let arc = |from: f32, to: f32, rad: f32| -> Vec<Pos2> {
        let n = 32;
        (0..=n)
            .map(|k| {
                let a = a0 + span * (from + (to - from) * k as f32 / n as f32);
                c + vec2(a.cos(), a.sin()) * rad
            })
            .collect()
    };
    p.add(Shape::line(
        arc(0.0, 1.0, r + 6.0),
        Stroke::new(3.0, CREAM_DARK),
    ));
    p.add(Shape::line(
        arc(0.0, *value, r + 6.0),
        Stroke::new(3.0, AMBER),
    ));
    // ticks
    for k in 0..=10 {
        let a = a0 + span * k as f32 / 10.0;
        let d = vec2(a.cos(), a.sin());
        p.line_segment(
            [c + d * (r + 9.0), c + d * (r + 12.0)],
            Stroke::new(1.0, INK_SOFT),
        );
    }
    p.circle_filled(c + vec2(0.0, 2.0), r, Color32::from_black_alpha(80));
    p.circle_filled(c, r, BAKELITE);
    // ridges
    for k in 0..24 {
        let a = k as f32 / 24.0 * std::f32::consts::TAU;
        let d = vec2(a.cos(), a.sin());
        p.line_segment(
            [c + d * (r - 2.5), c + d * r],
            Stroke::new(1.0, Color32::from_gray(55)),
        );
    }
    p.circle_filled(c, r - 5.0, Color32::from_rgb(0x2a, 0x24, 0x20));
    let a = a0 + span * *value;
    let d = vec2(a.cos(), a.sin());
    p.line_segment([c + d * 5.0, c + d * (r - 3.0)], Stroke::new(3.0, CREAM));
    let hot = resp.hovered() || resp.dragged();
    p.text(
        pos2(rect.center().x, rect.bottom() - 9.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        if hot { RUST } else { INK },
    );
    let resp = resp.on_hover_text(format!("{label}: {:.0} %", *value * 100.0));
    let _ = resp;
    *value != before
}

/// A vertical fader with a cream cap. `value` 0..1. Returns whether it changed.
pub fn fader(ui: &mut Ui, value: &mut f32, height: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, height), Sense::click_and_drag());
    let before = *value;
    let top = rect.top() + 10.0;
    let bottom = rect.bottom() - 10.0;
    if resp.dragged()
        && let Some(pt) = resp.interact_pointer_pos()
    {
        *value = ((bottom - pt.y) / (bottom - top)).clamp(0.0, 1.0);
    }
    if resp.double_clicked() {
        *value = 0.8;
    }
    let p = ui.painter_at(rect.expand(2.0));
    let x = rect.center().x;
    p.rect_filled(
        Rect::from_center_size(pos2(x, (top + bottom) / 2.0), vec2(5.0, bottom - top)),
        2.0,
        WALNUT_DARK,
    );
    for k in 0..=10 {
        let y = bottom - (bottom - top) * k as f32 / 10.0;
        let w = if k % 5 == 0 { 8.0 } else { 5.0 };
        p.line_segment(
            [pos2(x - 9.0 - w, y), pos2(x - 9.0, y)],
            Stroke::new(1.0, INK_SOFT),
        );
    }
    let y = bottom - (bottom - top) * *value;
    let cap = Rect::from_center_size(pos2(x, y), vec2(26.0, 14.0));
    p.rect_filled(
        cap.translate(vec2(0.0, 2.0)),
        3.0,
        Color32::from_black_alpha(80),
    );
    p.rect_filled(cap, CornerRadius::same(3), CREAM_DARK);
    p.line_segment(
        [pos2(cap.left() + 3.0, y), pos2(cap.right() - 3.0, y)],
        Stroke::new(2.0, INK),
    );
    *value != before
}

/// A round push button with a label under it; `lit` lights its ring.
pub fn button(ui: &mut Ui, label: &str, icon: &str, lit: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(58.0, 62.0), Sense::click());
    let p = ui.painter_at(rect.expand(4.0));
    let c = pos2(rect.center().x, rect.top() + 22.0);
    if lit {
        p.circle_filled(c, 26.0, AMBER.gamma_multiply(0.25));
    }
    p.circle_filled(c + vec2(0.0, 2.0), 20.0, Color32::from_black_alpha(90));
    let face = if resp.is_pointer_button_down_on() {
        CREAM_DARK
    } else if resp.hovered() {
        Color32::from_rgb(0xf6, 0xec, 0xd6)
    } else {
        CREAM
    };
    p.circle_filled(c, 20.0, face);
    p.circle_stroke(
        c,
        20.0,
        Stroke::new(2.0, if lit { AMBER } else { CREAM_DARK }),
    );
    p.text(
        c,
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(16.0),
        INK,
    );
    p.text(
        pos2(c.x, rect.bottom() - 8.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(10.0),
        INK,
    );
    resp.clicked()
}

/// The italic-KNT wordmark, stroked (the logo of tools/logo.py), `h` high at `at` (top left);
/// returns its width.
pub fn wordmark(p: &Painter, at: Pos2, h: f32, color: Color32) -> f32 {
    let s = h / 140.0;
    let word: [(char, f32, bool); 9] = [
        ('K', 92.0, true),
        ('O', 104.0, false),
        ('N', 92.0, true),
        ('T', 96.0, true),
        ('I', 18.0, false),
        ('N', 92.0, false),
        ('U', 90.0, false),
        ('U', 90.0, false),
        ('M', 104.0, false),
    ];
    let stroke = Stroke::new((10.0 * s).max(1.5), color);
    let mut x = 0.0;
    for (ch, w, italic) in word {
        let slant = |px: f32, py: f32| -> Pos2 {
            // the italic leans by tan(13°), around the baseline (y = 140)
            let dx = if italic {
                (140.0 - py) * 0.2309 - 140.0 * 0.2309 / 2.0
            } else {
                0.0
            };
            pos2(at.x + (x + px + dx) * s, at.y + py * s)
        };
        let line = |pts: &[(f32, f32)]| {
            Shape::line(pts.iter().map(|&(a, b)| slant(a, b)).collect(), stroke)
        };
        match ch {
            'K' => {
                p.add(line(&[(0.0, 0.0), (0.0, 140.0)]));
                p.add(line(&[(w, 0.0), (6.0, 78.4)]));
                p.add(line(&[(w * 0.3, 56.0), (w, 140.0)]));
            }
            'O' => {
                let pts: Vec<(f32, f32)> = (0..=48)
                    .map(|k| {
                        let a = k as f32 / 48.0 * std::f32::consts::TAU;
                        (w / 2.0 + a.cos() * w / 2.0, 70.0 + a.sin() * 70.0)
                    })
                    .collect();
                p.add(line(&pts));
            }
            'N' => {
                p.add(line(&[(0.0, 140.0), (0.0, 0.0), (w, 140.0), (w, 0.0)]));
            }
            'T' => {
                p.add(line(&[(0.0, 0.0), (w, 0.0)]));
                p.add(line(&[(w / 2.0, 0.0), (w / 2.0, 140.0)]));
            }
            'I' => {
                p.add(line(&[(w / 2.0, 0.0), (w / 2.0, 140.0)]));
            }
            'U' => {
                let r = w / 2.0;
                let mut pts = vec![(0.0, 0.0), (0.0, 140.0 - r)];
                for k in 0..=16 {
                    let a = std::f32::consts::PI * (1.0 - k as f32 / 16.0);
                    pts.push((r + a.cos() * r, 140.0 - r + a.sin() * r));
                }
                pts.push((w, 0.0));
                p.add(line(&pts));
            }
            'M' => {
                p.add(line(&[
                    (0.0, 140.0),
                    (0.0, 0.0),
                    (w / 2.0, 86.8),
                    (w, 0.0),
                    (w, 140.0),
                ]));
            }
            _ => {}
        }
        x += w + 38.0;
    }
    (x - 38.0) * s
}

/// A small helper: a rect from its left top and size.
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), Vec2::new(w, h))
}
