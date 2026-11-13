# Using Pig Pal

## Waveline Pro over Ethernet

The observed Waveline Pro A.3.02 unit initiates a TCP connection to Pig Pal and sends 1,956-byte binary records. Configure the monitor to send to your computer, then select **TCP listener** in **Settings → Connection**. For the binary stream, leave generic HL7 decoding and application ACKs off.

The bench configuration was:

| Setting | Value |
| --- | --- |
| Computer / receiver | `10.0.0.2:12345` |
| Allowed monitor sender | `10.0.0.3` |
| Generic HL7 / application ACKs | Off |

Use addresses appropriate to your Ethernet network. Pig Pal does not configure the monitor or scan for it. The monitor's source port may change between connections; it is distinct from the computer's listening port. Received bytes, recognized binary records, and decoded measurements are separate states.

User-supplied identifiers: software **A.3.02**, kernel **K1-LPC3250-V1.0**, module **M1-ERTIC-V1.0**, module type **5A**, module version **V2.0.0**.

See [WAVELINE_DECODER.md](WAVELINE_DECODER.md) for the supported format, numeric candidates, P1 reference evidence, and missing-data limitations. Photos now support raw ECG, pleth, and P1 waveform candidates; calibrated sample timing, gas channels, and P2 remain unresolved.

## Session workflow

1. **New session:** enter a subject/research ID and title. Start with **Synthetic demo**. Confirm authorization and start recording.
2. **Monitor input:** in Settings choose TCP connect, TCP listener, UDP listener, serial, or raw-only decoding. Enter the interface settings confirmed for your monitor. Nothing scans the network or configures the monitor.
3. **Notes:** quick markers timestamp immediately. **Add note…** records the opening marker immediately; saving appends a linked annotation with its own later timestamp.
4. **Microphone:** refresh devices in Settings and explicitly select the intended microphone. Start/pause independently. Each continuous interval is split into lossless mono Float32 WAV clips of at most 30 seconds. Pauses and detected input gaps stay separate.
5. **Stop & save:** review the session, reopen original audio/media with the system player, generate transcripts, or export a ZIP. Resume creates a new segment; the microphone starts off again.
6. **Redux:** download once in Settings. The installation is shared by every session in this workspace. **Transcribe saved audio** automatically loads the local model; there is no separate load button. Completed transcripts are retained and skipped on later runs. Changing the data directory creates a separate workspace, including its model installation.

## Raw capture, live CSV, and monitor view

For the observed A.3.02 binary TCP stream, leave generic HL7 decoding off. Matching records are inspected automatically during capture. The app exposes unverified candidate fields and raw byte-signal slices separately from normal measurements. For an existing session, choose **Inspect binary capture**. The saved view is a snapshot, and original files are not modified.

```sh
cargo run --release --locked -- --inspect-waveline /path/to/session-directory
cargo run --release --locked -- --inspect-waveline /path/to/wire.bin
```

This inspector recognizes the observed 1,956-byte format only. Field meanings, units, validity flags, unavailable values, checksums, and waveform sampling rates remain unverified. See [WAVELINE_DECODER.md](WAVELINE_DECODER.md) for offsets, evidence, and limits.

After stopping a session, **Export data only** and **Export with all media** include `waveline_candidates.csv`, with every saved candidate field, receipt timestamp, connection/field offsets, raw bytes, and mapping status. Original `wire.bin` and `journal.ndjson` files remain in the ZIP. Binary candidates are separate from `observations.csv`. Photo comparisons support HR, cuff SYS/DIA, SpO2/PR, T1 Celsius, and P1 fields on this unit. The decoder notes distinguish these matches from unresolved validity rules and calibration.

During raw Waveline recording, `segments/<UUID>/waveline_live.csv` appends and flushes one row per complete record. Use **Copy live CSV path** in the monitor area, then `tail -f '/copied/path/waveline_live.csv'` to watch it. UTC receipt timestamps retain nine fractional digits; elapsed time uses a monotonic clock. New captures timestamp at transport-read completion, before processing delays. Device acquisition timing is unverified. The exported `timestamp_basis` identifies older records whose timestamps were taken during journal processing. See the decoder notes for fragmentation, precision, and clock-accuracy limits.

New captures decode separate P1 SYS/DIA/MAP candidates from signed i32 LE bytes 36/40/44, supported by a connected display reference. These candidates appear in the arterial panel and live CSV; photo references support mmHg on this unit, while channel-validity flags remain unverified. Legacy captures without persisted arterial decoding remain blank/unmapped in normal exports. Saved exports also include `arterial_pressure.csv`. After more than three seconds without a complete record, the live CSV writes blank `missing` rows with `stream_status=stale` at most once per second. These gap checks have their own host timestamps; they are not received monitor records. Constant values are not automatically treated as stale, and zero is not silently converted to missing.

