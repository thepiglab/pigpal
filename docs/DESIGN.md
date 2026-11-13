# Desktop interface — November 13, 2026 (second pass)

Rebuilt from a small component kit in `src/theme.rs` so every screen shares one grid and one baseline system. The Waveline monitor canvas (`src/waveline_monitor.rs`) is unchanged.

- **Kit:** vector icons, one `Btn` component (primary / secondary / ghost / danger; 28, 32 or 38 px), 34 px text fields and dropdowns of identical width, a checkbox, underline tabs with counts, menus, chips and toasts. Rows are drawn by hand with `paint_on_baseline`, so time, dot, title and actions in a row share one baseline.
- **Files:** `ui.rs` holds state, capture logic, the sidebar, dialogs and settings; `ui_views.rs` the pages; `ui_parts.rs` the drawn rows, tiles and decoder panels.
- **Sidebar:** brand; New recording (or a live Recording card); search; session rows with date on the title baseline and flags (Demo, Interrupted). The footer holds free space plus three icon buttons: appearance (menu), data folder, settings.
- **Notifications:** short-lived toasts replace the permanent status bar; a background job shows a toast with a spinner. Errors stay modal.
- **Saved session:** title, one meta line, primary Replay, Transcribe (only if audio exists) and a ⋯ menu (exports, Finder, binary inspection, resume). Underline tabs. Timeline rows use a note's own words as the heading.
- **Settings:** modal with a left nav (General, Monitor, Audio, Transcription, Video & photos). Cancel restores everything, including a previewed appearance.

# Desktop interface — November 13, 2026

Visual thesis: a dark, low-glare "monitor night" workspace that sits naturally next to the Waveline canvas. Graphite surfaces, Inter type, one periwinkle accent for actions, and semantic colours reserved for state: red = recording, green = live/receiving, amber = stale or needs attention. The monitor's own vital colours (green, cyan, yellow, red) stay exclusive to the monitor canvas. Tokens and widgets live in `src/theme.rs`.

Layout:

- **Sidebar**: brand, New session (or a live REC card while capturing), search, compact session rows (title, subject · date, demo/interrupted flags), Settings, data folder, free disk space.
- **New session**: one centered form with a plain-language summary of the configured input ("Listens on 10.0.0.2:12345 for 10.0.0.3 · Waveline binary decoding").
- **Live capture**: header with RECORDING chip, timer, and Stop & save; main column with a connection strip and the Waveline view / decoder fields (or HL7 vitals and trend); right rail with microphone, quick markers, Add note, and timeline.
- **Saved session**: title with date, duration, segment, and state tags; primary Replay Waveline; Transcribe; Export and More menus; tabs for Timeline, Transcript, Media, and Monitor data. Lifecycle rows (start, stop, connection) are hidden unless "Show recording events" is on.
- **Replay**: full-width monitor with one transport bar (Play/Pause, previous/next record, time readout, Jump to, Display menu, scrubber). Space plays or pauses; ← and → step one record.
- **Dialogs**: settings, note, resume, error, and close confirmation are modal with a dimmed backdrop. Cancel in Settings restores the previous values.

Behavioural fixes in this pass:

- The live and saved timelines previously kept only the last 300 or 1,000 journal events, so markers older than a few minutes were crowded out by wire and binary-record events. Recorder now keeps a separate timeline of every marker, note, media, and lifecycle event, and review loads timeline events for the whole session.
- Notes appear at the time their "Note opened" placeholder was captured, and that placeholder is hidden.
- Replay is offered only for sessions with Waveline binary input; Transcribe is enabled only when audio exists.

Performance:

- Rendering moved from OpenGL (glow) to wgpu (Metal on macOS, DX12/Vulkan elsewhere) with a low-power GPU preference.
- Reactive repaint: idle screens no longer repaint every 150 ms (0 % CPU at idle). Recording repaints every 100 ms, and every 33 ms while the microphone is live or replay is playing.
- Media file listing, free disk space, and the transcription-model check are cached instead of being recomputed every frame.
- Review loading skips bulk `wire`/`binary_record` journal lines before JSON parsing.
- Release builds use fat LTO with one codegen unit.

Capture reliability (data first):

- The recorder runs on a dedicated background thread (`BackgroundCapture` in `src/ui.rs`) that polls every 20 ms, writes `wire.bin`, the journal and the live CSV, and syncs to disk each second. The window only borrows the recorder while a frame is drawn, so a hidden, minimized or unfocused window never pauses capture. The audio stream lives on its own thread, which keeps the recorder `Send`. `capture_continues_to_disk_without_ui_frames` streams 600 Waveline records over TCP with no frames drawn and checks every byte reaches disk.
- While recording, Pig Pal holds a macOS user-initiated, latency-critical activity (`src/platform.rs`), so App Nap cannot throttle it and the Mac does not idle-sleep. The display may still sleep.
- An unfocused window redraws once a second; a hidden one is not drawn at all.

Appearance: System / Light / Dark in the sidebar footer, saved in `settings.json`. System follows macOS live. The Waveline canvas stays dark in every mode, like the device.

Notes in replay: markers and notes appear as ticks on the scrubber, as a banner naming the note at the current moment, and as a clickable list beside the monitor. Clicking any of them, using ⏴ Note / Note ⏵, or pressing `[` / `]` jumps to that moment and pauses there, so the monitor shows the vitals at that time. A note is placed at the time it was opened. Each note on the saved-session timeline has a ⏵ Replay button that opens the replay at that moment.

