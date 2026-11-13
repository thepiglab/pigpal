# Experimental Waveline binary decoder

This is a partial reverse engineering of records captured from one Waveline Pro running A.3.02. It is a research inspector. Timestamped photos now support several numeric identities and raw waveform candidates, described below. Complete validity rules, calibration, and acquisition timing remain unresolved; there is no vendor protocol specification.


The [protocol research and photo reference table](WAVELINE_PROTOCOL_RESEARCH.md) documents online sources, unit-conversion searches, P1 scaling evidence, and the unresolved gas/ECG calibration limits.

## Observed framing

The monitor initiates TCP to the receiver at 10.0.0.2:12345. All 740 complete records in the two saved segments have:

- Bytes 0–3: `01 02 5b f6`.
- Bytes 4–7: little-endian uint32 length, 1956 (`a4 07 00 00`).
- Total size: 1956 bytes.
- Typical record receipt interval: approximately 0.96 seconds in this capture. Receipt timing is not a hardware sampling clock.

The Rust framer reassembles records across arbitrary TCP reads. It rejects other lengths, searches for the next matching header, retains at most one pending record, and reports unrecognized/trailing bytes. Signature and length checks do not establish a checksum or guarantee payload integrity. The original bytes remain in wire.bin regardless of recognition.

## Numeric hypotheses

Offsets are zero-based from the record start. Encodings are little-endian. Names and units are hypotheses; the app labels every value as a candidate and displays its raw encoding/offset. No normal measurement Observation is created.

| Offset | Encoding | Hypothesis | Last captured raw value |
|---|---|---|---|
| 14 | uint16 | Heart rate | 108 |
| 20 | uint16 | Respiration candidate, **not the displayed CO2 RR** | 0 |
| 28 | uint16 | Systolic pressure | 110 |
| 30 | uint16 | Diastolic pressure | 90 |
| 32 | uint16 | Mean pressure | 0 |
| 64 | uint16 | Oxygen saturation | 100 |
| 66 | uint16 | Pulse rate | 108 |
| 68 | float32 | Temperature 1 in Celsius (photo-supported) | approximately 19.6 |
| 72 | float32 | Temperature 2 | 0 |

Evidence supporting further investigation:

- Observed nonzero pressure-like triples are 103/96/98, 110/90/97, and 158/133/141. Their third values are near `(first + 2*second)/3`. This supports a pressure hypothesis but does not prove field identity or measurement validity.
- In 453 records where offsets 14 and 66 are both nonzero, their median absolute difference is one and about 97.35% differ by at most five. This is consistent with separate rate estimates, but is not an external validation.
- Offsets 64 and 66 become zero together throughout this capture. Their nonzero ranges resemble saturation and pulse data.
- Offset 68 is a finite, slowly changing float, approximately 18.2–19.6. Later photo comparisons support T1 Celsius; the earlier values have no contemporaneous readable temperature reference.

The third pressure field sometimes becomes zero while the other fields persist. Its validity/staleness semantics are unknown. Zeros and values such as 65535 are therefore never silently converted to clinical "unavailable" or accepted as valid measurements. Non-finite floats display no numeric value and preserve their original hexadecimal bits.

### User-reported display references — October 20, 2026

During segment `acc77f59-b5cc-4f43-9f2e-00cc01220628` (10:02:57–10:04:08 EDT), the user reported heart rate **91** and diastolic pressure **77**, and subsequently confirmed the fields looked correct. The 71 saved records contain heart-rate candidate values 90–91 at offset 14, ending at 91; the diastolic candidate at offset 30 stays at 77. This supports these two field identities on this unit. The reference was reported during capture, without an exact screen timestamp; it does not validate units, flags, alarm states, absent-sensor codes, or other numeric fields.

Original stream SHA-256: `95b53f96507d28d242038f097af2cabab7ba3c4d92023790e73594f0bc45fc45`. A retained copy and per-record analysis are in `../capture-analysis/waveline-2026-10-20-100257/`.


