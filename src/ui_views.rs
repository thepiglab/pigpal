//! The main pages: new recording, live capture, saved session and replay.
use super::*;

impl Desktop {
    // -----------------------------------------------------------------------
    // New recording

    pub(super) fn new_session(&mut self, ui: &mut egui::Ui) {
        if self.devices.is_empty() && self.device_scan.is_none() && !self.devices_scanned {
            self.devices_scanned = true;
            self.scan_devices();
        }
        ui.add_space(12.0);
        let width = ui.available_width().min(560.0);
        let pad = ((ui.available_width() - width) / 2.0).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(semibold("New recording", 22.0));
                ui.label(secondary(
                    "Saves the raw monitor stream, decoded vitals and optional audio to this laptop.",
                    13.5,
                ));
                ui.add_space(20.0);
                card().inner_margin(Margin::same(24)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 8.0;
                    let inner = ui.available_width();
                    field_label(ui, "Subject");
                    text_field(ui, "new_subject", &mut self.subject, "e.g. Pig 7", None);
                    ui.add_space(8.0);
                    field_label(ui, "Session title");
                    text_field(ui, "new_title", &mut self.title, "e.g. Baseline and intervention", None);
                    ui.add_space(8.0);
                    field_label(ui, "Monitor");
                    let before = self.settings.source.mode;
                    parts::source_select(ui, "new_source", &mut self.settings.source.mode, inner);
                    if self.settings.source.mode != before {
                        let mode = self.settings.source.mode;
                        self.persist(|s| s.source.mode = mode);
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.label(muted_text(source_summary(&self.settings.source), 12.0));
                        if parts::link(ui, "Edit").clicked() {
                            self.open_settings(SettingsTab::Connection);
                        }
                    });
                    ui.add_space(8.0);
                    field_label(ui, "Microphone");
                    self.microphone_select(ui, "new_mic", inner);
                    ui.label(muted_text("Optional. It starts paused; turn it on during the recording.", 12.0));
                    ui.add_space(12.0);
                    let r = ui.available_rect_before_wrap();
                    ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, line()));
                    ui.add_space(16.0);
                    checkbox(ui, &mut self.authorized, "I am authorized to record this session");
                    ui.add_space(12.0);
                    let ready = self.authorized
                        && !self.subject.trim().is_empty()
                        && !self.title.trim().is_empty()
                        && self.job.is_none();
                    if Btn::primary("Start recording")
                        .size(Size::Lg)
                        .width(inner)
                        .enabled(ready)
                        .show(ui)
                        .on_disabled_hover_text("Enter a subject and title, then confirm authorization")
                        .clicked()
                    {
                        self.start(None);
                    }
                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    dot(ui, if self.model_ready { live() } else { muted() }, 3.0);
                    ui.label(muted_text(
                        if self.model_ready {
                            "Offline transcription is installed."
                        } else {
                            "Offline transcription is not installed."
                        },
                        12.0,
                    ));
                    if !self.model_ready && parts::link(ui, "Set up").clicked() {
                        self.open_settings(SettingsTab::Transcription);
                    }
                });
            });
        });
    }

    // -----------------------------------------------------------------------
    // Live capture

    pub(super) fn live(&mut self, ui: &mut egui::Ui) {
        let mut stop = false;
        {
            let rec = self.recorder.as_ref().unwrap();
            let meta = format!(
                "{}  ·  Segment {}  ·  {}{}",
                rec.session.subject,
                rec.session.segments.len(),
                rec.source().mode.label(),
                if rec.session.synthetic && rec.source().mode != Source::Demo {
                    "  ·  Synthetic data"
                } else {
                    ""
                }
            );
            parts::page_header(ui, &rec.session.title, &meta, |ui| {
                stop = Btn::danger("Stop & save")
                    .icon(Icon::Stop)
                    .show(ui)
                    .on_hover_text("Finish this segment and save everything")
                    .clicked();
                ui.add_space(12.0);
                parts::rec_timer(ui, rec.journal.elapsed());
            });
        }
        if stop {
            self.stop(false);
            return;
        }
        ui.add_space(24.0);
        let total = ui.available_width();
        if total >= 860.0 {
            let main = total - RAIL_WIDTH - 24.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 24.0;
                ui.vertical(|ui| {
                    ui.set_width(main);
                    self.live_monitor(ui);
                });
                ui.vertical(|ui| {
                    ui.set_width(RAIL_WIDTH);
                    self.live_rail(ui);
                });
            });
        } else {
            self.live_monitor(ui);
            ui.add_space(16.0);
            self.live_rail(ui);
        }
    }
    fn live_monitor(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 0.0;
        let rec = self.recorder.as_ref().unwrap();
        let now = rec.journal.elapsed();
        let mut copy = None;
        parts::connection_strip(ui, rec, now, &mut copy);
        if let Some(path) = copy {
            ui.ctx().copy_text(path);
            self.toast("Live CSV path copied");
        }
        ui.add_space(16.0);
        let rec = self.recorder.as_ref().unwrap();
        let binary =
            !rec.source().hl7 && !matches!(rec.source().mode, Source::Demo | Source::Notes);
        if binary {
            tabs(
                ui,
                &mut self.monitor_view,
                &[
                    (MonitorView::Waveline, "Monitor", None),
                    (MonitorView::Decoder, "Decoded fields", None),
                ],
            );
            ui.add_space(16.0);
            let rec = self.recorder.as_ref().unwrap();
            let age = rec.last_binary_elapsed_s.map(|at| (now - at).max(0.0));
            match (self.monitor_view, &rec.binary_latest) {
                (MonitorView::Waveline, record) => {
                    let receipt = rec
                        .last_binary_received_utc
                        .map(|utc| utc.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true));
                    crate::waveline_monitor::show(ui, record.as_ref(), age, receipt.as_deref());
                }
                (MonitorView::Decoder, Some(record)) => {
                    card().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        parts::binary_panel(ui, record, rec.binary_records, age);
                    });
                }
                (MonitorView::Decoder, None) => parts::arterial_panel(ui, None, None),
            }
            return;
        }
        if rec.latest.is_empty() {
            let since = rec.last_received_elapsed_s.map(|at| (now - at).max(0.0));
            let (title, detail) = if rec.bytes > 0 && since.is_some_and(|age| age > 3.0) {
                ("Monitor stream is inactive", "Data received earlier is saved. Nothing new has arrived for more than three seconds.")
            } else if rec.bytes > 0 {
                (
                    "Receiving data, nothing decoded yet",
                    "Raw data is being saved. No supported HL7 measurements have been decoded yet.",
                )
            } else {
                (
                    "Waiting for the monitor",
                    "No data yet. Check the monitor's output and the connection settings.",
                )
            };
            parts::empty_state(ui, Icon::Wave, title, detail);
            return;
        }
        parts::vitals(ui, rec, now);
        ui.add_space(16.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            self.plot(ui);
        });
    }
    fn live_rail(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 0.0;
        self.audio_panel(ui);
        ui.add_space(16.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            section_title(ui, "Markers", None);
            ui.label(muted_text("Timestamped the moment you click.", 12.0));
            ui.add_space(12.0);
            let w = (ui.available_width() - 8.0) / 2.0;
            let mut clicked = None;
            for pair in [["Baseline", "Intervention"], ["Sample", "Event"]] {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for label in pair {
                        if Btn::secondary(label).width(w).show(ui).clicked() {
                            clicked = Some(label);
                        }
                    }
                });
                ui.add_space(8.0);
            }
            if let Some(label) = clicked {
                let r = self.recorder.as_mut().unwrap().marker(label, "", None);
                if self.report(r).is_some() {
                    self.toast(format!("{label} marker saved"));
                }
            }
            if Btn::primary("Add note")
                .icon(Icon::Note)
                .width(ui.available_width())
                .enabled(self.note_anchor.is_none())
                .show(ui)
                .on_hover_text("The time is captured when you click, before you type")
                .clicked()
            {
                let rec = self.recorder.as_mut().unwrap();
                self.note_opened_at = rec.journal.elapsed();
                let r = rec.marker("Note opened", "", None);
                self.note_anchor = self.report(r);
                self.note.clear();
            }
        });
        ui.add_space(16.0);
        card()
            .inner_margin(Margin {
                left: 20,
                right: 20,
                top: 20,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                section_title(ui, "Timeline", None);
                ui.add_space(8.0);
                let rec = self.recorder.as_ref().unwrap();
                let rows = parts::rows_from_events(&rec.timeline, false);
                egui::ScrollArea::vertical()
                    .id_salt("live_timeline")
                    .max_height(340.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        parts::timeline_list(ui, &rows, false, true);
                    });
            });
    }
    fn audio_panel(&mut self, ui: &mut egui::Ui) {
        let rec = self.recorder.as_ref().unwrap();
        let active = rec.mic.is_some();
        let now = rec.journal.elapsed();
        let (status, db) = self.waveform.status(now);
        let mut toggle = false;
        let mut pick = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(semibold("Microphone", 15.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (label, color) = if !active {
                        ("Off", muted())
                    } else if status == "Input clipping" {
                        ("Clipping", warn())
                    } else {
                        ("Recording", danger())
                    };
                    chip(ui, label, color);
                });
            });
            ui.add_space(12.0);
            egui::Frame::new()
                .fill(input())
                .stroke(Stroke::new(1.0, line()))
                .corner_radius(8)
                .inner_margin(Margin::symmetric(8, 4))
                .show(ui, |ui| self.waveform.show(ui, now, active, 52.0));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(muted_text(
                    if active {
                        status
                    } else if !self.waveform.is_empty() {
                        "Paused. Audio so far is saved."
                    } else {
                        "Starts only when you turn it on."
                    },
                    12.0,
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let (true, Some(d)) = (active, db) {
                        ui.label(mono(format!("{d:.0} dBFS"), 12.0).color(text_2()));
                    }
                });
            });
            ui.add_space(12.0);
            let w = ui.available_width();
            toggle = if active {
                Btn::secondary("Pause microphone")
                    .icon(Icon::Pause)
                    .width(w)
                    .show(ui)
                    .clicked()
            } else {
                Btn::primary("Turn on microphone")
                    .icon(Icon::Mic)
                    .width(w)
                    .show(ui)
                    .clicked()
            };
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add(
                    egui::Label::new(muted_text(
                        if self.settings.microphone.is_empty() {
                            "No input selected".to_string()
                        } else {
                            self.settings.microphone.clone()
                        },
                        12.0,
                    ))
                    .truncate(),
                );
                if !active && parts::link(ui, "Change").clicked() {
                    pick = true;
                }
            });
        });
        if pick {
            self.open_settings(SettingsTab::Audio);
        }
        if toggle {
            if !active && self.settings.microphone.is_empty() {
                self.open_settings(SettingsTab::Audio);
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
        let mut chosen = None;
        ui.horizontal(|ui| {
            section_title_inline(ui, "Trend", "Last 60 seconds");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let current = rec
                    .latest
                    .get(&self.plot_key)
                    .map(|v| v.observation.label.clone())
                    .unwrap_or_else(|| "Series".into());
                select(ui, "plot_series", &current, 220.0, |ui| {
                    for (key, v) in &rec.latest {
                        if menu_item(
                            ui,
                            None,
                            &v.observation.label,
                            Some(&format!(
                                "{} · {}",
                                v.observation.units, v.observation.source
                            )),
                            *key == self.plot_key,
                        )
                        .clicked()
                        {
                            chosen = Some(key.clone());
                        }
                    }
                });
            });
        });
        if let Some(key) = chosen {
            self.plot_key = key;
        }
        ui.add_space(12.0);
        let rec = self.recorder.as_ref().unwrap();
        parts::trend(ui, rec, &self.plot_key);
    }

    // -----------------------------------------------------------------------
    // Replay

    pub(super) fn replay_screen(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(session) = self.sessions.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        let Some(replay) = &mut self.replay else {
            return;
        };
        let mut close = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            close = Btn::ghost("Back")
                .icon(Icon::ChevronLeft)
                .show(ui)
                .on_hover_text("Back to the session")
                .clicked();
            ui.label(semibold(&session.title, 15.0));
            ui.label(muted_text(&session.subject, 13.0));
            tag(ui, "REPLAY");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Btn::ghost(if replay.show_library {
                    "Hide sessions"
                } else {
                    "Sessions"
                })
                .selected(replay.show_library)
                .show(ui)
                .clicked()
                {
                    replay.show_library = !replay.show_library;
                }
            });
        });
        ui.add_space(12.0);
        replay.show(ui);
        if close {
            self.replay = None;
        }
    }

    // -----------------------------------------------------------------------
    // Saved session

    pub(super) fn review(&mut self, ui: &mut egui::Ui, id: &str) {
        let Some(session) = self.sessions.iter().find(|s| s.id == id).cloned() else {
            return;
        };
        let dir = self.root.session_dir(id);
        let has_binary = session
            .segments
            .iter()
            .any(|s| !s.source.hl7 && !matches!(s.source.mode, Source::Demo | Source::Notes));
        let has_audio = self
            .media_files
            .iter()
            .any(|f| f.extension().and_then(|x| x.to_str()) == Some("wav"));
        let idle = self.job.is_none();

        let local = session.created.with_timezone(&chrono::Local);
        let mut meta = vec![
            session.subject.clone(),
            local.format("%b %-d, %Y at %H:%M").to_string(),
        ];
        if let Some(d) = session_duration(&session) {
            meta.push(format_duration(d));
        }
        meta.push(format!(
            "{} segment{}",
            session.segments.len(),
            if session.segments.len() == 1 { "" } else { "s" }
        ));
        let (state, _) = state_style(&session.state);
        if state != "Completed" {
            meta.push(state.to_string());
        }
        if session.synthetic {
            meta.push("Synthetic data".into());
        }
        let mut replay = false;
        let mut transcribe = false;
        let mut action: Option<&'static str> = None;
        parts::page_header(ui, &session.title, &meta.join("  ·  "), |ui| {
            let more = icon_button(ui, Icon::More, "More actions");
            menu(ui, "session_more", &more, 260.0, |ui| {
                if menu_item(
                    ui,
                    Some(Icon::Export),
                    "Export data",
                    Some("Journals, CSVs and metadata"),
                    false,
                )
                .clicked()
                {
                    action = Some("export");
                }
                if menu_item(
                    ui,
                    Some(Icon::Export),
                    "Export with media",
                    Some("Also audio, video and photos"),
                    false,
                )
                .clicked()
                {
                    action = Some("export_media");
                }
                ui.separator();
                if menu_item(ui, Some(Icon::Folder), "Show in Finder", None, false).clicked() {
                    action = Some("folder");
                }
                if has_binary
                    && menu_item(ui, Some(Icon::Wave), "Inspect binary capture", None, false)
                        .clicked()
                {
                    action = Some("inspect");
                }
                ui.separator();
                if menu_item(
                    ui,
                    Some(Icon::Resume),
                    "Resume recording…",
                    Some("Adds a new segment"),
                    false,
                )
                .clicked()
                {
                    action = Some("resume");
                }
            });
            if has_audio {
                transcribe = Btn::secondary("Transcribe")
                    .icon(Icon::Text)
                    .enabled(idle)
                    .show(ui)
                    .clicked();
            }
            if has_binary {
                replay = Btn::primary("Replay")
                    .icon(Icon::Play)
                    .enabled(idle)
                    .show(ui)
                    .on_hover_text("Play the saved monitor stream")
                    .clicked();
            }
        });
        if replay {
            self.open_replay(id.to_string(), None);
        }
        if transcribe {
            if !self.model_ready {
                self.open_settings(SettingsTab::Transcription);
            } else {
                let root = self.root.path.clone();
                let folder = dir.clone();
                self.progress = Some("Loading the transcription model…".into());
                self.job(move |tx| {
                    let n = model::transcribe_session(&root, &folder, |p| {
                        let _ = tx.send(Update::Progress(p));
                    })?;
                    Ok(format!("Saved {n} new transcript(s)"))
                });
            }
        }
        match action {
            Some(kind @ ("export" | "export_media")) if idle => {
                let source = dir.clone();
                let output = self.root.path.join("exports").join(format!(
                    "{}-{}.zip",
                    session.id,
                    store::id()
                ));
                let include_media = kind == "export_media";
                self.progress = Some("Preparing export…".into());
                self.job(move |_| {
                    export::archive(&source, &output, include_media)?;
                    Ok(format!("Export saved to {}", output.display()))
                });
            }
            Some("folder") => {
                let r = open::that(&dir).map_err(Into::into);
                self.report(r);
            }
            Some("inspect") if idle => self.inspect_binary(id, &dir),
            Some("resume") => {
                self.resume_prompt = true;
                self.authorized = false;
            }
            _ => {}
        }

        if has_audio && !self.model_ready {
            ui.add_space(16.0);
            let mut setup = false;
            parts::banner(
                ui,
                warn(),
                "Transcription needs a one-time offline model download.",
                |ui| {
                    setup = Btn::secondary("Set up").size(Size::Sm).show(ui).clicked();
                },
            );
            if setup {
                self.open_settings(SettingsTab::Transcription);
            }
        }
        ui.add_space(24.0);

        let notable = parts::rows_from_events(&self.review, false);
        let all = parts::rows_from_events(&self.review, true);
        tabs(
            ui,
            &mut self.review_tab,
            &[
                (
                    ReviewTab::Timeline,
                    "Timeline",
                    Some(notable.iter().filter(|r| r.marker).count()),
                ),
                (
                    ReviewTab::Transcript,
                    "Transcript",
                    Some(self.transcripts.len()),
                ),
                (ReviewTab::Media, "Media", Some(self.media_files.len())),
                (ReviewTab::Data, "Monitor data", None),
            ],
        );
        ui.add_space(16.0);
        match self.review_tab {
            ReviewTab::Timeline => {
                ui.horizontal(|ui| {
                    ui.label(muted_text("Newest first", 12.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        checkbox(ui, &mut self.timeline_all, "Show recording events");
                    });
                });
                ui.add_space(8.0);
                let rows = if self.timeline_all { &all } else { &notable };
                let mut jump = None;
                if rows.is_empty() {
                    parts::empty_state(
                        ui,
                        Icon::Flag,
                        "No markers or notes",
                        "Markers and notes added during the recording appear here. Turn on recording events to see starts, stops and connections.",
                    );
                } else {
                    card().inner_margin(Margin::symmetric(0, 8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        jump = parts::timeline_list(ui, rows, has_binary && idle, false);
                    });
                }
                if let Some(at) = jump {
                    self.open_replay(
                        id.to_string(),
                        Some(at.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)),
                    );
                }
            }
            ReviewTab::Transcript => self.transcript_tab(ui),
            ReviewTab::Media => self.media_tab(ui),
            ReviewTab::Data => self.data_tab(ui, id, &dir, has_binary),
        }
    }
    fn transcript_tab(&mut self, ui: &mut egui::Ui) {
        if self.transcripts.is_empty() {
            parts::empty_state(
                ui,
                Icon::Text,
                "No transcript yet",
                "Use Transcribe to turn this session's audio into text. It runs offline on this laptop.",
            );
            return;
        }
        for t in &self.transcripts {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    ui.label(mono(t.audio_started.with_timezone(&chrono::Local).format("%H:%M:%S").to_string(), 12.0).color(text_2()));
                    ui.label(muted_text(&t.engine, 12.0));
                });
                ui.add(egui::Label::new(RichText::new(&t.text).size(14.5).color(text())).wrap().selectable(true));
                if !t.timings_valid {
                    ui.label(muted_text("Some word timestamps fall outside this clip. Original results are kept.", 12.0).color(warn()));
                }
            });
            ui.add_space(12.0);
        }
    }
    fn media_tab(&mut self, ui: &mut egui::Ui) {
        if self.media_files.is_empty() {
            parts::empty_state(
                ui,
                Icon::Folder,
                "No saved media",
                "Audio, video and photos captured during this session appear here.",
            );
            return;
        }
        let mut open_file = None;
        card().inner_margin(Margin::symmetric(0, 6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (index, file) in self.media_files.iter().enumerate() {
                if parts::media_row(ui, file, index > 0) {
                    open_file = Some(file.clone());
                }
            }
        });
        if let Some(file) = open_file {
            let r = open::that(file).map_err(Into::into);
            self.report(r);
        }
    }
    fn data_tab(&mut self, ui: &mut egui::Ui, id: &str, dir: &std::path::Path, has_binary: bool) {
        if has_binary {
            match &self.binary_review {
                None => {
                    let mut inspect = false;
                    card().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 8.0;
                        section_title(ui, "Binary capture", None);
                        ui.label(secondary(
                            "Decode the saved Waveline records to check field values, raw signal slices and framing. Experimental.",
                            13.5,
                        ));
                        ui.add_space(4.0);
                        inspect = Btn::secondary("Inspect binary capture")
                            .icon(Icon::Wave)
                            .enabled(self.job.is_none())
                            .show(ui)
                            .clicked();
                    });
                    if inspect {
                        self.inspect_binary(id, dir);
                    }
                }
                Some(inspection) => {
                    tabs(
                        ui,
                        &mut self.monitor_view,
                        &[
                            (MonitorView::Waveline, "Monitor", None),
                            (MonitorView::Decoder, "Decoded fields", None),
                        ],
                    );
                    ui.add_space(8.0);
                    ui.label(muted_text(
                        format!(
                            "Last record of {}  ·  {} unrecognized bytes  ·  {} trailing bytes kept in the original files",
                            thousands(inspection.records),
                            inspection.discarded_bytes,
                            inspection.trailing_bytes
                        ),
                        12.0,
                    ));
                    ui.add_space(12.0);
                    match (&inspection.last, self.monitor_view) {
                        (Some(record), MonitorView::Waveline) => {
                            crate::waveline_monitor::show(ui, Some(record), None, None);
                        }
                        (Some(record), MonitorView::Decoder) => {
                            card().show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                parts::binary_panel(ui, record, inspection.records, None);
                            });
                        }
                        (None, _) => parts::empty_state(
                            ui,
                            Icon::Wave,
                            "No Waveline records",
                            "No Waveline binary records were found in this capture.",
                        ),
                    }
                }
            }
            ui.add_space(24.0);
        }
        section_title(
            ui,
            "HL7 observations",
            Some("Latest 50 messages. Exports include the full journal."),
        );
        ui.add_space(8.0);
        if self.review_messages.is_empty() {
            ui.label(muted_text("No decoded HL7 messages in this session.", 13.0));
            return;
        }
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            for e in self.review_messages.iter().rev() {
                if let Ok(message) =
                    serde_json::from_value::<crate::protocol::Message>(e.data["message"].clone())
                {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            mono(
                                e.utc
                                    .with_timezone(&chrono::Local)
                                    .format("%H:%M:%S")
                                    .to_string(),
                                12.0,
                            )
                            .color(muted()),
                        );
                        for o in message.observations {
                            ui.label(secondary(format!("{} ", o.label), 13.0));
                            ui.label(semibold(format!("{} {}", o.value_text, o.units), 13.0))
                                .on_hover_text(&o.source);
                        }
                    });
                }
            }
        });
    }
}

fn section_title_inline(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.label(semibold(title, 15.0));
    ui.label(muted_text(detail, 12.5));
}
