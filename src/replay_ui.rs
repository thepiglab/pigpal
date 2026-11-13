//! Saved-session playback controls. All changes are in-memory cursor state.
use crate::replay::{self, Capture, Player};
use crate::theme;
use chrono::{DateTime, Local, SecondsFormat, Utc};
use eframe::egui::{self, RichText, Sense, Vec2};
use std::time::Instant;

/// A marker or note from the session journal, placed at the moment it was
/// captured (for notes, when the note was opened, not when it was saved).
#[derive(Clone, Debug, PartialEq)]
pub struct ReplayNote {
    pub at: DateTime<Utc>,
    pub label: String,
    pub text: String,
    /// Free-text note (as opposed to a one-click marker such as "Baseline").
    pub is_note: bool,
}

impl ReplayNote {
    /// Heading and optional body. A free-text note reads best as its own words.
    pub fn heading(&self) -> (&str, Option<&str>) {
        if self.label == "Note" && !self.text.is_empty() {
            (&self.text, None)
        } else {
            (
                &self.label,
                (!self.text.is_empty()).then_some(self.text.as_str()),
            )
        }
    }
}

/// Notes within this many seconds before the cursor count as "now".
const NOTE_NOW_S: f64 = 10.0;
const NOTES_WIDTH: f32 = 290.0;

