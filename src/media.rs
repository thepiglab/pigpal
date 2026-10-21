use crate::store;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub struct Video {
    child: Child,
    dir: PathBuf,
    meta: Value,
}
impl Video {
    pub fn start(dir: &Path, format: &str, input: &str, elapsed: f64) -> Result<Self> {
        ensure!(
            !input.trim().is_empty(),
            "Enter the video input shown by FFmpeg"
        );
        ensure!(
            ["avfoundation", "dshow", "v4l2", "lavfi"].contains(&format),
            "Unsupported capture interface"
        );
        let uid = store::id();
        let folder = dir.join("video").join(&uid);
        fs::create_dir_all(&folder)?;
        let log = File::create(folder.join("capture.log"))?;
        let mut command = Command::new("ffmpeg");
        command.args(["-hide_banner", "-loglevel", "warning", "-n", "-f", format]);
        if format == "lavfi" {
            command.arg("-re");
        }
        if format == "avfoundation" {
            command.args(["-framerate", "30"]);
        }
        command
            .args([
                "-i",
                input,
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-crf",
                "24",
                "-pix_fmt",
                "yuv420p",
                "-force_key_frames",
                "expr:gte(t,n_forced*30)",
                "-f",
                "segment",
                "-segment_time",
                "30",
                "-reset_timestamps",
                "1",
            ])
            .arg(folder.join("%06d.mkv"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        let child = command
            .spawn()
            .context("Install FFmpeg and make it available on PATH to record video")?;
        let meta = json!({"id":uid,"started":store::utc(),"segment_elapsed_s":elapsed,"format":format,"input":input,"directory":format!("video/{uid}"),"state":"recording","timing_note":"Host process-start anchor; video PTS resets within each 30-second chunk. Device and encoder delay are not calibrated."});
        store::write_json(&folder.join("capture.json"), &meta)?;
        Ok(Self {
            child,
            dir: folder,
            meta,
        })
    }
    pub fn exited(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
    pub fn stop(&mut self) -> Result<Value> {
        if let Some(mut stdin) = self.child.stdin.take() {
            let _ = stdin.write_all(b"q\n");
        }
        let begin = Instant::now();
        let mut forced = false;
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            if begin.elapsed() > Duration::from_secs(5) {
                self.child.kill()?;
                forced = true;
                break self.child.wait()?;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        self.meta["ended"] = json!(store::utc());
        self.meta["state"] = json!(if status.success() && !forced {
            "closed"
        } else {
            "interrupted"
        });
        store::write_json(&self.dir.join("capture.json"), &self.meta)?;
        ensure!(status.success()&&!forced,"Video capture stopped with an error. Original chunks and capture.log are retained in {}",self.dir.display());
        Ok(self.meta.clone())
    }
}
impl Drop for Video {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
pub fn attach_photo(dir: &Path, input: &Path, caption: &str) -> Result<Value> {
    let ext = input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ensure!(
        ["jpg", "jpeg", "png"].contains(&ext.as_str()),
        "Choose a JPEG or PNG image"
    );
    let relative = format!("photos/{}.{}", store::id(), ext);
    fs::copy(input, dir.join(&relative))?;
    let info = json!({"file":relative,"caption":caption,"attached":store::utc(),"timing_note":"Attachment time; original image bytes preserved. This is not a synchronized exposure timestamp."});
    store::write_json(&dir.join(format!("{relative}.json")), &info)?;
    Ok(info)
}
