//! `knt`: Kontinuum from the command line.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use knt_engine::{CARTA, Command, Params, Recipe};

#[derive(Parser)]
#[command(
    name = "knt",
    version,
    about = "Kontinuum: ambient music that composes itself"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render music to a file (.wav, .flac or .ogg).
    Render {
        /// The output file.
        #[arg(short, long)]
        output: PathBuf,
        /// The recipe of the Menu: its number (see `knt menu`) or name.
        #[arg(short, long, default_value = "0")]
        recipe: String,
        /// The seed: the same seed and recipe give the same music.
        #[arg(short, long, default_value_t = 1)]
        seed: u64,
        /// How long, minutes.
        #[arg(short, long, default_value_t = 5.0)]
        minutes: f32,
        /// The fade out at the end, seconds.
        #[arg(long, default_value_t = 12.0)]
        fade: f32,
    },
    /// Play through the default audio device until Ctrl+C.
    Play {
        #[arg(short, long, default_value = "0")]
        recipe: String,
        #[arg(short, long)]
        seed: Option<u64>,
    },
    /// List the recipes of the Menu.
    Menu,
}

fn recipe(s: &str) -> Result<Recipe, String> {
    if let Ok(i) = s.parse::<usize>() {
        return CARTA
            .get(i)
            .map(|c| c.1)
            .ok_or_else(|| format!("no recipe {i}: the Menu has 0..{}", CARTA.len() - 1));
    }
    let low = s.to_lowercase();
    CARTA
        .iter()
        .find(|c| c.0.to_lowercase().contains(&low))
        .map(|c| c.1)
        .ok_or_else(|| format!("no recipe named {s:?} (see `knt menu`)"))
}

fn run() -> Result<(), String> {
    match Cli::parse().command {
        Cmd::Render {
            output,
            recipe: r,
            seed,
            minutes,
            fade,
        } => {
            let r = recipe(&r)?;
            if minutes.is_nan() || minutes <= 0.0 {
                return Err("--minutes must be positive".into());
            }
            eprintln!(
                "knt: rendering {minutes} min of \"{}\" (seed {seed}) to {}",
                knt_engine::recipe::name(r.id).unwrap_or("?"),
                output.display()
            );
            knt_cli::export(&output, seed, r, Params::default(), minutes, fade)
        }
        Cmd::Play { recipe: r, seed } => {
            let r = recipe(&r)?;
            let seed = seed.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(1)
            });
            let (mut player, audio) = knt_player::Player::new(seed, r, Params::default());
            player
                .send(Command::Play(true))
                .map_err(|_| "the command queue is full")?;
            let _stream =
                knt_player::output::start(audio).map_err(|e| format!("audio output: {e}"))?;
            eprintln!(
                "knt: playing \"{}\" (seed {seed}); Ctrl+C stops",
                knt_engine::recipe::name(r.id).unwrap_or("?")
            );
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let s = stop.clone();
            let _ = ctrlc::set_handler(move || s.store(true, std::sync::atomic::Ordering::SeqCst));
            let mut last = 0;
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let snap = *player.snapshot();
                if snap.section != last {
                    last = snap.section;
                    eprintln!(
                        "knt: section {} · {} {}",
                        snap.section,
                        knt_engine::note_name(snap.root),
                        snap.scale.map_or("", |s| s.name())
                    );
                }
            }
            Ok(())
        }
        Cmd::Menu => {
            for (i, (name, _, about)) in CARTA.iter().enumerate() {
                println!("{i}  {name}: {about}");
            }
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("knt: {e}");
            ExitCode::FAILURE
        }
    }
}
