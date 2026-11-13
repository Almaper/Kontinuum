//! What the stage shows besides the music: its options (what to draw), and the overlay the
//! window and the stream share (the audience, the chat's greetings and polls, the focus timer,
//! the notes played on a MIDI keyboard).

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// What the stage draws. Saved in the configuration folder (stage.toml).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// The sky follows the local time: dawn, day, dusk and night, with the sun and the moon.
    pub day_night: bool,
    /// A lake under the mountains, with reflections.
    pub lake: bool,
    /// A slow camera: the picture drifts and breathes.
    pub camera: bool,
    /// Each recipe's landscape (a forest, a planet, arches of light, a city far away…).
    pub scenery: bool,
    /// Rain in the rainy recipes, and the season's weather (snow, leaves, petals).
    pub weather: bool,
    /// The season follows the date.
    pub seasons: bool,
    /// The stage seen through a window: rain on the glass, a cup and a lamp on the sill.
    pub window: bool,
    /// Lumo, the little lantern spirit on the hill.
    pub mascot: bool,
    /// The seed's own constellation in the sky.
    pub constellation: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            day_night: true,
            lake: true,
            camera: true,
            scenery: true,
            weather: true,
            seasons: true,
            window: false,
            mascot: true,
            constellation: true,
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("Kontinuum"))
}

/// Loads a settings file of the configuration folder (the default if there is none).
pub fn load<T: serde::de::DeserializeOwned + Default>(name: &str) -> T {
    config_dir()
        .and_then(|d| std::fs::read_to_string(d.join(name)).ok())
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save<T: Serialize>(name: &str, v: &T) -> Result<(), String> {
    let d = config_dir().ok_or("no configuration folder")?;
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let p = d.join(name);
    std::fs::write(&p, toml::to_string(v).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", p.display()))
}

/// Seconds since the program started: the one clock of the overlay's times.
pub fn clock() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// The local time of day in hours (0..24), and the month (1..12).
pub fn local_time() -> (f32, u32) {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    (
        now.hour() as f32 + now.minute() as f32 / 60.0 + now.second() as f32 / 3600.0,
        now.month(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    pub fn of_month(m: u32) -> Season {
        match m {
            3..=5 => Season::Spring,
            6..=8 => Season::Summer,
            9..=11 => Season::Autumn,
            _ => Season::Winter,
        }
    }
}

/// A poll on the stage.
#[derive(Debug, Clone, Default)]
pub struct Poll {
    pub question: String,
    /// (option, votes).
    pub options: Vec<(String, u32)>,
    /// When it ends ([`clock`]).
    pub ends: f64,
}

/// The focus timer (Pomodoro).
#[derive(Debug, Clone, Default)]
pub struct Focus {
    /// "FOCUS" or "BREAK".
    pub label: String,
    pub left: f64,
    pub total: f64,
}

/// What the stage shows of the audience and the player, shared by the window and the stream.
#[derive(Debug, Clone, Default)]
pub struct Overlay {
    pub opts: Options,
    /// Who is watching (from YouTube), if known.
    pub viewers: Option<u32>,
    /// The chat's newcomers: (name, when).
    pub greets: Vec<(String, f64)>,
    /// A milestone: (text, when).
    pub milestone: Option<(String, f64)>,
    pub poll: Option<Poll>,
    pub focus: Option<Focus>,
    /// The notes played on the MIDI keyboard: (note, when).
    pub notes: Vec<(i32, f64)>,
    /// What the chat last did: (text, when), e.g. "Ana chose Orbit".
    pub chat_event: Option<(String, f64)>,
}

impl Overlay {
    /// Forgets what is too old to show.
    pub fn prune(&mut self, now: f64) {
        self.greets.retain(|g| now - g.1 < 10.0);
        self.notes.retain(|n| now - n.1 < 4.0);
        if self.milestone.as_ref().is_some_and(|m| now - m.1 > 10.0) {
            self.milestone = None;
        }
        if self.chat_event.as_ref().is_some_and(|m| now - m.1 > 10.0) {
            self.chat_event = None;
        }
    }
}

/// The overlay the window and the stream share.
pub fn shared() -> &'static std::sync::Mutex<Overlay> {
    static OVERLAY: OnceLock<std::sync::Mutex<Overlay>> = OnceLock::new();
    OVERLAY.get_or_init(|| {
        std::sync::Mutex::new(Overlay {
            opts: load("stage.toml"),
            ..Overlay::default()
        })
    })
}

/// A copy of the overlay to draw, without what is too old.
pub fn overlay_now() -> Overlay {
    let now = clock();
    shared()
        .lock()
        .map(|mut o| {
            o.prune(now);
            o.clone()
        })
        .unwrap_or_default()
}

/// Changes the overlay.
pub fn with_overlay(f: impl FnOnce(&mut Overlay)) {
    if let Ok(mut o) = shared().lock() {
        f(&mut o);
    }
}
