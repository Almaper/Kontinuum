//! The real-time player: the engine in an audio callback, driven by commands from the UI thread.
//!
//! [`Player::new`] gives two halves that share only lock-free single-producer single-consumer
//! channels, all allocated up front: the commands, the boxes the callback hands back to be
//! dropped (it frees nothing), the [`Snapshot`] (a triple buffer), and the tap: a copy of every
//! sample played, for the live stream ([`Player::tap`]).

pub mod output;

use knt_engine::{Command, Engine, Params, Recipe, Snapshot};

/// The output rate.
pub const RATE: u32 = knt_engine::RATE;
/// The frames the callback renders at a time (the resampler's chunk).
pub const CALL_FRAMES: usize = 1024;
/// The command queue's capacity.
pub const COMMANDS: usize = 1024;
/// The garbage queue's capacity.
pub const GARBAGE: usize = 64;
/// The tap's capacity: ten seconds of stereo samples.
pub const TAP: usize = 2 * 10 * RATE as usize;

/// The UI side.
pub struct Player {
    commands: rtrb::Producer<Command>,
    garbage: rtrb::Consumer<Command>,
    snapshot: triple_buffer::Output<Snapshot>,
    tap: Option<rtrb::Consumer<f32>>,
    midi: Option<rtrb::Producer<Command>>,
    shared: std::sync::Arc<std::sync::Mutex<Snapshot>>,
    tapping: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// The callback side.
pub struct Audio {
    engine: Engine,
    commands: rtrb::Consumer<Command>,
    /// The MIDI input's own queue (notes straight from the MIDI thread, no UI frame between).
    midi: rtrb::Consumer<Command>,
    garbage: rtrb::Producer<Command>,
    snapshot: triple_buffer::Input<Snapshot>,
    tap: rtrb::Producer<f32>,
    /// A copy of the snapshot for other threads (the live stream), updated when free.
    shared: std::sync::Arc<std::sync::Mutex<Snapshot>>,
    /// Whether the tap is read (a stream runs): when not, it is not filled.
    tapping: std::sync::Arc<std::sync::atomic::AtomicBool>,
    output_errors: u64,
    since_publish: usize,
}

impl Player {
    pub fn new(seed: u64, recipe: Recipe, params: Params) -> (Player, Audio) {
        let engine = Engine::new(seed, recipe, params);
        let (cp, cc) = rtrb::RingBuffer::new(COMMANDS);
        let (gp, gc) = rtrb::RingBuffer::new(GARBAGE);
        let (si, so) = triple_buffer::triple_buffer(&engine.snapshot());
        let (tp, tc) = rtrb::RingBuffer::new(TAP);
        let tapping = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (mp, mc) = rtrb::RingBuffer::new(COMMANDS);
        let shared = std::sync::Arc::new(std::sync::Mutex::new(engine.snapshot()));
        (
            Player {
                commands: cp,
                garbage: gc,
                snapshot: so,
                tap: Some(tc),
                midi: Some(mp),
                shared: shared.clone(),
                tapping: tapping.clone(),
            },
            Audio {
                engine,
                commands: cc,
                midi: mc,
                garbage: gp,
                snapshot: si,
                tap: tp,
                shared,
                tapping,
                output_errors: 0,
                since_publish: 0,
            },
        )
    }

    /// Queues a command; a full queue gives it back.
    pub fn send(&mut self, c: Command) -> Result<(), Command> {
        self.collect_garbage();
        self.commands.push(c).map_err(|rtrb::PushError::Full(c)| c)
    }

    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }

    /// The latest state.
    pub fn snapshot(&mut self) -> &Snapshot {
        self.snapshot.read()
    }

    /// The snapshot for another thread (the live stream): the latest the callback could store.
    pub fn shared_snapshot(&self) -> std::sync::Arc<std::sync::Mutex<Snapshot>> {
        self.shared.clone()
    }

    /// The flag that turns the tap on.
    pub fn tap_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.tapping.clone()
    }

    /// The MIDI queue's sending end (once): for the MIDI input's thread. Only boxless commands
    /// (notes, drums) belong in it.
    pub fn midi_sender(&mut self) -> Option<rtrb::Producer<Command>> {
        self.midi.take()
    }

    /// The tap's reading end (once): every sample the callback plays while
    /// [`Audio::tap_flag`] is set, interleaved stereo at [`RATE`].
    pub fn tap(&mut self) -> Option<rtrb::Consumer<f32>> {
        self.tap.take()
    }
}

impl Audio {
    /// The flag that turns the tap on (shared with the stream's thread).
    pub fn tap_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.tapping.clone()
    }

    /// The callback's body: interleaved stereo at 44.1 kHz.
    pub fn render_f32(&mut self, out: &mut [f32]) {
        while self.garbage.slots() > 0 {
            let Ok(c) = self.commands.pop() else { break };
            if let Some(old) = self.engine.command(c) {
                let _ = self.garbage.push(old);
            }
        }
        while let Ok(c) = self.midi.pop() {
            if let Some(old) = self.engine.command(c) {
                let _ = self.garbage.push(old);
            }
        }
        self.engine.render(out);
        if self.tapping.load(std::sync::atomic::Ordering::Relaxed) {
            // a stream that falls behind loses samples rather than stalling the sound
            let n = self.tap.slots().min(out.len()) & !1;
            if let Ok(chunk) = self.tap.write_chunk_uninit(n) {
                chunk.fill_from_iter(out[..n].iter().copied());
            }
        }
        self.since_publish += out.len() / 2;
        // the snapshot about 40 times a second
        if self.since_publish >= 1024 {
            self.since_publish = 0;
            let snap = self.engine.snapshot();
            *self.snapshot.input_buffer_mut() = snap;
            self.snapshot.publish();
            // never wait in the callback: when the stream's thread holds it, next time
            if let Ok(mut s) = self.shared.try_lock() {
                *s = snap;
            }
        }
    }

    /// A device callback output silence because the resampler failed.
    pub(crate) fn output_error(&mut self) {
        self.output_errors += 1;
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knt_engine::CARTA;

    #[test]
    fn commands_reach_the_engine_and_the_tap_copies() {
        let (mut p, mut a) = Player::new(3, CARTA[2].1, Params::default());
        let mut tap = p.tap().unwrap();
        a.tap_flag()
            .store(true, std::sync::atomic::Ordering::Relaxed);
        p.send(Command::Play(true)).unwrap();
        let mut buf = vec![0.0f32; 2 * CALL_FRAMES];
        for _ in 0..200 {
            a.render_f32(&mut buf);
        }
        assert!(p.snapshot().playing);
        let mut got = vec![0.0f32; buf.len()];
        // the last call's samples are the last in the tap
        let n = tap.slots();
        assert!(n >= buf.len());
        let skip = n - buf.len();
        tap.read_chunk(skip).unwrap().commit_all();
        let c = tap.read_chunk(buf.len()).unwrap();
        let (x, y) = c.as_slices();
        got[..x.len()].copy_from_slice(x);
        got[x.len()..].copy_from_slice(y);
        c.commit_all();
        assert_eq!(got, buf);
        let params = Params {
            volume: 0.1,
            ..Params::default()
        };
        p.send(Command::Params(Box::new(params))).unwrap();
        a.render_f32(&mut buf);
        assert_eq!(a.engine().params.volume, 0.1);
    }
}
