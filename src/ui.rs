use crate::{
    audio, export, media, model,
    recorder::Recorder,
    store::{self, Event, Root, Session},
    transport::{Source, SourceConfig},
};
use anyhow::Result;
use eframe::egui::{self, Color32, RichText, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

const TEAL: Color32 = Color32::from_rgb(30, 112, 94);
const INK: Color32 = Color32::from_rgb(31, 43, 42);
const PAPER: Color32 = Color32::from_rgb(248, 249, 247);
const LINE: Color32 = Color32::from_rgb(222, 228, 224);
const SOFT: Color32 = Color32::from_rgb(234, 241, 236);
const MINT: Color32 = Color32::from_rgb(128, 232, 204);
const MUTED: Color32 = Color32::from_rgb(105, 119, 113);
#[derive(Default, Serialize, Deserialize)]
struct Settings {
    source: SourceConfig,
    microphone: String,
    video_input: String,
}
enum Update {
    Progress(String),
    BinaryInspection(String, Box<crate::waveline::Inspection>),
    Done(std::result::Result<String, String>),
}
pub struct Desktop {
    root: Root,
    sessions: Vec<Session>,
    selected: Option<String>,
    recorder: Option<Recorder>,
    settings: Settings,
    settings_open: bool,
    subject: String,
    title: String,
    authorized: bool,
    error: Option<String>,
    notice: String,
    review: VecDeque<Event>,
    transcripts: Vec<model::Transcript>,
    job: Option<Receiver<Update>>,
    note: String,
    note_anchor: Option<String>,
    devices: Vec<String>,
    device_scan: Option<Receiver<std::result::Result<Vec<String>, String>>>,
    search: String,
    plot_key: String,
    photo_path: String,
    photo_caption: String,
    close_confirm: bool,
    waveform: crate::waveform::Waveform,
    settings_tab: usize,
    binary_review: Option<crate::waveline::Inspection>,
    waveline_monitor_view: bool,
}
impl Desktop {
    pub fn new(cc: &eframe::CreationContext<'_>, root: Root) -> Self {
        configure_style(&cc.egui_ctx);
        let settings = store::read_json(&root.path.join("settings.json")).unwrap_or_default();
        let sessions = root.sessions().unwrap_or_default();
        Self {
            root,
            sessions,
            selected: None,
            recorder: None,
            settings,
            settings_open: false,
            subject: String::new(),
            title: String::new(),
            authorized: false,
            error: None,
            notice: "Everything is saved locally on this laptop.".into(),
            review: VecDeque::new(),
            transcripts: vec![],
            job: None,
            note: String::new(),
            note_anchor: None,
            devices: vec![],
            device_scan: None,
            search: String::new(),
            plot_key: String::new(),
            photo_path: String::new(),
            photo_caption: String::new(),
            close_confirm: false,
            waveform: Default::default(),
            settings_tab: 0,
            binary_review: None,
            waveline_monitor_view: false,
        }
    }
    fn report<T>(&mut self, result: Result<T>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                None
            }
        }
    }
    fn refresh(&mut self) {
        let r = self.root.sessions();
        if let Some(s) = self.report(r) {
            self.sessions = s;
        }
    }
    fn select(&mut self, id: String) {
        if self.selected.as_deref() != Some(&id) {
            self.binary_review = None;
        }
        self.selected = Some(id.clone());
        let dir = self.root.session_dir(&id);
        let r = store::recent_events(&dir);
        if let Some((events, warnings)) = self.report(r) {
            self.review = events;
            if !warnings.is_empty() {
                self.notice = warnings.join("\n");
            }
        }
        let r = model::transcripts(&dir);
        if let Some(t) = self.report(r) {
            self.transcripts = t;
        }
    }
    fn start(&mut self, resume: Option<Session>) {
        let mut config = self.settings.source.clone();
        let session = if let Some(s) = resume {
            if s.synthetic {
                config.mode = Source::Demo;
            }
            s
        } else {
            let r = self
                .root
                .create(&self.subject, &self.title, config.mode == Source::Demo);
            let Some(s) = self.report(r) else {
                return;
            };
            s
        };
        let r = Recorder::start(&self.root, session, config);
        if let Some(rec) = self.report(r) {
            self.selected = Some(rec.session.id.clone());
            self.recorder = Some(rec);
            self.waveform.clear();
            self.authorized = false;
            self.note_anchor = None;
            self.notice = "Recording. Microphone starts only when you enable it.".into();
            self.refresh();
        }
    }
    fn stop(&mut self, interrupted: bool) {
        if let Some(mut rec) = self.recorder.take() {
            let id = rec.session.id.clone();
            let r = rec.stop(&self.root, interrupted);
            self.report(r);
            self.note_anchor = None;
            self.refresh();
            self.select(id);
            self.notice = "Capture saved. Review, transcribe, or export below.".into();
        }
    }
    fn job(&mut self, f: impl FnOnce(Sender<Update>) -> Result<String> + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let result = f(tx.clone()).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Update::Done(result));
        });
    }
    fn tick(&mut self) {
        if let Some(result) = self.device_scan.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.device_scan = None;
            match result {
                Ok(devices) => self.devices = devices,
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(rec) = &mut self.recorder {
            if let Err(e) = rec.poll() {
                self.error = Some(format!("Capture interrupted: {e:#}"));
                self.stop(true);
            }
        }
        if let Some(mic) = self.recorder.as_ref().and_then(|r| r.mic.as_ref()) {
            for level in mic.levels.try_iter() {
                self.waveform.push(level);
            }
        }
        loop {
            let update = self.job.as_ref().and_then(|rx| rx.try_recv().ok());
            match update {
                Some(Update::Progress(p)) => self.notice = p,
                Some(Update::BinaryInspection(id, inspection)) => {
                    if self.selected.as_deref() == Some(&id) {
                        self.binary_review = Some(*inspection);
                    }
                }
                Some(Update::Done(result)) => {
                    self.job = None;
                    match result {
                        Ok(msg) => self.notice = msg,
                        Err(e) => self.error = Some(e),
                    };
                    if let Some(id) = self.selected.clone() {
                        self.select(id);
                    }
                    break;
                }
                None => break,
            }
        }
    }
    fn scan_devices(&mut self) {
        if self.device_scan.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.device_scan = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(audio::Microphone::devices().map_err(|e| format!("{e:#}")));
        });
    }
    fn open_settings(&mut self, tab: usize) {
        self.settings_tab = tab;
        self.settings_open = true;
        self.scan_devices();
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library")
            .resizable(false)
            .exact_width(248.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(239, 242, 238))
                    .inner_margin(20),
            )
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 9, TEAL);
                    for (i, h) in [8.0, 17.0, 11.0, 21.0, 8.0].iter().enumerate() {
                        let x = r.left() + 8.0 + i as f32 * 4.0;
                        ui.painter().line_segment(
                            [
                                egui::pos2(x, r.center().y - h / 2.0),
                                egui::pos2(x, r.center().y + h / 2.0),
                            ],
                            egui::Stroke::new(2.0, Color32::WHITE),
                        );
                    }
                    ui.label(semibold("Pig Pal", 17.0));
                });
                ui.add_space(24.0);
                if ui
                    .add_enabled(
                        self.recorder.is_none() && self.job.is_none(),
                        primary("+  New session").min_size(Vec2::new(ui.available_width(), 42.0)),
                    )
                    .clicked()
                {
                    self.selected = None;
                    self.authorized = false;
                }
                ui.add_space(22.0);
                eyebrow(ui, "SESSION LIBRARY");
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .margin(Vec2::new(10.0, 10.0))
                        .hint_text("Search sessions…")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(10.0);
                let mut selection = None;
                egui::ScrollArea::vertical()
                    .id_salt("sessions")
                    .max_height((ui.available_height() - 138.0).max(60.0))
                    .show(ui, |ui| {
                        let mut count = 0;
                        for s in &self.sessions {
                            if !format!("{} {}", s.subject, s.title)
                                .to_lowercase()
                                .contains(&self.search.to_lowercase())
                            {
                                continue;
                            }
                            count += 1;
                            let selected = self.selected.as_ref() == Some(&s.id);
                            let fill = if selected {
                                Color32::WHITE
                            } else {
                                Color32::TRANSPARENT
                            };
                            let response = egui::Frame::new()
                                .fill(fill)
                                .corner_radius(10)
                                .inner_margin(12)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    if ui
                                        .add_enabled(
                                            self.recorder.is_none() && self.job.is_none(),
                                            egui::Button::new(
                                                semibold(&s.title, 13.0).color(if selected {
                                                    TEAL
                                                } else {
                                                    INK
                                                }),
                                            )
                                            .frame(false)
                                            .wrap()
                                            .min_size(Vec2::new(ui.available_width(), 32.0)),
                                        )
                                        .clicked()
                                    {
                                        selection = Some(s.id.clone());
                                    }
                                    ui.label(RichText::new(&s.subject).size(11.0).color(MUTED));
                                    ui.label(
                                        RichText::new(format!(
                                            "{}  ·  {}",
                                            s.created.with_timezone(&chrono::Local).format("%b %d"),
                                            if s.synthetic { "Demo" } else { &s.state }
                                        ))
                                        .size(11.0)
                                        .color(MUTED),
                                    );
                                })
                                .response;
                            if selected {
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        response.rect.left_top() + Vec2::new(0.0, 12.0),
                                        Vec2::new(3.0, response.rect.height() - 24.0),
                                    ),
                                    2,
                                    TEAL,
                                );
                            }
                            ui.add_space(4.0);
                        }
                        if count == 0 {
                            ui.label(
                                RichText::new(if self.sessions.is_empty() {
                                    "Your saved sessions will appear here."
                                } else {
                                    "No matching sessions."
                                })
                                .size(12.0)
                                .color(MUTED),
                            );
                        }
                    });
                if let Some(id) = selection {
                    self.select(id);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(RichText::new("LOCAL WORKSPACE").size(10.0).color(MUTED));
                    ui.add_space(8.0);
                    if ui
                        .add(
                            egui::Button::new("Settings")
                                .frame(false)
                                .min_size(Vec2::new(ui.available_width(), 40.0)),
                        )
                        .clicked()
                    {
                        self.open_settings(0);
                    }
                    if ui
                        .add(
                            egui::Button::new("Open data folder")
                                .frame(false)
                                .min_size(Vec2::new(ui.available_width(), 40.0)),
                        )
                        .clicked()
                    {
                        let r = open::that(&self.root.path).map_err(Into::into);
                        self.report(r);
                    }
                    ui.separator();
                });
            });
    }
    fn new_session(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        eyebrow(ui, "WORKSPACE  /  NEW SESSION");
        ui.add_space(18.0);
        ui.label(semibold("New session", 34.0));
        ui.label(RichText::new("Set up a recording for this subject and session.").color(MUTED));
        ui.add_space(28.0);
        surface().show(ui, |ui| {
            ui.set_width(ui.available_width().min(660.0));
            ui.label(semibold("Session details", 17.0));
            ui.add_space(16.0);
            ui.columns(2, |cols| {
                cols[0].label(semibold("Subject / research ID", 12.0));
                cols[0].add(
                    egui::TextEdit::singleline(&mut self.subject)
                        .margin(Vec2::new(10.0, 10.0))
                        .hint_text("Subject 001")
                        .desired_width(f32::INFINITY),
                );
                cols[1].label(semibold("Session title", 12.0));
                cols[1].add(
                    egui::TextEdit::singleline(&mut self.title)
                        .margin(Vec2::new(10.0, 10.0))
                        .hint_text("Baseline & intervention")
                        .desired_width(f32::INFINITY),
                );
            });
            ui.add_space(20.0);
            ui.separator();
            ui.add_space(14.0);
            ui.label(semibold("Monitor input", 12.0));
            ui.horizontal_wrapped(|ui| {
                source_combo(ui, &mut self.settings.source.mode);
                if ui.button("Configure input").clicked() {
                    self.open_settings(0);
                }
            });
            ui.label(
                RichText::new(if self.settings.source.mode == Source::Demo {
                    "Synthetic demo · generated measurements, no monitor connection."
                } else {
                    "Use the connection settings confirmed for your monitor."
                })
                .size(12.0)
                .color(MUTED),
            );
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                status_dot(ui, TEAL);
                ui.label(
                    RichText::new("Audio is optional. Enable the microphone after starting.")
                        .size(12.0)
                        .color(MUTED),
                );
            });
            ui.add_space(24.0);
            ui.checkbox(
                &mut self.authorized,
                "I am authorized to record this session",
            );
            ui.add_space(12.0);
            if ui
                .add_enabled(
                    self.authorized
                        && !self.subject.trim().is_empty()
                        && !self.title.trim().is_empty()
                        && self.job.is_none(),
                    primary("Start session").min_size(Vec2::new(180.0, 44.0)),
                )
                .clicked()
            {
                self.start(None);
            }
        });
        ui.add_space(28.0);
        ui.horizontal(|ui| {
            status_dot(ui, TEAL);
            ui.label(semibold(
                if model::installed(&self.root.path) {
                    "Offline transcription ready"
                } else {
                    "Your data stays on this laptop"
                },
                12.0,
            ));
        });
        ui.label(RichText::new("Save a session to review its audio, read transcripts, and export everything together.").size(12.0).color(MUTED));
    }
    fn live(&mut self, ui: &mut egui::Ui) {
        let rec = self.recorder.as_ref().unwrap();
        ui.horizontal(|ui| {
            status_dot(ui, TEAL);
            eyebrow(ui, "SESSION IN PROGRESS");
            if rec.session.synthetic {
                badge(ui, "SYNTHETIC DEMO");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format_time(rec.journal.elapsed()))
                        .monospace()
                        .size(19.0)
                        .color(INK),
                );
            });
        });
        ui.add_space(12.0);
        ui.label(semibold(&rec.session.title, 30.0));
        ui.label(
            RichText::new(format!(
                "{}   ·   Segment {}",
                rec.session.subject,
                rec.session.segments.len()
            ))
            .color(MUTED),
        );
        ui.add_space(16.0);
        let mut stop = false;
        ui.horizontal(|ui| {
            if ui.add(primary("Stop & save")).clicked() {
                stop = true;
            }
            if ui.button("Session settings").clicked() {
                self.open_settings(1);
            }
        });
        if stop {
            self.stop(false);
            return;
        }
        ui.add_space(20.0);
        self.audio_panel(ui);
        ui.add_space(24.0);
        let rec = self.recorder.as_ref().unwrap();
        ui.horizontal_wrapped(|ui| {
            eyebrow(ui, "MONITOR");
            ui.label(
                RichText::new(format!(
                    "{} bytes received  ·  {}  ·  {}",
                    rec.bytes,
                    if rec.binary_records > 0 {
                        format!("{} experimental records", rec.binary_records)
                    } else {
                        format!("{} decoded messages", rec.messages)
                    },
                    rec.status
                ))
                .size(11.0)
                .color(MUTED),
            );
        });
        ui.horizontal_wrapped(|ui| {
            let source = rec.source();
            if matches!(source.mode, Source::TcpListen | Source::Udp) {
                ui.label(
                    RichText::new(format!("Receiver {}:{}", source.bind, source.port))
                        .monospace()
                        .size(11.0)
                        .color(MUTED),
                );
            }
            if let Some(received) = rec.last_received_elapsed_s {
                ui.label(
                    RichText::new(format!(
                        "Last data {:.1}s ago",
                        (rec.journal.elapsed() - received).max(0.0)
                    ))
                    .size(11.0)
                    .color(MUTED),
                );
            }
        });
        ui.add_space(8.0);
        if rec.binary_records > 0 {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Live CSV · updated with each record")
                        .size(11.0)
                        .color(MUTED),
                );
                let path = rec.journal.dir.join("waveline_live.csv");
                if ui
                    .button("Copy live CSV path")
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    ui.ctx().copy_text(path.display().to_string());
                }
            });
        }
        if !rec.source().hl7 {
            ui.add_sized(
                Vec2::new(230.0, 40.0),
                egui::Checkbox::new(&mut self.waveline_monitor_view, "Waveline monitor view"),
            )
            .on_hover_text(
                "Compare received values in the monitor layout. Unmapped waveforms stay blank.",
            );
        }
        surface().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if self.waveline_monitor_view && !rec.source().hl7 {
                let receipt = rec.last_binary_received_utc
                    .map(|utc| utc.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true));
                crate::waveline_monitor::show(
                    ui,
                    rec.binary_latest.as_ref(),
                    rec.last_binary_elapsed_s.map(|at| (rec.journal.elapsed() - at).max(0.0)),
                    receipt.as_deref(),
                );
                return;
            }
            if let Some(record) = &rec.binary_latest {
                binary_panel(ui, record, rec.binary_records, rec.last_binary_elapsed_s.map(|at| (rec.journal.elapsed() - at).max(0.0)));
                return;
            }
            if !rec.source().hl7 {
                arterial_panel(ui, None, None);
                ui.add_space(16.0);
            }
            if rec.latest.is_empty() {
                let (title, detail) = if rec.bytes > 0 && rec.last_received_elapsed_s.is_some_and(|at| rec.journal.elapsed() - at > 3.0) {
                    (
                        "Monitor stream is inactive",
                        "Previously received data is saved. No new data has arrived for more than three seconds.",
                    )
                } else if rec.bytes > 0 {
                    if rec.source().hl7 {
                        (
                            "Data received · no decoded measurements",
                            "Raw data is saved. No supported HL7 measurements have been decoded yet.",
                        )
                    } else {
                        (
                            "Monitor data is being saved",
                            "Raw stream recording is enabled. Measurements are not decoded for this input.",
                        )
                    }
                } else {
                    (
                        "Waiting for monitor data",
                        "No data has arrived yet. Check the connection settings and monitor output.",
                    )
                };
                ui.label(semibold(title, 16.0));
                ui.label(
                    RichText::new(detail).color(MUTED),
                );
            } else {
                let width = ((ui.available_width() - 48.0) / 4.0).max(100.0);
                ui.horizontal_wrapped(|ui| {
                    for v in rec.latest.values().take(8) {
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            ui.label(RichText::new(&v.observation.label).size(12.0).color(MUTED));
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&v.observation.value_text)
                                        .monospace()
                                        .size(30.0)
                                        .color(INK),
                                );
                                ui.label(
                                    RichText::new(&v.observation.units).size(11.0).color(MUTED),
                                );
                            });
                            ui.label(
                                RichText::new(format!(
                                    "{:.0}s ago",
                                    (rec.journal.elapsed() - v.elapsed_s).max(0.0)
                                ))
                                .size(10.0)
                                .color(MUTED),
                            )
                            .on_hover_text(&v.observation.source);
                        });
                    }
                });
            }
        });
        ui.add_space(10.0);
        ui.collapsing("Measurement trend", |ui| {
            self.plot(ui);
        });
        ui.add_space(24.0);
        eyebrow(ui, "TIMELINE");
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for label in ["Baseline", "Intervention", "Sample", "Event"] {
                if ui.button(format!("+ {label}")).clicked() {
                    let r = self.recorder.as_mut().unwrap().marker(label, "", None);
                    self.report(r);
                }
            }
            if ui
                .add_enabled(self.note_anchor.is_none(), egui::Button::new("Add note…"))
                .clicked()
            {
                let r = self
                    .recorder
                    .as_mut()
                    .unwrap()
                    .marker("Note opened", "", None);
                self.note_anchor = self.report(r);
                self.note.clear();
            }
        });
        ui.add_space(12.0);
        if let Some(rec) = &self.recorder {
            timeline(ui, &rec.events);
        }
    }
    fn audio_panel(&mut self, ui: &mut egui::Ui) {
        let rec = self.recorder.as_ref().unwrap();
        let active = rec.mic.is_some();
        let now = rec.journal.elapsed();
        let (status, db) = self.waveform.status(now);
        let mut toggle = false;
        egui::Frame::new()
            .fill(Color32::from_rgb(23, 44, 40))
            .corner_radius(14)
            .inner_margin(22)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    status_dot(ui, if active { MINT } else { MUTED });
                    ui.label(
                        semibold(
                            if active {
                                "Microphone recording"
                            } else {
                                "Microphone paused"
                            },
                            14.0,
                        )
                        .color(Color32::WHITE),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    semibold(
                                        if active {
                                            "Pause microphone"
                                        } else {
                                            "Start microphone"
                                        },
                                        12.0,
                                    )
                                    .color(Color32::from_rgb(20, 46, 39)),
                                )
                                .fill(MINT)
                                .stroke(egui::Stroke::NONE),
                            )
                            .clicked()
                        {
                            toggle = true;
                        }
                    });
                });
                self.waveform.show(ui, now, active);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if active {
                            status
                        } else if !self.waveform.is_empty() {
                            "Audio saved · waveform paused"
                        } else {
                            "Audio starts only when you enable it"
                        })
                        .size(11.0)
                        .color(if status == "Input clipping" && active {
                            Color32::from_rgb(255, 178, 131)
                        } else {
                            Color32::from_rgb(166, 192, 181)
                        }),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let detail = if active {
                            db.map(|d| format!("{d:.0} dBFS  ·  Last 8 seconds"))
                                .unwrap_or_else(|| "Waiting for microphone samples".into())
                        } else if self.settings.microphone.is_empty() {
                            "Select an input in Session settings".into()
                        } else {
                            self.settings.microphone.clone()
                        };
                        ui.label(
                            RichText::new(detail)
                                .size(11.0)
                                .color(Color32::from_rgb(166, 192, 181)),
                        );
                    });
                });
            });
        if toggle {
            if !active && self.settings.microphone.is_empty() {
                self.open_settings(1);
                return;
            }
            if !active {
                self.waveform.clear();
            }
            let rec = self.recorder.as_mut().unwrap();
            let r = if active {
                rec.stop_mic()
            } else {
                rec.start_mic(&self.settings.microphone)
            };
            self.report(r);
        }
    }
    fn plot(&mut self, ui: &mut egui::Ui) {
        let rec = self.recorder.as_ref().unwrap();
        if rec.latest.is_empty() {
            return;
        }
        if !rec.latest.contains_key(&self.plot_key) {
            self.plot_key = rec.latest.keys().next().unwrap().clone();
        }
        egui::ComboBox::from_id_salt("plot_series")
            .selected_text(
                rec.latest
                    .get(&self.plot_key)
                    .map(|v| v.observation.label.as_str())
                    .unwrap_or("Series"),
            )
            .show_ui(ui, |ui| {
                for (key, v) in &rec.latest {
                    ui.selectable_value(
                        &mut self.plot_key,
                        key.clone(),
                        format!(
                            "{} · {} · {}",
                            v.observation.label, v.observation.units, v.observation.source
                        ),
                    );
                }
            });
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 140.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let end = rec.journal.elapsed().max(60.0);
        let start = end - 60.0;
        let points: Vec<_> = rec
            .history
            .iter()
            .filter(|(t, k, _)| *t >= start && *k == self.plot_key)
            .collect();
        let min = points
            .iter()
            .map(|(_, _, y)| *y)
            .fold(f64::INFINITY, f64::min);
        let max = points
            .iter()
            .map(|(_, _, y)| *y)
            .fold(f64::NEG_INFINITY, f64::max);
        for i in 0..4 {
            let y = rect.top() + i as f32 * rect.height() / 3.0;
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(1.0, LINE),
            );
        }
        for (t, _, y) in points {
            let px = rect.left() + ((*t - start) / 60.0) as f32 * rect.width();
            let py = rect.bottom()
                - 12.0
                - ((*y - min) / (max - min).max(1.0)) as f32 * (rect.height() - 24.0);
            painter.circle_filled(egui::pos2(px, py), 3.0, TEAL);
        }
        ui.label(
            RichText::new("Last 60 seconds · received observations · gaps are preserved")
                .size(12.0)
                .color(MUTED),
        );
    }
    fn review(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(session) = self.sessions.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        ui.add_space(20.0);
        eyebrow(ui, "LIBRARY  /  SAVED SESSION");
        ui.add_space(16.0);
        ui.label(
            RichText::new(format!(
                "{}  ·  {} segment(s)",
                session.subject,
                session.segments.len()
            ))
            .color(MUTED),
        );
        ui.label(semibold(&session.title, 30.0));
        ui.label(
            RichText::new(format!(
                "{}{}",
                session.state,
                if session.synthetic {
                    " · Synthetic demo"
                } else {
                    ""
                }
            ))
            .size(12.0)
            .color(TEAL),
        );
        ui.add_space(20.0);
        let dir = self.root.session_dir(id);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.job.is_none(), primary("Transcribe saved audio"))
                .clicked()
            {
                let root = self.root.path.clone();
                let folder = dir.clone();
                self.notice = "Loading downloaded Redux automatically…".into();
                self.job(move |tx| {
                    let n = model::transcribe_session(&root, &folder, |p| {
                        let _ = tx.send(Update::Progress(p));
                    })?;
                    Ok(format!("Saved {n} new transcript(s)."))
                });
            }
            if ui.button("Open session folder").clicked() {
                let r = open::that(&dir).map_err(Into::into);
                self.report(r);
            }
            if ui.add_enabled(self.job.is_none(), egui::Button::new("Inspect binary capture")).clicked() {
                let folder = dir.clone();
                let id = id.to_string();
                self.notice = "Inspecting saved binary records…".into();
                self.job(move |tx| {
                    let inspection = crate::waveline::inspect_session(&folder)?;
                    let count = inspection.records;
                    let _ = tx.send(Update::BinaryInspection(id, Box::new(inspection)));
                    Ok(format!("Inspected {count} experimental binary records. Field mappings remain unverified."))
                });
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (label, include_media) in
                [("Export data only", false), ("Export with all media", true)]
            {
                if ui
                    .add_enabled(self.job.is_none(), egui::Button::new(label))
                    .clicked()
                {
                    let dir = dir.clone();
                    let output = self.root.path.join("exports").join(format!(
                        "{}-{}.zip",
                        session.id,
                        store::id()
                    ));
                    self.notice = "Preparing export…".into();
                    self.job(move |_| {
                        export::archive(&dir, &output, include_media)?;
                        Ok(format!("Export saved: {}", output.display()))
                    });
                }
            }
        });
        if let Some(inspection) = &self.binary_review {
            ui.add_space(16.0);
            ui.add_sized(
                Vec2::new(230.0, 40.0),
                egui::Checkbox::new(&mut self.waveline_monitor_view, "Waveline monitor view"),
            );
            surface().show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some(record) = &inspection.last {
                    if self.waveline_monitor_view {
                        crate::waveline_monitor::show(ui, Some(record), None, None);
                    } else {
                        binary_panel(ui, record, inspection.records, None);
                    }
                } else {
                    ui.label("No matching Waveline binary records found in this capture.");
                }
                ui.label(
                    RichText::new(format!(
                        "{} unrecognized bytes · {} trailing bytes retained in originals",
                        inspection.discarded_bytes, inspection.trailing_bytes
                    ))
                    .size(11.0)
                    .color(MUTED),
                );
            });
        }
        if !model::installed(&self.root.path) {
            ui.horizontal(|ui| {
                ui.label("Download Redux once to enable offline transcription.");
                if ui.button("Model settings").clicked() {
                    self.settings_open = true;
                }
            });
        }
        ui.add_space(24.0);
        surface().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(semibold("Transcript", 18.0));
                ui.label(
                    RichText::new(format!("{} clips", self.transcripts.len()))
                        .size(12.0)
                        .color(MUTED),
                );
            });
            ui.add_space(12.0);
            if self.transcripts.is_empty() {
                ui.label(RichText::new("No transcript yet").color(MUTED));
                ui.label(
                    RichText::new(
                        "Use Transcribe saved audio to turn this session’s recordings into text.",
                    )
                    .size(12.0)
                    .color(MUTED),
                );
            }
            for t in &self.transcripts {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!(
                        "{}  ·  {}",
                        t.audio_started
                            .with_timezone(&chrono::Local)
                            .format("%H:%M:%S"),
                        t.engine
                    ))
                    .size(11.0)
                    .color(MUTED),
                );
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(RichText::new(&t.text).size(17.0))
                        .wrap()
                        .selectable(true),
                );
                if !t.timings_valid {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Some word timestamps exceed this clip. Original results are retained.",
                        )
                        .size(11.0)
                        .color(MUTED),
                    );
                }
                ui.add_space(12.0);
            }
        });
        ui.add_space(26.0);
        eyebrow(ui, "SESSION TIMELINE");
        ui.add_space(12.0);
        timeline(ui, &self.review);
        ui.add_space(14.0);
        ui.collapsing("Saved monitor observations", |ui| {
            ui.label(
                RichText::new("Latest 50 messages in this view; exports contain the full journal.")
                    .small()
                    .color(MUTED),
            );
            for e in self
                .review
                .iter()
                .rev()
                .filter(|e| e.kind == "message")
                .take(50)
            {
                if let Ok(message) =
                    serde_json::from_value::<crate::protocol::Message>(e.data["message"].clone())
                {
                    ui.label(
                        RichText::new(
                            e.utc
                                .with_timezone(&chrono::Local)
                                .format("%H:%M:%S")
                                .to_string(),
                        )
                        .monospace()
                        .color(MUTED),
                    );
                    for o in message.observations {
                        ui.label(format!(
                            "{}  {} {}  ·  {}",
                            o.label, o.value_text, o.units, o.source
                        ));
                    }
                }
            }
        });
        let mut files: Vec<_> = walkdir::WalkDir::new(dir.join("segments"))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && matches!(
                        e.path().extension().and_then(|x| x.to_str()),
                        Some("wav" | "mkv" | "jpg" | "png" | "jpeg")
                    )
            })
            .map(|e| e.path().to_path_buf())
            .collect();
        files.sort();
        ui.add_space(20.0);
        eyebrow(ui, "SAVED MEDIA");
        for (index, file) in files.into_iter().enumerate() {
            if ui
                .button(format!(
                    "Open {} {:02}",
                    if file.extension().and_then(|s| s.to_str()) == Some("wav") {
                        "audio clip"
                    } else {
                        "media file"
                    },
                    index + 1
                ))
                .clicked()
            {
                let r = open::that(file).map_err(Into::into);
                self.report(r);
            }
        }
        ui.add_space(24.0);
        ui.separator();
        ui.add_space(12.0);
        ui.collapsing("Continue this session", |ui| {
            ui.checkbox(
                &mut self.authorized,
                "Authorized to resume into a new segment",
            );
            if ui
                .add_enabled(
                    self.authorized && self.job.is_none(),
                    egui::Button::new("Resume session"),
                )
                .clicked()
            {
                self.start(Some(session));
            }
        });
    }
    fn settings(&mut self, ctx: &egui::Context) {
        let mut opened = self.settings_open;
        let mut saved = false;
        egui::Window::new("Settings").open(&mut opened).default_width(610.0).resizable(true).vscroll(true).show(ctx,|ui| {
            ui.horizontal(|ui| {
                for (index,label) in ["Connection","Audio","Transcription","Media"].iter().enumerate() {
                    ui.selectable_value(&mut self.settings_tab,index,*label);
                }
            });
            ui.add_space(16.0);ui.separator();ui.add_space(16.0);
            match self.settings_tab {
                0=>{
                    ui.label(semibold("Monitor connection",20.0));
                    ui.add_space(8.0);
                    ui.add_enabled_ui(self.recorder.is_none(),|ui| {
                        source_combo(ui,&mut self.settings.source.mode);
                        let c=&mut self.settings.source;
                        if matches!(c.mode,Source::TcpConnect|Source::TcpListen|Source::Udp){
                            ui.label("Monitor IP / allowed sender");ui.text_edit_singleline(&mut c.host);
                            if c.mode!=Source::TcpConnect {ui.label("Laptop bind address");ui.text_edit_singleline(&mut c.bind);ui.checkbox(&mut c.allow_any_peer,"Accept any sender on this interface");}
                            ui.horizontal(|ui|{ui.label("Port");ui.add(egui::DragValue::new(&mut c.port).range(1..=65535));});
                        }
                        if c.mode==Source::Serial {ui.label("Serial device (COM3 or /dev/ttyUSB0)");ui.text_edit_singleline(&mut c.serial_port);ui.horizontal(|ui|{ui.label("Baud · 8N1");ui.add(egui::DragValue::new(&mut c.baud).range(300..=921600));});}
                        if !matches!(c.mode,Source::Demo|Source::Notes){ui.checkbox(&mut c.hl7,"Decode generic HL7 v2 / MLLP");if c.hl7&&c.mode!=Source::Udp{ui.checkbox(&mut c.acknowledge,"Send original-mode ORU ACK after durable storage");}}
                    });
                    ui.add_space(16.0);
                    ui.collapsing("Waveline Pro reference",|ui|{ui.label("Software A.3.02 · Kernel K1-LPC3250-V1.0");ui.label("Module M1-ERTIC-V1.0 · Type 5A · Version V2.0.0");ui.label("Live export and protocol on this unit remain unverified. No automatic probing or monitor configuration is performed.");});
                },
                1=>{
                    ui.label(semibold("Microphone input",20.0));
                    ui.label(RichText::new("Choose the input used for recordings and the live waveform.").color(MUTED));
                    ui.add_space(16.0);
                    ui.add_enabled_ui(self.recorder.as_ref().is_none_or(|r|r.mic.is_none()),|ui| {
                        egui::ComboBox::from_id_salt("microphone").width(340.0).selected_text(if self.settings.microphone.is_empty(){"Choose a microphone"}else{&self.settings.microphone}).show_ui(ui,|ui| {
                            for d in &self.devices {ui.selectable_value(&mut self.settings.microphone,d.clone(),d);}
                        });
                    });
                    if ui.add_enabled(self.device_scan.is_none(),egui::Button::new("Refresh input devices")).clicked(){self.scan_devices();}
                    if self.device_scan.is_some() {ui.horizontal(|ui| {ui.spinner();ui.label("Finding audio inputs…");});}
                    ui.add_space(12.0);
                    ui.label(RichText::new("Audio starts paused in each segment. Pause the microphone to change inputs.").size(12.0).color(MUTED));
                },
                2=>{
                    ui.label(semibold("Offline transcription",20.0));
                    ui.label(RichText::new(model::engine_name()).color(MUTED));
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {status_dot(ui,TEAL);ui.label(if model::installed(&self.root.path){"Downloaded · ready for every session"}else{"Model not downloaded on this laptop"});});
                    if ui.add_enabled(self.recorder.is_none()&&self.job.is_none()&&!model::installed(&self.root.path),primary("Download Redux once")).clicked(){let root=self.root.path.clone();self.notice="Preparing Redux…".into();self.job(move|tx|{model::download(&root,|p|{let _=tx.send(Update::Progress(p));})?;Ok("Redux ready for every session. It loads automatically when you transcribe.".into())});}
                    ui.add_space(12.0);
                    ui.label(RichText::new("One download per laptop, shared by all sessions. Transcribe saved audio after stopping capture.").color(MUTED));
                },
                _=>{
                    ui.label(semibold("Video & photos",20.0));
                    ui.label("Optional video input through FFmpeg");ui.text_edit_singleline(&mut self.settings.video_input);
                    ui.label(RichText::new(if cfg!(target_os="macos"){"AVFoundation input, e.g. 0:none. Confirm the device index with FFmpeg."}else if cfg!(windows){"DirectShow input, e.g. video=USB Video. Use the exact device name."}else{"Video4Linux input, e.g. /dev/video0."}).small().color(MUTED));
                    if let Some(rec)=self.recorder.as_mut(){
                        if ui.button(if rec.video.is_some(){"Stop video"}else{"Start video"}).clicked(){
                            let r=if let Some(mut video)=rec.video.take(){video.stop().and_then(|info|rec.event("video_closed",info).map(|_|()))}else{
                                let fmt=if cfg!(target_os="macos"){"avfoundation"}else if cfg!(windows){"dshow"}else{"v4l2"};
                                media::Video::start(&rec.journal.dir,fmt,&self.settings.video_input,rec.journal.elapsed()).map(|v|rec.video=Some(v))
                            };self.report(r);
                        }
                    }
                    ui.add_space(20.0);ui.separator();ui.add_space(12.0);
                    ui.label("Attach an existing photo (absolute file path)");ui.text_edit_singleline(&mut self.photo_path);ui.add(egui::TextEdit::singleline(&mut self.photo_caption).hint_text("Photo caption"));
                    if ui.add_enabled(self.recorder.is_some(),egui::Button::new("Attach photo to active session")).clicked(){let rec=self.recorder.as_mut().unwrap();let r=media::attach_photo(&rec.journal.dir,&PathBuf::from(&self.photo_path),&self.photo_caption).and_then(|info|rec.event("photo",info));self.report(r);}
                }
            }
            ui.add_space(24.0);ui.separator();ui.add_space(12.0);
            if ui.add(primary("Save settings")).clicked(){let r=store::write_json(&self.root.path.join("settings.json"),&self.settings);if self.report(r).is_some(){self.notice="Settings saved.".into();saved=true;}}
        });
        self.settings_open = opened && !saved;
    }
}
impl eframe::App for Desktop {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick();
        ctx.request_repaint_after(Duration::from_millis(
            if self.recorder.as_ref().is_some_and(|r| r.mic.is_some()) {
                33
            } else {
                150
            },
        ));
        if ctx.input(|i| i.viewport().close_requested())
            && (self.recorder.is_some() || self.job.is_some())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_confirm = true;
        }
        self.sidebar(ctx);
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(PAPER)
                    .inner_margin(egui::Margin::symmetric(28, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if self.job.is_some() {
                        ui.spinner();
                    }
                    ui.add(
                        egui::Label::new(RichText::new(&self.notice).small().color(MUTED))
                            .truncate(),
                    )
                    .on_hover_text(&self.notice);
                    if let Ok(bytes) = fs2::available_space(&self.root.path) {
                        ui.label(
                            RichText::new(format!("{:.1} GB free", bytes as f64 / 1e9))
                                .small()
                                .color(MUTED),
                        );
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(PAPER)
                    .inner_margin(egui::Margin::symmetric(32, 24)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("workspace", self.selected.clone(), self.recorder.is_some()))
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        if self.recorder.is_some() {
                            self.live(ui);
                        } else if let Some(id) = self.selected.clone() {
                            self.review(ui, &id);
                        } else {
                            self.new_session(ui);
                        }
                    });
            });
        if self.settings_open {
            self.settings(ctx);
        }
        if self.note_anchor.is_some() {
            egui::Window::new("Add note")
                .collapsible(false)
                .default_width(480.0)
                .show(ctx, |ui| {
                    ui.label("The event was timestamped when you opened this note.");
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.note)
                            .desired_rows(6)
                            .desired_width(f32::INFINITY),
                    );
                    if self.note.is_empty() {
                        response.request_focus();
                    }
                    ui.horizontal(|ui| {
                        if ui.add(primary("Save note")).clicked() {
                            if let Some(rec) = self.recorder.as_mut() {
                                let r = rec.marker("Note", &self.note, self.note_anchor.as_deref());
                                if self.report(r).is_some() {
                                    self.note_anchor = None;
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.note_anchor = None;
                        }
                    });
                });
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new("Action needs attention")
                .collapsible(false)
                .default_width(520.0)
                .show(ctx, |ui| {
                    ui.label(error);
                    if ui.button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
        }
        if self.close_confirm {
            egui::Window::new("Close Pig Pal?")
                .collapsible(false)
                .show(ctx, |ui| {
                    if self.job.is_some() {
                        ui.label("Wait for the current transcription or export to finish.");
                    } else if ui.button("Stop, save & close").clicked() {
                        self.stop(false);
                        if self.error.is_none() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    if ui.button("Keep open").clicked() {
                        self.close_confirm = false;
                    }
                });
        }
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(mut rec) = self.recorder.take() {
            let _ = rec.stop(&self.root, true);
        }
    }
}
pub(crate) fn configure_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "Inter Semibold".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")).into(),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "Inter".into());
    fonts.families.insert(
        egui::FontFamily::Name("Semibold".into()),
        vec!["Inter Semibold".into()],
    );
    ctx.set_fonts(fonts);
    let mut v = egui::Visuals::light();
    v.panel_fill = PAPER;
    v.window_fill = Color32::WHITE;
    v.extreme_bg_color = Color32::WHITE;
    v.override_text_color = Some(INK);
    v.faint_bg_color = SOFT;
    v.selection.bg_fill = SOFT;
    v.selection.stroke = egui::Stroke::new(1.0, TEAL);
    v.window_corner_radius = 14.into();
    v.window_stroke = egui::Stroke::new(1.0, LINE);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, LINE);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = 8.into();
        w.bg_stroke = egui::Stroke::new(1.0, LINE);
        w.fg_stroke = egui::Stroke::new(1.0, INK);
        w.expansion = 0.0;
    }
    v.widgets.inactive.weak_bg_fill = Color32::WHITE;
    v.widgets.inactive.bg_fill = Color32::WHITE;
    v.widgets.hovered.weak_bg_fill = SOFT;
    v.widgets.hovered.bg_fill = SOFT;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, TEAL);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(218, 233, 224);
    v.widgets.active.bg_fill = SOFT;
    ctx.set_visuals(v);
    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = Vec2::new(12.0, 8.0);
    s.spacing.button_padding = Vec2::new(16.0, 12.0);
    s.spacing.interact_size.y = 24.0;
    s.spacing.combo_height = 280.0;
    s.spacing.window_margin = 24.into();
    s.animation_time = 0.12;
    s.text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    s.text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
    s.text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
    s.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(26.0, egui::FontFamily::Name("Semibold".into())),
    );
    ctx.set_style(s);
}
fn semibold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(egui::FontId::new(
        size,
        egui::FontFamily::Name("Semibold".into()),
    ))
}
fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(semibold(text, 10.0).color(MUTED));
}
fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(8.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(r.center(), 3.0, color);
}
fn badge(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(SOFT)
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(semibold(text, 9.0).color(TEAL));
        });
}
fn surface() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, LINE))
        .corner_radius(12)
        .inner_margin(24)
}
fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(semibold(text, 13.0).color(Color32::WHITE))
        .fill(TEAL)
        .stroke(egui::Stroke::NONE)
        .min_size(Vec2::new(0.0, 40.0))
}
fn source_combo(ui: &mut egui::Ui, source: &mut Source) {
    egui::ComboBox::from_id_salt(ui.id().with("source"))
        .selected_text(source.label())
        .show_ui(ui, |ui| {
            for item in Source::ALL {
                ui.selectable_value(source, item, item.label());
            }
        });
}
fn format_time(seconds: f64) -> String {
    let s = seconds as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}
