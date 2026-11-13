//! Read-only monitor layout for the existing decoded Waveline records.
//! This view does not generate signals, infer alarm states, or modify capture.
use crate::waveline::Record;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};

/// Photo-derived reference canvas, cropped to the active measurement area.
pub const WIDTH: f32 = 1024.0;
pub const HEIGHT: f32 = 690.0;
const GREEN: Color32 = Color32::from_rgb(20, 245, 32);
const CYAN: Color32 = Color32::from_rgb(0, 225, 255);
const YELLOW: Color32 = Color32::from_rgb(224, 242, 42);
const RED: Color32 = Color32::from_rgb(255, 65, 48);
const DIM: Color32 = Color32::from_rgb(154, 173, 193);
const DARK: Color32 = Color32::from_rgb(7, 9, 11);

fn number(record: Option<&Record>, offset: usize, decimals: usize) -> String {
    record
        .and_then(|r| r.fields.iter().find(|f| f.offset == offset))
        .and_then(|f| f.value)
        .filter(|v| v.is_finite())
        .map(|v| format!("{v:.decimals$}"))
        .unwrap_or_else(|| "---".into())
}

fn temperature_1(record: Option<&Record>) -> String {
    record
        .and_then(|r| r.fields.iter().find(|f| f.offset == 68))
        .and_then(|f| f.value)
        .filter(|v| v.is_finite())
        .map(|c| format!("{c:.1} C / {:.1} F", c * 1.8 + 32.0))
        .unwrap_or_else(|| "---".into())
}

struct Screen<'a> {
    painter: &'a egui::Painter,
    origin: Pos2,
    scale: f32,
}
impl Screen<'_> {
    fn point(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.scale
    }
    fn text(&self, x: f32, y: f32, size: f32, color: Color32, text: impl ToString) {
        self.painter.text(
            self.point(x, y),
            Align2::LEFT_TOP,
            text.to_string(),
            FontId::monospace(size * self.scale),
            color,
        );
    }
    fn rule(&self, x: f32, y: f32, x2: f32, y2: f32) {
        self.painter.line_segment(
            [self.point(x, y), self.point(x2, y2)],
            Stroke::new(self.scale.max(0.5), Color32::from_rgb(72, 86, 99)),
        );
    }
    fn reading(&self, x: f32, y: f32, size: f32, width: f32, color: Color32, text: impl ToString) {
        let text = text.to_string();
        let measured = self.painter.layout_no_wrap(
            text.clone(),
            FontId::new(size * self.scale, egui::FontFamily::Name("Semibold".into())),
            color,
        );
        let fit = (width * self.scale / measured.size().x.max(1.0)).min(1.0);
        self.painter.text(
            self.point(x + width, y),
            Align2::RIGHT_TOP,
            text,
            FontId::new(
                size * fit * self.scale,
                egui::FontFamily::Name("Semibold".into()),
            ),
            color,
        );
    }
    fn history_wave(&self, y: f32, offset: usize, color: Color32, history: &History<'_>) {
        let plot = egui::Rect::from_min_max(self.point(12.0, y + 5.0), self.point(702.0, y + 87.0));
        let painter = self
            .painter
            .with_clip_rect(self.painter.clip_rect().intersect(plot));
        let gain = if offset == 180 { history.ecg_zoom } else { 1.0 };
        for block in history.blocks {
            let Some(signal) = block
                .record
                .signal_slices
                .iter()
                .find(|s| s.offset == offset)
            else {
                continue;
            };
            if signal.samples.len() < 2 {
                continue;
            }
            // Each block is its own path. Never connect through packet boundaries,
            // gaps, repeated receipt timestamps or connection changes.
            let points = signal
                .samples
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let t = block.start_s
                        + (block.end_s - block.start_s) * i as f64
                            / (signal.samples.len() - 1) as f64;
                    let raw = if offset == 180 {
                        128.0 + (f32::from(v) - 128.0) * gain
                    } else {
                        f32::from(v)
                    };
                    self.point(
                        12.0 + ((t + history.window_s) / history.window_s) as f32 * 690.0,
                        y + 82.0 - raw / 255.0 * 72.0,
                    )
                })
                .collect();
            painter.add(egui::Shape::line(
                points,
                Stroke::new(self.scale.max(0.8), color),
            ));
        }
    }
    fn raw_wave(&self, y: f32, samples: &[u8], color: Color32) {
        if samples.len() < 2 {
            return;
        }
        // Fixed raw-byte axis; never auto-scale, smooth, infer millivolts,
        // interpolate time, or concatenate packets across a capture gap.
        let points = samples
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                self.point(
                    12.0 + i as f32 / (samples.len() - 1) as f32 * 690.0,
                    y + 82.0 - f32::from(v) / 255.0 * 72.0,
                )
            })
            .collect();
        self.painter.add(egui::Shape::line(
            points,
            Stroke::new(self.scale.max(0.8), color),
        ));
    }
}

