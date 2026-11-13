//! Painted building blocks for the pages. Rows are laid out by hand so every
//! column shares one baseline and nothing shifts between screens.
use super::*;
use chrono::{DateTime, Utc};

// ---------------------------------------------------------------------------
// Shell pieces

pub(super) fn list_heading(ui: &mut egui::Ui, label: &str, count: usize) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), Sense::hover());
    let painter = ui.painter();
    let f = bold(12.0);
    let b = centered_baseline(painter, &f, rect.center().y);
    paint_on_baseline(
        painter,
        painter.layout_no_wrap(label.into(), f.clone(), muted()),
        rect.left() + 8.0,
        b,
    );
    paint_on_baseline_right(
        painter,
        painter.layout_no_wrap(count.to_string(), font(12.0), muted()),
        rect.right() - 8.0,
        b,
    );
}

pub(super) fn session_row(
    ui: &mut egui::Ui,
    s: &Session,
    selected: bool,
    enabled: bool,
) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, 54.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && response.hovered();
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 8, selected_fill());
    } else if hovered {
        painter.rect_filled(rect, 8, hover());
    }
    let alpha = if enabled || selected { 1.0 } else { 0.5 };
    let left = rect.left() + 10.0;
    let right = rect.right() - 10.0;
    let top_baseline = rect.top() + 22.0;
    let bottom_baseline = rect.top() + 41.0;
    let date = s
        .created
        .with_timezone(&chrono::Local)
        .format("%b %-d")
        .to_string();
    let date_rect = paint_on_baseline_right(
        painter,
        painter.layout_no_wrap(date, font(12.0), muted().gamma_multiply(alpha)),
        right,
        top_baseline,
    );
    let title = if s.title.trim().is_empty() {
        "Untitled"
    } else {
        &s.title
    };
    paint_on_baseline(
        painter,
        layout(
            painter,
            title,
            bold(13.5),
            text().gamma_multiply(alpha),
            Some(date_rect.left() - left - 8.0),
            true,
        ),
        left,
        top_baseline,
    );
    let (state, state_color) = state_style(&s.state);
    let flag = if s.state == "interrupted" || s.state == "recording" {
        Some((state, state_color))
    } else if s.synthetic {
        Some(("Demo", muted()))
    } else {
        None
    };
    let mut subtitle_width = right - left;
    if let Some((label, color)) = flag {
        let r = paint_on_baseline_right(
            painter,
            painter.layout_no_wrap(label.into(), bold(11.0), color.gamma_multiply(alpha)),
            right,
            bottom_baseline,
        );
        subtitle_width = r.left() - left - 8.0;
    }
    paint_on_baseline(
        painter,
        layout(
            painter,
            &s.subject,
            font(12.0),
            text_2().gamma_multiply(alpha * 0.9),
            Some(subtitle_width),
            true,
        ),
        left,
        bottom_baseline,
    );
    if hovered {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}
fn selected_fill() -> Color32 {
    theme::palette().selected
}

pub(super) fn recording_card(ui: &mut egui::Ui, rec: &Recorder) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 64.0), Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        10,
        danger_soft(),
        Stroke::new(1.0, danger().gamma_multiply(0.35)),
        egui::StrokeKind::Inside,
    );
    let left = rect.left() + 12.0;
    let b1 = rect.top() + 24.0;
    painter.circle_filled(Pos2::new(left + 4.0, b1 - 4.5), 4.0, danger());
    paint_on_baseline(
        painter,
        painter.layout_no_wrap("Recording".into(), bold(12.5), danger()),
        left + 14.0,
        b1,
    );
    paint_on_baseline_right(
        painter,
        painter.layout_no_wrap(format_time(rec.journal.elapsed()), mono_font(12.5), text()),
        rect.right() - 12.0,
        b1,
    );
    paint_on_baseline(
        painter,
        layout(
            painter,
            &rec.session.title,
            font(12.5),
            text_2(),
            Some(width - 24.0),
            true,
        ),
        left,
        rect.top() + 45.0,
    );
}

