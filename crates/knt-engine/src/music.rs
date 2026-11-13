//! Scales, chords and voice leading: the harmony the director chooses and the layers play.

use serde::{Deserialize, Serialize};

/// The scales the director composes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    Major,
    Lydian,
    Mixolydian,
    Dorian,
    Minor,
    Phrygian,
    /// The minor pentatonic: no half steps, nothing can clash.
    PentaMinor,
    PentaMajor,
}

impl Scale {
    pub const ALL: [Scale; 8] = [
        Scale::Major,
        Scale::Lydian,
        Scale::Mixolydian,
        Scale::Dorian,
        Scale::Minor,
        Scale::Phrygian,
        Scale::PentaMinor,
        Scale::PentaMajor,
    ];

    /// The semitones of the scale's degrees from its root.
    pub fn steps(self) -> &'static [u8] {
        match self {
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::PentaMinor => &[0, 3, 5, 7, 10],
            Scale::PentaMajor => &[0, 2, 4, 7, 9],
        }
    }

    /// The name the UI and the stage show.
    pub fn name(self) -> &'static str {
        match self {
            Scale::Major => "major",
            Scale::Lydian => "Lydian",
            Scale::Mixolydian => "Mixolydian",
            Scale::Dorian => "Dorian",
            Scale::Minor => "minor",
            Scale::Phrygian => "Phrygian",
            Scale::PentaMinor => "minor pentatonic",
            Scale::PentaMajor => "major pentatonic",
        }
    }

    /// Whether its third is minor (for the chord names).
    pub fn is_minor(self) -> bool {
        matches!(
            self,
            Scale::Dorian | Scale::Minor | Scale::Phrygian | Scale::PentaMinor
        )
    }

    /// The MIDI note of scale degree `d` (any integer, octaves wrap) above `root`.
    pub fn note(self, root: i32, d: i32) -> i32 {
        let s = self.steps();
        let n = s.len() as i32;
        let oct = d.div_euclid(n);
        root + oct * 12 + s[d.rem_euclid(n) as usize] as i32
    }

    /// Whether the pitch class of `note` is in the scale on `root`.
    pub fn contains(self, root: i32, note: i32) -> bool {
        let pc = (note - root).rem_euclid(12) as u8;
        self.steps().contains(&pc)
    }
}

/// Note names, with sharps.
pub const NOTE_NAMES: [&str; 12] = [
    "C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B",
];

pub fn note_name(n: i32) -> &'static str {
    NOTE_NAMES[n.rem_euclid(12) as usize]
}

/// The most notes a chord holds.
pub const MAX_CHORD: usize = 6;

/// A chord: up to [`MAX_CHORD`] MIDI notes, low to high.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Chord {
    pub notes: [i32; MAX_CHORD],
    pub len: u8,
    /// The scale degree it was built on (for the director's progressions).
    pub degree: i8,
}

impl Chord {
    pub fn notes(&self) -> &[i32] {
        &self.notes[..self.len as usize]
    }

    pub fn from_notes(v: &[i32], degree: i8) -> Self {
        let mut c = Chord {
            degree,
            ..Chord::default()
        };
        let mut sorted = [0i32; MAX_CHORD];
        let n = v.len().min(MAX_CHORD);
        sorted[..n].copy_from_slice(&v[..n]);
        sorted[..n].sort_unstable();
        c.notes = sorted;
        c.len = n as u8;
        c
    }

    /// The lowest note.
    pub fn bass(&self) -> Option<i32> {
        self.notes().first().copied()
    }

    /// A stacked chord on degree `d` of `scale` at `root`: the root, third and fifth of the
    /// degree, plus `extra` color notes (the 9th, then the 7th), voiced around `center`.
    pub fn build(scale: Scale, root: i32, d: i32, extra: usize, center: i32) -> Self {
        let mut v = [0i32; MAX_CHORD];
        v[0] = scale.note(root, d);
        v[1] = scale.note(root, d + 2);
        v[2] = scale.note(root, d + 4);
        let mut n = 3;
        for c in [d + 8, d + 6, d + 3].iter().take(extra.min(3)) {
            v[n] = scale.note(root, *c);
            n += 1;
        }
        Chord::voiced(
            &mut v[..n],
            center,
            6,
            d.rem_euclid(scale.steps().len() as i32) as i8,
        )
    }

