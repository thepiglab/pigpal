//! Read-only replay of raw Waveline capture bytes, using recorded receipt times.
use crate::{
    store::{self, Event, Session},
    waveline::{Framer, Record},
};
use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, NaiveTime, TimeZone, Utc};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Duration,
};

/// One received block positioned in the preceding receipt interval for visual
/// comparison only. Saved replay may read ahead one regular block, clipped at the
/// cursor by the renderer. These are not hardware timestamps; blocks stay separate.
pub struct WaveBlock<'a> {
    pub start_s: f64,
    pub end_s: f64,
    pub record: &'a Record,
}

pub struct Frame {
    pub utc: DateTime<Utc>,
    pub segment: String,
    pub connection: String,
    pub timestamp_basis: String,
    pub elapsed_s: Option<f64>,
    pub peer: String,
    pub record: Record,
}

pub struct Capture {
    pub frames: Vec<Frame>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub warnings: Vec<String>,
}

impl Capture {
    /// Decode original wire bytes with today's candidate decoder. Journals and
    /// exports are never rewritten, including legacy captures without decoded events.
    pub fn load(dir: &Path) -> Result<Self> {
        Self::load_optional(dir)?.context("No timestamped Waveline records found in this session")
    }
    /// Sessions with no Waveline payload still support normal CSV exports.
    pub fn load_optional(dir: &Path) -> Result<Option<Self>> {
        let session: Session = store::read_json(&dir.join("session.json"))?;
        ensure!(
            session.state != "recording",
            "Stop and save the session before replaying it"
        );
        let mut frames = Vec::new();
        let mut warnings = Vec::new();
        for segment in &session.segments {
            let path = dir.join("segments").join(&segment.id);
            let mut wire = match File::open(path.join("wire.bin")) {
                Ok(wire) => wire,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let wire_len = wire.metadata()?.len();
            let mut framers: HashMap<String, Framer> = HashMap::new();
            // Bounded journal IO; an incomplete tail is reported and retained.
            let journal = File::open(path.join("journal.ndjson"))?;
            let mut reader = std::io::BufReader::new(journal);
            let mut line = String::new();
            loop {
                use std::io::BufRead;
                line.clear();
                if reader.read_line(&mut line)? == 0 {
                    break;
                }
                if !line.ends_with('\n') {
                    warnings.push(format!("Incomplete journal tail in segment {}", segment.id));
                    break;
                }
                let e: Event =
                    serde_json::from_str(&line).context("Invalid replay journal event")?;
                if e.kind != "wire" || e.data["direction"] != "rx" {
                    continue;
                }
                let d = &e.data;
                let offset = d["offset"].as_u64().context("Wire offset missing")?;
                let length = d["length"].as_u64().context("Wire length missing")?;
                ensure!(
                    offset
                        .checked_add(length)
                        .is_some_and(|end| end <= wire_len),
                    "Wire range exceeds saved file in segment {}",
                    segment.id
                );
                let connection = d["connection"].as_str().context("Connection missing")?;
                let (utc, basis) = match d.get("received_utc") {
                    Some(utc) => (
                        serde_json::from_value(utc.clone())?,
                        "transport_read_completion",
                    ),
                    None => (e.utc, "legacy_journal_processing"),
                };
                let framer = framers.entry(connection.into()).or_default();
                wire.seek(SeekFrom::Start(offset))?;
                let mut remaining = length;
                let mut buffer = [0_u8; 65536];
                while remaining > 0 {
                    let n = remaining.min(buffer.len() as u64) as usize;
                    wire.read_exact(&mut buffer[..n])?;
                    for record in framer.push(&buffer[..n]) {
                        frames.push(Frame {
                            utc,
                            segment: segment.id.clone(),
                            connection: connection.into(),
                            timestamp_basis: basis.into(),
                            elapsed_s: if d.get("received_utc").is_some() {
                                d.get("received_elapsed_s")
                                    .and_then(serde_json::Value::as_f64)
                            } else {
                                Some(e.elapsed_s)
                            },
                            peer: d
                                .get("peer")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .into(),
                            record,
                        });
                    }
                    remaining -= n as u64;
                }
            }
            for framer in framers.values() {
                if framer.discarded > 0 || framer.pending_bytes() > 0 {
                    warnings.push(format!(
                        "Segment {}: {} unrecognized bytes, {} trailing bytes",
                        segment.id,
                        framer.discarded,
                        framer.pending_bytes()
                    ));
                }
            }
        }
        if frames.is_empty() {
            return Ok(None);
        }
        // Stable ordering preserves wire order for records with the same read timestamp.
        frames.sort_by_key(|frame| frame.utc);
        let start = frames[0].utc.min(session.created);
        let end = session
            .segments
            .iter()
            .filter_map(|s| s.ended)
            .max()
            .unwrap_or(frames.last().unwrap().utc)
            .max(frames.last().unwrap().utc);
        Ok(Some(Self {
            frames,
            start,
            end,
            warnings,
        }))
    }
}

pub struct Player {
    pub capture: Capture,
    pub cursor: DateTime<Utc>,
    pub index: Option<usize>,
    pub playing: bool,
}
impl Player {
    pub fn new(capture: Capture) -> Self {
        let cursor = capture.frames[0].utc;
        Self {
            capture,
            cursor,
            index: Some(0),
            playing: false,
        }
    }
    pub fn frame(&self) -> Option<&Frame> {
        self.index.map(|i| &self.capture.frames[i])
    }
    pub fn age(&self) -> Option<f64> {
        self.frame()
            .map(|f| (self.cursor - f.utc).num_microseconds().unwrap_or(i64::MAX) as f64 / 1e6)
    }
    pub fn seek(&mut self, utc: DateTime<Utc>) -> Result<()> {
        ensure!(
            utc >= self.capture.start && utc <= self.capture.end,
            "Time is outside this session's recorded range"
        );
        self.cursor = utc;
        self.index = self
            .capture
            .frames
            .partition_point(|f| f.utc <= utc)
            .checked_sub(1);
        self.playing = false;
        Ok(())
    }
    pub fn step(&mut self, forward: bool) {
        let len = self.capture.frames.len();
        let i = match (self.index, forward) {
            (Some(i), true) => (i + 1).min(len - 1),
            (Some(i), false) => i.saturating_sub(1),
            (None, _) => 0,
        };
        self.index = Some(i);
        self.cursor = self.capture.frames[i].utc;
        self.playing = false;
    }
    pub fn advance(&mut self, elapsed: Duration) {
        if !self.playing {
            return;
        }
        let delta = chrono::Duration::from_std(elapsed).unwrap_or(chrono::Duration::MAX);
        let next = self
            .cursor
            .checked_add_signed(delta)
            .unwrap_or(self.capture.end)
            .min(self.capture.end);
        self.cursor = next;
        self.index = self
            .capture
            .frames
            .partition_point(|f| f.utc <= next)
            .checked_sub(1);
        if next == self.capture.end {
            self.playing = false;
        }
    }
    pub fn wave_blocks(&self, window_s: f64) -> Vec<WaveBlock<'_>> {
        let Some(index) = self.index else {
            return Vec::new();
        };
        let current = &self.capture.frames[index];
        // All saved bytes are already in memory. Place the next regular block in
        // its preceding receipt interval so the trace reaches the moving cursor,
        // rather than scrolling a blank strip until the next receipt. Numeric
        // selection is unchanged. Do not place batches, gaps or new connections.
        let buffered_index = self
            .capture
            .frames
            .get(index + 1)
            .filter(|next| {
                let interval =
                    (next.utc - current.utc).num_microseconds().unwrap_or(0) as f64 / 1e6;
                next.segment == current.segment
                    && next.connection == current.connection
                    && (0.5..=1.5).contains(&interval)
                    && (index == 0 || self.capture.frames[index - 1].utc != current.utc)
                    && self
                        .capture
                        .frames
                        .get(index + 2)
                        .is_none_or(|after| after.utc != next.utc)
            })
            .map_or(index, |_| index + 1);
        let mut blocks = Vec::new();
        // Hard bound prevents bursts or malformed receipt times from growing render work.
        for i in (1..=buffered_index).rev().take(64) {
            let frame = &self.capture.frames[i];
            let previous = &self.capture.frames[i - 1];
            let end_s = (frame.utc - self.cursor)
                .num_microseconds()
                .unwrap_or(i64::MIN) as f64
                / 1e6;
            if end_s < -window_s
                || frame.segment != current.segment
                || frame.connection != current.connection
            {
                break;
            }
            if previous.segment != frame.segment || previous.connection != frame.connection {
                break;
            }
            let interval = (frame.utc - previous.utc).num_microseconds().unwrap_or(0) as f64 / 1e6;
            // Duplicate/batched timestamps and gaps cannot locate an individual block.
            // Leave these intervals blank rather than manufacture a continuous trace.
            if !(0.5..=1.5).contains(&interval) {
                continue;
            }
            blocks.push(WaveBlock {
                start_s: end_s - interval,
                end_s,
                record: &frame.record,
            });
        }
        blocks.reverse();
        blocks
    }
    pub fn position_s(&self) -> f64 {
        (self.cursor - self.capture.start).num_milliseconds() as f64 / 1000.0
    }
    pub fn duration_s(&self) -> f64 {
        (self.capture.end - self.capture.start).num_milliseconds() as f64 / 1000.0
    }
}