pub(super) fn nav_item(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 8, selected_fill());
    } else if response.hovered() {
        painter.rect_filled(rect, 8, hover());
    }
    let color = if selected { text() } else { text_2() };
    paint_icon(
        painter,
        Rect::from_center_size(
            Pos2::new(rect.left() + 18.0, rect.center().y),
            Vec2::splat(15.0),
        ),
        icon,
        if selected { accent() } else { muted() },
    );
    let f = if selected { bold(13.5) } else { font(13.5) };
    let b = centered_baseline(painter, &f, rect.center().y);
    paint_on_baseline(
        painter,
        painter.layout_no_wrap(label.into(), f, color),
        rect.left() + 36.0,
        b,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) fn dialog_heading(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.label(semibold(title, 17.0));
    if !detail.is_empty() {
        ui.add(egui::Label::new(secondary(detail, 13.0)).wrap());
    }
    ui.add_space(16.0);
    ui.spacing_mut().item_spacing.y = 8.0;
}

pub(super) fn link(ui: &mut egui::Ui, text_: &str) -> Response {
    let f = bold(12.0);
    let g = ui
        .painter()
        .layout_no_wrap(text_.into(), f.clone(), accent());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(g.size().x, 18.0), Sense::click());
    let color = if response.hovered() {
        accent_hover()
    } else {
        accent()
    };
    let painter = ui.painter();
    let b = centered_baseline(painter, &f, rect.center().y);
    let r = paint_on_baseline(
        painter,
        painter.layout_no_wrap(text_.into(), f, color),
        rect.left(),
        b,
    );
    if response.hovered() {
        painter.hline(r.x_range(), b + 2.0, Stroke::new(1.0, color));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(super) fn note_box(ui: &mut egui::Ui, text_: &str) {
    egui::Frame::new()
        .fill(raised())
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(secondary(text_, 12.5)).wrap());
        });
}

pub(super) fn status_line(ui: &mut egui::Ui, color: Color32, text_: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        dot(ui, color, 4.0);
        ui.label(secondary(text_, 13.5));
    });
}

pub(super) fn banner(
    ui: &mut egui::Ui,
    color: Color32,
    text_: &str,
    actions: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                dot(ui, color, 4.0);
                ui.label(RichText::new(text_).size(13.5).color(text()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
            });
        });
}

pub(super) fn source_select(ui: &mut egui::Ui, id: &str, source: &mut Source, width: f32) {
    let mut chosen = None;
    select(ui, id, source.label(), width, |ui| {
        for item in Source::ALL {
            let detail = match item {
                Source::Demo => "Generated data for trying Pig Pal",
                Source::TcpConnect => "Pig Pal connects to the monitor",
                Source::TcpListen => "The monitor connects to Pig Pal (Waveline)",
                Source::Udp => "Datagrams sent to this laptop",
                Source::Serial => "RS-232 / USB serial adapter",
                Source::Notes => "No monitor; notes, audio and media only",
            };
            if menu_item(ui, None, item.label(), Some(detail), item == *source).clicked() {
                chosen = Some(item);
            }
        }
    });
    if let Some(item) = chosen {
        *source = item;
    }
}

/// Page title, meta line and right-aligned actions (laid out right to left).
pub(super) fn page_header(
    ui: &mut egui::Ui,
    title: &str,
    meta: &str,
    actions: impl FnOnce(&mut egui::Ui),
) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 56.0), Sense::hover());
    let painter = ui.painter().clone();
    let actions_rect = Rect::from_min_max(Pos2::new(rect.center().x, rect.top()), rect.max);
    let mut actions_left = rect.right();
    ui.allocate_new_ui(
        egui::UiBuilder::new()
            .max_rect(actions_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            actions(ui);
            actions_left = ui.min_rect().left();
        },
    );
    let text_width = (actions_left - rect.left() - 24.0).max(120.0);
    paint_on_baseline(
        &painter,
        layout(&painter, title, bold(22.0), text(), Some(text_width), true),
        rect.left(),
        rect.top() + 24.0,
    );
    paint_on_baseline(
        &painter,
        layout(&painter, meta, font(13.5), text_2(), Some(text_width), true),
        rect.left(),
        rect.top() + 48.0,
    );
}