### Timestamped photo validation — October 21 analysis

The user confirmed that `IMG_1507`, `IMG_1508`, and `IMG_1511` were taken on October 20, 2026, at **08:52:51, 10:38:42, and 11:57:32 EDT**. The monitor clock is about four hours and five minutes behind; it is not used as the capture clock.

- At **14:38:42.032086 UTC**, bytes 14/28/30/64/66 match photo HR **88**, cuff **80/52**, SpO2 **95**, and PR **61**. P1 bytes 36/40/44 are **178/178/178**, visibly displayed in the photo. T1 at byte 68 is **37.4 C**, matching **99.3 F** after conversion.
- At **15:57:31.676649 UTC**, the record matches photo HR **110**, cuff **148/115**, P1 **127/87/100**, and T1 **37.0 C = 98.6 F**. SpO2/PR raw zeros and a flat pleth block accompany the photographed Sensor Off state.
- The first photo overlaps the earlier **Waveline Ethernet test** session, before the session named pig 6 began. It shows ECG Lead Off and P1/P2 Sensor Off; bytes 180–251 are flat 128, with raw P1/P2 351/351/351. Delayed, batched legacy journal timestamps prevent exact second-level matching at this time.
- Byte **20 is 19 and 54** at the later photo times, while gas RR is **12 and 7**. Its respiration interpretation remains tentative, plausibly impedance RESP, and it must not populate the CO2 RR slot. No identifiable gas numeric/waveform block was established from this capture.
- The small right-hand NIBP photo values **39 and 43** appear while **Auto measuring** is displayed; they do not validate MAP. Byte 32 is zero then and becomes nonzero with a subsequent cuff result. Transient cuff pressure is a possible screen interpretation, not a mapped Ethernet field.
- ECG bytes 180–251 show matching active/lead-off states and ECG-like complexes with rhythm consistent with the photographed HR. Pleth bytes 356–415 and P1 bytes 668–795 similarly agree with the photographed states. Raw P1 is all 255 at the 178/178/178 display, so waveform clipping cannot be treated as missing numeric pressure.

The analysis recovered **577,800 ECG-candidate bytes** from **8,025 pig 6 records**. Separate derived CSVs contain all four candidate waveform blocks and retrospective numeric fields, with sample-rate/acquisition-time fields empty and mapping_verified false. The original journals, live CSVs, saved exports, and ZIP were preserved; 64 source files were SHA-256 checked unchanged. Photo agreement supports field identities, not millisecond clock synchronization or physiological accuracy.

Local evidence and reproducible scripts are retained under `capture-analysis/pig6-photo-correlation-2026-10-21/` (Git-ignored), with `REPORT.md`, working copies, photo references, byte-variation statistics, and verified derived CSVs. Existing normal exports keep their persisted historical labels and are not silently reinterpreted.

## Signal slices

Four changing regions are displayed as unsigned raw byte slices. Photo comparisons support candidate identities, but no clinical calibration or hardware sample clock is assigned:

| Offset | Bytes shown | Interpretation |
|---|---|---|
| 180 | 72 | ECG waveform candidate; active lead was II in the reference photos |
| 356 | 60 | Pleth waveform candidate |
| 424 | 128 | Respiration waveform candidate, plausibly impedance; not CO2 |
| 668 | 128 | P1/ART waveform candidate |

Live graphs show sample index versus raw byte value (0–255) from the latest complete record. Historical replay can show multiple separate blocks over an explicitly estimated receipt-time window; it does not establish continuous acquisition timing. The candidates are supported by shape and sensor-state comparisons, not a full protocol definition. Gain, acquisition timestamps, validity bits, exact lead/channel identifiers, and sample rates need verification. Some records repeat blocks; concatenating them is not a verified continuous waveform reconstruction.

## Use and verification

