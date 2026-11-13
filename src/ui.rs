use crate::{
    audio, export, media, model,
    recorder::Recorder,
    store::{self, Event, Root, Session},
    theme::{self, *},
    transport::{Source, SourceConfig},
};
use anyhow::Result;
use eframe::egui::{
    self, Align2, Color32, Margin, Pos2, Rangef, Rect, Response, RichText, Sense, Shape, Stroke,
    Vec2,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex, MutexGuard,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// Keeps a capture running independently of the window.
///
/// The recorder normally sits in `slot`, where a dedicated thread polls it every
/// 20 ms: it reads the monitor connection, writes `wire.bin`, the journal and the
/// live CSV, and syncs to disk each second. While a frame is being drawn the UI
/// borrows the recorder out of the slot and returns it at the end of the frame.
/// When the window is hidden, minimized or occluded and no frames are drawn, the
/// background thread keeps capturing on its own.
struct BackgroundCapture {
    slot: Arc<Mutex<Option<Recorder>>>,
    stop: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
    /// Holds off App Nap and idle sleep for as long as the capture runs.
    _activity: Option<crate::platform::Activity>,
}
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
impl BackgroundCapture {
    fn spawn() -> Self {
        let slot: Arc<Mutex<Option<Recorder>>> = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let failure: Arc<Mutex<Option<String>>> = Arc::default();
        let (s, st, f) = (slot.clone(), stop.clone(), failure.clone());
        let thread = std::thread::Builder::new()
            .name("pigpal-capture".into())
            .spawn(move || {
                while !st.load(Ordering::Relaxed) {
                    if lock(&f).is_none() {
                        if let Some(rec) = lock(&s).as_mut() {
                            if let Err(e) = rec.poll() {
                                *lock(&f) = Some(format!("{e:#}"));
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            })
            .ok();
        Self {
            slot,
            stop,
            failure,
            thread,
            _activity: crate::platform::Activity::begin("Pig Pal is recording monitor data"),
        }
    }
    fn take(&self) -> Option<Recorder> {
        lock(&self.slot).take()
    }
    fn put(&self, rec: Recorder) {
        *lock(&self.slot) = Some(rec);
    }
    fn failure(&self) -> Option<String> {
        lock(&self.failure).take()
    }
    fn shutdown(mut self) -> Option<Recorder> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.take()
    }
}

const SIDEBAR_WIDTH: f32 = 260.0;
const RAIL_WIDTH: f32 = 300.0;
const MAX_REVIEW_MESSAGES: usize = 50;
/// Recording bookkeeping hidden from the review timeline unless asked for.
const LIFECYCLE_KINDS: &[&str] = &["start", "stop", "connection", "disconnect"];

#[derive(Default, Clone, Serialize, Deserialize)]
struct Settings {
    source: SourceConfig,
    microphone: String,
    video_input: String,
    /// Older settings files without this field open in System mode.
    #[serde(default)]
    appearance: Appearance,
}
enum Update {
    Progress(String),
    BinaryInspection(String, Box<crate::waveline::Inspection>),
    Replay(String, Box<crate::replay::Capture>, Option<String>),
    Done(std::result::Result<String, String>),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReviewTab {
    Timeline,
    Transcript,
    Media,
    Data,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum MonitorView {
    Waveline,
    Decoder,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Connection,
    Audio,
    Transcription,
    Media,
}
impl SettingsTab {
    const ALL: [(Self, Icon, &'static str); 5] = [
        (Self::General, Icon::Settings, "General"),
        (Self::Connection, Icon::Wave, "Monitor"),
        (Self::Audio, Icon::Mic, "Audio"),
        (Self::Transcription, Icon::Text, "Transcription"),
        (Self::Media, Icon::Folder, "Video & photos"),
    ];
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Info,
    Warn,
}
struct Toast {
    text: String,
    kind: ToastKind,
    until: Instant,
}

pub struct Desktop {
    root: Root,
    sessions: Vec<Session>,
    selected: Option<String>,
    /// The active recorder while the UI holds it during a frame (see BackgroundCapture).
    recorder: Option<Recorder>,
    capture: Option<BackgroundCapture>,
    settings: Settings,
    /// Settings as they were when the dialog opened, restored on Cancel.
    settings_saved: Settings,
    settings_open: bool,
    settings_tab: SettingsTab,
    subject: String,
    title: String,
    authorized: bool,
    error: Option<String>,
    toasts: Vec<Toast>,
    /// Latest progress text from the running background job.
    progress: Option<String>,
    devices_scanned: bool,
    review: Vec<Event>,
    review_messages: VecDeque<Event>,
    review_tab: ReviewTab,
    media_files: Vec<PathBuf>,
    transcripts: Vec<model::Transcript>,
    job: Option<Receiver<Update>>,
    note: String,
    note_anchor: Option<String>,
    note_opened_at: f64,
    devices: Vec<String>,
    device_scan: Option<Receiver<std::result::Result<Vec<String>, String>>>,
    search: String,
    plot_key: String,
    photo_path: String,
    photo_caption: String,
    close_confirm: bool,
    resume_prompt: bool,
    timeline_all: bool,
    waveform: crate::waveform::Waveform,
    binary_review: Option<crate::waveline::Inspection>,
    monitor_view: MonitorView,
    replay: Option<crate::replay_ui::Viewer>,
    disk_free: Option<u64>,
    disk_checked: Option<Instant>,
    model_ready: bool,
}
impl Desktop {
    pub fn new(cc: &eframe::CreationContext<'_>, root: Root) -> Self {
        configure_style(&cc.egui_ctx);
        let settings: Settings =
            store::read_json(&root.path.join("settings.json")).unwrap_or_default();
        theme::set_appearance(&cc.egui_ctx, settings.appearance);
        let sessions = root.sessions().unwrap_or_default();
        let model_ready = model::installed(&root.path);
        Self {
            root,
            sessions,
            selected: None,
            recorder: None,
            capture: None,
            settings_saved: settings.clone(),
            settings,
            settings_open: false,
            settings_tab: SettingsTab::Connection,
            subject: String::new(),
            title: String::new(),
            authorized: false,
            error: None,
            toasts: Vec::new(),
            progress: None,
            devices_scanned: false,
            review: Vec::new(),
            review_messages: VecDeque::new(),
            review_tab: ReviewTab::Timeline,
            media_files: Vec::new(),
            transcripts: vec![],
            job: None,
            note: String::new(),
            note_anchor: None,
            note_opened_at: 0.0,
            devices: vec![],
            device_scan: None,
            search: String::new(),
            plot_key: String::new(),
            photo_path: String::new(),
            photo_caption: String::new(),
            close_confirm: false,
            resume_prompt: false,
            timeline_all: false,
            waveform: Default::default(),
            binary_review: None,
            monitor_view: MonitorView::Waveline,
            replay: None,
            disk_free: None,
            disk_checked: None,
            model_ready,
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
    fn busy(&self) -> bool {
        self.recorder.is_some() || self.job.is_some()
    }
    fn select(&mut self, id: String) {
        if self.selected.as_deref() != Some(&id) {
            self.binary_review = None;
            self.replay = None;
            self.resume_prompt = false;
            self.review_tab = ReviewTab::Timeline;
        }
        self.selected = Some(id.clone());
        let dir = self.root.session_dir(&id);
        match store::review_events(&dir, MAX_REVIEW_MESSAGES) {
            Ok(events) => {
                self.review = events.timeline;
                self.review_messages = events.messages;
                if !events.warnings.is_empty() {
                    self.toast_warn(events.warnings.join(" "));
                }
            }
            Err(e) => self.error = Some(format!("{e:#}")),
        }
        let r = model::transcripts(&dir);
        if let Some(t) = self.report(r) {
            self.transcripts = t;
        }
        // Cached once per selection instead of walking the folder every frame.
        self.media_files = walkdir::WalkDir::new(dir.join("segments"))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && matches!(
                        e.path()
                            .extension()
                            .and_then(|x| x.to_str())
                            .map(str::to_ascii_lowercase)
                            .as_deref(),
                        Some("wav" | "mkv" | "jpg" | "png" | "jpeg")
                    )
            })
            .map(|e| e.path().to_path_buf())
            .collect();
        self.media_files.sort();
        self.model_ready = model::installed(&self.root.path);
    }
    pub fn open_replay(&mut self, id: String, at: Option<String>) {
        if self.busy() {
            return;
        }
        self.select(id.clone());
        let folder = self.root.session_dir(&id);
        self.progress = Some("Loading the replay…".into());
        self.job(move |tx| {
            let capture = crate::replay::Capture::load(&folder)?;
            let count = capture.frames.len();
            let _ = tx.send(Update::Replay(id, Box::new(capture), at));
            Ok(format!(
                "Replay ready · {count} raw records decoded · original files unchanged"
            ))
        });
    }
    fn start(&mut self, resume: Option<Session>) {
        self.replay = None;
        self.resume_prompt = false;
        let mut config = self.settings.source.clone();
        let session = if let Some(s) = resume {
            if s.synthetic {
                config.mode = Source::Demo;
            }
            s
        } else {
            let r = self.root.create(
                self.subject.trim(),
                self.title.trim(),
                config.mode == Source::Demo,
            );
            let Some(s) = self.report(r) else {
                return;
            };
            s
        };
        let r = Recorder::start(&self.root, session, config);
        if let Some(rec) = self.report(r) {
            self.selected = Some(rec.session.id.clone());
            self.recorder = Some(rec);
            self.capture = Some(BackgroundCapture::spawn());
            self.waveform.clear();
            self.authorized = false;
            self.note_anchor = None;
            self.toast("Recording started");
            self.refresh();
        }
    }
    /// Bring the recorder into the UI for this frame and surface background errors.
    fn reclaim(&mut self) {
        let Some(capture) = &self.capture else {
            return;
        };
        if self.recorder.is_none() {
            self.recorder = capture.take();
        }
        if let Some(error) = capture.failure() {
            self.error = Some(format!("Capture interrupted: {error}"));
            self.stop(true);
        }
    }
    /// Hand the recorder back to the background thread at the end of a frame.
    fn park(&mut self) {
        if let (Some(capture), Some(rec)) = (&self.capture, self.recorder.take()) {
            capture.put(rec);
        }
    }
    fn stop(&mut self, interrupted: bool) {
        if let Some(capture) = self.capture.take() {
            if let Some(rec) = capture.shutdown() {
                self.recorder = Some(rec);
            }
        }
        if let Some(mut rec) = self.recorder.take() {
            let id = rec.session.id.clone();
            let r = rec.stop(&self.root, interrupted);
            self.report(r);
            self.note_anchor = None;
            self.refresh();
            self.select(id);
            self.toast("Recording saved");
            self.subject.clear();
            self.title.clear();
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
        if let Some(replay) = &mut self.replay {
            replay.tick();
        }
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
                Some(Update::Progress(p)) => self.progress = Some(p),
                Some(Update::BinaryInspection(id, inspection)) => {
                    if self.selected.as_deref() == Some(&id) {
                        self.binary_review = Some(*inspection);
                        self.review_tab = ReviewTab::Data;
                    }
                }
                Some(Update::Replay(id, capture, at)) => {
                    if self.selected.as_deref() == Some(&id) {
                        self.replay = Some(crate::replay_ui::Viewer::new(
                            *capture,
                            at.as_deref(),
                            replay_notes(&self.review),
                        ));
                    }
                }
                Some(Update::Done(result)) => {
                    self.job = None;
                    self.progress = None;
                    match result {
                        Ok(msg) => self.toast(msg),
                        Err(e) => self.error = Some(e),
                    };
                    if let Some(id) = self.selected.clone() {
                        if self.recorder.is_none() {
                            self.select(id);
                        }
                    }
                    self.model_ready = model::installed(&self.root.path);
                    self.disk_checked = None;
                    break;
                }
                None => break,
            }
        }
        // Free space changes slowly; check at most every five seconds.
        if self
            .disk_checked
            .is_none_or(|t| t.elapsed() > Duration::from_secs(5))
        {
            self.disk_free = fs2::available_space(&self.root.path).ok();
            self.disk_checked = Some(Instant::now());
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
    fn open_settings(&mut self, tab: SettingsTab) {
        self.settings_tab = tab;
        if !self.settings_open {
            self.settings_saved = self.settings.clone();
        }
        self.settings_open = true;
        self.scan_devices();
    }
    fn new_session_view(&mut self) {
        self.selected = None;
        self.replay = None;
        self.authorized = false;
        self.resume_prompt = false;
    }

    fn inspect_binary(&mut self, id: &str, dir: &std::path::Path) {
        let folder = dir.to_path_buf();
        let id = id.to_string();
        self.progress = Some("Inspecting saved binary records…".into());
        self.job(move |tx| {
            let inspection = crate::waveline::inspect_session(&folder)?;
            let count = inspection.records;
            let _ = tx.send(Update::BinaryInspection(id, Box::new(inspection)));
            Ok(format!(
                "Inspected {count} experimental binary records. Field mappings remain unverified."
            ))
        });
    }

    // -----------------------------------------------------------------------
    // Notifications and settings persistence

    fn toast(&mut self, text: impl Into<String>) {
        self.push_toast(text.into(), ToastKind::Info);
    }
    fn toast_warn(&mut self, text: impl Into<String>) {
        self.push_toast(text.into(), ToastKind::Warn);
    }
    fn push_toast(&mut self, text: String, kind: ToastKind) {
        self.toasts.retain(|t| t.text != text);
        self.toasts.push(Toast {
            text,
            kind,
            until: Instant::now()
                + Duration::from_secs(if kind == ToastKind::Warn { 8 } else { 4 }),
        });
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
    }
    /// Save choices made outside the Settings dialog (appearance, quick pickers)
    /// without committing anything still being edited inside it.
    fn persist(&mut self, edit: impl Fn(&mut Settings)) {
        edit(&mut self.settings);
        edit(&mut self.settings_saved);
        let r = store::write_json(&self.root.path.join("settings.json"), &self.settings_saved);
        self.report(r);
    }
    fn set_appearance(&mut self, ctx: &egui::Context, appearance: Appearance) {
        theme::set_appearance(ctx, appearance);
        self.persist(|s| s.appearance = appearance);
    }

    // -----------------------------------------------------------------------
    // Sidebar

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("library")
            .resizable(false)
            .exact_width(SIDEBAR_WIDTH)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(sidebar())
                    .inner_margin(Margin::symmetric(12, 16)),
            )
            .show(ctx, |ui| {
                let edge = ui.max_rect().right() + 12.0;
                ui.ctx().layer_painter(egui::LayerId::background()).vline(
                    edge,
                    ui.ctx().screen_rect().y_range(),
                    Stroke::new(1.0, line()),
                );
                ui.spacing_mut().item_spacing.y = 0.0;

                // Brand
                let (row, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
                let mark = Rect::from_min_size(
                    Pos2::new(row.left() + 4.0, row.top() + 1.0),
                    Vec2::splat(28.0),
                );
                let painter = ui.painter();
                painter.rect_filled(mark, 8, accent());
                paint_icon(painter, mark.shrink(5.0), Icon::Wave, on_accent());
                let f = bold(15.0);
                let b = centered_baseline(painter, &f, mark.center().y);
                paint_on_baseline(
                    painter,
                    painter.layout_no_wrap("Pig Pal".into(), f, text()),
                    mark.right() + 10.0,
                    b,
                );
                ui.add_space(20.0);

                // Primary action or the live recording card
                if let Some(rec) = &self.recorder {
                    parts::recording_card(ui, rec);
                } else if Btn::primary("New recording")
                    .icon(Icon::Plus)
                    .width(ui.available_width())
                    .enabled(self.job.is_none())
                    .show(ui)
                    .clicked()
                {
                    self.new_session_view();
                }
                ui.add_space(12.0);
                text_field(
                    ui,
                    "session_search",
                    &mut self.search,
                    "Search sessions",
                    Some(Icon::Search),
                );
                ui.add_space(20.0);

                let needle = self.search.trim().to_lowercase();
                let matches: Vec<usize> = self
                    .sessions
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| {
                        needle.is_empty()
                            || s.subject.to_lowercase().contains(&needle)
                            || s.title.to_lowercase().contains(&needle)
                    })
                    .map(|(i, _)| i)
                    .collect();
                parts::list_heading(ui, "Sessions", matches.len());
                ui.add_space(4.0);

                let footer = 52.0;
                let mut selection = None;
                let enabled = !self.busy();
                egui::ScrollArea::vertical()
                    .id_salt("sessions")
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - footer).max(60.0))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for &i in &matches {
                            let s = &self.sessions[i];
                            let is_selected = self.selected.as_ref() == Some(&s.id);
                            if parts::session_row(ui, s, is_selected, enabled).clicked() {
                                selection = Some(s.id.clone());
                            }
                        }
                        if matches.is_empty() {
                            ui.add_space(8.0);
                            ui.label(muted_text(
                                if self.sessions.is_empty() {
                                    "Recordings you save appear here."
                                } else {
                                    "No sessions match."
                                },
                                12.5,
                            ));
                        }
                    });
                if let Some(id) = selection {
                    self.select(id);
                }

                // Footer: storage on the left, utilities on the right.
                let rect = Rect::from_min_max(
                    Pos2::new(ui.max_rect().left(), ui.max_rect().bottom() - 40.0),
                    ui.max_rect().right_bottom(),
                );
                ui.painter().hline(
                    rect.x_range().expand(12.0),
                    rect.top() - 8.0,
                    Stroke::new(1.0, line()),
                );
                let mut appearance = self.settings.appearance;
                ui.allocate_new_ui(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if let Some(bytes) = self.disk_free {
                            ui.add_space(4.0);
                            ui.label(muted_text(format!("{} free", format_bytes(bytes)), 12.0))
                                .on_hover_text(
                                    "Free space on the drive that holds your recordings",
                                );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if icon_button(ui, Icon::Settings, "Settings").clicked() {
                                self.open_settings(SettingsTab::General);
                            }
                            if icon_button(ui, Icon::Folder, "Open data folder").clicked() {
                                let r = open::that(&self.root.path).map_err(Into::into);
                                self.report(r);
                            }
                            let theme_button = icon_button(
                                ui,
                                self.settings.appearance.icon(),
                                &format!("Appearance: {}", self.settings.appearance.label()),
                            );
                            menu_above(ui, "appearance_menu", &theme_button, 190.0, |ui| {
                                for option in Appearance::ALL {
                                    if menu_item(
                                        ui,
                                        Some(option.icon()),
                                        option.label(),
                                        None,
                                        appearance == option,
                                    )
                                    .clicked()
                                    {
                                        appearance = option;
                                    }
                                }
                            });
                        });
                    },
                );
                if appearance != self.settings.appearance {
                    self.set_appearance(ui.ctx(), appearance);
                }
            });
    }

    // -----------------------------------------------------------------------
    // Dialogs

    fn settings(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut save = false;
        let response = egui::Modal::new(egui::Id::new("settings"))
            .frame(modal_frame().inner_margin(Margin::ZERO))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let size = Vec2::new(760.0, 540.0);
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                let nav = Rect::from_min_size(rect.min, Vec2::new(200.0, size.y));
                ui.painter().rect_filled(
                    nav,
                    egui::CornerRadius {
                        nw: 14,
                        sw: 14,
                        ne: 0,
                        se: 0,
                    },
                    sidebar(),
                );
                ui.painter()
                    .vline(nav.right(), nav.y_range(), Stroke::new(1.0, line()));

                // Navigation
                ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(nav.shrink2(Vec2::new(12.0, 20.0))),
                    |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.label(semibold("Settings", 16.0));
                        });
                        ui.add_space(14.0);
                        for (tab, icon, label) in SettingsTab::ALL {
                            if parts::nav_item(ui, icon, label, self.settings_tab == tab).clicked()
                            {
                                self.settings_tab = tab;
                            }
                        }
                    },
                );

                // Content
                let content = Rect::from_min_max(
                    Pos2::new(nav.right() + 28.0, rect.top() + 24.0),
                    Pos2::new(rect.right() - 28.0, rect.bottom() - 76.0),
                );
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content), |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    egui::ScrollArea::vertical()
                        .id_salt("settings_content")
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.settings_content(ui));
                });

                // Footer
                let footer =
                    Rect::from_min_max(Pos2::new(nav.right(), rect.bottom() - 64.0), rect.max);
                ui.painter()
                    .hline(footer.x_range(), footer.top(), Stroke::new(1.0, line()));
                ui.allocate_new_ui(
                    egui::UiBuilder::new()
                        .max_rect(footer.shrink2(Vec2::new(24.0, 16.0)))
                        .layout(egui::Layout::right_to_left(egui::Align::Center)),
                    |ui| {
                        save = Btn::primary("Save changes").show(ui).clicked();
                        close = Btn::ghost("Cancel").show(ui).clicked();
                    },
                );
            });
        if save {
            let r = store::write_json(&self.root.path.join("settings.json"), &self.settings);
            if self.report(r).is_some() {
                self.settings_saved = self.settings.clone();
                self.toast("Settings saved");
                self.settings_open = false;
            }
        } else if close || response.should_close() {
            let appearance = self.settings_saved.appearance;
            self.settings = self.settings_saved.clone();
            theme::set_appearance(ctx, appearance);
            self.settings_open = false;
        }
    }
    fn settings_content(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        match self.settings_tab {
            SettingsTab::General => {
                parts::dialog_heading(ui, "General", "How Pig Pal looks on this laptop.");
                field_label(ui, "Appearance");
                let mut appearance = self.settings.appearance;
                select(ui, "appearance_select", appearance.label(), 240.0, |ui| {
                    for option in Appearance::ALL {
                        if menu_item(
                            ui,
                            Some(option.icon()),
                            option.label(),
                            None,
                            appearance == option,
                        )
                        .clicked()
                        {
                            appearance = option;
                        }
                    }
                });
                if appearance != self.settings.appearance {
                    self.settings.appearance = appearance;
                    theme::set_appearance(ui.ctx(), appearance);
                }
                ui.label(muted_text("Match system follows macOS. The monitor view stays dark in every mode, like the device.", 12.0));
                ui.add_space(16.0);
                field_label(ui, "Data folder");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            mono(self.root.path.display().to_string(), 12.0).color(text_2()),
                        )
                        .truncate(),
                    );
                });
                if Btn::secondary("Open in Finder")
                    .icon(Icon::Folder)
                    .size(Size::Sm)
                    .show(ui)
                    .clicked()
                {
                    let r = open::that(&self.root.path).map_err(Into::into);
                    self.report(r);
                }
            }
            SettingsTab::Connection => {
                parts::dialog_heading(
                    ui,
                    "Monitor",
                    "How Pig Pal receives data from the Waveline Pro. Locked while recording.",
                );
                ui.add_enabled_ui(self.recorder.is_none(), |ui| {
                    field_label(ui, "Input");
                    parts::source_select(
                        ui,
                        "settings_source",
                        &mut self.settings.source.mode,
                        width,
                    );
                    ui.add_space(8.0);
                    let c = &mut self.settings.source;
                    if matches!(c.mode, Source::TcpConnect | Source::TcpListen | Source::Udp) {
                        split_row(
                            ui,
                            120.0,
                            |ui| {
                                field_label(ui, "Monitor IP / allowed sender");
                                text_field(ui, "settings_host", &mut c.host, "10.0.0.3", None);
                            },
                            |ui| {
                                field_label(ui, "Port");
                                let mut port = c.port.to_string();
                                if text_field(ui, "settings_port", &mut port, "12345", None)
                                    .changed()
                                {
                                    if let Ok(p) = port.trim().parse::<u16>() {
                                        if p > 0 {
                                            c.port = p;
                                        }
                                    }
                                }
                            },
                        );
                        if c.mode != Source::TcpConnect {
                            ui.add_space(8.0);
                            field_label(ui, "Laptop address to listen on");
                            text_field(ui, "settings_bind", &mut c.bind, "10.0.0.2", None);
                            ui.add_space(8.0);
                            checkbox(
                                ui,
                                &mut c.allow_any_peer,
                                "Accept any sender on this interface",
                            );
                        }
                    }
                    if c.mode == Source::Serial {
                        split_row(
                            ui,
                            120.0,
                            |ui| {
                                field_label(ui, "Serial device");
                                text_field(
                                    ui,
                                    "settings_serial",
                                    &mut c.serial_port,
                                    "/dev/ttyUSB0",
                                    None,
                                );
                            },
                            |ui| {
                                field_label(ui, "Baud (8N1)");
                                let mut baud = c.baud.to_string();
                                if text_field(ui, "settings_baud", &mut baud, "9600", None)
                                    .changed()
                                {
                                    if let Ok(b) = baud.trim().parse::<u32>() {
                                        c.baud = b.clamp(300, 921_600);
                                    }
                                }
                            },
                        );
                    }
                    if !matches!(c.mode, Source::Demo | Source::Notes) {
                        ui.add_space(8.0);
                        checkbox(ui, &mut c.hl7, "Decode generic HL7 v2 / MLLP")
                            .on_hover_text("Leave off for the Waveline Pro binary stream");
                        if c.hl7 && c.mode != Source::Udp {
                            ui.add_space(4.0);
                            checkbox(
                                ui,
                                &mut c.acknowledge,
                                "Send ORU acknowledgements after saving",
                            );
                        }
                    }
                });
                ui.add_space(12.0);
                parts::note_box(ui, &source_summary(&self.settings.source));
                ui.add_space(12.0);
                ui.label(muted_text(
                    "Tested setup: the monitor at 10.0.0.3 connects by TCP to this laptop at 10.0.0.2:12345 (software A.3.02). Pig Pal never probes or reconfigures the monitor.",
                    12.0,
                ));
            }
            SettingsTab::Audio => {
                parts::dialog_heading(ui, "Audio", "Microphone used for recordings and the level meter. Audio always starts paused.");
                ui.add_enabled_ui(
                    self.recorder.as_ref().is_none_or(|r| r.mic.is_none()),
                    |ui| {
                        field_label(ui, "Microphone");
                        self.microphone_select(ui, "settings_mic", width);
                    },
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if Btn::secondary("Refresh devices")
                        .size(Size::Sm)
                        .enabled(self.device_scan.is_none())
                        .show(ui)
                        .clicked()
                    {
                        self.scan_devices();
                    }
                    if self.device_scan.is_some() {
                        ui.add(egui::Spinner::new().size(14.0).color(muted()));
                        ui.label(muted_text("Looking for inputs…", 12.0));
                    }
                });
                ui.label(muted_text(
                    "Pause the microphone to switch inputs during a recording.",
                    12.0,
                ));
            }
            SettingsTab::Transcription => {
                parts::dialog_heading(ui, "Transcription", model::engine_name());
                parts::status_line(
                    ui,
                    if self.model_ready { live() } else { warn() },
                    if self.model_ready {
                        "Model installed. Ready for every session."
                    } else {
                        "Model not installed on this laptop."
                    },
                );
                ui.add_space(8.0);
                if !self.model_ready
                    && Btn::primary("Download model")
                        .icon(Icon::Export)
                        .enabled(self.recorder.is_none() && self.job.is_none())
                        .show(ui)
                        .clicked()
                {
                    let root = self.root.path.clone();
                    self.progress = Some("Preparing the transcription model…".into());
                    self.job(move |tx| {
                        model::download(&root, |p| {
                            let _ = tx.send(Update::Progress(p));
                        })?;
                        Ok("Transcription model installed".into())
                    });
                }
                ui.label(muted_text("A one-time download shared by all sessions. Transcription runs offline after a recording is saved.", 12.0));
            }
            SettingsTab::Media => {
                parts::dialog_heading(
                    ui,
                    "Video & photos",
                    "Optional extras attached to the active recording.",
                );
                field_label(ui, "Video input (FFmpeg)");
                text_field(
                    ui,
                    "settings_video",
                    &mut self.settings.video_input,
                    if cfg!(target_os = "macos") {
                        "0:none"
                    } else {
                        "/dev/video0"
                    },
                    None,
                );
                ui.label(muted_text(
                    if cfg!(target_os = "macos") {
                        "AVFoundation device index, e.g. 0:none. Confirm it with FFmpeg."
                    } else if cfg!(windows) {
                        "DirectShow device, e.g. video=USB Video."
                    } else {
                        "Video4Linux device, e.g. /dev/video0."
                    },
                    12.0,
                ));
                if let Some(rec) = self.recorder.as_mut() {
                    let running = rec.video.is_some();
                    if Btn::secondary(if running { "Stop video" } else { "Start video" })
                        .size(Size::Sm)
                        .show(ui)
                        .clicked()
                    {
                        let r = if let Some(mut video) = rec.video.take() {
                            video
                                .stop()
                                .and_then(|info| rec.event("video_closed", info).map(|_| ()))
                        } else {
                            let fmt = if cfg!(target_os = "macos") {
                                "avfoundation"
                            } else if cfg!(windows) {
                                "dshow"
                            } else {
                                "v4l2"
                            };
                            media::Video::start(
                                &rec.journal.dir,
                                fmt,
                                &self.settings.video_input,
                                rec.journal.elapsed(),
                            )
                            .map(|v| rec.video = Some(v))
                        };
                        self.report(r);
                    }
                }
                ui.add_space(16.0);
                field_label(ui, "Attach a photo");
                text_field(
                    ui,
                    "settings_photo",
                    &mut self.photo_path,
                    "Absolute path, e.g. /Users/…/photo.jpg",
                    None,
                );
                ui.add_space(4.0);
                text_field(
                    ui,
                    "settings_caption",
                    &mut self.photo_caption,
                    "Caption",
                    None,
                );
                ui.add_space(4.0);
                let attach = Btn::secondary("Attach to recording")
                    .size(Size::Sm)
                    .enabled(self.recorder.is_some() && !self.photo_path.trim().is_empty())
                    .show(ui);
                if self.recorder.is_none() {
                    ui.label(muted_text("Available while a recording is running.", 12.0));
                }
                if attach.clicked() {
                    let rec = self.recorder.as_mut().unwrap();
                    let r = media::attach_photo(
                        &rec.journal.dir,
                        &PathBuf::from(&self.photo_path),
                        &self.photo_caption,
                    )
                    .and_then(|info| rec.event("photo", info));
                    if self.report(r).is_some() {
                        self.toast("Photo attached");
                        self.photo_path.clear();
                        self.photo_caption.clear();
                    }
                }
            }
        }
    }
    /// Microphone picker. Choosing an input saves it immediately.
    fn microphone_select(&mut self, ui: &mut egui::Ui, id: &str, width: f32) {
        let current = if self.settings.microphone.is_empty() {
            "Choose a microphone".to_string()
        } else {
            self.settings.microphone.clone()
        };
        let mut chosen = None;
        let response = select(ui, id, &current, width, |ui| {
            if self.devices.is_empty() {
                ui.label(muted_text(
                    if self.device_scan.is_some() {
                        "Looking for inputs…"
                    } else {
                        "No inputs found"
                    },
                    12.5,
                ));
            }
            for d in &self.devices {
                if menu_item(ui, Some(Icon::Mic), d, None, *d == self.settings.microphone).clicked()
                {
                    chosen = Some(d.clone());
                }
            }
        });
        if response.clicked() && self.devices.is_empty() {
            self.scan_devices();
        }
        if let Some(device) = chosen {
            self.persist(|s| s.microphone = device.clone());
        }
    }
    fn note_dialog(&mut self, ctx: &egui::Context) {
        let mut save = false;
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::new("note"))
            .frame(modal_frame())
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(480.0);
                parts::dialog_heading(
                    ui,
                    "Add note",
                    &format!(
                        "Saved at {}, when you opened this note.",
                        format_time(self.note_opened_at)
                    ),
                );
                let response = ui.add(
                    egui::TextEdit::multiline(&mut self.note)
                        .margin(Margin::same(12))
                        .desired_rows(5)
                        .desired_width(f32::INFINITY)
                        .hint_text(RichText::new("What happened?").color(muted())),
                );
                if self.note.is_empty() {
                    response.request_focus();
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.label(muted_text("⌘ Return to save", 12.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        save |= Btn::primary("Save note").show(ui).clicked();
                        cancel |= Btn::ghost("Cancel").show(ui).clicked();
                    });
                });
                save |= ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
            });
        if save {
            if let Some(rec) = self.recorder.as_mut() {
                let r = rec.marker("Note", &self.note, self.note_anchor.as_deref());
                if self.report(r).is_some() {
                    self.note_anchor = None;
                    self.toast("Note saved");
                }
            } else {
                self.note_anchor = None;
            }
        } else if cancel || response.should_close() {
            self.note_anchor = None;
        }
    }
    fn resume_dialog(&mut self, ctx: &egui::Context) {
        let Some(session) = self
            .selected
            .as_ref()
            .and_then(|id| self.sessions.iter().find(|s| &s.id == id))
            .cloned()
        else {
            self.resume_prompt = false;
            return;
        };
        let mut resume = false;
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::new("resume"))
            .frame(modal_frame())
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(440.0);
                parts::dialog_heading(
                    ui,
                    "Resume recording",
                    &format!("Recording continues as a new segment of “{}”. Earlier segments are not changed.", session.title),
                );
                checkbox(ui, &mut self.authorized, "I am authorized to resume this recording");
                ui.add_space(20.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    resume = Btn::primary("Resume recording")
                        .enabled(self.authorized && self.job.is_none())
                        .show(ui)
                        .clicked();
                    cancel = Btn::ghost("Cancel").show(ui).clicked();
                });
            });
        if resume {
            self.start(Some(session));
        } else if cancel || response.should_close() {
            self.resume_prompt = false;
        }
    }
    fn error_dialog(&mut self, ctx: &egui::Context, error: String) {
        let mut dismiss = false;
        let response = egui::Modal::new(egui::Id::new("error"))
            .frame(modal_frame())
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(460.0);
                parts::dialog_heading(ui, "Something needs attention", "");
                ui.add(
                    egui::Label::new(secondary(error, 13.5))
                        .wrap()
                        .selectable(true),
                );
                ui.add_space(20.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    dismiss = Btn::primary("OK").width(72.0).show(ui).clicked();
                });
            });
        if dismiss || response.should_close() {
            self.error = None;
        }
    }
    fn close_dialog(&mut self, ctx: &egui::Context) {
        let mut keep = false;
        let mut stop = false;
        let response = egui::Modal::new(egui::Id::new("close"))
            .frame(modal_frame())
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                parts::dialog_heading(
                    ui,
                    "Quit Pig Pal?",
                    if self.job.is_some() {
                        "A transcription or export is still running. Wait for it to finish."
                    } else {
                        "The recording will be stopped and saved before Pig Pal quits."
                    },
                );
                ui.add_space(4.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.job.is_none() {
                        stop = Btn::danger("Stop, save & quit").show(ui).clicked();
                    }
                    keep = Btn::ghost("Keep recording").show(ui).clicked();
                });
            });
        if stop {
            self.stop(false);
            if self.error.is_none() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if keep || response.should_close() {
            self.close_confirm = false;
        }
    }
    fn toasts(&mut self, ctx: &egui::Context, offset_x: f32) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        let mut items: Vec<(Option<String>, String, Color32)> = self
            .toasts
            .iter()
            .map(|t| {
                (
                    None,
                    t.text.clone(),
                    if t.kind == ToastKind::Warn {
                        warn()
                    } else {
                        live()
                    },
                )
            })
            .collect();
        if self.job.is_some() {
            let text = self.progress.clone().unwrap_or_else(|| "Working…".into());
            items.push((Some(String::new()), text, accent()));
        }
        if items.is_empty() {
            return;
        }
        if let Some(next) = self.toasts.iter().map(|t| t.until).min() {
            ctx.request_repaint_after(next.saturating_duration_since(now));
        }
        egui::Area::new(egui::Id::new("toasts"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(offset_x, -20.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (spinner, text_, color) in items {
                    egui::Frame::new()
                        .fill(surface())
                        .stroke(Stroke::new(1.0, line_strong()))
                        .corner_radius(10)
                        .inner_margin(Margin::symmetric(14, 10))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 6],
                            blur: 20,
                            spread: 0,
                            color: Color32::from_black_alpha(palette().shadow),
                        })
                        .show(ui, |ui| {
                            ui.set_max_width(520.0);
                            ui.horizontal(|ui| {
                                if spinner.is_some() {
                                    ui.add(egui::Spinner::new().size(14.0).color(color));
                                } else {
                                    dot(ui, color, 3.5);
                                }
                                ui.add(
                                    egui::Label::new(RichText::new(text_).size(13.0).color(text()))
                                        .truncate(),
                                );
                            });
                        });
                }
            });
    }

    fn frame(&mut self, ctx: &egui::Context) {
        self.tick();
        // Capture runs on its own thread, so these rates only affect what is shown.
        // An unfocused window refreshes once a second; a hidden or minimized one
        // is not drawn at all.
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let mic = self.recorder.as_ref().is_some_and(|r| r.mic.is_some());
        let playing = self.replay.as_ref().is_some_and(|r| r.player.playing);
        if !focused && (self.recorder.is_some() || playing) {
            ctx.request_repaint_after(Duration::from_millis(1000));
        } else if mic || playing {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else if self.recorder.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if self.job.is_some() || self.device_scan.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.busy() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_confirm = true;
        }
        ctx.layer_painter(egui::LayerId::background())
            .rect_filled(ctx.screen_rect(), 0, bg());
        let replay_mode = self.replay.is_some() && self.recorder.is_none();
        let show_sidebar = !replay_mode || self.replay.as_ref().is_some_and(|r| r.show_library);
        if show_sidebar {
            self.sidebar(ctx);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(bg()).inner_margin(if replay_mode {
                Margin::symmetric(20, 16)
            } else {
                Margin {
                    left: 36,
                    right: 36,
                    top: 28,
                    bottom: 0,
                }
            }))
            .show(ctx, |ui| {
                if replay_mode {
                    if let Some(id) = self.selected.clone() {
                        self.replay_screen(ui, &id);
                    }
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt(("workspace", self.selected.clone(), self.recorder.is_some()))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.recorder.is_some() {
                            self.live(ui);
                        } else if let Some(id) = self.selected.clone() {
                            self.review(ui, &id);
                        } else {
                            self.new_session(ui);
                        }
                        ui.add_space(40.0);
                    });
            });
        self.toasts(
            ctx,
            if show_sidebar {
                SIDEBAR_WIDTH / 2.0
            } else {
                0.0
            },
        );
        if self.settings_open {
            self.settings(ctx);
        }
        if self.note_anchor.is_some() {
            self.note_dialog(ctx);
        }
        if self.resume_prompt && self.recorder.is_none() {
            self.resume_dialog(ctx);
        }
        if let Some(error) = self.error.clone() {
            self.error_dialog(ctx, error);
        }
        if self.close_confirm {
            self.close_dialog(ctx);
        }
    }
}

