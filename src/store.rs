use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::Instant,
};
use uuid::Uuid;

pub fn id() -> String {
    Uuid::new_v4().to_string()
}
pub fn utc() -> DateTime<Utc> {
    Utc::now()
}
pub struct Root {
    pub path: PathBuf,
    _lock: File,
}
impl Root {
    pub fn open(path: PathBuf) -> Result<Self> {
        fs::create_dir_all(&path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join(".workspace.lock"))?;
        lock.try_lock_exclusive()
            .context("This workspace is already open in another Pig Pal process")?;
        fs::create_dir_all(path.join("sessions"))?;
        fs::create_dir_all(path.join("models"))?;
        let root = Self { path, _lock: lock };
        for mut session in root.sessions()? {
            if session.state == "recording" {
                session.state = "interrupted".into();
                for s in &mut session.segments {
                    if s.state == "recording" {
                        s.state = "interrupted".into();
                        write_json(
                            &root
                                .session_dir(&session.id)
                                .join("segments")
                                .join(&s.id)
                                .join("segment.json"),
                            s,
                        )?;
                    }
                }
                root.save(&session)?;
            }
        }
        Ok(root)
    }
    pub fn default_path() -> PathBuf {
        let preferred = directories::ProjectDirs::from("org", "Research", "PigPal")
            .map(|p| p.data_local_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("PigPalData"));
        let legacy = directories::ProjectDirs::from("org", "Research", "SurgeryBuddyDesktop")
            .map(|p| p.data_local_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("SurgeryBuddyDesktopData"));
        compatible_data_path(preferred, legacy)
    }
    pub fn session_dir(&self, id: &str) -> PathBuf {
        self.path.join("sessions").join(id)
    }
    pub fn save(&self, s: &Session) -> Result<()> {
        write_json(&self.session_dir(&s.id).join("session.json"), s)
    }
    pub fn sessions(&self) -> Result<Vec<Session>> {
        let mut result = Vec::new();
        for entry in fs::read_dir(self.path.join("sessions"))? {
            let p = entry?.path().join("session.json");
            if p.is_file() {
                let s: Session =
                    read_json(&p).with_context(|| format!("Cannot read {}", p.display()))?;
                result.push(s);
            }
        }
        result.sort_by(|a, b| b.created.cmp(&a.created));
        Ok(result)
    }
    pub fn create(&self, subject: &str, title: &str, synthetic: bool) -> Result<Session> {
        ensure!(
            !subject.trim().is_empty() && !title.trim().is_empty(),
            "Enter a subject and session title"
        );
        let s = Session {
            schema: 2,
            id: id(),
            subject: subject.trim().into(),
            title: title.trim().into(),
            created: utc(),
            state: "ready".into(),
            synthetic,
            segments: vec![],
        };
        fs::create_dir_all(self.session_dir(&s.id).join("segments"))?;
        self.save(&s)?;
        Ok(s)
    }
}

// Retain existing recordings, settings, model installations, and copied CSV
// paths. New installations use PigPal; an explicit --data-dir still overrides it.
fn compatible_data_path(preferred: PathBuf, legacy: PathBuf) -> PathBuf {
    if !preferred.exists() && legacy.exists() {
        legacy
    } else {
        preferred
    }
}

#[cfg(test)]
mod data_path_tests {
    use super::*;

