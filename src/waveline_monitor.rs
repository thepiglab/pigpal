//! Read-only monitor layout for the existing decoded Waveline records.
//! This view does not generate signals, infer alarm states, or modify capture.
use crate::waveline::Record;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

const GREEN: Color32 = Color32::from_rgb(57, 244, 77);
const CYAN: Color32 = Color32::from_rgb(56, 232, 237);
const YELLOW: Color32 = Color32::from_rgb(236, 245, 78);
const RED: Color32 = Color32::from_rgb(238, 93, 83);
const DIM: Color32 = Color32::from_rgb(154, 173, 193);
const DARK: Color32 = Color32::from_rgb(30, 39, 47);

fn number(record: Option<&Record>, offset: usize, decimals: usize) -> String {
    record
        .and_then(|r| r.fields.iter().find(|f| f.offset == offset))
        .and_then(|f| f.value)
        .filter(|v| v.is_finite())
        .map(|v| format!("{v:.decimals$}"))
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
    fn rect(&self, x: f32, y: f32, w: f32, h: f32, color: Color32) {
        self.painter.rect_filled(
            Rect::from_min_size(self.point(x, y), Vec2::new(w, h) * self.scale),
            0.0,
            color,
        );
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
        let measured =
            self.painter
                .layout_no_wrap(text.clone(), FontId::monospace(size * self.scale), color);
        let fit = (width * self.scale / measured.size().x.max(1.0)).min(1.0);
        self.text(x, y, size * fit, color, text);
    }
}

