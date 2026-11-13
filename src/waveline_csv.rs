//! Derived CSV views of saved wire records. Source captures are read-only.
use crate::{export::safe_cell, replay::Capture, waveline::CSV_HEADER};
use anyhow::Result;
use chrono::SecondsFormat;
use std::{collections::HashMap, fs, path::Path};

const CHANNELS: [(usize, &str); 4] = [
    (180, "ecg"),
    (356, "pleth"),
    (424, "respiration"),
    (668, "p1"),
];

/// Write a wide receipt table and lossless sample rows from today's decoder.
/// The caller selects the destination; the CLI requires a new directory.
pub fn write_capture(capture: &Capture, dest: &Path) -> Result<()> {
    let mut readings = csv::Writer::from_path(dest.join("waveline_readings.csv"))?;
    let mut header = vec!["record_id", "segment_id"];
    header.extend(CSV_HEADER);
    header.extend([
        "ecg_state_raw",
        "pleth_state_raw",
        "p1_state_raw",
        "p2_state_raw",
        "previous_received_utc",
        "receipt_interval_s",
        "ecg_samples_per_receipt_second_estimate",
        "pleth_samples_per_receipt_second_estimate",
        "respiration_samples_per_receipt_second_estimate",
        "p1_samples_per_receipt_second_estimate",
        "sample_clock_verified",
    ]);
    readings.write_record(header)?;
    let mut samples = csv::Writer::from_path(dest.join("waveline_waveforms.csv"))?;
    samples.write_record([
        "record_id",
        "received_utc",
        "channel",
        "field_offset",
        "sample_index",
        "sample_count",
        "raw_value",
        "units",
    ])?;
    // Batched timestamps must not be interpreted as a sample clock. Keep every
    // block, including duplicates, disconnected traces, zeros and clipped values.
    let mut counts = HashMap::new();
    for frame in &capture.frames {
        *counts
            .entry((&frame.segment, &frame.connection, frame.utc))
            .or_insert(0_usize) += 1;
    }
    let mut previous = HashMap::new();
    for (i, frame) in capture.frames.iter().enumerate() {
        let record_id = (i + 1).to_string();
        let utc = frame.utc.to_rfc3339_opts(SecondsFormat::Nanos, true);
        let prior = previous.insert((&frame.segment, &frame.connection), frame);
        let interval = prior
            .and_then(|p| (frame.utc - p.utc).num_nanoseconds())
            .map(|n| n as f64 / 1e9);
        let regular = interval.is_some_and(|s| (0.5..=1.5).contains(&s))
            && counts[&(&frame.segment, &frame.connection, frame.utc)] == 1
            && prior.is_some_and(|p| counts[&(&p.segment, &p.connection, p.utc)] == 1);
        let mut row = vec![record_id.clone(), safe_cell(&frame.segment)];
        row.extend(frame.record.csv_row(
            &frame.connection,
            &frame.peer,
            frame.utc,
            frame.elapsed_s,
            &frame.timestamp_basis,
        )?);
        let state = &frame.record.channel_status;
        row.extend([state.ecg, state.pleth, state.p1, state.p2].map(|v| v.to_string()));
        row.push(
            prior
                .map(|p| p.utc.to_rfc3339_opts(SecondsFormat::Nanos, true))
                .unwrap_or_default(),
        );
        row.push(interval.map(|s| format!("{s:.9}")).unwrap_or_default());
        for (offset, _) in CHANNELS {
            row.push(if regular {
                frame
                    .record
                    .signal_slices
                    .iter()
                    .find(|s| s.offset == offset)
                    .map(|s| format!("{:.9}", s.samples.len() as f64 / interval.unwrap()))
                    .unwrap_or_default()
            } else {
                String::new()
            });
        }
        row.push("false".into());
        readings.write_record(row)?;
        for (offset, channel) in CHANNELS {
            if let Some(signal) = frame
                .record
                .signal_slices
                .iter()
                .find(|s| s.offset == offset)
            {
                let count = signal.samples.len().to_string();
                let offset = offset.to_string();
                for (j, value) in signal.samples.iter().enumerate() {
                    samples.write_record([
                        record_id.as_str(),
                        utc.as_str(),
                        channel,
                        offset.as_str(),
                        &j.to_string(),
                        count.as_str(),
                        &value.to_string(),
                        "raw_u8",
                    ])?;
                }
            }
        }
    }
    readings.flush()?;
    samples.flush()?;
    fs::write(dest.join("waveline_csv_README.txt"), format!(
        "Waveline raw-derived CSVs: {} records.\n\
        waveline_readings.csv: one row per complete received record. Includes HR, cuff and P1 pressures, SpO2, PR, Celsius temperatures, raw state bytes and four ordered waveform JSON arrays. Original streams and older exports are never rewritten by retrospective decoding.\n\
        waveline_waveforms.csv: one row per raw sample. record_id joins the wide table. sample_index is zero-based within each channel/block, not a continuous sample clock. field_offset is the channel's zero-based byte offset in the 1956-byte record. received_utc repeats the exact timestamp of the read completing the whole record; it is NOT the acquisition time of each sample. No per-sample acquisition timestamps are invented.\n\
        Typical block lengths: ECG 72, pleth 60, respiration 128, P1 128. One numeric value per vital per record is distinct from these multiple waveform values. A repeated HR or cuff value is not a new measurement.\n\
        All waveform amplitudes are uncalibrated unsigned byte values, 0-255; units=raw_u8. ECG mV gain and P1 mmHg waveform scaling remain unresolved. Respiration is not the decoded gas/CO2 waveform. Gas values remain unmapped. Zero/flat/clipped blocks are preserved, with raw state bytes available for interpretation.\n\
        *_samples_per_receipt_second_estimate = block sample count divided by the preceding receipt interval, only for regular 0.5-1.5 second intervals within the same connection and segment and without batched timestamps at either endpoint. These estimates are NOT confirmed hardware sampling rates. First records, batches and gaps have blank rate cells. sample_clock_verified=false. Receipt timestamps retain nine fractional digits; precision is not clock accuracy.\n\
        New live waveline_live.csv files include the same four raw sample-array columns. Missing/gap rows have empty arrays. Existing live files are not edited. New column names omit the old candidate suffix; mapping_verified=false still records the decoder's experimental status.\n\
        Decoder warnings:\n{}\n", capture.frames.len(), capture.warnings.join("\n")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        replay::Frame,
        waveline::{Record, MAGIC, RECORD_LEN},
    };
    #[test]
    fn csv_keeps_every_sample_and_distinguishes_receipt_rates_from_sample_times() {
        let dir = tempfile::tempdir().unwrap();
        let start: chrono::DateTime<chrono::Utc> =
            "2026-10-20T15:57:31.123456789Z".parse().unwrap();
        let mut bytes = vec![0; RECORD_LEN];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4..8].copy_from_slice(&(RECORD_LEN as u32).to_le_bytes());
        bytes[68..72].copy_from_slice(&f32::NAN.to_le_bytes());
        bytes[40..44].copy_from_slice(&(-3_i32).to_le_bytes());
        for (offset, length) in [(180, 72), (356, 60), (424, 128), (668, 128)] {
            for j in 0..length {
                bytes[offset + j] = if j == 0 { 255 } else { j as u8 };
            }
        }
        let frames = [0, 1, 2, 2, 3, 10, 11]
            .into_iter()
            .enumerate()
            .map(|(i, sec)| Frame {
                utc: start + chrono::Duration::seconds(sec),
                segment: "segment".into(),
                connection: if i == 6 { "new" } else { "connection" }.into(),
                timestamp_basis: "transport_read_completion".into(),
                elapsed_s: Some(sec as f64),
                peer: "=metadata".into(),
                record: Record::decode(&bytes, (i * RECORD_LEN) as u64).unwrap(),
            })
            .collect();
        let capture = Capture {
            frames,
            start,
            end: start + chrono::Duration::seconds(11),
            warnings: vec![],
        };
        write_capture(&capture, dir.path()).unwrap();
        let mut input = csv::Reader::from_path(dir.path().join("waveline_readings.csv")).unwrap();
        let h = input.headers().unwrap().clone();
        let col = |name| h.iter().position(|v| v == name).unwrap();
        assert!(!h.iter().any(|s| s.ends_with("_candidate")));
        let rows: Vec<_> = input.records().map(Result::unwrap).collect();
        assert_eq!(rows.len(), 7);
        assert_eq!(
            &rows[0][col("received_utc")],
            "2026-10-20T15:57:31.123456789Z"
        );
        assert_eq!(&rows[0][col("source")], "'=metadata");
        assert_eq!(&rows[0][col("temperature_1")], "");
        assert_eq!(&rows[0][col("arterial_diastolic")], "-3");
        assert_eq!(
            &rows[1][col("ecg_samples_per_receipt_second_estimate")],
            "72.000000000"
        );
        for i in [0, 2, 3, 4, 5, 6] {
            assert_eq!(&rows[i][col("ecg_samples_per_receipt_second_estimate")], "");
        }
        for row in &rows {
            assert_eq!(&row[col("sample_clock_verified")], "false");
            let values: Vec<u8> = serde_json::from_str(&row[col("ecg_raw_samples_json")]).unwrap();
            assert_eq!(values, capture.frames[0].record.signal_slices[0].samples);
        }
        let samples: Vec<_> = csv::Reader::from_path(dir.path().join("waveline_waveforms.csv"))
            .unwrap()
            .records()
            .map(Result::unwrap)
            .collect();
        assert_eq!(samples.len(), 7 * (72 + 60 + 128 + 128));
        for (i, frame) in capture.frames.iter().enumerate() {
            for signal in &frame.record.signal_slices {
                let exported: Vec<u8> = samples
                    .iter()
                    .filter(|r| r[0] == (i + 1).to_string() && r[3] == signal.offset.to_string())
                    .map(|r| r[6].parse().unwrap())
                    .collect();
                assert_eq!(exported, signal.samples);
            }
        }
    }
}