Live matching records are shown automatically when generic HL7 decoding is off. Experimental records have their own counter and journal event, tagged `experimental: true` and `mapping_verified: false`. They remain separate from HL7 observations, clinical trends, and note/marker context. If no new complete record arrives for three seconds, the live panel marks its snapshot stale.

After **Stop & save**, either export button includes `exports/waveline_candidates.csv`. It has one row per candidate field per record, with segment, receipt time, connection, connection-relative record offset, field offset, encoding, numeric candidate, raw hexadecimal bytes, and experimental/mapping status. Zeros remain literal zeros; non-finite numeric values are blank with raw bytes retained. Waveform bytes remain in each segment's `wire.bin`. `observations.csv` remains reserved for normally decoded observations. An empty observations file does not mean a raw Waveline capture failed.

### Live CSV and timestamp precision

With HL7 decoding off, every new segment creates `waveline_live.csv`. Each complete Waveline record appends one wide row with the nine candidate fields. The writer flushes each row immediately, syncs periodically during recording, and syncs on stop. **Copy live CSV path** in the active monitor area copies its exact path. For example, `tail -f '/path/to/segment/waveline_live.csv'` displays new rows while the session runs. Spreadsheet apps may take a snapshot when opening a CSV; automatic spreadsheet refresh is not provided.

`received_utc` is UTC with nine fractional digits, for example `2026-10-20T14:09:07.123456789Z`; `elapsed_s` is monotonic time since the segment's recorder clock started, written to nine decimal places in the live file. New captures timestamp immediately after the transport read completes, before copying/queueing/decoding. A record split across reads uses the final read's timestamp; records completed by the same read share that timestamp. No artificial per-sample timestamps are generated.

The journal retains both transport receipt timestamps and its own event-processing timestamps. The long-form export uses `timestamp_basis=transport_read_completion` for new records and includes `journaled_utc` and `journaled_elapsed_s` separately. Older captures without transport timestamps use their unchanged event times, explicitly labeled `legacy_journal_processing`. Precision is preserved through CSV export; decimal places do not establish absolute clock accuracy. These timestamps measure receipt by the Mac application, not monitor acquisition, sensor sampling, or kernel packet arrival. TCP buffering, monitor transmission delay, and the operating-system clock still affect the relationship to measurement time. Zeros can represent unknown device states and are not independently validated measurements.

Sessions recorded before the binary decoder was added may contain only wire events. Their original bytes remain in exported ZIPs and can be inspected with the binary inspector; the CSV exporter uses persisted decoded events and does not invent historical decoding timestamps.

### Invasive arterial pressure and missing data

The cuff-pressure candidates at offsets 28/30/32 remain separate from P1. On October 20 the user connected P1 and reported approximately **73/33, MAP 44**, correcting the initial systolic typo to 73 and noting rapid changes. The contemporaneous stream's signed little-endian 32-bit integers at **36 (SYS), 40 (DIA), and 44 (MAP)** varied around that report, including 73/33/46, 73/30/44, and 68/33/44, while the cuff triplet held independently at 76/60/65. A prior connected transition included 38/-24/-3, establishing the need for signed decoding. This supports a P1 field hypothesis; it is not a precisely synchronized full-triplet validation or vendor protocol confirmation. Later photos support mmHg on this unit. Validity flags, channel timestamps, and complete absent-transducer behavior remain unverified.

New raw decoding populates `Record.arterial_pressure` and the existing `arterial_systolic_candidate`, `arterial_diastolic_candidate`, and `arterial_mean_candidate` CSV columns with these raw P1 candidates. The live UI and optional monitor view use the same decoded sample. The status is `candidate`; global `mapping_verified` remains false. Previously persisted journal records without P1 decoding remain blank/unmapped in ordinary saved exports; originals are preserved for explicit retrospective decoding. No prior CSV or journal is silently rewritten.