pub(crate) fn arterial_panel(
    ui: &mut egui::Ui,
    record: Option<&crate::waveline::Record>,
    age: Option<f64>,
) {
    let stale = age.is_some_and(|age| age > crate::waveline::STALE_AFTER_S);
    let sample = record.and_then(|r| r.arterial_pressure.as_ref());
    let values = crate::waveline::ArterialPressure::cells(if stale { None } else { sample });
    let has_values = !stale && values.iter().take(3).any(|v| !v.is_empty());
    let status = if stale {
        "Not reading · stream stale"
    } else if has_values {
        if age.is_some() {
            "Receiving · candidate values"
        } else {
            "Saved arterial sample"
        }
    } else {
        "Not reading yet"
    };
    egui::Frame::new().fill(SOFT).corner_radius(10).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(semibold("Arterial line pressure (ART/P1)", 16.0));
            ui.label(RichText::new(status).size(12.0).strong().color(
                if has_values { TEAL } else { Color32::from_rgb(139, 91, 35) }
            ));
        });
        ui.add_space(12.0);
        ui.columns(3, |columns| {
            for (index, label) in ["Arterial systolic", "Arterial diastolic", "Arterial mean (MAP)"].iter().enumerate() {
                columns[index].label(RichText::new(*label).size(12.0).color(MUTED));
                columns[index].label(RichText::new(if values[index].is_empty() { "—" } else { &values[index] })
                    .monospace().size(25.0).color(if has_values { INK } else { MUTED }));
            }
        });
        ui.add_space(8.0);
        let detail = if sample.is_none() {
            "No decoded P1 sample available yet. Waiting for a new monitor record; older captures may have no P1 decoding."
        } else if stale {
            "No new complete monitor record for more than 3 seconds. Arterial values are hidden until new decoded samples arrive."
        } else if !has_values {
            "The latest decoded arterial sample has no available values. This section stays visible while waiting."
        } else {
            "P1 candidates · i32 LE bytes 36 / 40 / 44. Connected display reference supports these fields. Units and channel-validity flags remain unverified; fresh records can contain unavailable or retained device values."
        };
        ui.label(RichText::new(detail).size(11.0).color(MUTED));
    });
}