/// `age_seconds` is the recorder's monotonic age, not an inferred device time.
/// A record without an age is a saved snapshot and is explicitly labeled so.
pub fn show(
    ui: &mut egui::Ui,
    record: Option<&Record>,
    age_seconds: Option<f64>,
    receipt_utc: Option<&str>,
) {
    let fresh =
        record.is_some() && age_seconds.is_some_and(|age| age <= crate::waveline::STALE_AFTER_S);
    let saved = record.is_some() && age_seconds.is_none();
    let visible = if fresh || saved { record } else { None };
    let status = if fresh {
        "LIVE RECEIVED DATA"
    } else if saved {
        "SAVED SNAPSHOT - NOT LIVE"
    } else if record.is_some() {
        "STALE - WAITING FOR NEW DATA"
    } else {
        "WAITING FOR MONITOR DATA"
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(status).strong().color(if fresh {
            Color32::from_rgb(30, 112, 94)
        } else {
            Color32::from_rgb(159, 85, 40)
        }));
        if let Some(age) = age_seconds {
            ui.label(format!("Last complete record {age:.1}s ago"));
        }
    });
    if let Some(utc) = receipt_utc {
        ui.label(
            egui::RichText::new(format!("Received UTC: {utc}"))
                .monospace()
                .small(),
        );
    }
    ui.label(egui::RichText::new(
        "Candidate field labels and units remain unverified. Unmapped channels stay blank. Zeros retain their raw meaning.",
    ).small());
    ui.add_space(12.0);

    let width = ui.available_width().min(1100.0);
    let scale = width / 1384.0;
    let (bounds, _) = ui.allocate_exact_size(Vec2::new(width, 1080.0 * scale), Sense::hover());
    let painter = ui.painter().with_clip_rect(bounds);
    painter.rect_filled(bounds, 34.0 * scale, Color32::from_rgb(224, 215, 192));
    let housing = Screen {
        painter: &painter,
        origin: bounds.min,
        scale,
    };
    housing.text(
        54.0,
        22.0,
        32.0,
        Color32::from_rgb(45, 55, 52),
        "waveline Pro",
    );
    housing.text(1120.0, 24.0, 30.0, Color32::from_rgb(45, 94, 112), "DRE");
    housing.rect(44.0, 76.0, 1296.0, 916.0, Color32::from_rgb(107, 109, 100));
    let screen = Screen {
        painter: &painter,
        origin: housing.point(52.0, 84.0),
        scale,
    };
    screen.rect(0.0, 0.0, 1280.0, 900.0, DARK);
    screen.rect(0.0, 0.0, 1280.0, 36.0, Color32::from_rgb(129, 158, 195));
    screen.text(14.0, 8.0, 18.0, DARK, "PIG PAL / RECEIVED VALUES");
    screen.text(725.0, 8.0, 17.0, DARK, status);
    for y in [166.0, 297.0, 426.0, 556.0, 685.0, 814.0] {
        screen.rule(0.0, y, 1280.0, y);
    }
    screen.rule(908.0, 36.0, 908.0, 814.0);

    for (y, title, color) in [
        (52.0, "ECG II", GREEN),
        (182.0, "ECG II", GREEN),
        (313.0, "CO2", Color32::from_rgb(89, 136, 255)),
        (442.0, "PLETH", CYAN),
        (572.0, "ART", RED),
        (701.0, "PA", RED),
    ] {
        screen.text(16.0, y, 22.0, color, title);
        screen.text(260.0, y + 42.0, 20.0, DIM, "Waveform not mapped");
    }
    screen.text(655.0, 352.0, 20.0, DIM, "RR candidate");
    screen.text(800.0, 377.0, 37.0, YELLOW, number(visible, 20, 0));
    screen.text(
        20.0,
        392.0,
        19.0,
        DIM,
        format!(
            "T1 {}   T2 {}  (raw)",
            number(visible, 68, 1),
            number(visible, 72, 1),
        ),
    );
    screen.text(922.0, 48.0, 22.0, GREEN, "ECG / HR");
    screen.reading(1060.0, 72.0, 88.0, 204.0, GREEN, number(visible, 14, 0));
    screen.text(922.0, 178.0, 22.0, YELLOW, "NIBP candidate");
    screen.reading(
        922.0,
        211.0,
        48.0,
        342.0,
        YELLOW,
        format!("{} / {}", number(visible, 28, 0), number(visible, 30, 0),),
    );
    screen.text(
        927.0,
        274.0,
        17.0,
        YELLOW,
        format!("MAP {}", number(visible, 32, 0)),
    );
    screen.text(922.0, 310.0, 22.0, Color32::from_rgb(89, 136, 255), "GAS");
    screen.text(935.0, 347.0, 19.0, DIM, "O2   N2O   ISO");
    screen.text(935.0, 378.0, 28.0, DIM, "---  ---   ---");
    screen.text(922.0, 440.0, 22.0, CYAN, "SpO2 candidate");
    screen.reading(1060.0, 465.0, 85.0, 204.0, CYAN, number(visible, 64, 0));
    screen.text(
        927.0,
        524.0,
        21.0,
        GREEN,
        format!("PR {}", number(visible, 66, 0)),
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
    screen.text(922.0, 570.0, 22.0, RED, "P1 / ART");
    screen.reading(
        927.0,
        613.0,
        40.0,
        337.0,
        RED,
        format!("{} / {}", pressure(0), pressure(1)),
    );
    screen.text(
        927.0,
        665.0,
        17.0,
        DIM,
        if arterial.iter().take(3).any(|v| !v.is_empty()) {
            format!("MAP {} · candidate", pressure(2))
        } else {
            if record.is_some() && !saved && !fresh {
                "Stream stale"
            } else {
                "Not reading yet"
            }
            .into()
        },
    );
    screen.text(922.0, 699.0, 22.0, RED, "P2");
    screen.text(927.0, 742.0, 40.0, RED, "--- / ---");
    screen.text(927.0, 794.0, 17.0, DIM, "Not mapped");
    screen.rect(0.0, 814.0, 1280.0, 86.0, Color32::from_rgb(125, 158, 206));
    screen.text(
        18.0,
        835.0,
        20.0,
        DARK,
        "READ ONLY / DATA FROM THE RECORDER",
    );
    screen.text(
        18.0,
        867.0,
        16.0,
        DARK,
        "No simulated traces or inferred device alarms",
    );
    housing.text(
        58.0,
        1016.0,
        21.0,
        Color32::from_rgb(106, 94, 67),
        "Masimo SET",
    );
    housing.text(
        470.0,
        1018.0,
        19.0,
        Color32::from_rgb(85, 89, 78),
        if saved {
            "Saved monitor snapshot"
        } else {
            "Live monitor comparison"
        },
    );
    painter.circle_filled(
        housing.point(1245.0, 1031.0),
        30.0 * scale,
        Color32::from_rgb(151, 155, 140),
    );

    // Widget text also exposes the painted readings to accessibility tools.
    ui.label(
        egui::RichText::new(format!(
            "HR {} · RR {} · NIBP {}/{} (MAP {}) · SpO2 {} · PR {} · T1 {} · T2 {} · P1 {}/{} (MAP {})",
            number(visible, 14, 0),
            number(visible, 20, 0),
            number(visible, 28, 0),
            number(visible, 30, 0),
            number(visible, 32, 0),
            number(visible, 64, 0),
            number(visible, 66, 0),
            number(visible, 68, 1),
            number(visible, 72, 1),
            pressure(0),
            pressure(1),
            pressure(2),
        ))
        .monospace()
        .small(),
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
    fn monitor_displays_record_values_and_hides_all_readings_when_stale() {
        let mut record = record();
        record.arterial_pressure = Some(crate::waveline::ArterialPressure {
            systolic: Some(73.0),
            diastolic: Some(-3.0),
            mean: Some(44.0),
        });
        let before = record.clone();
        let expected = "HR 91 · RR 0 · NIBP 96/77 (MAP 83) · SpO2 99 · PR 90 · T1 37.4 · T2 --- · P1 73/-3 (MAP 44)";
        for age in [0.1, 3.0] {
            let text = rendered(Some(&record), Some(age));
            assert!(text.contains("LIVE RECEIVED DATA"));
            assert!(text.contains(expected));
            assert!(text.contains("2026-10-20T15:18:28.908175000Z"));
            assert!(text.contains("Waveform not mapped"));
        }
        let saved = rendered(Some(&record), None);
        assert!(saved.contains("SAVED SNAPSHOT - NOT LIVE"));
        assert!(!saved.contains("LIVE RECEIVED DATA"));
        assert!(saved.contains(expected));
        for (sample, age) in [(Some(&record), Some(3.001)), (None, None)] {
            let text = rendered(sample, age);
            assert!(!text.contains(expected));
            assert!(text.contains("HR --- · RR --- · NIBP ---/--- (MAP ---)"));
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