### Waveline monitor comparison

Enable **Waveline monitor view** in an active raw-stream session to show HR, the separate byte-20 respiration candidate, cuff SYS/DIA/MAP, SpO2, PR, temperatures, and P1 SYS/DIA/MAP in the Waveline-style layout. It uses the same decoded record as the CSV. T1 displays Celsius and converted Fahrenheit to one decimal place while CSV retains the original Celsius candidate at full decoded precision. Byte 20 is not placed in the gas-RR slot: photo gas rates differ from that field. Gas RR stays unmapped. Receipt UTC is taken directly from that record's transport timestamp. Turning the view off returns to decoder details without changing capture.

After more than three seconds without a complete record, the view marks the stream stale and hides its numeric readings and waveform blocks. Missing values use dashes; P2 and gas values stay blank. ECG, pleth, and P1 show only the latest raw unsigned-byte block, labeled as candidates with unverified timing. They have no clinical vertical scale or calibrated time axis, and the second ECG cascade is not reconstructed. Device validity flags remain unverified, so a fresh record is not proof of fresh or valid sensor measurements.

For a saved session, choose **Inspect binary capture**, then enable the same toggle. This shows the last decoded record and explicitly says **SAVED SNAPSHOT - NOT LIVE**. It is not playback. UI validation used generated localhost packets in an isolated workspace, including changed readings, negative P1, missing temperature, a stream gap, and recovery.

### Replay a historical session

Select a saved session in the library and click **Replay Waveline**. Replay decodes that session's original `wire.bin` using the current candidate decoder, without rewriting journals or exports. It supports older captures that have wire events but no persisted decoder records.

- Enter **HH:MM:SS** in the **Time** field. A complete valid entry immediately updates the paused monitor; **Enter** or **Go** also applies it. The date and computer's local time zone appear beside the field. Full local dates (`YYYY-MM-DD HH:MM:SS`) and ISO timestamps with an explicit offset are also accepted.
- **Previous record** and **Next record** pause and visit individual records, including records sharing a receipt timestamp. The slider seeks across the full saved session. Replay controls stay visible while scrolling; **Display & details → Fit whole monitor** shows the whole monitor, and turning it off gives a larger scrollable view.
- **Play 1×** follows recorded receipt-time intervals; **Pause** freezes the cursor. Gaps between records/segments remain in the timeline. After three seconds without a recorded complete frame, readings and waveforms clear until recorded data resumes. Numeric readings use only the latest record at or before the cursor. Waveform display can read ahead one regular saved block, clipped at the cursor, to prevent a blank strip between chunks.
- The clock is the replay cursor. **Record received** shows the actual selected receipt timestamp; a seek displays the latest record at or before the cursor. All historical views say **REPLAY**, never live data. Candidate waveforms remain separate raw blocks with unverified sample timing/calibration.
- **Session details** exits replay. Selecting another library session clears the previous replay. Replay itself performs no recording, transport connection, export, or original-file writes.

For the October 20, 2026 photo references, use **pig 6 / sutff**, with Eastern local time:

| Enter | Expected candidates |
| --- | --- |
| `10:38:42` | HR 88, cuff 80/52, SpO2 95, PR 61, T1 37.4 C / 99.3 F, P1 178/178/178 |
| `11:57:32` | HR 110, cuff 148/115, T1 37.0 C / 98.6 F, P1 127/87/100; raw SpO2/PR zeros |

The 08:52:51 photo belongs to the earlier **MONITOR CONNECTION TEST / Waveline Ethernet test** recording. Its journal timestamps were delayed and batched: exact `08:52:51` shows a gap, while stepping around `08:52:47` and `08:53:02` exposes the adjacent recorded blocks. Those legacy times are labeled `legacy_journal_processing`, and cannot establish exact photo synchronization.

To open replay directly or inspect it without a GUI:

```sh
cargo run --release --locked -- --replay SESSION_ID --replay-at '2026-10-20T11:57:32-04:00'
cargo run --release --locked -- --inspect-replay /path/to/session --replay-at '2026-10-20T11:57:32-04:00'
```

Use `--data-dir` to open a separate workspace containing session copies. The headless inspector reports the selected raw record, timestamp basis, and gap state without opening the workspace for writing.

## Storage and recovery

Use **Open data folder** for the exact location. Default paths come from the OS application-data convention (macOS Application Support, Windows LocalAppData, Linux XDG data). `--data-dir /absolute/path` selects a separate workspace.

