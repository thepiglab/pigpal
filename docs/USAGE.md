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

See [WAVELINE_DECODER.md](WAVELINE_DECODER.md) for the supported format, numeric candidates, P1 reference evidence, and missing-data limitations. Waveforms and P2 remain unmapped.

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

After stopping a session, **Export data only** and **Export with all media** include `waveline_candidates.csv`, with every saved candidate field, receipt timestamp, connection/field offsets, raw bytes, and mapping status. Original `wire.bin` and `journal.ndjson` files remain in the ZIP. Binary candidates are separate from `observations.csv`. Limited user-reported screen agreement for HR 91 and diastolic 77 is recorded in the decoder notes; other field identities and validity rules still need verification.

During raw Waveline recording, `segments/<UUID>/waveline_live.csv` appends and flushes one row per complete record. Use **Copy live CSV path** in the monitor area, then `tail -f '/copied/path/waveline_live.csv'` to watch it. UTC receipt timestamps retain nine fractional digits; elapsed time uses a monotonic clock. New captures timestamp at transport-read completion, before processing delays. Device acquisition timing is unverified. The exported `timestamp_basis` identifies older records whose timestamps were taken during journal processing. See the decoder notes for fragmentation, precision, and clock-accuracy limits.

New captures decode separate P1 SYS/DIA/MAP candidates from signed i32 LE bytes 36/40/44, supported by a connected display reference. These candidates appear in the arterial panel and live CSV; units and channel-validity flags remain unverified. Legacy captures without persisted arterial decoding remain blank/unmapped in normal exports. Saved exports also include `arterial_pressure.csv`. After more than three seconds without a complete record, the live CSV writes blank `missing` rows with `stream_status=stale` at most once per second. These gap checks have their own host timestamps; they are not received monitor records. Constant values are not automatically treated as stale, and zero is not silently converted to missing.

### Waveline monitor comparison

Enable **Waveline monitor view** in an active raw-stream session to show HR, RR, cuff SYS/DIA/MAP, SpO2, PR, temperatures, and P1 SYS/DIA/MAP in the Waveline-style layout. It uses the same decoded record as the CSV. Temperatures display one decimal place while CSV retains full decoded precision. Receipt UTC is taken directly from that record's transport timestamp. Turning the view off returns to decoder details without changing capture.

After more than three seconds without a complete record, the view marks the stream stale and hides its numeric readings. Missing values use dashes; unmapped waveforms, P2, and gas values stay blank. Device validity flags remain unverified, so a fresh record is not proof of fresh or valid sensor measurements.

For a saved session, choose **Inspect binary capture**, then enable the same toggle. This shows the last decoded record and explicitly says **SAVED SNAPSHOT - NOT LIVE**. It is not playback. UI validation used generated localhost packets in an isolated workspace, including changed readings, negative P1, missing temperature, a stream gap, and recovery.

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
