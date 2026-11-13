//! The Menu: the recipes of the music. A recipe sets the key and the scales, which layers sound
//! and how much, how often the percussion and the choir come in, how fast things change, the
//! space, and the colors of the stage.

use serde::{Deserialize, Serialize};

use crate::music::Scale;

/// How loud each layer is, 0..1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Levels {
    pub drone: f32,
    pub pad: f32,
    pub texture: f32,
    pub bells: f32,
    pub perc: f32,
    pub choir: f32,
    pub modular: f32,
}

impl Default for Levels {
    fn default() -> Self {
        Levels {
            drone: 0.7,
            pad: 0.7,
            texture: 0.4,
            bells: 0.4,
            perc: 0.5,
            choir: 0.4,
            modular: 0.5,
        }
    }
}

impl Levels {
    pub fn get(&self, l: crate::Layer) -> f32 {
        use crate::Layer::*;
        match l {
            Drone => self.drone,
            Pad => self.pad,
            Texture => self.texture,
            Bells => self.bells,
            Perc => self.perc,
            Choir => self.choir,
            Modular => self.modular,
        }
    }

    pub fn get_mut(&mut self, l: crate::Layer) -> &mut f32 {
        use crate::Layer::*;
        match l {
            Drone => &mut self.drone,
            Pad => &mut self.pad,
            Texture => &mut self.texture,
            Bells => &mut self.bells,
            Perc => &mut self.perc,
            Choir => &mut self.choir,
            Modular => &mut self.modular,
        }
    }
}

/// A recipe of the Menu.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recipe {
    /// Which recipe (the built-in ones are 0..[`CARTA`].len(); the UI's own above 1000).
    pub id: u16,
    /// The key's root, a MIDI note in the octave of middle C (57 = A).
    pub root: i32,
    /// The scales the director picks from.
    pub scales: [Scale; 3],
    /// How likely a new section changes key, 0..1.
    pub modulation: f32,
    /// How fast the music changes, 0 (glacial) .. 1 (restless).
    pub pace: f32,
    pub levels: Levels,
    /// 0 (dark) .. 1 (bright).
    pub brightness: f32,
    /// How many notes (bells, chord colors), 0..1.
    pub density: f32,
    /// How likely a section has a pulse (the occasional percussion), 0..1.
    pub pulse: f32,
    /// The pulse's tempo.
    pub bpm: f32,
    /// How likely a section has the choir, 0..1.
    pub choir: f32,
    /// How likely a section has the modular synth, 0..1.
    pub modular: f32,
    /// The reverb's size and the delay's amount, 0..1.
    pub space: f32,
    pub echo: f32,
    /// The stage's colors: background, main, accent (RGB).
    pub palette: [[u8; 3]; 3],
    /// Drift: every few sections the director moves to another recipe of the Menu.
    pub drift: bool,
}

impl Default for Recipe {
    fn default() -> Self {
        CARTA[0].1
    }
}

const fn levels(drone: f32, pad: f32, texture: f32, bells: f32, perc: f32, choir: f32) -> Levels {
    Levels {
        drone,
        pad,
        texture,
        bells,
        perc,
        choir,
        modular: 0.5,
    }
}

