//! The YouTube chat: Kontinuum reads it (the YouTube Data API, with an API key) and lets the
//! audience take part, safely:
//!
//! - votes for the next recipe (`!orbit`, `!dusk`… or `!1`, `!2`, `!3` in a poll), and a poll
//!   now and then; nature (`!nature rain`) and the lo-fi mode (`!lofi`), with cooldowns;
//! - a welcome on the stage for whoever writes for the first time;
//! - the people watching (one star each), and milestones (subscribers, likes).
//!
//! Nothing a viewer writes is shown on the stage: only fixed commands do anything, and the
//! only words shown are names, and only clean ones. A message with a blocked word (built in,
//! in English and Spanish, plus the user's own) makes the stage ignore its author for the
//! rest of the stream; blocked names are always ignored. YouTube's own moderation (held
//! messages, blocked words, slow mode) still applies first.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use knt_engine::CARTA;
use knt_engine::extras::Ambience;
use serde::{Deserialize, Serialize};

/// The chat's settings (chat.toml).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// A YouTube Data API key (Google Cloud console > APIs & Services > Credentials).
    pub api_key: String,
    /// The live video: its link or its id.
    pub video: String,
    /// The audience may vote for the recipe.
    pub votes: bool,
    /// A poll every this many minutes (0: none).
    pub poll_minutes: u32,
    /// The audience may change the nature and the lo-fi mode.
    pub sounds: bool,
    /// Newcomers are welcomed on the stage.
    pub greet: bool,
    /// More blocked words, comma-separated.
    pub blocked_words: String,
    /// Names the stage always ignores, comma-separated.
    pub blocked_names: String,
    /// Seconds between reads of the chat (each costs 5 of the 10,000 daily quota units).
    pub every: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            api_key: String::new(),
            video: String::new(),
            votes: true,
            poll_minutes: 20,
            sounds: true,
            greet: true,
            blocked_words: String::new(),
            blocked_names: String::new(),
            every: 15,
        }
    }
}

/// The video id in a YouTube link (watch?v=, youtu.be/, /live/, /shorts/) or the id itself.
pub fn video_id(s: &str) -> Option<String> {
    let s = s.trim();
    let ok = |id: &str| {
        id.len() == 11
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    if ok(s) {
        return Some(s.to_string());
    }
    for key in ["v=", "youtu.be/", "/live/", "/shorts/", "/embed/"] {
        if let Some(i) = s.find(key) {
            let rest = &s[i + key.len()..];
            let id: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if ok(&id) {
                return Some(id);
            }
        }
    }
    None
}

// ---- moderation ----

/// Built-in blocked words (insults, slurs and obscenities, English and Spanish), matched in
/// the normalized text (see [`normalize`]).
const BLOCKED: &[&str] = &[
    "fuck",
    "shit",
    "bitch",
    "cunt",
    "dick",
    "cock",
    "pussy",
    "asshole",
    "bastard",
    "slut",
    "whore",
    "nigg",
    "fag",
    "retard",
    "rape",
    "nazi",
    "kys",
    "killyourself",
    "porn",
    "sex",
    "puta",
    "puto",
    "mierda",
    "joder",
    "cono",
    "cabron",
    "gilipollas",
    "polla",
    "verga",
    "pendejo",
    "maricon",
    "zorra",
    "subnormal",
    "culero",
    "chinga",
    "follar",
    "hijodeputa",
    "malparido",
    "idiota",
    "imbecil",
    "estupid",
    "stupid",
    "idiot",
    "moron",
];

/// Lowercase, no accents, the usual stand-ins for letters (0→o, 1→i, 3→e, 4→a, 5→s, 7→t,
/// @→a, $→s), no separators, and no letter repeated: "P.u.t@a" and "puuuta" become "puta".
pub fn normalize(s: &str) -> String {
    let mut out = String::new();
    let mut last = '\0';
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' | '4' | '@' => 'a',
            'é' | 'è' | 'ë' | 'ê' | '3' => 'e',
            'í' | 'ì' | 'ï' | 'î' | '1' | '!' | '|' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | '0' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            '5' | '$' => 's',
            '7' => 't',
            c if c.is_alphanumeric() => c,
            _ => continue,
        };
        if c != last {
            out.push(c);
        }
        last = c;
    }
    out
}

