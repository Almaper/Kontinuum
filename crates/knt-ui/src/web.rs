//! The control page of the server mode: a small web server, behind a password, that shows what
//! plays (a picture of the stage, the stream's state, the log, the texts for YouTube) and lets
//! one change it from a phone (start and stop, the recipe, nature, lo-fi, a new seed) and edit
//! every setting (saved to server.toml and applied at once).
//!
//! Plain HTTP: the password and the page travel unencrypted, as on any `http://` page.
//! A failed sign-in waits a second; ten from one address lock it out for ten minutes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::net::IpAddr;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tiny_http::{Header, Request, Response, Server};

/// What the page asks of the server.
#[derive(Debug)]
pub enum WebCmd {
    Recipe(usize),
    Clock(bool),
    NewSeed,
    NextSection,
    Ambience(String),
    Level(f32),
    Lofi(bool),
    Binaural(f32),
    /// Live now (Some(true)), stopped (Some(false)), or as the schedule says (None).
    Live(Option<bool>),
    /// New settings (the JSON of server.toml, with secrets left as [`SECRET`] when unchanged).
    Save(serde_json::Value),
}

/// What stands for a secret that is set (keys, the password) in the settings sent to the page.
pub const SECRET: &str = "••••••";

/// What the page shows, kept by the server's loop.
#[derive(Default)]
pub struct Shared {
    pub state: Mutex<serde_json::Value>,
    pub settings: Mutex<serde_json::Value>,
    pub frame: Mutex<Vec<u8>>,
    pub password: Mutex<String>,
}

/// The log's last lines, for the page.
pub fn log_lines() -> &'static Mutex<VecDeque<String>> {
    static LOG: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(VecDeque::new()))
}

pub fn push_log(line: String) {
    if let Ok(mut l) = log_lines().lock() {
        l.push_back(line);
        while l.len() > 60 {
            l.pop_front();
        }
    }
}

/// Random bytes as hex (the system's randomness; a clock-based fallback).
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_ok();
    if !ok {
        use std::hash::{BuildHasher, Hasher};
        for (k, b) in buf.iter_mut().enumerate() {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_usize(k);
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            );
            *b = h.finish() as u8;
        }
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// A password that is easy to type on a phone: three groups of four letters and digits.
pub fn new_password() -> String {
    const ABC: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let hex = random_hex(12);
    let mut out = String::new();
    for (k, pair) in hex.as_bytes().chunks(2).enumerate() {
        let v = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("00"), 16).unwrap_or(0);
        if k > 0 && k % 4 == 0 {
            out.push('-');
        }
        out.push(ABC[v as usize % ABC.len()] as char);
    }
    out
}

fn same(a: &str, b: &str) -> bool {
    // the same time for any wrong password of the same length
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

const PAGE: &str = include_str!("../../../assets/panel.html");

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("a valid header")
}

fn cookie(req: &Request) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.equiv("Cookie"))
        .and_then(|h| {
            h.value
                .as_str()
                .split(';')
                .map(str::trim)
                .find_map(|c| c.strip_prefix("knt=").map(String::from))
        })
}

fn json(v: &serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(v.to_string()).with_header(header("Content-Type", "application/json"))
}

fn status(code: u16, text: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(text).with_status_code(code)
}

/// Starts the page on `port`, on its own thread.
pub fn start(port: u16, shared: Arc<Shared>, tx: mpsc::Sender<WebCmd>) -> Result<(), String> {
    let server = Server::http(("0.0.0.0", port))
        .map_err(|e| format!("the control page, port {port}: {e}"))?;
    std::thread::Builder::new()
        .name("kontinuum-web".into())
        .spawn(move || serve(server, shared, tx))
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn serve(server: Server, shared: Arc<Shared>, tx: mpsc::Sender<WebCmd>) {
    let mut sessions: HashSet<String> = HashSet::new();
    let mut fails: HashMap<IpAddr, (u32, Instant)> = HashMap::new();
    for mut req in server.incoming_requests() {
        let ip = req.remote_addr().map(|a| a.ip());
        let url = req.url().split('?').next().unwrap_or("/").to_string();
        let method = req.method().as_str().to_string();
        let signed = cookie(&req).is_some_and(|c| sessions.contains(&c));
        let mut body = String::new();
        if method == "POST" {
            let _ = req.as_reader().take(1 << 20).read_to_string(&mut body);
        }
        let resp = match (method.as_str(), url.as_str()) {
            ("GET", "/") => Response::from_string(PAGE)
                .with_header(header("Content-Type", "text/html; charset=utf-8"))
                .with_header(header("Cache-Control", "no-store")),
            ("POST", "/login") => {
                let locked = ip
                    .and_then(|i| fails.get(&i))
                    .is_some_and(|(n, at)| *n >= 10 && at.elapsed() < Duration::from_secs(600));
                let given = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| v["password"].as_str().map(String::from))
                    .unwrap_or_default();
                let right = shared
                    .password
                    .lock()
                    .map(|p| p.clone())
                    .unwrap_or_default();
                if locked {
                    status(429, "Too many tries: wait ten minutes.")
                } else if !right.is_empty() && same(given.trim(), &right) {
                    if let Some(i) = ip {
                        fails.remove(&i);
                    }
                    let token = random_hex(24);
                    sessions.insert(token.clone());
                    Response::from_string("ok").with_header(header(
                        "Set-Cookie",
                        &format!("knt={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=2592000"),
                    ))
                } else {
                    if let Some(i) = ip {
                        let e = fails.entry(i).or_insert((0, Instant::now()));
                        if e.1.elapsed() > Duration::from_secs(600) {
                            *e = (0, Instant::now());
                        }
                        e.0 += 1;
                        e.1 = Instant::now();
                    }
                    std::thread::sleep(Duration::from_secs(1));
                    status(401, "Wrong password.")
                }
            }
            _ if !signed => status(401, "Sign in first."),
            ("POST", "/logout") => {
                if let Some(c) = cookie(&req) {
                    sessions.remove(&c);
                }
                status(200, "ok")
            }
            ("GET", "/api/state") => {
                json(&shared.state.lock().map(|s| s.clone()).unwrap_or_default())
            }
            ("GET", "/api/settings") => json(
                &shared
                    .settings
                    .lock()
                    .map(|s| s.clone())
                    .unwrap_or_default(),
            ),
            ("GET", "/api/frame.png") => {
                Response::from_data(shared.frame.lock().map(|f| f.clone()).unwrap_or_default())
                    .with_header(header("Content-Type", "image/png"))
                    .with_header(header("Cache-Control", "no-store"))
            }
            ("POST", "/api/cmd") => match command(&body) {
                Some(c) => {
                    let _ = tx.send(c);
                    status(200, "ok")
                }
                None => status(400, "Unknown command."),
            },
            _ => status(404, "Not found."),
        };
        let _ = req.respond(resp);
    }
}

