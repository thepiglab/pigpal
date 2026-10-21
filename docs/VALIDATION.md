# Desktop validation — October 20, 2026

The separate Rust desktop project builds in release mode, has been packaged as a locally signed macOS app, and is open on the validation Mac with capture stopped. The iPad project and original archive are preserved.

## Verified on this Mac

| Check | Result / evidence |
|---|---|
| Rust acceptance suite | 22 tests passed: eight audio/waveform/binary-decoder unit tests and fourteen core/integration tests. `validation/waveline-decoder-tests.log`. |
| Rust static analysis | `cargo clippy --all-targets --locked -- -D warnings` passed. `validation/waveline-decoder-clippy.log`. |
| Real local TCP | Fragmented MLLP input, exact wire bytes, four decoded observations, original-mode ORU ACK only after synchronized storage. |
| Real local UDP | Datagram reception and generic MLLP decoding passed. No real monitor was contacted. |
| Parser | Every split boundary, multiple frames, oversized/corrupt frame recovery, repetitions, strict numerical parsing, patient/order boundaries, charset rejection and conservative ACK negotiation. |
| Persistence | Exclusive workspace lock, interrupted recovery, note-opening timestamp versus later annotation, and resume without changing earlier segment bytes. |
| Audio writer | Exact Float32 sample equality across 30-second chunk boundaries, exact sample counts, and a deliberate gap retained as separate clip anchors. Generated samples only. |
| Export | ZIP entries independently compared with source bytes, valid CSV escaping, active-export rejection, incomplete journal tail retained and reported. |
| Video | One explicitly run optional FFmpeg test passed: generated video finalized as H.264 MKV, decoded successfully, and more than ten frames counted. `validation/video-acceptance.log`. |
| Native UI | Started synthetic session, received live values, added marker and note, stopped and reviewed saved note, exported ZIP, resumed into a second segment, verified microphone remained off, stopped again. |
| Portable Redux | Pinned CPU engine built; 213,319,296-byte model downloaded and SHA-256 verified; a public WAV fixture transcribed locally with matching reference text. `validation/portable-inference.log`. |
| Mac Redux | FluidAudio 0.17.7 Core ML with CPU + Metal GPU transcribed the same 7.435-second fixture with matching text. The final end-to-end inference took about 2.44 seconds including helper launch/model loading. `validation/end-to-end.json`. |
| Model installation | Shared model installed in the desktop application's data folder. Preparation of the existing download completed in about 1.78 seconds. No per-session download or separate manual load step. |
| Original audio | SHA-256 values before and after Mac transcription matched. A second transcription pass saved zero new transcripts, preserving existing results. |
| App bundle | Release binary and speech helpers packaged; `codesign --verify --deep --strict` passed. Local ad-hoc signature; not notarized for distribution. |
| iPad preservation | All 59 files in the source/configuration/documentation baseline have unchanged SHA-256 values. `validation/ipad-preservation.json`. Original `SurgeryBuddy.zip` is retained. |

The public fixture transcript is: “Well, I don't wish to see it any more, observed Phoebe, turning away her eyes. It is certainly very like the old portrait.” This is a pipeline check on one known clip, not an accuracy benchmark for medical speech.

## Mac engine decision

The initial CPU + Neural Engine load waited inside Apple's ANE compiler. Process inspection showed an already-running ANECompilerService consuming CPU with more than thirteen hours of elapsed time. Only this task's superseded helper processes were stopped; the unrelated system service was left alone.

The Core ML CPU + Metal GPU configuration completed correctly and is the default on Apple Silicon. `SURGERYBUDDY_COREML_COMPUTE=ane` opts into a deliberate Neural Engine comparison. Intel Macs select Core ML CPU. The download/preparation path uses the same selected compute configuration, avoiding an implicit ANE preparation step. Local inference uses `AsrModels.loadLocal`, not a download-capable loader. Rust bounds helper execution to three minutes per clip and twenty minutes for initial preparation.