/// The words of a text, normalized; letters written apart ("f u c k", "p.u.t.a") are joined
/// into one word.
fn words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut run = String::new();
    for w in text
        .split_whitespace()
        .map(normalize)
        .filter(|w| !w.is_empty())
    {
        if w.chars().count() == 1 {
            run.push_str(&w);
            continue;
        }
        if !run.is_empty() {
            out.push(std::mem::take(&mut run));
        }
        out.push(w);
    }
    if !run.is_empty() {
        out.push(run);
    }
    out
}

/// Whether a text is free of blocked words (the built-in ones and `extra`).
pub fn is_clean(text: &str, extra: &[String]) -> bool {
    let words = words(text);
    for b in BLOCKED
        .iter()
        .map(|b| b.to_string())
        .chain(extra.iter().map(|w| normalize(w)))
    {
        if b.is_empty() {
            continue;
        }
        let hit = if b.len() <= 4 {
            // a short word must stand alone, or nearly ("putas"), not inside another
            // ("cocktail", "computadora")
            words
                .iter()
                .any(|w| w.starts_with(&b) && w.len() <= b.len() + 2)
        } else {
            words.iter().any(|w| w.contains(&b))
        };
        if hit {
            return false;
        }
    }
    true
}

fn list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

// ---- commands ----

/// The recipe a command names, in English or Spanish.
pub fn recipe_command(word: &str) -> Option<usize> {
    let w = normalize(word);
    let names: [&[&str]; 8] = [
        &["mist", "earlymist", "niebla", "sleep", "dormir"],
        &[
            "focus",
            "deepfocus",
            "foco",
            "concentracion",
            "study",
            "estudiar",
        ],
        &["cathedral", "catedral", "choir", "coro"],
        &["copper", "copperrain", "cobre", "lluviacobre"],
        &["forest", "wetforest", "bosque"],
        &["orbit", "space", "orbita", "espacio"],
        &["dusk", "atardecer", "sunset"],
        &["drift", "deriva", "mix"],
    ];
    names.iter().position(|n| n.contains(&w.as_str()))
}

fn ambience_word(word: &str) -> Option<Ambience> {
    match normalize(word).as_str() {
        "rain" | "lluvia" => Some(Ambience::Rain),
        "forest" | "bosque" | "birds" | "pajaros" => Some(Ambience::Forest),
        "fire" | "fuego" | "chimenea" => Some(Ambience::Fire),
        "sea" | "mar" | "ocean" | "oceano" | "waves" | "olas" => Some(Ambience::Sea),
        "off" | "none" | "no" | "nada" => Some(Ambience::None),
        _ => None,
    }
}

/// A chat message, as the stage needs it.
#[derive(Debug, Clone)]
pub struct Message {
    pub author: String,
    pub author_id: String,
    pub text: String,
    pub owner: bool,
}

/// What the chat asks of Kontinuum.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Recipe(usize),
    Ambience(Ambience),
    Lofi(bool),
    Greet(String),
    Milestone(String),
    Viewers(u32),
    Event(String),
}

/// A poll in progress.
#[derive(Debug, Clone)]
pub struct Poll {
    pub options: Vec<usize>,
    pub votes: Vec<u32>,
    pub voters: HashSet<String>,
    pub ends: f64,
}

/// The chat's logic, apart from the network: what each message does.
#[derive(Debug, Default)]
pub struct Brain {
    pub seen: HashSet<String>,
    pub ignored: HashSet<String>,
    pub poll: Option<Poll>,
    pub last_poll: f64,
    /// When each author last changed a sound, and when anyone did.
    sound_by: HashMap<String, f64>,
    sound_at: f64,
    lofi: bool,
    subs: Option<u64>,
    likes: Option<u64>,
    rng: u32,
}