pub struct History<'a> {
    pub blocks: &'a [crate::replay::WaveBlock<'a>],
    pub window_s: f64,
    pub ecg_zoom: f32,
}

/// `age_seconds` is the recorder's monotonic age, not an inferred device time.
/// A record without an age is a saved snapshot and is explicitly labeled so.
pub fn show(
    ui: &mut egui::Ui,
    record: Option<&Record>,
    age_seconds: Option<f64>,
    receipt_utc: Option<&str>,
) {
    show_mode(ui, record, age_seconds, receipt_utc, None, None);
}

pub fn show_replay(
    ui: &mut egui::Ui,
    record: Option<&Record>,
    age_seconds: Option<f64>,
    receipt_utc: Option<&str>,
    playing: bool,
    history: Option<History<'_>>,
) {
    show_mode(ui, record, age_seconds, receipt_utc, Some(playing), history);
}

fn show_mode(
    ui: &mut egui::Ui,
    record: Option<&Record>,
    age_seconds: Option<f64>,
    receipt_utc: Option<&str>,
    replay: Option<bool>,
    history: Option<History<'_>>,
) {
    let fresh =
        record.is_some() && age_seconds.is_some_and(|age| age <= crate::waveline::STALE_AFTER_S);
    let saved = record.is_some() && age_seconds.is_none() && replay.is_none();
    let visible = if fresh || saved { record } else { None };
    let status = if let Some(playing) = replay {
        if !fresh {
            "REPLAY GAP - NO RECORDED DATA"
        } else if playing {
            "REPLAY PLAYING - SAVED DATA"
        } else {
            "REPLAY PAUSED - SAVED DATA"
        }
    } else if fresh {
        "LIVE RECEIVED DATA"
    } else if saved {
        "SAVED SNAPSHOT - NOT LIVE"
    } else if record.is_some() {
        "STALE - WAITING FOR NEW DATA"
    } else {
        "WAITING FOR MONITOR DATA"
    };
    if replay.is_none() {
        ui.horizontal(|ui| {
            crate::theme::chip(
                ui,
                status,
                if fresh {
                    crate::theme::live()
                } else {
                    crate::theme::warn()
                },
            )
            .on_hover_text(
                "Field validity and waveform calibration remain unverified. Unmapped channels stay blank. Zeros retain their raw meaning.",
            );
            if let Some(age) = age_seconds {
                ui.label(crate::theme::muted_text(
                    format!("Last complete record {age:.1}s ago"),
                    12.0,
                ));
            }
            if let Some(utc) = receipt_utc {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        crate::theme::mono(format!("Received UTC: {utc}"), 11.0)
                            .color(crate::theme::muted()),
                    );
                });
            }
        });
        ui.add_space(8.0);
    }
    // Keep the reference aspect ratio at every window size and Retina density.
    // Coordinates are logical layout units, not a claim about the physical LCD.
    let width = ui.available_width();
    let scale = width / WIDTH;
    let (bounds, response) =
        ui.allocate_exact_size(Vec2::new(width, HEIGHT * scale), Sense::hover());
    response.on_hover_text("Photo-derived measurement area with the device header and toolbar omitted. Waveforms use a fixed raw 0–255 axis and one received block, not the device's calibrated sweep.");
    let painter = ui.painter().with_clip_rect(bounds);
    let screen = Screen {
        painter: &painter,
        origin: bounds.min - Vec2::new(0.0, 28.0 * scale),
        scale,
    };
    let gas_blue = Color32::from_rgb(40, 100, 255);
    let violet = Color32::from_rgb(200, 104, 255);
    painter.rect_filled(bounds, 10.0 * scale.min(1.0), DARK);
    painter.rect_stroke(
        bounds,
        10.0 * scale.min(1.0),
        Stroke::new(1.0, crate::theme::line()),
        egui::StrokeKind::Inside,
    );
    for y in [143.0, 258.0, 373.0, 488.0, 603.0, 718.0] {
        screen.rule(0.0, y, WIDTH, y);
    }
    screen.rule(718.0, 28.0, 718.0, 718.0);

    for (y, title, color, offset) in [
        (34.0, "ECG", GREEN, Some(180)),
        (149.0, "ECG cascade", GREEN, None),
        (264.0, "CO2", gas_blue, None),
        (379.0, "PLETH", CYAN, Some(356)),
        (494.0, "ART", RED, Some(668)),
        (609.0, "PA", RED, None),
    ] {
        screen.text(9.0, y, 14.0, color, title);
        if let Some(offset) = offset {
            screen.text(
                500.0,
                y,
                12.0,
                DIM,
                if let Some(h) = &history {
                    format!("{:.0}s RECEIPT HISTORY", h.window_s)
                } else {
                    "RAW BLOCK / 0-255".into()
                },
            );
            if let Some(signal) =
                visible.and_then(|r| r.signal_slices.iter().find(|s| s.offset == offset))
            {
                if let Some(history) = &history {
                    screen.history_wave(y + 15.0, offset, color, history);
                } else {
                    screen.raw_wave(y + 15.0, &signal.samples, color);
                }
            } else {
                screen.text(220.0, y + 49.0, 15.0, DIM, "No current waveform block");
            }
        } else {
            screen.text(265.0, y + 49.0, 15.0, DIM, "Waveform not mapped");
        }
    }
    // Gas labels follow the photographed layout, with missing values kept explicit.
    screen.text(62.0, 264.0, 14.0, DIM, "mmHg ET --- FI ---");
    screen.text(611.0, 285.0, 14.0, DIM, "MAC ---");
    screen.text(595.0, 351.0, 15.0, CYAN, "RR --- rpm");
    let fahrenheit = visible
        .and_then(|r| r.fields.iter().find(|f| f.offset == 68))
        .and_then(|f| f.value)
        .filter(|v| v.is_finite())
        .map(|c| format!("{:.1}", c * 1.8 + 32.0))
        .unwrap_or_else(|| "---".into());
    screen.text(
        55.0,
        351.0,
        15.0,
        Color32::from_rgb(180, 213, 255),
        format!("°F T1: {fahrenheit}  T2: ---"),
    );
    screen.text(70.0, 379.0, 13.0, GREEN, "Pi: --- %");
    let spo2_off = visible.is_some_and(Record::spo2_sensor_off_candidate);
    if spo2_off {
        screen.text(245.0, 379.0, 13.0, CYAN, "Sensor off");
    }

    screen.text(725.0, 34.0, 15.0, GREEN, "ECG");
    screen.text(982.0, 34.0, 14.0, GREEN, "bpm");
    screen.text(729.0, 119.0, 15.0, DIM, "ST --- mV");
    screen.reading(810.0, 47.0, 92.0, 204.0, GREEN, number(visible, 14, 0));
    screen.text(725.0, 149.0, 15.0, YELLOW, "NIBP");
    screen.text(970.0, 149.0, 14.0, YELLOW, "mmHg");
    screen.reading(
        727.0,
        177.0,
        55.0,
        224.0,
        YELLOW,
        format!("{}/{}", number(visible, 28, 0), number(visible, 30, 0)),
    );
    screen.text(961.0, 179.0, 11.0, DIM, "MAP*");
    screen.reading(
        959.0,
        197.0,
        30.0,
        56.0,
        YELLOW,
        if number(visible, 32, 0) == "0" {
            "---".into()
        } else {
            number(visible, 32, 0)
        },
    );
    screen.text(
        725.0,
        239.0,
        11.0,
        DIM,
        "*Raw MAP 0 shown as ---; phase unknown",
    );

    screen.text(725.0, 264.0, 15.0, gas_blue, "GAS");
    for (x, label, color) in [
        (763.0, "O2%", gas_blue),
        (831.0, "N2O%", gas_blue),
        (900.0, "ISO%", violet),
        (970.0, "N/A%", CYAN),
    ] {
        screen.text(x, 287.0, 13.0, color, label);
        screen.text(x, 312.0, 20.0, color, "---");
        screen.text(x, 342.0, 20.0, color, "---");
    }
    screen.text(725.0, 316.0, 13.0, DIM, "ET");
    screen.text(725.0, 346.0, 13.0, DIM, "FI");
    screen.text(725.0, 379.0, 15.0, CYAN, "SpO2");
    screen.text(998.0, 379.0, 14.0, CYAN, "%");
    screen.reading(
        815.0,
        400.0,
        78.0,
        199.0,
        CYAN,
        if spo2_off {
            "---".into()
        } else {
            number(visible, 64, 0)
        },
    );
    screen.text(
        736.0,
        461.0,
        23.0,
        GREEN,
        format!(
            "{} bpm",
            if spo2_off {
                "---".into()
            } else {
                number(visible, 66, 0)
            }
        ),
    );

    let arterial = crate::waveline::ArterialPressure::cells(
        visible.and_then(|r| r.arterial_pressure.as_ref()),
    );
    let pressure = |i: usize| {
        if arterial[i].is_empty() {
            "---"
        } else {
            &arterial[i]
        }
    };
    screen.text(725.0, 494.0, 15.0, RED, "P1");
    screen.text(970.0, 494.0, 14.0, RED, "mmHg");
    screen.reading(
        727.0,
        530.0,
        46.0,
        195.0,
        RED,
        format!("{}/{}", pressure(0), pressure(1)),
    );
    screen.reading(945.0, 530.0, 46.0, 69.0, RED, pressure(2));
    screen.text(
        725.0,
        584.0,
        11.0,
        DIM,
        if arterial.iter().take(3).any(|v| !v.is_empty()) {
            "SYS / DIA                 MAP"
        } else {
            "Not reading yet"
        },
    );
    screen.text(725.0, 609.0, 15.0, RED, "P2");
    screen.text(970.0, 609.0, 14.0, RED, "mmHg");
    screen.reading(727.0, 645.0, 43.0, 287.0, RED, "--- / ---  ---");
    screen.text(725.0, 699.0, 11.0, DIM, "Not mapped");

    if replay.is_none() {
        ui.add_space(4.0);
        ui.label(crate::theme::muted_text(
            if history.is_some() {
                "Photo reference · measurement area only · Raw blocks positioned by receipt interval; sample timing and device gain unverified"
            } else {
                "Photo reference · measurement area only · Latest block / raw 0-255 / timing unverified · device sweep/gain not reproduced"
            },
            11.0,
        ));
        if let Some(history) = &history {
            ui.label(crate::theme::muted_text(format!("{:.0}s receipt history · blocks separate · within-block time estimated · ECG visual zoom {:.0}× (not mV gain)", history.window_s, history.ecg_zoom), 11.0));
        }
    }
    // Widget text also exposes the painted readings to accessibility tools.
    ui.label(
        crate::theme::mono(format!(
            "HR {} · RESP {} · Gas RR unmapped · NIBP {}/{} (MAP {}) · SpO2 {} · PR {} · T1 {} · T2 {} · P1 {}/{} (MAP {})",
            number(visible, 14, 0),
            number(visible, 20, 0),
            number(visible, 28, 0),
            number(visible, 30, 0),
            number(visible, 32, 0),
            number(visible, 64, 0),
            number(visible, 66, 0),
            temperature_1(visible),
            number(visible, 72, 1),
            pressure(0),
            pressure(1),
            pressure(2),
        ), 11.0)
        .color(crate::theme::muted()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record {
        let mut bytes = vec![0; crate::waveline::RECORD_LEN];
        bytes[..4].copy_from_slice(&crate::waveline::MAGIC);
        bytes[4..8].copy_from_slice(&(crate::waveline::RECORD_LEN as u32).to_le_bytes());
        for (offset, value) in [
            (14, 91_u16),
            (28, 96),
            (30, 77),
            (32, 83),
            (64, 99),
            (66, 90),
        ] {
            bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes[68..72].copy_from_slice(&37.4_f32.to_le_bytes());
        bytes[72..76].copy_from_slice(&f32::NAN.to_le_bytes());
        Record::decode(&bytes, 0).unwrap()
    }

    #[test]
    fn comparison_preserves_zero_missing_and_display_rounding_without_changing_record() {
        let record = record();
        assert_eq!(number(Some(&record), 14, 0), "91");
        assert_eq!(number(Some(&record), 30, 0), "77");
        assert_eq!(number(Some(&record), 20, 0), "0");
        assert_eq!(number(Some(&record), 68, 1), "37.4");
        assert_eq!(temperature_1(Some(&record)), "37.4 C / 99.3 F");
        assert_eq!(temperature_1(None), "---");
        assert_eq!(number(Some(&record), 72, 1), "---");
        assert_eq!(number(Some(&record), 999, 0), "---");
        assert_eq!(number(None, 14, 0), "---");
        assert_ne!(
            record.fields.iter().find(|f| f.offset == 68).unwrap().value,
            Some(37.4)
        );
    }

    fn rendered(record: Option<&Record>, age: Option<f64>) -> String {
        let context = egui::Context::default();
        crate::ui::configure_style(&context);
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show(ui, record, age, Some("2026-10-20T15:18:28.908175000Z"))
            });
        });
        output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::epaint::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn photo_sensor_off_signature_hides_display_without_rewriting_raw_values() {
        let mut r = record();
        r.channel_status.pleth = 2;
        for field in &mut r.fields {
            if [64, 66].contains(&field.offset) {
                field.value = Some(0.0);
            }
        }
        let before = r.clone();
        assert!(r.spo2_sensor_off_candidate());
        let text = rendered(Some(&r), Some(0.1));
        assert!(text.contains("Sensor off"));
        assert!(text.contains("--- bpm"));
        assert!(
            text.contains("SpO2 0 · PR 0"),
            "Raw summary must retain original zeros"
        );
        assert_eq!(r, before);
        r.channel_status.pleth = 4;
        assert!(
            !r.spo2_sensor_off_candidate(),
            "Zero alone must not be interpreted as sensor off"
        );
        r.channel_status.pleth = 2;
        r.fields.iter_mut().find(|f| f.offset == 64).unwrap().value = Some(95.0);
        assert!(!r.spo2_sensor_off_candidate());
    }

    #[test]
    fn monitor_displays_record_values_and_hides_all_readings_when_stale() {
        let mut record = record();
        record.arterial_pressure = Some(crate::waveline::ArterialPressure {
            systolic: Some(73.0),
            diastolic: Some(-3.0),
            mean: Some(44.0),
        });
        let before = record.clone();
        let expected = "HR 91 · RESP 0 · Gas RR unmapped · NIBP 96/77 (MAP 83) · SpO2 99 · PR 90 · T1 37.4 C / 99.3 F · T2 --- · P1 73/-3 (MAP 44)";
        for age in [0.1, 3.0] {
            let text = rendered(Some(&record), Some(age));
            assert!(text.contains("LIVE RECEIVED DATA"));
            assert!(text.contains(expected));
            assert!(text.contains("2026-10-20T15:18:28.908175000Z"));
            assert!(text.contains("Waveform not mapped"));
            assert!(text.contains("Latest block / raw 0-255 / timing unverified"));
            assert!(!text.contains("No current waveform block"));
        }
        let saved = rendered(Some(&record), None);
        assert!(saved.contains("SAVED SNAPSHOT - NOT LIVE"));
        assert!(!saved.contains("LIVE RECEIVED DATA"));
        assert!(saved.contains(expected));
        for (sample, age) in [(Some(&record), Some(3.001)), (None, None)] {
            let text = rendered(sample, age);
            assert!(!text.contains(expected));
            assert!(text.contains("HR --- · RESP --- · Gas RR unmapped · NIBP ---/--- (MAP ---)"));
            assert!(text.contains("No current waveform block"));
            assert!(text.contains("P1 ---/--- (MAP ---)"));
            assert!(!text.contains("LIVE RECEIVED DATA"));
        }
        assert_eq!(record, before, "Rendering must not alter recorded values");
        record.fields[0].value = Some(105.0);
        let changed = rendered(Some(&record), Some(0.0));
        assert!(changed.contains("HR 105"));
        assert!(!changed.contains("HR 91"));
    }
}