For this fixture, Core ML's final punctuation token ended at 7.440 seconds, 5 ms beyond the 7.435-second source clip. The app flags the timing bounds and retains the exact raw result; it does not silently alter the audio or model offsets. Text matched the reference. The portable engine's timestamps were within the clip. Hardware/audio alignment and ASR accuracy require separate validation.

## Remaining checks and scope

- Windows and Linux build/test/package CI is supplied but has not run. Source uses cross-platform Rust APIs and the portable CPU adapter; this is not a claim of tested Windows/Linux binaries.
- No camera, UVC capture card, or serial cable was activated. The physical Waveline Ethernet connection was separately tested as described below. Automated audio tests use generated samples and a public WAV fixture; the separately authorized laptop microphone check is recorded below. Video used FFmpeg's generated test pattern.
- Waveline A.3.02 binary Ethernet streaming is confirmed on this unit. Physiological field meanings, HL7 activation, and serial baud/framing remain unverified. The app stores the user-supplied hardware identifiers for reference and implements generic reception, not a vendor driver.
- Full-duration capture, sleep/wake, cable loss, storage exhaustion, thermal behavior, and institutional workflow acceptance remain hardware tests.
- The simplified desktop build has optional FFmpeg recording and original-image attachment. It does not yet port the iPad's live video preview, optical OCR/calibration, direct still-camera UI, or automatic iPad data import.
- The app is a local research recorder. It provides no clinical alarms or automatic interpretation of vendor-local measurement codes.

## Evidence files

`validation/desktop-recording.png` and `desktop-review.png` show the native synthetic UI workflow. `desktop-final.png` shows the packaged app with the saved local transcript. Test logs, speech results, preservation hashes, package/signature checks, and `end-to-end.json` are retained under `validation/`. Early failed attempts are also retained and are not counted as passing validation.

## Interface and microphone verification — October 20

The interface now uses bundled Inter typography, a light workspace, a compact session library, clearer saved-transcript review, tabbed settings, and a prominent live audio strip. The eight-second waveform displays actual recorded-channel peak levels in 25-ms windows with a current RMS dBFS readout. Display queues and history are bounded and do not block the recording writer.

A slow input initialization previously shifted the live envelope outside its visible time window. Anchoring the clock before device setup fixes this. Mac device discovery now reads CoreAudio metadata without opening each virtual device. A separate CPAL 0.15.3 disconnect-listener reference cycle prevented stream destruction and could hang Pause; the vendored backport uses a weak reference, as upstream 0.16.0 does. The application explicitly pauses, drops the stream, drains, and finalizes the writer.

The final authorized native UI check used **MacBook Pro Microphone**, showed a visible scrolling waveform and changing dBFS level, and paused normally. It saved one finalized **4.821333-second**, 48 kHz mono Float32 clip containing **231,424 frames**. The session was then stopped and saved. WAV header, payload sample count, closed metadata, pause/save journal events, and unchanged file size after stopping were checked. Evidence: `validation/redesign-waveform-live.png` and `validation/redesign-microphone.json`. Microphone capture is off.

Earlier failed attempts are retained in the same LOCAL AUDIO TEST session as interrupted segments. One pre-fix attempt captured approximately 14.5 seconds because Pause hung, exceeding the intended ten-second test; subsequent checks used a process watchdog. Another retained interrupted clip contains approximately 8.5 seconds. These attempts are not counted as passing checks, and their partial files were not rewritten. The final check stopped normally before the watchdog deadline. Input initialization also stalled during earlier attempts; the final post-wake start completed promptly. Selected-input startup is still synchronous, so behavior with unavailable or unresponsive drivers and sleep/wake needs further hardware testing.

New generated-audio tests cover peak/RMS levels, callback chunking independence, gaps, preservation of original samples when the display queue is full, bounded history, silence/clipping, and stale-input status. No new room audio is captured by the automated suite. The final screenshot shows generated monitor values alongside actual microphone audio; it is not a connected-monitor test.