pub(super) fn rec_timer(ui: &mut egui::Ui, elapsed: f64) {
    let f = mono_font(15.0);
    let g = ui
        .painter()
        .layout_no_wrap(format_time(elapsed), f.clone(), text());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(g.size().x + 20.0, 32.0), Sense::hover());
    let painter = ui.painter();
    painter.circle_filled(Pos2::new(rect.left() + 5.0, rect.center().y), 5.0, danger());
    let b = centered_baseline(painter, &f, rect.center().y);
    paint_on_baseline(painter, g, rect.left() + 18.0, b);
}

pub(super) fn empty_state(ui: &mut egui::Ui, icon: Icon, title: &str, detail: &str) {
    card()
        .inner_margin(Margin::symmetric(24, 32))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let (r, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
                ui.painter().rect_filled(r, 10, raised());
                paint_icon(ui.painter(), r.shrink(9.0), icon, muted());
                ui.add_space(4.0);
                ui.label(semibold(title, 14.5));
                ui.set_max_width(440.0);
                ui.add(egui::Label::new(muted_text(detail, 13.0)).wrap());
            });
        });
}

// ---------------------------------------------------------------------------
// Live

pub(super) fn connection_strip(
    ui: &mut egui::Ui,
    rec: &Recorder,
    now: f64,
    copy: &mut Option<String>,
) {
    let since = rec.last_received_elapsed_s.map(|at| (now - at).max(0.0));
    let (state, color) = match since {
        None => ("Waiting for data", muted()),
        Some(age) if age > crate::waveline::STALE_AFTER_S => ("Stream inactive", warn()),
        Some(_) => ("Receiving", live()),
    };
    let source = rec.source();
    let mut stats: Vec<(String, String)> = Vec::new();
    if matches!(source.mode, Source::TcpListen | Source::Udp) {
        stats.push((
            "Listening".into(),
            format!("{}:{}", source.bind, source.port),
        ));
    }
    stats.push(("Received".into(), format_bytes(rec.bytes)));
    stats.push(if rec.binary_records > 0 || !source.hl7 {
        ("Records".into(), thousands(rec.binary_records))
    } else {
        ("Messages".into(), thousands(rec.messages))
    });
    stats.push((
        "Last data".into(),
        since.map_or("—".into(), |a| format!("{a:.1} s ago")),
    ));
    card()
        .inner_margin(Margin::symmetric(16, 0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::hover());
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child.spacing_mut().item_spacing.x = 0.0;
            chip(&mut child, state, color).on_hover_text(&rec.status);
            let painter = child.painter().clone();
            let mut x = child.cursor().left() + 20.0;
            let label_font = font(12.0);
            let value_font = bold(13.0);
            let b = centered_baseline(&painter, &value_font, rect.center().y);
            for (label, value) in &stats {
                painter.vline(
                    x - 10.0,
                    Rangef::new(rect.center().y - 9.0, rect.center().y + 9.0),
                    Stroke::new(1.0, line()),
                );
                let l = paint_on_baseline(
                    &painter,
                    painter.layout_no_wrap(label.clone(), label_font.clone(), muted()),
                    x,
                    b,
                );
                let v = paint_on_baseline(
                    &painter,
                    painter.layout_no_wrap(value.clone(), value_font.clone(), text()),
                    l.right() + 6.0,
                    b,
                );
                x = v.right() + 20.0;
            }
            if rec.binary_records > 0 {
                let path = rec
                    .journal
                    .dir
                    .join("waveline_live.csv")
                    .display()
                    .to_string();
                child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if Btn::ghost("Copy CSV path")
                        .size(Size::Sm)
                        .show(ui)
                        .on_hover_text(&path)
                        .clicked()
                    {
                        *copy = Some(path.clone());
                    }
                });
            }
        });
    // Routine connection states are already summed up by the chip; only
    // show the status text when it says something else (errors, warnings).
    let routine = ["Connected", "Listening", "Starting", "Waiting"];
    if !rec.status.is_empty() && !routine.iter().any(|r| rec.status.starts_with(r)) {
        ui.add_space(6.0);
        ui.label(muted_text(&rec.status, 12.0));
    }
}

