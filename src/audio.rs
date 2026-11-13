use crate::store::{id, utc, write_json};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub file: String,
    pub started: DateTime<Utc>,
    pub segment_elapsed_s: f64,
    pub frames: u64,
    pub sample_rate: u32,
    pub duration_s: f64,
    pub device: String,
    pub state: String,
    pub timing_note: String,
}
pub enum AudioEvent {
    Closed(Clip),
    Error(String),
}
/// Display-only 25 ms envelope, computed from the same channel saved to disk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub elapsed: f64,
    pub peak: f32,
    pub rms: f32,
}

struct Meter {
    window: usize,
    count: usize,
    peak: f32,
    squares: f64,
}
impl Meter {
    fn new(rate: u32) -> Self {
        Self {
            window: (rate / 40).max(1) as usize,
            count: 0,
            peak: 0.0,
            squares: 0.0,
        }
    }
    fn reset(&mut self) {
        self.count = 0;
        self.peak = 0.0;
        self.squares = 0.0;
    }
    fn push(&mut self, samples: &[f32], start: f64, rate: u32, mut emit: impl FnMut(Level)) {
        for (i, sample) in samples.iter().enumerate() {
            let v = if sample.is_finite() {
                sample.abs().min(1.0)
            } else {
                0.0
            };
            self.peak = self.peak.max(v);
            self.squares += f64::from(v) * f64::from(v);
            self.count += 1;
            if self.count == self.window {
                emit(Level {
                    elapsed: start + (i + 1) as f64 / f64::from(rate),
                    peak: self.peak,
                    rms: (self.squares / self.count as f64).sqrt() as f32,
                });
                self.reset();
            }
        }
    }
}
struct Block {
    samples: Vec<f32>,
    at: DateTime<Utc>,
    elapsed: f64,
    gap: bool,
}
/// The platform audio stream cannot move between threads, so it lives on its own
/// small thread for its whole life. That keeps `Microphone` (and the recorder that
/// owns it) `Send`, which lets capture run independently of the window.
pub struct Microphone {
    stream_stop: Option<Sender<()>>,
    stream_thread: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    pub rx: Receiver<AudioEvent>,
    pub levels: Receiver<Level>,
    failed: Arc<AtomicBool>,
}
impl Microphone {
    pub fn devices() -> Result<Vec<String>> {
        #[cfg(target_os = "macos")]
        return crate::audio_devices_macos::input_names();
        #[cfg(not(target_os = "macos"))]
        Ok(cpal::default_host()
            .input_devices()?
            .filter_map(|d| d.name().ok())
            .collect())
    }
    pub fn start(dir: PathBuf, device_name: &str, elapsed: f64) -> Result<Self> {
        // Anchor before any device/permission calls, keeping live samples on the
        // session timeline even when initial device setup takes time.
        let clock = Instant::now();
        let (tx, rx) = mpsc::sync_channel::<Block>(64);
        let (out, events) = mpsc::channel();
        // A slow renderer may lose display frames, never recorded samples.
        let (level_tx, levels) = mpsc::sync_channel(320);
        let failed = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let stream_failed = failed.clone();
        let stream_out = out.clone();
        let name = device_name.to_string();
        let stream_thread = thread::Builder::new()
            .name("pigpal-audio-stream".into())
            .spawn(move || {
                let built = build_stream(
                    &name,
                    elapsed,
                    clock,
                    tx,
                    level_tx,
                    stream_failed,
                    stream_out,
                );
                let stream = match built {
                    Ok((stream, rate)) => {
                        let _ = ready_tx.send(Ok(rate));
                        stream
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                // Block until stop is requested or the Microphone is dropped.
                let _ = stop_rx.recv();
                let _ = stream.pause();
                drop(stream); // Release callback sender before the writer finalizes.
            })?;
        let rate = match ready_rx.recv() {
            Ok(Ok(rate)) => rate,
            Ok(Err(e)) => {
                let _ = stream_thread.join();
                return Err(e);
            }
            Err(_) => {
                let _ = stream_thread.join();
                bail!("Microphone setup stopped unexpectedly");
            }
        };
        let name = device_name.to_string();
        let writer_flag = failed.clone();
        let writer = thread::Builder::new()
            .name("pigpal-audio-writer".into())
            .spawn(move || {
                if let Err(e) = write_blocks(&dir, &name, rate, rx, &out) {
                    writer_flag.store(true, Ordering::Relaxed);
                    let _ = out.send(AudioEvent::Error(format!("Audio storage failed: {e:#}")));
                }
            })?;
        Ok(Self {
            stream_stop: Some(stop_tx),
            stream_thread: Some(stream_thread),
            writer: Some(writer),
            rx: events,
            levels,
            failed,
        })
    }
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
    pub fn stop(&mut self) {
        // Stopping the stream thread drops the stream and its callback sender,
        // so the writer drains every queued block and finalizes the WAV.
        drop(self.stream_stop.take());
        if let Some(thread) = self.stream_thread.take() {
            let _ = thread.join();
        }
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}
impl Drop for Microphone {
    fn drop(&mut self) {
        self.stop();
    }
}
/// Open the selected input and start its stream on the calling thread.
fn build_stream(
    device_name: &str,
    elapsed: f64,
    clock: Instant,
    tx: mpsc::SyncSender<Block>,
    level_tx: mpsc::SyncSender<Level>,
    failed: Arc<AtomicBool>,
    out: Sender<AudioEvent>,
) -> Result<(cpal::Stream, u32)> {
    let host = cpal::default_host();
    let device = host
        .devices()?
        .find(|d| d.name().ok().as_deref() == Some(device_name))
        .context("Selected microphone is unavailable. Choose an input device.")?;
    let supported = device.default_input_config()?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let err_flag = failed.clone();
    let err_out = out.clone();
    let error = move |e| {
        err_flag.store(true, Ordering::Relaxed);
        let _ = err_out.send(AudioEvent::Error(format!(
            "Microphone interrupted: {e}. Restart explicitly."
        )));
    };
    let channels = config.channels as usize;
    let rate = config.sample_rate.0;
    let mut meter = Meter::new(rate);
    let flag = failed.clone();
    let drop_out = out.clone();
    let mut previous: Option<(cpal::StreamInstant, f64)> = None;
    let mut deliver = move |samples: Vec<f32>, info: &cpal::InputCallbackInfo| {
        if flag.load(Ordering::Relaxed) {
            return;
        }
        let capture = info.timestamp().capture;
        let gap = previous
            .map(|(p, d)| {
                capture
                    .duration_since(&p)
                    .map(|t| (t.as_secs_f64() - d).abs() > 0.05)
                    .unwrap_or(true)
            })
            .unwrap_or(false);
        let duration = samples.len() as f64 / rate as f64;
        previous = Some((capture, duration));
        let start = elapsed + clock.elapsed().as_secs_f64() - duration;
        if gap {
            meter.reset();
        }
        meter.push(&samples, start, rate, |level| {
            let _ = level_tx.try_send(level);
        });
        // Receipt minus block duration is an estimate, not calibrated hardware time.
        let block = Block {
            samples,
            at: utc() - chrono::Duration::microseconds((duration * 1e6) as i64),
            elapsed: start,
            gap,
        };
        if tx.try_send(block).is_err() {
            flag.store(true, Ordering::Relaxed);
            let _=drop_out.send(AudioEvent::Error("Audio queue overflow: capture stopped, received samples retained. Restart microphone explicitly.".into()));
        }
    };
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |d: &[f32], i| deliver(d.chunks(channels).map(|s| s[0]).collect(), i),
            error,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |d: &[i16], i| {
                deliver(
                    d.chunks(channels).map(|s| s[0] as f32 / 32768.0).collect(),
                    i,
                )
            },
            error,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config,
            move |d: &[u16], i| {
                deliver(
                    d.chunks(channels)
                        .map(|s| (s[0] as f32 - 32768.0) / 32768.0)
                        .collect(),
                    i,
                )
            },
            error,
            None,
        )?,
        _ => bail!("This input sample format is unsupported; choose another microphone"),
    };
    stream.play()?;
    Ok((stream, rate))
}

fn write_blocks(
    dir: &Path,
    device: &str,
    rate: u32,
    rx: Receiver<Block>,
    out: &Sender<AudioEvent>,
) -> Result<()> {
    let mut current: Option<(hound::WavWriter<BufWriter<File>>, Clip)> = None;
    for block in rx {
        if block.gap {
            finish(dir, &mut current, out)?;
        }
        let mut offset = 0;
        while offset < block.samples.len() {
            if current.is_none() {
                let uid = id();
                let file = format!("audio/{uid}.wav");
                let clip=Clip{id:uid,file:file.clone(),started:block.at+chrono::Duration::microseconds((offset as f64/rate as f64*1e6) as i64),segment_elapsed_s:(block.elapsed+offset as f64/rate as f64).max(0.0),frames:0,sample_rate:rate,duration_s:0.0,device:device.into(),state:"open".into(),timing_note:"Channel 0, Float32 PCM; host callback minus buffer duration estimates first sample. Pauses and detected gaps create separate clips.".into()};
                let spec = hound::WavSpec {
                    channels: 1,
                    sample_rate: rate,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                };
                let writer = hound::WavWriter::create(dir.join(&file), spec)?;
                write_json(&dir.join(format!("{file}.json")), &clip)?;
                current = Some((writer, clip));
            }
            let (writer, clip) = current.as_mut().unwrap();
            let count =
                (block.samples.len() - offset).min((rate as u64 * 30 - clip.frames) as usize);
            for &sample in &block.samples[offset..offset + count] {
                writer.write_sample(sample)?;
            }
            clip.frames += count as u64;
            offset += count;
            if clip.frames >= rate as u64 * 30 {
                finish(dir, &mut current, out)?;
            }
        }
    }
    finish(dir, &mut current, out)
}
fn finish(
    dir: &Path,
    current: &mut Option<(hound::WavWriter<BufWriter<File>>, Clip)>,
    out: &Sender<AudioEvent>,
) -> Result<()> {
    if let Some((writer, mut clip)) = current.take() {
        writer.finalize()?;
        // Flushing needs a writable handle on Windows (FlushFileBuffers);
        // a read-only handle fails there with "Access is denied".
        std::fs::OpenOptions::new()
            .write(true)
            .open(dir.join(&clip.file))?
            .sync_all()?;
        clip.duration_s = clip.frames as f64 / clip.sample_rate as f64;
        clip.state = "closed".into();
        write_json(&dir.join(format!("{}.json", clip.file)), &clip)?;
        let _ = out.send(AudioEvent::Closed(clip));
    }
    Ok(())
}

/// Import a WAV as a separate, explicitly labelled clip. The original file bytes are preserved.
pub fn import_wav(segment_dir: &Path, input: &Path, elapsed: f64) -> Result<Clip> {
    let reader = hound::WavReader::open(input)?;
    let spec = reader.spec();
    anyhow::ensure!(
        spec.channels == 1,
        "Import mono WAV audio; original recordings remain unchanged"
    );
    anyhow::ensure!(
        reader.duration() as f64 / spec.sample_rate as f64 <= 30.0,
        "Import clips of at most 30 seconds"
    );
    let uid = id();
    let file = format!("audio/{uid}.wav");
    std::fs::copy(input, segment_dir.join(&file))?;
    let clip = Clip {
        id: uid,
        file,
        started: utc(),
        segment_elapsed_s: elapsed,
        frames: reader.duration() as u64,
        sample_rate: spec.sample_rate,
        duration_s: reader.duration() as f64 / spec.sample_rate as f64,
        device: "Imported test fixture".into(),
        state: "closed".into(),
        timing_note: "Imported audio; timestamp is import time, not original acquisition time."
            .into(),
    };
    write_json(&segment_dir.join(format!("{}.json", clip.file)), &clip)?;
    Ok(clip)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meter_tracks_silence_speech_and_clipping_without_touching_samples() {
        let samples = [
            vec![0.0; 400],
            vec![0.1; 400],
            vec![-0.8; 400],
            vec![1.2; 400],
        ]
        .concat();
        let original = samples.clone();
        let mut result = vec![];
        Meter::new(16000).push(&samples, 3.0, 16000, |v| result.push(v));
        assert_eq!(samples, original);
        assert_eq!(result.len(), 4);
        for (v, expected) in result.iter().zip([0.0, 0.1, 0.8, 1.0]) {
            assert!((v.peak - expected).abs() < 0.00001);
            assert!((v.rms - expected).abs() < 0.00001);
        }
        assert_eq!(result[0].elapsed, 3.025);
        assert_eq!(result[3].elapsed, 3.1);
    }
    #[test]
    fn meter_is_independent_of_callback_size_and_resets_at_gaps() {
        let samples: Vec<_> = (0..4800).map(|i| (i as f32 * 0.37).sin() * 0.4).collect();
        let mut whole = vec![];
        Meter::new(48000).push(&samples, 0.0, 48000, |v| whole.push(v));
        let mut split = vec![];
        let mut meter = Meter::new(48000);
        for (n, chunk) in samples.chunks(512).enumerate() {
            meter.push(chunk, (n * 512) as f64 / 48000.0, 48000, |v| split.push(v));
        }
        assert_eq!(whole.len(), split.len());
        for (a, b) in whole.iter().zip(split) {
            assert_eq!(a.peak, b.peak);
            assert_eq!(a.rms, b.rms);
            assert!((a.elapsed - b.elapsed).abs() < 1e-12);
        }
        meter.push(&[1.0; 600], 0.1, 48000, |_| panic!("Partial window"));
        meter.reset();
        meter.push(&[0.0; 1200], 10.0, 48000, |v| {
            assert_eq!(v.peak, 0.0);
            assert_eq!(v.elapsed, 10.025);
        });
    }
    #[test]
    fn saturated_display_queue_does_not_interrupt_audio_storage() {
        let (tx, rx) = mpsc::sync_channel(1);
        let samples = vec![0.25; 16000];
        let mut meter = Meter::new(16000);
        meter.push(&samples, 0.0, 16000, |level| {
            let _ = tx.try_send(level);
        });
        assert_eq!(rx.try_iter().count(), 1);
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("audio")).unwrap();
        let (block_tx, block_rx) = mpsc::channel();
        let (out, events) = mpsc::channel();
        block_tx
            .send(Block {
                samples: samples.clone(),
                at: utc(),
                elapsed: 0.0,
                gap: false,
            })
            .unwrap();
        drop(block_tx);
        write_blocks(temp.path(), "display overflow test", 16000, block_rx, &out).unwrap();
        let AudioEvent::Closed(clip) = events.try_recv().unwrap() else {
            panic!("No finalized clip")
        };
        let actual: Vec<f32> = hound::WavReader::open(temp.path().join(clip.file))
            .unwrap()
            .into_samples()
            .map(Result::unwrap)
            .collect();
        assert_eq!(actual, samples);
    }
    #[test]
    fn sample_counts_and_gap_anchors_survive_chunking() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("audio")).unwrap();
        let (tx, rx) = mpsc::channel();
        let (out, events) = mpsc::channel();
        let begin = utc();
        let a: Vec<f32> = (0..4000).map(|i| (i as f32 * 0.1).sin()).collect();
        let b = vec![0.25; 200];
        tx.send(Block {
            samples: a.clone(),
            at: begin,
            elapsed: 0.0,
            gap: false,
        })
        .unwrap();
        tx.send(Block {
            samples: b.clone(),
            at: begin + chrono::Duration::seconds(50),
            elapsed: 50.0,
            gap: true,
        })
        .unwrap();
        drop(tx);
        write_blocks(temp.path(), "synthetic samples", 100, rx, &out).unwrap();
        drop(out);
        let clips: Vec<_> = events
            .into_iter()
            .filter_map(|e| {
                if let AudioEvent::Closed(c) = e {
                    Some(c)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            clips.iter().map(|c| c.frames).collect::<Vec<_>>(),
            vec![3000, 1000, 200]
        );
        assert_eq!(clips[2].segment_elapsed_s, 50.0);
        assert_eq!((clips[2].started - clips[1].started).num_seconds(), 20);
        let actual: Vec<f32> = clips
            .iter()
            .flat_map(|c| {
                hound::WavReader::open(temp.path().join(&c.file))
                    .unwrap()
                    .into_samples::<f32>()
                    .map(|v| v.unwrap())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(actual, a.into_iter().chain(b).collect::<Vec<_>>());
    }
}
