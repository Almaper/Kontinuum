//! The main window: the header with the display and the transport, the Carta, the six layers,
//! the director's knobs and the log of what it does.

use std::collections::VecDeque;
use std::path::PathBuf;

use eframe::egui::{
    self, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui, UiBuilder, pos2,
    vec2,
};
use knt_engine::{AUTOS, CARTA, Command, Event, Layer, Mode, Params, Recipe, Snapshot, note_name};
use knt_player::Player;

use crate::midi::{Map, Midi, Msg, Target};
use crate::stage;
use crate::stream::{Config, State, Stream};
use crate::theme::{self, *};

/// The design size of the window; the content scales to the window.
pub const W: f32 = 1280.0;
pub const H: f32 = 800.0;

/// A screenshot request (`--screenshot`).
pub struct Shot {
    pub path: PathBuf,
    pub frames: u32,
    pub after: u32,
}

pub struct App {
    pub player: Option<Player>,
    pub params: Params,
    sent: Params,
    pub recipe: usize,
    pub seed: u64,
    pub playing: bool,
    /// The automations and the layers' levels over the last minute (10 a second).
    history: VecDeque<([f32; 6], [f32; knt_engine::LAYERS])>,
    last_sample: f64,
    /// The stage, full window.
    pub stage: bool,
    pub shot: Option<Shot>,
    /// What the window last read from the player.
    snap: Snapshot,
    pub status: Option<String>,
    pub midi: Midi,
    midi_map: Map,
    /// The control waiting for a controller to move (MIDI learn).
    learning: Option<Target>,
    midi_open: bool,
    /// The last MIDI message, for the MIDI window, and when.
    midi_last: Option<(String, f64)>,
    /// The live stream: its settings, the running one, and what it needs from the player.
    stream_cfg: Config,
    stream: Option<Stream>,
    stream_open: bool,
    tap: Option<rtrb::Consumer<f32>>,
    tap_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    shared: Option<std::sync::Arc<std::sync::Mutex<Snapshot>>>,
    stream_msg: Option<String>,
    /// The smoothed picture state, for the stage and the layers' windows.
    visual: stage::Visual,
}

impl Drop for App {
    fn drop(&mut self) {
        // ffmpeg ends cleanly (and a recording is finished) when the window closes
        if let Some(s) = self.stream.take() {
            s.stop();
        }
    }
}

impl App {
    pub fn new(mut player: Option<Player>, seed: u64) -> Self {
        let tap = player.as_mut().and_then(|p| p.tap());
        let tap_flag = player.as_ref().map(|p| p.tap_flag());
        let shared = player.as_ref().map(|p| p.shared_snapshot());
        App {
            player,
            params: Params::default(),
            sent: Params::default(),
            recipe: 7,
            seed,
            playing: false,
            history: VecDeque::new(),
            last_sample: 0.0,
            stage: false,
            shot: None,
            snap: Snapshot::default(),
            status: None,
            midi: Midi::new(None),
            midi_map: Map::load(),
            learning: None,
            midi_open: false,
            midi_last: None,
            stream_cfg: Config::load(),
            stream: None,
            stream_open: false,
            tap,
            tap_flag,
            shared,
            stream_msg: None,
            visual: stage::Visual::default(),
        }
    }

    fn send(&mut self, c: Command) {
        if let Some(p) = &mut self.player {
            let _ = p.send(c);
        }
    }

    pub fn set_playing(&mut self, on: bool) {
        self.playing = on;
        self.send(Command::Play(on));
    }

    pub fn choose_recipe(&mut self, i: usize) {
        self.recipe = i.min(CARTA.len() - 1);
        let r: Recipe = CARTA[self.recipe].1;
        self.send(Command::Recipe(Box::new(r)));
    }

    pub fn new_seed(&mut self) {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        self.seed = t % 100_000;
        self.send(Command::Seed(self.seed));
    }

    /// The MIDI messages since the last frame: controllers move their targets (or are
    /// learned); the notes already went to the player.
    fn midi_in(&mut self, now: f64) {
        while let Ok(m) = self.midi.rx.try_recv() {
            let text = match m {
                Msg::NoteOn { ch, note, vel } => {
                    format!(
                        "ch {} note {} {} vel {vel}",
                        ch + 1,
                        note,
                        note_name(note as i32)
                    )
                }
                Msg::NoteOff { ch, note } => format!("ch {} note off {}", ch + 1, note),
                Msg::Cc { ch, cc, value } => {
                    if let Some(t) = self.learning.take() {
                        self.midi_map.set((ch, cc), t);
                        if let Err(e) = self.midi_map.save() {
                            self.status = Some(format!("MIDI map not saved: {e}"));
                        }
                    }
                    if let Some(t) = self.midi_map.get((ch, cc)) {
                        t.apply(&mut self.params, value);
                    }
                    format!("ch {} CC {} = {}", ch + 1, cc, value)
                }
            };
            self.midi_last = Some((text, now));
        }
    }

