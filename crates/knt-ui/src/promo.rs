//! The texts for a live stream: the title, the description (with the replay's chapters), the
//! pinned comment, the hashtags and the tags, written from what is playing (the recipe, the
//! key, the seed), in English, Spanish or both.
//!
//! YouTube's rules they follow: a title of at most 100 characters; the three hashtags that
//! YouTube can show by the title come first in the description, and there are few of them in
//! all (more than 60 and YouTube ignores every one); the tags field is at most 500 characters;
//! the replay's chapters start at 00:00, and there are at least three of them.

use serde::{Deserialize, Serialize};

/// Which languages the texts are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Lang {
    English,
    Spanish,
    #[default]
    Both,
}

impl Lang {
    fn en(self) -> bool {
        self != Lang::Spanish
    }
    fn es(self) -> bool {
        self != Lang::English
    }
}

/// What the texts say about a recipe of the Menu.
struct Profile {
    /// The search words a viewer types ("Deep Sleep Music"): the title starts with them.
    lead: &'static str,
    lead_es: &'static str,
    emoji: &'static str,
    /// "Best for" lines: (emoji, English, Spanish).
    best: [(&'static str, &'static str, &'static str); 3],
    /// "soft, sleepy ambient": what it sounds like.
    mood: &'static str,
    mood_es: &'static str,
    /// What it is good for.
    uses: &'static str,
    uses_es: &'static str,
    /// The description of the recipe, in Spanish (the Menu has the English one).
    about_es: &'static str,
    /// The three hashtags to show by the title, then one or two more.
    hashtags: [&'static str; 5],
    /// The Spanish hashtag.
    hashtag_es: &'static str,
    /// Search words for the tags field.
    keywords: &'static [&'static str],
}

const PROFILES: [Profile; 8] = [
    Profile {
        lead: "Deep Sleep Music",
        lead_es: "Música para Dormir",
        emoji: "😴",
        best: [
            ("😴", "Sleep and insomnia", "Dormir e insomnio"),
            ("🛌", "Winding down at night", "Desconectar por la noche"),
            ("🧘", "Rest and relaxation", "Descanso y relajación"),
        ],
        mood: "soft, sleepy ambient",
        mood_es: "ambient suave para dormir",
        uses: "sleep, rest and quiet nights",
        uses_es: "dormir, descansar y noches tranquilas",
        about_es: "Grave, lento y casi sin percusión. Para dormir.",
        hashtags: [
            "#SleepMusic",
            "#AmbientMusic",
            "#CalmMusic",
            "#DeepSleep",
            "#Relaxing",
        ],
        hashtag_es: "#MusicaParaDormir",
        keywords: &[
            "sleep music",
            "ambient for sleep",
            "calm music",
            "deep sleep",
        ],
    },
    Profile {
        lead: "Deep Focus Music",
        lead_es: "Música para Concentrarse",
        emoji: "🧠",
        best: [
            ("📖", "Studying", "Estudiar"),
            ("💻", "Deep work and coding", "Trabajo profundo y programar"),
            ("✍️", "Writing", "Escribir"),
        ],
        mood: "deep focus ambient",
        mood_es: "ambient para concentrarse",
        uses: "study, work and deep focus",
        uses_es: "estudiar, trabajar y concentrarse",
        about_es: "Un pulso suave de vez en cuando y cambios muy lentos. Para concentrarse.",
        hashtags: [
            "#FocusMusic",
            "#StudyMusic",
            "#AmbientMusic",
            "#DeepWork",
            "#Concentration",
        ],
        hashtag_es: "#MusicaParaEstudiar",
        keywords: &[
            "focus music",
            "study music",
            "music for work",
            "deep work",
            "concentration",
        ],
    },
    Profile {
        lead: "Meditation Music",
        lead_es: "Música para Meditar",
        emoji: "🕯️",
        best: [
            ("🧘", "Meditation and yoga", "Meditación y yoga"),
            ("🕯️", "Stillness and prayer", "Calma y recogimiento"),
            ("😴", "Falling asleep", "Quedarse dormido"),
        ],
        mood: "choral drone ambient",
        mood_es: "drone ambient con coros",
        uses: "meditation, yoga and stillness",
        uses_es: "meditar, yoga y calma",
        about_es: "Coros lejanos y una reverb enorme que te envuelve.",
        hashtags: [
            "#MeditationMusic",
            "#DroneMusic",
            "#AmbientMusic",
            "#Meditation",
            "#Choir",
        ],
        hashtag_es: "#MusicaParaMeditar",
        keywords: &[
            "meditation music",
            "drone ambient",
            "ambient choir",
            "yoga music",
        ],
    },
    Profile {
        lead: "Relaxing Rain Ambient",
        lead_es: "Música Relajante de Lluvia",
        emoji: "🌧️",
        best: [
            ("☕", "Cozy evenings", "Tardes tranquilas"),
            ("📚", "Reading", "Leer"),
            ("😌", "Stress relief", "Quitar el estrés"),
        ],
        mood: "warm, rainy ambient",
        mood_es: "ambient cálido de lluvia",
        uses: "relaxing, reading and cozy evenings",
        uses_es: "relajarse, leer y tardes tranquilas",
        about_es: "Gotas, campanas y tonos cálidos. Para relajarse.",
        hashtags: [
            "#RelaxingMusic",
            "#AmbientMusic",
            "#RainyDay",
            "#CozyVibes",
            "#Calm",
        ],
        hashtag_es: "#MusicaRelajante",
        keywords: &[
            "relaxing music",
            "rainy ambient",
            "cozy music",
            "music for reading",
        ],
    },
    Profile {
        lead: "Relaxing Forest Ambient",
        lead_es: "Música Relajante de Bosque",
        emoji: "🌿",
        best: [
            ("🌿", "Unwinding", "Relajarse"),
            ("📚", "Reading", "Leer"),
            ("🧘", "Yoga and stretching", "Yoga y estiramientos"),
        ],
        mood: "earthy forest ambient",
        mood_es: "ambient de bosque",
        uses: "relaxing, reading and slow afternoons",
        uses_es: "relajarse, leer y tardes lentas",
        about_es: "Gotas, tambores de mano y tonos de musgo.",
        hashtags: [
            "#AmbientMusic",
            "#RelaxingMusic",
            "#ForestVibes",
            "#Nature",
            "#Calm",
        ],
        hashtag_es: "#MusicaRelajante",
        keywords: &[
            "forest ambient",
            "relaxing music",
            "nature ambient",
            "calm music",
        ],
    },
    Profile {
        lead: "Space Ambient Music",
        lead_es: "Música Ambient Espacial",
        emoji: "🌌",
        best: [
            ("🌌", "Late nights", "Noches largas"),
            ("💻", "Focus and coding", "Concentrarse y programar"),
            ("😴", "Drifting off", "Dejarse llevar"),
        ],
        mood: "cosmic space ambient",
        mood_es: "ambient espacial",
        uses: "focus, late nights and drifting off",
        uses_es: "concentrarse, trasnochar y dejarse llevar",
        about_es: "Frío y espacioso, con campanas brillantes.",
        hashtags: [
            "#SpaceAmbient",
            "#AmbientMusic",
            "#SpaceMusic",
            "#Cosmic",
            "#FocusMusic",
        ],
        hashtag_es: "#MusicaAmbiental",
        keywords: &[
            "space ambient",
            "space music",
            "cosmic ambient",
            "music for focus",
        ],
    },
    Profile {
        lead: "Evening Ambient Music",
        lead_es: "Música Ambient para el Atardecer",
        emoji: "🌇",
        best: [
            ("🌇", "Winding down", "Desconectar"),
            ("✍️", "Writing", "Escribir"),
            ("🍷", "Quiet evenings", "Tardes tranquilas"),
        ],
        mood: "melancholic evening ambient",
        mood_es: "ambient melancólico de atardecer",
        uses: "winding down, writing and evenings",
        uses_es: "desconectar, escribir y atardeceres",
        about_es: "Melancólico, con una armonía más rica.",
        hashtags: [
            "#AmbientMusic",
            "#ChillMusic",
            "#EveningVibes",
            "#Melancholic",
            "#Calm",
        ],
        hashtag_es: "#MusicaRelajante",
        keywords: &[
            "evening ambient",
            "melancholic music",
            "chill ambient",
            "music for writing",
        ],
    },
    Profile {
        lead: "Ambient Music Mix",
        lead_es: "Música Ambiental",
        emoji: "🌊",
        best: [
            ("💻", "Long work sessions", "Jornadas largas"),
            ("📚", "Reading and studying", "Leer y estudiar"),
            ("😌", "Relaxing", "Relajarse"),
        ],
        mood: "ever-changing ambient",
        mood_es: "ambient que cambia sin parar",
        uses: "long work sessions, reading and rest",
        uses_es: "jornadas largas, leer y descansar",
        about_es: "Cambia de ambiente cada pocos minutos, por toda la carta. Para directos largos.",
        hashtags: [
            "#AmbientMusic",
            "#RelaxingMusic",
            "#FocusMusic",
            "#LongMix",
            "#Calm",
        ],
        hashtag_es: "#MusicaAmbiental",
        keywords: &[
            "ambient mix",
            "long ambient",
            "relaxing music",
            "focus music",
        ],
    },
];

/// The hashtags of every stream, after the recipe's.
const COMMON: [&str; 3] = ["#GenerativeMusic", "#LiveAmbient", "#Kontinuum"];
const COMMON_KEYWORDS: [&str; 8] = [
    "ambient",
    "ambient music",
    "drone",
    "generative music",
    "live ambient",
    "ambient live stream",
    "relaxing music",
    "Kontinuum",
];

const REPO: &str = "https://github.com/Almaper/Kontinuum";

/// What is playing, for the texts.
#[derive(Debug, Clone, Default)]
pub struct Playing {
    /// The recipe's place in the Menu.
    pub recipe: usize,
    pub name: String,
    /// The Menu's description of it.
    pub about: String,
    /// "E major".
    pub key: String,
    pub seed: u64,
    /// The sleep timer, if any.
    pub minutes: Option<u32>,
}

/// The texts of a stream.
#[derive(Debug, Clone, Default)]
pub struct Promo {
    pub title: String,
    pub description: String,
    pub pinned: String,
    /// All the hashtags, in order (the first three are the ones by the title).
    pub hashtags: Vec<String>,
    /// The tags field: comma-separated, at most 500 characters.
    pub tags: String,
}

/// A recipe the stream played, from when (seconds since it started).
#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub at: f64,
    pub name: String,
}

/// Keeps the recipes a stream plays, for the replay's chapters.
#[derive(Debug, Clone, Default)]
pub struct Chapters(pub Vec<Chapter>);

impl Chapters {
    /// Notes what is playing now; a new chapter when the recipe changes.
    pub fn note(&mut self, at: f64, name: &str) {
        if self.0.last().is_some_and(|c| c.name == name) {
            return;
        }
        // a recipe that played for less than 10 seconds is not a chapter
        if let Some(last) = self.0.last_mut()
            && at - last.at < 10.0
        {
            last.name = name.to_string();
            if self.0.len() >= 2 && self.0[self.0.len() - 2].name == name {
                self.0.pop();
            }
            return;
        }
        self.0.push(Chapter {
            at: if self.0.is_empty() { 0.0 } else { at },
            name: name.into(),
        });
    }