```text
workspace/
  settings.json
  models/parakeet-redux/ or redux-gguf/
  sessions/<UUID>/
    session.json
    segments/<UUID>/
      segment.json
      journal.ndjson
      wire.bin
      audio/*.wav + *.wav.json
      video/<UUID>/*.mkv + capture.json + capture.log
      photos/* + sidecars
      transcripts/*.transcript.json
    exports/*.csv + READ_ME.txt
  exports/*.zip
```

Schema 2 is a desktop format, not an automatic import of the iPad schema. A workspace lock prevents concurrent writers. An uncleanly stopped session is marked interrupted on reopening. Earlier segments, original wire bytes, and incomplete journal tails remain intact. Complete malformed journal rows cause a visible error; incomplete final rows are reported and omitted from derived exports.

Receipt UTC and per-segment monotonic elapsed time are retained. Audio timing is estimated from callbacks; video anchors use process start. These are not calibrated hardware acquisition timestamps. Live graphs plot received points without filling gaps. Repeated displayed/cuff values do not prove repeated measurements. Keep the laptop awake and powered during capture. Sleep, unplugging, disk errors, and device changes need bench testing with your hardware.

The app checks a 256 MB free-space reserve during capture. Plan adequate storage for the recording and an additional full copy for all-media exports. ZIPs and local files are not encrypted by the app. Use OS disk encryption/access controls and approved storage as appropriate. This is a research recorder, not a replacement for the primary monitor or its alarms.

## Audio recording and transcription

In an active session, choose a microphone under **Session settings → Audio**, then press **Start microphone**. The green audio strip shows the last eight seconds of real input amplitude, with a current loudness reading. Silence stays flat; orange bars mean the input is clipping. **Pause microphone** stops the input and finalizes the clip. Transcripts are still generated from saved audio after the session ends. The Redux download remains shared across all sessions.

