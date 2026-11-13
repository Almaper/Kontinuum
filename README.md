<p align="center">
  <img src="assets/logo.svg" alt="Kontinuum" width="560">
</p>

# Kontinuum

Kontinuum is ambient and drone music that composes itself. Press play and a generative
**director** writes the music as it goes: it picks keys and chords, chooses which layers sound
in each section, moves the filters, the space and the density, and keeps changing for hours
without repeating itself. You can also play along with a MIDI keyboard or pads.

## What is in it

- **Seven layers:**
  - **drone:** long, low notes;
  - **pad:** slow chords;
  - **texture:** air and drops;
  - **bells:** sparse and distant;
  - **percussion:** a pulse that comes in only now and then;
  - **choir:** synthesized voices that morph between vowels;
  - **modular:** Berlin-school sequences or low-pass-gate plucks.

  Each section, the director picks two to four of them and a timbre for each (seven pads,
  from PWM strings to a wavefolder; five drones, from a deep sine to a ladder filter), so the
  music never sounds like the same piece for long.
- **The director:** it composes in sections of a few chords and can change key, bring the pulse
  or the choir in and out, and drift through the Menu. Its automations are slow random walks,
  and you set how far they move with **Evolution**.
- **The Menu:** recipes that set the mood: *Early Mist*, *Deep Focus*, *Cathedral*, *Copper
  Rain*, *Wet Forest*, *Orbit*, *Dusk* and *Drift*.
- **Seeds:** the same seed and recipe always give the same piece.
- **Playing along:** three modes.
  - **Accompany:** the director follows the chords you play.
  - **Solo:** only you play.
  - **Relay:** the director rests while you play and takes over after a while without notes.
- **The stage:** a full-screen 16:9 picture of what every layer and the director are doing,
  made to be streamed. F11 toggles it.
- **Go live:** stream the stage and the music to YouTube by yourself, without OBS: paste the
  stream key (YouTube Studio > Go live > Stream) in the LIVE window and press Go live. It
  reconnects by itself and keeps going with the window minimized; it can also record to a
  file. It uses [ffmpeg](https://ffmpeg.org): the Windows download includes it; on macOS
  `brew install ffmpeg`, on Linux your package manager's.
- **MIDI:** a keyboard plays the layer marked KEYS (Accompany, Solo or Relay), channel 10 or a
  pad controller plays the percussion, and MIDI learn ties your knobs and faders to the
  director's knobs and the layers' levels.
- **Sleep timer** and **export** of long pieces to WAV, FLAC or Ogg.

## Command line

```sh
knt menu                                         # the recipes
knt render -r 2 -s 7 -m 10 -o cathedral.flac     # 10 minutes of Cathedral, seed 7
knt play -r "deep focus"                         # play until Ctrl+C
```

## Building

Rust (see `rust-toolchain.toml`). On Linux, the ALSA headers too (`libasound2-dev`).

```sh
cargo run --release -p knt-ui        # the app
cargo run --release -p knt-cli -- menu
cargo test --workspace
```

## Roadmap

Planned next:

- your own recipes;
- recording what you play as a song;
- the music following the time of day.

## License

Kontinuum is free software under the [GNU General Public License v3.0](LICENSE)
(GPL-3.0-only). See [NOTICE](NOTICE).