/// Decoded HL7 readings as a grid of tiles.
pub(super) fn vitals(ui: &mut egui::Ui, rec: &Recorder, now: f64) {
    let values: Vec<_> = rec.latest.values().take(8).collect();
    let width = ui.available_width();
    // Balance rows: 4 tiles in a 3-wide space become 2 × 2, not 3 + 1.
    let max = ((width / 190.0).floor() as usize).clamp(2, 4);
    let rows = values.len().div_ceil(max).max(1);
    let columns = values.len().div_ceil(rows).max(1);
    let gap = 12.0;
    let tile_w = (width - gap * (columns - 1) as f32) / columns as f32;
    for chunk in values.chunks(columns) {
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, 104.0), Sense::hover());
        for (i, v) in chunk.iter().enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(row.left() + i as f32 * (tile_w + gap), row.top()),
                Vec2::new(tile_w, 104.0),
            );
            let painter = ui.painter();
            painter.rect(
                rect,
                12,
                surface(),
                Stroke::new(1.0, line()),
                egui::StrokeKind::Inside,
            );
            let left = rect.left() + 16.0;
            paint_on_baseline(
                painter,
                layout(
                    painter,
                    &v.observation.label,
                    font(12.5),
                    text_2(),
                    Some(tile_w - 32.0),
                    true,
                ),
                left,
                rect.top() + 26.0,
            );
            let value = paint_on_baseline(
                painter,
                painter.layout_no_wrap(v.observation.value_text.clone(), bold(30.0), text()),
                left,
                rect.top() + 64.0,
            );
            paint_on_baseline(
                painter,
                painter.layout_no_wrap(v.observation.units.clone(), font(12.5), muted()),
                value.right() + 6.0,
                rect.top() + 64.0,
            );
            paint_on_baseline(
                painter,
                painter.layout_no_wrap(
                    format!("{:.0} s ago", (now - v.elapsed_s).max(0.0)),
                    font(11.5),
                    muted(),
                ),
                left,
                rect.top() + 88.0,
            );
            ui.interact(
                rect,
                ui.id().with(("vital", &v.observation.label)),
                Sense::hover(),
            )
            .on_hover_text(&v.observation.source);
        }
        ui.add_space(gap);
    }
}