Choose **Transcribe saved audio** after stopping a session. Model setup and platform-specific engines are described in [BUILDING.md](BUILDING.md#local-speech-engines). The original audio and raw transcription result are retained; model-generated words and timestamps are unverified.

## Optional video and images

Install FFmpeg separately and make it available on PATH. Enter the confirmed video input in Settings, then explicitly start video during a session. Audio from that input is disabled; the microphone is independent. FFmpeg saves H.264 in 30-second MKV segments with a capture log and timestamp sidecar. There is no built-in video preview in this desktop version.

List video inputs outside capture:

```sh
# macOS (the listing exits with a nonzero status)
ffmpeg -f avfoundation -list_devices true -i ""
# Windows
ffmpeg -list_devices true -f dshow -i dummy
# Linux
ls /dev/video*
```

Typical selected input strings: macOS `0:none`, Windows `video=USB Video`, Linux `/dev/video0`. These examples are not a verified mapping to a monitor. Existing JPEG/PNG images can be attached during a session; their original bytes are preserved, and their timestamp is attachment time.

## Other input modes

Pig Pal also supports explicitly configured TCP connect, UDP listener, serial, and generic HL7 v2 over MLLP. These are separate from the observed Waveline binary stream. Serial uses the selected baud rate with 8N1 and no flow control.

Generic decoding preserves all received OBX values, repetitions, raw identifiers, units, and device timestamp strings. It never guesses that a vendor code means HR or BP. A connected socket, received bytes, and decoded observations are separate states. Optional original-mode ORU ACK is off by default and occurs only after wire and journal synchronization. No ACK is sent for enhanced negotiation or for incoming ACK messages. UDP datagrams are independently framed; cross-datagram reconstruction is not attempted.

## Earlier project artifacts

Pig Pal is the Rust desktop project. The original iPad project remains separate. The retired OCR/Monitor Lab application's code and dependencies were removed; existing runs, exports, reference images, and validation artifacts are retained locally and ignored by Git.

### Photo-matched monitor layout

The comparison view uses the supplied monitor photos as a 1024×768 logical reference, cropped to the 1024×690 measurement area to omit the device header and inactive toolbar. This is a layout coordinate system, not a verified native LCD resolution. The view scales uniformly with the window and Retina display density. Replay fills the available window with a compact toolbar and timeline; the session sidebar starts collapsed and **Sessions** restores it. **Display & details** contains trace window, ECG zoom, fit mode and receipt diagnostics. **Fit whole monitor** fits the full view beneath replay controls; uncheck it for a larger, scrollable view. The monitor scales beyond its logical reference size on larger windows.

Channel colors, six trace rows, the right-hand numeric column, Fahrenheit T1, gas placeholders, and the P1 SYS/DIA/MAP arrangement follow the October 20 photos. The device header and inactive toolbar are omitted; use Pig Pal's playback controls above the monitor. Alarm state, cuff mode/countdown, lead/gain settings, and unknown channels are not invented from the photos. The NIBP side value is explicitly labeled raw MAP; it does not claim to decode the photo's transient cuff-pressure value. T2 stays unmapped in the Fahrenheit display; its original raw candidate remains in the numeric summary.

Replay offers 4/8/12-second receipt-history windows (default 8 s). Each block is drawn separately over its preceding receipt interval only when that interval is 0.5–1.5 s within one connection and segment. The next saved block is buffered when its interval is regular and belongs to the same connection and segment; only its portion at or before the cursor is visible. This fills the scrolling edge between normal chunks without advancing numeric readings. Blocks with batched timestamps or longer gaps are not placed on an invented time axis. This is an estimated within-block display, not a verified acquisition clock or the monitor's 25/12.5 mm/s sweep. The ECG visual zoom (default 2×, adjustable 1/2/4×) enlarges deviations around raw 128; it is not electrical gain in mV. Other traces retain their fixed raw 0–255 axis. Zooming clips at the plot edge and never changes captured samples. The live/snapshot view still shows the latest individual block.

At the photo-supported SpO₂ signature (state byte 349 = 2, SpO₂ = 0, PR = 0), the large readings show dashes and “Sensor off (candidate)”; raw zeros remain in the summary, journals and exports. Zero alone does not trigger that interpretation. Raw cuff MAP zero displays as dashes, with an explicit note; it is not replaced by the photo's 43. P1 MAP 100 remains independently decoded. ECG cascade and gas traces remain unmapped. All raw numeric values and export precision are unchanged.

The UI remains native Rust/egui. No data migration is required; the existing workspace compatibility and original exports are preserved. Replay demonstrations use the separate copied workspace.

## Complete waveform CSVs

New live CSVs have 28 columns: the previous 24 receipt/vital/status columns, now
without `_candidate` suffixes, followed by `ecg_raw_samples_json`,
`pleth_raw_samples_json`, `respiration_raw_samples_json`, and `p1_raw_samples_json`.
Each is a quoted CSV cell containing an ordered JSON array of unsigned raw bytes.
A missing waveform or stream-gap row has an empty cell. A real all-zero or flat
block stays an array, not a missing value. Older live CSVs are never rewritten.
`arterial_status=received` describes decoded bytes being present, not verified
channel validity. `mapping_verified=false` remains explicit.

Saved exports include two new tables reconstructed from `wire.bin` and its
receipt journal, including old captures that did not persist waveform decoding:

- `waveline_readings.csv`: one row per complete frame. It includes all supported
  scalar fields, all four sample arrays, segment/connection/offset provenance,
  raw channel state bytes, and receipt-based rate estimates. `record_id` is a
  one-based identifier within this export. Temperature 1 is stored in Celsius.
- `waveline_waveforms.csv`: one row per raw sample, with `record_id`, whole-block
  `received_utc`, channel, base byte offset, zero-based `sample_index`, block
  `sample_count`, `raw_value`, and `units=raw_u8`. Join `record_id` to the readings
  table for connection, state and timing context.

The older long-form field table remains named `waveline_candidates.csv` for
compatibility; new exports call its numeric column `numeric`. Existing exports
remain unchanged. Re-exporting writes a new `exports/export-<UUID>/` directory
when an export folder already exists.

A frame carries one HR/pressure/etc. number but multiple waveform points: ECG 72,
pleth 60, respiration 128, P1 128. At about one frame per second, the counts suggest
approximately 72/60/128/128 values per second, respectively. These are **not
confirmed hardware rates**; monitor decimation, buffering and repeated blocks
remain possible. The `*_samples_per_receipt_second_estimate` columns divide
sample count by the preceding receipt interval only when it is 0.5–1.5 seconds,
within the same segment and connection, with no batched timestamp at either end.
Otherwise the rate cell is empty. `sample_clock_verified=false` is always retained.
Every raw sample is exported, including duplicated, clipped or disconnected blocks.

All rows belonging to one block repeat the timestamp of the transport read
completing that frame (or labeled legacy journal time), preserving nine fractional
digits. This does not mean every sample was acquired at that instant. No exact
per-sample timestamp, ECG mV value, or calibrated pressure waveform is fabricated.
Gas/CO2 remains unmapped; the respiration block is not substituted for capnography.

To export independently without replacing existing tables, run:

```sh
cargo run --release --locked -- --export-waveline /path/to/session --output-dir /path/to/new-directory
```

The command requires a new directory and reads the session without opening it for
writing. It writes the two tables and `waveline_csv_README.txt` there. In Python,
`json.loads(row["ecg_raw_samples_json"])` recovers the ordered array from a row
read using `csv.DictReader`.