pub struct Viewer {
    pub player: Player,
    input: String,
    error: Option<String>,
    last_tick: Instant,
    fit_monitor: bool,
    pub show_library: bool,
    trace_window_s: f64,
    ecg_zoom: f32,
    notes: Vec<ReplayNote>,
    show_notes: bool,
    /// Note last scrolled into view in the notes panel.
    followed: Option<usize>,
}
impl Viewer {
    pub fn new(capture: Capture, at: Option<&str>, mut notes: Vec<ReplayNote>) -> Self {
        notes.sort_by_key(|n| n.at);
        let show_notes = !notes.is_empty();
        let mut viewer = Self {
            notes,
            show_notes,
            followed: None,
            player: Player::new(capture),
            input: String::new(),
            error: None,
            last_tick: Instant::now(),
            fit_monitor: true,
            show_library: false,
            trace_window_s: 8.0,
            ecg_zoom: 2.0,
        };
        if let Some(at) = at {
            match replay::parse_time(at, viewer.player.cursor) {
                Ok(time) => viewer.jump(time),
                Err(error) => viewer.error = Some(error.to_string()),
            }
        }
        viewer.sync_input();
        viewer
    }
    /// Move the cursor to `at`, clamped to the recorded range, and pause there so
    /// the monitor shows the vitals at that moment.
    pub fn jump(&mut self, at: DateTime<Utc>) {
        let at = at.clamp(self.player.capture.start, self.player.capture.end);
        let _ = self.player.seek(at);
        self.sync_input();
        self.error = None;
        self.last_tick = Instant::now();
    }
    #[cfg(test)]
    pub fn notes(&self) -> &[ReplayNote] {
        &self.notes
    }
    /// Index of the latest note at or before the cursor.
    pub fn current_note(&self) -> Option<usize> {
        self.notes
            .partition_point(|n| n.at <= self.player.cursor)
            .checked_sub(1)
    }
    fn note_target(&self, forward: bool) -> Option<DateTime<Utc>> {
        // Half a second of slack so repeated presses move past the current note.
        let slack = chrono::Duration::milliseconds(500);
        if forward {
            self.notes
                .iter()
                .find(|n| n.at > self.player.cursor + slack)
                .map(|n| n.at)
        } else {
            self.notes
                .iter()
                .rev()
                .find(|n| n.at < self.player.cursor - slack)
                .map(|n| n.at)
        }
    }
    pub fn step_note(&mut self, forward: bool) {
        if let Some(at) = self.note_target(forward) {
            self.jump(at);
        }
    }
    pub fn tick(&mut self) {
        let now = Instant::now();
        self.player.advance(now - self.last_tick);
        self.last_tick = now;
    }
    fn sync_input(&mut self) {
        self.input = self
            .player
            .cursor
            .with_timezone(&Local)
            .format("%H:%M:%S")
            .to_string();
    }
    fn seek_input(&mut self) {
        match replay::parse_time(&self.input, self.player.cursor)
            .and_then(|time| self.player.seek(time))
        {
            Ok(()) => {
                self.error = None;
                self.last_tick = Instant::now();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
    fn toggle_play(&mut self) {
        if !self.player.playing && self.player.cursor >= self.player.capture.end {
            let _ = self.player.seek(self.player.capture.frames[0].utc);
        }
        self.player.playing = !self.player.playing;
        self.last_tick = Instant::now();
    }
    fn step(&mut self, forward: bool) {
        self.player.step(forward);
        self.sync_input();
        self.error = None;
        self.last_tick = Instant::now();
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        use crate::theme::{muted, text, Btn, Icon};
        // Keyboard: Space plays/pauses, arrows step one record, unless typing.
        if !ui.ctx().wants_keyboard_input() {
            let (space, left, right, prev_note, next_note) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Space),
                    i.key_pressed(egui::Key::ArrowLeft),
                    i.key_pressed(egui::Key::ArrowRight),
                    i.key_pressed(egui::Key::OpenBracket),
                    i.key_pressed(egui::Key::CloseBracket),
                )
            });
            if prev_note {
                self.step_note(false);
            }
            if next_note {
                self.step_note(true);
            }
            if space {
                self.toggle_play();
            }
            if left && self.player.index.is_some_and(|i| i > 0) {
                self.step(false);
            }
            if right
                && self
                    .player
                    .index
                    .is_none_or(|i| i + 1 < self.player.capture.frames.len())
            {
                self.step(true);
            }
        }
        // One transport card: controls, clock and scrubber. Display options and
        // receipt metadata live in a menu instead of crowding the monitor.
        theme::card()
            .inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 8 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                ui.horizontal(|ui| {
                    let (label, icon) = if self.player.playing {
                        ("Pause", Icon::Pause)
                    } else {
                        ("Play", Icon::Play)
                    };
                    if Btn::primary(label).icon(icon).width(92.0).show(ui).on_hover_text("Space").clicked() {
                        self.toggle_play();
                    }
                    let can_back = self.player.index.is_some_and(|i| i > 0);
                    let can_fwd = self
                        .player
                        .index
                        .is_none_or(|i| i + 1 < self.player.capture.frames.len());
                    if Btn::ghost("").icon(Icon::Prev).enabled(can_back).show(ui).on_hover_text("Previous record (←)").clicked() {
                        self.step(false);
                    }
                    if Btn::ghost("").icon(Icon::Next).enabled(can_fwd).show(ui).on_hover_text("Next record (→)").clicked() {
                        self.step(true);
                    }
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(self.player.cursor.with_timezone(&Local).format("%H:%M:%S%.3f").to_string())
                            .monospace()
                            .size(17.0)
                            .color(text()),
                    )
                    .on_hover_text(self.player.cursor.with_timezone(&Local).format("%A, %B %-d, %Y").to_string());
                    ui.add_space(4.0);
                    ui.label(theme::muted_text(
                        format!(
                            "Record {} of {}",
                            self.player.index.map_or(0, |i| i + 1),
                            self.player.capture.frames.len()
                        ),
                        12.5,
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.notes.is_empty()
                            && Btn::ghost(format!("Notes  {}", self.notes.len()))
                                .icon(Icon::Note)
                                .selected(self.show_notes)
                                .show(ui)
                                .on_hover_text("Show or hide the notes list")
                                .clicked()
                        {
                            self.show_notes = !self.show_notes;
                        }
                        let display = theme::icon_button(ui, Icon::Settings, "Display options");
                        self.display_menu(ui, &display);
                        ui.add_space(4.0);
                        ui.allocate_ui(Vec2::new(150.0, 34.0), |ui| {
                            let response = theme::text_field(ui, "replay_seek_time", &mut self.input, "Jump to time", None)
                                .on_hover_text("Local time on the shown date. Also accepts YYYY-MM-DD HH:MM:SS or an ISO time with offset. Press Return to jump.");
                            let auto_seek = response.changed() && replay::parse_time(&self.input, self.player.cursor).is_ok();
                            if auto_seek || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                                self.seek_input();
                            }
                        });
                    });
                });
                if let Some(error) = &self.error {
                    ui.label(theme::muted_text(error, 12.0).color(theme::danger()));
                }
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let stamp = |t: chrono::DateTime<chrono::Utc>| {
                        RichText::new(t.with_timezone(&Local).format("%H:%M:%S").to_string())
                            .monospace()
                            .size(11.5)
                            .color(muted())
                    };
                    ui.label(stamp(self.player.capture.start));
                    let mut seconds = self.player.position_s();
                    ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(100.0);
                    let slider = ui.add(
                        egui::Slider::new(&mut seconds, 0.0..=self.player.duration_s()).show_value(false),
                    );
                    if slider.changed() {
                        let time = self.player.capture.start
                            + chrono::Duration::milliseconds((seconds * 1000.0).round() as i64);
                        let _ = self.player.seek(time.min(self.player.capture.end));
                        self.sync_input();
                        self.error = None;
                        self.last_tick = Instant::now();
                    }
                    if let Some(at) = self.note_ticks(ui, slider.rect) {
                        self.jump(at);
                    }
                    ui.label(stamp(self.player.capture.end));
                });
            });
        ui.add_space(12.0);
        let panel = self.show_notes && !self.notes.is_empty();
        let main_width = ui.available_width() - if panel { NOTES_WIDTH + 16.0 } else { 0.0 };
        let mut jump = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.vertical(|ui| {
                ui.set_width(main_width);
                if !self.notes.is_empty() {
                    jump = jump.or(self.note_banner(ui));
                    ui.add_space(12.0);
                }
                self.monitor(ui);
            });
            if panel {
                ui.vertical(|ui| {
                    ui.set_width(NOTES_WIDTH);
                    jump = jump.or(self.notes_panel(ui));
                });
            }
        });
        if let Some(at) = jump {
            self.jump(at);
        }
    }
    fn display_menu(&mut self, ui: &mut egui::Ui, anchor: &egui::Response) {
        use crate::theme::{menu, menu_item};
        let mut fit = None;
        let mut window = None;
        let mut zoom = None;
        menu(ui, "replay_display", anchor, 300.0, |ui| {
            if menu_item(ui, None, "Fit monitor to window", None, self.fit_monitor).clicked() {
                fit = Some(!self.fit_monitor);
            }
            ui.separator();
            ui.label(theme::muted_text("Trace history", 11.5));
            for seconds in [4.0, 8.0, 12.0] {
                if menu_item(
                    ui,
                    None,
                    &format!("{seconds:.0} seconds"),
                    None,
                    self.trace_window_s == seconds,
                )
                .clicked()
                {
                    window = Some(seconds);
                }
            }
            ui.separator();
            ui.label(theme::muted_text("ECG visual zoom (not mV gain)", 11.5));
            for z in [1.0, 2.0, 4.0] {
                if menu_item(ui, None, &format!("{z:.0}×"), None, self.ecg_zoom == z).clicked() {
                    zoom = Some(z);
                }
            }
            ui.separator();
            let small = |t: String| theme::muted_text(t, 11.5);
            ui.label(small(
                "Saved replay · raw 0–255 · timing and calibration unverified".into(),
            ));
            if let Some(frame) = self.player.frame() {
                ui.label(small(format!(
                    "Received {}",
                    frame.utc.with_timezone(&Local).format("%H:%M:%S%.6f %Z")
                )));
                ui.label(small(format!(
                    "{} · segment {}",
                    frame.timestamp_basis, frame.segment
                )));
                ui.label(small(format!(
                    "Connection {} · offset {}",
                    frame.connection, frame.record.stream_offset
                )));
                ui.label(small(frame.utc.to_rfc3339_opts(SecondsFormat::Nanos, true)));
                ui.label(small(format!(
                    "Raw state bytes: ECG {} · pleth {} · P1 {} · P2 {}",
                    frame.record.channel_status.ecg,
                    frame.record.channel_status.pleth,
                    frame.record.channel_status.p1,
                    frame.record.channel_status.p2
                )));
            } else {
                ui.label(small("No record received yet at this time.".into()));
            }
            for warning in &self.player.capture.warnings {
                ui.label(small(warning.clone()).color(theme::warn()));
            }
        });
        if let Some(f) = fit {
            self.fit_monitor = f;
        }
        if let Some(w) = window {
            self.trace_window_s = w;
        }
        if let Some(z) = zoom {
            self.ecg_zoom = z;
        }
    }
    fn monitor(&mut self, ui: &mut egui::Ui) {
        let wave_blocks = self.player.wave_blocks(self.trace_window_s);
        let frame = self.player.frame();
        let utc = frame.map(|f| f.utc.to_rfc3339_opts(SecondsFormat::Nanos, true));
        // Reserve only the compact raw-reading caption, rather than several rows
        // of diagnostics. Rendering can grow beyond the reference canvas size.
        let width = if self.fit_monitor {
            ui.available_width().min(
                (ui.available_height() - 36.0).max(160.0) * crate::waveline_monitor::WIDTH
                    / crate::waveline_monitor::HEIGHT,
            )
        } else {
            ui.available_width()
        };
        egui::ScrollArea::vertical()
            .id_salt("replay_monitor")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        crate::waveline_monitor::show_replay(
                            ui,
                            frame.map(|f| &f.record),
                            self.player.age(),
                            utc.as_deref(),
                            self.player.playing,
                            Some(crate::waveline_monitor::History {
                                blocks: &wave_blocks,
                                window_s: self.trace_window_s,
                                ecg_zoom: self.ecg_zoom,
                            }),
                        );
                    });
                });
            });
    }

    /// Note and marker ticks over the scrubber. Returns a time when one is clicked.
    fn note_ticks(&self, ui: &mut egui::Ui, rect: egui::Rect) -> Option<DateTime<Utc>> {
        let duration = self.player.duration_s();
        if self.notes.is_empty() || duration <= 0.0 {
            return None;
        }
        // Match egui's slider geometry so ticks line up with the handle.
        let range = rect.x_range().shrink(rect.height() / 2.5);
        let current = self.current_note();
        let mut clicked = None;
        for (i, note) in self.notes.iter().enumerate() {
            let offset = (note.at - self.player.capture.start).num_milliseconds() as f64 / 1000.0;
            let x = range.min + (offset / duration).clamp(0.0, 1.0) as f32 * range.span();
            let color = if note.is_note {
                crate::theme::note()
            } else {
                crate::theme::accent()
            };
            let tick = egui::Rect::from_center_size(
                egui::pos2(x, rect.center().y - 9.0),
                Vec2::new(10.0, 12.0),
            );
            let response = ui
                .interact(tick, ui.id().with(("note_tick", i)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!(
                    "{} · {}{}",
                    note.at.with_timezone(&Local).format("%H:%M:%S"),
                    note.label,
                    if note.text.is_empty() {
                        String::new()
                    } else {
                        format!("\n{}", note.text)
                    }
                ));
            let strong = response.hovered() || current == Some(i);
            let painter = ui.painter();
            let (w, h) = if strong { (3.0, 12.0) } else { (2.0, 9.0) };
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(x - w / 2.0, rect.center().y - 3.0 - h),
                    Vec2::new(w, h),
                ),
                1,
                color,
            );
            if response.clicked() {
                clicked = Some(note.at);
            }
        }
        clicked
    }
    /// One line above the monitor naming the note that applies to this moment.
    fn note_banner(&self, ui: &mut egui::Ui) -> Option<DateTime<Utc>> {
        use crate::theme::*;
        let current = self.current_note().map(|i| &self.notes[i]);
        let age = current.map(|n| (self.player.cursor - n.at).num_milliseconds() as f64 / 1000.0);
        let near = age.is_some_and(|a| a <= NOTE_NOW_S);
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::click());
        let painter = ui.painter().clone();
        let fill = if near { accent_soft() } else { surface() };
        painter.rect(
            rect,
            10,
            fill,
            egui::Stroke::new(
                1.0,
                if near {
                    accent().gamma_multiply(0.4)
                } else {
                    line()
                },
            ),
            egui::StrokeKind::Inside,
        );
        let b = centered_baseline(&painter, &bold(13.5), rect.center().y);
        let mut x = rect.left() + 16.0;
        let next = self.note_target(true);
        let right_reserve = if next.is_some() { 120.0 } else { 16.0 };
        match (current, age) {
            (Some(note), Some(age)) => {
                painter.circle_filled(
                    egui::pos2(x + 4.0, b - 4.5),
                    4.5,
                    if note.is_note {
                        theme::note()
                    } else {
                        accent()
                    },
                );
                x += 18.0;
                let (heading, body) = note.heading();
                let r = paint_on_baseline(
                    &painter,
                    layout(
                        &painter,
                        heading,
                        bold(13.5),
                        text(),
                        Some((rect.width() - right_reserve - 260.0).max(120.0)),
                        true,
                    ),
                    x,
                    b,
                );
                x = r.right() + 10.0;
                let r = paint_on_baseline(
                    &painter,
                    painter.layout_no_wrap(
                        note.at.with_timezone(&Local).format("%H:%M:%S").to_string(),
                        mono_font(12.0),
                        muted(),
                    ),
                    x,
                    b,
                );
                x = r.right() + 8.0;
                let when = if near {
                    "now".to_string()
                } else {
                    format!("{} ago", short_duration(age))
                };
                let r = paint_on_baseline(
                    &painter,
                    painter.layout_no_wrap(when, font(12.5), if near { accent() } else { muted() }),
                    x,
                    b,
                );
                x = r.right() + 14.0;
                if let Some(body) = body {
                    paint_on_baseline(
                        &painter,
                        layout(
                            &painter,
                            body,
                            font(13.5),
                            text_2(),
                            Some(rect.right() - right_reserve - x),
                            true,
                        ),
                        x,
                        b,
                    );
                }
            }
            _ => {
                paint_on_baseline(
                    &painter,
                    painter.layout_no_wrap(
                        "No notes yet at this point in the recording".into(),
                        font(13.0),
                        muted(),
                    ),
                    x,
                    b,
                );
            }
        }
        let mut clicked = None;
        if let Some(next) = next {
            let r = egui::Rect::from_min_max(
                egui::pos2(rect.right() - 120.0, rect.top()),
                egui::pos2(rect.right() - 8.0, rect.bottom()),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(r)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if Btn::ghost("Next note")
                .trailing(Icon::ChevronRight)
                .size(Size::Sm)
                .show(&mut child)
                .on_hover_text("]")
                .clicked()
            {
                clicked = Some(next);
            }
        }
        if clicked.is_none() && current.is_some() {
            let response = response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Jump to this note");
            if response.clicked() {
                clicked = current.map(|n| n.at);
            }
        }
        clicked
    }
    /// Every note and marker; click one to see the monitor at that moment.
    fn notes_panel(&mut self, ui: &mut egui::Ui) -> Option<DateTime<Utc>> {
        use crate::theme::*;
        let current = self.current_note();
        let follow = current != self.followed;
        self.followed = current;
        let mut clicked = None;
        let mut step = None;
        card()
            .inner_margin(egui::Margin {
                left: 8,
                right: 8,
                top: 12,
                bottom: 8,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(semibold("Notes", 15.0));
                    ui.label(muted_text(self.notes.len().to_string(), 12.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if Btn::ghost("")
                            .icon(Icon::ChevronRight)
                            .size(Size::Sm)
                            .enabled(self.note_target(true).is_some())
                            .show(ui)
                            .on_hover_text("Next note (])")
                            .clicked()
                        {
                            step = Some(true);
                        }
                        if Btn::ghost("")
                            .icon(Icon::ChevronLeft)
                            .size(Size::Sm)
                            .enabled(self.note_target(false).is_some())
                            .show(ui)
                            .on_hover_text("Previous note ([)")
                            .clicked()
                        {
                            step = Some(false);
                        }
                    });
                });
                ui.add_space(8.0);
                let max_h = (ui.available_height() - 4.0).max(160.0);
                egui::ScrollArea::vertical()
                    .id_salt("replay_notes")
                    .auto_shrink([false, true])
                    .max_height(max_h)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let width = ui.available_width();
                        for (i, note) in self.notes.iter().enumerate() {
                            let active = current == Some(i);
                            let painter = ui.painter().clone();
                            let text_w = width - 32.0;
                            let (heading, body_text) = note.heading();
                            let time = painter.layout_no_wrap(
                                note.at.with_timezone(&Local).format("%H:%M:%S").to_string(),
                                mono_font(11.5),
                                muted(),
                            );
                            // Heading wraps beside the clock time; the body wraps below.
                            let head = layout(
                                &painter,
                                heading,
                                bold(13.5),
                                text(),
                                Some(width - 28.0 - time.size().x - 24.0),
                                false,
                            );
                            let body = body_text.map(|t| {
                                layout(&painter, t, font(13.0), text_2(), Some(text_w), false)
                            });
                            let head_extra = head.size().y - 18.0;
                            let height = 12.0
                                + 18.0
                                + head_extra.max(0.0)
                                + body.as_ref().map_or(0.0, |b| b.size().y + 4.0)
                                + 12.0;
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
                            if active {
                                painter.rect_filled(rect, 8, palette().selected);
                            } else if response.hovered() {
                                painter.rect_filled(rect, 8, hover());
                            }
                            let b = rect.top() + 12.0 + 13.5;
                            let color = if note.is_note {
                                theme::note()
                            } else {
                                accent()
                            };
                            painter.circle_filled(
                                egui::pos2(rect.left() + 14.0, b - 4.5),
                                4.0,
                                color,
                            );
                            paint_on_baseline_right(&painter, time, rect.right() - 12.0, b);
                            let head_rect =
                                paint_on_baseline(&painter, head, rect.left() + 28.0, b);
                            if let Some(body) = body {
                                painter.galley(
                                    egui::pos2(rect.left() + 28.0, head_rect.bottom() + 4.0),
                                    body,
                                    text_2(),
                                );
                            }
                            if active && follow {
                                ui.scroll_to_rect(rect, Some(egui::Align::Center));
                            }
                            if response
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                clicked = Some(note.at);
                            }
                        }
                    });
            });
        if let Some(forward) = step {
            clicked = self.note_target(forward);
        }
        clicked
    }
}

