//! Visual design tokens and egui theme wiring.
//!
//! Follows the `design-rules` library in the repository root:
//! - 8pt spacing scale with 4pt half-step (`01-layout-spacing-grids.md`)
//! - type scale with named roles, no ad hoc sizes (`02-typography.md`)
//! - semantic color roles, 4.5:1 text contrast, accent reserved for action,
//!   dark mode as a redesign not a filter (`03-color.md`)
//! - one primary action per view, empty/loading/error states designed
//!   (`04-visual-hierarchy.md`, `05-components-forms-states.md`)

use eframe::egui;

/// Spacing scale: multiples of 8 with a 4 half-step (Rule 1.1).
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 16.0;
    pub const L: f32 = 24.0;

    /// Integer variants for egui `Margin` (i8), same 8pt values.
    pub const I_S: i8 = 8;
    pub const I_M: i8 = 16;
    pub const I_L: i8 = 24;

    /// Integer variant for egui `CornerRadius` (u8).
    pub const I_RADIUS: u8 = 8;
}

/// Type scale on a ~1.25 modular ratio, snapped to the 4pt grid (Rule 2.4).
/// Body text 16px sits in the 15 to 25px sweet spot (Rule 2.1).
pub mod type_scale {
    pub const CAPTION: f32 = 12.0;
    pub const BODY: f32 = 16.0;
    pub const TITLE: f32 = 20.0;
    pub const HEADLINE: f32 = 24.0;
}

/// Semantic color roles (Rule 3.3), dark-first (Rule 3.6):
/// desaturated surfaces, never pure black/white, all text pairs ≥ 4.5:1.
#[derive(Clone, Copy)]
#[allow(dead_code)] // full role set kept for future light theme
pub struct Palette {
    /// 60% dominant: window background.
    pub surface: egui::Color32,
    /// Raised surfaces: cards, panels, headerbar.
    pub surface_high: egui::Color32,
    /// 30% structural: borders, dividers, hovered rows.
    pub outline: egui::Color32,
    /// Primary text on `surface` (contrast ≈ 13:1).
    pub on_surface: egui::Color32,
    /// Muted secondary text, still ≥ 4.5:1 on `surface`.
    pub on_surface_muted: egui::Color32,
    /// 10% accent, spent only on the primary action and active states (Rule 3.5).
    pub accent: egui::Color32,
    /// Text on `accent` (contrast ≥ 4.5:1).
    pub on_accent: egui::Color32,
    pub success: egui::Color32,
    pub error: egui::Color32,
    pub warning: egui::Color32,
}

impl Palette {
    pub const DARK: Self = Self {
        surface: egui::Color32::from_rgb(0x1b, 0x1b, 0x1f),
        surface_high: egui::Color32::from_rgb(0x26, 0x26, 0x2b),
        outline: egui::Color32::from_rgb(0x3d, 0x3d, 0x44),
        on_surface: egui::Color32::from_rgb(0xe6, 0xe6, 0xea),
        on_surface_muted: egui::Color32::from_rgb(0x9a, 0x9a, 0xa6),
        accent: egui::Color32::from_rgb(0x7c, 0xb4, 0xff),
        on_accent: egui::Color32::from_rgb(0x0a, 0x22, 0x3a),
        success: egui::Color32::from_rgb(0x6f, 0xd6, 0x9b),
        error: egui::Color32::from_rgb(0xff, 0x8a, 0x80),
        warning: egui::Color32::from_rgb(0xff, 0xd1, 0x86),
    };
}

/// Apply the token system to the egui context.
pub fn install(ctx: &egui::Context) {
    let p = Palette::DARK;
    let mut style = (*ctx.style()).clone();

    // Spacing: every value from the 8pt scale (Rule 1.1); intra-component
    // padding one step smaller than inter-component gaps (Rule 1.2).
    style.spacing.item_spacing = egui::vec2(space::M, space::S);
    style.spacing.button_padding = egui::vec2(space::M, space::S);
    style.spacing.menu_margin = egui::Margin::same(space::I_S);
    style.spacing.window_margin = egui::Margin::same(space::I_L);
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.interact_size = egui::vec2(48.0, 40.0); // ≥ 24×24 target (Rule 5.5)

    // Geometry: soft corners, hairline strokes.
    let rounding = egui::CornerRadius::same(space::I_RADIUS);
    style.visuals.window_corner_radius = rounding;
    style.visuals.menu_corner_radius = rounding;
    for w in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        w.corner_radius = rounding;
    }

    // Color roles.
    style.visuals.panel_fill = p.surface;
    style.visuals.extreme_bg_color = p.surface_high;
    style.visuals.faint_bg_color = p.surface_high; // striped rows
    style.visuals.window_stroke = egui::Stroke::new(1.0_f32, p.outline);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, p.on_surface);
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, p.outline);
    style.visuals.widgets.noninteractive.weak_bg_fill = p.surface_high;
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, p.on_surface);
    style.visuals.widgets.inactive.weak_bg_fill = p.surface_high;
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, p.on_surface);
    style.visuals.widgets.hovered.weak_bg_fill = p.outline;
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, p.on_surface);

    // Accent reserved for selection/focus/links (Rule 3.5).
    style.visuals.selection.bg_fill = p.accent.gamma_multiply(0.35);
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, p.accent);
    style.visuals.hyperlink_color = p.accent;

    style.visuals.override_text_color = Some(p.on_surface);
    let spacing = style.spacing;
    ctx.set_visuals(style.visuals);
    ctx.all_styles_mut(move |s| s.spacing = spacing.clone());

    // Type scale (Rule 2.4): named roles only, body 16px.
    ctx.all_styles_mut(|s| {
        s.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::proportional(type_scale::HEADLINE),
        );
        s.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::proportional(type_scale::BODY),
        );
        s.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(type_scale::BODY),
        );
        s.text_styles.insert(
            egui::TextStyle::Name("Title".into()),
            egui::FontId::proportional(type_scale::TITLE),
        );
        s.text_styles.insert(
            egui::TextStyle::Name("Caption".into()),
            egui::FontId::proportional(type_scale::CAPTION),
        );
        s.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::monospace(type_scale::BODY),
        );
    });
}
