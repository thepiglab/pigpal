//! Experimental structural decoder for the observed A.3.02 TCP stream.
//! Field meanings, validity flags, units, and signal sampling rates are unverified.
//! These records never become clinical/HL7 Observations or marker context.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
};

pub const MAGIC: [u8; 4] = [1, 2, 0x5b, 0xf6];
pub const RECORD_LEN: usize = 1956;
pub const STALE_AFTER_S: f64 = 3.0;

/// Separate from cuff-pressures. None at the record level means
/// no arterial decoding was persisted (for example, a legacy record).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ArterialPressure {
    pub systolic: Option<f64>,
    pub diastolic: Option<f64>,
    pub mean: Option<f64>,
}
impl ArterialPressure {
    pub fn cells(sample: Option<&Self>) -> [String; 4] {
        let Some(sample) = sample else {
            return [
                String::new(),
                String::new(),
                String::new(),
                "unmapped".into(),
            ];
        };
        let values =
            [sample.systolic, sample.diastolic, sample.mean].map(|v| v.filter(|v| v.is_finite()));
        [
            values[0].map(|v| v.to_string()).unwrap_or_default(),
            values[1].map(|v| v.to_string()).unwrap_or_default(),
            values[2].map(|v| v.to_string()).unwrap_or_default(),
            if values.iter().any(Option::is_some) {
                "received"
            } else {
                "unavailable"
            }
            .into(),
        ]
    }
}

pub const CSV_HEADER: [&str; 28] = [
    "received_utc",
    "elapsed_s",
    "connection",
    "source",
    "connection_offset",
    "record_length",
    "timestamp_basis",
    "mapping_verified",
    "heart_rate",
    "respiration",
    "systolic",
    "diastolic",
    "mean_pressure",
    "spo2",
    "pulse_rate",
    "temperature_1",
    "temperature_2",
    "arterial_systolic",
    "arterial_diastolic",
    "arterial_mean",
    "arterial_status",
    "row_type",
    "stream_status",
    "last_complete_record_age_s",
    "ecg_raw_samples_json",
    "pleth_raw_samples_json",
    "respiration_raw_samples_json",
    "p1_raw_samples_json",
];