fn binary_panel(ui: &mut egui::Ui, record: &crate::waveline::Record, count: u64, age: Option<f64>) {
    ui.label(semibold("Experimental binary decoder", 18.0));
    ui.label(RichText::new("Candidate field meanings and units are unverified. These values are excluded from measurement trends and note context.").size(12.0).color(MUTED));
    ui.label(
        RichText::new(format!(
            "{count} complete records · latest record offset {}",
            record.stream_offset
        ))
        .size(11.0)
        .color(MUTED),
    );
    let freshness = age
        .map(|age| {
            if age > 3.0 {
                format!("STALE SNAPSHOT · last complete record {age:.1}s ago")
            } else {
                format!("Last complete record {age:.1}s ago")
            }
        })
        .unwrap_or_else(|| "Saved capture snapshot · not live data".into());
    ui.label(
        RichText::new(freshness)
            .size(12.0)
            .color(if age.is_some_and(|age| age > 3.0) {
                Color32::from_rgb(156, 90, 34)
            } else {
                MUTED
            }),
    );
    ui.add_space(12.0);
    arterial_panel(ui, Some(record), age);
    ui.add_space(16.0);
    let width = ((ui.available_width() - 32.0) / 3.0).max(100.0);
    egui::Grid::new("binary_candidates")
        .num_columns(3)
        .spacing([16.0, 12.0])
        .show(ui, |ui| {
            for (i, field) in record.fields.iter().enumerate() {
                ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.label(RichText::new(field.hypothesis).size(12.0).color(MUTED));
                    let value = field
                        .value
                        .map(|v| {
                            if field.encoding == "u16 LE" {
                                format!("{v:.0}")
                            } else {
                                format!("{v:.3}")
                            }
                        })
                        .unwrap_or_else(|| "Non-finite".into());
                    ui.label(RichText::new(value).monospace().size(23.0).color(INK))
                        .on_hover_text(format!("Original bytes: {}", field.raw_hex));
                    ui.label(
                        RichText::new(format!(
                            "{} · byte {} · raw value",
                            field.encoding, field.offset
                        ))
                        .monospace()
                        .size(10.0)
                        .color(MUTED),
                    );
                });
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
    ui.add_space(12.0);
    egui::CollapsingHeader::new("Raw signal slices · uncalibrated").default_open(true).show(ui,|ui| {
        ui.label(RichText::new("Latest record only · unsigned bytes 0–255 · sample rate and channel identities unverified").size(11.0).color(MUTED));
        for signal in &record.signal_slices {
            ui.label(RichText::new(format!("Byte {} · {} samples",signal.offset,signal.samples.len())).monospace().size(11.0).color(MUTED));
            let (rect,_) = ui.allocate_exact_size(egui::vec2(ui.available_width(),56.0),egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.line_segment([egui::pos2(rect.left(),rect.center().y),egui::pos2(rect.right(),rect.center().y)],egui::Stroke::new(1.0,LINE));
            let points = signal.samples.iter().enumerate().map(|(i,&v)| egui::pos2(rect.left()+i as f32/(signal.samples.len()-1).max(1) as f32*rect.width(),rect.bottom()-f32::from(v)/255.0*rect.height())).collect();
            painter.add(egui::Shape::line(points,egui::Stroke::new(1.5,TEAL)));
        }
    });
}
fn timeline(ui: &mut egui::Ui, events: &VecDeque<Event>) {
    let mut any = false;
    for e in events
        .iter()
        .rev()
        .filter(|e| {
            matches!(
                e.kind.as_str(),
                "marker"
                    | "annotation"
                    | "audio_closed"
                    | "photo"
                    | "start"
                    | "stop"
                    | "video_closed"
                    | "audio_error"
            )
        })
        .take(100)
    {
        any = true;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(
                    e.utc
                        .with_timezone(&chrono::Local)
                        .format("%H:%M:%S")
                        .to_string(),
                )
                .monospace()
                .size(11.0)
                .color(MUTED),
            );
            let friendly = match e.kind.as_str() {
                "audio_closed" => "Audio saved",
                "start" => "Session started",
                "stop" => "Session saved",
                "video_closed" => "Video saved",
                "audio_error" => "Audio interrupted",
                "photo" => "Photo attached",
                other => other,
            };
            let label = e.data["label"].as_str().unwrap_or(friendly);
            ui.label(semibold(label, 12.0));
            if let Some(note) = e.data["note"].as_str() {
                ui.label(note);
            }
            if e.kind == "audio_closed" {
                ui.label(format!(
                    "{:.1}s · {}",
                    e.data["duration_s"].as_f64().unwrap_or(0.0),
                    e.data["device"].as_str().unwrap_or("")
                ));
            }
            if let Some(detail) = e.data["detail"].as_str() {
                ui.label(detail);
            }
        });
    }
    if !any {
        ui.label(RichText::new("Markers and notes will appear here.").color(MUTED));
    }
}

