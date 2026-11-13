//! MIDI input: a keyboard plays the chosen layer, channel 10 (or a pad controller's drum notes)
//! plays the percussion, and knobs and faders move Kontinuum's controls once learned.
//!
//! Notes go from the MIDI thread straight to the audio callback's MIDI queue (no UI frame in
//! between); control changes go to the UI thread, which applies them to the settings.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use knt_engine::{Command, Layer, Params};
use midir::{MidiInput, MidiInputConnection};
use serde::{Deserialize, Serialize};

/// What a learned controller moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Evolution,
    Pace,
    Brightness,
    Density,
    Space,
    Echo,
    Warmth,
    Volume,
    Level(Layer),
}

impl Target {
    pub const ALL: [Target; 15] = [
        Target::Volume,
        Target::Evolution,
        Target::Pace,
        Target::Brightness,
        Target::Density,
        Target::Space,
        Target::Echo,
        Target::Warmth,
        Target::Level(Layer::Drone),
        Target::Level(Layer::Pad),
        Target::Level(Layer::Texture),
        Target::Level(Layer::Bells),
        Target::Level(Layer::Perc),
        Target::Level(Layer::Choir),
        Target::Level(Layer::Modular),
    ];

    pub fn name(self) -> String {
        match self {
            Target::Evolution => "Evolution".into(),
            Target::Pace => "Pace".into(),
            Target::Brightness => "Brightness".into(),
            Target::Density => "Density".into(),
            Target::Space => "Space".into(),
            Target::Echo => "Echo".into(),
            Target::Warmth => "Warmth".into(),
            Target::Volume => "Volume".into(),
            Target::Level(l) => format!("{} level", l.name()),
        }
    }

    /// Sets the target from a controller value 0..127.
    pub fn apply(self, p: &mut Params, v: u8) {
        let x = v.min(127) as f32 / 127.0;
        let bias = x * 2.0 - 1.0;
        match self {
            Target::Evolution => p.evolution = x,
            Target::Pace => p.pace_bias = bias,
            Target::Brightness => p.brightness_bias = bias,
            Target::Density => p.density_bias = bias,
            Target::Space => p.space_bias = bias,
            Target::Echo => p.echo_bias = bias,
            Target::Warmth => p.warmth = x,
            Target::Volume => p.volume = x,
            Target::Level(l) => *p.levels.get_mut(l) = x,
        }
    }
}

/// A controller: its channel (0..15) and number.
pub type Cc = (u8, u8);

/// The learned controllers, with the usual defaults: the mod wheel moves the brightness, CC 7
/// the volume, CC 74 the brightness too, CC 91 the space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Map {
    pub cc: BTreeMap<String, Target>,
}

impl Default for Map {
    fn default() -> Self {
        let mut m = Map {
            cc: BTreeMap::new(),
        };
        for ch in 0..16 {
            for (cc, t) in [
                (1, Target::Brightness),
                (7, Target::Volume),
                (74, Target::Brightness),
                (91, Target::Space),
            ] {
                m.cc.insert(Self::key((ch, cc)), t);
            }
        }
        m
    }
}

impl Map {
    fn key(c: Cc) -> String {
        format!("{}:{}", c.0 + 1, c.1)
    }

    pub fn get(&self, c: Cc) -> Option<Target> {
        self.cc.get(&Self::key(c)).copied()
    }

    pub fn set(&mut self, c: Cc, t: Target) {
        // one controller per target: learning replaces the old ones
        self.cc.retain(|_, v| *v != t);
        self.cc.insert(Self::key(c), t);
    }

    /// The controllers of a target ("ch:cc").
    /// A controller on all sixteen channels shows as "CC n".
    pub fn of(&self, t: Target) -> Vec<String> {
        let keys: Vec<&String> = self
            .cc
            .iter()
            .filter(|(_, v)| **v == t)
            .map(|(k, _)| k)
            .collect();
        let mut out: Vec<String> = Vec::new();
        for k in &keys {
            let Some((_, cc)) = k.split_once(':') else {
                continue;
            };
            let all = (1..=16).all(|ch| keys.iter().any(|k| **k == format!("{ch}:{cc}")));
            let label = if all {
                format!("CC {cc}")
            } else {
                (*k).clone()
            };
            if !out.contains(&label) {
                out.push(label);
            }
        }
        out
    }

    fn path() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(std::path::PathBuf::from))
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
            })?;
        Some(base.join("Kontinuum").join("midi.toml"))
    }

    pub fn load() -> Map {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::path().ok_or("no configuration folder")?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, toml::to_string(self).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{}: {e}", p.display()))
    }
}

/// A MIDI message the input understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    NoteOn { ch: u8, note: u8, vel: u8 },
    NoteOff { ch: u8, note: u8 },
    Cc { ch: u8, cc: u8, value: u8 },
}

/// Parses one MIDI message (running status is not used by USB devices).
pub fn parse(b: &[u8]) -> Option<Msg> {
    let (&status, data) = b.split_first()?;
    let ch = status & 0x0F;
    match (status & 0xF0, data) {
        (0x90, [n, 0, ..]) => Some(Msg::NoteOff { ch, note: *n }),
        (0x90, [n, v, ..]) => Some(Msg::NoteOn {
            ch,
            note: *n,
            vel: *v,
        }),
        (0x80, [n, ..]) => Some(Msg::NoteOff { ch, note: *n }),
        (0xB0, [c, v, ..]) => Some(Msg::Cc {
            ch,
            cc: *c,
            value: *v,
        }),
        _ => None,
    }
}

