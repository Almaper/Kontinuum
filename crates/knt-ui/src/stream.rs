//! The live stream: the stage and the music, encoded by ffmpeg and sent to YouTube (RTMP), or
//! recorded to a file, with no OBS. It runs on its own thread, so it keeps going when the
//! window is hidden or minimized.
//!
//! The audio comes from the player's tap (every sample the sound card plays); the video is the
//! stage drawn in software ([`crate::canvas::SkiaCanvas`]), one frame each 1/fps of audio, so
//! picture and sound stay in step with the sound card's clock. ffmpeg reads the frames on its
//! standard input and the audio from a local TCP socket. When ffmpeg or the connection fails,
//! the stream starts again after a few seconds, until it is stopped.

use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command as Proc, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use knt_engine::Snapshot;
use serde::{Deserialize, Serialize};

use crate::canvas::SkiaCanvas;

/// Another place to stream to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Destination {
    pub name: String,
    pub url: String,
    pub key: String,
    pub on: bool,
}

/// Streaming by the clock: every day from `start` to `stop` ("HH:MM", local time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    pub on: bool,
    pub start: String,
    pub stop: String,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            on: false,
            start: "22:00".into(),
            stop: "06:00".into(),
        }
    }
}

/// "HH:MM" in hours.
pub fn parse_hm(s: &str) -> Option<f32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h as f32 + m as f32 / 60.0)
}

impl Schedule {
    /// Whether `hour` (0..24) is within the schedule (it may cross midnight).
    pub fn within(&self, hour: f32) -> bool {
        match (parse_hm(&self.start), parse_hm(&self.stop)) {
            (Some(a), Some(b)) if a <= b => hour >= a && hour < b,
            (Some(a), Some(b)) => hour >= a || hour < b,
            _ => false,
        }
    }
}

/// The folder of the recordings (the user's Videos, else home).
pub fn recordings_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let videos = home.join("Videos");
    let base = if videos.is_dir() { videos } else { home };
    base.join("Kontinuum")
}

/// The stream's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// The RTMP server (YouTube's primary ingest by default).
    pub url: String,
    /// The stream key (from YouTube Studio > Go live > Stream).
    pub key: String,
    /// 720 or 1080.
    pub height: u32,
    pub fps: u32,
    /// Record to this file instead of streaming (.mkv, .mp4 or .flv).
    pub record: Option<PathBuf>,
    /// ffmpeg's path (None: next to Kontinuum, else on the PATH).
    pub ffmpeg: Option<PathBuf>,
    /// Upright video (9:16), for Shorts and phones.
    pub vertical: bool,
    /// More places to stream to at the same time (Twitch, Kick…).
    pub extra: Vec<Destination>,
    /// Keep a copy of every stream in the recordings folder.
    pub keep_copy: bool,
    /// Start and stop by the clock.
    pub schedule: Schedule,
    /// The language of the title, description and comment it writes for YouTube.
    pub lang: crate::promo::Lang,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            url: "rtmp://a.rtmp.youtube.com/live2".into(),
            key: String::new(),
            height: 720,
            fps: 30,
            record: None,
            ffmpeg: None,
            vertical: false,
            extra: vec![
                Destination {
                    name: "Twitch".into(),
                    url: "rtmp://live.twitch.tv/app".into(),
                    key: String::new(),
                    on: false,
                },
                Destination {
                    name: "Kick".into(),
                    url: "rtmps://fa723fc1b171.global-contribute.live-video.net/app".into(),
                    key: String::new(),
                    on: false,
                },
            ],
            keep_copy: false,
            schedule: Schedule::default(),
            lang: crate::promo::Lang::default(),
        }
    }
}