fn command(body: &str) -> Option<WebCmd> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let f = |k: &str| v[k].as_f64().map(|x| x as f32);
    Some(match v["do"].as_str()? {
        "recipe" => WebCmd::Recipe(v["value"].as_u64()? as usize),
        "clock" => WebCmd::Clock(v["value"].as_bool()?),
        "seed" => WebCmd::NewSeed,
        "next" => WebCmd::NextSection,
        "ambience" => WebCmd::Ambience(v["value"].as_str()?.to_string()),
        "level" => WebCmd::Level(f("value")?),
        "lofi" => WebCmd::Lofi(v["value"].as_bool()?),
        "binaural" => WebCmd::Binaural(f("value")?),
        "live" => WebCmd::Live(v["value"].as_bool()),
        "save" => WebCmd::Save(v["value"].clone()),
        _ => return None,
    })
}

/// The settings for the page: each secret that is set shown as [`SECRET`].
pub fn masked(settings: &serde_json::Value) -> serde_json::Value {
    let mut v = settings.clone();
    let mask = |x: &mut serde_json::Value| {
        if x.as_str().is_some_and(|s| !s.is_empty()) {
            *x = serde_json::Value::String(SECRET.into());
        }
    };
    mask(&mut v["stream"]["key"]);
    mask(&mut v["chat"]["api_key"]);
    mask(&mut v["web"]["password"]);
    if let Some(a) = v["stream"]["extra"].as_array_mut() {
        for d in a {
            mask(&mut d["key"]);
        }
    }
    v
}

/// New settings from the page, with the secrets it left as [`SECRET`] taken from the old ones.
pub fn unmask(new: &serde_json::Value, old: &serde_json::Value) -> serde_json::Value {
    let mut v = new.clone();
    let keep = |x: &mut serde_json::Value, o: &serde_json::Value| {
        if x.as_str() == Some(SECRET) {
            *x = o.clone();
        }
    };
    keep(&mut v["stream"]["key"], &old["stream"]["key"]);
    keep(&mut v["chat"]["api_key"], &old["chat"]["api_key"]);
    keep(&mut v["web"]["password"], &old["web"]["password"]);
    if let (Some(a), Some(b)) = (
        v["stream"]["extra"].as_array_mut(),
        old["stream"]["extra"].as_array(),
    ) {
        for (k, d) in a.iter_mut().enumerate() {
            if d["key"].as_str() == Some(SECRET) {
                d["key"] = b.get(k).map(|o| o["key"].clone()).unwrap_or_default();
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_and_secrets() {
        let p = new_password();
        assert_eq!(p.len(), 14);
        assert_eq!(p.matches('-').count(), 2);
        assert_ne!(p, new_password());
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
        let old = serde_json::json!({
            "stream": {"key": "real-key", "extra": [{"key": "tw"}, {"key": ""}]},
            "chat": {"api_key": ""},
            "web": {"password": "pw"}
        });
        let m = masked(&old);
        assert_eq!(m["stream"]["key"], SECRET);
        assert_eq!(m["chat"]["api_key"], "");
        assert_eq!(m["stream"]["extra"][0]["key"], SECRET);
        let mut changed = m.clone();
        changed["web"]["password"] = "new-pw".into();
        let u = unmask(&changed, &old);
        assert_eq!(u["stream"]["key"], "real-key");
        assert_eq!(u["stream"]["extra"][0]["key"], "tw");
        assert_eq!(u["web"]["password"], "new-pw");
    }
}