/// A derived, append-only view; wire.bin and the journal remain the originals.
pub struct LiveCsv {
    writer: csv::Writer<File>,
    last_record_elapsed: Option<f64>,
    last_gap_elapsed: Option<f64>,
}
impl LiveCsv {
    pub fn create(dir: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join("waveline_live.csv"))?;
        let mut writer = csv::Writer::from_writer(file);
        writer.write_record(CSV_HEADER)?;
        writer.flush()?;
        Ok(Self {
            writer,
            last_record_elapsed: None,
            last_gap_elapsed: None,
        })
    }
    pub fn append(
        &mut self,
        record: &Record,
        connection: &str,
        peer: &str,
        utc: chrono::DateTime<chrono::Utc>,
        elapsed_s: f64,
    ) -> Result<()> {
        let row = record.csv_row(
            connection,
            peer,
            utc,
            Some(elapsed_s),
            "transport_read_completion",
        )?;
        // All text columns come from local socket metadata, not device text.
        self.writer.write_record(row)?;
        // Expose each complete record to tail/readers before capture stops.
        self.writer.flush()?;
        self.last_record_elapsed = Some(elapsed_s);
        self.last_gap_elapsed = None;
        Ok(())
    }
    /// Missing rows are gap observations, never fabricated monitor records.
    pub fn append_stale(
        &mut self,
        utc: chrono::DateTime<chrono::Utc>,
        elapsed_s: f64,
    ) -> Result<Option<f64>> {
        let age = elapsed_s - self.last_record_elapsed.unwrap_or(0.0);
        if age <= STALE_AFTER_S
            || self
                .last_gap_elapsed
                .is_some_and(|last| elapsed_s - last < 1.0)
        {
            return Ok(None);
        }
        let mut row = vec![String::new(); CSV_HEADER.len()];
        row[0] = utc.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        row[1] = format!("{elapsed_s:.9}");
        row[6] = "host_gap_check".into();
        row[7] = "false".into();
        row[20] = "stale".into();
        row[21] = "missing".into();
        row[22] = "stale".into();
        row[23] = format!("{age:.9}");
        self.writer.write_record(row)?;
        self.writer.flush()?;
        self.last_gap_elapsed = Some(elapsed_s);
        Ok(Some(age))
    }
    pub fn sync(&mut self) -> Result<()> {
        self.writer.flush()?;
        self.writer.get_ref().sync_data()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Field {
    pub offset: usize,
    pub encoding: &'static str,
    pub hypothesis: &'static str,
    pub value: Option<f64>,
    pub raw_hex: String,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SignalSlice {
    pub offset: usize,
    pub hypothesis: &'static str,
    pub samples: Vec<u8>,
}
/// Raw channel state bytes retained separately from the numeric candidates.
/// Only the exact photo-supported signature is used to hide a missing SpO2 display.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChannelStatus {
    pub ecg: u8,
    pub pleth: u8,
    pub p1: u8,
    pub p2: u8,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Record {
    /// Byte offset within this connection/file, not a hardware timestamp.
    pub stream_offset: u64,
    pub fields: Vec<Field>,
    pub signal_slices: Vec<SignalSlice>,
    pub channel_status: ChannelStatus,
    pub arterial_pressure: Option<ArterialPressure>,
}
impl Record {
    /// One receipt row retains every waveform sample in ordered JSON arrays.
    /// No per-sample acquisition time or calibrated amplitude is invented.
    pub fn csv_row(
        &self,
        connection: &str,
        peer: &str,
        utc: chrono::DateTime<chrono::Utc>,
        elapsed_s: Option<f64>,
        timestamp_basis: &str,
    ) -> Result<Vec<String>> {
        let mut row = vec![
            utc.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            elapsed_s
                .filter(|v| v.is_finite())
                .map(|v| format!("{v:.9}"))
                .unwrap_or_default(),
            crate::export::safe_cell(connection),
            crate::export::safe_cell(peer),
            self.stream_offset.to_string(),
            RECORD_LEN.to_string(),
            timestamp_basis.into(),
            "false".into(),
        ];
        row.extend(self.fields.iter().map(|f| {
            f.value
                .filter(|v| v.is_finite())
                .map(|v| v.to_string())
                .unwrap_or_default()
        }));
        row.extend(ArterialPressure::cells(self.arterial_pressure.as_ref()));
        row.extend(["record".into(), "receiving".into(), "0".into()]);
        for offset in [180, 356, 424, 668] {
            row.push(
                self.signal_slices
                    .iter()
                    .find(|s| s.offset == offset)
                    .map(|s| serde_json::to_string(&s.samples))
                    .transpose()?
                    .unwrap_or_default(),
            );
        }
        Ok(row)
    }
    pub fn spo2_sensor_off_candidate(&self) -> bool {
        self.channel_status.pleth == 2
            && [64, 66].into_iter().all(|offset| {
                self.fields
                    .iter()
                    .any(|f| f.offset == offset && f.value == Some(0.0))
            })
    }
    pub fn decode(data: &[u8], stream_offset: u64) -> Result<Self> {
        ensure!(data.len() == RECORD_LEN, "Unexpected binary record size");
        ensure!(data[..4] == MAGIC, "Unrecognized binary record signature");
        ensure!(
            u32::from_le_bytes(data[4..8].try_into().unwrap()) == RECORD_LEN as u32,
            "Binary record length mismatch"
        );
        let mut fields = Vec::new();
        for (offset, hypothesis) in [
            (14, "Heart rate"),
            (20, "Respiration (not CO2 RR)"),
            (28, "Systolic pressure"),
            (30, "Diastolic pressure"),
            (32, "Mean pressure"),
            (64, "Saturation"),
            (66, "Pulse rate"),
        ] {
            fields.push(Field {
                offset,
                encoding: "u16 LE",
                hypothesis,
                value: Some(
                    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap()) as f64,
                ),
                raw_hex: hex(&data[offset..offset + 2]),
            });
        }
        for (offset, hypothesis) in [(68, "Temperature 1 (Celsius)"), (72, "Temperature 2")] {
            let value = f32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
            fields.push(Field {
                offset,
                encoding: "f32 LE",
                hypothesis,
                value: value.is_finite().then_some(f64::from(value)),
                raw_hex: hex(&data[offset..offset + 4]),
            });
        }
        Ok(Self {
            stream_offset,
            fields,
            channel_status: ChannelStatus {
                ecg: data[89],
                pleth: data[349],
                p1: data[661],
                p2: data[797],
            },
            // Connected P1 display reference, October 20: approximately 73/33,
            // MAP 44, while the independent cuff triplet held at 76/60/65.
            // Negative values in the captured stream establish signed 32-bit
            // encoding. Channel validity/update flags are still unknown:
            // preserve raw candidates, including zero and unchanged values.
            arterial_pressure: Some(ArterialPressure {
                systolic: Some(i32::from_le_bytes(data[36..40].try_into().unwrap()) as f64),
                diastolic: Some(i32::from_le_bytes(data[40..44].try_into().unwrap()) as f64),
                mean: Some(i32::from_le_bytes(data[44..48].try_into().unwrap()) as f64),
            }),
            // Photo comparisons support channel identity, not calibration or
            // sample timestamps. Retain unsigned bytes including flat/clipped
            // blocks; these are not calibrated ECG or pressure measurements.
            signal_slices: [
                (180, 72, "ECG waveform"),
                (356, 60, "Pleth waveform"),
                (424, 128, "Respiration waveform (not CO2)"),
                (668, 128, "P1 waveform"),
            ]
            .into_iter()
            .map(|(offset, len, hypothesis)| SignalSlice {
                offset,
                hypothesis,
                samples: data[offset..offset + len].to_vec(),
            })
            .collect(),
        })
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// TCP read boundaries are irrelevant. Memory is bounded to one 1956-byte record.
#[derive(Default)]
pub struct Framer {
    pending: VecDeque<u8>,
    seen: u64,
    pub discarded: u64,
}
impl Framer {
    pub fn push(&mut self, input: &[u8]) -> Vec<Record> {
        let mut records = Vec::new();
        for &byte in input {
            self.pending.push_back(byte);
            self.seen += 1;
            loop {
                if self.pending.len() < 4 {
                    break;
                }
                let signature = self.pending.iter().take(4).copied().eq(MAGIC);
                let length_matches = self.pending.len() < 8
                    || u32::from_le_bytes(std::array::from_fn(|i| self.pending[i + 4]))
                        == RECORD_LEN as u32;
                if !signature || !length_matches {
                    self.pending.pop_front();
                    self.discarded += 1;
                    continue;
                }
                if self.pending.len() < RECORD_LEN {
                    break;
                }
                let bytes: Vec<_> = self.pending.drain(..).collect();
                // Signature, fixed size, and declared length were checked above.
                if let Ok(record) = Record::decode(&bytes, self.seen - RECORD_LEN as u64) {
                    records.push(record);
                }
                break;
            }
        }
        records
    }
    pub fn pending_bytes(&self) -> usize {
        self.pending.len()
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Inspection {
    pub records: u64,
    pub discarded_bytes: u64,
    pub trailing_bytes: usize,
    pub last: Option<Record>,
}
pub fn inspect_file(path: &Path) -> Result<Inspection> {
    let file = File::open(path)?;
    // A stable byte-count snapshot also permits inspecting an active capture.
    let limit = file.metadata()?.len();
    let mut input = file.take(limit);
    let mut framer = Framer::default();
    let mut result = Inspection::default();
    let mut buf = [0; 8192];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for record in framer.push(&buf[..n]) {
            result.records += 1;
            result.last = Some(record);
        }
    }
    result.discarded_bytes = framer.discarded;
    result.trailing_bytes = framer.pending_bytes();
    Ok(result)
}
pub fn inspect_session(dir: &Path) -> Result<Inspection> {
    let session: crate::store::Session = crate::store::read_json(&dir.join("session.json"))?;
    let mut result = Inspection::default();
    for segment in session.segments {
        let path = dir.join("segments").join(segment.id).join("wire.bin");
        let next = inspect_file(&path)?;
        result.records += next.records;
        result.discarded_bytes += next.discarded_bytes;
        result.trailing_bytes += next.trailing_bytes;
        if next.last.is_some() {
            result.last = next.last;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Vec<u8> {
        let mut b = vec![0; RECORD_LEN];
        b[..4].copy_from_slice(&MAGIC);
        b[4..8].copy_from_slice(&(RECORD_LEN as u32).to_le_bytes());
        b[14..16].copy_from_slice(&108_u16.to_le_bytes());
        b[68..72].copy_from_slice(&19.6_f32.to_le_bytes());
        for (i, v) in b[180..252].iter_mut().enumerate() {
            *v = i as u8;
        }
        b
    }
    #[test]
    fn photo_reference_keeps_gas_rr_separate_and_retains_p1_wave_bytes() {
        let mut b = sample();
        // 11:57:32 EDT photo: HR 110, P1 127/87/100, T1 98.6 F.
        // The preceding receipt has raw RESP 54, whereas photo gas RR is 7.
        b[14..16].copy_from_slice(&110_u16.to_le_bytes());
        b[20..22].copy_from_slice(&54_u16.to_le_bytes());
        b[68..72].copy_from_slice(&37.0_f32.to_le_bytes());
        for (offset, value) in [(36, 127_i32), (40, 87), (44, 100)] {
            b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        // Deliberately include endpoints to catch normalization or clipping.
        for (i, value) in b[668..796].iter_mut().enumerate() {
            *value = if i % 2 == 0 { 0 } else { 255 };
        }
        let r = Record::decode(&b, 0).unwrap();
        assert_eq!(r.fields.len(), 9); // Live CSV schema remains compatible.
        assert_eq!(r.fields[1].value, Some(54.0));
        assert!(r.fields[1].hypothesis.contains("not CO2 RR"));
        assert_eq!(r.fields[7].value, Some(37.0));
        assert_eq!(
            ArterialPressure::cells(r.arterial_pressure.as_ref()),
            ["127", "87", "100", "received"]
        );
        assert_eq!(r.signal_slices.len(), 4);
        assert_eq!(r.signal_slices[3].offset, 668);
        assert_eq!(r.signal_slices[3].samples, b[668..796]);
    }
    #[test]
    fn every_fragment_boundary_preserves_raw_numbers_and_signal_slices() {
        let b = sample();
        let expected = Record::decode(&b, 0).unwrap();
        for cut in 0..=b.len() {
            let mut f = Framer::default();
            let mut records = f.push(&b[..cut]);
            records.extend(f.push(&b[cut..]));
            assert_eq!(records, [expected.clone()]);
            assert_eq!(f.discarded, 0);
            assert_eq!(f.pending_bytes(), 0);
        }
        assert_eq!(expected.fields[0].value, Some(108.0));
        assert_eq!(expected.fields[7].value, Some(f64::from(19.6_f32)));
        assert_eq!(
            expected.signal_slices[0].samples,
            (0..72).collect::<Vec<u8>>()
        );
    }
    #[test]
    fn garbage_invalid_lengths_multiple_records_and_partial_tail_are_bounded() {
        let b = sample();
        let mut bad = b[..8].to_vec();
        bad[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut input = vec![0x55; 10000];
        input.extend(&bad);
        input.extend(&b);
        input.extend(&b);
        input.extend(&b[..100]);
        let mut f = Framer::default();
        let mut records = vec![];
        for chunk in input.chunks(7) {
            records.extend(f.push(chunk));
            assert!(f.pending_bytes() < RECORD_LEN);
        }
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].stream_offset, 10008);
        assert_eq!(records[1].stream_offset, 10008 + RECORD_LEN as u64);
        assert_eq!(f.discarded, 10008);
        assert_eq!(f.pending_bytes(), 100);
    }
    #[test]
    fn unknown_zero_sentinel_and_nonfinite_values_are_not_clinically_normalized() {
        let mut b = sample();
        b[64..66].copy_from_slice(&65535_u16.to_le_bytes());
        b[68..72].copy_from_slice(&f32::NAN.to_le_bytes());
        let r = Record::decode(&b, 0).unwrap();
        assert_eq!(r.fields[4].value, Some(0.0));
        assert_eq!(r.fields[5].value, Some(65535.0));
        assert_eq!(r.fields[7].value, None);
        assert_eq!(r.fields[7].raw_hex, hex(&b[68..72]));
        assert!(Record::decode(&b[..100], 0).is_err());
        b[0] = 0;
        assert!(Record::decode(&b, 0).is_err());
    }
    #[test]
    fn connected_p1_reference_decodes_signed_i32_separately_from_cuff() {
        let mut bytes = sample();
        for (offset, value) in [(28, 76_u16), (30, 60), (32, 65)] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        // Captured connected P1 record: 73 / 33 (46), distinct from cuff.
        bytes[36..48].copy_from_slice(&[73, 0, 0, 0, 33, 0, 0, 0, 46, 0, 0, 0]);
        let mut record = Record::decode(&bytes, 1956).unwrap();
        assert_eq!(
            ArterialPressure::cells(record.arterial_pressure.as_ref()),
            ["73", "33", "46", "received"]
        );
        assert_eq!(record.fields[2].value, Some(76.0));
        assert_eq!(record.fields[3].value, Some(60.0));
        assert_eq!(record.fields[4].value, Some(65.0));
        // Another captured transition contains 38 / -24 (-3).
        bytes[36..48].copy_from_slice(&[38, 0, 0, 0, 232, 255, 255, 255, 253, 255, 255, 255]);
        record = Record::decode(&bytes, 0).unwrap();
        assert_eq!(
            ArterialPressure::cells(record.arterial_pressure.as_ref()),
            ["38", "-24", "-3", "received"]
        );
        let temp = tempfile::tempdir().unwrap();
        let mut writer = LiveCsv::create(temp.path()).unwrap();
        writer
            .append(&record, "p1", "monitor", chrono::Utc::now(), 1.0)
            .unwrap();
        let row = csv::Reader::from_path(temp.path().join("waveline_live.csv"))
            .unwrap()
            .records()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(
            (&row[17], &row[18], &row[19], &row[20]),
            ("38", "-24", "-3", "received")
        );
    }
    #[test]
    fn arterial_missing_and_stale_rows_never_carry_values_forward() {
        let temp = tempfile::tempdir().unwrap();
        let mut csv = LiveCsv::create(temp.path()).unwrap();
        let mut record = Record::decode(&sample(), 0).unwrap();
        record.arterial_pressure = None; // Legacy/unmapped persisted record.
        assert_eq!(ArterialPressure::cells(None), ["", "", "", "unmapped"]);
        let read = || {
            csv::Reader::from_path(temp.path().join("waveline_live.csv"))
                .unwrap()
                .records()
                .map(Result::unwrap)
                .collect::<Vec<_>>()
        };
        let utc = chrono::Utc::now();
        csv.append(&record, "test", "10.0.0.3:1", utc, 0.0).unwrap();
        assert!(read()[0].iter().skip(17).take(3).all(str::is_empty));
        assert_eq!(&read()[0][20], "unmapped");
        assert_eq!(csv.append_stale(utc, 3.0).unwrap(), None);
        assert_eq!(csv.append_stale(utc, 3.1).unwrap(), Some(3.1));
        assert_eq!(csv.append_stale(utc, 3.2).unwrap(), None);
        assert_eq!(csv.append_stale(utc, 4.2).unwrap(), Some(4.2));
        let rows = read();
        assert_eq!(rows.len(), 3);
        for gap in &rows[1..] {
            assert!(gap.iter().skip(8).take(12).all(str::is_empty));
            assert_eq!(&gap[6], "host_gap_check");
            assert_eq!(&gap[20], "stale");
            assert_eq!(&gap[21], "missing");
            assert_eq!(&gap[22], "stale");
            assert!(
                gap.iter().skip(24).all(str::is_empty),
                "Gaps must not repeat waveform arrays"
            );
        }
        // A future independently mapped decoder may supply signed/zero values.
        // Neither zero nor an unchanged value implies loss of a sample.
        record.arterial_pressure = Some(ArterialPressure {
            systolic: Some(0.0),
            diastolic: Some(-2.0),
            mean: Some(f64::NAN),
        });
        csv.append(&record, "test", "10.0.0.3:1", utc, 4.3).unwrap();
        csv.append(&record, "test", "10.0.0.3:1", utc, 5.0).unwrap();
        let rows = read();
        assert_eq!(&rows[3][17], "0");
        assert_eq!(&rows[3][18], "-2");
        assert_eq!(&rows[3][19], "");
        assert_eq!(&rows[3][20], "received");
        assert_eq!(&rows[4][22], "receiving");
        assert_eq!(csv.append_stale(utc, 7.9).unwrap(), None);
        assert_eq!(
            csv.append_stale(utc, 8.1).unwrap(),
            Some(3.0999999999999996)
        );
        record.arterial_pressure = Some(ArterialPressure::default());
        csv.append(&record, "test", "10.0.0.3:1", utc, 8.2).unwrap();
        let rows = read();
        assert!(rows
            .last()
            .unwrap()
            .iter()
            .skip(17)
            .take(3)
            .all(str::is_empty));
        assert_eq!(&rows.last().unwrap()[20], "unavailable");
    }
}