## Physical Waveline Ethernet — October 20

The USB Ethernet adapter `en7` negotiated 100baseTX full duplex. User-supplied ARP capture identified the monitor as `10.0.0.3`, MAC `1e:30:6c:a2:45:03`, requesting `10.0.0.2`. The Mac Ethernet service was changed from DHCP to manual `10.0.0.2/24` with no gateway (`0.0.0.0`); Wi-Fi was unchanged. All three ping requests received replies. A subsequent user-supplied packet-header capture showed repeated monitor TCP SYNs to `10.0.0.2:12345`.

Surgery Buddy was configured as TCP listener at `10.0.0.2:12345`, allowed sender `10.0.0.3`, generic HL7 decoding off, and application ACKs off. The monitor initiated a connection, and `lsof` independently confirmed the app owned both the listener and established socket. The monitor's source port, initially `1203`, is separate from the Mac listener port and may change on reconnection. No application commands were sent to the monitor. Microphone and video remained off.

Session `149e67e4-fb27-400c-bcfb-5510dc1da60e`, **Waveline Ethernet test**, retains the original wire bytes and receipt journal. A snapshot contained 375,552 bytes fitting 192 consecutive 1,956-byte candidate records with prefix `01 02 5b f6` and matching little-endian length at bytes 4–7. Median receipt interval was about 0.96 seconds. These are structural observations from this capture, not a validated physiological decoder. No `MSH|` headers were present. The monitor reportedly displayed live values, but exact reference readings were not supplied. Field labels, units, missing-value sentinels, waveform encodings, and any integrity checks remain unknown.

The UI now separates received bytes from decoded messages, displays the listener address and last-data age, and explicitly reports raw data saving when interpretation is disabled. An additional real-loopback raw-binary test verifies exact stored bytes, receipt tracking, and absence of fabricated observations. Evidence: `validation/waveline-ethernet-status.json`, `waveline-stream-analysis.json`, `waveline-connected.png`, and the updated build/test logs. A verified vendor protocol or controlled reference captures are needed before displaying interpreted binary-stream vital signs.

## Experimental binary inspection — October 20

A structural decoder was implemented against the saved 740 complete records (217 in the first segment and 523 in the second). It recognizes the observed four-byte signature and fixed little-endian length, reassembles arbitrary TCP fragmentation/coalescing, bounds pending memory to one record, and resynchronizes after unrecognized bytes or invalid lengths. No application command or handshake was added.

Nine raw numeric fields and three unsigned-byte slices are exposed as **experimental hypotheses**. Candidate pressure triplets include 103/96/98, 110/90/97, and 158/133/141. The candidate heart/pulse fields differ by at most five in about 97.35% of the 453 records where both are nonzero; median absolute difference is one. The candidate saturation and pulse fields become zero together in all 740 records. This is internal consistency evidence, not verification against the device display. Last captured candidate values include rate 108, saturation 100, pulse 108, pressure 110/90 with third field zero, and a float of approximately 19.6. No units or unavailable-value semantics have been established.

The app shows these under Experimental binary decoder, outside normal measurement trends, observation exports, and marker context. Saved inspection explicitly says it is a snapshot. Live experimental snapshots show time since the last complete record and become marked stale after three seconds; raw capture also reports inactivity. Zeros, 65535, and other potential sentinels remain raw numbers; non-finite floats retain their original bits and have no numeric interpretation.

New tests cover all 1,957 split boundaries of a generated record; multiple records, garbage, corrupt/oversized lengths, truncated tails, and bounded state; unnormalized sentinels/non-finite values; and fragmented real-loopback binary capture with exact original byte retention and empty clinical observation/history/marker context. The raw-data inspector also has a command-line entry point that reads files without opening the workspace for writing. Tests use generated fixtures, not patient captures.

