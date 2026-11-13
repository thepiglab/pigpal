//! Design system: palettes, type, and a small kit of painted components.
//!
//! Everything outside the Waveline monitor canvas is built from these pieces so
//! spacing, heights and baselines line up the same way on every screen.
//!
//! Rules
//! - Spacing comes from a 4 px grid: 4, 8, 12, 16, 24, 32.
//! - Controls are 32 px tall (`Size::Md`), or 38 px for the main call to action.
//! - Corner radii: 6 (chips, small), 8 (controls), 12 (cards), 14 (dialogs).
//! - Type: 22 page title, 15 section, 13.5 body, 12.5 secondary, 11.5 caption,
//!   12 mono for clock times. Rows put every column on one shared baseline.
//! - One accent for actions. Red = recording/destructive, green = live,
//!   amber = stale/attention, violet = notes. Vital-sign colours belong to the
//!   monitor canvas only, which stays dark in both themes like the device.
use eframe::egui::{
    self, text::LayoutJob, Color32, CornerRadius, FontFamily, FontId, Galley, Margin, Painter,
    Pos2, Rect, Response, RichText, Sense, Shape, Stroke, TextFormat, Theme, ThemePreference, Ui,
    Vec2,
};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

// ---------------------------------------------------------------------------
// Palettes

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}
impl Appearance {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];
    fn preference(self) -> ThemePreference {
        match self {
            Self::System => ThemePreference::System,
            Self::Light => ThemePreference::Light,
            Self::Dark => ThemePreference::Dark,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Match system",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Self::System => Icon::Auto,
            Self::Light => Icon::Sun,
            Self::Dark => Icon::Moon,
        }
    }
}

pub struct Palette {
    pub bg: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub input: Color32,
    pub selected: Color32,
    pub line: Color32,
    pub line_strong: Color32,
    pub text: Color32,
    pub text_2: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub on_accent: Color32,
    pub accent_soft: Color32,
    pub selection: Color32,
    pub live: Color32,
    pub warn: Color32,
    pub danger: Color32,
    pub danger_hover: Color32,
    pub on_danger: Color32,
    pub danger_soft: Color32,
    pub note: Color32,
    pub shadow: u8,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(12, 14, 18),
    sidebar: Color32::from_rgb(16, 19, 24),
    surface: Color32::from_rgb(21, 25, 31),
    raised: Color32::from_rgb(29, 34, 42),
    hover: Color32::from_rgb(35, 41, 50),
    input: Color32::from_rgb(16, 19, 24),
    selected: Color32::from_rgb(31, 37, 52),
    line: Color32::from_rgb(35, 41, 50),
    line_strong: Color32::from_rgb(50, 58, 70),
    text: Color32::from_rgb(232, 235, 240),
    text_2: Color32::from_rgb(166, 175, 188),
    muted: Color32::from_rgb(116, 125, 139),
    accent: Color32::from_rgb(132, 160, 255),
    accent_hover: Color32::from_rgb(156, 180, 255),
    on_accent: Color32::from_rgb(9, 14, 30),
    accent_soft: Color32::from_rgb(29, 37, 61),
    selection: Color32::from_rgb(52, 66, 112),
    live: Color32::from_rgb(61, 214, 140),
    warn: Color32::from_rgb(242, 181, 75),
    danger: Color32::from_rgb(255, 99, 104),
    danger_hover: Color32::from_rgb(255, 128, 132),
    on_danger: Color32::from_rgb(32, 6, 8),
    danger_soft: Color32::from_rgb(52, 24, 28),
    note: Color32::from_rgb(190, 152, 255),
    shadow: 150,
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(247, 248, 250),
    sidebar: Color32::from_rgb(241, 243, 246),
    surface: Color32::from_rgb(255, 255, 255),
    raised: Color32::from_rgb(243, 245, 248),
    hover: Color32::from_rgb(235, 238, 243),
    input: Color32::from_rgb(255, 255, 255),
    selected: Color32::from_rgb(230, 236, 252),
    line: Color32::from_rgb(228, 231, 237),
    line_strong: Color32::from_rgb(210, 215, 223),
    text: Color32::from_rgb(22, 26, 33),
    text_2: Color32::from_rgb(74, 83, 97),
    muted: Color32::from_rgb(113, 122, 136),
    accent: Color32::from_rgb(54, 88, 218),
    accent_hover: Color32::from_rgb(44, 74, 196),
    on_accent: Color32::from_rgb(255, 255, 255),
    accent_soft: Color32::from_rgb(232, 238, 255),
    selection: Color32::from_rgb(200, 214, 255),
    live: Color32::from_rgb(20, 138, 82),
    warn: Color32::from_rgb(176, 110, 8),
    danger: Color32::from_rgb(212, 46, 56),
    danger_hover: Color32::from_rgb(188, 34, 44),
    on_danger: Color32::from_rgb(255, 255, 255),
    danger_soft: Color32::from_rgb(253, 236, 237),
    note: Color32::from_rgb(122, 72, 212),
    shadow: 34,
};

