//! What `knt` and the app share: the encoders and the offline export.

pub mod encode;

use std::path::Path;

use knt_engine::{Params, Recipe};

pub use encode::Format;

/// Renders `minutes` of music from `seed` and `recipe` (fading out over the last `fade` seconds)
/// and writes it to `path`, encoded by its extension (.wav, .flac or .ogg).
pub fn export(
    path: &Path,
    seed: u64,
    recipe: Recipe,
    params: Params,
    minutes: f32,
    fade: f32,
) -> Result<(), String> {
    let format =
        Format::from_extension(path.extension().and_then(|e| e.to_str())).ok_or_else(|| {
            format!(
                "{}: the extension must be .wav, .flac or .ogg",
                path.display()
            )
        })?;
    let audio = knt_engine::render(seed, recipe, params, minutes * 60.0, fade);
    let pcm = encode::to_i16(&audio);
    let bytes = encode::encode(format, &pcm, 0.6).map_err(|e| format!("encoding: {e}"))?;
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_every_format() {
        let dir = std::env::temp_dir().join(format!("knt-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = knt_engine::CARTA[0].1;
        for ext in ["wav", "flac", "ogg"] {
            let p = dir.join(format!("a.{ext}"));
            export(&p, 1, r, Params::default(), 0.05, 1.0).unwrap();
            assert!(std::fs::metadata(&p).unwrap().len() > 1000, "{ext}");
        }
        let wav = std::fs::read(dir.join("a.wav")).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        // 3 seconds of 16-bit stereo
        assert_eq!(wav.len(), 44 + 3 * 44_100 * 4);
        assert!(export(&dir.join("a.mp3"), 1, r, Params::default(), 0.01, 0.0).is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