fn short_duration(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    match (s / 3600, (s / 60) % 60, s % 60) {
        (0, 0, s) => format!("{s}s"),
        (0, m, s) => format!("{m}m {s:02}s"),
        (h, m, _) => format!("{h}h {m:02}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::Frame;

    fn at(s: u32) -> DateTime<Utc> {
        format!("2026-10-20T14:00:{s:02}Z").parse().unwrap()
    }
    fn viewer(notes: Vec<ReplayNote>) -> Viewer {
        let mut bytes = vec![0; crate::waveline::RECORD_LEN];
        bytes[..4].copy_from_slice(&crate::waveline::MAGIC);
        bytes[4..8].copy_from_slice(&(crate::waveline::RECORD_LEN as u32).to_le_bytes());
        let record = crate::waveline::Record::decode(&bytes, 0).unwrap();
        let frames = (0..50)
            .map(|i| Frame {
                utc: at(i),
                segment: "s".into(),
                connection: "c".into(),
                timestamp_basis: "test".into(),
                elapsed_s: Some(f64::from(i)),
                peer: String::new(),
                record: record.clone(),
            })
            .collect();
        let capture = Capture {
            frames,
            start: at(0),
            end: at(49),
            warnings: vec![],
        };
        Viewer::new(capture, None, notes)
    }
    fn note(s: u32, label: &str) -> ReplayNote {
        ReplayNote {
            at: at(s),
            label: label.into(),
            text: String::new(),
            is_note: false,
        }
    }

    #[test]
    fn notes_navigate_and_jump_to_the_recorded_moment() {
        // Out of order on purpose; the viewer sorts them.
        let mut v = viewer(vec![
            note(30, "Sample"),
            note(10, "Baseline"),
            note(55, "After end"),
        ]);
        assert_eq!(v.notes()[0].label, "Baseline");
        assert_eq!(v.current_note(), None);
        v.step_note(true);
        assert_eq!(v.player.cursor, at(10));
        assert_eq!(v.current_note(), Some(0));
        assert_eq!(
            v.player.index,
            Some(10),
            "monitor shows the record at the note"
        );
        v.step_note(true);
        assert_eq!(v.player.cursor, at(30));
        // A note after the capture clamps to its end instead of failing.
        v.step_note(true);
        assert_eq!(v.player.cursor, at(49));
        v.step_note(false);
        assert_eq!(v.player.cursor, at(30));
        assert!(!v.player.playing, "jumping pauses on the moment");
        // Opening replay at an RFC 3339 time (from the session timeline).
        let v = viewer(vec![]);
        let v2 = Viewer::new(
            Capture {
                frames: v.player.capture.frames,
                start: at(0),
                end: at(49),
                warnings: vec![],
            },
            Some("2026-10-20T14:00:21Z"),
            vec![],
        );
        assert_eq!(v2.player.cursor, at(21));
    }
}
