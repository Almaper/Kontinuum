//! The server mode: Kontinuum with no window and no sound card, streaming on its own (on a
//! cloud server, say). `Kontinuum --server [server.toml]`.
//!
//! The music is rendered in real time on a thread of its own and fed to the stream; the stage
//! is drawn by the stream as usual. A settings file says what to play and where to stream;
//! the chat, the schedule, the time of day and a new seed every few hours keep it alive. It
//! writes the texts for YouTube next to the settings, and logs to the standard output (the
//! service's journal).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use knt_engine::extras::Ambience;
use knt_engine::{CARTA, Command, Params, Snapshot};
use serde::{Deserialize, Serialize};

use crate::stream::{State, Stream};

/// What to play.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Music {
    /// A recipe of the Menu by name ("Deep Focus"), or "clock": the time of day chooses.
    pub recipe: String,
    /// The seed; 0: a new one at each start.
    pub seed: u64,
    /// A new seed every this many hours (0: never), so that a long stream keeps changing.
    pub new_seed_hours: f32,
    /// Nature under the music: "none", "rain", "forest", "fire" or "sea"; and how loud (0..1).
    pub ambience: String,
    pub ambience_level: f32,
    pub lofi: bool,
    /// Binaural tones: the beat in Hz (0: off).
    pub binaural: f32,
    /// How far the director moves (0..1).
    pub evolution: f32,
}

impl Default for Music {
    fn default() -> Self {
        Music {
            recipe: "Deep Focus".into(),
            seed: 0,
            new_seed_hours: 3.0,
            ambience: "none".into(),
            ambience_level: 0.5,
            lofi: false,
            binaural: 0.0,
            evolution: Params::default().evolution,
        }
    }
}

/// The control page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Web {
    pub enabled: bool,
    pub port: u16,
    /// Its password (one is made up the first time).
    pub password: String,
}

impl Default for Web {
    fn default() -> Self {
        Web {
            enabled: true,
            port: 8080,
            password: String::new(),
        }
    }
}

/// The server's settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub music: Music,
    pub stream: crate::stream::Config,
    pub stage: crate::scene::Options,
    pub chat: crate::chat::Config,
    pub web: Web,
}

/// The settings file's place when none is given.
pub fn default_path() -> PathBuf {
    crate::scene::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("server.toml")
}

fn log(s: &str) {
    let line = format!("{} {s}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("{line}");
    crate::web::push_log(line);
}

fn recipe_index(name: &str) -> Option<usize> {
    let n = name.trim().to_lowercase();
    CARTA
        .iter()
        .position(|c| c.0.to_lowercase() == n)
        .or_else(|| n.parse::<usize>().ok().filter(|&i| i < CARTA.len()))
        .or_else(|| crate::chat::recipe_command(&n))
}

fn new_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        % 100_000
}

fn params_of(m: &Music) -> Params {
    Params {
        ambience: Ambience::ALL
            .into_iter()
            .find(|a| a.name() == m.ambience.trim().to_lowercase())
            .unwrap_or(Ambience::None),
        ambience_level: m.ambience_level.clamp(0.0, 1.0),
        lofi: m.lofi,
        binaural: m.binaural.max(0.0),
        evolution: m.evolution.clamp(0.0, 1.0),
        ..Params::default()
    }
}

const HEAD: &str = "# Kontinuum's server settings. The easy way to change them is the control page\n\
                    # (http://THIS-SERVER:8080, the password is in [web]). By hand: edit, then\n\
                    #   sudo systemctl restart kontinuum\n\
                    # [music] recipe: Early Mist, Deep Focus, Cathedral, Copper Rain, Wet Forest, Orbit,\n\
                    #   Dusk or Drift; or \"clock\" to follow the time of day.\n\n";

/// Writes the settings, with a word on how to change them.
pub fn save(path: &Path, s: &Settings) -> Result<(), String> {
    let body = toml::to_string_pretty(s).map_err(|e| e.to_string())?;
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, format!("{HEAD}{body}")).map_err(|e| format!("{}: {e}", path.display()))
}