static DARK_ACTIVE: AtomicBool = AtomicBool::new(true);

/// The palette for the theme currently on screen.
pub fn palette() -> &'static Palette {
    if DARK_ACTIVE.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}
/// Call once per frame so colour accessors match the theme egui resolved
/// (including the operating-system setting when following "System").
pub fn sync(ctx: &egui::Context) {
    DARK_ACTIVE.store(ctx.theme() == Theme::Dark, Ordering::Relaxed);
}
pub fn set_appearance(ctx: &egui::Context, appearance: Appearance) {
    ctx.set_theme(appearance.preference());
    sync(ctx);
}

macro_rules! accessors {
    ($($name:ident),*) => {$(
        #[inline]
        #[allow(dead_code)]
        pub fn $name() -> Color32 {
            palette().$name
        }
    )*};
}
accessors!(
    bg,
    sidebar,
    surface,
    raised,
    hover,
    input,
    selected,
    line,
    line_strong,
    text,
    text_2,
    muted,
    accent,
    accent_hover,
    on_accent,
    accent_soft,
    live,
    warn,
    danger,
    danger_hover,
    on_danger,
    danger_soft,
    note
);

// ---------------------------------------------------------------------------
// Type

pub const SEMIBOLD: &str = "Semibold";

pub fn semibold_family() -> FontFamily {
    FontFamily::Name(SEMIBOLD.into())
}
pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, semibold_family())
}
pub fn mono_font(size: f32) -> FontId {
    FontId::monospace(size)
}
pub fn semibold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(bold(size))
}
pub fn muted_text(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).size(size).color(muted())
}
pub fn secondary(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).size(size).color(text_2())
}
pub fn mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).monospace().size(size)
}

/// Lay out text, optionally wrapped and/or clipped to one line with an ellipsis.
pub fn layout(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    wrap: Option<f32>,
    one_line: bool,
) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(
        text.to_string(),
        TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
    if let Some(width) = wrap {
        job.wrap.max_width = width.max(1.0);
    }
    if one_line {
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
    }
    painter.layout_job(job)
}
/// Distance from the galley's top to its first baseline.
pub fn ascent(galley: &Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|r| r.glyphs.first())
        .map_or(galley.size().y * 0.78, |g| g.pos.y)
}
/// Paint a galley so its first line sits on `baseline`. Returns its rect.
pub fn paint_on_baseline(painter: &Painter, galley: Arc<Galley>, x: f32, baseline: f32) -> Rect {
    let pos = Pos2::new(x, baseline - ascent(&galley));
    let rect = Rect::from_min_size(pos, galley.size());
    painter.galley(pos, galley, Color32::PLACEHOLDER);
    rect
}
/// Paint a galley right-aligned at `right` on `baseline`.
pub fn paint_on_baseline_right(
    painter: &Painter,
    galley: Arc<Galley>,
    right: f32,
    baseline: f32,
) -> Rect {
    let x = right - galley.size().x;
    paint_on_baseline(painter, galley, x, baseline)
}
/// Baseline for text of `font` vertically centred in a box at `center_y`.
pub fn centered_baseline(painter: &Painter, font: &FontId, center_y: f32) -> f32 {
    let probe = painter.layout_no_wrap("Hg".into(), font.clone(), Color32::WHITE);
    center_y - probe.size().y / 2.0 + ascent(&probe)
}

// ---------------------------------------------------------------------------
// Setup