The Ethernet adapter was absent during this work. The previously open session had stopped receiving data at 09:02:46 EDT and was finalized before updating the app. The captured files were kept byte-for-byte; no new live hardware validation of candidate field meanings is claimed. See `docs/WAVELINE_DECODER.md`, `validation/waveline-decoder-findings.json`, and `waveline-candidate-fields.csv` for the reproducible hypotheses and source hashes. A protocol specification or controlled display-reference captures remain necessary to promote any fields to verified measurements.

## Research capture, live CSV and timestamp verification — October 20

The user reported displayed HR **91** and diastolic **77** during segment `acc77f59-b5cc-4f43-9f2e-00cc01220628`, then confirmed the fields looked correct. That segment retained 71 complete records; offsets 14 and 30 agree with these limited references. Exact screen observation times, other fields, units, validity codes, and waveform calibration remain unverified.

Two further live Ethernet tests used the saved TCP-listener profile at `10.0.0.2:12345`, allowing only `10.0.0.3`, with generic HL7 and application ACKs off. Segment `b0a9467d-0705-4977-a21e-4942c9ef7547` received 124 complete records in about 124 seconds without a capture warning. After the live CSV/timestamp changes, segment `9d9f8312-65dd-4f97-9410-abebf4893f02` retained 48 complete records and 48 live CSV rows. `tail -f` observed appends before stop, and the UI exposed **Copy live CSV path**. Audio/video were not activated.

New receive timestamps are taken immediately after the transport read completes. This removes UI queue delay from exported receipt times. In the sampled live check, journal processing lag ranged from 0.791 to 147.272 ms; these processing timestamps are retained separately. UTC and monotonic receipt timestamps had no backwards steps, and adjacent UTC/monotonic interval differences were below 2 microseconds in that sample. UTC strings in the live CSV have nine fractional digits (the observed OS clock values were at microsecond granularity). A read-only `sntp -t 3 time.apple.com` check reported `+0.001841 +/- 0.010854` seconds; the clock was not changed. None of this establishes monitor acquisition timing or waveform sample timing.

The rebuilt app reopened the completed session and exported 2,187 candidate rows representing all 243 persisted decoded records. Every exported value, raw hex string, UTC timestamp (compared at nanosecond precision), elapsed float, and processing timestamp matched its journal source exactly. `serde_json/float_roundtrip` prevents loss of a floating-point ULP during rereading. All original wire files and the live CSV matched ZIP contents byte-for-byte. The first two pre-decoder segments contain 740 additional raw records, retained unchanged and inspectable as raw bytes; the exporter does not fabricate decoded historical events/times for them.

Validation: **24 tests passed**, one generated-video test remained explicitly ignored. The updated loopback test checks fragmented binary records, no application ACKs, reconnection, CSV separation, and original-byte ZIP retention. New regression tests check delayed processing, final-fragment timestamps, live readability before stopping, non-finite values, nanosecond UTC precision, and exact elapsed-float/legacy timestamp round trips. Formatting, strict Clippy, release packaging, and macOS ad-hoc signature verification passed. Logs: `validation/waveline-real-test-{tests,clippy,package}.log`. Reproducible export verification and local backup: `../capture-analysis/real-test-readiness/`.

## Arterial-pressure schema and missing-data handling — October 20

Added separate arterial SYS/DIA/MAP CSV fields, a saved `arterial_pressure.csv` export, and an optional arterial sample type that preserves finite signed/zero values and leaves unavailable/non-finite values empty. The raw Waveline decoder deliberately returns `arterial_pressure: null`: no ART/P1 or IBP1/IBP2 field mapping or validity rule has been established. These changes do **not** establish working invasive-pressure decoding. The existing pressure triplet remains separate. Connected reference captures or a vendor protocol are still required to complete that decoder.

The recorder now observes gaps in complete binary records. After more than three seconds, it appends at most one blank live CSV `missing` row per second and a `binary_gap` journal event. All candidate numeric fields are blank on these gap rows, so no old value is carried forward. Gap timestamps are explicitly `host_gap_check`. Resumed records return to `receiving`; repeated equal numbers do not trigger a gap. Channel-level staleness inside continuing frames remains unverified because channel validity/update flags are unknown.