/// The built-in Menu: (name, recipe, description).
pub const CARTA: [(&str, Recipe, &str); 8] = [
    (
        "Early Mist",
        Recipe {
            id: 0,
            root: 50,
            scales: [Scale::Dorian, Scale::PentaMinor, Scale::Minor],
            modulation: 0.15,
            pace: 0.15,
            levels: levels(0.8, 0.6, 0.45, 0.12, 0.3, 0.35),
            brightness: 0.25,
            density: 0.25,
            pulse: 0.05,
            bpm: 56.0,
            choir: 0.3,
            modular: 0.15,
            space: 0.85,
            echo: 0.3,
            palette: [[0x1c, 0x1a, 0x2a], [0x9a, 0x8f, 0xb8], [0xe8, 0xb8, 0x7a]],
            drift: false,
        },
        "Low, slow and almost without percussion. For sleep.",
    ),
    (
        "Deep Focus",
        Recipe {
            id: 1,
            root: 53,
            scales: [Scale::Lydian, Scale::Major, Scale::PentaMajor],
            modulation: 0.1,
            pace: 0.2,
            levels: levels(0.6, 0.65, 0.3, 0.15, 0.6, 0.15),
            brightness: 0.5,
            density: 0.35,
            pulse: 0.55,
            bpm: 72.0,
            choir: 0.15,
            modular: 0.6,
            space: 0.6,
            echo: 0.45,
            palette: [[0x17, 0x22, 0x24], [0x7f, 0xb8, 0xb0], [0xf2, 0xc0, 0x6b]],
            drift: false,
        },
        "A soft pulse now and then, and very slow changes. For focus.",
    ),
    (
        "Cathedral",
        Recipe {
            id: 2,
            root: 55,
            scales: [Scale::Minor, Scale::Dorian, Scale::Phrygian],
            modulation: 0.25,
            pace: 0.3,
            levels: levels(0.7, 0.6, 0.2, 0.12, 0.2, 0.8),
            brightness: 0.4,
            density: 0.3,
            pulse: 0.1,
            bpm: 60.0,
            choir: 0.9,
            modular: 0.1,
            space: 1.0,
            echo: 0.25,
            palette: [[0x21, 0x18, 0x14], [0xd9, 0xb8, 0x82], [0xf6, 0xe7, 0xc4]],
            drift: false,
        },
        "Distant choirs and a huge, enveloping reverb.",
    ),
    (
        "Copper Rain",
        Recipe {
            id: 3,
            root: 52,
            scales: [Scale::PentaMajor, Scale::Mixolydian, Scale::Major],
            modulation: 0.2,
            pace: 0.35,
            levels: levels(0.55, 0.6, 0.65, 0.3, 0.45, 0.25),
            brightness: 0.55,
            density: 0.6,
            pulse: 0.3,
            bpm: 66.0,
            choir: 0.25,
            modular: 0.35,
            space: 0.7,
            echo: 0.6,
            palette: [[0x26, 0x18, 0x10], [0xd0, 0x7a, 0x3c], [0xf4, 0xd0, 0x8a]],
            drift: false,
        },
        "Drops, bells and warm tones. To relax.",
    ),
    (
        "Wet Forest",
        Recipe {
            id: 4,
            root: 48,
            scales: [Scale::Dorian, Scale::Mixolydian, Scale::PentaMinor],
            modulation: 0.2,
            pace: 0.3,
            levels: levels(0.6, 0.5, 0.75, 0.12, 0.6, 0.2),
            brightness: 0.4,
            density: 0.45,
            pulse: 0.4,
            bpm: 64.0,
            choir: 0.2,
            modular: 0.3,
            space: 0.65,
            echo: 0.35,
            palette: [[0x14, 0x1e, 0x16], [0x7a, 0x9a, 0x5c], [0xd8, 0xc0, 0x7e]],
            drift: false,
        },
        "Drops, hand drums and mossy tones.",
    ),
    (
        "Orbit",
        Recipe {
            id: 5,
            root: 54,
            scales: [Scale::Lydian, Scale::PentaMajor, Scale::Major],
            modulation: 0.35,
            pace: 0.4,
            levels: levels(0.6, 0.5, 0.35, 0.45, 0.15, 0.2),
            brightness: 0.8,
            density: 0.65,
            pulse: 0.1,
            bpm: 70.0,
            choir: 0.2,
            modular: 0.5,
            space: 0.9,
            echo: 0.75,
            palette: [[0x0e, 0x12, 0x22], [0x6c, 0x8c, 0xd8], [0xe0, 0xe8, 0xff]],
            drift: false,
        },
        "Cold and spacious, with bright bells.",
    ),
    (
        "Dusk",
        Recipe {
            id: 6,
            root: 51,
            scales: [Scale::Minor, Scale::Dorian, Scale::Major],
            modulation: 0.4,
            pace: 0.45,
            levels: levels(0.6, 0.8, 0.3, 0.15, 0.35, 0.45),
            brightness: 0.45,
            density: 0.5,
            pulse: 0.25,
            bpm: 68.0,
            choir: 0.5,
            modular: 0.3,
            space: 0.75,
            echo: 0.4,
            palette: [[0x2a, 0x16, 0x1a], [0xc8, 0x6a, 0x5a], [0xf0, 0xb0, 0x6a]],
            drift: false,
        },
        "Melancholic, with richer harmony.",
    ),
    (
        "Drift",
        Recipe {
            id: 7,
            root: 50,
            scales: [Scale::Dorian, Scale::Lydian, Scale::Minor],
            modulation: 0.3,
            pace: 0.35,
            levels: levels(0.65, 0.65, 0.45, 0.2, 0.4, 0.4),
            brightness: 0.5,
            density: 0.45,
            pulse: 0.3,
            bpm: 66.0,
            choir: 0.4,
            modular: 0.4,
            space: 0.75,
            echo: 0.45,
            palette: [[0x22, 0x1a, 0x14], [0xc8, 0x8a, 0x4a], [0xef, 0xe3, 0xc8]],
            drift: true,
        },
        "Changes mood every few minutes, through the whole Menu. For long streams.",
    ),
];

/// The name of a built-in recipe (None: a recipe of the user's).
pub fn name(id: u16) -> Option<&'static str> {
    CARTA.get(id as usize).map(|c| c.0)
}