const POLL_SECONDS: f64 = 90.0;

impl Brain {
    fn random(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.rng >> 8
    }

    /// Starts a poll among `first` (if any) and other recipes, not `current`.
    pub fn start_poll(&mut self, now: f64, current: usize, first: Option<usize>) {
        let mut options: Vec<usize> = first.into_iter().collect();
        let mut guard = 0;
        while options.len() < 3 && guard < 100 {
            guard += 1;
            let r = self.random() as usize % CARTA.len();
            if r != current && !options.contains(&r) {
                options.push(r);
            }
        }
        self.poll = Some(Poll {
            votes: vec![0; options.len()],
            options,
            voters: HashSet::new(),
            ends: now + POLL_SECONDS,
        });
        self.last_poll = now;
    }

    /// One message: what it asks, if it is allowed.
    pub fn message(&mut self, m: &Message, cfg: &Config, now: f64, current: usize) -> Vec<Action> {
        let mut out = Vec::new();
        let names = list(&cfg.blocked_names);
        if self.ignored.contains(&m.author_id) || names.contains(&m.author.to_lowercase()) {
            return out;
        }
        let extra = list(&cfg.blocked_words);
        if !is_clean(&m.text, &extra) {
            // for the rest of the stream, the stage ignores this author
            self.ignored.insert(m.author_id.clone());
            return out;
        }
        let first_time = self.seen.insert(m.author_id.clone());
        // a name with a blocked word inside it is not shown (its votes still count)
        let name_ok = {
            let n = normalize(&m.author);
            BLOCKED
                .iter()
                .map(|b| b.to_string())
                .chain(extra.iter().map(|w| normalize(w)))
                .all(|b| b.is_empty() || !n.contains(&b))
        };
        if first_time && cfg.greet && !m.owner && name_ok {
            let name: String = m.author.trim_start_matches('@').chars().take(24).collect();
            out.push(Action::Greet(name));
        }
        let text = m.text.trim();
        let Some(cmd) = text.strip_prefix('!') else {
            return out;
        };
        let mut words = cmd.split_whitespace();
        let Some(head) = words.next() else {
            return out;
        };
        let head = head.to_lowercase();
        // a vote: !1..!3 in a poll, or a recipe's name (which starts a poll)
        if cfg.votes {
            let choice = match (&self.poll, head.parse::<usize>()) {
                (Some(p), Ok(k)) if k >= 1 && k <= p.options.len() => Some(p.options[k - 1]),
                _ => recipe_command(&head),
            };
            if let Some(r) = choice {
                if self.poll.is_none() {
                    if r == current {
                        return out;
                    }
                    self.start_poll(now, current, Some(r));
                    out.push(Action::Event(format!(
                        "{} asks for {}: vote in the chat",
                        short(&m.author),
                        CARTA[r].0
                    )));
                }
                if let Some(p) = &mut self.poll
                    && let Some(k) = p.options.iter().position(|&o| o == r)
                    && p.voters.insert(m.author_id.clone())
                {
                    p.votes[k] += 1;
                }
                return out;
            }
        }
        // the sounds: one change per person every 5 minutes, one for all every 2
        if cfg.sounds {
            let allowed = now - self.sound_at > 120.0
                && self
                    .sound_by
                    .get(&m.author_id)
                    .is_none_or(|t| now - t > 300.0)
                || m.owner;
            let act = match head.as_str() {
                "nature" | "naturaleza" | "sound" | "sonido" => {
                    words.next().and_then(ambience_word).map(Action::Ambience)
                }
                "lofi" => {
                    self.lofi = !self.lofi;
                    Some(Action::Lofi(self.lofi))
                }
                w => ambience_word(w)
                    .filter(|a| *a != Ambience::None)
                    .map(Action::Ambience),
            };
            if let Some(a) = act {
                if allowed {
                    self.sound_at = now;
                    self.sound_by.insert(m.author_id.clone(), now);
                    let what = match &a {
                        Action::Ambience(Ambience::None) => "silence under the music".to_string(),
                        Action::Ambience(x) => format!("the sound of {}", x.name()),
                        Action::Lofi(true) => "the lo-fi beat".into(),
                        Action::Lofi(false) => "the music without the beat".into(),
                        _ => String::new(),
                    };
                    out.push(Action::Event(format!(
                        "{} brought {what}",
                        short(&m.author)
                    )));
                    out.push(a);
                } else if head == "lofi" {
                    self.lofi = !self.lofi;
                }
            }
        }
        out
    }