README animation (`docs/media/replay.gif`): `PIGPAL_GIF_DIR=<frames> PIGPAL_SNAPSHOT_DATA=<copy of a data dir> cargo test --release --lib replay_gif -- --ignored`, then encode the frames at 10 fps with an ffmpeg palette (`palettegen=max_colors=256`, `paletteuse=dither=bayer:bayer_scale=5`, scaled to 1000 px wide).

Headless screenshots of every screen: `PIGPAL_SNAPSHOT_DIR=<out> PIGPAL_SNAPSHOT_DATA=<copy of a data dir> cargo test --release --lib snapshots -- --ignored` (copies the data again into a temporary folder; originals are never opened for writing).

---

## Previous light design — October 20, 2026


Visual thesis: a quiet, light research workspace with warm neutral surfaces, precise Inter typography, one green accent, and a dark audio strip as the live visual anchor.

Content plan: compact persistent session library; session setup or saved review in the main workspace; recording state and actions first during capture, followed by live audio, measurements, and timestamped notes. Settings are divided into Connection, Audio, Transcription, and Media.

Interaction thesis: a smoothly scrolling eight-second waveform driven by actual microphone samples; restrained hover/selection feedback; progressive disclosure for trends, saved observations, and resuming a session. No simulated audio activity is displayed.

## Interface review

| Before | After |
|---|---|
| Default dark egui surfaces and Ubuntu type | Light neutral surfaces, bundled Inter regular/semibold, dark ink text, green primary actions. |
| Oversized gray buttons and edge-to-edge prose | Consistent 40-pixel controls, 32-pixel workspace gutters, deliberate type scale and rounded input surfaces. |
| Large uppercase brand and loosely stacked library entries | Compact waveform mark, clear new-session action, contained selected-session row and quiet metadata. |
| All session inputs in one long column | Aligned subject/title fields and a clear input/authorization/start sequence. |
| Microphone only indicated by an ON/OFF label | Prominent audio strip with scrolling peak envelope, current RMS dBFS, quiet/waiting/clipping states, exact input selection and pause/start control. |
| Display had no independent audio telemetry | Fixed 25-ms envelope windows from the recorded channel, bounded nonblocking display queue, eight seconds of bounded UI history; full display queue does not affect disk recording. |
| Static 150-ms repaint cadence | 33-ms refresh while microphone is active; no decorative animation or automatic noise normalization. |
| Spread-out live measurements and always-expanded trend | Aligned measurement strip; optional trend; markers and notes directly below. |
| Plain transcript prose with raw filenames | Selectable transcript text on a reading surface; readable media action labels; source filenames retained on disk. |
| Internal event labels such as audio_closed | Human-readable timeline labels such as Audio saved and Session started. |
| Long all-in-one settings window | Connection / Audio / Transcription / Media tabs; device refresh on opening and explicit input choice. |
| Central scroll position carried between screens | Independent scroll state for new, live, and each saved session. |
| Connected peer port could be mistaken for the laptop listener | Explicit Receiver IP:port line alongside the connected monitor peer. |
| Zero decoded messages looked like no data was arriving | Separate received-byte and decoded-message counters with time since the last received bytes. |
| Raw binary capture still said Waiting for measurements | Clear raw-data-saving state; decoding status is distinct from transport receipt. |
| Full export path crowded the status footer | Single-line status with the full message available on hover. |
| Device enumeration probed every virtual audio driver | Read-only Mac device metadata and background discovery; only the selected input is opened. |
| Mac audio stream survived Drop in CPAL 0.15.3 | Backported the upstream weak-listener fix; pause releases the stream, finalizes audio, and retains a frozen waveform. |
| Startup delays shifted the audio display into the past | Session clock anchored before device setup and permission calls. |
| Binary data could only be retained as opaque bytes | Experimental numeric candidates and three raw signal slices, visibly separated from verified measurements. |
| Saved binary captures had no inspection action | Background Inspect binary capture action, with record counts and an explicit saved-snapshot label. |
| A retained socket could outlive incoming data | Inactive-stream wording and stale experimental-snapshot age after three seconds without new data. |
| Always-visible resume authorization | Continue this session disclosure keeps saved review focused. |

## Audio display semantics

The display reads the exact mono channel used by the WAV writer. Peak and RMS are calculated over 25-ms sample windows. The waveform uses a fixed -60 to 0 dBFS peak scale, so silence stays flat and quiet room noise is not automatically enlarged into a full-height signal. Orange bars indicate peaks at or above 0.98. The current status uses a short trailing peak window and current RMS, and reports missing input after 500 ms. Discontinuities reset partial envelope windows, and absent frames remain gaps. Starting audio or a new segment clears display history. Pausing freezes the latest envelope in muted green.

The visualization queue is capped at 320 entries and uses nonblocking sends. The renderer retains at most 320 entries and never writes to the audio file or durable journal. Device discovery runs in the background. Opening the selected input remains synchronous. The timing anchor is created before device setup. CPAL 0.15.3 is vendored with the upstream macOS weak-listener fix; Pause explicitly pauses, drops, drains, and finalizes the capture. Persistence, shared model, and export formats are unchanged.