#[cfg(test)]
mod replay_tests {
    use super::*;
    #[test]
    fn replay_is_never_labeled_live_and_clears_waveforms_across_gaps() {
        let mut bytes = vec![0; crate::waveline::RECORD_LEN];
        bytes[..4].copy_from_slice(&crate::waveline::MAGIC);
        bytes[4..8].copy_from_slice(&(crate::waveline::RECORD_LEN as u32).to_le_bytes());
        bytes[14..16].copy_from_slice(&110_u16.to_le_bytes());
        let record = Record::decode(&bytes, 0).unwrap();
        for (playing, age) in [(false, 0.323), (true, 0.5), (false, 4.0)] {
            let context = egui::Context::default();
            crate::ui::configure_style(&context);
            let output = context.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show_replay(ui, Some(&record), Some(age), None, playing, None);
                });
            });
            let text = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::epaint::Shape::Text(t) => Some(t.galley.job.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            assert!(!text.contains("LIVE RECEIVED DATA"));
            if age > 3.0 {
                assert!(text.contains("HR ---"));
            } else {
                assert!(text.contains("HR 110"));
            }
            let ecg_paths = output
                .shapes
                .iter()
                .filter(|s| {
                    matches!(&s.shape,
                egui::epaint::Shape::Path(p) if p.points.len()==72)
                })
                .count();
            assert_eq!(ecg_paths, usize::from(age <= 3.0));
        }
    }
}