fn visuals(p: &Palette, dark: bool) -> egui::Visuals {
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.dark_mode = dark;
    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.line_strong);
    v.window_corner_radius = CornerRadius::same(14);
    v.window_shadow = shadow(p.shadow, 40, 12);
    v.popup_shadow = shadow(p.shadow, 24, 8);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = p.input;
    v.faint_bg_color = p.surface;
    v.code_bg_color = p.raised;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.danger;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.striped = false;
    v.collapsing_header_frame = false;

    let w = &mut v.widgets;
    for (state, fill, stroke, fg) in [
        (&mut w.noninteractive, p.surface, p.line, p.text_2),
        (&mut w.inactive, p.raised, p.line, p.text),
        (&mut w.hovered, p.hover, p.line_strong, p.text),
        (&mut w.active, p.line_strong, p.accent, p.text),
        (&mut w.open, p.hover, p.line_strong, p.text),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.fg_stroke = Stroke::new(1.5, fg);
        state.corner_radius = CornerRadius::same(8);
        state.expansion = 0.0;
    }
    v
}
fn shadow(alpha: u8, blur: u8, y: i8) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, y],
        blur,
        spread: 0,
        color: Color32::from_black_alpha(alpha),
    }
}

pub fn configure(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "Inter Semibold".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")).into(),
    );
    let proportional = fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .expect("default proportional family");
    proportional.insert(0, "Inter".into());
    let mut semibold = vec!["Inter Semibold".to_string()];
    semibold.extend(proportional.iter().skip(1).cloned());
    fonts.families.insert(semibold_family(), semibold);
    ctx.set_fonts(fonts);

    for (theme, palette, dark) in [(Theme::Dark, &DARK, true), (Theme::Light, &LIGHT, false)] {
        ctx.set_visuals_of(theme, visuals(palette, dark));
        ctx.style_mut_of(theme, |s| {
            s.spacing.item_spacing = Vec2::new(8.0, 8.0);
            s.spacing.button_padding = Vec2::new(12.0, 7.0);
            s.spacing.interact_size = Vec2::new(32.0, 32.0);
            s.spacing.combo_height = 320.0;
            s.spacing.window_margin = Margin::same(24);
            s.spacing.menu_margin = Margin::same(6);
            s.spacing.icon_width = 16.0;
            s.spacing.icon_width_inner = 8.0;
            s.spacing.icon_spacing = 8.0;
            s.spacing.slider_rail_height = 4.0;
            s.spacing.scroll = egui::style::ScrollStyle::floating();
            s.animation_time = 0.12;
            s.interaction.tooltip_delay = 0.35;
            s.text_styles
                .insert(egui::TextStyle::Body, FontId::proportional(13.5));
            s.text_styles
                .insert(egui::TextStyle::Small, FontId::proportional(11.5));
            s.text_styles.insert(
                egui::TextStyle::Button,
                FontId::new(13.0, semibold_family()),
            );
            s.text_styles
                .insert(egui::TextStyle::Monospace, FontId::monospace(12.5));
            s.text_styles.insert(
                egui::TextStyle::Heading,
                FontId::new(22.0, semibold_family()),
            );
        });
    }
    sync(ctx);
}

// ---------------------------------------------------------------------------
// Icons (16 × 16 grid, stroked at 1.6 px)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Plus,
    Search,
    Settings,
    Folder,
    Sun,
    Moon,
    Auto,
    Play,
    Pause,
    Prev,
    Next,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    More,
    Mic,
    Flag,
    Note,
    Export,
    Stop,
    Check,
    Text,
    Wave,
    Resume,
}

pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 16.0;
    let c = rect.center();
    let p = |x: f32, y: f32| Pos2::new(c.x + x * s, c.y + y * s);
    let stroke = Stroke::new(1.6 * s, color);
    let line = |pts: &[(f32, f32)]| {
        painter.add(Shape::line(
            pts.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    let closed = |pts: &[(f32, f32)]| {
        painter.add(Shape::closed_line(
            pts.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    let filled = |pts: &[(f32, f32)]| {
        painter.add(Shape::convex_polygon(
            pts.iter().map(|&(x, y)| p(x, y)).collect(),
            color,
            Stroke::NONE,
        ));
    };
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| -> Vec<Pos2> {
        (0..=20)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / 20.0;
                p(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    use std::f32::consts::{PI, TAU};
    match icon {
        Icon::Plus => {
            line(&[(-5.5, 0.0), (5.5, 0.0)]);
            line(&[(0.0, -5.5), (0.0, 5.5)]);
        }
        Icon::Search => {
            painter.circle_stroke(p(-1.2, -1.2), 4.6 * s, stroke);
            line(&[(2.3, 2.3), (6.0, 6.0)]);
        }
        Icon::Settings => {
            painter.circle_stroke(c, 2.3 * s, stroke);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                line(&[
                    (4.6 * a.cos(), 4.6 * a.sin()),
                    (6.6 * a.cos(), 6.6 * a.sin()),
                ]);
            }
            painter.circle_stroke(c, 4.8 * s, stroke);
        }
        Icon::Folder => closed(&[
            (-6.5, -4.5),
            (-2.0, -4.5),
            (-0.5, -3.0),
            (6.5, -3.0),
            (6.5, 5.0),
            (-6.5, 5.0),
        ]),
        Icon::Sun => {
            painter.circle_stroke(c, 3.0 * s, stroke);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                line(&[
                    (5.2 * a.cos(), 5.2 * a.sin()),
                    (6.8 * a.cos(), 6.8 * a.sin()),
                ]);
            }
        }
        Icon::Moon => {
            // Crescent: outer arc, then the inner arc back.
            let mut pts = arc(0.0, 0.0, 6.0, -PI * 0.38, PI * 1.38);
            let mut inner = arc(2.6, -2.2, 4.6, PI * 0.95, -PI * 0.12);
            pts.append(&mut inner);
            painter.add(Shape::closed_line(pts, stroke));
        }
        Icon::Auto => {
            painter.circle_stroke(c, 6.0 * s, stroke);
            let mut half = arc(0.0, 0.0, 6.0, PI * 0.5, PI * 1.5);
            half.push(p(0.0, -6.0));
            painter.add(Shape::convex_polygon(half, color, Stroke::NONE));
        }
        Icon::Play => filled(&[(-3.5, -5.5), (5.5, 0.0), (-3.5, 5.5)]),
        Icon::Pause => {
            painter.rect_filled(Rect::from_min_max(p(-4.5, -5.5), p(-1.2, 5.5)), 1.0, color);
            painter.rect_filled(Rect::from_min_max(p(1.2, -5.5), p(4.5, 5.5)), 1.0, color);
        }
        Icon::Prev => {
            painter.rect_filled(Rect::from_min_max(p(-5.5, -5.0), p(-3.6, 5.0)), 0.5, color);
            filled(&[(5.0, -5.0), (5.0, 5.0), (-3.0, 0.0)]);
        }
        Icon::Next => {
            painter.rect_filled(Rect::from_min_max(p(3.6, -5.0), p(5.5, 5.0)), 0.5, color);
            filled(&[(-5.0, -5.0), (3.0, 0.0), (-5.0, 5.0)]);
        }
        Icon::ChevronLeft => line(&[(2.5, -5.0), (-2.5, 0.0), (2.5, 5.0)]),
        Icon::ChevronRight => line(&[(-2.5, -5.0), (2.5, 0.0), (-2.5, 5.0)]),
        Icon::ChevronDown => line(&[(-4.5, -2.0), (0.0, 2.5), (4.5, -2.0)]),
        Icon::More => {
            for x in [-5.0, 0.0, 5.0] {
                painter.circle_filled(p(x, 0.0), 1.5 * s, color);
            }
        }
        Icon::Mic => {
            painter.rect_stroke(
                Rect::from_min_max(p(-2.6, -6.5), p(2.6, 2.0)),
                2.6 * s,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.add(Shape::line(arc(0.0, -0.5, 5.0, 0.0, PI), stroke));
            line(&[(0.0, 4.5), (0.0, 7.0)]);
        }
        Icon::Flag => {
            line(&[(-4.5, -6.5), (-4.5, 7.0)]);
            filled(&[(-4.5, -6.5), (5.5, -3.8), (-4.5, -1.0)]);
        }
        Icon::Note => {
            closed(&[
                (-5.0, -6.5),
                (2.5, -6.5),
                (5.0, -4.0),
                (5.0, 6.5),
                (-5.0, 6.5),
            ]);
            line(&[(-2.5, -1.5), (2.5, -1.5)]);
            line(&[(-2.5, 1.5), (2.5, 1.5)]);
            line(&[(-2.5, 4.0), (0.5, 4.0)]);
        }
        Icon::Export => {
            line(&[(-6.0, 1.5), (-6.0, 6.0), (6.0, 6.0), (6.0, 1.5)]);
            line(&[(0.0, 3.0), (0.0, -6.5)]);
            line(&[(-3.5, -3.0), (0.0, -6.5), (3.5, -3.0)]);
        }
        Icon::Stop => {
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::splat(9.0 * s)),
                2.0 * s,
                color,
            );
        }
        Icon::Check => line(&[(-5.0, 0.5), (-1.5, 4.0), (5.0, -3.5)]),
        Icon::Text => {
            for (y, w) in [(-4.5, 6.0), (-1.5, 6.0), (1.5, 6.0), (4.5, 2.5)] {
                line(&[(-6.0, y), (-6.0 + 2.0 * w, y)]);
            }
        }
        Icon::Wave => line(&[
            (-7.0, 0.0),
            (-4.0, 0.0),
            (-2.5, -5.0),
            (0.5, 5.0),
            (2.5, -2.0),
            (4.0, 0.0),
            (7.0, 0.0),
        ]),
        Icon::Resume => {
            painter.add(Shape::line(
                arc(0.0, 0.0, 5.5, -PI * 0.15, PI * 1.6),
                stroke,
            ));
            filled(&[(4.0, -5.8), (6.8, -1.2), (2.0, -1.6)]);
        }
    }
}

// ---------------------------------------------------------------------------
// Buttons

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Sm,
    Md,
    Lg,
}
impl Size {
    fn height(self) -> f32 {
        match self {
            Size::Sm => 28.0,
            Size::Md => 32.0,
            Size::Lg => 38.0,
        }
    }
    fn font(self) -> FontId {
        bold(match self {
            Size::Sm => 12.5,
            Size::Md => 13.0,
            Size::Lg => 13.5,
        })
    }
    fn pad(self) -> f32 {
        match self {
            Size::Sm => 10.0,
            Size::Md => 12.0,
            Size::Lg => 16.0,
        }
    }
}

/// One button component for the whole app: icon and/or label, fixed heights,
/// painted states. `Btn::new(Kind::Primary, "Save").icon(Icon::Check).show(ui)`.
pub struct Btn {
    kind: Kind,
    label: String,
    icon: Option<Icon>,
    trailing: Option<Icon>,
    size: Size,
    width: Option<f32>,
    enabled: bool,
    selected: bool,
}
impl Btn {
    pub fn new(kind: Kind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            icon: None,
            trailing: None,
            size: Size::Md,
            width: None,
            enabled: true,
            selected: false,
        }
    }
    pub fn primary(label: impl Into<String>) -> Self {
        Self::new(Kind::Primary, label)
    }
    pub fn secondary(label: impl Into<String>) -> Self {
        Self::new(Kind::Secondary, label)
    }
    pub fn ghost(label: impl Into<String>) -> Self {
        Self::new(Kind::Ghost, label)
    }
    pub fn danger(label: impl Into<String>) -> Self {
        Self::new(Kind::Danger, label)
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn trailing(mut self, icon: Icon) -> Self {
        self.trailing = Some(icon);
        self
    }
    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn show(self, ui: &mut Ui) -> Response {
        let font = self.size.font();
        let h = self.size.height();
        let icon_size = if self.size == Size::Sm { 14.0 } else { 16.0 };
        let galley = (!self.label.is_empty()).then(|| {
            ui.painter()
                .layout_no_wrap(self.label.clone(), font.clone(), text())
        });
        let text_w = galley.as_ref().map_or(0.0, |g| g.size().x);
        let gap = 7.0;
        let mut content = text_w;
        if self.icon.is_some() {
            content += icon_size + if galley.is_some() { gap } else { 0.0 };
        }
        if self.trailing.is_some() {
            content += icon_size + gap - 2.0;
        }
        let natural = if galley.is_none() {
            h
        } else {
            content + self.size.pad() * 2.0
        };
        let w = self.width.unwrap_or(natural).max(natural.min(h));
        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(w, h), sense);
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let hovered = self.enabled && response.hovered();
        let pressed = self.enabled && response.is_pointer_button_down_on();
        let (mut fill, mut fg, mut stroke) = match self.kind {
            Kind::Primary => (
                if hovered { accent_hover() } else { accent() },
                on_accent(),
                Stroke::NONE,
            ),
            Kind::Danger => (
                if hovered { danger_hover() } else { danger() },
                on_danger(),
                Stroke::NONE,
            ),
            Kind::Secondary => (
                if hovered { hover() } else { surface() },
                text(),
                Stroke::new(1.0, line_strong()),
            ),
            Kind::Ghost => (
                if hovered || self.selected {
                    hover()
                } else {
                    Color32::TRANSPARENT
                },
                if hovered || self.selected {
                    text()
                } else {
                    text_2()
                },
                Stroke::NONE,
            ),
        };
        if pressed {
            fill = fill.gamma_multiply(0.92);
        }
        if !self.enabled {
            fill = fill.gamma_multiply(0.5);
            fg = fg.gamma_multiply(0.45);
            stroke.color = stroke.color.gamma_multiply(0.6);
        }
        let painter = ui.painter();
        painter.rect(rect, 8, fill, stroke, egui::StrokeKind::Inside);
        if response.has_focus() {
            painter.rect_stroke(
                rect.expand(2.0),
                10,
                Stroke::new(2.0, accent().gamma_multiply(0.6)),
                egui::StrokeKind::Outside,
            );
        }
        let mut x = rect.center().x - content / 2.0;
        if let Some(icon) = self.icon {
            let r = Rect::from_center_size(
                Pos2::new(x + icon_size / 2.0, rect.center().y),
                Vec2::splat(icon_size),
            );
            paint_icon(painter, r, icon, fg);
            x += icon_size + if galley.is_some() { gap } else { 0.0 };
        }
        if let Some(galley) = galley {
            let baseline = centered_baseline(painter, &font, rect.center().y);
            let g = painter.layout_no_wrap(self.label, font, fg);
            paint_on_baseline(painter, g, x, baseline);
            x += galley.size().x + gap - 2.0;
        }
        if let Some(icon) = self.trailing {
            let r = Rect::from_center_size(
                Pos2::new(x + icon_size / 2.0, rect.center().y),
                Vec2::splat(icon_size - 2.0),
            );
            paint_icon(painter, r, icon, fg.gamma_multiply(0.8));
        }
        if self.enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

/// Square icon-only button with a tooltip.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    Btn::ghost("").icon(icon).show(ui).on_hover_text(tooltip)
}

// ---------------------------------------------------------------------------
// Containers and small pieces

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(surface())
        .stroke(Stroke::new(1.0, line()))
        .corner_radius(12)
        .inner_margin(Margin::same(20))
}
pub fn modal_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(surface())
        .stroke(Stroke::new(1.0, line_strong()))
        .corner_radius(14)
        .inner_margin(Margin::same(24))
        .shadow(shadow(palette().shadow.saturating_add(20), 48, 16))
}

/// Pill with a coloured dot, e.g. "● Receiving". 22 px tall.
pub fn chip(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = bold(11.5);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), color);
    let size = Vec2::new(galley.size().x + 26.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 11, color.gamma_multiply(0.14));
    painter.circle_filled(Pos2::new(rect.left() + 11.0, rect.center().y), 3.0, color);
    let baseline = centered_baseline(painter, &font, rect.center().y);
    paint_on_baseline(painter, galley, rect.left() + 18.0, baseline);
    response
}

