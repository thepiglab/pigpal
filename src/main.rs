use anyhow::{Context, Result};
use pigpal::{
    audio, export, model, recorder::Recorder, store::Root, transport::SourceConfig, ui::Desktop,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--print-data-dir") {
        println!("{}", Root::default_path().display());
        return Ok(());
    }
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if let Some(path) = value("--export-waveline") {
        let output =
            value("--output-dir").context("--export-waveline requires a new --output-dir")?;
        let capture = pigpal::replay::Capture::load(&PathBuf::from(path))?;
        let output = PathBuf::from(output);
        std::fs::create_dir(&output).context(
            "Use a new output directory; existing files are never replaced by this command",
        )?;
        pigpal::waveline_csv::write_capture(&capture, &output)?;
        println!(
            "{}",
            serde_json::json!({"directory":output,"records":capture.frames.len(),"warnings":capture.warnings})
        );
        return Ok(());
    }
    if let Some(path) = value("--inspect-waveline") {
        let path = PathBuf::from(path);
        let inspection = if path.is_dir() {
            pigpal::waveline::inspect_session(&path)?
        } else {
            pigpal::waveline::inspect_file(&path)?
        };
        println!("{}", serde_json::to_string_pretty(&inspection)?);
        return Ok(());
    }
    if let Some(path) = value("--inspect-replay") {
        let mut player =
            pigpal::replay::Player::new(pigpal::replay::Capture::load(&PathBuf::from(path))?);
        if let Some(at) = value("--replay-at") {
            player.seek(pigpal::replay::parse_time(&at, player.cursor)?)?;
        }
        let frame = player.frame();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "records":player.capture.frames.len(),"cursor_utc":player.cursor,
                "record_index":player.index,"record_age_s":player.age(),
                "display_has_data":player.age().is_some_and(|age| age<=pigpal::waveline::STALE_AFTER_S),
                "received_utc":frame.map(|f|f.utc),
                "timestamp_basis":frame.map(|f|&f.timestamp_basis),
                "record":frame.map(|f|&f.record),"warnings":player.capture.warnings,
            }))?
        );
        return Ok(());
    }
    let root = Root::open(
        value("--data-dir")
            .map(PathBuf::from)
            .unwrap_or_else(Root::default_path),
    )?;
    if args.iter().any(|a| a == "--download-model") {
        return model::download(&root.path, |p| println!("{p}"));
    }
    if let Some(id) = value("--transcribe") {
        let n = model::transcribe_session(&root.path, &root.session_dir(&id), |p| println!("{p}"))?;
        println!("Saved {n} transcripts");
        return Ok(());
    }
    if let Some(seconds) = value("--demo-seconds") {
        let seconds = seconds
            .parse::<f64>()
            .context("--demo-seconds expects a number")?;
        anyhow::ensure!(
            seconds.is_finite() && (0.1..=86400.0).contains(&seconds),
            "Demo duration must be between 0.1 and 86400 seconds"
        );
        let session = root.create("SYNTHETIC DESKTOP TEST", "Desktop acceptance", true)?;
        let mut rec = Recorder::start(&root, session, SourceConfig::default())?;
        let marker = rec.marker("Baseline", "", None)?;
        let start = Instant::now();
        while start.elapsed().as_secs_f64() < seconds {
            rec.poll()?;
            std::thread::sleep(Duration::from_millis(20));
        }
        rec.marker("Note", "Synthetic desktop acceptance note", Some(&marker))?;
        if let Some(fixture) = value("--audio-fixture") {
            let clip = audio::import_wav(
                &rec.journal.dir,
                &PathBuf::from(fixture),
                rec.journal.elapsed(),
            )?;
            rec.event("audio_closed", serde_json::to_value(clip)?)?;
        }
        rec.stop(&root, false)?;
        let dir = root.session_dir(&rec.session.id);
        export::archive(
            &dir,
            &root
                .path
                .join("exports")
                .join(format!("{}.zip", rec.session.id)),
            true,
        )?;
        println!(
            "{}",
            serde_json::json!({"session_id":rec.session.id,"directory":dir,"messages":rec.messages,"bytes":rec.bytes})
        );
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Pig Pal")
            .with_inner_size([1320.0, 860.0])
            .with_min_inner_size([940.0, 680.0]),
        // Native Metal / DX12 / Vulkan through wgpu instead of OpenGL, which
        // macOS has deprecated. Low-power GPU selection keeps laptops cool.
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(
                eframe::egui_wgpu::WgpuSetupCreateNew {
                    power_preference: eframe::wgpu::PowerPreference::LowPower,
                    ..Default::default()
                },
            ),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "Pig Pal",
        options,
        Box::new(move |cc| {
            let mut desktop = Desktop::new(cc, root);
            if let Some(index) = args.iter().position(|arg| arg == "--replay") {
                if let Some(id) = args.get(index + 1) {
                    let at = args
                        .iter()
                        .position(|arg| arg == "--replay-at")
                        .and_then(|i| args.get(i + 1))
                        .cloned();
                    desktop.open_replay(id.clone(), at);
                }
            }
            Ok(Box::new(desktop))
        }),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