#[cfg(test)]
mod arterial_ui_tests {
    use super::*;

    fn rendered(record: Option<&crate::waveline::Record>, age: Option<f64>) -> String {
        let context = egui::Context::default();
        configure_style(&context);
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| arterial_panel(ui, record, age));
        });
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn arterial_section_stays_visible_and_replaces_placeholders_only_with_available_samples() {
        let text = rendered(None, None);
        for label in [
            "Arterial line pressure (ART/P1)",
            "Arterial systolic",
            "Arterial diastolic",
            "Arterial mean (MAP)",
            "Not reading yet",
        ] {
            assert!(text.contains(label), "Missing persistent label: {label}");
        }
        assert_eq!(text.matches('—').count(), 3);
        assert!(text.contains("No decoded P1 sample"));
        let mut bytes = vec![0; crate::waveline::RECORD_LEN];
        bytes[..4].copy_from_slice(&crate::waveline::MAGIC);
        bytes[4..8].copy_from_slice(&(crate::waveline::RECORD_LEN as u32).to_le_bytes());
        let mut record = crate::waveline::Record::decode(&bytes, 0).unwrap();
        record.arterial_pressure = Some(crate::waveline::ArterialPressure {
            systolic: Some(123.0),
            diastolic: Some(67.0),
            mean: Some(84.0),
        });
        let fresh = rendered(Some(&record), Some(0.1));
        assert!(fresh.contains("Receiving"));
        for value in ["123", "67", "84"] {
            assert!(fresh.contains(value));
        }
        let stale = rendered(Some(&record), Some(4.0));
        assert!(stale.contains("stream stale"));
        assert_eq!(stale.matches('—').count(), 3);
        for value in ["123", "67", "84"] {
            assert!(!stale.contains(value));
        }
        record.arterial_pressure = Some(crate::waveline::ArterialPressure::default());
        let unavailable = rendered(Some(&record), Some(0.1));
        assert!(unavailable.contains("Not reading yet"));
        assert_eq!(unavailable.matches('—').count(), 3);
    }
}