/// Small neutral label, 20 px tall.
pub fn tag(ui: &mut Ui, text: &str) -> Response {
    let font = bold(10.5);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), text_2());
    let size = Vec2::new(galley.size().x + 12.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 5, raised());
    let baseline = centered_baseline(painter, &font, rect.center().y);
    paint_on_baseline(painter, galley, rect.left() + 6.0, baseline);
    response
}

pub fn dot(ui: &mut Ui, color: Color32, radius: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(radius * 2.0 + 4.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
}

/// Underline tabs with optional counts. Returns true when the selection changed.
pub fn tabs<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    items: &[(T, &str, Option<usize>)],
) -> bool {
    let mut changed = false;
    let height = 38.0;
    let full = ui.available_width();
    let (row, _) = ui.allocate_exact_size(Vec2::new(full, height), Sense::hover());
    let label_font = bold(13.0);
    let count_font = bold(11.0);
    let baseline = centered_baseline(ui.painter(), &label_font, row.top() + 15.0);
    ui.painter()
        .hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, line()));
    let mut x = row.left();
    for (i, (item, label, count)) in items.iter().enumerate() {
        let selected = *value == *item;
        let g = ui
            .painter()
            .layout_no_wrap(label.to_string(), label_font.clone(), text());
        let count_g = count.map(|n| {
            ui.painter()
                .layout_no_wrap(n.to_string(), count_font.clone(), muted())
        });
        let w = g.size().x + count_g.as_ref().map_or(0.0, |c| c.size().x + 14.0);
        let rect = Rect::from_min_size(Pos2::new(x, row.top()), Vec2::new(w, height));
        let response = ui
            .interact(rect, ui.id().with(("tab", i)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let color = if selected || response.hovered() {
            text()
        } else {
            text_2()
        };
        let painter = ui.painter();
        let label_rect = paint_on_baseline(
            painter,
            painter.layout_no_wrap(label.to_string(), label_font.clone(), color),
            x,
            baseline,
        );
        if let Some(c) = count_g {
            let pill = Rect::from_min_size(
                Pos2::new(label_rect.right() + 6.0, row.top() + 6.0),
                Vec2::new(c.size().x + 8.0, 18.0),
            );
            painter.rect_filled(pill, 9, if selected { accent_soft() } else { raised() });
            let b = centered_baseline(painter, &count_font, pill.center().y);
            let c = painter.layout_no_wrap(
                count.unwrap_or(0).to_string(),
                count_font.clone(),
                if selected { accent() } else { muted() },
            );
            paint_on_baseline(painter, c, pill.left() + 4.0, b);
        }
        if selected {
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(x, row.bottom() - 2.0),
                    Pos2::new(x + w, row.bottom()),
                ),
                1,
                accent(),
            );
        }
        if response.clicked() && !selected {
            *value = *item;
            changed = true;
        }
        x += w + 24.0;
    }
    changed
}