impl eframe::App for Desktop {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::sync(ctx);
        self.reclaim();
        self.frame(ctx);
        self.park();
    }
    fn on_exit(&mut self) {
        if let Some(capture) = self.capture.take() {
            if let Some(rec) = capture.shutdown() {
                self.recorder = Some(rec);
            }
        }
        if let Some(mut rec) = self.recorder.take() {
            let _ = rec.stop(&self.root, true);
        }
    }
}

pub(crate) fn configure_style(ctx: &egui::Context) {
    theme::configure(ctx);
}

// ---------------------------------------------------------------------------
// Helpers

fn state_style(state: &str) -> (&'static str, Color32) {
    match state {
        "completed" => ("Completed", text_2()),
        "recording" => ("Recording", danger()),
        "interrupted" => ("Interrupted", warn()),
        _ => ("Saved", text_2()),
    }
}

fn session_duration(s: &Session) -> Option<f64> {
    let total: f64 = s
        .segments
        .iter()
        .filter_map(|seg| {
            seg.ended
                .map(|end| (end - seg.started).num_milliseconds() as f64 / 1000.0)
        })
        .sum();
    (total > 0.0).then_some(total)
}

fn source_summary(c: &SourceConfig) -> String {
    let decode = if c.hl7 {
        "HL7 v2 decoding"
    } else {
        "Waveline binary decoding (experimental)"
    };
    match c.mode {
        Source::Demo => "Generated measurements · no monitor connection".into(),
        Source::Notes => "No monitor · notes, audio and media only".into(),
        Source::TcpListen => format!(
            "Listens on {}:{} for {} · {decode}",
            c.bind,
            c.port,
            if c.allow_any_peer {
                "any sender"
            } else {
                c.host.as_str()
            }
        ),
        Source::Udp => format!("UDP on {}:{} · {decode}", c.bind, c.port),
        Source::TcpConnect => format!("Connects to {}:{} · {decode}", c.host, c.port),
        Source::Serial => format!(
            "{} at {} baud · {decode}",
            if c.serial_port.is_empty() {
                "No serial device"
            } else {
                &c.serial_port
            },
            c.baud
        ),
    }
}

