//! What the stage draws on: egui's painter in the window, or a software pixmap (tiny-skia) for
//! the live stream, so the stream shows exactly what the window shows, even when the window is
//! hidden or minimized.

use eframe::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

/// The drawing operations the stage uses, in the canvas's own coordinates (origin at its top
/// left, `size()` wide and high).
pub trait Canvas {
    fn size(&self) -> (f32, f32);
    /// The whole canvas, a vertical gradient.
    fn background(&mut self, top: Color32, bottom: Color32);
    fn circle(&mut self, c: Pos2, r: f32, color: Color32);
    fn ring(&mut self, c: Pos2, r: f32, width: f32, color: Color32);
    fn line(&mut self, pts: &[Pos2], width: f32, color: Color32);
    fn text(&mut self, at: Pos2, align: Align2, text: &str, size: f32, color: Color32);
    /// Soft clouds of color (center, radius, color at the center): fading to nothing at the
    /// radius.
    fn clouds(&mut self, clouds: &[(Pos2, f32, Color32)]);
    /// The area under a line (`pts`, left to right) down to `base`: `top` at the line, `bottom`
    /// at the base.
    fn fill_below(&mut self, pts: &[Pos2], base: f32, top: Color32, bottom: Color32);
    /// The backdrop (what moves slowly: the sky, the clouds, the rays, the mountains) may be
    /// kept between frames: true when the canvas put back the last one, so it is not drawn.
    fn backdrop_kept(&mut self) -> bool {
        false
    }
    /// The backdrop is drawn: the canvas may keep it.
    fn backdrop_done(&mut self) {}
}

/// The window's painter, over `rect`.
pub struct EguiCanvas<'a> {
    pub p: &'a Painter,
    pub rect: Rect,
}

impl EguiCanvas<'_> {
    fn at(&self, q: Pos2) -> Pos2 {
        self.rect.min + q.to_vec2()
    }
}

impl Canvas for EguiCanvas<'_> {
    fn size(&self) -> (f32, f32) {
        (self.rect.width(), self.rect.height())
    }

    fn background(&mut self, top: Color32, bottom: Color32) {
        let r = self.rect;
        let mut m = egui::Mesh::default();
        m.colored_vertex(r.left_top(), top);
        m.colored_vertex(r.right_top(), top);
        m.colored_vertex(r.right_bottom(), bottom);
        m.colored_vertex(r.left_bottom(), bottom);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        self.p.add(Shape::mesh(m));
    }

    fn circle(&mut self, c: Pos2, r: f32, color: Color32) {
        self.p.circle_filled(self.at(c), r, color);
    }

    fn ring(&mut self, c: Pos2, r: f32, width: f32, color: Color32) {
        self.p
            .circle_stroke(self.at(c), r, Stroke::new(width, color));
    }

    fn line(&mut self, pts: &[Pos2], width: f32, color: Color32) {
        let pts: Vec<Pos2> = pts.iter().map(|&q| self.at(q)).collect();
        self.p.add(Shape::line(pts, Stroke::new(width, color)));
    }

    fn text(&mut self, at: Pos2, align: Align2, text: &str, size: f32, color: Color32) {
        self.p
            .text(self.at(at), align, text, FontId::proportional(size), color);
    }

    /// A triangle fan per cloud: the color at the center, transparent at the edge (the GPU
    /// blends it smoothly).
    fn clouds(&mut self, clouds: &[(Pos2, f32, Color32)]) {
        for &(c, r, col) in clouds {
            let mut m = egui::Mesh::default();
            let c = self.at(c);
            m.colored_vertex(c, col);
            let n = 48;
            for k in 0..=n {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                m.colored_vertex(c + egui::vec2(a.cos(), a.sin()) * r, Color32::TRANSPARENT);
            }
            for k in 0..n {
                m.add_triangle(0, k + 1, k + 2);
            }
            self.p.add(Shape::mesh(m));
        }
    }

    fn fill_below(&mut self, pts: &[Pos2], base: f32, top: Color32, bottom: Color32) {
        let mut m = egui::Mesh::default();
        for &q in pts {
            m.colored_vertex(self.at(q), top);
            m.colored_vertex(self.at(Pos2::new(q.x, base)), bottom);
        }
        for i in 0..pts.len().saturating_sub(1) as u32 {
            let k = i * 2;
            m.add_triangle(k, k + 1, k + 2);
            m.add_triangle(k + 1, k + 3, k + 2);
        }
        self.p.add(Shape::mesh(m));
    }
}

/// A software canvas: an RGBA pixmap.
pub struct SkiaCanvas {
    pub pixmap: tiny_skia::Pixmap,
    font: fontdue::Font,
    /// The last background and its colors: a gradient is drawn once and copied after.
    background: Option<((Color32, Color32), Vec<u8>)>,
    /// The backdrop (the sky, its clouds, the rays, the mountains), and the frames since it was
    /// drawn: it moves slowly, so it is drawn again only every few frames.
    clouded: Option<Vec<u8>>,
    clouded_age: u32,
    /// The sky with its clouds alone, and its age: the clouds are the dearest part, and move
    /// the slowest.
    sky: Option<Vec<u8>>,
    sky_age: u32,
}

