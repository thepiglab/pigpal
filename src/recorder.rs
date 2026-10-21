use crate::{
    audio::{AudioEvent, Microphone},
    protocol::{self, Framer, Observation},
    store::{self, Event, Journal, Root, Session},
    transport::{self, Input, SourceConfig, Worker},
};
use anyhow::{ensure, Result};
use serde_json::json;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs::{File, OpenOptions},
    io::Write,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct LiveValue {
    pub observation: Observation,
    pub utc: chrono::DateTime<chrono::Utc>,
    pub elapsed_s: f64,
    pub connection: String,
}

pub struct Recorder {
    pub session: Session,
    pub journal: Journal,
    pub events: VecDeque<Event>,
    pub latest: BTreeMap<String, LiveValue>,
    pub history: VecDeque<(f64, String, f64)>,
    pub status: String,
    pub bytes: u64,
    pub messages: u64,
    pub last_received_elapsed_s: Option<f64>,
    pub binary_records: u64,
    pub binary_latest: Option<crate::waveline::Record>,
    pub last_binary_elapsed_s: Option<f64>,
    pub last_binary_received_utc: Option<chrono::DateTime<chrono::Utc>>,
    pub mic: Option<Microphone>,
    pub video: Option<crate::media::Video>,
    wire: File,
    binary_csv: Option<crate::waveline::LiveCsv>,
    worker: Option<Worker>,
    framers: HashMap<String, Framer>,
    binary_framers: HashMap<String, crate::waveline::Framer>,
    seen: HashSet<String>,
    profile: SourceConfig,
    last_check: Instant,
}
impl Recorder {
    pub fn start(root: &Root, mut session: Session, profile: SourceConfig) -> Result<Self> {
        profile.validate()?;
        ensure!(
            (profile.mode == transport::Source::Demo) == session.synthetic,
            "Keep synthetic demo data in synthetic sessions; select the original session's input type before resuming"
        );
        ensure!(
            fs2::available_space(&root.path)? > 256 * 1024 * 1024,
            "Less than 256 MB free; choose a drive with more space"
        );
        let journal = Journal::create(root, &mut session, profile.clone())?;
        let wire = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(journal.dir.join("wire.bin"))?;
        let worker = transport::start(profile.clone())?;
        let binary_csv = if profile.hl7 {
            None
        } else {
            Some(crate::waveline::LiveCsv::create(&journal.dir)?)
        };
        let mut rec = Self {
            session,
            journal,
            events: VecDeque::new(),
            latest: BTreeMap::new(),
            history: VecDeque::new(),
            status: "Starting".into(),
            bytes: 0,
            messages: 0,
            last_received_elapsed_s: None,
            binary_records: 0,
            binary_latest: None,
            last_binary_elapsed_s: None,
            last_binary_received_utc: None,
            mic: None,
            video: None,
            wire,
            binary_csv,
            worker: Some(worker),
            framers: HashMap::new(),
            binary_framers: HashMap::new(),
            seen: HashSet::new(),
            profile,
            last_check: Instant::now(),
        };
        rec.event(
            "start",
            json!({"source":rec.profile,"synthetic":rec.session.synthetic}),
        )?;
        rec.journal.sync()?;
        Ok(rec)
    }
    pub fn event(&mut self, kind: &str, data: serde_json::Value) -> Result<String> {
        let event = self.journal.append(kind, data)?;
        let id = event.id.clone();
        if self.events.len() == 300 {
            self.events.pop_front();
        }
        self.events.push_back(event);
        Ok(id)
    }
    pub fn source(&self) -> &SourceConfig {
        &self.profile
    }
    pub fn marker(&mut self, label: &str, note: &str, amends: Option<&str>) -> Result<String> {
        let context:Vec<_>=self.latest.values().map(|v|json!({"observation":v.observation,"received_utc":v.utc,"receipt_age_s":(self.journal.elapsed()-v.elapsed_s).max(0.0),"connection":v.connection})).collect();
        let id = self.event(
            if amends.is_some() {
                "annotation"
            } else {
                "marker"
            },
            json!({"label":label,"note":note,"amends_id":amends,"context":context}),
        )?;
        self.journal.sync()?;
        Ok(id)
    }
    pub fn poll(&mut self) -> Result<()> {
        for _ in 0..256 {
            let input = self.worker.as_ref().and_then(|w| w.rx.try_recv().ok());
            match input {
                Some(i) => self.input(i)?,
                None => break,
            }
        }
        self.poll_audio()?;
        let checked_elapsed_s = self.journal.elapsed();
        let checked_utc = store::utc();
        let gap = if let Some(csv) = &mut self.binary_csv {
            csv.append_stale(checked_utc, checked_elapsed_s)?
        } else {
            None
        };
        if let Some(age) = gap {
            self.event(
                "binary_gap",
                json!({
                    "checked_utc":checked_utc, "checked_elapsed_s":checked_elapsed_s,
                    "last_complete_record_age_s":age, "stream_status":"stale",
                    "arterial_status":"stale", "timestamp_basis":"host_gap_check"
                }),
            )?;
        }
        if self.mic.as_ref().is_some_and(|m| m.failed()) {
            self.stop_mic()?;
        }
        if self
            .video
            .as_mut()
            .is_some_and(|v| v.exited().unwrap_or(true))
        {
            if let Some(mut video) = self.video.take() {
                match video.stop() {
                    Ok(info) => {
                        self.event("video_closed", info)?;
                    }
                    Err(e) => {
                        self.status = e.to_string();
                        self.event("video_error", json!({"detail":e.to_string()}))?;
                    }
                }
            }
        }
        if self.last_check.elapsed() > Duration::from_secs(1) {
            self.wire.sync_data()?;
            if let Some(csv) = &mut self.binary_csv {
                csv.sync()?;
            }
            self.journal.sync()?;
            self.last_check = Instant::now();
            ensure!(
                fs2::available_space(&self.journal.dir)? > 256 * 1024 * 1024,
                "Capture stopped: less than 256 MB free"
            );
        }
        Ok(())
    }
    fn input(&mut self, input: Input) -> Result<()> {
        match input {
            Input::Connected { connection, peer } => {
                self.status = format!("Connected • {peer}");
                self.framers.insert(connection.clone(), Framer::default());
                self.event("connection", json!({"connection":connection,"peer":peer}))?;
            }
            Input::Disconnected { connection } => {
                if self
                    .binary_framers
                    .remove(&connection)
                    .is_some_and(|f| f.pending_bytes() > 0)
                    && self.binary_records > 0
                {
                    self.event("warning", json!({"detail":"Incomplete experimental binary record at disconnect; original bytes retained","connection":connection}))?;
                }
                if self
                    .framers
                    .remove(&connection)
                    .is_some_and(|f| f.has_partial())
                {
                    self.event("warning",json!({"detail":"Incomplete MLLP frame at disconnect; original bytes retained","connection":connection}))?;
                }
                self.status = "Source disconnected; session remains open".into();
                self.event("disconnect", json!({"connection":connection}))?;
            }
            Input::Status(detail) | Input::Error(detail) => {
                self.status = detail.clone();
                self.event("status", json!({"detail":detail}))?;
            }
            Input::Bytes {
                connection,
                peer,
                bytes,
                reply,
                received_utc,
                received_at,
            } => {
                let received_elapsed_s = self.journal.elapsed_at(received_at);
                let offset = self.bytes;
                self.wire.write_all(&bytes)?;
                self.bytes += bytes.len() as u64;
                self.last_received_elapsed_s = Some(received_elapsed_s);
                self.event("wire",json!({"connection":connection,"peer":peer,"offset":offset,"length":bytes.len(),"direction":"rx","received_utc":received_utc,"received_elapsed_s":received_elapsed_s,"timestamp_basis":"transport_read_completion"}))?;
                let mut ack = Vec::new();
                if !self.profile.hl7 {
                    let records = self
                        .binary_framers
                        .entry(connection.clone())
                        .or_default()
                        .push(&bytes);
                    for record in records {
                        if let Some(csv) = &mut self.binary_csv {
                            csv.append(
                                &record,
                                &connection,
                                &peer,
                                received_utc,
                                received_elapsed_s,
                            )?;
                        }
                        self.binary_records += 1;
                        self.last_binary_elapsed_s = Some(received_elapsed_s);
                        self.last_binary_received_utc = Some(received_utc);
                        self.event(
                            "binary_record",
                            json!({
                                "experimental":true,
                                "mapping_verified":false,
                                "connection":connection,
                                "peer":peer,
                                "connection_offset":record.stream_offset,
                                "record_length":crate::waveline::RECORD_LEN,
                                "fields":record.fields,
                                "arterial_pressure":record.arterial_pressure,
                                "received_utc":received_utc,
                                "received_elapsed_s":received_elapsed_s,
                                "timestamp_basis":"transport_read_completion",
                            }),
                        )?;
                        self.binary_latest = Some(record);
                    }
                } else {
                    let (frames, broken) = self
                        .framers
                        .entry(connection.clone())
                        .or_default()
                        .push(&bytes);
                    if broken > 0 {
                        self.event("warning",json!({"detail":"Invalid or oversized MLLP frame; bytes retained","count":broken}))?;
                    }
                    for frame in frames {
                        match protocol::parse(&frame) {
                            Ok(mut message) => {
                                let key = format!("{connection}:{}", message.control_id);
                                if !message.control_id.is_empty() {
                                    message.repeated_control_id = !self.seen.insert(key);
                                }
                                if self.seen.len() > 10000 {
                                    self.seen.clear();
                                } // bounded duplicate history; never discard observations
                                self.messages += 1;
                                let elapsed = self.journal.elapsed();
                                let time = store::utc();
                                for o in &message.observations {
                                    let key = format!("{connection}:{}", o.key());
                                    if let Some(n) = o.numeric {
                                        if self.history.len() == 4000 {
                                            self.history.pop_front();
                                        }
                                        self.history.push_back((elapsed, key.clone(), n));
                                    }
                                    if self.latest.len() < 256 || self.latest.contains_key(&key) {
                                        self.latest.insert(
                                            key,
                                            LiveValue {
                                                observation: o.clone(),
                                                utc: time,
                                                elapsed_s: elapsed,
                                                connection: connection.clone(),
                                            },
                                        );
                                    }
                                }
                                self.event(
                                    "message",
                                    json!({"connection":connection,"peer":peer,"message":message}),
                                )?;
                                if self.profile.acknowledge {
                                    if let Some(a) = protocol::acknowledgment(&message) {
                                        ack.extend(a);
                                    }
                                }
                            }
                            Err(e) => {
                                self.event(
                                    "decode_error",
                                    json!({"detail":e.to_string(),"connection":connection}),
                                )?;
                            }
                        }
                    }
                }
                self.wire.sync_data()?;
                self.journal.sync()?;
                if let Some(reply) = reply {
                    if !ack.is_empty() {
                        self.event("ack_attempt", json!({"connection":connection,"bytes":ack}))?;
                        self.journal.sync()?;
                    }
                    let _ = reply.send(ack);
                }
            }
        }
        Ok(())
    }
    pub fn start_mic(&mut self, device: &str) -> Result<()> {
        ensure!(self.mic.is_none(), "Microphone is already recording");
        self.mic = Some(Microphone::start(
            self.journal.dir.clone(),
            device,
            self.journal.elapsed(),
        )?);
        self.event("microphone_start", json!({"device":device}))?;
        Ok(())
    }
    fn poll_audio(&mut self) -> Result<()> {
        loop {
            let event = self.mic.as_ref().and_then(|m| m.rx.try_recv().ok());
            match event {
                Some(AudioEvent::Closed(clip)) => {
                    self.event("audio_closed", serde_json::to_value(clip)?)?;
                    self.journal.sync()?;
                }
                Some(AudioEvent::Error(error)) => {
                    self.status = error.clone();
                    self.event("audio_error", json!({"detail":error}))?;
                }
                None => break,
            }
        }
        Ok(())
    }
    pub fn stop_mic(&mut self) -> Result<()> {
        if let Some(mic) = self.mic.as_mut() {
            mic.stop();
        }
        self.poll_audio()?;
        if self.mic.take().is_some() {
            self.event("microphone_pause", json!({}))?;
            self.journal.sync()?;
        }
        Ok(())
    }
    pub fn stop(&mut self, root: &Root, interrupted: bool) -> Result<()> {
        self.stop_mic()?;
        if let Some(mut video) = self.video.take() {
            match video.stop() {
                Ok(info) => {
                    self.event("video_closed", info)?;
                }
                Err(e) => {
                    self.event("video_error", json!({"detail":e.to_string()}))?;
                }
            }
        }
        if let Some(worker) = self.worker.take() {
            worker.request_stop();
            while !worker.finished() {
                if let Ok(input) = worker.rx.recv_timeout(Duration::from_millis(100)) {
                    self.input(input)?;
                }
            }
            while let Ok(input) = worker.rx.try_recv() {
                self.input(input)?;
            }
        }
        self.event("stop", json!({"interrupted":interrupted}))?;
        self.wire.sync_all()?;
        if let Some(csv) = &mut self.binary_csv {
            csv.sync()?;
        }
        self.journal.sync()?;
        self.session.state = if interrupted {
            "interrupted"
        } else {
            "completed"
        }
        .into();
        if let Some(segment) = self.session.segments.last_mut() {
            segment.state = self.session.state.clone();
            segment.ended = Some(store::utc());
            store::write_json(&self.journal.dir.join("segment.json"), segment)?;
        }
        root.save(&self.session)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_stream_records_blank_gaps_and_recovers_without_fabricated_samples() {
        let temp = tempfile::tempdir().unwrap();
        let root = Root::open(temp.path().join("data")).unwrap();
        let session = root.create("Bench", "Missing stream", false).unwrap();
        let profile = SourceConfig {
            mode: transport::Source::Notes,
            hl7: false,
            ..Default::default()
        };
        let mut rec = Recorder::start(&root, session, profile).unwrap();
        rec.poll().unwrap();
        assert!(!rec.events.iter().any(|e| e.kind == "binary_gap"));
        std::thread::sleep(Duration::from_millis(3100));
        rec.poll().unwrap();
        rec.poll().unwrap();
        assert_eq!(
            rec.events.iter().filter(|e| e.kind == "binary_gap").count(),
            1
        );
        assert_eq!(rec.bytes, 0);
        assert_eq!(rec.binary_records, 0);
        let mut bytes = vec![0; crate::waveline::RECORD_LEN];
        bytes[..4].copy_from_slice(&crate::waveline::MAGIC);
        bytes[4..8].copy_from_slice(&(crate::waveline::RECORD_LEN as u32).to_le_bytes());
        rec.input(Input::Bytes {
            connection: "test".into(),
            peer: "10.0.0.3:1234".into(),
            bytes,
            reply: None,
            received_utc: chrono::Utc::now(),
            received_at: Instant::now(),
        })
        .unwrap();
        rec.poll().unwrap();
        assert_eq!(rec.binary_records, 1);
        rec.stop(&root, false).unwrap();
        let rows = csv::Reader::from_path(rec.journal.dir.join("waveline_live.csv"))
            .unwrap()
            .records()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(&rows[0][21], "missing");
        assert!(rows[0].iter().skip(8).take(12).all(str::is_empty));
        assert_eq!(&rows[1][21], "record");
        assert_eq!(&rows[1][22], "receiving");
        assert_eq!(&rows[1][20], "candidate");
        assert!(rows[1].iter().skip(17).take(3).all(|cell| cell == "0"));
        let dir = crate::export::csvs(&root.session_dir(&rec.session.id)).unwrap();
        let rows = csv::Reader::from_path(dir.join("arterial_pressure.csv"))
            .unwrap()
            .records()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(&rows[0][9], "stale");
        assert_eq!(&rows[1][9], "candidate");
        assert!(rows[1].iter().skip(6).take(3).all(|cell| cell == "0"));
    }
    #[test]
    fn queued_fragments_keep_receipt_precision_and_flush_live_csv_before_stop() {
        let temp = tempfile::tempdir().unwrap();
        let root = Root::open(temp.path().join("data")).unwrap();
        let session = root.create("Bench", "Receipt timestamps", false).unwrap();
        let profile = SourceConfig {
            mode: transport::Source::Notes,
            hl7: false,
            ..Default::default()
        };
        let mut rec = Recorder::start(&root, session, profile).unwrap();
        let path = rec.journal.dir.join("waveline_live.csv");
        let read_csv = || {
            csv::Reader::from_path(&path)
                .unwrap()
                .records()
                .map(Result::unwrap)
                .collect::<Vec<_>>()
        };
        assert!(read_csv().is_empty());
        let mut record = vec![0; crate::waveline::RECORD_LEN];
        record[..4].copy_from_slice(&crate::waveline::MAGIC);
        record[4..8].copy_from_slice(&(crate::waveline::RECORD_LEN as u32).to_le_bytes());
        record[14..16].copy_from_slice(&91_u16.to_le_bytes());
        record[30..32].copy_from_slice(&77_u16.to_le_bytes());
        record[72..76].copy_from_slice(&f32::NAN.to_le_bytes());
        let first = chrono::DateTime::parse_from_rfc3339("2026-10-20T14:09:07.123456700Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let received = chrono::DateTime::parse_from_rfc3339("2026-10-20T14:09:07.123456789Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let input = |bytes, utc, instant| Input::Bytes {
            connection: "connection-1".into(),
            peer: "10.0.0.3:1234".into(),
            bytes,
            reply: None,
            received_utc: utc,
            received_at: instant,
        };
        rec.input(input(record[..500].to_vec(), first, Instant::now()))
            .unwrap();
        assert!(read_csv().is_empty(), "Do not publish incomplete records");
        assert!(rec.last_binary_received_utc.is_none());
        let received_at = Instant::now();
        let received_elapsed = rec.journal.elapsed_at(received_at);
        let mut remainder = record[500..].to_vec();
        remainder.extend(&record);
        let queued = input(remainder, received, received_at);
        std::thread::sleep(Duration::from_millis(25));
        rec.input(queued).unwrap();
        assert_eq!(rec.last_binary_received_utc, Some(received));
        let rows = read_csv();
        assert_eq!(
            rows.len(),
            2,
            "Live rows must be readable before Stop & save"
        );
        for row in &rows {
            assert_eq!(&row[0], "2026-10-20T14:09:07.123456789Z");
            assert_eq!(&row[1], &format!("{received_elapsed:.9}"));
            assert_eq!(&row[6], "transport_read_completion");
            assert_eq!(&row[8], "91");
            assert_eq!(&row[11], "77");
            assert_eq!(&row[12], "0");
            assert_eq!(&row[16], "", "Non-finite values must remain blank");
        }
        assert_eq!(&rows[0][4], "0");
        assert_eq!(&rows[1][4], "1956");
        let decoded: Vec<_> = rec
            .events
            .iter()
            .filter(|e| e.kind == "binary_record")
            .collect();
        for event in &decoded {
            assert_eq!(
                event.data["received_utc"],
                received.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
            );
            assert_eq!(event.data["received_elapsed_s"], received_elapsed);
            assert!(
                event.elapsed_s - received_elapsed >= 0.020,
                "Processing delay must not change receipt time"
            );
        }
        rec.stop(&root, false).unwrap();
        let exports = crate::export::csvs(&root.session_dir(&rec.session.id)).unwrap();
        let exported = csv::Reader::from_path(exports.join("waveline_candidates.csv"))
            .unwrap()
            .records()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(exported.len(), 18);
        for row in &exported {
            assert_eq!(
                chrono::DateTime::parse_from_rfc3339(&row[2]).unwrap(),
                received
            );
            assert_eq!(row[3].parse::<f64>().unwrap(), received_elapsed);
            assert_eq!(&row[15], "transport_read_completion");
        }
        assert_eq!(read_csv().len(), 2, "Stopping must not duplicate live rows");
    }
}