fn format_time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}
fn format_duration(seconds: f64) -> String {
    let s = seconds.round() as u64;
    match (s / 3600, (s / 60) % 60, s % 60) {
        (0, 0, s) => format!("{s}s"),
        (0, m, s) => format!("{m}m {s:02}s"),
        (h, m, _) => format!("{h}h {m:02}m"),
    }
}
fn format_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB", b / 1e3)
    } else {
        format!("{bytes} B")
    }
}
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Timeline rows with each note placed at the moment its "Note opened" marker
/// was captured; that placeholder marker itself is dropped.
fn timeline_rows(events: &[Event]) -> Vec<(chrono::DateTime<chrono::Utc>, &Event)> {
    let anchors: HashMap<&str, &Event> = events
        .iter()
        .filter(|e| e.kind == "marker")
        .map(|e| (e.id.as_str(), e))
        .collect();
    let amended: HashSet<&str> = events
        .iter()
        .filter(|e| e.kind == "annotation")
        .filter_map(|e| e.data["amends_id"].as_str())
        .collect();
    let mut rows: Vec<(chrono::DateTime<chrono::Utc>, &Event)> = events
        .iter()
        .filter(|e| store::TIMELINE_KINDS.contains(&e.kind.as_str()))
        .filter(|e| !(e.kind == "marker" && amended.contains(e.id.as_str())))
        .map(|e| {
            let at = e.data["amends_id"]
                .as_str()
                .and_then(|id| anchors.get(id))
                .map_or(e.utc, |anchor| anchor.utc);
            (at, e)
        })
        .collect();
    rows.sort_by_key(|(at, e)| (*at, e.sequence));
    rows
}