impl Config {
    fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("Kontinuum").join("stream.toml"))
    }

    pub fn load() -> Config {
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

    pub fn size(&self) -> (u32, u32) {
        let (w, h) = if self.height >= 1080 {
            (1920, 1080)
        } else {
            (1280, 720)
        };
        if self.vertical { (h, w) } else { (w, h) }
    }

    /// The video bitrate, kbit/s (YouTube's recommendations for 30 fps).
    fn kbps(&self) -> u32 {
        if self.height >= 1080 { 6000 } else { 3500 }
    }

    /// Where ffmpeg is: the configured path, next to this program, or on the PATH.
    pub fn ffmpeg(&self) -> PathBuf {
        if let Some(p) = &self.ffmpeg {
            return p.clone();
        }
        let exe = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from))
        {
            let p = dir.join(exe);
            if p.is_file() {
                return p;
            }
        }
        PathBuf::from(exe)
    }

    /// Where the stream goes, for ffmpeg: the RTMP URL with the key, or the file; several
    /// places at once through ffmpeg's tee (a failing one does not stop the others). Also what
    /// to show of it (no keys).
    fn target(&self) -> Result<(Vec<String>, String), String> {
        if let Some(f) = &self.record {
            let fmt = match f
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
            {
                Some(e) if e == "mp4" => "mp4",
                Some(e) if e == "flv" => "flv",
                _ => "matroska",
            };
            return Ok((
                vec![
                    "-f".into(),
                    fmt.into(),
                    "-y".into(),
                    f.display().to_string(),
                ],
                f.display().to_string(),
            ));
        }
        let key = self.key.trim();
        if key.is_empty() {
            return Err("Paste your stream key (YouTube Studio > Go live > Stream).".into());
        }
        let hide = |url: &str, key: &str| {
            format!(
                "{}/••••{}",
                url.trim_end_matches('/'),
                &key[key.len().saturating_sub(4)..]
            )
        };
        let url = format!("{}/{}", self.url.trim_end_matches('/'), key);
        let mut outs = vec![(url, "flv".to_string())];
        let mut shown = vec![hide(&self.url, key)];
        for d in &self.extra {
            let k = d.key.trim();
            if d.on && !k.is_empty() && !d.url.trim().is_empty() {
                outs.push((
                    format!("{}/{}", d.url.trim().trim_end_matches('/'), k),
                    "flv".into(),
                ));
                shown.push(format!("{} ({})", d.name, hide(&d.url, k)));
            }
        }
        if self.keep_copy {
            let dir = recordings_dir();
            let _ = std::fs::create_dir_all(&dir);
            let name = chrono::Local::now()
                .format("Kontinuum live %Y-%m-%d %H-%M.mkv")
                .to_string();
            let f = dir.join(name);
            shown.push(f.display().to_string());
            // forward slashes: the tee reads a backslash as an escape (Windows takes both)
            outs.push((
                f.display().to_string().replace('\\', "/"),
                "matroska".into(),
            ));
        }
        if outs.len() == 1 {
            let (url, _) = outs.remove(0);
            return Ok((vec!["-f".into(), "flv".into(), url], shown.remove(0)));
        }
        // the tee: each output in brackets of options, separated by |
        let tee = outs
            .iter()
            .map(|(u, f)| format!("[f={f}:onfail=ignore]{}", u.replace('|', "\\|")))
            .collect::<Vec<_>>()
            .join("|");
        Ok((
            vec![
                "-map".into(),
                "1:v".into(),
                "-map".into(),
                "0:a".into(),
                "-flags".into(),
                "+global_header".into(),
                "-f".into(),
                "tee".into(),
                tee,
            ],
            shown.join("  +  "),
        ))
    }
}

/// Where a stream is.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum State {
    #[default]
    Off,
    Starting,
    Live,
    /// Waiting to start again after a failure.
    Retrying(String),
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub state: State,
    /// When the current connection went live.
    pub since: Option<Instant>,
    pub frames: u64,
    pub restarts: u32,
    /// ffmpeg's last message.
    pub log: String,
    pub target: String,
}

/// A running stream.
pub struct Stream {
    stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<Status>>,
    thread: Option<std::thread::JoinHandle<rtrb::Consumer<f32>>>,
}