    #[test]
    fn rename_retains_existing_workspace_and_prefers_an_existing_new_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let preferred = temp.path().join("PigPal");
        let legacy = temp.path().join("SurgeryBuddyDesktop");
        assert_eq!(
            compatible_data_path(preferred.clone(), legacy.clone()),
            preferred
        );
        fs::create_dir(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), "original").unwrap();
        assert_eq!(
            compatible_data_path(preferred.clone(), legacy.clone()),
            legacy
        );
        fs::create_dir(&preferred).unwrap();
        assert_eq!(
            compatible_data_path(preferred.clone(), legacy.clone()),
            preferred
        );
        assert_eq!(
            fs::read_to_string(legacy.join("settings.json")).unwrap(),
            "original"
        );
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub schema: u32,
    pub id: String,
    pub subject: String,
    pub title: String,
    pub created: DateTime<Utc>,
    pub state: String,
    pub synthetic: bool,
    pub segments: Vec<Segment>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: String,
    pub started: DateTime<Utc>,
    pub ended: Option<DateTime<Utc>>,
    pub state: String,
    pub source: crate::transport::SourceConfig,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub sequence: u64,
    pub id: String,
    pub segment_id: String,
    pub utc: DateTime<Utc>,
    pub elapsed_s: f64,
    pub kind: String,
    pub data: Value,
}
pub struct Journal {
    file: File,
    pub dir: PathBuf,
    pub segment_id: String,
    clock: Instant,
    sequence: u64,
}
impl Journal {
    pub fn create(
        root: &Root,
        s: &mut Session,
        source: crate::transport::SourceConfig,
    ) -> Result<Self> {
        let segment = Segment {
            id: id(),
            started: utc(),
            ended: None,
            state: "recording".into(),
            source,
        };
        let dir = root.session_dir(&s.id).join("segments").join(&segment.id);
        fs::create_dir_all(&dir)?;
        for name in ["audio", "video", "photos", "transcripts"] {
            fs::create_dir_all(dir.join(name))?;
        }
        write_json(&dir.join("segment.json"), &segment)?;
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dir.join("journal.ndjson"))?;
        let segment_id = segment.id.clone();
        s.segments.push(segment);
        s.state = "recording".into();
        root.save(s)?;
        Ok(Self {
            file,
            dir,
            segment_id,
            clock: Instant::now(),
            sequence: 0,
        })
    }
    pub fn elapsed(&self) -> f64 {
        self.clock.elapsed().as_secs_f64()
    }
    pub fn elapsed_at(&self, instant: Instant) -> f64 {
        instant.duration_since(self.clock).as_secs_f64()
    }
    pub fn append(&mut self, kind: &str, data: Value) -> Result<Event> {
        self.sequence += 1;
        let event = Event {
            sequence: self.sequence,
            id: id(),
            segment_id: self.segment_id.clone(),
            utc: utc(),
            elapsed_s: self.elapsed(),
            kind: kind.into(),
            data,
        };
        let mut bytes = serde_json::to_vec(&event)?;
        bytes.push(b'\n');
        self.file.write_all(&bytes)?;
        Ok(event)
    }
    pub fn sync(&mut self) -> Result<()> {
        self.file.sync_data()?;
        Ok(())
    }
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("Missing parent directory")?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temp, value)?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
}
/// Stream the journal; tolerate only a final incomplete line, preserving its original bytes.
pub fn visit_events(dir: &Path, mut visit: impl FnMut(Event) -> Result<()>) -> Result<Vec<String>> {
    let s: Session = read_json(&dir.join("session.json"))?;
    let mut warnings = vec![];
    for segment in s.segments {
        let path = dir.join("segments").join(segment.id).join("journal.ndjson");
        let mut reader = BufReader::new(File::open(&path)?);
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            if !line.ends_with('\n') {
                warnings.push(format!(
                    "Incomplete journal tail retained: {}",
                    path.display()
                ));
                break;
            }
            let event = serde_json::from_str(&line)
                .with_context(|| format!("Invalid complete journal line in {}", path.display()))?;
            visit(event)?;
        }
    }
    Ok(warnings)
}
pub fn recent_events(dir: &Path) -> Result<(std::collections::VecDeque<Event>, Vec<String>)> {
    let mut events = std::collections::VecDeque::new();
    let warnings = visit_events(dir, |e| {
        if events.len() == 1000 {
            events.pop_front();
        }
        events.push_back(e);
        Ok(())
    })?;
    Ok((events, warnings))
}
