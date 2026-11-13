# Building Pig Pal

## Run the monitor app

The monitor recorder is a native Rust app. From the repository root:

```sh
cargo run --release --locked
```

For a separate workspace:

```sh
cargo run --release --locked -- --data-dir /absolute/path/to/workspace
```

Rust 1.88+ is required. The macOS package targets macOS 15+ and requires Xcode command-line tools to build. Ethernet recording, numeric display, raw capture, and CSV export do not require the optional speech helper or model.

## Package for macOS

Install Python 3 and Git in addition to the Rust/Xcode tools. Build the local speech helper, then package the app:

```sh
python3 tools/build-speech.py
python3 tools/package.py
open "dist/Pig Pal.app"
```

The speech helper uses Swift and Core ML. Python runs the build/packaging scripts; the packaged app and compiled helper do not require a Python installation. Download the optional transcription model through the app's Settings, once per workspace.

The package is locally ad-hoc signed. It is not Developer ID notarized for distribution to other Macs. Local validation has been performed on macOS; Windows/Linux builds are supplied but have not been validated on hardware.

## Releases

GitHub Actions (`.github/workflows/build.yml`) tests every push and pull request on macOS,
Windows and Linux and packages the macOS DMG and Windows ZIP as run artifacts. A push to `main`
whose `Cargo.toml` version has no release yet publishes one: it tags `v<version>` and attaches
both packages with the notes in `.github/release-notes.md`. To release, bump `version` in
`Cargo.toml` and merge to `main`.

Locally, `python3 tools/package.py --dmg` builds the same `PigPal-<version>-macos-arm64.dmg`.

## Windows and Linux

The portable speech helper uses CMake and a C++17 compiler. Windows builds require Rust MSVC, Visual Studio C++ build tools, Git, CMake, and Python.

On Debian/Ubuntu:

```sh
sudo apt install build-essential pkg-config cmake git python3 libasound2-dev libx11-dev libxi-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev
python3 tools/build-speech.py
cargo test --locked
python3 tools/package.py
```

The packager produces `dist/PigPal/` and a platform-specific ZIP. Linux still needs its system audio/graphics libraries.

## Stage an update

```sh
python3 tools/package.py --output-dir dist/staged
```

Stop/save and close an app instance before opening an update against the same workspace. A workspace lock prevents concurrent writers. Staging a separate bundle does not update a running app.

## Local speech engines

| Platform | Engine |
|---|---|
| Apple Silicon, macOS 15+ | FluidAudio 0.17.7, Redux Core ML, CPU + Metal GPU |
| Intel Mac, macOS 15+ | FluidAudio 0.17.7, Redux Core ML on CPU |
| Windows / Linux | Pinned parakeet.cpp packed Redux on CPU |

Both helpers run locally as child processes and return JSON to Rust. The model stays on disk and is loaded automatically for each clip; it does not download again. Transcription runs after capture. The platform-specific engines may produce different text/timestamps. Raw engine output is retained alongside normalized transcript records. Word offsets outside the source clip are flagged; they are not silently corrected.

Apple Silicon defaults to Core ML with Metal GPU + CPU. The Neural Engine compiler was blocked on a pre-existing job on the validation Mac, while Metal inference completed correctly. For a deliberate ANE comparison, set `PIGPAL_COREML_COMPUTE=ane` before launch. Intel Macs use CPU. Inference has a three-minute timeout per clip; first-time model preparation has a twenty-minute timeout.

The portable GGUF download is pinned to a Hugging Face revision and verified with SHA-256 before installation. macOS uses FluidAudio's Redux download/compilation and a successful local model load before marking the installation ready. Model licenses/attribution are in [THIRD_PARTY.md](../THIRD_PARTY.md).

## Existing workspaces and settings

Pig Pal keeps using an existing Surgery Buddy desktop data folder automatically when no Pig Pal workspace exists. This preserves saved session IDs, live CSV paths, settings, and installed models. New installations use the PigPal application-data folder. `--data-dir` always selects an explicit workspace. Both `PIGPAL_SPEECH_ENGINE` and `PIGPAL_COREML_COMPUTE` are supported; the corresponding legacy `SURGERYBUDDY_*` variables remain accepted for compatibility.

## Tests and command-line checks

```sh
cargo test --locked
cargo run --locked -- --data-dir validation/data --demo-seconds 3
# Optional fixture import (mono WAV, <=30 seconds); does not activate microphone:
cargo run --locked -- --data-dir validation/data --demo-seconds 3 --audio-fixture /path/to/fixture.wav
# Explicit global model preparation / local inference:
cargo run --locked -- --data-dir validation/data --download-model
cargo run --locked -- --data-dir validation/data --transcribe SESSION_UUID
```

See [VALIDATION.md](VALIDATION.md) for actual results and remaining hardware checks. In-app video preview, automatic iPad import, live speech transcription, and clinical validation are outside this desktop build.


See [recording and storage](USAGE.md), [decoder notes](WAVELINE_DECODER.md), and [the original validation record](VALIDATION.md) for details. Run synthetic tests in a separate workspace; they do not validate physical sensor meanings or timing.
