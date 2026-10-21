# Desktop interface — October 20, 2026

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