/// A clock-only input uses the currently displayed local date. Full local date
/// and RFC3339 inputs allow multi-day captures and explicit time-zone offsets.
pub fn parse_time(text: &str, current: DateTime<Utc>) -> Result<DateTime<Utc>> {
    let text = text.trim();
    if let Ok(utc) = DateTime::parse_from_rfc3339(text) {
        return Ok(utc.with_timezone(&Utc));
    }
    let naive = if let Ok(time) = NaiveTime::parse_from_str(text, "%H:%M:%S%.f") {
        current.with_timezone(&Local).date_naive().and_time(time)
    } else if let Ok(date) = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f") {
        date
    } else {
        bail!("Enter HH:MM:SS, YYYY-MM-DD HH:MM:SS, or an ISO time with offset");
    };
    Local.from_local_datetime(&naive).single().map(|d| d.with_timezone(&Utc))
        .context("Local time is ambiguous or missing at a daylight-saving change; enter an ISO time with offset")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stamp(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }
    fn capture() -> Capture {
        let start = stamp("2026-10-20T14:38:40Z");
        let mut bytes = vec![0; crate::waveline::RECORD_LEN];
        bytes[..4].copy_from_slice(&crate::waveline::MAGIC);
        bytes[4..8].copy_from_slice(&(crate::waveline::RECORD_LEN as u32).to_le_bytes());
        let frames = [(0.25, 88_u16), (1.0, 89), (1.0, 90), (10.0, 110)]
            .into_iter()
            .map(|(s, hr)| {
                bytes[14..16].copy_from_slice(&hr.to_le_bytes());
                Frame {
                    utc: start + chrono::Duration::milliseconds((s * 1000.0) as i64),
                    segment: "a".into(),
                    connection: "c".into(),
                    timestamp_basis: "transport_read_completion".into(),
                    elapsed_s: Some(s),
                    peer: "monitor".into(),
                    record: Record::decode(&bytes, 0).unwrap(),
                }
            })
            .collect();
        Capture {
            frames,
            start,
            end: start + chrono::Duration::seconds(12),
            warnings: vec![],
        }
    }
    #[test]
    fn history_does_not_place_future_batched_or_disconnected_blocks() {
        let mut p = Player::new(capture());
        p.seek(stamp("2026-10-20T14:38:40.900Z")).unwrap();
        assert!(p.wave_blocks(8.0).is_empty());
        p.seek(stamp("2026-10-20T14:38:41Z")).unwrap();
        let blocks = p.wave_blocks(8.0);
        assert_eq!(
            blocks.len(),
            1,
            "Equal-time second record must not be positioned with an invented interval"
        );
        assert_eq!(blocks[0].record.fields[0].value, Some(89.0));
        assert_eq!((blocks[0].start_s, blocks[0].end_s), (-0.75, 0.0));
        p.seek(stamp("2026-10-20T14:38:50Z")).unwrap();
        assert!(
            p.wave_blocks(8.0).is_empty(),
            "No trace across a nine-second gap"
        );
        p.seek(stamp("2026-10-20T14:38:41Z")).unwrap();
        p.capture.frames[2].connection = "new-connection".into();
        assert!(p.wave_blocks(8.0).is_empty());
    }

    #[test]
    fn buffered_history_reaches_cursor_without_advancing_numeric_readings() {
        let mut capture = capture();
        capture.frames.truncate(3);
        let start = capture.frames[0].utc;
        for (i, frame) in capture.frames.iter_mut().enumerate() {
            frame.utc = start + chrono::Duration::seconds(i as i64);
        }
        let mut p = Player::new(capture);
        for millis in [1, 250, 500, 999, 1000, 1001, 1750, 1999] {
            p.seek(start + chrono::Duration::milliseconds(millis))
                .unwrap();
            let selected = p.index.unwrap();
            let blocks = p.wave_blocks(8.0);
            let last = blocks.last().unwrap();
            assert!(last.start_s <= 0.0 && last.end_s >= 0.0);
            assert!(last.end_s <= 1.0);
            assert!(std::ptr::eq(
                last.record,
                &p.capture.frames[selected + 1].record
            ));
            assert!(p.frame().unwrap().utc <= p.cursor);
            assert_eq!(selected, usize::from(millis >= 1000));
        }
        // A true gap never supplies an interpolated/read-ahead block.
        p.capture.frames[2].utc = start + chrono::Duration::seconds(10);
        p.seek(start + chrono::Duration::milliseconds(1500))
            .unwrap();
        assert!(p.wave_blocks(8.0).iter().all(|b| b.end_s < 0.0));
        // Nor does a regular-looking interval across a connection or segment.
        p.capture.frames[2].utc = start + chrono::Duration::seconds(2);
        p.capture.frames[2].connection = "other".into();
        assert!(p.wave_blocks(8.0).iter().all(|b| b.end_s < 0.0));
        p.capture.frames[2].connection = "c".into();
        p.capture.frames[2].segment = "other".into();
        assert!(p.wave_blocks(8.0).iter().all(|b| b.end_s < 0.0));
    }

    #[test]
    fn seeking_updates_paused_values_without_using_future_records() {
        let mut p = Player::new(capture());
        p.seek(stamp("2026-10-20T14:38:40Z")).unwrap();
        assert!(p.frame().is_none());
        p.seek(stamp("2026-10-20T14:38:40.900Z")).unwrap();
        assert_eq!(p.frame().unwrap().record.fields[0].value, Some(88.0));
        assert!(!p.playing);
        p.seek(stamp("2026-10-20T14:38:41Z")).unwrap();
        assert_eq!(p.index, Some(2));
        assert!(p.seek(stamp("2026-10-20T14:00:00Z")).is_err());
        assert_eq!(p.index, Some(2));
    }
    #[test]
    fn stepping_visits_equal_timestamps_and_playback_preserves_gaps() {
        let mut p = Player::new(capture());
        p.step(true);
        assert_eq!(p.index, Some(1));
        p.step(true);
        assert_eq!(p.index, Some(2));
        p.step(false);
        assert_eq!(p.index, Some(1));
        p.playing = true;
        p.advance(Duration::from_secs(4));
        assert_eq!(p.index, Some(2));
        assert_eq!(p.age(), Some(4.0));
        p.advance(Duration::from_secs(5));
        assert_eq!(p.index, Some(3));
        assert_eq!(p.age(), Some(0.0));
        p.advance(Duration::from_secs(100));
        assert!(!p.playing);
        assert_eq!(p.cursor, p.capture.end);
    }
    #[test]
    fn clock_input_uses_display_date_and_explicit_offsets_are_honored() {
        let date = stamp("2026-10-20T14:38:42Z");
        assert_eq!(parse_time("2026-10-20T10:38:42-04:00", date).unwrap(), date);
        let local = date.with_timezone(&Local).format("%H:%M:%S").to_string();
        assert_eq!(parse_time(&local, date).unwrap(), date);
        assert!(parse_time("25:00:00", date).is_err());
    }
    #[test]
    fn loader_reassembles_interleaved_connections_and_preserves_files() {
        use crate::store::{Event, Segment};
        use sha2::{Digest, Sha256};
        let temp = tempfile::tempdir().unwrap();
        let start = stamp("2026-10-20T14:38:40Z");
        let path = temp.path().join("segments/a");
        std::fs::create_dir_all(&path).unwrap();
        let mut a = vec![0; crate::waveline::RECORD_LEN];
        a[..4].copy_from_slice(&crate::waveline::MAGIC);
        a[4..8].copy_from_slice(&(crate::waveline::RECORD_LEN as u32).to_le_bytes());
        a[14..16].copy_from_slice(&110_u16.to_le_bytes());
        let mut b = a.clone();
        b[14..16].copy_from_slice(&88_u16.to_le_bytes());
        let mut wire = a[..1000].to_vec();
        wire.extend(&b);
        wire.extend(&a[1000..]);
        let mut journal = String::new();
        for (seq, connection, offset, length, receipt) in [
            (1, "a", 0, 1000, Some("2026-10-20T14:38:40.123456789Z")),
            (2, "b", 1000, 1956, Some("2026-10-20T14:38:40.234567891Z")),
            (3, "a", 2956, 956, None),
        ] {
            let mut data = serde_json::json!({"connection":connection,"offset":offset,"length":length,"direction":"rx"});
            if let Some(utc) = receipt {
                data["received_utc"] = utc.into();
            }
            let event = Event {
                sequence: seq,
                id: seq.to_string(),
                segment_id: "a".into(),
                utc: start + chrono::Duration::seconds(seq as i64),
                elapsed_s: seq as f64,
                kind: "wire".into(),
                data,
            };
            journal.push_str(&serde_json::to_string(&event).unwrap());
            journal.push('\n');
        }
        journal.push_str("{incomplete");
        let session = Session {
            schema: 2,
            id: "test".into(),
            subject: "Synthetic".into(),
            title: "Replay".into(),
            created: start,
            state: "completed".into(),
            synthetic: true,
            segments: vec![Segment {
                id: "a".into(),
                started: start,
                ended: Some(start + chrono::Duration::seconds(10)),
                state: "completed".into(),
                source: Default::default(),
            }],
        };
        store::write_json(&temp.path().join("session.json"), &session).unwrap();
        std::fs::write(path.join("wire.bin"), &wire).unwrap();
        std::fs::write(path.join("journal.ndjson"), &journal).unwrap();
        let before = Sha256::digest(&wire);
        let capture = Capture::load(temp.path()).unwrap();
        assert_eq!(capture.frames.len(), 2);
        assert_eq!(capture.frames[0].record.fields[0].value, Some(88.0));
        assert_eq!(
            capture.frames[0].utc,
            stamp("2026-10-20T14:38:40.234567891Z")
        );
        assert_eq!(capture.frames[1].record.fields[0].value, Some(110.0));
        assert_eq!(capture.frames[1].utc, start + chrono::Duration::seconds(3));
        assert_eq!(
            capture.frames[1].timestamp_basis,
            "legacy_journal_processing"
        );
        assert_eq!(capture.frames[1].record.stream_offset, 0);
        assert_eq!(capture.warnings.len(), 1);
        assert_eq!(
            Sha256::digest(std::fs::read(path.join("wire.bin")).unwrap()),
            before
        );
        assert_eq!(
            std::fs::read_to_string(path.join("journal.ndjson")).unwrap(),
            journal
        );
        let mut invalid = session;
        invalid.state = "recording".into();
        store::write_json(&temp.path().join("session.json"), &invalid).unwrap();
        assert!(Capture::load(temp.path()).is_err());
    }
}