    /// The chapters' lines, if there are enough for YouTube (three or more).
    pub fn lines(&self) -> Option<String> {
        if self.0.len() < 3 {
            return None;
        }
        Some(
            self.0
                .iter()
                .map(|c| format!("{} {}", clock(c.at), c.name))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

fn clock(s: f64) -> String {
    let s = s.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn fits(t: &str) -> bool {
    t.chars().count() <= 100
}

/// The title, as the most watched streams of the kind write theirs: the words a viewer
/// searches for first ("Deep Sleep Music"), then 🔴 LIVE, how long it plays, what it is good
/// for, and the recipe last. The seed picks one of a few forms, so streams differ.
fn title(p: &Playing, f: &Profile, lang: Lang) -> String {
    let (len, len_es) = match p.minutes {
        Some(m) if m >= 120 => (format!("{} Hours", m / 60), format!("{} Horas", m / 60)),
        Some(m) if m >= 60 => ("1 Hour".into(), "1 Hora".into()),
        Some(m) => (format!("{m} Minutes"), format!("{m} Minutos")),
        None => (String::new(), String::new()),
    };
    // "2 Hours of deep focus ambient", or "Endless deep focus ambient"
    let len = if len.is_empty() {
        "Endless".to_string()
    } else {
        format!("{len} of")
    };
    let len_es = if len_es.is_empty() {
        "Sin Fin".to_string()
    } else {
        len_es
    };
    let e = f.emoji;
    let en = [
        format!(
            "🔴 {} {e} {} for {} | LIVE · {}",
            f.lead,
            cap(f.mood),
            f.uses,
            p.name
        ),
        format!(
            "{} {e} {len} {} that never repeats | 🔴 LIVE {}",
            f.lead, f.mood, p.name
        ),
        format!(
            "🔴 LIVE {} ~ {} | {len} {} · {}",
            f.lead,
            cap(f.uses),
            f.mood,
            p.name
        ),
    ];
    let es = [
        format!(
            "🔴 {} {e} {} | EN DIRECTO · {}",
            f.lead_es,
            cap(f.mood_es),
            p.name
        ),
        format!(
            "{} {e} {len_es} que Nunca se Repite | 🔴 EN DIRECTO {}",
            f.lead_es, p.name
        ),
        format!(
            "🔴 EN DIRECTO {} ~ {} | {}",
            f.lead_es,
            cap(f.uses_es),
            p.name
        ),
    ];
    let i = (p.seed % 3) as usize;
    let mut t = match lang {
        Lang::English => en[i].clone(),
        Lang::Spanish => es[i].clone(),
        Lang::Both => {
            let both = format!("{} | {}", en[i], f.lead_es);
            if fits(&both) { both } else { en[i].clone() }
        }
    };
    // the shortest form if that is too long, cut at a character if even that is
    if !fits(&t) {
        t = format!("🔴 {} {e} {} | LIVE · {}", f.lead, f.mood, p.name);
    }
    while !fits(&t) {
        t.pop();
    }
    t.trim_end().to_string()
}

/// The texts of a stream of `p`, with the replay's chapters if there are enough.
pub fn write(p: &Playing, lang: Lang, chapters: &Chapters) -> Promo {
    let f = &PROFILES[p.recipe.min(PROFILES.len() - 1)];

    let mut hashtags: Vec<String> = f.hashtags[..3].iter().map(|h| h.to_string()).collect();
    if lang.es() {
        hashtags.push(f.hashtag_es.into());
    }
    hashtags.extend(f.hashtags[3..].iter().map(|h| h.to_string()));
    hashtags.extend(COMMON.iter().map(|h| h.to_string()));
    hashtags.dedup();

    let seed = format!("{:04}", p.seed);
    let mut d = String::new();
    d += &hashtags[..3].join(" ");
    d += "\n\n";
    let best = |es: bool| {
        f.best
            .iter()
            .map(|(e, en, sp)| format!("{e} {}", if es { sp } else { en }))
            .collect::<Vec<_>>()
            .join("\n")
    };
    if lang.en() {
        d += &format!(
            "{lead} {e} composed live, right now, and never the same twice. Nothing here is \
             pre-recorded and nothing loops: Kontinuum, a generative ambient instrument, writes \
             every note as you listen.\n\n\
             🎧 Now playing: {name}, {mood} in {key} (seed {seed}). {about}\n\n\
             Best for:\n{best}\n\n\
             Best with headphones, at low volume. If it helps you focus or rest, a 👍 helps the \
             stream reach more people, and tell us in the chat where you are listening from.\n\n",
            lead = f.lead,
            e = f.emoji,
            name = p.name,
            mood = f.mood,
            key = p.key,
            about = p.about,
            best = best(false),
        );
    }
    if lang.es() {
        d += &format!(
            "{lead} {e} compuesta en directo, ahora mismo, y nunca igual. Nada está pregrabado \
             y nada se repite: Kontinuum, un instrumento de ambient generativo, escribe cada \
             nota mientras escuchas.\n\n\
             🎧 Suena: {name}, {mood} en {key} (semilla {seed}). {about}\n\n\
             Ideal para:\n{best}\n\n\
             Mejor con auriculares y a volumen bajo. Si te ayuda a concentrarte o descansar, un \
             👍 ayuda a que el directo llegue a más gente. Cuéntanos en el chat desde dónde \
             escuchas.\n\n",
            lead = f.lead_es,
            e = f.emoji,
            name = p.name,
            mood = f.mood_es,
            key = p.key,
            about = f.about_es,
            best = best(true),
        );
    }
    if let Some(c) = chapters.lines() {
        d += if lang.en() {
            "Chapters\n"
        } else {
            "Capítulos\n"
        };
        d += &c;
        d += "\n\n";
    }
    d += &format!(
        "{} Kontinuum (open source): {REPO}\n\n",
        if lang.en() {
            "Made live with"
        } else {
            "Hecho en directo con"
        }
    );
    d += &hashtags[3..].join(" ");

    let pinned = match lang {
        Lang::English => format!(
            "🎧 Now playing: {} in {} · seed {seed}. Composed live, it never repeats. What are \
             you working on today?",
            p.name, p.key
        ),
        Lang::Spanish => format!(
            "🎧 Suena: {} en {} · semilla {seed}. Se compone en directo y nunca se repite. ¿En \
             qué estás trabajando hoy?",
            p.name, p.key
        ),
        Lang::Both => format!(
            "🎧 Now playing / Suena: {} in {} · seed {seed}. Composed live, it never repeats. \
             What are you working on today? / ¿En qué estás hoy?",
            p.name, p.key
        ),
    };

    let mut words: Vec<String> = vec![
        f.lead.to_lowercase(),
        p.name.to_lowercase(),
        format!("{} ambient", p.name),
    ];
    if lang.es() {
        words.push(f.lead_es.to_lowercase());
    }
    words.extend(f.keywords.iter().map(|k| k.to_string()));
    if lang.es() {
        words.extend(
            [
                "musica ambiental",
                "musica relajante",
                "música para estudiar",
            ]
            .map(String::from),
        );
    }
    words.extend(COMMON_KEYWORDS.iter().map(|k| k.to_string()));
    let mut tags = String::new();
    let mut seen = Vec::new();
    for w in words {
        if seen.contains(&w) {
            continue;
        }
        // YouTube counts the quotes around a tag with spaces
        let cost = w.chars().count() + if w.contains(' ') { 2 } else { 0 };
        let sep = if tags.is_empty() { 0 } else { 1 };
        if tags.chars().count() + sep + cost > 500 {
            break;
        }
        if sep > 0 {
            tags.push(',');
        }
        tags += &w;
        seen.push(w);
    }

    Promo {
        title: title(p, f, lang),
        description: d,
        pinned,
        hashtags,
        tags,
    }
}

/// The words a viewer searches for, for a recipe ("Deep Focus Music"), in a language.
pub fn lead(recipe: usize, spanish: bool) -> &'static str {
    let f = &PROFILES[recipe.min(PROFILES.len() - 1)];
    if spanish { f.lead_es } else { f.lead }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knt_engine::CARTA;

    fn playing(i: usize, seed: u64) -> Playing {
        Playing {
            recipe: i,
            name: CARTA[i].0.into(),
            about: CARTA[i].2.into(),
            key: "C♯ Dorian".into(),
            seed,
            minutes: Some(120),
        }
    }

    #[test]
    fn follows_youtubes_rules() {
        for (i, c) in CARTA.iter().enumerate() {
            for seed in 0..3 {
                for lang in [Lang::English, Lang::Spanish, Lang::Both] {
                    let p = write(&playing(i, seed), lang, &Chapters::default());
                    assert!(p.title.chars().count() <= 100, "{}", p.title);
                    assert!(p.hashtags.len() <= 12, "{:?}", p.hashtags);
                    assert!(
                        p.hashtags
                            .iter()
                            .all(|h| h.starts_with('#') && !h.contains(' '))
                    );
                    assert!(p.description.starts_with(&p.hashtags[..3].join(" ")));
                    assert!(p.tags.chars().count() <= 500);
                    assert!(p.description.contains(c.0));
                }
            }
        }
    }

    #[test]
    fn chapters_start_at_zero_and_skip_short_ones() {
        let mut c = Chapters::default();
        c.note(3.0, "Dusk");
        c.note(5.0, "Dusk");
        assert_eq!(c.lines(), None);
        c.note(400.0, "Orbit");
        c.note(405.0, "Drift"); // replaces Orbit, which lasted 5 s
        c.note(4000.0, "Cathedral");
        assert_eq!(
            c.lines().unwrap(),
            "00:00 Dusk\n06:40 Drift\n1:06:40 Cathedral"
        );
    }
}