/// Markers and notes for the replay viewer, oldest first.
fn replay_notes(events: &[Event]) -> Vec<crate::replay_ui::ReplayNote> {
    timeline_rows(events)
        .into_iter()
        .filter(|(_, e)| matches!(e.kind.as_str(), "marker" | "annotation"))
        .map(|(at, e)| {
            let text = e.data["note"].as_str().unwrap_or("").trim().to_string();
            crate::replay_ui::ReplayNote {
                at,
                label: e.data["label"]
                    .as_str()
                    .filter(|l| !l.is_empty())
                    .unwrap_or("Note")
                    .to_string(),
                is_note: e.kind == "annotation" || !text.is_empty(),
                text,
            }
        })
        .collect()
}

#[path = "ui_parts.rs"]
mod parts;
#[path = "ui_views.rs"]
mod views;
#[cfg(test)]
pub(crate) use parts::arterial_panel;

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

    #[test]
    fn helpers_format_human_readable_values() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(8025), "8,025");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(format_duration(42.0), "42s");
        assert_eq!(format_duration(125.0), "2m 05s");
        assert_eq!(format_duration(8460.0), "2h 21m");
        assert_eq!(format_bytes(1_946_220), "1.9 MB");
    }
}

#[cfg(test)]
mod replay_note_tests {
    use super::*;