    /// Time passes: a poll ends, a new one begins.
    pub fn tick(&mut self, cfg: &Config, now: f64, current: usize) -> Vec<Action> {
        let mut out = Vec::new();
        if let Some(p) = &self.poll
            && now >= p.ends
        {
            let best = p.votes.iter().copied().max().unwrap_or(0);
            if best > 0 {
                let k = p.votes.iter().position(|&v| v == best).unwrap_or(0);
                let r = p.options[k];
                out.push(Action::Event(format!("The chat chose {}", CARTA[r].0)));
                if r != current {
                    out.push(Action::Recipe(r));
                }
            }
            self.poll = None;
        }
        if cfg.votes
            && cfg.poll_minutes > 0
            && self.poll.is_none()
            && now - self.last_poll > cfg.poll_minutes as f64 * 60.0
            && !self.seen.is_empty()
        {
            self.start_poll(now, current, None);
            out.push(Action::Event(
                "What next? Vote !1, !2 or !3 in the chat".into(),
            ));
        }
        out
    }

    /// The channel's numbers: a milestone when one is crossed.
    pub fn numbers(&mut self, subs: Option<u64>, likes: Option<u64>) -> Vec<Action> {
        const SUBS: [u64; 14] = [
            10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10_000, 25_000, 50_000, 100_000, 1_000_000,
        ];
        const LIKES: [u64; 8] = [10, 25, 50, 100, 250, 500, 1000, 5000];
        let mut out = Vec::new();
        let cross = |old: Option<u64>, new: Option<u64>, marks: &[u64]| -> Option<u64> {
            let (o, n) = (old?, new?);
            marks.iter().rev().find(|&&m| o < m && n >= m).copied()
        };
        if let Some(m) = cross(self.subs, subs, &SUBS) {
            out.push(Action::Milestone(format!("{m} subscribers · thank you")));
        }
        if let Some(m) = cross(self.likes, likes, &LIKES) {
            out.push(Action::Milestone(format!("{m} likes · thank you")));
        }
        if subs.is_some() {
            self.subs = subs;
        }
        if likes.is_some() {
            self.likes = likes;
        }
        out
    }
}

fn short(name: &str) -> String {
    name.trim_start_matches('@').chars().take(24).collect()
}

// ---- the network ----

/// What the reader finds.
#[derive(Debug)]
pub enum Event {
    Message(Message),
    Numbers {
        viewers: Option<u32>,
        likes: Option<u64>,
        subs: Option<u64>,
    },
    Status(String),
}

/// Reads the chat on its own thread until `stop`.
pub struct Reader {
    stop: Arc<AtomicBool>,
    pub rx: mpsc::Receiver<Event>,
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn get(url: &str) -> Result<serde_json::Value, String> {
    match ureq::get(url).timeout(Duration::from_secs(15)).call() {
        Ok(r) => r.into_json().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(code, r)) => {
            let body: serde_json::Value = r.into_json().unwrap_or_default();
            let msg = body["error"]["message"].as_str().unwrap_or("").to_string();
            Err(format!("YouTube said {code}: {msg}"))
        }
        Err(e) => Err(e.to_string()),
    }
}

fn number(v: &serde_json::Value) -> Option<u64> {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_u64())
}

const API: &str = "https://www.googleapis.com/youtube/v3";