pub(super) fn trend(ui: &mut egui::Ui, rec: &Recorder, key: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 150.0), Sense::hover());
    let painter = ui.painter_at(rect.expand(2.0));
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + 36.0, rect.top() + 6.0),
        Pos2::new(rect.right(), rect.bottom() - 20.0),
    );
    let end = rec.journal.elapsed().max(60.0);
    let start = end - 60.0;
    let points: Vec<(f64, f64)> = rec
        .history
        .iter()
        .filter(|(t, k, _)| *t >= start && k == key)
        .map(|(t, _, y)| (*t, *y))
        .collect();
    let (mut min, mut max) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), (_, y)| {
            (a.min(*y), b.max(*y))
        });
    if !min.is_finite() {
        min = 0.0;
        max = 1.0;
    }
    if max - min < 4.0 {
        let mid = (max + min) / 2.0;
        min = mid - 2.0;
        max = mid + 2.0;
    }
    for i in 0..=3 {
        let y = plot.top() + i as f32 * plot.height() / 3.0;
        painter.hline(plot.x_range(), y, Stroke::new(1.0, line()));
        let value = max - (max - min) * i as f64 / 3.0;
        painter.text(
            Pos2::new(rect.left(), y),
            Align2::LEFT_CENTER,
            format!("{value:.0}"),
            mono_font(11.0),
            muted(),
        );
    }
    for (label, frac) in [("−60 s", 0.0), ("−30 s", 0.5), ("now", 1.0)] {
        let x = plot.left() + frac * plot.width();
        let align = if frac == 0.0 {
            Align2::LEFT_BOTTOM
        } else if frac == 1.0 {
            Align2::RIGHT_BOTTOM
        } else {
            Align2::CENTER_BOTTOM
        };
        painter.text(
            Pos2::new(x, rect.bottom()),
            align,
            label,
            font(11.0),
            muted(),
        );
    }
    let to_screen = |t: f64, y: f64| {
        Pos2::new(
            plot.left() + ((t - start) / 60.0) as f32 * plot.width(),
            plot.bottom() - ((y - min) / (max - min)) as f32 * plot.height(),
        )
    };
    // Connect samples less than five seconds apart; longer gaps stay visible.
    let mut run: Vec<Pos2> = Vec::new();
    let mut last_t = f64::NEG_INFINITY;
    for &(t, y) in &points {
        if t - last_t > 5.0 {
            if run.len() > 1 {
                painter.add(Shape::line(
                    std::mem::take(&mut run),
                    Stroke::new(2.0, accent()),
                ));
            }
            run.clear();
        }
        run.push(to_screen(t, y));
        last_t = t;
    }
    if run.len() > 1 {
        painter.add(Shape::line(run, Stroke::new(2.0, accent())));
    }
    if let Some(&(t, y)) = points.last() {
        let p = to_screen(t, y);
        painter.circle_filled(p, 4.0, accent());
        painter.circle_stroke(p, 4.0, Stroke::new(2.0, surface()));
    }
}

// ---------------------------------------------------------------------------
// Timeline

pub(super) struct Row {
    pub at: DateTime<Utc>,
    pub color: Color32,
    pub title: String,
    pub body: Option<String>,
    pub meta: Option<String>,
    /// Marker or note (as opposed to lifecycle/media bookkeeping).
    pub marker: bool,
}

/// Build timeline rows, newest first. Without `lifecycle`, starts, stops and
/// connection events are left out.
pub(super) fn rows_from_events(events: &[Event], lifecycle: bool) -> Vec<Row> {
    let mut rows: Vec<Row> = timeline_rows(events)
        .into_iter()
        .filter(|(_, e)| lifecycle || !LIFECYCLE_KINDS.contains(&e.kind.as_str()))
        .map(|(at, e)| {
            let friendly = match e.kind.as_str() {
                "audio_closed" => "Audio saved",
                "start" => "Recording started",
                "stop" => "Recording saved",
                "video_closed" => "Video saved",
                "video_error" => "Video interrupted",
                "audio_error" => "Audio interrupted",
                "photo" => "Photo attached",
                "connection" => "Monitor connected",
                "disconnect" => "Monitor disconnected",
                "annotation" => "Note",
                other => other,
            };
            let title = e.data["label"]
                .as_str()
                .filter(|l| !l.is_empty())
                .unwrap_or(friendly)
                .to_string();
            let note = e.data["note"]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty());
            let marker = matches!(e.kind.as_str(), "marker" | "annotation");
            let color = match e.kind.as_str() {
                "annotation" => note_color(),
                "marker" if note.is_some() || title == "Note" => note_color(),
                "marker" => accent(),
                "audio_closed" | "photo" | "video_closed" => live(),
                "audio_error" | "video_error" | "disconnect" => warn(),
                _ => muted(),
            };
            let mut body = note
                .map(str::to_string)
                .or_else(|| e.data["detail"].as_str().map(str::to_string));
            if e.kind == "audio_closed" {
                body = Some(format!(
                    "{} · {}",
                    format_duration(e.data["duration_s"].as_f64().unwrap_or(0.0)),
                    e.data["device"].as_str().unwrap_or("")
                ));
            }
            let meta = e.data["context"]
                .as_array()
                .filter(|c| !c.is_empty())
                .map(|context| {
                    context
                        .iter()
                        .take(6)
                        .filter_map(|c| {
                            let o = &c["observation"];
                            Some(format!(
                                "{} {}{}",
                                o["label"].as_str()?,
                                o["value_text"].as_str()?,
                                o["units"]
                                    .as_str()
                                    .map(|u| format!(" {u}"))
                                    .unwrap_or_default()
                            ))
                        })
                        .collect::<Vec<_>>()
                        .join("  ·  ")
                });
            // A note's own words make the clearest heading.
            let (title, body) = if title == "Note" && body.is_some() && marker {
                (body.unwrap_or_default(), None)
            } else {
                (title, body)
            };
            Row {
                at,
                color,
                title,
                body,
                meta: meta.filter(|m| !m.is_empty()),
                marker,
            }
        })
        .collect();
    rows.reverse();
    rows
}
fn note_color() -> Color32 {
    theme::note()
}