An explicitly unavailable/non-finite decoded component is empty, and an unavailable whole sample has status `unavailable`. There is not yet a reliable device flag to identify missing P1 within otherwise complete frames. The 10:38 photo confirms that P1 178/178/178 was actually displayed; equal values must not be classified as missing. The photos support 351/351/351 with status byte 661 or 797 set to 1 as a disconnected-state hypothesis, but a complete validity decoder is not established. Do not invent a cutoff, suppress real zero/negative values, or interpret repeated equal values as missing. P1 candidates may therefore contain an unavailable or retained device value even when the complete-record stream is fresh. P2 has no active reference. Raw waveform candidates are described above.

New live CSVs also include `row_type`, `stream_status`, and `last_complete_record_age_s`. On complete records these are `record`, `receiving`, and zero. After **more than three seconds** without a complete record, the recorder writes at most one `missing` row per second, with all numeric candidates empty, `stream_status=stale`, and the last-complete-record age. The timestamp basis is `host_gap_check`: this is the time the host observed a gap, not a new monitor measurement or packet. Matching `binary_gap` events preserve gaps in the journal and the arterial CSV export. A resumed complete stream writes fresh record rows without carrying forward old values or modifying earlier rows.

Repeated equal numbers do not imply a stale sample. A channel can retain an old value inside otherwise fresh monitor records; determining that requires the channel's validity/update flags or a device timestamp. The three-second rule detects missing complete records, not hidden per-channel staleness. Empty CSV cells are used instead of literal `NaN`, which keeps the format compatible with spreadsheet and data-analysis readers. The raw integer fields, including zeros and possible sentinel codes, remain unchanged in the journal and candidate export.

For a research test, keep **TCP listener**, bind `10.0.0.2:12345`, allowed sender `10.0.0.3`, generic HL7 off, and application ACKs off. Start a session with the intended subject and title, check that the byte/record counters advance and last-data age stays current, and add markers for the protocol events. Audio starts paused; start it separately only when needed. Stop and save before exporting, and keep the complete ZIP with the original wire and journal. Normal TCP acknowledgments are handled by the operating system even with application ACKs off.

For saved data, open a session and choose **Inspect binary capture**. The worker reads each segment in order and explicitly labels the result as a saved snapshot. The CLI accepts a session directory or individual wire.bin:

```sh
cargo run --release --locked -- --inspect-waveline /path/to/session
```

The CLI returns record counts, unrecognized/trailing byte counts, latest raw fields, and raw signal slices as JSON. It does not open the workspace for writing. Existing captures are not rewritten. `validation/waveline-decoder-findings.json` holds source hashes and independent Python statistics; `validation/waveline-candidate-fields.csv` contains offset-named raw values.

Generated-fixture and real-loopback tests validate framing, byte retention, raw decoding, and separation from verified measurements. They do not validate the hypothesized medical meanings. Next verification requires the vendor protocol or controlled, synchronized captures with known monitor-screen readings and documented sensor conditions.

## Waveform-inclusive exports and simplified labels

New CSV headers and display labels omit the `candidate` suffix; internal JSON
compatibility and `mapping_verified=false` are retained. This label change does
not establish validity rules or waveform calibration.

`waveline_live.csv` now appends the four complete waveform arrays to each new
record row. `waveline_readings.csv` and `waveline_waveforms.csv` reconstruct
supported scalar and waveform values from the original wire data, preserving
all sample bytes, order, connection offsets and receipt timestamps. No averaging
is used. Typical blocks hold 72 ECG, 60 pleth, 128 RESP and 128 P1 values.
Receipt-derived values-per-second estimates are distinct from the unknown
hardware sampling rates and are blank for first records, batches and gaps.
See [CSV schema and usage](USAGE.md#complete-waveform-csvs).

Existing exports and live files are preserved; later standard exports use a new
subdirectory if an export directory exists. The standalone `--export-waveline`
command requires a new output directory. Missing sample acquisition timestamps
and calibrated units are not supplied through guesses.
