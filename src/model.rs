use crate::{
    audio::Clip,
    store::{self, Session},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::Duration,
};

const REVISION: &str = "741158ae71e64ef5c89385862c18f777d07a97a1";
const SHA256: &str = "574614b9a4d9f72ab202877a7ad6a1f4bf819dd1d27b9d42dfe8cd429fcebdd5";
pub fn native_mac() -> bool {
    cfg!(target_os = "macos")
        && std::env::var("PIGPAL_SPEECH_ENGINE")
            .or_else(|_| std::env::var("SURGERYBUDDY_SPEECH_ENGINE"))
            .as_deref()
            != Ok("portable")
}
pub fn engine_name() -> &'static str {
    if native_mac() {
        if cfg!(target_arch = "aarch64") {
            "Redux • Apple Core ML / Metal"
        } else {
            "Redux • Core ML / CPU"
        }
    } else {
        "Redux • portable CPU"
    }
}
pub fn model_dir(root: &Path) -> PathBuf {
    root.join("models").join(if native_mac() {
        "parakeet-redux"
    } else {
        "redux-gguf"
    })
}
pub fn installed(root: &Path) -> bool {
    model_dir(root).join("ready.json").is_file()
}
pub fn executable(name: &str) -> Result<PathBuf> {
    let name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let exe = std::env::current_exe()?;
    for base in [
        exe.parent().unwrap().to_path_buf(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin"),
    ] {
        let path = base.join(&name);
        if path.is_file() {
            return Ok(path);
        }
    }
    anyhow::bail!(
        "Speech engine {name} is missing. Run tools/build-speech.py, or use the packaged app."
    )
}
pub fn download(root: &Path, progress: impl Fn(String)) -> Result<()> {
    if installed(root) {
        progress("Redux is already downloaded for every session.".into());
        return Ok(());
    }
    let dir = model_dir(root);
    fs::create_dir_all(&dir)?;
    if native_mac() {
        progress("Downloading and compiling Redux for this Mac. First preparation can take several minutes.".into());
        let output = run_helper(
            Command::new(executable("pigpal-speech-macos")?)
                .arg("download")
                .arg(&dir),
            1200,
        )?;
        ensure!(
            output.status.success(),
            "Core ML preparation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else {
        let _ = executable("parakeet-cli")?;
        let url = format!(
            "https://huggingface.co/mudler/parakeet-cpp-gguf/resolve/{REVISION}/redux-packed.gguf"
        );
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(1800))
            .build()?;
        let mut response = client.get(url).send()?.error_for_status()?;
        let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
        let mut hash = Sha256::new();
        let mut bytes = 0;
        let mut buffer = [0; 128 * 1024];
        let mut last = 0;
        loop {
            let n = response.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            temp.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
            bytes += n;
            if bytes - last > 4 * 1024 * 1024 {
                progress(format!("Downloading Redux: {} / 213 MB", bytes / 1_000_000));
                last = bytes;
            }
        }
        ensure!(
            format!("{:x}", hash.finalize()) == SHA256,
            "Model checksum mismatch; incomplete download was not installed"
        );
        temp.as_file().sync_all()?;
        temp.persist(dir.join("redux-packed.gguf"))
            .map_err(|e| e.error)?;
    }
    store::write_json(
        &dir.join("ready.json"),
        &json!({"engine":engine_name(),"prepared":store::utc(),"model":"moondream/parakeet-redux","portable_revision":REVISION,"portable_sha256":SHA256}),
    )?;
    progress("Redux downloaded. All sessions can now transcribe offline.".into());
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub confidence: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub id: String,
    pub clip_id: String,
    pub audio_file: String,
    pub audio_started: chrono::DateTime<chrono::Utc>,
    pub segment_elapsed_s: f64,
    pub generated: chrono::DateTime<chrono::Utc>,
    pub engine: String,
    pub model: String,
    pub text: String,
    pub words: Vec<Word>,
    pub timings_valid: bool,
    pub raw: Value,
}
pub fn transcripts(session_dir: &Path) -> Result<Vec<Transcript>> {
    let mut result: Vec<Transcript> = Vec::new();
    for entry in walkdir::WalkDir::new(session_dir.join("segments")).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .ends_with(".transcript.json")
        {
            result.push(store::read_json(entry.path())?);
        }
    }
    result.sort_by_key(|t| t.audio_started);
    Ok(result)
}
pub fn transcribe_session(
    root: &Path,
    session_dir: &Path,
    progress: impl Fn(String),
) -> Result<usize> {
    let session: Session = store::read_json(&session_dir.join("session.json"))?;
    ensure!(
        session.state != "recording",
        "Stop and save before transcription"
    );
    ensure!(installed(root), "Download Redux once in Settings first");
    let mut count = 0;
    for segment in session.segments {
        let dir = session_dir.join("segments").join(segment.id);
        let mut clips = fs::read_dir(dir.join("audio"))?.collect::<std::io::Result<Vec<_>>>()?;
        clips.sort_by_key(|e| e.file_name());
        for entry in clips {
            if !entry.file_name().to_string_lossy().ends_with(".wav.json") {
                continue;
            }
            let clip: Clip = store::read_json(&entry.path())?;
            if clip.state != "closed" || clip.duration_s < 0.3 {
                continue;
            }
            let dest = dir
                .join("transcripts")
                .join(format!("{}.transcript.json", clip.id));
            if dest.is_file() {
                continue;
            }
            ensure!(
                clip.duration_s <= 30.1,
                "Audio clip exceeds the 30-second memory bound"
            );
            progress(format!("Transcribing clip {} locally…", count + 1));
            let audio = dir.join(&clip.file);
            let raw = if native_mac() {
                let temporary = tempfile::NamedTempFile::new_in(dir.join("transcripts"))?;
                let output = run_helper(
                    Command::new(executable("pigpal-speech-macos")?)
                        .arg("transcribe")
                        .arg(model_dir(root))
                        .arg(&audio)
                        .arg(temporary.path()),
                    180,
                )?;
                ensure!(
                    output.status.success(),
                    "Core ML transcription failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                store::read_json::<Value>(temporary.path())?
            } else {
                let output = run_helper(
                    Command::new(executable("parakeet-cli")?)
                        .args(["transcribe", "--model"])
                        .arg(model_dir(root).join("redux-packed.gguf"))
                        .arg("--input")
                        .arg(&audio)
                        .arg("--json")
                        .env("PARAKEET_DEVICE", "cpu"),
                    180,
                )?;
                ensure!(
                    output.status.success(),
                    "Redux transcription failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                serde_json::from_slice::<Value>(&output.stdout)
                    .context("Speech engine returned invalid JSON")?
            };
            let text = raw
                .get("text")
                .and_then(Value::as_str)
                .context("Missing transcript text")?
                .to_string();
            let words = raw
                .get("words")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|w| Word {
                            text: w
                                .get("text")
                                .or_else(|| w.get("w"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .into(),
                            start: w.get("start").and_then(Value::as_f64).unwrap_or(-1.0),
                            end: w.get("end").and_then(Value::as_f64).unwrap_or(-1.0),
                            confidence: w
                                .get("confidence")
                                .or_else(|| w.get("conf"))
                                .and_then(Value::as_f64),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let timings_valid = words
                .iter()
                .all(|w| w.start >= 0.0 && w.end >= w.start && w.end <= clip.duration_s);
            let transcript = Transcript {
                id: store::id(),
                clip_id: clip.id,
                audio_file: clip.file,
                audio_started: clip.started,
                segment_elapsed_s: clip.segment_elapsed_s,
                generated: store::utc(),
                engine: engine_name().into(),
                model: "moondream/parakeet-redux".into(),
                text,
                words,
                timings_valid,
                raw,
            };
            store::write_json(&dest, &transcript)?;
            count += 1;
        }
    }
    Ok(count)
}
pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn run_helper(command: &mut Command, seconds: u64) -> Result<Output> {
    let mut out = tempfile::tempfile()?;
    let mut err = tempfile::tempfile()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(out.try_clone()?))
        .stderr(Stdio::from(err.try_clone()?))
        .spawn()?;
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(seconds) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Speech preparation/inference timed out after {seconds} seconds. Original recordings and downloaded model files were retained.");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    out.seek(SeekFrom::Start(0))?;
    err.seek(SeekFrom::Start(0))?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    out.read_to_end(&mut stdout)?;
    err.read_to_end(&mut stderr)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