/// Vertical timeline: time | dot on a rail | title, body, meta | optional Replay.
/// Every column of a row shares the title's baseline.
pub(super) fn timeline_list(
    ui: &mut egui::Ui,
    rows: &[Row],
    replay: bool,
    compact: bool,
) -> Option<DateTime<Utc>> {
    if rows.is_empty() {
        ui.label(muted_text("Markers and notes appear here.", 13.0));
        return None;
    }
    let mut jump = None;
    let width = ui.available_width();
    let pad = if compact { 0.0 } else { 20.0 };
    let time_w = if compact { 58.0 } else { 70.0 };
    let dot_x = pad + time_w + 6.0;
    let text_x = dot_x + 18.0;
    let action_w = if replay { 92.0 } else { 0.0 };
    let text_w = (width - text_x - pad - action_w).max(80.0);
    let title_font = bold(13.5);
    let n = rows.len();
    for (i, row) in rows.iter().enumerate() {
        let painter = ui.painter().clone();
        // Titles wrap rather than truncate; the narrow live rail skips context.
        let title = layout(
            &painter,
            &row.title,
            title_font.clone(),
            text(),
            Some(text_w),
            false,
        );
        let body = row
            .body
            .as_ref()
            .map(|b| layout(&painter, b, font(13.0), text_2(), Some(text_w), false));
        let meta = row
            .meta
            .as_ref()
            .filter(|_| !compact)
            .map(|m| layout(&painter, m, font(12.0), muted(), Some(text_w), false));
        let top_pad = if compact { 8.0 } else { 12.0 };
        let mut height = top_pad + title.size().y.max(18.0);
        if let Some(b) = &body {
            height += 4.0 + b.size().y;
        }
        if let Some(m) = &meta {
            height += 4.0 + m.size().y;
        }
        height += top_pad;
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let hovered = response.hovered();
        if hovered && replay && row.marker && !compact {
            painter.rect_filled(rect.shrink2(Vec2::new(8.0, 0.0)), 8, hover());
        }
        let baseline = rect.top() + top_pad + 13.5;
        let dot_center = Pos2::new(rect.left() + dot_x, baseline - 4.5);
        // Rail connecting the dots.
        let rail = Stroke::new(1.5, line_strong());
        if i > 0 {
            painter.vline(
                dot_center.x,
                Rangef::new(rect.top(), dot_center.y - 7.0),
                rail,
            );
        }
        if i + 1 < n {
            painter.vline(
                dot_center.x,
                Rangef::new(dot_center.y + 7.0, rect.bottom()),
                rail,
            );
        }
        painter.circle_filled(dot_center, 4.5, row.color);
        let local = row
            .at
            .with_timezone(&chrono::Local)
            .format("%H:%M:%S")
            .to_string();
        paint_on_baseline(
            &painter,
            painter.layout_no_wrap(local, mono_font(if compact { 11.5 } else { 12.0 }), muted()),
            rect.left() + pad,
            baseline,
        );
        let title_rect = paint_on_baseline(&painter, title, rect.left() + text_x, baseline);
        let mut y = title_rect.bottom() + 4.0;
        if let Some(b) = body {
            let h = b.size().y;
            painter.galley(Pos2::new(rect.left() + text_x, y), b, text_2());
            y += h + 4.0;
        }
        if let Some(m) = meta {
            painter.galley(Pos2::new(rect.left() + text_x, y), m, muted());
        }
        if replay && row.marker {
            let r = Rect::from_min_size(
                Pos2::new(rect.right() - pad - action_w + 8.0, baseline - 4.5 - 14.0),
                Vec2::new(action_w - 8.0, 28.0),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(r)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if Btn::ghost("Replay")
                .icon(Icon::Play)
                .size(Size::Sm)
                .show(&mut child)
                .on_hover_text("Open the replay at this moment")
                .clicked()
            {
                jump = Some(row.at);
            }
        }
    }
    jump
}

pub(super) fn media_row(ui: &mut egui::Ui, file: &std::path::Path, divider: bool) -> bool {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 56.0), Sense::hover());
    let painter = ui.painter().clone();
    if divider {
        painter.hline(
            Rangef::new(rect.left() + 20.0, rect.right() - 20.0),
            rect.top(),
            Stroke::new(1.0, line()),
        );
    }
    let ext = file
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (kind, icon) = match ext.as_str() {
        "wav" => ("Audio", Icon::Mic),
        "mkv" => ("Video", Icon::Play),
        _ => ("Photo", Icon::Folder),
    };
    let tile = Rect::from_center_size(
        Pos2::new(rect.left() + 38.0, rect.center().y),
        Vec2::splat(32.0),
    );
    painter.rect_filled(tile, 8, raised());
    paint_icon(&painter, tile.shrink(8.0), icon, text_2());
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    let x = tile.right() + 14.0;
    paint_on_baseline(
        &painter,
        layout(
            &painter,
            name,
            bold(13.5),
            text(),
            Some(width - 240.0),
            true,
        ),
        x,
        rect.top() + 25.0,
    );
    paint_on_baseline(
        &painter,
        painter.layout_no_wrap(
            format!("{kind}  ·  {}", format_bytes(size)),
            font(12.0),
            muted(),
        ),
        x,
        rect.top() + 43.0,
    );
    let r = Rect::from_min_max(
        Pos2::new(rect.right() - 120.0, rect.top()),
        Pos2::new(rect.right() - 20.0, rect.bottom()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(r)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    Btn::secondary("Open")
        .size(Size::Sm)
        .show(&mut child)
        .clicked()
}

// ---------------------------------------------------------------------------
// Decoder panels (experimental Waveline fields)

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
            "Receiving"
        } else {
            "Saved arterial sample"
        }
    } else {
        "Not reading yet"
    };
    let red = Color32::from_rgb(232, 72, 64);
    egui::Frame::new()
        .fill(raised())
        .corner_radius(10)
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                ui.label(semibold("Arterial line pressure (ART/P1)", 14.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    chip(ui, status, if has_values { live() } else { warn() });
                });
            });
            ui.add_space(6.0);
            ui.columns(3, |columns| {
                for (index, label) in ["Arterial systolic", "Arterial diastolic", "Arterial mean (MAP)"]
                    .iter()
                    .enumerate()
                {
                    columns[index].spacing_mut().item_spacing.y = 2.0;
                    columns[index].label(muted_text(*label, 12.0));
                    columns[index].label(
                        RichText::new(if values[index].is_empty() { "—" } else { &values[index] })
                            .font(bold(28.0))
                            .color(if has_values { red } else { muted() }),
                    );
                }
            });
            let detail = if sample.is_none() {
                "No decoded P1 sample available yet. Waiting for a new monitor record; older captures may have no P1 decoding."
            } else if stale {
                "No new complete monitor record for more than 3 seconds. Arterial values are hidden until new decoded samples arrive."
            } else if !has_values {
                "The latest decoded arterial sample has no available values. This section stays visible while waiting."
            } else {
                "P1 · i32 LE bytes 36 / 40 / 44. Connected display reference supports these fields. Units and channel-validity flags remain unverified; fresh records can contain unavailable or retained device values."
            };
            ui.add(egui::Label::new(muted_text(detail, 12.0)).wrap());
        });
}

