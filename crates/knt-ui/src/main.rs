//! Kontinuum: the app.
//!
//! ```text
//! Kontinuum [--seed N] [--recipe N] [--play] [--stage] [--size WxH] [--screenshot OUT.png]
//! ```

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod stage;
mod theme;

use std::path::PathBuf;

use eframe::egui;
use knt_engine::{CARTA, Params};

struct Kontinuum {
    app: app::App,
}

impl eframe::App for Kontinuum {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.show(ui);
    }
}

fn main() -> eframe::Result {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() % 100_000)
        .unwrap_or(1);
    let mut recipe = 7usize;
    let mut play = false;
    let mut stage = false;
    let mut size = [app::W, app::H];
    let mut shot: Option<PathBuf> = None;
    let mut warmup = 0u32;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--seed" => seed = args.next().and_then(|v| v.parse().ok()).unwrap_or(seed),
            "--recipe" => recipe = args.next().and_then(|v| v.parse().ok()).unwrap_or(recipe),
            "--play" => play = true,
            "--stage" => stage = true,
            "--size" => {
                if let Some((w, h)) = args.next().as_deref().and_then(|v| v.split_once('x')) {
                    size = [w.parse().unwrap_or(size[0]), h.parse().unwrap_or(size[1])];
                }
            }
            "--screenshot" => shot = args.next().map(PathBuf::from),
            // seconds of music before the screenshot
            "--warmup" => warmup = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            _ => {
                eprintln!(
                    "usage: Kontinuum [--seed N] [--recipe 0..{}] [--play] [--stage] \
                     [--size WxH] [--screenshot OUT.png]",
                    CARTA.len() - 1
                );
                std::process::exit(2);
            }
        }
    }
    let recipe = recipe.min(CARTA.len() - 1);
    let (mut player, audio) = knt_player::Player::new(seed, CARTA[recipe].1, Params::default());
    // a warm-up renders the music ahead (for screenshots of a piece that has started)
    let mut audio = audio;
    if warmup > 0 {
        let _ = player.send(knt_engine::Command::Play(true));
        let mut buf = vec![0.0f32; 2 * 1024];
        for _ in 0..(warmup as usize * 44_100 / 1024) {
            audio.render_f32(&mut buf);
        }
    }
    let stream = match knt_player::output::start(audio) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("Kontinuum: audio output: {e}; running without sound");
            None
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kontinuum")
            .with_app_id("Kontinuum")
            .with_inner_size(size)
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Kontinuum",
        options,
        Box::new(move |cc| {
            theme::install(&cc.egui_ctx);
            let _keep = &stream;
            let mut a = app::App::new(Some(player), seed);
            a.recipe = recipe;
            if play || warmup > 0 {
                a.set_playing(true);
            }
            a.stage = stage;
            a.shot = shot.map(|path| app::Shot {
                path,
                frames: 0,
                after: 12,
            });
            Ok(Box::new(Kontinuum { app: a }))
        }),
    )
}