The current suite passed **28 tests**, with one generated-video test explicitly ignored. Pressure regression tests cover unmapped/unavailable/stale blanks, a real three-second recorder gap, recovery, zero/negative values, non-finite values, and arterial CSV export. Formatting and strict Clippy passed. The staged release at `dist/arterial-update/Surgery Buddy.app` passed signature verification. Its read-only inspector re-read 668 records from the ongoing test session with zero discarded or trailing bytes and arterial pressure correctly unset. The real recording stayed active, and its existing CSV schema was not rewritten or replaced. Logs and inspection evidence: `validation/waveline-arterial-{tests,clippy,package}.log` and `validation/waveline-arterial-replay.json`.

New columns/gap rows apply to new recording segments, not to an already-running instance. No physical arterial transducer or simulator was activated for this change.

## Visible arterial status and live CSV deployment — October 20

ART/P1 now has a persistent section with separate systolic, diastolic, and mean placeholders and “Not reading yet.” It appears before the first record, in the live decoder, in saved inspection, and in the optional monitor view. Missing samples never reuse an earlier arterial value. The message explicitly states that the ART/P1 byte mapping still needs verification; connecting a transducer alone does not establish a decoder mapping.

The suite passed 29 tests, with one optional generated-video test ignored. The new headless UI regression verifies the labels, empty placeholders, supplied candidate samples, missing samples, and stale streams. Strict Clippy and release packaging passed. Evidence: `validation/waveline-arterial-ui-{tests,clippy,package}.log`.

After explicit approval to save and restart, the real `sutff` session's first segment was finalized at 14:53:06 UTC. The updated primary bundle was installed, signed, and verified, and the same session resumed as Segment 2 (`87ba41b9-1969-407d-9ce3-fac36aab7883`) at 14:54:07 UTC. Native UI verification confirmed a live connection to `10.0.0.3:3771` and the visible ART/P1 placeholders. The isolated preview was closed. Microphone remained paused.

An independent read of the growing CSV checked all 35 journaled complete records available at that snapshot: 24 columns on every row, all nine candidate values exactly matching both the original wire bytes and journal, receipt UTC preserved, elapsed timestamps preserved to the CSV's nine decimal places, and all three arterial cells empty with `arterial_status=unmapped`. All prior segment files retained their saved SHA-256 values. Evidence: `validation/arterial-ui-live-verification.json` and `validation/arterial-ui-saved-segment-hashes.json`.

The CSV contains the currently decoded numeric fields; `wire.bin` retains the original signal and other undecoded bytes. Blank cells represent unavailable/non-finite decoded values, unmapped arterial values, or complete-stream gaps. Raw candidate zero/sentinel values remain literal until channel validity flags can be identified; channel-level missingness within otherwise continuing records cannot yet be reliably inferred.

## Connected P1 reference and decoder deployment — October 20

The user subsequently activated P1 and reported approximately 73/33, MAP 44, with rapid changes. Incoming signed i32 LE fields at bytes 36/40/44 changed around that reference, while cuff fields 28/30/32 held at 76/60/65. Recorded examples include 73/33/46 and 68/33/44. Negative transition values (38/-24/-3) support signed decoding. The decoder now populates the existing arterial sample, UI, journal, live CSV, and saved arterial CSV. Units and device validity/update flags remain unverified; all values retain candidate status. Earlier inactive-looking equal triplets are not automatically suppressed as missing, since no sentinel or validity rule is established.

Thirty tests passed, with the optional generated-video test ignored. The TCP test verifies the connected reference triplet through fragmentation, reconnect, journal, live CSV, and saved arterial export. Signed-negative decoding, stale blanks, missing optional values, and the native arterial UI are also covered. Strict Clippy, release packaging, and installed-bundle signature verification passed. Logs: `validation/p1-connected-{tests,clippy,package}.log`.

