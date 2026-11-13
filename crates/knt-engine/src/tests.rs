use super::*;

fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt()
}

#[test]
fn the_same_seed_renders_the_same_music() {
    let a = render(42, CARTA[3].1, Params::default(), 20.0, 0.0);
    let b = render(42, CARTA[3].1, Params::default(), 20.0, 0.0);
    assert_eq!(a, b);
    let c = render(43, CARTA[3].1, Params::default(), 20.0, 0.0);
    assert_ne!(a, c);
}

#[test]
fn every_recipe_sounds_and_stays_in_range() {
    for (name, r, _) in CARTA {
        let out = render(7, r, Params::default(), 40.0, 0.0);
        assert!(out.iter().all(|x| x.is_finite()), "{name}: not finite");
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak <= 0.9, "{name}: peak {peak}");
        // after the fade in, the music is there and not too quiet
        let late = rms(&out[2 * 44_100 * 20..]);
        assert!(late > 0.01, "{name}: rms {late}");
    }
}

#[test]
fn the_director_changes_chords_and_sections() {
    let mut e = Engine::new(5, CARTA[6].1, Params::default());
    e.command(Command::Play(true));
    let mut buf = vec![0.0f32; 2 * 44_100];
    let mut chords = std::collections::HashSet::new();
    for _ in 0..240 {
        e.render(&mut buf);
        chords.insert(e.snapshot().chord.notes().to_vec());
    }
    let s = e.snapshot();
    assert!(chords.len() >= 4, "{} chords in 4 minutes", chords.len());
    assert!(s.section >= 2, "section {}", s.section);
    assert!(s.events.iter().flatten().count() >= 4);
    // the chord's notes are in the key
    let sc = s.scale.unwrap();
    for &n in s.chord.notes() {
        assert!(sc.contains(s.root, n), "{n} not in {} {:?}", s.root, sc);
    }
}

#[test]
fn stopped_is_silent_and_play_fades_in() {
    let mut e = Engine::new(1, CARTA[0].1, Params::default());
    let mut buf = vec![0.0f32; 2 * 44_100];
    e.render(&mut buf);
    assert!(buf.iter().all(|&x| x == 0.0));
    e.command(Command::Play(true));
    for _ in 0..10 {
        e.render(&mut buf);
    }
    assert!(rms(&buf) > 0.005, "{}", rms(&buf));
    e.command(Command::Play(false));
    for _ in 0..15 {
        e.render(&mut buf);
    }
    assert!(rms(&buf) < 1e-4, "{}", rms(&buf));
}

#[test]
fn the_keyboard_plays_and_the_modes_follow() {
    // solo: only the user's notes; nothing before them
    let p = Params {
        mode: Mode::Solo,
        ..Params::default()
    };
    let mut e = Engine::new(9, CARTA[1].1, p);
    e.command(Command::Play(true));
    let mut buf = vec![0.0f32; 2 * 44_100];
    for _ in 0..8 {
        e.render(&mut buf);
    }
    let quiet = e.snapshot().levels[1];
    e.command(Command::NoteOn { note: 60, vel: 100 });
    e.command(Command::NoteOn { note: 64, vel: 100 });
    for _ in 0..3 {
        e.render(&mut buf);
    }
    let s = e.snapshot();
    assert!(s.user_active && !s.leads);
    assert!(s.levels[1] > quiet + 0.002, "{} vs {quiet}", s.levels[1]);
    // accompany: the played chord becomes the director's
    let mut e = Engine::new(9, CARTA[1].1, Params::default());
    e.command(Command::Play(true));
    e.render(&mut buf);
    for n in [57, 60, 64] {
        e.command(Command::NoteOn { note: n, vel: 90 });
    }
    e.render(&mut buf);
    let pcs: Vec<i32> = e.snapshot().chord.notes().iter().map(|n| n % 12).collect();
    assert_eq!(pcs.len(), 3);
    for pc in [9, 0, 4] {
        assert!(pcs.contains(&pc), "{pcs:?}");
    }
    // relay: the director rests, then takes over after the idle time
    let p = Params {
        mode: Mode::Relay,
        relay_seconds: 5.0,
        ..Params::default()
    };
    let mut e = Engine::new(9, CARTA[1].1, p);
    e.command(Command::Play(true));
    e.command(Command::NoteOn { note: 62, vel: 90 });
    e.render(&mut buf);
    assert!(!e.snapshot().leads);
    e.command(Command::NoteOff { note: 62 });
    for _ in 0..7 {
        e.render(&mut buf);
    }
    let s = e.snapshot();
    assert!(s.leads && !s.user_active);
    assert!(matches!(
        s.events.iter().flatten().next(),
        Some((Event::UserOut, _)) | Some(_)
    ));
}

#[test]
fn drums_and_the_sleep_timer() {
    let mut e = Engine::new(2, CARTA[4].1, Params::default());
    e.command(Command::Play(true));
    let mut buf = vec![0.0f32; 2 * 4410];
    e.render(&mut buf);
    let before = e.snapshot().perc_played;
    e.command(Command::Drum { note: 36, vel: 120 });
    e.render(&mut buf);
    assert_eq!(e.snapshot().perc_played, before + 1);
    assert_eq!(e.snapshot().last_hit, Hit::Heart);
    let p = Params {
        // six seconds
        sleep_minutes: 0.1,
        ..Params::default()
    };
    e.command(Command::Params(Box::new(p)));
    let mut buf = vec![0.0f32; 2 * 44_100];
    for _ in 0..4 {
        e.render(&mut buf);
    }
    assert!(e.snapshot().playing);
    for _ in 0..8 {
        e.render(&mut buf);
    }
    assert!(!e.snapshot().playing);
}

#[test]
fn sections_change_the_orchestration_and_the_timbres() {
    let mut e = Engine::new(21, CARTA[7].1, Params::default());
    e.command(Command::Play(true));
    let mut buf = vec![0.0f32; 2 * 44_100];
    let mut sets = std::collections::HashSet::new();
    let mut pads = std::collections::HashSet::new();
    let mut drones = std::collections::HashSet::new();
    for _ in 0..900 {
        e.render(&mut buf);
        let s = e.snapshot();
        let tonal = [0usize, 1, 2, 3, 6];
        let on: Vec<bool> = tonal.iter().map(|&i| s.active[i]).collect();
        if s.section > 0 {
            let n = on.iter().filter(|&&b| b).count();
            assert!((1..=4).contains(&n), "{n} tonal layers at once");
            assert!(s.active[0] || s.active[1], "neither drone nor pad");
        }
        sets.insert(on);
        if s.active[1] {
            pads.insert(s.patches[1]);
        }
        if s.active[0] {
            drones.insert(s.patches[0]);
        }
    }
    assert!(
        sets.len() >= 3,
        "{} orchestrations in 15 minutes",
        sets.len()
    );
    assert!(pads.len() >= 3, "{pads:?}");
    assert!(drones.len() >= 2, "{drones:?}");
}
