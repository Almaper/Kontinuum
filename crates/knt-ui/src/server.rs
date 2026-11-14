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

/// The server's settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub music: Music,
    pub stream: crate::stream::Config,
    pub stage: crate::scene::Options,
    pub chat: crate::chat::Config,
}

/// The settings file's place when none is given.
pub fn default_path() -> PathBuf {
    crate::scene::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("server.toml")
}

fn log(s: &str) {
    println!("{} {s}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
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

/// Writes a settings file to fill in, with every option.
pub fn write_template(path: &Path) -> Result<(), String> {
    let s = Settings::default();
    let body = toml::to_string_pretty(&s).map_err(|e| e.to_string())?;
    let head = "# Kontinuum's server settings. Paste your stream key in [stream] key, then restart:\n\
                #   sudo systemctl restart kontinuum\n\
                # [music] recipe: a recipe of the Menu (Early Mist, Deep Focus, Cathedral, Copper Rain,\n\
                #   Wet Forest, Orbit, Dusk, Drift), or \"clock\" to follow the time of day.\n\
                # [stream] schedule: on = true streams only between start and stop (local time).\n\
                # [chat] api_key and video: the YouTube chat (votes, welcomes, viewers).\n\n";
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, format!("{head}{body}")).map_err(|e| format!("{}: {e}", path.display()))
}

/// Runs until killed.
pub fn run(path: PathBuf) -> Result<(), String> {
    if !path.exists() {
        write_template(&path)?;
        return Err(format!(
            "No settings yet: wrote {}. Paste your stream key in it and start again.",
            path.display()
        ));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let cfg: Settings = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let texts_path = path.with_file_name("youtube-texts.txt");

    crate::scene::with_overlay(|o| o.opts = cfg.stage);
    let clock = cfg.music.recipe.trim().eq_ignore_ascii_case("clock");
    let mut recipe = if clock {
        crate::app::recipe_for_hour(crate::scene::local_time().0)
    } else {
        recipe_index(&cfg.music.recipe)
            .ok_or_else(|| format!("No recipe {:?} in the Menu", cfg.music.recipe))?
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
    let shared: Arc<Mutex<Snapshot>> = player.shared_snapshot();
    let _ = player.send(Command::Play(true));
    log(&format!(
        "Kontinuum {} server: {} (seed {seed}), {}",
        env!("CARGO_PKG_VERSION"),
        CARTA[recipe].0,
        if clock {
            "following the clock"
        } else {
            "one recipe"
        }
    ));

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
    let mut last_state = String::new();
    let mut chat = None;
    let mut brain = crate::chat::Brain::default();
    let mut seeded = Instant::now();
    let mut written = String::new();
    let mut chapters = crate::promo::Chapters::default();
    let mut live_since: Option<Instant> = None;
    if !cfg.chat.api_key.trim().is_empty() {
        match crate::chat::Reader::start(&cfg.chat) {
            Ok(r) => chat = Some(r),
            Err(e) => log(&format!("chat: {e}")),
        }
    }
    loop {
        std::thread::sleep(Duration::from_millis(100));
        player.collect_garbage();
        let now = crate::scene::clock();
        let (hour, _) = crate::scene::local_time();
        // the schedule
        let want = !cfg.stream.schedule.on || cfg.stream.schedule.within(hour);
        if want && stream.is_none() {
            if let Some(t) = tap.take() {
                log(&format!("going live: {}", CARTA[recipe].0));
                chapters = crate::promo::Chapters::default();
                live_since = Some(Instant::now());
                stream = Some(Stream::start(
                    cfg.stream.clone(),
                    t,
                    flag.clone(),
                    shared.clone(),
                ));
            }
        } else if !want && stream.is_some() {
            log("the schedule ends: stopping");
            tap = stream.take().and_then(|s| s.stop());
            live_since = None;
        }
        if let Some(s) = &stream {
            let st = s.status();
            let line = match &st.state {
                State::Live => format!("live: {}", st.target),
                State::Retrying(e) => format!("reconnecting: {e}"),
                other => format!("{other:?}"),
            };
            if line != last_state {
                log(&line);
                last_state = line;
            }
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
        let snap = *shared.lock().map_err(|e| e.to_string())?;
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
                key,
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
        }
        if quit.load(Ordering::SeqCst) {
            return Ok(());
        }
    }
}