/// The settings, or new ones (written) the first time; a password for the page if none.
pub fn load(path: &Path) -> Result<Settings, String> {
    let mut s: Settings = if path.exists() {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?
    } else {
        Settings::default()
    };
    if s.web.password.trim().is_empty() || !path.exists() {
        if s.web.password.trim().is_empty() {
            s.web.password = crate::web::new_password();
        }
        save(path, &s)?;
    }
    Ok(s)
}

/// Runs until killed: the control page once, the music and the stream again after each change
/// of the settings.
pub fn run(path: PathBuf) -> Result<(), String> {
    let mut cfg = load(&path)?;
    let shared = Arc::new(crate::web::Shared::default());
    let (tx, rx) = std::sync::mpsc::channel();
    if cfg.web.enabled {
        if let Ok(mut p) = shared.password.lock() {
            *p = cfg.web.password.clone();
        }
        match crate::web::start(cfg.web.port, shared.clone(), tx) {
            Ok(()) => log(&format!(
                "the control page: http://THIS-SERVER:{} (password: {})",
                cfg.web.port, cfg.web.password
            )),
            Err(e) => log(&e),
        }
    }
    loop {
        match session(&path, &cfg, &shared, &rx)? {
            Some(next) => {
                log("the new settings are saved: starting again with them");
                cfg = next;
                if let Ok(mut p) = shared.password.lock() {
                    *p = cfg.web.password.clone();
                }
            }
            None => return Ok(()),
        }
    }
}