/// Rounded checkbox with a label, 20 px tall.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, label: &str) -> Response {
    let font = font(13.5);
    let g = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), text());
    let size = Vec2::new(18.0 + 10.0 + g.size().x, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    let painter = ui.painter();
    let bx = Rect::from_min_size(
        Pos2::new(rect.left(), rect.center().y - 9.0),
        Vec2::splat(18.0),
    );
    let hovered = response.hovered();
    if *checked {
        painter.rect_filled(bx, 5, if hovered { accent_hover() } else { accent() });
        paint_icon(painter, bx.shrink(3.0), Icon::Check, on_accent());
    } else {
        painter.rect(
            bx,
            5,
            input(),
            Stroke::new(1.0, if hovered { muted() } else { line_strong() }),
            egui::StrokeKind::Inside,
        );
    }
    let baseline = centered_baseline(painter, &font, rect.center().y);
    paint_on_baseline(painter, g, bx.right() + 10.0, baseline);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Single-line text field with an optional leading icon. Exactly 34 px tall
/// and as wide as the available space, matching [`select`].
pub fn text_field(
    ui: &mut Ui,
    id_salt: &str,
    text: &mut String,
    hint: &str,
    icon: Option<Icon>,
) -> Response {
    let id = ui.make_persistent_id(id_salt);
    let focused = ui.memory(|m| m.has_focus(id));
    let width = ui.available_width();
    let (rect, background) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());
    ui.painter().rect(
        rect,
        8,
        input(),
        Stroke::new(
            if focused { 1.5 } else { 1.0 },
            if focused {
                accent()
            } else if background.hovered() {
                muted()
            } else {
                line_strong()
            },
        ),
        egui::StrokeKind::Inside,
    );
    if let Some(icon) = icon {
        paint_icon(
            ui.painter(),
            Rect::from_center_size(
                Pos2::new(rect.left() + 17.0, rect.center().y),
                Vec2::splat(15.0),
            ),
            icon,
            muted(),
        );
    }
    let left = if icon.is_some() { 32.0 } else { 11.0 };
    let inner = Rect::from_min_max(
        Pos2::new(rect.left() + left, rect.top()),
        Pos2::new(rect.right() - 11.0, rect.bottom()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let response = child.add(
        egui::TextEdit::singleline(text)
            .id(id)
            .frame(false)
            .margin(Margin::ZERO)
            .font(egui::TextStyle::Body)
            .desired_width(inner.width())
            .hint_text(RichText::new(hint).color(muted())),
    );
    if background.clicked() {
        ui.memory_mut(|m| m.request_focus(id));
    }
    response
}

/// Lay out controls side by side: `left` takes the remaining width and
/// `right` gets a fixed width, separated by a 12 px gap.
pub fn split_row(
    ui: &mut Ui,
    right_width: f32,
    left: impl FnOnce(&mut Ui),
    right: impl FnOnce(&mut Ui),
) {
    let width = ui.available_width();
    let gap = 12.0;
    let left_width = width - right_width - gap;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.allocate_ui_with_layout(
            Vec2::new(left_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(left_width);
                ui.spacing_mut().item_spacing.y = 8.0;
                left(ui);
            },
        );
        ui.allocate_ui_with_layout(
            Vec2::new(right_width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(right_width);
                ui.spacing_mut().item_spacing.y = 8.0;
                right(ui);
            },
        );
    });
}

