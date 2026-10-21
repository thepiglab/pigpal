# Pig Pal

A Rust desktop app for the **Waveline Pro** patient monitor. Pig Pal receives
monitor data over Ethernet, displays decoded readings in a Waveline-style view,
and saves the complete incoming stream alongside timestamped CSVs.

```text
Waveline Pro -- Ethernet / TCP --> Pig Pal
                                   |-- live monitor view
                                   `-- raw bytes + CSV exports
```

Numeric decoding is experimental. See the [decoder notes](docs/WAVELINE_DECODER.md)
for supported fields and verification limits; ECG waveforms are not yet mapped.

## Build and run

On macOS, install **Rust 1.88+** and the **Xcode command-line tools**, then run
from the repository root:

```sh
cargo run --release --locked
```

For a packaged app, optional audio transcription, or Windows/Linux setup, see
[Building Pig Pal](docs/BUILDING.md). Python is only used by the build and
packaging scripts; the packaged app does not require it.

## Connect the monitor

1. Connect the computer and Waveline Pro to the same Ethernet network.
2. In **Settings → Connection**, choose **TCP listener** with generic HL7 decoding
   and application ACKs off. The tested setup listens on `10.0.0.2:12345` for
   monitor `10.0.0.3`; use the addresses configured for your network and monitor.
3. Start a session and enable **Waveline monitor view**. Use **Copy live CSV path**
   to watch incoming rows, then **Stop & save** to review or export the session.

## More details

- [Recording, exports, and storage](docs/USAGE.md)
- [Ethernet decoding](docs/WAVELINE_DECODER.md) and [validation results](docs/VALIDATION.md)
- [Dependency licenses](THIRD_PARTY.md)