    fn sync(&mut self, now: f64) {
        self.midi_in(now);
        if self.params != self.sent {
            self.sent = self.params;
            self.send(Command::Params(Box::new(self.params)));
        }
        if let Some(p) = &mut self.player {
            self.snap = *p.snapshot();
        }
        if now - self.last_sample > 0.1 {
            self.last_sample = now;
            self.history.push_back((self.snap.auto, self.snap.levels));
            while self.history.len() > 600 {
                self.history.pop_front();
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.sync(now);
        // fluid while the music plays (every screen refresh); idle, a few times a second
        if self.playing || self.stage {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.visual.update(&self.snap, now);
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            let on = !self.playing;
            self.set_playing(on);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F11) || i.key_pressed(egui::Key::Escape)) {
            self.stage = !self.stage && !ctx.input(|i| i.key_pressed(egui::Key::Escape));
        }
        let full = ctx.content_rect();
        if self.stage {
            let p = ui.painter_at(full);
            p.rect_filled(full, 0.0, Color32::BLACK);
            let w = full.width().min(full.height() * 16.0 / 9.0);
            let r = Rect::from_center_size(full.center(), vec2(w, w * 9.0 / 16.0));
            let mut cv = crate::canvas::EguiCanvas {
                p: &p.with_clip_rect(r),
                rect: r,
            };
            let badge = self.stream_badge();
            self.visual.update(&self.snap, now);
            stage::draw(&mut cv, &self.snap, &self.visual, badge.as_deref());
            self.stream_window(&ctx);
            let r = ui.interact(full, egui::Id::unique("stage"), Sense::click());
            if r.double_clicked() {
                self.stage = false;
            }
            self.screenshot(&ctx);
            return;
        }
        // the design is W x H: scale it to the window
        let scale = (full.width() / W).min(full.height() / H);
        ctx.set_zoom_factor(scale.clamp(0.5, 3.0) * ctx.zoom_factor());
        let full = ctx.content_rect();
        let o = full.min
            + vec2(
                (full.width() - W).max(0.0) / 2.0,
                (full.height() - H).max(0.0) / 2.0,
            );
        let r = |x: f32, y: f32, w: f32, h: f32| theme::rect(o.x + x, o.y + y, w, h);
        theme::wood(ui.painter(), full);
        self.header(ui, r(16.0, 14.0, W - 32.0, 104.0), now);
        self.carta(ui, r(16.0, 130.0, W - 32.0, 86.0));
        self.layers(ui, r(16.0, 228.0, W - 32.0, 292.0), now);
        self.director(ui, r(16.0, 532.0, 640.0, 254.0));
        self.log(ui, r(668.0, 532.0, W - 684.0, 254.0), now);
        if let Some(s) = &self.status {
            ui.painter().text(
                pos2(o.x + 20.0, o.y + H - 4.0),
                Align2::LEFT_BOTTOM,
                s,
                FontId::proportional(11.0),
                CREAM_DARK,
            );
        }
        self.midi_window(&ctx, now);
        self.stream_window(&ctx);
        self.screenshot(&ctx);
    }

    /// The stage's badge while streaming or recording: "LIVE 0:12:03".
    fn stream_badge(&self) -> Option<String> {
        let st = self.stream.as_ref()?.status();
        let rec = self.stream_cfg.record.is_some();
        let label = if rec { "REC" } else { "LIVE" };
        match st.state {
            State::Live => {
                let t = st.since.map_or(0, |s| s.elapsed().as_secs());
                Some(format!(
                    "{label}  {}:{:02}:{:02}",
                    t / 3600,
                    t / 60 % 60,
                    t % 60
                ))
            }
            State::Starting => Some(format!("{label}  connecting…")),
            State::Retrying(_) => Some(format!("{label}  reconnecting…")),
            _ => None,
        }
    }

    pub fn go_live(&mut self) {
        let (Some(tap), Some(flag), Some(shared)) =
            (self.tap.take(), self.tap_flag.clone(), self.shared.clone())
        else {
            self.stream_msg = Some("There is no audio player to stream.".into());
            return;
        };
        if self.stream_cfg.record.is_none() && self.stream_cfg.key.trim().is_empty() {
            self.tap = Some(tap);
            self.stream_msg = Some("Paste your stream key first.".into());
            return;
        }
        if let Err(e) = self.stream_cfg.save() {
            self.stream_msg = Some(format!("Settings not saved: {e}"));
        }
        if !self.playing {
            self.set_playing(true);
        }
        self.stream = Some(Stream::start(self.stream_cfg.clone(), tap, flag, shared));
    }

    pub fn stop_stream(&mut self) {
        if let Some(s) = self.stream.take() {
            self.tap = s.stop();
        }
    }

    /// The LIVE window: where to stream (the key), the quality, a recording instead, and the
    /// stream's state.
    fn stream_window(&mut self, ctx: &egui::Context) {
        if !self.stream_open {
            return;
        }
        // a stream that failed for good gives the tap back
        if let Some(s) = &self.stream
            && let State::Failed(e) = s.status().state
        {
            self.stream_msg = Some(e);
            self.stop_stream();
        }
        let mut open = true;
        let mut go = false;
        let mut stop = false;
        egui::Window::new("Go live")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(pos2(700.0, 120.0))
            .show(ctx, |ui| {
                let running = self.stream.is_some();
                ui.add_enabled_ui(!running, |ui| {
                    let c = &mut self.stream_cfg;
                    ui.label("Stream the stage and the music to YouTube, without OBS.");
                    egui::Grid::new("stream_cfg").num_columns(2).show(ui, |ui| {
                        ui.label("Stream key:");
                        ui.add(
                            egui::TextEdit::singleline(&mut c.key)
                                .password(true)
                                .desired_width(260.0),
                        )
                        .on_hover_text("YouTube Studio > Go live > Stream > Stream key");
                        ui.end_row();
                        ui.label("Server:");
                        ui.add(egui::TextEdit::singleline(&mut c.url).desired_width(260.0));
                        ui.end_row();
                        ui.label("Quality:");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut c.height, 720, "720p");
                            ui.selectable_value(&mut c.height, 1080, "1080p");
                            ui.label("30 fps");
                        });
                        ui.end_row();
                        ui.label("Record instead:");
                        let mut rec = c.record.is_some();
                        if ui
                            .checkbox(&mut rec, "to a file (to test, or to keep)")
                            .changed()
                        {
                            c.record = rec.then(|| {
                                let dir = std::env::var_os("USERPROFILE")
                                    .or_else(|| std::env::var_os("HOME"))
                                    .map(PathBuf::from)
                                    .unwrap_or_else(|| PathBuf::from("."));
                                dir.join("Kontinuum live.mkv")
                            });
                        }
                        ui.end_row();
                    });
                    if let Some(r) = &self.stream_cfg.record {
                        ui.label(egui::RichText::new(format!("File: {}", r.display())).small());
                    }
                });
                ui.separator();
                match &self.stream {
                    None => {
                        if ui
                            .button(egui::RichText::new("● Go live").strong().color(RUST))
                            .clicked()
                        {
                            go = true;
                        }
                    }
                    Some(s) => {
                        let st = s.status();
                        let text = match &st.state {
                            State::Starting => "Connecting…".to_string(),
                            State::Live => format!(
                                "ON AIR · {} · {} frames{}",
                                self.stream_badge().unwrap_or_default(),
                                st.frames,
                                if st.restarts > 0 {
                                    format!(" · {} reconnects", st.restarts)
                                } else {
                                    String::new()
                                }
                            ),
                            State::Retrying(e) => format!("Reconnecting: {e}"),
                            State::Failed(e) => format!("Failed: {e}"),
                            State::Off => "Off".into(),
                        };
                        ui.colored_label(RUST, text);
                        if !st.target.is_empty() {
                            ui.label(egui::RichText::new(format!("To {}", st.target)).small());
                        }
                        if !st.log.is_empty() {
                            ui.label(egui::RichText::new(format!("ffmpeg: {}", st.log)).small());
                        }
                        if ui.button("■ Stop").clicked() {
                            stop = true;
                        }
                    }
                }
                if let Some(m) = &self.stream_msg {
                    ui.colored_label(RUST, m);
                }
                ui.label(
                    egui::RichText::new(format!(
                        "Uses ffmpeg ({}). The stream keeps going with the window minimized.",
                        self.stream_cfg.ffmpeg().display()
                    ))
                    .small(),
                );
            });
        if go {
            self.stream_msg = None;
            self.go_live();
        }
        if stop {
            self.stop_stream();
        }
        if !open {
            self.stream_open = false;
        }
    }

    /// The MIDI window: the input port, the last message, and the controllers to learn.
    fn midi_window(&mut self, ctx: &egui::Context, now: f64) {
        if !self.midi_open {
            return;
        }
        let mut open = true;
        egui::Window::new("MIDI")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_pos(pos2(820.0, 120.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Input:");
                    let current = self.midi.port.clone().unwrap_or_else(|| "none".into());
                    let mut choice = None;
                    egui::ComboBox::from_id_salt("midi_port")
                        .selected_text(current)
                        .width(220.0)
                        .show_ui(ui, |ui| {
                            for p in Midi::ports() {
                                if ui
                                    .selectable_label(self.midi.port.as_deref() == Some(&p), &p)
                                    .clicked()
                                {
                                    choice = Some(Some(p));
                                }
                            }
                            if ui
                                .selectable_label(self.midi.port.is_none(), "none")
                                .clicked()
                            {
                                choice = Some(None);
                            }
                        });
                    match choice {
                        Some(Some(p)) => self.midi.connect(&p),
                        Some(None) => self.midi.disconnect(),
                        None => {}
                    }
                });
                if let Some(e) = &self.midi.error {
                    ui.colored_label(RUST, e);
                }
                let last = match &self.midi_last {
                    Some((t, at)) if now - at < 5.0 => t.clone(),
                    _ => "—".into(),
                };
                ui.label(format!("Last message: {last}"));
                ui.label(
                    "Keys play the layer marked KEYS; channel 10 plays the percussion (drum pads).",
                );
                ui.separator();
                ui.label("MIDI learn: click Learn, then move a knob or fader of your controller.");
                egui::Grid::new("midi_learn").striped(true).show(ui, |ui| {
                    for t in Target::ALL {
                        ui.label(t.name());
                        let cc = self.midi_map.of(t);
                        ui.label(if cc.is_empty() {
                            "—".to_string()
                        } else {
                            cc.join(", ")
                        });
                        let armed = self.learning == Some(t);
                        if ui
                            .selectable_label(
                                armed,
                                if armed { "Move a control…" } else { "Learn" },
                            )
                            .clicked()
                        {
                            self.learning = if armed { None } else { Some(t) };
                        }
                        ui.end_row();
                    }
                });
            });
        if !open {
            self.midi_open = false;
            self.learning = None;
        }
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(shot) = &mut self.shot else {
            return;
        };
        shot.frames += 1;
        ctx.request_repaint();
        if shot.frames == shot.after {
            let path = shot.path.clone();
            ctx.request_screenshot(Box::new(move |image: std::sync::Arc<egui::ColorImage>| {
                let [w, h] = image.size;
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    let _ = img.save(&path);
                }
                std::process::exit(0);
            }));
        }
    }

    // ---- the header ----

    fn header(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        let p = ui.painter().clone();
        plate(&p, r, None);
        wordmark(&p, pos2(r.left() + 30.0, r.top() + 28.0), 34.0, INK);
        p.text(
            pos2(r.left() + 30.0, r.top() + 80.0),
            Align2::LEFT_CENTER,
            spaced("ambient · drone · generative"),
            FontId::proportional(10.0),
            INK_SOFT,
        );
        // the display
        let d = Rect::from_min_size(pos2(r.left() + 320.0, r.top() + 12.0), vec2(440.0, 80.0));
        display(&p, d);
        let s = &self.snap;
        let dish = CARTA[s.recipe as usize % CARTA.len()].0;
        let glow = AMBER;
        p.text(
            d.left_top() + vec2(16.0, 14.0),
            Align2::LEFT_TOP,
            dish,
            FontId::proportional(22.0),
            glow,
        );
        let key = match s.scale {
            Some(sc) if s.section > 0 => format!("{} {}", note_name(s.root), sc.name()),
            _ => "—".into(),
        };
        let chord = if s.chord.len > 0 {
            s.chord
                .notes()
                .iter()
                .map(|&n| note_name(n))
                .collect::<Vec<_>>()
                .join(" · ")
        } else {
            "—".into()
        };
        p.text(
            d.left_top() + vec2(16.0, 46.0),
            Align2::LEFT_TOP,
            format!("{key}   ·   chord {chord}"),
            FontId::proportional(13.0),
            glow.gamma_multiply(0.8),
        );
        let t = s.time as u64;
        p.text(
            d.right_top() + vec2(-16.0, 14.0),
            Align2::RIGHT_TOP,
            format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60),
            FontId::monospace(20.0),
            glow,
        );
        p.text(
            d.right_top() + vec2(-16.0, 46.0),
            Align2::RIGHT_TOP,
            format!("section {}   seed {}", s.section, self.seed),
            FontId::monospace(12.0),
            glow.gamma_multiply(0.7),
        );
        // the chord's progress, a thin line at the bottom of the glass
        let y = d.bottom() - 8.0;
        p.line_segment(
            [pos2(d.left() + 16.0, y), pos2(d.right() - 16.0, y)],
            Stroke::new(2.0, AMBER_DIM),
        );
        p.line_segment(
            [
                pos2(d.left() + 16.0, y),
                pos2(d.left() + 16.0 + (d.width() - 32.0) * s.chord_progress, y),
            ],
            Stroke::new(2.0, AMBER),
        );
        // the transport
        let mut b = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
            pos2(d.right() + 18.0, r.top() + 14.0),
            vec2(r.right() - d.right() - 30.0, 80.0),
        )));
        b.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let playing = self.playing;
            if theme::button(
                ui,
                if playing { "STOP" } else { "PLAY" },
                if playing { "■" } else { "▶" },
                playing,
            ) {
                self.set_playing(!playing);
            }
            if theme::button(ui, "NEXT", "⏭", false) {
                self.send(Command::NextSection);
            }
            if theme::button(ui, "SEED", "⚄", false) {
                self.new_seed();
            }
            if theme::button(ui, "STAGE", "◐", self.stage) {
                self.stage = true;
            }
            if theme::button(ui, "EXPORT", "⤓", false) {
                self.export();
            }
            let live = self.midi.port.is_some();
            if theme::button(ui, "MIDI", "♪", live || self.midi_open) {
                self.midi_open = !self.midi_open;
            }
            let on_air = self.stream.is_some();
            if theme::button(
                ui,
                if on_air { "ON AIR" } else { "LIVE" },
                "●",
                on_air || self.stream_open,
            ) {
                self.stream_open = !self.stream_open;
            }
        });
        let _ = now;
    }

    fn export(&mut self) {
        let dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let name = format!("Kontinuum - {} - {}.ogg", CARTA[self.recipe].0, self.seed);
        let path = dir.join(name);
        let (seed, recipe, params) = (self.seed, CARTA[self.recipe].1, self.params);
        let out = path.clone();
        std::thread::spawn(move || {
            let _ = knt_cli::export(&out, seed, recipe, params, 10.0, 15.0);
        });
        self.status = Some(format!("Exporting 10 minutes to {} ...", path.display()));
    }

    // ---- the Carta ----

    fn carta(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        let n = CARTA.len();
        let gap = 10.0;
        let w = (r.width() - gap * (n - 1) as f32) / n as f32;
        for (i, (name, recipe, about)) in CARTA.iter().enumerate() {
            let t = Rect::from_min_size(
                pos2(r.left() + i as f32 * (w + gap), r.top()),
                vec2(w, r.height()),
            );
            let resp = ui.interact(t, egui::Id::unique(("carta", i)), Sense::click());
            let chosen = self.recipe == i;
            let playing = self.snap.recipe as usize == i && self.playing;
            p.rect_filled(
                t.translate(vec2(0.0, 2.0)),
                8.0,
                Color32::from_black_alpha(80),
            );
            p.rect_filled(t, 8.0, if chosen { CREAM } else { CREAM_DARK });
            if chosen {
                p.rect_stroke(t, 8.0, Stroke::new(2.0, AMBER), egui::StrokeKind::Inside);
            }
            // the dish's colors
            for (k, c) in recipe.palette.iter().enumerate() {
                p.circle_filled(
                    pos2(t.left() + 16.0 + k as f32 * 13.0, t.top() + 16.0),
                    5.0,
                    Color32::from_rgb(c[0], c[1], c[2]),
                );
            }
            if playing {
                theme::lamp(&p, pos2(t.right() - 14.0, t.top() + 16.0), 1.0);
            }
            p.text(
                pos2(t.left() + 10.0, t.top() + 34.0),
                Align2::LEFT_TOP,
                *name,
                FontId::proportional(13.5),
                INK,
            );
            let short: String = about.chars().take(26).collect();
            p.text(
                pos2(t.left() + 10.0, t.top() + 54.0),
                Align2::LEFT_TOP,
                if about.chars().count() > 26 {
                    format!("{short}…")
                } else {
                    short
                },
                FontId::proportional(9.5),
                INK_SOFT,
            );
            if resp.on_hover_text(*about).clicked() {
                self.choose_recipe(i);
            }
        }
    }

    // ---- the layers ----

    fn layers(&mut self, ui: &mut Ui, r: Rect, now: f64) {
        let n = Layer::ALL.len();
        let gap = 12.0;
        let w = (r.width() - gap * (n - 1) as f32) / n as f32;
        for (i, layer) in Layer::ALL.into_iter().enumerate() {
            let m = Rect::from_min_size(
                pos2(r.left() + i as f32 * (w + gap), r.top()),
                vec2(w, r.height()),
            );
            let p = ui.painter().clone();
            plate(&p, m, Some(layer.name()));
            // the live window
            let win = Rect::from_min_size(m.left_top() + vec2(12.0, 32.0), vec2(w - 24.0, 120.0));
            display(&p, win);
            self.layer_view(&p.with_clip_rect(win.shrink(3.0)), win, layer, now);
            // the fader, the meter and the mute
            let mut c = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(
                pos2(m.left() + 14.0, win.bottom() + 8.0),
                vec2(40.0, m.bottom() - win.bottom() - 16.0),
            )));
            let v = self.params.levels.get_mut(layer);
            theme::fader(&mut c, v, m.bottom() - win.bottom() - 18.0);
            // the meter
            let lvl = (self.snap.levels[i] * self.snap.gains[i].max(0.05) * 8.0)
                .sqrt()
                .min(1.0);
            let mr = Rect::from_min_size(
                pos2(m.left() + 64.0, win.bottom() + 16.0),
                vec2(8.0, m.bottom() - win.bottom() - 34.0),
            );
            p.rect_filled(mr, 2.0, DISPLAY);
            let hgt = mr.height() * lvl;
            p.rect_filled(
                Rect::from_min_max(pos2(mr.left(), mr.bottom() - hgt), mr.right_bottom()),
                2.0,
                AMBER,
            );
            // mute lamp
            let lc = pos2(m.right() - 34.0, win.bottom() + 30.0);
            let muted = self.params.mutes[i];
            theme::lamp(
                &p,
                lc,
                if muted {
                    0.0
                } else {
                    self.snap.gains[i].clamp(0.25, 1.0)
                },
            );
            let mr = Rect::from_center_size(lc, vec2(40.0, 40.0));
            if ui
                .interact(mr, egui::Id::unique(("mute", i)), Sense::click())
                .on_hover_text("Mute / unmute")
                .clicked()
            {
                self.params.mutes[i] = !muted;
            }
            p.text(
                lc + vec2(0.0, 18.0),
                Align2::CENTER_CENTER,
                if muted { "MUTED" } else { "ON" },
                FontId::proportional(9.0),
                INK_SOFT,
            );
            // what the director does with this layer
            let note = match layer {
                Layer::Perc if !self.snap.pulse => "resting",
                Layer::Perc => "pulse",
                Layer::Choir if !self.snap.choir => "silent",
                Layer::Choir => "singing",
                _ if !self.snap.active[i] => "resting",
                _ => self.snap.patches[i],
            };
            p.text(
                pos2(m.right() - 34.0, m.bottom() - 30.0),
                Align2::CENTER_CENTER,
                note,
                FontId::proportional(10.0),
                INK,
            );
            let play = self.params.play_layer == layer;
            let kr =
                Rect::from_center_size(pos2(m.right() - 34.0, m.bottom() - 54.0), vec2(56.0, 18.0));
            p.rect_filled(kr, 9.0, if play { AMBER } else { CREAM_DARK });
            p.text(
                kr.center(),
                Align2::CENTER_CENTER,
                "KEYS",
                FontId::proportional(9.0),
                INK,
            );
            if ui
                .interact(kr, egui::Id::unique(("kb", i)), Sense::click())
                .on_hover_text("The MIDI keyboard plays this layer")
                .clicked()
            {
                self.params.play_layer = layer;
            }
        }
    }

    /// A small live picture of each layer.
    fn layer_view(&self, p: &Painter, r: Rect, layer: Layer, now: f64) {
        let s = &self.snap;
        let t = now as f32;
        let i = Layer::ALL.iter().position(|&l| l == layer).unwrap_or(0);
        let lvl = self.visual.levels[i];
        let recipe = CARTA[s.recipe as usize % CARTA.len()].1;
        let c = stage::layer_color(&recipe, i, self.visual.hue);
        match layer {
            Layer::Drone => {
                let bright = (s.drone_cutoff / 2000.0).clamp(0.1, 1.0);
                for k in 0..3 {
                    let pts: Vec<Pos2> = (0..=60)
                        .map(|j| {
                            let x = r.left() + r.width() * j as f32 / 60.0;
                            let ph =
                                j as f32 / 60.0 * std::f32::consts::TAU * (1.0 + k as f32 * 0.5);
                            let y = r.center().y
                                + (ph + t * (0.3 + k as f32 * 0.1)).sin() * r.height() * 0.3 * lvl;
                            pos2(x, y)
                        })
                        .collect();
                    p.add(Shape::line(
                        pts,
                        Stroke::new(1.5, c.gamma_multiply(bright * (1.0 - k as f32 * 0.25))),
                    ));
                }
            }
            Layer::Pad => {
                let notes = s.chord.notes();
                for (k, &n) in notes.iter().enumerate() {
                    let x = r.left() + r.width() * (k as f32 + 0.5) / notes.len().max(1) as f32;
                    let y = r.bottom()
                        - 14.0
                        - ((n - 36) as f32 / 40.0).clamp(0.0, 1.0) * (r.height() - 30.0);
                    let rad = 6.0 + 6.0 * lvl * (1.0 + (t * 0.7 + k as f32).sin() * 0.2);
                    p.circle_filled(pos2(x, y), rad + 6.0, c.gamma_multiply(0.15));
                    p.circle_filled(pos2(x, y), rad, c.gamma_multiply(0.85));
                    p.text(
                        pos2(x, r.bottom() - 6.0),
                        Align2::CENTER_BOTTOM,
                        note_name(n),
                        FontId::proportional(9.0),
                        c.gamma_multiply(0.7),
                    );
                }
            }
            Layer::Texture => {
                let center = ((s.texture_center / 150.0).log2() / 5.5).clamp(0.0, 1.0);
                let mut seed = (t * 20.0) as u32 | 1;
                for _ in 0..90 {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    let x = r.left() + (seed % 1000) as f32 / 1000.0 * r.width();
                    let spread = (seed >> 10) % 1000;
                    let y = r.bottom() - r.height() * (center * 0.8 + 0.1)
                        + (spread as f32 / 1000.0 - 0.5) * r.height() * 0.5;
                    p.circle_filled(pos2(x, y), 1.2, c.gamma_multiply(0.3 + lvl * 0.7));
                }
            }
            Layer::Bells => {
                for &(n, age) in &s.bells {
                    if age.is_finite() && age < 6.0 {
                        let x = r.left() + ((n - 60) as f32 / 40.0).clamp(0.05, 0.95) * r.width();
                        let a = (1.0 - age / 6.0).max(0.0);
                        let rad = 4.0 + age * 10.0;
                        p.circle_stroke(
                            pos2(x, r.center().y),
                            rad,
                            Stroke::new(1.5, c.gamma_multiply(a)),
                        );
                        p.circle_filled(pos2(x, r.center().y), 3.0, c.gamma_multiply(a));
                    }
                }
            }
            Layer::Perc => {
                let beat = ((t * 1.1).fract() - 0.5).abs() * 2.0;
                let rad = 10.0 + 30.0 * lvl * beat;
                let col = if s.pulse { c } else { AMBER_DIM };
                p.circle_stroke(
                    r.center(),
                    rad,
                    Stroke::new(2.0, col.gamma_multiply(0.4 + lvl)),
                );
                p.circle_filled(r.center(), 5.0 + 6.0 * lvl, col);
            }
            Layer::Modular => {
                // sixteen steps, the lit one moving with the pulse of the level
                let step = ((t * 4.0) as usize) % 16;
                for k in 0..16 {
                    let x = r.left() + r.width() * (k as f32 + 0.5) / 16.0;
                    let on = k == step && lvl > 0.02;
                    let col = if on { c } else { AMBER_DIM };
                    p.circle_filled(
                        pos2(x, r.center().y),
                        if on { 4.0 + 3.0 * lvl } else { 2.5 },
                        col,
                    );
                }
                let sweep = (t * 0.2).sin() * 0.5 + 0.5;
                let pts: Vec<Pos2> = (0..=40)
                    .map(|j| {
                        let x = r.left() + r.width() * j as f32 / 40.0;
                        let d = (j as f32 / 40.0 - sweep).abs();
                        pos2(
                            x,
                            r.bottom() - 10.0 - (1.0 - d).powi(6) * 40.0 * lvl.max(0.1),
                        )
                    })
                    .collect();
                p.add(Shape::line(pts, Stroke::new(1.2, c.gamma_multiply(0.6))));
            }
            Layer::Choir => {
                let v = knt_engine::layers::VOWEL_NAMES[(s.vowel.round() as usize).min(4)];
                for k in 0..3 {
                    let pts: Vec<Pos2> = (0..=50)
                        .map(|j| {
                            let x = r.left() + r.width() * j as f32 / 50.0;
                            let env = (j as f32 / 50.0 * std::f32::consts::PI).sin();
                            let y = r.center().y
                                + (j as f32 * 0.4 + t * (0.8 + k as f32 * 0.3)).sin()
                                    * env
                                    * r.height()
                                    * 0.25
                                    * lvl;
                            pos2(x, y)
                        })
                        .collect();
                    p.add(Shape::line(
                        pts,
                        Stroke::new(1.2, c.gamma_multiply(0.7 - k as f32 * 0.2)),
                    ));
                }
                p.text(
                    r.right_top() + vec2(-8.0, 6.0),
                    Align2::RIGHT_TOP,
                    v,
                    FontId::proportional(11.0),
                    c.gamma_multiply(0.8),
                );
            }
        }
    }

    // ---- the director ----

    fn director(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        plate(&p, r, Some("The director"));
        let mut c = ui.new_child(
            UiBuilder::new().max_rect(r.shrink2(vec2(20.0, 30.0)).translate(vec2(0.0, 12.0))),
        );
        c.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let bias = |v: &mut f32| (*v + 1.0) / 2.0;
            let pr = &mut self.params;
            theme::knob(ui, &mut pr.evolution, "EVOLUTION", 0.6);
            let mut x = bias(&mut pr.pace_bias);
            if theme::knob(ui, &mut x, "PACE", 0.5) {
                pr.pace_bias = x * 2.0 - 1.0;
            }
            let mut x = bias(&mut pr.brightness_bias);
            if theme::knob(ui, &mut x, "BRIGHTNESS", 0.5) {
                pr.brightness_bias = x * 2.0 - 1.0;
            }
            let mut x = bias(&mut pr.density_bias);
            if theme::knob(ui, &mut x, "DENSITY", 0.5) {
                pr.density_bias = x * 2.0 - 1.0;
            }
            let mut x = bias(&mut pr.space_bias);
            if theme::knob(ui, &mut x, "SPACE", 0.5) {
                pr.space_bias = x * 2.0 - 1.0;
            }
            let mut x = bias(&mut pr.echo_bias);
            if theme::knob(ui, &mut x, "ECHO", 0.5) {
                pr.echo_bias = x * 2.0 - 1.0;
            }
            theme::knob(ui, &mut pr.warmth, "WARMTH", 0.35);
            theme::knob(ui, &mut pr.volume, "VOLUME", 0.8);
        });
        c.add_space(8.0);
        c.horizontal(|ui| {
            ui.label(egui::RichText::new("When you play:").color(INK).strong());
            for m in Mode::ALL {
                if ui
                    .selectable_label(self.params.mode == m, m.name())
                    .clicked()
                {
                    self.params.mode = m;
                }
            }
            ui.separator();
            ui.checkbox(&mut self.params.hold_harmony, "Hold harmony");
        });
        c.horizontal(|ui| {
            ui.label(egui::RichText::new("Sleep timer:").color(INK).strong());
            for (label, m) in [
                ("Off", 0.0),
                ("15 min", 15.0),
                ("30 min", 30.0),
                ("60 min", 60.0),
            ] {
                if ui
                    .selectable_label(self.params.sleep_minutes == m, label)
                    .clicked()
                {
                    self.params.sleep_minutes = m;
                }
            }
            if let Some(left) = self.snap.sleep_left {
                ui.label(egui::RichText::new(format!("{:.0} min left", left / 60.0)).color(RUST));
            }
        });
    }

    // ---- the log ----

    fn log(&mut self, ui: &mut Ui, r: Rect, _now: f64) {
        let p = ui.painter().clone();
        plate(&p, r, Some("Log & automations"));
        let d = Rect::from_min_size(
            r.left_top() + vec2(14.0, 32.0),
            vec2(r.width() - 28.0, 92.0),
        );
        display(&p, d);
        let mut y = d.top() + 10.0;
        for (e, age) in self.snap.events.iter().flatten().take(5) {
            let a = (1.0 - age / 120.0).clamp(0.35, 1.0);
            p.text(
                pos2(d.left() + 12.0, y),
                Align2::LEFT_TOP,
                format!("{:>5}  {}", ago(*age), event_text(e)),
                FontId::monospace(11.0),
                AMBER.gamma_multiply(a),
            );
            y += 15.0;
        }
        if self.snap.events.iter().all(|e| e.is_none()) {
            p.text(
                d.center(),
                Align2::CENTER_CENTER,
                "Press PLAY to start",
                FontId::proportional(12.0),
                AMBER_DIM,
            );
        }
        // the automations: one lane each, the last minute
        let lanes = Rect::from_min_max(
            pos2(d.left(), d.bottom() + 10.0),
            pos2(d.right(), r.bottom() - 14.0),
        );
        let lane_h = lanes.height() / AUTOS.len() as f32;
        for (k, name) in AUTOS.iter().enumerate() {
            let lr = Rect::from_min_size(
                pos2(lanes.left(), lanes.top() + k as f32 * lane_h),
                vec2(lanes.width(), lane_h - 2.0),
            );
            p.text(
                pos2(lr.left(), lr.center().y),
                Align2::LEFT_CENTER,
                *name,
                FontId::proportional(10.0),
                INK_SOFT,
            );
            let gr = Rect::from_min_max(pos2(lr.left() + 78.0, lr.top()), lr.right_bottom());
            p.line_segment(
                [
                    pos2(gr.left(), gr.center().y),
                    pos2(gr.right(), gr.center().y),
                ],
                Stroke::new(1.0, CREAM_DARK),
            );
            let n = self.history.len();
            if n > 1 {
                let pts: Vec<Pos2> = self
                    .history
                    .iter()
                    .enumerate()
                    .map(|(j, (a, _))| {
                        let x = gr.right() - (n - 1 - j) as f32 / 600.0 * gr.width();
                        pos2(x, gr.center().y - a[k] * gr.height() * 0.45)
                    })
                    .collect();
                p.add(Shape::line(pts, Stroke::new(1.5, RUST)));
            }
        }
    }
}