    fn event(seq: u64, id: &str, s: u32, kind: &str, data: serde_json::Value) -> Event {
        Event {
            sequence: seq,
            id: id.into(),
            segment_id: "seg".into(),
            utc: format!("2026-10-20T14:00:{s:02}Z").parse().unwrap(),
            elapsed_s: f64::from(s),
            kind: kind.into(),
            data,
        }
    }

    #[test]
    fn notes_are_placed_when_opened_and_markers_kept() {
        let events = vec![
            event(1, "start", 0, "start", serde_json::json!({})),
            event(
                2,
                "a",
                5,
                "marker",
                serde_json::json!({"label":"Note opened","note":""}),
            ),
            event(
                3,
                "b",
                20,
                "marker",
                serde_json::json!({"label":"Baseline","note":""}),
            ),
            event(
                4,
                "c",
                40,
                "annotation",
                serde_json::json!({"label":"Note","note":"carotid sample","amends_id":"a"}),
            ),
        ];
        let notes = replay_notes(&events);
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].label, "Note");
        assert_eq!(notes[0].text, "carotid sample");
        assert!(notes[0].is_note);
        assert_eq!(
            notes[0].at, events[1].utc,
            "note uses the time it was opened"
        );
        assert_eq!(notes[1].label, "Baseline");
        assert!(!notes[1].is_note);
    }
}

