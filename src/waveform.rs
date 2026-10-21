//! A bounded, display-only view of the saved microphone channel.
use crate::audio::Level;
use eframe::egui::{self, Color32, Vec2};
use std::collections::VecDeque;

#[derive(Default)]
pub struct Waveform {
    frames: VecDeque<Level>,
}
impl Waveform {
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    pub fn clear(&mut self) {
        self.frames.clear();
    }
    pub fn push(&mut self, level: Level) {
        self.frames.push_back(level);
        while self.frames.len() > 320 {
            self.frames.pop_front();
        }
    }
    pub fn status(&self, now: f64) -> (&'static str, Option<f32>) {
        let Some(last) = self.frames.back().filter(|v| now - v.elapsed < 0.5) else {
            return ("Waiting for audio", None);
        };
        let peak = self
            .frames
            .iter()
            .rev()
            .take_while(|v| now - v.elapsed < 0.5)
            .map(|v| v.peak)
            .fold(0.0_f32, f32::max);
        let db = 20.0 * last.rms.max(0.00001).log10();
        (
            if peak >= 0.98 {
                "Input clipping"
            } else if db < -55.0 {
                "Quiet input"
            } else {
                "Receiving audio"
            },
            Some(db),
        )
    }
    pub fn show(&self, ui: &mut egui::Ui, now: f64, active: bool) {
        let now = if active {
            now
        } else {
            self.frames.back().map_or(now, |f| f.elapsed)
        };
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 94.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let baseline = Color32::from_rgb(54, 78, 74);
        painter.line_segment(
            [
                egui::pos2(rect.left(), rect.center().y),
                egui::pos2(rect.right(), rect.center().y),
            ],
            egui::Stroke::new(1.0, baseline),
        );
        // Fixed time scale: new samples enter at the right and travel left over eight seconds.
        // Missing callback frames remain gaps, including when the UI falls behind.
        let bar_width = (rect.width() / 320.0 * 0.68).clamp(1.0, 3.0);
        for v in &self.frames {
            let age = now - v.elapsed;
            if !(0.0..=8.0).contains(&age) {
                continue;
            }
            let x = rect.right() - (age / 8.0) as f32 * rect.width();
            // Fixed logarithmic mapping, no automatic normalization of noise into "speech".
            let db = 20.0 * v.peak.max(0.001).log10();
            let height =
                (2.0 + ((db + 60.0) / 60.0).clamp(0.0, 1.0).powf(1.5) * 82.0).min(rect.height());
            let color = if !active {
                baseline
            } else if v.peak >= 0.98 {
                Color32::from_rgb(255, 178, 131)
            } else {
                Color32::from_rgb(128, 232, 204)
            };
            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(x, rect.center().y),
                    Vec2::new(bar_width, height),
                ),
                2,
                color,
            );
        }
        response.on_hover_text("Live amplitude · last 8 seconds · recorded microphone channel");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_history_reports_silence_clipping_and_stale_input() {
        let mut w = Waveform::default();
        for i in 0..1000 {
            w.push(Level {
                elapsed: i as f64 / 40.0,
                peak: 0.0,
                rms: 0.0,
            });
        }
        assert_eq!(w.frames.len(), 320);
        assert_eq!(w.status(25.0).0, "Quiet input");
        w.push(Level {
            elapsed: 25.0,
            peak: 1.0,
            rms: 0.7,
        });
        assert_eq!(w.status(25.1).0, "Input clipping");
        assert_eq!(w.status(26.0).0, "Waiting for audio");
        w.clear();
        assert!(w.frames.is_empty());
    }
}