fn paint(c: Color32) -> tiny_skia::Paint<'static> {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let mut p = tiny_skia::Paint::default();
    p.set_color_rgba8(r, g, b, a);
    p.anti_alias = true;
    p
}

impl SkiaCanvas {
    pub fn new(w: u32, h: u32) -> Self {
        let font = fontdue::Font::from_bytes(
            epaint_default_fonts::UBUNTU_LIGHT,
            fontdue::FontSettings::default(),
        )
        .expect("the bundled font");
        SkiaCanvas {
            pixmap: tiny_skia::Pixmap::new(w, h).expect("a pixmap"),
            font,
            background: None,
            clouded: None,
            clouded_age: 0,
            sky: None,
            sky_age: 0,
        }
    }

    /// The pixels, RGBA (premultiplied; the stage's background is opaque, so it is plain RGBA).
    pub fn rgba(&self) -> &[u8] {
        self.pixmap.data()
    }
}

impl Canvas for SkiaCanvas {
    fn size(&self) -> (f32, f32) {
        (self.pixmap.width() as f32, self.pixmap.height() as f32)
    }

    fn background(&mut self, top: Color32, bottom: Color32) {
        if let Some((k, px)) = &self.background
            && *k == (top, bottom)
        {
            self.pixmap.data_mut().copy_from_slice(px);
            return;
        }
        let (w, h) = self.size();
        let c = |c: Color32| {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            tiny_skia::Color::from_rgba8(r, g, b, a)
        };
        let shader = tiny_skia::LinearGradient::new(
            tiny_skia::Point::from_xy(0.0, 0.0),
            tiny_skia::Point::from_xy(0.0, h),
            vec![
                tiny_skia::GradientStop::new(0.0, c(top)),
                tiny_skia::GradientStop::new(1.0, c(bottom)),
            ],
            tiny_skia::SpreadMode::Pad,
            tiny_skia::Transform::identity(),
        );
        let mut p = tiny_skia::Paint::default();
        if let Some(s) = shader {
            p.shader = s;
        }
        if let Some(r) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, h) {
            self.pixmap
                .fill_rect(r, &p, tiny_skia::Transform::identity(), None);
        }
        self.background = Some(((top, bottom), self.pixmap.data().to_vec()));
    }

    /// The clouds drawn at an eighth of the size and scaled up smoothly: soft and cheap.
    fn clouds(&mut self, clouds: &[(Pos2, f32, Color32)]) {
        // drawn about once a second (every sixth backdrop), copied in between
        if self.sky_age < 6
            && let Some(c) = &self.sky
        {
            self.pixmap.data_mut().copy_from_slice(c);
            self.sky_age += 1;
            return;
        }
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let (sw, sh) = ((w / 8).max(1), (h / 8).max(1));
        let Some(mut small) = tiny_skia::Pixmap::new(sw, sh) else {
            return;
        };
        for &(c, r, col) in clouds {
            let [cr, cg, cb, ca] = col.to_srgba_unmultiplied();
            let (cx, cy, rr) = (c.x / 8.0, c.y / 8.0, r / 8.0);
            let shader = tiny_skia::RadialGradient::new(
                tiny_skia::Point::from_xy(cx, cy),
                tiny_skia::Point::from_xy(cx, cy),
                rr,
                vec![
                    tiny_skia::GradientStop::new(0.0, tiny_skia::Color::from_rgba8(cr, cg, cb, ca)),
                    tiny_skia::GradientStop::new(1.0, tiny_skia::Color::from_rgba8(cr, cg, cb, 0)),
                ],
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            );
            let (Some(shader), Some(path)) =
                (shader, tiny_skia::PathBuilder::from_circle(cx, cy, rr))
            else {
                continue;
            };
            let p = tiny_skia::Paint {
                shader,
                anti_alias: true,
                ..Default::default()
            };
            small.fill_path(
                &path,
                &p,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
        let pp = tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        };
        self.pixmap.draw_pixmap(
            0,
            0,
            small.as_ref(),
            &pp,
            tiny_skia::Transform::from_scale(w as f32 / sw as f32, h as f32 / sh as f32),
            None,
        );
        match &mut self.sky {
            Some(c) => c.copy_from_slice(self.pixmap.data()),
            None => self.sky = Some(self.pixmap.data().to_vec()),
        }
        self.sky_age = 0;
    }

    /// The backdrop is drawn ten times a second, copied in between.
    fn backdrop_kept(&mut self) -> bool {
        if self.clouded_age < 3
            && let Some(c) = &self.clouded
        {
            self.pixmap.data_mut().copy_from_slice(c);
            self.clouded_age += 1;
            return true;
        }
        false
    }

    fn backdrop_done(&mut self) {
        match &mut self.clouded {
            Some(c) => c.copy_from_slice(self.pixmap.data()),
            None => self.clouded = Some(self.pixmap.data().to_vec()),
        }
        self.clouded_age = 0;
    }

    fn circle(&mut self, c: Pos2, r: f32, color: Color32) {
        if r <= 0.0 {
            return;
        }
        // a speck is a square: much cheaper than a path, and the same at that size
        if r < 2.0 {
            if let Some(rect) = tiny_skia::Rect::from_xywh(c.x - r, c.y - r, 2.0 * r, 2.0 * r) {
                // no anti-aliasing: tiny-skia's anti-aliased hairlines assert on some specks
                let mut p = paint(color);
                p.anti_alias = false;
                self.pixmap
                    .fill_rect(rect, &p, tiny_skia::Transform::identity(), None);
            }
            return;
        }
        if let Some(path) = tiny_skia::PathBuilder::from_circle(c.x, c.y, r) {
            self.pixmap.fill_path(
                &path,
                &paint(color),
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }

    fn ring(&mut self, c: Pos2, r: f32, width: f32, color: Color32) {
        if r <= 0.0 {
            return;
        }
        if let Some(path) = tiny_skia::PathBuilder::from_circle(c.x, c.y, r) {
            let stroke = tiny_skia::Stroke {
                width,
                ..Default::default()
            };
            self.pixmap.stroke_path(
                &path,
                &paint(color),
                &stroke,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }

    fn line(&mut self, pts: &[Pos2], width: f32, color: Color32) {
        let mut b = tiny_skia::PathBuilder::new();
        for (i, q) in pts.iter().enumerate() {
            if i == 0 {
                b.move_to(q.x, q.y);
            } else {
                b.line_to(q.x, q.y);
            }
        }
        if let Some(path) = b.finish() {
            let stroke = tiny_skia::Stroke {
                width,
                line_cap: tiny_skia::LineCap::Round,
                line_join: tiny_skia::LineJoin::Round,
                ..Default::default()
            };
            self.pixmap.stroke_path(
                &path,
                &paint(color),
                &stroke,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }

    fn fill_below(&mut self, pts: &[Pos2], base: f32, top: Color32, bottom: Color32) {
        let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
            return;
        };
        let mut b = tiny_skia::PathBuilder::new();
        b.move_to(first.x, base);
        for q in pts {
            b.line_to(q.x, q.y);
        }
        b.line_to(last.x, base);
        b.close();
        let Some(path) = b.finish() else {
            return;
        };
        // one color, between the two (a gradient costs several times more in software)
        let c = crate::theme::lerp(top, bottom, 0.4);
        let p = paint(c);
        self.pixmap.fill_path(
            &path,
            &p,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    fn text(&mut self, at: Pos2, align: Align2, text: &str, size: f32, color: Color32) {
        // the bundled font has no ♯: a plain sharp sign
        let text = text.replace('♯', "#");
        let glyphs: Vec<(fontdue::Metrics, Vec<u8>)> = text
            .chars()
            .map(|ch| self.font.rasterize(ch, size))
            .collect();
        let width: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
        let line = self.font.horizontal_line_metrics(size);
        let (ascent, descent) = line.map_or((size * 0.8, -size * 0.2), |l| (l.ascent, l.descent));
        let height = ascent - descent;
        let x0 = match align.x() {
            egui::Align::Min => at.x,
            egui::Align::Center => at.x - width / 2.0,
            egui::Align::Max => at.x - width,
        };
        let top = match align.y() {
            egui::Align::Min => at.y,
            egui::Align::Center => at.y - height / 2.0,
            egui::Align::Max => at.y - height,
        };
        let baseline = top + ascent;
        let [r, g, b, a] = color.to_srgba_unmultiplied();
        let (pw, ph) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        let data = self.pixmap.data_mut();
        let mut x = x0;
        for (m, bitmap) in &glyphs {
            let gx = (x + m.xmin as f32).round() as i32;
            let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
            for row in 0..m.height as i32 {
                for col in 0..m.width as i32 {
                    let (px, py) = (gx + col, gy + row);
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        continue;
                    }
                    let cov = bitmap[(row * m.width as i32 + col) as usize] as u32 * a as u32 / 255;
                    if cov == 0 {
                        continue;
                    }
                    let i = ((py * pw + px) * 4) as usize;
                    for (k, c) in [r, g, b].into_iter().enumerate() {
                        let d = data[i + k] as u32;
                        data[i + k] = ((c as u32 * cov + d * (255 - cov)) / 255) as u8;
                    }
                    data[i + 3] = 255;
                }
            }
            x += m.advance_width;
        }
    }
}