/// The command a note becomes: channel 10 is the drums.
pub fn note_command(m: Msg) -> Option<Command> {
    match m {
        Msg::NoteOn { ch: 9, note, vel } => Some(Command::Drum { note, vel }),
        Msg::NoteOff { ch: 9, .. } => None,
        Msg::NoteOn { note, vel, .. } => Some(Command::NoteOn { note, vel }),
        Msg::NoteOff { note, .. } => Some(Command::NoteOff { note }),
        Msg::Cc { .. } => None,
    }
}

/// The MIDI input: one connected port.
pub struct Midi {
    conn: Option<MidiInputConnection<()>>,
    pub port: Option<String>,
    pub rx: Receiver<Msg>,
    tx: Sender<Msg>,
    /// Where the notes go (the player's MIDI queue), handed to each connection.
    notes: std::sync::Arc<std::sync::Mutex<Option<rtrb::Producer<Command>>>>,
    pub error: Option<String>,
}

impl Midi {
    pub fn new(notes: Option<rtrb::Producer<Command>>) -> Self {
        let (tx, rx) = channel();
        Midi {
            conn: None,
            port: None,
            rx,
            tx,
            notes: std::sync::Arc::new(std::sync::Mutex::new(notes)),
            error: None,
        }
    }

    /// The input ports' names.
    pub fn ports() -> Vec<String> {
        let Ok(m) = MidiInput::new("Kontinuum") else {
            return Vec::new();
        };
        m.ports()
            .iter()
            .filter_map(|p| m.port_name(p).ok())
            .filter(|n| !n.contains("Midi Through"))
            .collect()
    }

    /// Connects to the port named `name` (the old connection closes).
    pub fn connect(&mut self, name: &str) {
        self.conn = None;
        self.port = None;
        let mut input = match MidiInput::new("Kontinuum") {
            Ok(m) => m,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        input.ignore(midir::Ignore::All);
        let Some(port) = input
            .ports()
            .into_iter()
            .find(|p| input.port_name(p).ok().as_deref() == Some(name))
        else {
            self.error = Some(format!("no MIDI port named {name}"));
            return;
        };
        let tx = self.tx.clone();
        let notes = self.notes.clone();
        match input.connect(
            &port,
            "kontinuum-in",
            move |_t, bytes, _| {
                let Some(m) = parse(bytes) else { return };
                if let Some(c) = note_command(m)
                    && let Ok(mut q) = notes.lock()
                    && let Some(q) = q.as_mut()
                {
                    let _ = q.push(c);
                }
                let _ = tx.send(m);
            },
            (),
        ) {
            Ok(c) => {
                self.conn = Some(c);
                self.port = Some(name.to_string());
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Connects to the first port, when there is one and nothing is connected.
    pub fn connect_first(&mut self) {
        if self.conn.is_none()
            && let Some(p) = Self::ports().first()
        {
            let p = p.clone();
            self.connect(&p);
        }
    }

    pub fn disconnect(&mut self) {
        self.conn = None;
        self.port = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_and_commands() {
        assert_eq!(
            parse(&[0x90, 60, 100]),
            Some(Msg::NoteOn {
                ch: 0,
                note: 60,
                vel: 100
            })
        );
        assert_eq!(
            parse(&[0x93, 60, 0]),
            Some(Msg::NoteOff { ch: 3, note: 60 })
        );
        assert_eq!(
            parse(&[0x80, 61, 30]),
            Some(Msg::NoteOff { ch: 0, note: 61 })
        );
        assert_eq!(
            parse(&[0xB1, 74, 64]),
            Some(Msg::Cc {
                ch: 1,
                cc: 74,
                value: 64
            })
        );
        assert_eq!(parse(&[0xF8]), None);
        assert!(matches!(
            note_command(Msg::NoteOn {
                ch: 9,
                note: 36,
                vel: 90
            }),
            Some(Command::Drum { note: 36, vel: 90 })
        ));
        assert!(matches!(
            note_command(Msg::NoteOff { ch: 0, note: 60 }),
            Some(Command::NoteOff { note: 60 })
        ));
    }

    #[test]
    fn learning() {
        let mut m = Map::default();
        assert_eq!(m.get((0, 1)), Some(Target::Brightness));
        m.set((2, 20), Target::Level(Layer::Pad));
        assert_eq!(m.get((2, 20)), Some(Target::Level(Layer::Pad)));
        m.set((2, 21), Target::Level(Layer::Pad));
        assert_eq!(m.get((2, 20)), None);
        assert_eq!(m.of(Target::Level(Layer::Pad)), ["3:21"]);
        let text = toml::to_string(&m).unwrap();
        assert_eq!(toml::from_str::<Map>(&text).unwrap(), m);
        let mut p = Params::default();
        Target::Level(Layer::Choir).apply(&mut p, 127);
        Target::Pace.apply(&mut p, 0);
        assert_eq!(p.levels.choir, 1.0);
        assert_eq!(p.pace_bias, -1.0);
    }
}

#[cfg(test)]
mod labels {
    use super::*;

    #[test]
    fn a_controller_on_every_channel_is_one_label() {
        let m = Map::default();
        let mut b = m.of(Target::Brightness);
        b.sort();
        assert_eq!(b, ["CC 1", "CC 74"]);
    }
}