fn ago(s: f32) -> String {
    if s < 60.0 {
        format!("{s:.0}s")
    } else {
        format!("{:.0}m", s / 60.0)
    }
}

/// What an event says, in the log and on the stage.
pub fn event_text(e: &Event) -> String {
    match e {
        Event::Chord { root, minor } => {
            format!(
                "Chord {}{}",
                note_name(*root),
                if *minor { "m" } else { "" }
            )
        }
        Event::Key { root, scale } => format!("Key: {} {}", note_name(*root), scale.name()),
        Event::Section(n) => format!("Section {n}"),
        Event::PulseIn => "The pulse comes in".into(),
        Event::PulseOut => "The pulse fades out".into(),
        Event::ChoirIn => "The choir comes in".into(),
        Event::ChoirOut => "The choir falls silent".into(),
        Event::Recipe(id) => format!(
            "Now: {}",
            knt_engine::recipe::name(*id).unwrap_or("your recipe")
        ),
        Event::UserIn => "You are playing".into(),
        Event::UserOut => "The director takes over".into(),
        Event::Enters(l, "") => format!("{} comes in", l.name()),
        Event::Enters(l, t) => format!("{} comes in: {t}", l.name()),
        Event::Leaves(l) => format!("{} fades out", l.name()),
    }
}
