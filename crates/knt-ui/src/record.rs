//! Recording what plays (the music, and what is played on a keyboard) to a WAV file, from the
//! player's tap, on its own thread.

use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub struct Recorder {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<rtrb::Consumer<f32>>>,
    pub path: PathBuf,
    pub started: std::time::Instant,
}

fn header(w: &mut impl Write, frames: u32) -> std::io::Result<()> {
    let data = frames * 4;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&2u16.to_le_bytes())?; // stereo
    w.write_all(&44_100u32.to_le_bytes())?;
    w.write_all(&(44_100u32 * 4).to_le_bytes())?;
    w.write_all(&4u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data.to_le_bytes())
}

impl Recorder {
    /// Starts recording the tap to `path`; `flag` turns the tap on.
    pub fn start(
        mut tap: rtrb::Consumer<f32>,
        flag: Arc<AtomicBool>,
        path: PathBuf,
    ) -> Result<Recorder, String> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let mut file =
            std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        header(&mut file, 0).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        // what was in the tap before is old: dropped
        while tap.pop().is_ok() {}
        flag.store(true, Ordering::SeqCst);
        let thread = std::thread::Builder::new()
            .name("kontinuum-record".into())
            .spawn(move || {
                let mut out = std::io::BufWriter::new(file);
                let mut frames: u64 = 0;
                let mut buf: Vec<u8> = Vec::new();
                while !s2.load(Ordering::SeqCst) {
                    let n = tap.slots();
                    if n == 0 {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    if let Ok(c) = tap.read_chunk(n) {
                        buf.clear();
                        let (a, b) = c.as_slices();
                        for x in a.iter().chain(b) {
                            let v = (x.clamp(-1.0, 1.0) * 32767.0) as i16;
                            buf.extend_from_slice(&v.to_le_bytes());
                        }
                        c.commit_all();
                        frames += (n / 2) as u64;
                        let _ = out.write_all(&buf);
                    }
                }
                flag.store(false, Ordering::SeqCst);
                if let Ok(mut f) = out.into_inner() {
                    let frames = frames.min(u32::MAX as u64 / 4 - 9) as u32;
                    let _ = f
                        .seek(SeekFrom::Start(0))
                        .and_then(|_| header(&mut f, frames));
                }
                tap
            })
            .map_err(|e| e.to_string())?;
        Ok(Recorder {
            stop,
            thread: Some(thread),
            path,
            started: std::time::Instant::now(),
        })
    }

    /// Stops; gives the tap back.
    pub fn stop(mut self) -> Option<rtrb::Consumer<f32>> {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().and_then(|t| t.join().ok())
    }
}