#[cfg(test)]
mod background_capture_tests {
    use super::*;
    use egui_kittest::Harness;
    use std::io::Write;

    /// Monitor data must keep flowing to disk while no frames are drawn, as
    /// when the window is minimized, hidden or behind other windows.
    #[test]
    fn capture_continues_to_disk_without_ui_frames() {
        let temp = tempfile::tempdir().unwrap();
        let root = Root::open(temp.path().to_path_buf()).unwrap();
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut h = Harness::builder().build_eframe(move |cc| Desktop::new(cc, root));
        {
            let d = h.state_mut();
            d.settings.source = SourceConfig {
                mode: Source::TcpListen,
                port,
                hl7: false,
                ..Default::default()
            };
            d.subject = "Bench".into();
            d.title = "Minimized capture".into();
            d.authorized = true;
            d.start(None);
            // End the frame so the background thread owns the recorder.
            d.park();
            assert!(d.recorder.is_none());
        }
        let mut record = vec![0; crate::waveline::RECORD_LEN];
        record[..4].copy_from_slice(&crate::waveline::MAGIC);
        record[4..8].copy_from_slice(&(crate::waveline::RECORD_LEN as u32).to_le_bytes());
        const COUNT: usize = 600; // far more than the 128-chunk transport queue
        let end = Instant::now() + Duration::from_secs(3);
        let mut stream = loop {
            if let Ok(s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                break s;
            }
            assert!(Instant::now() < end, "listener did not start");
            std::thread::sleep(Duration::from_millis(20));
        };
        for _ in 0..COUNT {
            // Small writes produce many separate transport reads.
            for chunk in record.chunks(489) {
                stream.write_all(chunk).unwrap();
            }
        }
        stream.flush().unwrap();
        // No UI frames are run while waiting: only the background thread polls.
        let capture = h.state().capture.as_ref().unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let done = lock(&capture.slot)
                .as_ref()
                .is_some_and(|r| r.binary_records == COUNT as u64);
            if done {
                break;
            }
            assert!(Instant::now() < end, "background capture fell behind");
            std::thread::sleep(Duration::from_millis(20));
        }
        let dir = lock(&capture.slot).as_ref().unwrap().journal.dir.clone();
        assert_eq!(
            std::fs::metadata(dir.join("wire.bin")).unwrap().len(),
            (COUNT * crate::waveline::RECORD_LEN) as u64
        );
        drop(stream);
        let d = h.state_mut();
        d.reclaim();
        d.stop(false);
        assert!(d.capture.is_none() && d.recorder.is_none());
        let session = d
            .sessions
            .iter()
            .find(|s| s.title == "Minimized capture")
            .unwrap();
        assert_eq!(session.state, "completed");
    }
}

#[cfg(test)]
#[path = "ui_snapshots.rs"]
mod snapshots;
