# Local macOS lifecycle fix

This directory contains the published CPAL 0.15.3 crate (Apache-2.0).
Upstream: https://github.com/RustAudio/cpal/tree/v0.15.3

`src/host/coreaudio/macos/mod.rs` changes the device-disconnect callback
from a strong `Stream` capture to `Arc::downgrade`/`upgrade`, following
the fix present in upstream CPAL 0.16.0. This breaks the ownership cycle
between the stream and its stored listener. Dropping an explicitly
selected input can then release its callback and close the WAV writer's
channel. An unused `Rc` import is also removed.

The application explicitly pauses the stream before dropping it and
joining the writer. The backport retains the previously validated
0.15.3 audio backend; no Windows/Linux backend code is changed.

Reference: https://github.com/RustAudio/cpal/blob/v0.16.0/src/host/coreaudio/macos/mod.rs