/// The music and the stream with `cfg`, until new settings come (returned) or the end.
fn session(
    path: &Path,
    cfg: &Settings,
    shared: &Arc<crate::web::Shared>,
    rx: &std::sync::mpsc::Receiver<crate::web::WebCmd>,
) -> Result<Option<Settings>, String> {
    use crate::web::WebCmd;
    let texts_path = path.with_file_name("youtube-texts.txt");
    if let Ok(mut s) = shared.settings.lock() {
        *s = crate::web::masked(&serde_json::to_value(cfg).map_err(|e| e.to_string())?);
    }
    crate::scene::with_overlay(|o| o.opts = cfg.stage);
    let mut clock = cfg.music.recipe.trim().eq_ignore_ascii_case("clock");
    let mut recipe = if clock {
        crate::app::recipe_for_hour(crate::scene::local_time().0)
    } else {
        recipe_index(&cfg.music.recipe).unwrap_or_else(|| {
            log(&format!(
                "no recipe {:?} in the Menu: Deep Focus",
                cfg.music.recipe
            ));
            1
        })
    };
    let mut seed = if cfg.music.seed == 0 {
        new_seed()
    } else {
        cfg.music.seed
    };
    let mut params = params_of(&cfg.music);
    let (mut player, mut audio) = knt_player::Player::new(seed, CARTA[recipe].1, params);
    let tap = player.tap().ok_or("no tap")?;
    let flag = player.tap_flag();
    let snapshot: Arc<Mutex<Snapshot>> = player.shared_snapshot();
    let _ = player.send(Command::Play(true));
    log(&format!(
        "Kontinuum {}: {} (seed {seed}){}",
        env!("CARGO_PKG_VERSION"),
        CARTA[recipe].0,
        if clock { ", following the clock" } else { "" }
    ));
    let has_key = cfg.stream.record.is_some() || !cfg.stream.key.trim().is_empty();
    if !has_key {
        log("no stream key yet: paste it in the control page (Settings > Stream)");
    }

    // the music, in real time: rendered a little ahead of the clock, never behind
    let quit = Arc::new(AtomicBool::new(false));
    let q2 = quit.clone();
    std::thread::Builder::new()
        .name("kontinuum-music".into())
        .spawn(move || {
            let mut buf = vec![0.0f32; 1024 * 2];
            let start = Instant::now();
            let mut frames: u64 = 0;
            while !q2.load(Ordering::SeqCst) {
                let ahead = frames as f64 / 44_100.0 - start.elapsed().as_secs_f64();
                if ahead > 0.1 {
                    std::thread::sleep(Duration::from_secs_f64((ahead - 0.05).min(0.05)));
                    continue;
                }
                audio.render_f32(&mut buf);
                frames += 1024;
            }
        })
        .map_err(|e| e.to_string())?;

    let mut tap = Some(tap);
    let mut stream: Option<Stream> = None;
    let mut manual: Option<bool> = None;
    let mut last_state = String::new();
    let mut chat = None;
    let mut brain = crate::chat::Brain::default();
    let mut seeded = Instant::now();
    let mut written = String::new();
    let mut texts = serde_json::Value::Null;
    let mut chapters = crate::promo::Chapters::default();
    let mut live_since: Option<Instant> = None;
    let mut vis = crate::stage::Visual::default();
    let mut last_frame: Option<Instant> = None;
    let mut last_state_at = Instant::now() - Duration::from_secs(5);
    let started = Instant::now();
    if !cfg.chat.api_key.trim().is_empty() {
        match crate::chat::Reader::start(&cfg.chat) {
            Ok(r) => chat = Some(r),
            Err(e) => log(&format!("chat: {e}")),
        }
    }
    let finish = |stream: Option<Stream>, quit: &AtomicBool| {
        if let Some(s) = stream {
            s.stop();
        }
        quit.store(true, Ordering::SeqCst);
    };
    loop {
        std::thread::sleep(Duration::from_millis(100));
        player.collect_garbage();
        let now = crate::scene::clock();
        let (hour, _) = crate::scene::local_time();
        // the page's commands
        while let Ok(c) = rx.try_recv() {
            match c {
                WebCmd::Recipe(r) if r < CARTA.len() => {
                    clock = false;
                    recipe = r;
                    log(&format!("the page chose {}", CARTA[r].0));
                    let _ = player.send(Command::Recipe(Box::new(CARTA[r].1)));
                }
                WebCmd::Recipe(_) => {}
                WebCmd::Clock(on) => {
                    clock = on;
                    log(if on {
                        "following the clock"
                    } else {
                        "not following the clock"
                    });
                }
                WebCmd::NewSeed => {
                    seed = new_seed();
                    seeded = Instant::now();
                    log(&format!("a new seed: {seed}"));
                    let _ = player.send(Command::Seed(seed));
                }
                WebCmd::NextSection => {
                    let _ = player.send(Command::NextSection);
                }
                WebCmd::Ambience(a) => {
                    params.ambience = Ambience::ALL
                        .into_iter()
                        .find(|x| x.name() == a)
                        .unwrap_or(Ambience::None);
                    let _ = player.send(Command::Params(Box::new(params)));
                }
                WebCmd::Level(l) => {
                    params.ambience_level = l.clamp(0.0, 1.0);
                    let _ = player.send(Command::Params(Box::new(params)));
                }
                WebCmd::Lofi(on) => {
                    params.lofi = on;
                    let _ = player.send(Command::Params(Box::new(params)));
                }
                WebCmd::Binaural(b) => {
                    params.binaural = b.max(0.0);
                    let _ = player.send(Command::Params(Box::new(params)));
                }
                WebCmd::Live(m) => {
                    manual = m;
                    log(match m {
                        Some(true) => "the page: live now",
                        Some(false) => "the page: stop",
                        None => "the page: as the schedule says",
                    });
                }
                WebCmd::Save(v) => {
                    let old = serde_json::to_value(cfg).unwrap_or_default();
                    let v = crate::web::unmask(&v, &old);
                    match serde_json::from_value::<Settings>(v) {
                        Ok(next) => match save(path, &next) {
                            Ok(()) => {
                                finish(stream.take(), &quit);
                                return Ok(Some(next));
                            }
                            Err(e) => log(&format!("the settings were not saved: {e}")),
                        },
                        Err(e) => log(&format!("the settings were not saved: {e}")),
                    }
                }
            }
        }
        // live or not: the page decides, else the schedule
        let scheduled = !cfg.stream.schedule.on || cfg.stream.schedule.within(hour);
        let want = has_key && manual.unwrap_or(scheduled);
        if want && stream.is_none() {
            if let Some(t) = tap.take() {
                log(&format!("going live: {}", CARTA[recipe].0));
                chapters = crate::promo::Chapters::default();
                live_since = Some(Instant::now());
                stream = Some(Stream::start(
                    cfg.stream.clone(),
                    t,
                    flag.clone(),
                    snapshot.clone(),
                ));
            }
        } else if !want && stream.is_some() {
            log("stopping the stream");
            tap = stream.take().and_then(|s| s.stop());
            live_since = None;
        }
        let state_line = match &stream {
            Some(s) => {
                let st = s.status();
                match &st.state {
                    State::Live => format!("live: {}", st.target),
                    State::Retrying(e) => format!("reconnecting: {e}"),
                    other => format!("{other:?}"),
                }
            }
            None if !has_key => "waiting for the stream key".into(),
            None if manual == Some(false) => "stopped".into(),
            None => format!(
                "off: the schedule says {} to {}",
                cfg.stream.schedule.start, cfg.stream.schedule.stop
            ),
        };
        if state_line != last_state {
            log(&state_line);
            last_state = state_line.clone();
        }
        // the time of day, a new seed now and then
        if clock {
            let r = crate::app::recipe_for_hour(hour);
            if r != recipe {
                recipe = r;
                log(&format!("the clock chooses {}", CARTA[r].0));
                let _ = player.send(Command::Recipe(Box::new(CARTA[r].1)));
            }
        }
        if cfg.music.new_seed_hours > 0.0
            && seeded.elapsed().as_secs_f32() > cfg.music.new_seed_hours * 3600.0
        {
            seeded = Instant::now();
            seed = new_seed();
            log(&format!("a new seed: {seed}"));
            let _ = player.send(Command::Seed(seed));
        }
        // the chat
        let mut acts = Vec::new();
        if let Some(r) = &chat {
            while let Ok(e) = r.rx.try_recv() {
                match e {
                    crate::chat::Event::Message(m) => {
                        acts.extend(brain.message(&m, &cfg.chat, now, recipe))
                    }
                    crate::chat::Event::Numbers {
                        viewers,
                        likes,
                        subs,
                    } => {
                        if let Some(v) = viewers {
                            acts.push(crate::chat::Action::Viewers(v));
                        }
                        acts.extend(brain.numbers(subs, likes));
                    }
                    crate::chat::Event::Status(s) => log(&format!("chat: {s}")),
                }
            }
        }
        acts.extend(brain.tick(&cfg.chat, now, recipe));
        for a in acts {
            use crate::chat::Action;
            match a {
                Action::Recipe(r) => {
                    recipe = r;
                    log(&format!("the chat chose {}", CARTA[r].0));
                    let _ = player.send(Command::Recipe(Box::new(CARTA[r].1)));
                }
                Action::Ambience(x) => {
                    params.ambience = x;
                    let _ = player.send(Command::Params(Box::new(params)));
                }
                Action::Lofi(on) => {
                    params.lofi = on;
                    let _ = player.send(Command::Params(Box::new(params)));
                }
                Action::Greet(n) => crate::scene::with_overlay(|o| o.greets.push((n, now))),
                Action::Milestone(t) => {
                    crate::scene::with_overlay(|o| o.milestone = Some((t, now)))
                }
                Action::Viewers(v) => crate::scene::with_overlay(|o| o.viewers = Some(v)),
                Action::Event(t) => {
                    log(&format!("chat: {t}"));
                    crate::scene::with_overlay(|o| o.chat_event = Some((t, now)));
                }
            }
        }
        let poll = brain.poll.as_ref().map(|p| crate::scene::Poll {
            question: "What next?".into(),
            options: p
                .options
                .iter()
                .zip(&p.votes)
                .map(|(&r, &v)| (CARTA[r].0.to_string(), v))
                .collect(),
            ends: p.ends,
        });
        crate::scene::with_overlay(|o| o.poll = poll);
        // the texts for YouTube, written again when what they say changes
        let snap = *snapshot.lock().map_err(|e| e.to_string())?;
        vis.update(&snap, now);
        let i = (snap.recipe as usize).min(CARTA.len() - 1);
        if let Some(t) = live_since {
            chapters.note(t.elapsed().as_secs_f64(), CARTA[i].0);
        }
        let key = match snap.scale {
            Some(sc) => format!("{} {}", knt_engine::note_name(snap.root), sc.name()),
            None => format!(
                "{} {}",
                knt_engine::note_name(CARTA[i].1.root),
                CARTA[i].1.scales[0].name()
            ),
        };
        let p = crate::promo::write(
            &crate::promo::Playing {
                recipe: i,
                name: CARTA[i].0.into(),
                about: CARTA[i].2.into(),
                key: key.clone(),
                seed,
                minutes: None,
            },
            cfg.stream.lang,
            &chapters,
        );
        let text = format!(
            "TITLE\n{}\n\nDESCRIPTION\n{}\n\nPINNED COMMENT\n{}\n\nTAGS\n{}\n",
            p.title, p.description, p.pinned, p.tags
        );
        if text != written {
            if std::fs::write(&texts_path, &text).is_ok() && written.is_empty() {
                log(&format!(
                    "the texts for YouTube are in {}",
                    texts_path.display()
                ));
            }
            written = text;
            texts = serde_json::json!({
                "title": p.title, "description": p.description, "pinned": p.pinned,
                "hashtags": p.hashtags.join(" "), "tags": p.tags,
            });
        }
        // what the page shows: twice a second; a picture every five seconds
        if last_state_at.elapsed() > Duration::from_millis(500) {
            last_state_at = Instant::now();
            let ov = crate::scene::overlay_now();
            let st = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "state": state_line,
                "live": stream.as_ref().is_some_and(|s| s.status().state == State::Live),
                "streaming": stream.is_some(),
                "manual": manual,
                "has_key": has_key,
                "recipe": i,
                "recipes": CARTA.iter().map(|c| c.0).collect::<Vec<_>>(),
                "clock": clock,
                "seed": seed,
                "key": key,
                "section": snap.section,
                "ambience": params.ambience.name(),
                "level": params.ambience_level,
                "lofi": params.lofi,
                "binaural": params.binaural,
                "viewers": ov.viewers,
                "uptime": started.elapsed().as_secs(),
                "live_for": live_since.map(|t| t.elapsed().as_secs()),
                "schedule": if cfg.stream.schedule.on {
                    format!("{} – {}", cfg.stream.schedule.start, cfg.stream.schedule.stop)
                } else {
                    String::new()
                },
                "chat": chat.is_some(),
                "texts": texts,
                "log": crate::web::log_lines().lock().map(|l| l.iter().rev().take(30).cloned().collect::<Vec<_>>()).unwrap_or_default(),
            });
            if let Ok(mut s) = shared.state.lock() {
                *s = st;
            }
        }
        if last_frame.is_none_or(|t| t.elapsed() > Duration::from_secs(5)) {
            last_frame = Some(Instant::now());
            let mut cv = crate::canvas::SkiaCanvas::new(640, 360);
            crate::stage::draw(&mut cv, &snap, &vis, None, &crate::scene::overlay_now());
            let mut png = Vec::new();
            if let Some(img) = image::RgbaImage::from_raw(640, 360, cv.rgba().to_vec())
                && img
                    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                    .is_ok()
                && let Ok(mut f) = shared.frame.lock()
            {
                *f = png;
            }
        }
    }
}
