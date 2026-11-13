use crate::{
    model,
    protocol::Message,
    store::{self, Session},
};
use anyhow::{ensure, Result};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};
use zip::write::SimpleFileOptions;

#[derive(serde::Deserialize)]
struct BinaryRecord {
    connection: String,
    peer: String,
    connection_offset: u64,
    record_length: usize,
    experimental: bool,
    mapping_verified: bool,
    fields: Vec<BinaryField>,
    #[serde(default)]
    received_utc: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    received_elapsed_s: Option<f64>,
    #[serde(default)]
    arterial_pressure: Option<crate::waveline::ArterialPressure>,
}
#[derive(serde::Deserialize)]
struct BinaryField {
    offset: usize,
    encoding: String,
    hypothesis: String,
    value: Option<f64>,
    raw_hex: String,
}
pub fn safe_cell(s: &str) -> String {
    if s.trim_start().starts_with(['=', '+', '-', '@']) || s.starts_with(['\t', '\r', '\n']) {
        format!("'{s}")
    } else {
        s.into()
    }
}
fn row(w: &mut csv::Writer<File>, values: Vec<String>) -> Result<()> {
    w.write_record(values.iter().map(|s| safe_cell(s)))?;
    Ok(())
}
pub fn csvs(dir: &Path) -> Result<PathBuf> {
    let session: Session = store::read_json(&dir.join("session.json"))?;
    ensure!(
        session.state != "recording",
        "Stop capture before exporting"
    );
    let exports = dir.join("exports");
    // Preserve earlier exports, including the original pig 6 tables. A later
    // export gets a new directory rather than rewriting an existing schema.
    let dest = if exports.exists() {
        exports.join(format!("export-{}", store::id()))
    } else {
        exports
    };
    fs::create_dir_all(&dest)?;
    let mut obs = csv::Writer::from_path(dest.join("observations.csv"))?;
    obs.write_record([
        "segment_id",
        "sequence",
        "received_utc",
        "elapsed_s",
        "source",
        "patient_raw",
        "order_raw",
        "code",
        "label",
        "sub_id",
        "repeat_index",
        "value_raw",
        "numeric",
        "units_raw",
        "status",
        "device_time_raw",
        "control_id",
        "repeated_control_id",
    ])?;
    let mut events = csv::Writer::from_path(dest.join("events.csv"))?;
    events.write_record([
        "segment_id",
        "sequence",
        "host_utc",
        "elapsed_s",
        "kind",
        "event_id",
        "amends_id",
        "label",
        "note",
        "context_json",
    ])?;
    let mut binary = csv::Writer::from_path(dest.join("waveline_candidates.csv"))?;
    binary.write_record([
        "segment_id",
        "sequence",
        "received_utc",
        "elapsed_s",
        "connection",
        "source",
        "connection_offset",
        "record_length",
        "field_offset",
        "encoding",
        "hypothesis",
        "numeric",
        "raw_hex",
        "experimental",
        "mapping_verified",
        "timestamp_basis",
        "journaled_utc",
        "journaled_elapsed_s",
    ])?;
    let mut arterial = csv::Writer::from_path(dest.join("arterial_pressure.csv"))?;
    arterial.write_record([
        "segment_id",
        "sequence",
        "host_utc",
        "elapsed_s",
        "connection",
        "source",
        "arterial_systolic",
        "arterial_diastolic",
        "arterial_mean",
        "arterial_status",
        "timestamp_basis",
        "last_complete_record_age_s",
    ])?;
    let warnings = store::visit_events(dir, |e| {
        if e.kind == "binary_record" {
            let record: BinaryRecord = serde_json::from_value(e.data.clone())?;
            let (received_utc, elapsed_s, basis) =
                match (record.received_utc, record.received_elapsed_s) {
                    (Some(utc), Some(elapsed)) => (utc, elapsed, "transport_read_completion"),
                    (None, None) => (e.utc, e.elapsed_s, "legacy_journal_processing"),
                    _ => anyhow::bail!("Incomplete receipt timestamp in binary record {}", e.id),
                };
            let pressure =
                crate::waveline::ArterialPressure::cells(record.arterial_pressure.as_ref());
            // Values are finite typed numbers; escape only text metadata so a
            // valid negative pressure remains numeric in CSV.
            arterial.write_record([
                safe_cell(&e.segment_id),
                e.sequence.to_string(),
                received_utc.to_rfc3339(),
                elapsed_s.to_string(),
                safe_cell(&record.connection),
                safe_cell(&record.peer),
                pressure[0].clone(),
                pressure[1].clone(),
                pressure[2].clone(),
                pressure[3].clone(),
                basis.into(),
                "0".into(),
            ])?;
            for field in record.fields {
                row(
                    &mut binary,
                    vec![
                        e.segment_id.clone(),
                        e.sequence.to_string(),
                        received_utc.to_rfc3339(),
                        elapsed_s.to_string(),
                        record.connection.clone(),
                        record.peer.clone(),
                        record.connection_offset.to_string(),
                        record.record_length.to_string(),
                        field.offset.to_string(),
                        field.encoding,
                        field.hypothesis,
                        field.value.map(|n| n.to_string()).unwrap_or_default(),
                        field.raw_hex,
                        record.experimental.to_string(),
                        record.mapping_verified.to_string(),
                        basis.into(),
                        e.utc.to_rfc3339(),
                        e.elapsed_s.to_string(),
                    ],
                )?;
            }
        }
        if e.kind == "binary_gap" {
            let checked_utc: chrono::DateTime<chrono::Utc> =
                serde_json::from_value(e.data["checked_utc"].clone())?;
            let elapsed: f64 = serde_json::from_value(e.data["checked_elapsed_s"].clone())?;
            let age: f64 = serde_json::from_value(e.data["last_complete_record_age_s"].clone())?;
            arterial.write_record([
                safe_cell(&e.segment_id),
                e.sequence.to_string(),
                checked_utc.to_rfc3339(),
                elapsed.to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                "stale".into(),
                "host_gap_check".into(),
                age.to_string(),
            ])?;
        }
        if e.kind == "message" {
            let msg: Message = serde_json::from_value(e.data["message"].clone())?;
            for o in msg.observations {
                row(
                    &mut obs,
                    vec![
                        e.segment_id.clone(),
                        e.sequence.to_string(),
                        e.utc.to_rfc3339(),
                        e.elapsed_s.to_string(),
                        o.source,
                        o.patient_raw,
                        o.order_raw,
                        o.code,
                        o.label,
                        o.sub_id,
                        o.repeat_index.to_string(),
                        o.value_raw,
                        o.numeric.map(|n| n.to_string()).unwrap_or_default(),
                        o.units_raw,
                        o.result_status,
                        o.device_time_raw,
                        msg.control_id.clone(),
                        msg.repeated_control_id.to_string(),
                    ],
                )?;
            }
        }
        if e.kind == "marker" || e.kind == "annotation" {
            row(
                &mut events,
                vec![
                    e.segment_id,
                    e.sequence.to_string(),
                    e.utc.to_rfc3339(),
                    e.elapsed_s.to_string(),
                    e.kind,
                    e.id,
                    e.data["amends_id"].as_str().unwrap_or("").into(),
                    e.data["label"].as_str().unwrap_or("").into(),
                    e.data["note"].as_str().unwrap_or("").into(),
                    e.data["context"].to_string(),
                ],
            )?;
        }
        Ok(())
    })?;
    if let Some(capture) = crate::replay::Capture::load_optional(dir)? {
        crate::waveline_csv::write_capture(&capture, &dest)?;
    }
    obs.flush()?;
    events.flush()?;
    binary.flush()?;
    arterial.flush()?;
    let mut transcripts = csv::Writer::from_path(dest.join("transcripts.csv"))?;
    transcripts.write_record([
        "clip_id",
        "audio_started_utc",
        "segment_elapsed_s",
        "engine",
        "text_unverified",
    ])?;
    let mut words = csv::Writer::from_path(dest.join("transcript_words.csv"))?;
    words.write_record([
        "clip_id",
        "text_unverified",
        "start_offset_s",
        "end_offset_s",
        "estimated_start_utc",
        "estimated_end_utc",
        "timings_within_clip",
    ])?;
    for t in model::transcripts(dir)? {
        row(
            &mut transcripts,
            vec![
                t.clip_id.clone(),
                t.audio_started.to_rfc3339(),
                t.segment_elapsed_s.to_string(),
                t.engine,
                t.text,
            ],
        )?;
        for w in t.words {
            let absolute = |offset: f64| {
                if t.timings_valid {
                    (t.audio_started + chrono::Duration::microseconds((offset * 1e6) as i64))
                        .to_rfc3339()
                } else {
                    String::new()
                }
            };
            row(
                &mut words,
                vec![
                    t.clip_id.clone(),
                    w.text,
                    w.start.to_string(),
                    w.end.to_string(),
                    absolute(w.start),
                    absolute(w.end),
                    t.timings_valid.to_string(),
                ],
            )?;
        }
    }
    transcripts.flush()?;
    words.flush()?;
    fs::write(dest.join("READ_ME.txt"),format!("Pig Pal schema 2. Synthetic: {}.\nOriginal records: segments/*/journal.ndjson, wire.bin, audio WAVs and sidecars.\nUTC is host time, not monitor acquisition time. Elapsed time restarts in each segment.\nWaveline timestamp_basis distinguishes transport_read_completion from legacy_journal_processing. New records use the read that completes the record; multiple records in one read share a timestamp. Full timestamp precision is retained. journaled_utc and journaled_elapsed_s preserve processing times separately.\nEach new raw segment also includes waveline_live.csv, flushed after every complete record, with nine fractional UTC digits and monotonic elapsed time. Decimal precision does not establish clock accuracy or monitor acquisition timing.\nArterial SYS/DIA/MAP values decode signed i32 LE bytes 36/40/44, supported by a connected P1 display reference. Legacy journal records without arterial decoding remain blank/unmapped; original wire bytes are retained for retrospective decoding. Channel validity and units remain unverified. Cuff pressures are separate. After >3 seconds without complete records, new live files write blank missing rows at most once per second (host_gap_check). Unchanged numbers are not proof of staleness; channel validity flags remain unknown.\nRepeated cuff values are not new cuff measurements. No interpolation or guessed vendor code mapping.\nLegacy-format binary fields are in waveline_candidates.csv, one row per field per record, separate from observations.csv. Names and units remain hypotheses. Zeros and other possible unavailable codes are preserved literally; validity and freshness are unknown. Connection offsets restart on reconnect. Original signal bytes remain in wire.bin.\nNew waveline_readings.csv and waveline_waveforms.csv reconstruct all mapped scalar and waveform values from the saved raw stream. The readings table has complete ordered JSON sample arrays; the waveform table has one row per raw byte sample. Only block receipt times are known; sample indices are not exact acquisition times. Receipt-based rate estimates are explicitly labeled and blank for gaps/batches. See waveline_csv_README.txt.\nAudio pauses remain gaps. Text and word timestamps are unverified model outputs. Original audio is preserved.\nCSV text is escaped against spreadsheet formulas. Original text remains in JSON.\nArchives are not encrypted.\n{}\n",session.synthetic,warnings.join("\n")))?;
    Ok(dest)
}
pub fn archive(dir: &Path, dest: &Path, media: bool) -> Result<()> {
    csvs(dir)?;
    let parent = dest.parent().unwrap();
    fs::create_dir_all(parent)?;
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    let mut zip = zip::ZipWriter::new(temp);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .large_file(true);
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry?;
        ensure!(
            !entry.file_type().is_symlink(),
            "Refusing to follow a symlink in a recording archive"
        );
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(dir)?;
        if !media
            && relative
                .components()
                .any(|c| matches!(c.as_os_str().to_str(), Some("audio" | "video" | "photos")))
        {
            continue;
        }
        let name = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        zip.start_file(name, options)?;
        std::io::copy(&mut File::open(entry.path())?, &mut zip)?;
    }
    let mut temp = zip.finish()?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(dest).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        recorder::Recorder,
        store::Root,
        transport::{Source, SourceConfig},
    };

    #[test]
    fn csv_preserves_saved_elapsed_float_and_distinguishes_legacy_timestamps() {
        let temp = tempfile::tempdir().unwrap();
        let root = Root::open(temp.path().join("data")).unwrap();
        let session = root.create("Bench", "CSV precision", false).unwrap();
        let mut rec = Recorder::start(
            &root,
            session,
            SourceConfig {
                mode: Source::Notes,
                hl7: false,
                ..Default::default()
            },
        )
        .unwrap();
        let mut data = serde_json::json!({
            "connection":"test", "peer":"10.0.0.3:1234", "connection_offset":0,
            "record_length":1956,"experimental":true,"mapping_verified":false,
            "fields":[{"offset":14,"encoding":"u16 LE","hypothesis":"Heart rate","value":91.0,"raw_hex":"5b 00"}],
            "received_utc":"2026-10-20T14:09:07.123456789Z",
            "received_elapsed_s":50.450164333000004_f64,
            "arterial_pressure":{"systolic":0.0,"diastolic":-2.0,"mean":null}
        });
        rec.event("binary_record", data.clone()).unwrap();
        data.as_object_mut().unwrap().remove("received_utc");
        data.as_object_mut().unwrap().remove("received_elapsed_s");
        data.as_object_mut().unwrap().remove("arterial_pressure");
        rec.event("binary_record", data).unwrap();
        let legacy = rec.events.back().unwrap().clone();
        rec.event(
            "binary_gap",
            serde_json::json!({
                "checked_utc":"2026-10-20T14:09:11.123456789Z",
                "checked_elapsed_s":54.450164333000004_f64,
                "last_complete_record_age_s":4.0,
            }),
        )
        .unwrap();
        rec.stop(&root, false).unwrap();
        let dir = csvs(&root.session_dir(&rec.session.id)).unwrap();
        let rows = csv::Reader::from_path(dir.join("waveline_candidates.csv"))
            .unwrap()
            .records()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(&rows[0][3], "50.450164333000004");
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339(&rows[0][2])
                .unwrap()
                .timestamp_subsec_nanos(),
            123456789
        );
        assert_eq!(&rows[0][15], "transport_read_completion");
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339(&rows[1][2]).unwrap(),
            legacy.utc
        );
        assert_eq!(rows[1][3].parse::<f64>().unwrap(), legacy.elapsed_s);
        assert_eq!(&rows[1][15], "legacy_journal_processing");
        let pressure = csv::Reader::from_path(dir.join("arterial_pressure.csv"))
            .unwrap()
            .records()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        assert_eq!(pressure.len(), 3);
        assert_eq!(&pressure[0][6], "0");
        assert_eq!(&pressure[0][7], "-2");
        assert_eq!(&pressure[0][8], "");
        assert_eq!(&pressure[0][9], "received");
        assert!(pressure[1].iter().skip(6).take(3).all(str::is_empty));
        assert_eq!(&pressure[1][9], "unmapped");
        assert!(pressure[2].iter().skip(6).take(3).all(str::is_empty));
        assert_eq!(&pressure[2][9], "stale");
        assert_eq!(&pressure[2][10], "host_gap_check");
        let original = std::fs::read(dir.join("waveline_candidates.csv")).unwrap();
        let next = csvs(&root.session_dir(&rec.session.id)).unwrap();
        assert_ne!(next, dir);
        assert_eq!(
            std::fs::read(dir.join("waveline_candidates.csv")).unwrap(),
            original
        );
        assert!(next.join("waveline_candidates.csv").exists());
    }
}
