# Third-party components

- Rust dependencies are locked in `Cargo.lock`; each retains its own license.
- egui/eframe 0.31.1: MIT or Apache-2.0, https://github.com/emilk/egui
- CPAL 0.15.3: Apache-2.0, https://github.com/RustAudio/cpal. Vendored in `third_party/cpal` with a macOS disconnect-listener ownership fix backported from 0.16.0; see `SURGERYBUDDY-PATCH.md` there. Its license is included in packaged applications.
- FluidAudio 0.17.7: Apache-2.0, https://github.com/FluidInference/FluidAudio. Its optional/embedded dependencies retain their licenses. `native/macos/Package.resolved` locks the speech dependency.
- parakeet.cpp: MIT, https://github.com/mudler/parakeet.cpp, revision `5c9b0fbbd8e611d88e47d7e1ebae9bca9b447e8f`, including its pinned ggml dependencies. This adapter is used for Windows/Linux; the Mac build uses Core ML by default. See upstream LICENSE and third-party notices in the fetched source.
- Parakeet Redux weights: CC BY 4.0. Model by Moondream, https://huggingface.co/moondream/parakeet-redux, derived from NVIDIA Parakeet TDT 0.6B v3. macOS conversion by FluidInference. Portable GGUF conversion by mudler/parakeet.cpp, https://huggingface.co/mudler/parakeet-cpp-gguf, revision `741158ae71e64ef5c89385862c18f777d07a97a1`. `redux-packed.gguf`: 213319296 bytes, SHA-256 `574614b9a4d9f72ab202877a7ad6a1f4bf819dd1d27b9d42dfe8cd429fcebdd5`.
- Model weights are downloaded separately, not embedded in the source or app bundle.
- FFmpeg is an optional separately installed executable, not bundled. Its license depends on the user's build.
- The speech acceptance fixture is the upstream parakeet.cpp `tests/fixtures/speech.wav` public speech fixture. No new microphone recording is made by automated tests.

Inter 4.1 by Rasmus Andersson is bundled under the SIL Open Font License 1.1. Source: https://github.com/rsms/inter/releases/tag/v4.1. License: `assets/fonts/LICENSE.txt` (included in packaged applications).