impl Stream {
    /// Starts streaming: `tap` is the player's tap (it comes back when the stream stops), `flag`
    /// turns it on, `snapshot` is the player's shared snapshot.
    pub fn start(
        config: Config,
        tap: rtrb::Consumer<f32>,
        flag: Arc<AtomicBool>,
        snapshot: Arc<Mutex<Snapshot>>,
    ) -> Stream {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(Status {
            state: State::Starting,
            ..Status::default()
        }));
        let (s2, st2) = (stop.clone(), status.clone());
        let thread = std::thread::Builder::new()
            .name("kontinuum-stream".into())
            .spawn(move || run(config, tap, flag, snapshot, s2, st2))
            .expect("the stream thread");
        Stream {
            stop,
            status,
            thread: Some(thread),
        }
    }

    /// Stops the stream; returns the tap.
    pub fn stop(mut self) -> Option<rtrb::Consumer<f32>> {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().and_then(|t| t.join().ok())
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

fn set(status: &Mutex<Status>, f: impl FnOnce(&mut Status)) {
    if let Ok(mut s) = status.lock() {
        f(&mut s);
    }
}

fn run(
    config: Config,
    mut tap: rtrb::Consumer<f32>,
    flag: Arc<AtomicBool>,
    snapshot: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
) -> rtrb::Consumer<f32> {
    let (w, h) = config.size();
    let mut canvas = SkiaCanvas::new(w, h);
    let mut restarts = 0;
    while !stop.load(Ordering::SeqCst) {
        set(&status, |s| {
            s.state = State::Starting;
            s.restarts = restarts;
        });
        let result = session(
            &config,
            &mut tap,
            &flag,
            &snapshot,
            &stop,
            &status,
            &mut canvas,
        );
        flag.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => break,
            Err(e) if config.record.is_some() => {
                set(&status, |s| s.state = State::Failed(e));
                break;
            }
            Err(e) => {
                restarts += 1;
                set(&status, |s| {
                    s.state = State::Retrying(e);
                    s.since = None;
                });
                // a few seconds, then again
                for _ in 0..50 {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
    }
    flag.store(false, Ordering::SeqCst);
    set(&status, |s| {
        if !matches!(s.state, State::Failed(_)) {
            s.state = State::Off;
        }
        s.since = None;
    });
    tap
}

/// One connection: ffmpeg from start to end. `Ok` when stopped, `Err` when it failed.
fn session(
    config: &Config,
    tap: &mut rtrb::Consumer<f32>,
    flag: &AtomicBool,
    snapshot: &Mutex<Snapshot>,
    stop: &AtomicBool,
    status: &Arc<Mutex<Status>>,
    canvas: &mut SkiaCanvas,
) -> Result<(), String> {
    let (target, shown) = config.target()?;
    let (w, h) = config.size();
    let fps = config.fps.clamp(15, 60);
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("audio socket: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let kbps = config.kbps();
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "warning",
        // the audio first: ffmpeg opens its inputs in order, and connects to the socket at
        // once, while the raw video on its standard input would wait for data (no
        // -thread_queue_size: recent ffmpeg refuses it on inputs)
        // raw inputs: nothing to probe (by default ffmpeg would read seconds of audio before
        // it opens the video, and both pipes would wait for each other)
        "-probesize",
        "32",
        "-analyzeduration",
        "0",
        "-f",
        "f32le",
        "-ar",
        "44100",
        "-ac",
        "2",
        "-i",
        &format!("tcp://127.0.0.1:{port}"),
        "-probesize",
        "32",
        "-analyzeduration",
        "0",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
        &format!("{w}x{h}"),
        "-r",
        &fps.to_string(),
        "-i",
        "pipe:0",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-pix_fmt",
        "yuv420p",
        "-g",
        &(fps * 2).to_string(),
        "-keyint_min",
        &(fps * 2).to_string(),
        "-b:v",
        &format!("{kbps}k"),
        "-maxrate",
        &format!("{kbps}k"),
        "-bufsize",
        &format!("{}k", kbps * 2),
        "-c:a",
        "aac",
        "-b:a",
        "160k",
        "-ar",
        "44100",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.extend(target);
    let ffmpeg = config.ffmpeg();
    let mut child: Child = Proc::new(&ffmpeg)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "ffmpeg could not start ({}): {e}. Install ffmpeg, or put ffmpeg next to \
                 Kontinuum.",
                ffmpeg.display()
            )
        })?;
    // ffmpeg's messages, the last one kept
    if let Some(err) = child.stderr.take() {
        let st = status.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
                set(&st, |s| s.log = line);
            }
        });
    }
    // ffmpeg connects for the audio when it opens its inputs
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let mut audio = loop {
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(_) if t0.elapsed() < Duration::from_secs(10) => {
                if let Ok(Some(code)) = child.try_wait() {
                    let log = status.lock().map(|s| s.log.clone()).unwrap_or_default();
                    return Err(format!("ffmpeg stopped ({code}): {log}"));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(format!("ffmpeg did not connect for the audio: {e}"));
            }
        }
    };
    let _ = audio.set_nonblocking(false);
    let _ = audio.set_nodelay(true);
    let mut video = child.stdin.take().ok_or("no ffmpeg input")?;
    // from now on the tap fills: drop what is old
    while tap.pop().is_ok() {}
    flag.store(true, Ordering::SeqCst);
    set(status, |s| {
        s.state = State::Live;
        s.since = Some(Instant::now());
        s.frames = 0;
        s.target = shown;
    });
    let per_frame = 2 * 44_100 / fps as usize;
    let mut buf: Vec<f32> = Vec::with_capacity(per_frame * 4);
    let mut bytes: Vec<u8> = Vec::with_capacity(per_frame * 4);
    let mut frames = 0u64;
    let mut vis = crate::stage::Visual::default();
    let mut quiet = Instant::now();
    let result = loop {
        if stop.load(Ordering::SeqCst) {
            break Ok(());
        }
        let n = tap.slots();
        if n > 0 {
            if let Ok(c) = tap.read_chunk(n) {
                let (a, b) = c.as_slices();
                buf.extend_from_slice(a);
                buf.extend_from_slice(b);
                c.commit_all();
            }
            quiet = Instant::now();
        } else if quiet.elapsed() > Duration::from_secs(3) {
            break Err("no sound from the audio output for 3 s".into());
        }
        if buf.len() < per_frame {
            std::thread::sleep(Duration::from_millis(4));
            continue;
        }
        // one frame of picture per 1/fps of sound
        let chunk: Vec<f32> = buf.drain(..per_frame).collect();
        bytes.clear();
        for x in &chunk {
            bytes.extend_from_slice(&x.to_le_bytes());
        }
        if let Err(e) = audio.write_all(&bytes) {
            break Err(format!("the connection dropped: {e}"));
        }
        // the picture's clock: one frame of time per frame (smooth, whatever the snapshots)
        let snap = snapshot.lock().map(|s| *s).unwrap_or_default();
        vis.update(&snap, frames as f64 / fps as f64);
        // drawing falls behind the sound (a slow computer): the last picture again, so the
        // stream keeps real time
        if buf.len() < 2 * per_frame {
            let badge = if config.record.is_some() {
                "REC"
            } else {
                "LIVE"
            };
            crate::stage::draw(
                canvas,
                &snap,
                &vis,
                Some(badge),
                &crate::scene::overlay_now(),
            );
        }
        if let Err(e) = video.write_all(canvas.rgba()) {
            break Err(format!("the connection dropped: {e}"));
        }
        frames += 1;
        if frames.is_multiple_of(fps as u64) {
            set(status, |s| s.frames = frames);
        }
    };
    flag.store(false, Ordering::SeqCst);
    drop(video);
    drop(audio);
    // ffmpeg finishes the file (or the connection) when its inputs end
    let t1 = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            _ if t1.elapsed() > Duration::from_secs(10) => {
                let _ = child.kill();
                break;
            }
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    if result.is_err() {
        let log = status.lock().map(|s| s.log.clone()).unwrap_or_default();
        return result.map_err(|e| {
            if log.is_empty() {
                e
            } else {
                format!("{e} ({log})")
            }
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use knt_engine::{CARTA, Command, Params};

    /// Records three seconds to a file with the system's ffmpeg (skipped without one): the
    /// audio callback is simulated by a thread that renders in real time.
    #[test]
    fn several_destinations_go_through_the_tee() {
        let mut c = Config {
            key: "abcd-1234-efgh-5678-ijkl".into(),
            ..Config::default()
        };
        c.extra[0].on = true;
        c.extra[0].key = "live_123".into();
        let (args, shown) = c.target().unwrap();
        assert_eq!(args[args.len() - 2], "tee");
        let tee = args.last().unwrap();
        assert!(tee.starts_with("[f=flv:onfail=ignore]rtmp://a.rtmp.youtube.com/live2/abcd"));
        assert!(tee.contains("|[f=flv:onfail=ignore]rtmp://live.twitch.tv/app/live_123"));
        assert!(!shown.contains("live_123") && !shown.contains("abcd-1234"));
        // one destination: plain flv
        c.extra[0].on = false;
        let (args, _) = c.target().unwrap();
        assert_eq!(args[0], "-f");
        assert_eq!(args[1], "flv");
    }

    #[test]
    fn schedules_cross_midnight() {
        let s = Schedule {
            on: true,
            start: "22:00".into(),
            stop: "06:30".into(),
        };
        assert!(s.within(23.0) && s.within(3.0) && !s.within(7.0) && !s.within(21.9));
        let d = Schedule {
            on: true,
            start: "09:00".into(),
            stop: "17:00".into(),
        };
        assert!(d.within(9.0) && !d.within(17.0));
        assert_eq!(parse_hm("7:5"), Some(7.0 + 5.0 / 60.0));
        assert_eq!(parse_hm("25:00"), None);
    }

    #[test]
    fn records_a_file() {
        if Proc::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("skipped: no ffmpeg on the PATH");
            return;
        }
        let (mut player, mut audio) = knt_player::Player::new(4, CARTA[3].1, Params::default());
        player.send(Command::Play(true)).unwrap();
        let tap = player.tap().unwrap();
        let flag = player.tap_flag();
        let shared = player.shared_snapshot();
        let done = Arc::new(AtomicBool::new(false));
        let d2 = done.clone();
        let cb = std::thread::spawn(move || {
            let mut buf = vec![0.0f32; 2 * 441];
            let t = Instant::now();
            let mut n = 0u64;
            while !d2.load(Ordering::SeqCst) {
                audio.render_f32(&mut buf);
                n += 441;
                // real time
                let due = Duration::from_secs_f64(n as f64 / 44_100.0);
                if let Some(wait) = due.checked_sub(t.elapsed()) {
                    std::thread::sleep(wait);
                }
            }
        });
        let out = std::env::temp_dir().join(format!("knt-stream-{}.mkv", std::process::id()));
        let config = Config {
            record: Some(out.clone()),
            ..Config::default()
        };
        let s = Stream::start(config, tap, flag, shared);
        std::thread::sleep(Duration::from_secs(
            std::env::var("KNT_RECORD_SECONDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
        ));
        let st = s.status();
        assert_eq!(st.state, State::Live, "{st:?}");
        assert!(st.frames >= 60, "{st:?}");
        assert!(s.stop().is_some());
        done.store(true, Ordering::SeqCst);
        cb.join().unwrap();
        let probe = Proc::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
            ])
            .arg(&out)
            .output();
        if let Ok(p) = probe {
            let text = String::from_utf8_lossy(&p.stdout);
            assert!(text.contains("video") && text.contains("audio"), "{text}");
        }
        assert!(std::fs::metadata(&out).unwrap().len() > 50_000);
        // KNT_KEEP_RECORDING=1 keeps the file (to look at it)
        if std::env::var_os("KNT_KEEP_RECORDING").is_none() {
            std::fs::remove_file(out).ok();
        } else {
            eprintln!("kept {}", out.display());
        }
    }
}

#[cfg(test)]
mod speed {
    use super::*;

    /// How long the stage takes to draw in software (printed; a stream needs well under 33 ms).
    #[test]
    fn stage_frame_time() {
        let mut c = SkiaCanvas::new(1280, 720);
        let s = Snapshot {
            levels: [0.2; knt_engine::LAYERS],
            gains: [1.0; knt_engine::LAYERS],
            active: [true; knt_engine::LAYERS],
            chord: knt_engine::Chord::from_notes(&[48, 55, 60, 64, 67], 0),
            ..Default::default()
        };
        let t = Instant::now();
        let mut vis = crate::stage::Visual::default();
        for k in 0..20 {
            vis.update(&s, k as f64 * 0.033);
            crate::stage::draw(
                &mut c,
                &s,
                &vis,
                Some("LIVE"),
                &crate::scene::Overlay::default(),
            );
        }
        eprintln!(
            "stage frame: {:.1} ms",
            t.elapsed().as_secs_f64() * 1000.0 / 20.0
        );
    }
}