impl Reader {
    pub fn start(cfg: &Config) -> Result<Reader, String> {
        let key = cfg.api_key.trim().to_string();
        if key.is_empty() {
            return Err("Paste a YouTube Data API key first.".into());
        }
        let id = video_id(&cfg.video).ok_or("Paste the link of your live video.")?;
        let every = cfg.every.max(5) as u64;
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let s2 = stop.clone();
        std::thread::Builder::new()
            .name("kontinuum-chat".into())
            .spawn(move || read(key, id, every, s2, tx))
            .map_err(|e| e.to_string())?;
        Ok(Reader { stop, rx })
    }
}

fn read(key: String, id: String, every: u64, stop: Arc<AtomicBool>, tx: mpsc::Sender<Event>) {
    let sleep = |secs: u64| {
        for _ in 0..secs * 10 {
            if stop.load(Ordering::SeqCst) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        true
    };
    let say = |s: String| {
        let _ = tx.send(Event::Status(s));
    };
    // the video: its chat and its channel
    let (chat, channel) = loop {
        let url = format!("{API}/videos?part=liveStreamingDetails,snippet&id={id}&key={key}");
        match get(&url) {
            Ok(v) => {
                let item = &v["items"][0];
                let chat = item["liveStreamingDetails"]["activeLiveChatId"]
                    .as_str()
                    .map(String::from);
                let channel = item["snippet"]["channelId"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                match chat {
                    Some(c) => break (c, channel),
                    None => say(
                        "The video has no live chat yet (is it live?). Trying again in a minute."
                            .into(),
                    ),
                }
            }
            Err(e) => say(e),
        }
        if !sleep(60) {
            return;
        }
    };
    say("Reading the chat.".into());
    let mut page = String::new();
    let mut first = true;
    let mut last_numbers = 0u64;
    let mut t = 0u64;
    while !stop.load(Ordering::SeqCst) {
        let mut url = format!(
            "{API}/liveChat/messages?liveChatId={chat}&part=snippet,authorDetails&maxResults=200&key={key}"
        );
        if !page.is_empty() {
            url.push_str(&format!("&pageToken={page}"));
        }
        let mut wait = every;
        match get(&url) {
            Ok(v) => {
                page = v["nextPageToken"].as_str().unwrap_or("").to_string();
                if let Some(ms) = v["pollingIntervalMillis"].as_u64() {
                    wait = wait.max(ms.div_ceil(1000));
                }
                // the first page is the chat's past: not acted on
                if !first {
                    for it in v["items"].as_array().into_iter().flatten() {
                        if it["snippet"]["type"].as_str() != Some("textMessageEvent") {
                            continue;
                        }
                        let a = &it["authorDetails"];
                        let _ = tx.send(Event::Message(Message {
                            author: a["displayName"].as_str().unwrap_or("").into(),
                            author_id: a["channelId"].as_str().unwrap_or("").into(),
                            text: it["snippet"]["displayMessage"]
                                .as_str()
                                .unwrap_or("")
                                .into(),
                            owner: a["isChatOwner"].as_bool().unwrap_or(false),
                        }));
                    }
                }
                first = false;
            }
            Err(e) => {
                say(e);
                wait = 60;
            }
        }
        // the numbers: every two minutes (1 unit each)
        if t >= last_numbers + 120 || last_numbers == 0 {
            last_numbers = t.max(1);
            let v = get(&format!(
                "{API}/videos?part=liveStreamingDetails,statistics&id={id}&key={key}"
            ));
            let s = (!channel.is_empty())
                .then(|| {
                    get(&format!(
                        "{API}/channels?part=statistics&id={channel}&key={key}"
                    ))
                    .ok()
                })
                .flatten();
            if let Ok(v) = v {
                let item = &v["items"][0];
                let _ = tx.send(Event::Numbers {
                    viewers: number(&item["liveStreamingDetails"]["concurrentViewers"])
                        .map(|n| n as u32),
                    likes: number(&item["statistics"]["likeCount"]),
                    subs: s
                        .as_ref()
                        .and_then(|s| number(&s["items"][0]["statistics"]["subscriberCount"])),
                });
            }
        }
        if !sleep(wait) {
            return;
        }
        t += wait;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(author: &str, text: &str) -> Message {
        Message {
            author: author.into(),
            author_id: format!("id-{author}"),
            text: text.into(),
            owner: false,
        }
    }

    #[test]
    fn finds_the_video_id() {
        for s in [
            "https://www.youtube.com/watch?v=abcdefghijk",
            "https://youtu.be/abcdefghijk?si=x",
            "https://www.youtube.com/live/abcdefghijk?feature=share",
            "abcdefghijk",
        ] {
            assert_eq!(video_id(s).as_deref(), Some("abcdefghijk"), "{s}");
        }
        assert_eq!(video_id("hello"), None);
    }

    #[test]
    fn blocks_insults_even_disguised() {
        let none: Vec<String> = vec![];
        assert!(!is_clean("eres una P.u.t@a", &none));
        assert!(!is_clean("puuuuta", &none));
        assert!(!is_clean("F U C K this", &none));
        assert!(!is_clean("sh1t", &none));
        assert!(is_clean("Hola desde Madrid!", &none));
        assert!(is_clean(
            "I love this cocktail of sounds, Dickens would too",
            &none
        ));
        assert!(is_clean("this hit the spot", &none));
        assert!(!is_clean("hijo de puta", &none));
        assert!(!is_clean("you are a pepino", &["pepino".into()]));
    }

    #[test]
    fn votes_polls_and_cooldowns() {
        let cfg = Config::default();
        let mut b = Brain::default();
        // a newcomer is welcomed, and asks for Orbit: a poll starts with Orbit in it
        let a = b.message(&msg("Ana", "!orbit"), &cfg, 0.0, 1);
        assert!(a.contains(&Action::Greet("Ana".into())));
        let p = b.poll.as_ref().unwrap();
        assert_eq!(p.options[0], 5);
        assert_eq!(p.votes[0], 1);
        // a second vote by the same person does not count
        b.message(&msg("Ana", "!1"), &cfg, 1.0, 1);
        assert_eq!(b.poll.as_ref().unwrap().votes[0], 1);
        b.message(&msg("Luis", "!2"), &cfg, 2.0, 1);
        b.message(&msg("Eva", "!orbita"), &cfg, 3.0, 1);
        let out = b.tick(&cfg, 100.0, 1);
        assert!(out.contains(&Action::Recipe(5)));
        // an insult: ignored, and so is everything its author writes afterwards
        assert!(
            b.message(&msg("Troll", "!dusk puta"), &cfg, 200.0, 5)
                .is_empty()
        );
        assert!(b.message(&msg("Troll", "!dusk"), &cfg, 201.0, 5).is_empty());
        assert!(b.poll.is_none());
        // a name that is an insult is not greeted
        assert!(
            b.message(&msg("xXputaXx", "hola"), &cfg, 202.0, 5)
                .is_empty()
        );
        assert!(!b.message(&msg("Marta", "hola"), &cfg, 203.0, 5).is_empty());
        // the sounds: one change, then the cooldown
        let a = b.message(&msg("Luis", "!nature rain"), &cfg, 300.0, 5);
        assert!(a.contains(&Action::Ambience(Ambience::Rain)));
        let a = b.message(&msg("Eva", "!fire"), &cfg, 310.0, 5);
        assert!(!a.iter().any(|x| matches!(x, Action::Ambience(_))));
        let a = b.message(&msg("Eva", "!fuego"), &cfg, 430.0, 5);
        assert!(a.contains(&Action::Ambience(Ambience::Fire)));
    }

    #[test]
    fn milestones_when_crossed() {
        let mut b = Brain::default();
        assert!(b.numbers(Some(95), Some(8)).is_empty());
        let m = b.numbers(Some(101), Some(12));
        assert!(m.contains(&Action::Milestone("100 subscribers · thank you".into())));
        assert!(m.contains(&Action::Milestone("10 likes · thank you".into())));
        assert!(b.numbers(Some(102), Some(13)).is_empty());
    }
}
