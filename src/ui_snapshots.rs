//! Headless screenshots of every major screen, rendered through wgpu.
//!
//! Run with:
//! `PIGPAL_SNAPSHOT_DIR=/tmp/shots PIGPAL_SNAPSHOT_DATA=/path/to/copy cargo test --release snapshots -- --ignored --nocapture`
//!
//! `PIGPAL_SNAPSHOT_DATA` must point at a *copy* of a data directory; the
//! harness copies it again into a temporary folder so originals are never touched.
use super::*;
use egui_kittest::Harness;
use std::path::Path;

fn copy_dir(from: &Path, to: &Path) {
    for entry in walkdir::WalkDir::new(from)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let target = to.join(entry.path().strip_prefix(from).unwrap());
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn harness(root: Root, size: Vec2) -> Harness<'static, Desktop> {
    Harness::builder()
        .wgpu()
        .with_size(size)
        .with_pixels_per_point(2.0)
        .with_max_steps(8)
        .build_eframe(move |cc| Desktop::new(cc, root))
}

fn shoot(h: &mut Harness<'_, Desktop>, dir: &Path, name: &str) {
    for _ in 0..4 {
        h.step();
    }
    let image = h.render().expect("render");
    image.save(dir.join(format!("{name}.png"))).unwrap();
    println!("saved {name}");
}

fn settle(h: &mut Harness<'_, Desktop>, until: impl Fn(&Desktop) -> bool) {
    for _ in 0..400 {
        h.step();
        if until(h.state()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
#[ignore = "writes PNG screenshots; run explicitly"]
fn snapshots() {
    let out = PathBuf::from(std::env::var("PIGPAL_SNAPSHOT_DIR").expect("PIGPAL_SNAPSHOT_DIR"));
    std::fs::create_dir_all(&out).unwrap();
    let temp = tempfile::tempdir().unwrap();
    if let Ok(data) = std::env::var("PIGPAL_SNAPSHOT_DATA") {
        copy_dir(Path::new(&data), temp.path());
    }
    let size = Vec2::new(1280.0, 820.0);
    let root = Root::open(temp.path().to_path_buf()).unwrap();
    let mut h = harness(root, size);
    shoot(&mut h, &out, "01_new_session");

    // Saved session review: prefer the richest real capture.
    let pick = {
        let d = h.state();
        d.sessions
            .iter()
            .find(|s| s.subject == "pig 6")
            .or_else(|| d.sessions.first())
            .map(|s| s.id.clone())
    };
    if let Some(id) = pick.clone() {
        h.state_mut().select(id.clone());
        shoot(&mut h, &out, "02_review");
        h.state_mut().open_replay(id, None);
        settle(&mut h, |d| d.replay.is_some() && d.job.is_none());
        if let Some(r) = h.state_mut().replay.as_mut() {
            // Land on the second note so the banner, list and ticks all show.
            r.step_note(true);
            r.step_note(true);
        }
        shoot(&mut h, &out, "03_replay");
        theme::set_appearance(&h.ctx, Appearance::Light);
        shoot(&mut h, &out, "07_replay_light");
        h.state_mut().replay = None;
        shoot(&mut h, &out, "06_review_light");
        theme::set_appearance(&h.ctx, Appearance::Dark);
    }
    h.state_mut().open_settings(SettingsTab::Connection);
    shoot(&mut h, &out, "04_settings");
    h.state_mut().settings_open = false;

    // Live demo capture in an isolated, empty data directory.
    let demo = tempfile::tempdir().unwrap();
    let root = Root::open(demo.path().to_path_buf()).unwrap();
    let mut h = harness(root, size);
    theme::set_appearance(&h.ctx, Appearance::Light);
    {
        let d = h.state_mut();
        d.settings.source.mode = Source::Demo;
        d.subject = "Pig 7".into();
        d.title = "Baseline anesthesia".into();
        d.authorized = true;
        d.start(None);
    }
    settle(&mut h, |d| {
        d.recorder.as_ref().is_some_and(|r| r.latest.len() >= 4)
    });
    for _ in 0..40 {
        h.step();
        std::thread::sleep(Duration::from_millis(50));
    }
    {
        let d = h.state_mut();
        d.reclaim();
        if let Some(rec) = d.recorder.as_mut() {
            let _ = rec.marker("Baseline", "", None);
            let _ = rec.marker("Note", "Induction complete, stable vitals", None);
        }
    }
    shoot(&mut h, &out, "05_live_light");
    theme::set_appearance(&h.ctx, Appearance::Dark);
    shoot(&mut h, &out, "05_live");
    h.state_mut().stop(false);
}

/// Frames for the README animation: pig 6 replay playing through a note.
/// `PIGPAL_GIF_DIR=/tmp/frames PIGPAL_SNAPSHOT_DATA=… cargo test --release --lib replay_gif -- --ignored`
#[test]
#[ignore = "writes PNG frames; run explicitly"]
fn replay_gif_frames() {
    let out = PathBuf::from(std::env::var("PIGPAL_GIF_DIR").expect("PIGPAL_GIF_DIR"));
    std::fs::create_dir_all(&out).unwrap();
    let temp = tempfile::tempdir().unwrap();
    copy_dir(Path::new(&std::env::var("PIGPAL_SNAPSHOT_DATA").unwrap()), temp.path());
    let root = Root::open(temp.path().to_path_buf()).unwrap();
    let mut h = Harness::builder()
        .wgpu()
        .with_size(Vec2::new(1280.0, 800.0))
        .with_pixels_per_point(1.5)
        .with_max_steps(8)
        .build_eframe(move |cc| Desktop::new(cc, root));
    theme::set_appearance(&h.ctx, Appearance::Dark);
    let id = h
        .state()
        .sessions
        .iter()
        .find(|s| s.subject == "pig 6")
        .map(|s| s.id.clone())
        .expect("pig 6 session");
    h.state_mut().open_replay(id, None);
    settle(&mut h, |d| d.replay.is_some() && d.job.is_none());
    let start: chrono::DateTime<chrono::Utc> = h
        .state()
        .replay
        .as_ref()
        .unwrap()
        .notes()
        .iter()
        .find(|n| n.text.contains("carotid"))
        .map(|n| n.at)
        .unwrap()
        - chrono::Duration::milliseconds(4000);
    for i in 0..90 {
        let at = start + chrono::Duration::milliseconds(i * 100);
        {
            let d = h.state_mut();
            d.toasts.clear();
            d.replay.as_mut().unwrap().freeze_at(at, true);
        }
        h.step();
        h.step();
        let image = h.render().expect("render");
        image.save(out.join(format!("frame_{i:03}.png"))).unwrap();
    }
}
