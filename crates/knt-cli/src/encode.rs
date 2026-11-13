//! The output encoders: WAV, FLAC and Ogg Vorbis.

use std::error::Error;
use std::num::{NonZeroU8, NonZeroU32};

use flacenc::component::BitRepr;
use flacenc::error::Verify;
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

pub const SAMPLE_RATE: u32 = 44100;

/// The Ogg stream serial of every .ogg file Kontinuum writes ("KN"; fixed, so the same music
/// gives the same file).
const OGG_SERIAL: i32 = 0x4B4E;

/// The container chosen by the output file's extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Wav,
    Flac,
    Ogg,
}

impl Format {
    pub fn from_extension(ext: Option<&str>) -> Option<Format> {
        match ext?.to_ascii_lowercase().as_str() {
            "wav" => Some(Format::Wav),
            "flac" => Some(Format::Flac),
            "ogg" => Some(Format::Ogg),
            _ => None,
        }
    }
}

/// Encodes interleaved 16-bit stereo samples.
pub fn encode(format: Format, samples: &[i16], quality: f32) -> Result<Vec<u8>, Box<dyn Error>> {
    match format {
        Format::Wav => Ok(wav(samples)),
        Format::Flac => flac(samples),
        Format::Ogg => ogg(samples, quality),
    }
}

/// A 16-bit stereo 44.1 kHz WAV file.
pub fn wav(samples: &[i16]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// f32 samples (-1..1) to 16-bit, with a light triangular dither.
pub fn to_i16(samples: &[f32]) -> Vec<i16> {
    let mut seed = 0x2545_F491u32;
    let mut noise = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    samples
        .iter()
        .map(|&x| {
            let d = noise() - noise();
            (x * 32_767.0 + d).round().clamp(-32_768.0, 32_767.0) as i16
        })
        .collect()
}

fn flac(samples: &[i16]) -> Result<Vec<u8>, Box<dyn Error>> {
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| format!("FLAC encoder configuration: {e:?}"))?;
    let pcm: Vec<i32> = samples.iter().map(|&s| s as i32).collect();
    let source = flacenc::source::MemSource::from_samples(&pcm, 2, 16, SAMPLE_RATE as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| format!("FLAC encoding failed: {e:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| format!("FLAC output failed: {e:?}"))?;
    Ok(sink.as_slice().to_vec())
}

fn ogg(samples: &[i16], quality: f32) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut out = Vec::new();
    // A fixed stream serial, so that the same samples give the same file.
    let mut enc = VorbisEncoderBuilder::new_with_serial(
        NonZeroU32::new(SAMPLE_RATE).unwrap(),
        NonZeroU8::new(2).unwrap(),
        &mut out,
        OGG_SERIAL,
    )
    .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
        target_quality: quality,
    })
    .build()?;
    for chunk in samples.chunks(2 * 4096) {
        let l: Vec<f32> = chunk
            .iter()
            .step_by(2)
            .map(|&s| s as f32 / 32768.0)
            .collect();
        let r: Vec<f32> = chunk
            .iter()
            .skip(1)
            .step_by(2)
            .map(|&s| s as f32 / 32768.0)
            .collect();
        enc.encode_audio_block([&l[..], &r[..]])?;
    }
    enc.finish()?;
    Ok(out)
}