pub(super) fn binary_panel(
    ui: &mut egui::Ui,
    record: &crate::waveline::Record,
    count: u64,
    age: Option<f64>,
) {
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.horizontal(|ui| {
        ui.label(semibold("Experimental binary decoder", 15.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let freshness = age
                .map(|age| {
                    if age > 3.0 {
                        format!("STALE SNAPSHOT · last complete record {age:.1}s ago")
                    } else {
                        format!("Last complete record {age:.1}s ago")
                    }
                })
                .unwrap_or_else(|| "Saved capture snapshot · not live data".into());
            chip(
                ui,
                &freshness,
                if age.is_some_and(|age| age <= 3.0) {
                    live()
                } else {
                    warn()
                },
            );
        });
    });
    ui.add(
        egui::Label::new(muted_text(
            format!(
                "{} complete records · latest record offset {}. Field meanings and waveform calibration are unverified; these values are not used in trends or note context.",
                thousands(count),
                record.stream_offset
            ),
            12.0,
        ))
        .wrap(),
    );
    ui.add_space(10.0);
    arterial_panel(ui, Some(record), age);
    ui.add_space(14.0);
    let width = ui.available_width();
    let columns = ((width / 190.0).floor() as usize).clamp(2, 4);
    let gap = 12.0;
    let tile_w = (width - gap * (columns - 1) as f32) / columns as f32;
    for chunk in record.fields.chunks(columns) {
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, 78.0), Sense::hover());
        for (i, field) in chunk.iter().enumerate() {
            let rect = Rect::from_min_size(
                Pos2::new(row.left() + i as f32 * (tile_w + gap), row.top()),
                Vec2::new(tile_w, 78.0),
            );
            let painter = ui.painter();
            painter.rect(
                rect,
                10,
                surface(),
                Stroke::new(1.0, line()),
                egui::StrokeKind::Inside,
            );
            let left = rect.left() + 12.0;
            paint_on_baseline(
                painter,
                layout(
                    painter,
                    field.hypothesis,
                    font(12.0),
                    text_2(),
                    Some(tile_w - 24.0),
                    true,
                ),
                left,
                rect.top() + 22.0,
            );
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
            paint_on_baseline(
                painter,
                painter.layout_no_wrap(value, bold(20.0), text()),
                left,
                rect.top() + 48.0,
            );
            paint_on_baseline(
                painter,
                painter.layout_no_wrap(
                    format!("{} · byte {}", field.encoding, field.offset),
                    mono_font(10.5),
                    muted(),
                ),
                left,
                rect.top() + 67.0,
            );
            ui.interact(rect, ui.id().with(("field", field.offset)), Sense::hover())
                .on_hover_text(format!("Original bytes: {}", field.raw_hex));
        }
        ui.add_space(gap);
    }
    ui.add_space(6.0);
    ui.label(semibold("Raw signal slices", 14.0));
    ui.label(muted_text(
        "Latest record only · unsigned bytes 0–255 · calibration and sample timing unverified",
        12.0,
    ));
    for signal in &record.signal_slices {
        ui.add_space(4.0);
        ui.label(
            mono(
                format!(
                    "{} · byte {} · {} samples",
                    signal.hypothesis,
                    signal.offset,
                    signal.samples.len()
                ),
                11.5,
            )
            .color(text_2()),
        );
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6, input());
        painter.hline(rect.x_range(), rect.center().y, Stroke::new(1.0, line()));
        let points = signal
            .samples
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                Pos2::new(
                    rect.left()
                        + i as f32 / (signal.samples.len() - 1).max(1) as f32 * rect.width(),
                    rect.bottom() - 4.0 - f32::from(v) / 255.0 * (rect.height() - 8.0),
                )
            })
            .collect();
        painter.add(Shape::line(points, Stroke::new(1.5, accent())));
    }
}