Segment 4 was saved before installation. The same session resumed in Segment 5 (`3530760d-3d77-4124-8275-0bf49ffad808`) at 15:21:52 UTC. The native app showed live P1 values with “Receiving · candidate values.” Independent validation checked 25 real incoming records against raw signed integers, journal values, live CSV cells, and receipt timestamps. All 16 saved files from earlier segments retained their SHA-256 hashes. Evidence: `validation/p1-connected-live-verification.json` and `validation/p1-before-update-hashes.json`.

The complete pre-update Segment 4 was separately redecoded to `../capture-analysis/p1-active/p1-retrospective.csv`, with original receipt timestamps and explicit candidate/unverified provenance. The original CSV and journal remain unchanged. No P1 waveform or per-channel absence flag is claimed to be decoded.

## Finished Rust monitor toggle — October 20, evening

Completed the Waveline-style toggle for active raw capture and saved binary inspection. Saved views explicitly say “SAVED SNAPSHOT - NOT LIVE”; live views clear all displayed values after the existing three-second complete-record timeout and restore them on receipt. Receipt UTC now comes from a dedicated recorder field instead of searching the bounded event history. P1 appears in the accessible value summary, the redundant arterial panel above the replica was removed, and cuff MAP sits below SYS/DIA to avoid overlap. Large HR, SpO2, cuff, and P1 strings shrink to fit their available width without changing saved numbers.

All 30 automated tests passed, with one optional generated-video test ignored. Strict Clippy and release packaging passed. An isolated native app received 115 generated binary records over localhost, including a changed HR, negative P1, and non-finite T2. The native toggle, rendered monitor face, stale blanks, recovery, saved-snapshot label, and return to decoder details were checked through the UI. Every numeric CSV cell matched the source bytes; T2 was empty, and all numeric cells in 43 gap rows were empty. Gap rows include the initial wait for the synthetic sender. Original monitor input was unplugged; no hardware comparison is claimed for this validation.

The installed primary bundle passed signature verification and launched successfully with recording stopped. All 107 original session files were unchanged. The synthetic fixture resides only in `validation/monitor-view-ui/`; it was not imported into the real workspace. Evidence: `validation/monitor-view-{tests,clippy,package}.log`, `validation/monitor-view-verification.json`, and `validation/monitor-view-original-hashes.json`.


## Pig Pal repository migration — October 20, evening

Moved the complete Rust desktop project into `/Users/mkieffer/programming/uf/pig-lab/pigpal`, preserving the destination repository's existing `.git`. Monitor Lab and `capture-analysis/` are included in that root. The original iPad project remains at `../SurgeryBuddy/SurgeryBuddy/`. Previous app bundles and Rust/Swift build outputs are retained under ignored `validation/legacy-*` paths; current packages are built into `dist/`.

The Cargo package and executable are now `pigpal`, the macOS bundle is `Pig Pal.app` with identifier `org.research.pigpal.desktop`, and the native speech helper is `pigpal-speech-macos`. App labels, the monitor header, export documentation, HL7 sender identity, Monitor Lab branding, packaging, and CI artifact names use Pig Pal. New environment-variable names are `PIGPAL_SPEECH_ENGINE` and `PIGPAL_COREML_COMPUTE`; legacy names remain accepted.

Existing installations automatically retain the legacy application-data directory when no PigPal workspace exists. This keeps session IDs, CSV paths, settings, and installed models accessible without rewriting recordings. New installations use the PigPal data directory. A regression test covers fresh installs, legacy workspaces, and precedence when both exist. `--data-dir` continues to override the default.

The migrated Rust suite passed 31 tests, with the optional generated-video test ignored. Strict Clippy passed. The relocated Monitor Lab Python environment imports OpenCV, NumPy, pyserial, and initializes its OCR engine. The debug executable reports the correct existing data path. All 108 original recording/settings files retain their saved SHA-256 hashes. Build and migration evidence is in `validation/pigpal-*`.