    /// Each note of `v` to the octave within `spread` of `center`, sorted, without repeats.
    /// (No allocation: the director calls this in the audio thread.)
    fn voiced(v: &mut [i32], center: i32, spread: i32, degree: i8) -> Chord {
        for n in v.iter_mut() {
            while *n < center - spread {
                *n += 12;
            }
            while *n > center + spread {
                *n -= 12;
            }
        }
        v.sort_unstable();
        let mut c = Chord {
            degree,
            ..Chord::default()
        };
        for &n in v.iter() {
            if c.len as usize == MAX_CHORD {
                break;
            }
            if c.len == 0 || c.notes[c.len as usize - 1] != n {
                c.notes[c.len as usize] = n;
                c.len += 1;
            }
        }
        c
    }

    /// `self` moved note by note to the octaves nearest `prev`'s notes (smooth voice leading):
    /// each note goes to the octave closest to the previous chord's middle.
    pub fn led_from(&self, prev: &Chord) -> Chord {
        let Some(&lo) = prev.notes().first() else {
            return *self;
        };
        let hi = *prev.notes().last().unwrap();
        let mut v = self.notes;
        Chord::voiced(&mut v[..self.len as usize], (lo + hi) / 2, 7, self.degree)
    }

    /// A short name: the root of the degree and "m" when its third is minor ("Dm", "F").
    pub fn name(&self, scale: Scale, root: i32) -> String {
        let d = self.degree as i32;
        let r = scale.note(root, d);
        let third = scale.note(root, d + 2) - r;
        let m = if third.rem_euclid(12) == 3 { "m" } else { "" };
        format!("{}{m}", note_name(r))
    }
}

/// A chord from notes someone plays: their pitch classes, voiced around `center`.
pub fn chord_from_played(played: &[i32], center: i32) -> Option<Chord> {
    if played.is_empty() {
        return None;
    }
    let mut v = [0i32; 16];
    let n = played.len().min(16);
    v[..n].copy_from_slice(&played[..n]);
    Some(Chord::voiced(&mut v[..n], center, 6, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_and_degrees() {
        // C major: degree 0 = 60, degree 7 = 72, degree -1 = 59
        assert_eq!(Scale::Major.note(60, 0), 60);
        assert_eq!(Scale::Major.note(60, 7), 72);
        assert_eq!(Scale::Major.note(60, -1), 59);
        assert_eq!(Scale::PentaMinor.note(57, 5), 69);
        assert!(Scale::Minor.contains(57, 60));
        assert!(!Scale::Minor.contains(57, 61));
    }

    #[test]
    fn chords() {
        // A minor, degree 0 with a 9th: A C E B
        let c = Chord::build(Scale::Minor, 57, 0, 1, 60);
        let pcs: Vec<i32> = c.notes().iter().map(|n| n.rem_euclid(12)).collect();
        for pc in [9, 0, 4, 11] {
            assert!(pcs.contains(&pc), "{pcs:?}");
        }
        assert!(c.notes().iter().all(|&n| (54..=66).contains(&n)));
        assert_eq!(c.name(Scale::Minor, 57), "Am");
        let f = Chord::build(Scale::Minor, 57, 5, 0, 60);
        assert_eq!(f.name(Scale::Minor, 57), "F");
        let led = f.led_from(&c);
        assert!(
            led.notes().iter().all(|&n| (53..=67).contains(&n)),
            "{led:?}"
        );
        let p = chord_from_played(&[48, 52, 79], 60).unwrap();
        assert_eq!(p.notes(), &[55, 60, 64]);
    }
}