/// Field label above a control.
pub fn field_label(ui: &mut Ui, label: &str) {
    ui.label(semibold(label, 12.5).color(text_2()));
    ui.add_space(-2.0);
}

/// Dropdown trigger styled like a text field; options are added in `add`.
/// Returns the trigger response.
pub fn select<R>(
    ui: &mut Ui,
    id_salt: &str,
    current: &str,
    width: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> Response {
    let id = ui.make_persistent_id(id_salt);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());
    let open = ui.memory(|m| m.is_popup_open(id));
    let painter = ui.painter();
    painter.rect(
        rect,
        8,
        input(),
        Stroke::new(
            if open { 1.5 } else { 1.0 },
            if open {
                accent()
            } else if response.hovered() {
                muted()
            } else {
                line_strong()
            },
        ),
        egui::StrokeKind::Inside,
    );
    let f = font(13.5);
    let baseline = centered_baseline(painter, &f, rect.center().y);
    let g = layout(painter, current, f, text(), Some(width - 44.0), true);
    paint_on_baseline(painter, g, rect.left() + 11.0, baseline);
    paint_icon(
        painter,
        Rect::from_center_size(
            Pos2::new(rect.right() - 17.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        Icon::ChevronDown,
        muted(),
    );
    if response.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup::popup_below_widget(
        ui,
        id,
        &response,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(width - 12.0);
            add(ui)
        },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Popup menu anchored below `anchor` (toggled when it is clicked).
pub fn menu<R>(
    ui: &mut Ui,
    id_salt: &str,
    anchor: &Response,
    width: f32,
    add: impl FnOnce(&mut Ui) -> R,
) {
    let id = ui.make_persistent_id(id_salt);
    if anchor.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup::popup_below_widget(
        ui,
        id,
        anchor,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(width);
            ui.spacing_mut().item_spacing.y = 2.0;
            add(ui)
        },
    );
}
/// Same as [`menu`], opening above the anchor (for controls near the bottom edge).
pub fn menu_above<R>(
    ui: &mut Ui,
    id_salt: &str,
    anchor: &Response,
    width: f32,
    add: impl FnOnce(&mut Ui) -> R,
) {
    let id = ui.make_persistent_id(id_salt);
    if anchor.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    egui::popup::popup_above_or_below_widget(
        ui,
        id,
        anchor,
        egui::AboveOrBelow::Above,
        egui::PopupCloseBehavior::CloseOnClick,
        |ui| {
            ui.set_min_width(width);
            ui.spacing_mut().item_spacing.y = 2.0;
            add(ui)
        },
    );
}

/// Menu row: optional icon, label, optional second line, optional check mark.
pub fn menu_item(
    ui: &mut Ui,
    icon: Option<Icon>,
    label: &str,
    detail: Option<&str>,
    checked: bool,
) -> Response {
    let width = ui.available_width().max(160.0);
    let height = if detail.is_some() { 48.0 } else { 34.0 };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 6, hover());
    }
    let mut x = rect.left() + 10.0;
    let first_line_center = if detail.is_some() {
        rect.top() + 16.0
    } else {
        rect.center().y
    };
    if let Some(icon) = icon {
        paint_icon(
            painter,
            Rect::from_center_size(Pos2::new(x + 8.0, first_line_center), Vec2::splat(15.0)),
            icon,
            text_2(),
        );
        x += 26.0;
    }
    let f = font(13.5);
    let baseline = centered_baseline(painter, &f, first_line_center);
    paint_on_baseline(
        painter,
        painter.layout_no_wrap(label.into(), f, text()),
        x,
        baseline,
    );
    if let Some(detail) = detail {
        let f = font(12.0);
        paint_on_baseline(
            painter,
            painter.layout_no_wrap(detail.into(), f, muted()),
            x,
            baseline + 17.0,
        );
    }
    if checked {
        paint_icon(
            painter,
            Rect::from_center_size(
                Pos2::new(rect.right() - 16.0, first_line_center),
                Vec2::splat(14.0),
            ),
            Icon::Check,
            accent(),
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Section heading row with an optional trailing note.
pub fn section_title(ui: &mut Ui, title: &str, detail: Option<&str>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    let painter = ui.painter();
    let f = bold(15.0);
    let baseline = centered_baseline(painter, &f, rect.center().y);
    let r = paint_on_baseline(
        painter,
        painter.layout_no_wrap(title.into(), f, text()),
        rect.left(),
        baseline,
    );
    if let Some(detail) = detail {
        paint_on_baseline(
            painter,
            painter.layout_no_wrap(detail.into(), font(12.5), muted()),
            r.right() + 8.0,
            baseline,
        );
    }
}
