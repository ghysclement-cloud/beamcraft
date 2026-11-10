//! Thème visuel de BEAMCraft : couleurs de la maquette, polices, réglages egui.

use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};

/// Palette de couleurs (reprise de la maquette, thèmes sombre et clair).
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub chrome: Color32,
    pub panel: Color32,
    pub ribbon: Color32,
    pub view: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub muted: Color32,
    pub strong: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub sel_bg: Color32,
    pub btn_bg: Color32,
    pub btn_border: Color32,
    pub input_bg: Color32,
    pub input_border: Color32,
    pub ok: Color32,
    pub warn: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl Palette {
    pub const SOMBRE: Palette = Palette {
        dark: true,
        bg: rgb(0x14181D),
        chrome: rgb(0x0F1216),
        panel: rgb(0x1B2027),
        ribbon: rgb(0x1B2027),
        view: rgb(0x12161B),
        border: rgb(0x2A313A),
        text: rgb(0xE6E9ED),
        text2: rgb(0xC9D0D7),
        muted: rgb(0x9AA3AD),
        strong: rgb(0xFFFFFF),
        accent: rgb(0x4C9AFF),
        accent_text: rgb(0x7DB4FF),
        sel_bg: rgb(0x1F3352),
        btn_bg: rgb(0x222A33),
        btn_border: rgb(0x3A4450),
        input_bg: rgb(0x14181D),
        input_border: rgb(0x2F3741),
        ok: rgb(0x7FD1A0),
        warn: rgb(0xE8B36A),
    };

    pub const CLAIR: Palette = Palette {
        dark: false,
        bg: rgb(0xF3F4F6),
        chrome: rgb(0xFFFFFF),
        panel: rgb(0xFFFFFF),
        ribbon: rgb(0xF7F8FA),
        view: rgb(0xEDF0F4),
        border: rgb(0xD5DAE0),
        text: rgb(0x1C2128),
        text2: rgb(0x333B45),
        muted: rgb(0x5B6470),
        strong: rgb(0x0E1116),
        accent: rgb(0x1F6FD6),
        accent_text: rgb(0x155AB0),
        sel_bg: rgb(0xDCE9FB),
        btn_bg: rgb(0xEEF1F5),
        btn_border: rgb(0xC9D0D8),
        input_bg: rgb(0xFFFFFF),
        input_border: rgb(0xC9D0D8),
        ok: rgb(0x1E7F4A),
        warn: rgb(0x9A5800),
    };

    pub fn get(dark: bool) -> Palette {
        if dark {
            Self::SOMBRE
        } else {
            Self::CLAIR
        }
    }
}

/// Famille de police « demi-gras » (Inter SemiBold).
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn font_bold(size: f32) -> FontId {
    FontId::new(size, semibold())
}

pub fn font_mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Charge Inter et JetBrains Mono (les polices par défaut restent en secours).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-SemiBold.ttf"
        ))),
    );
    fonts.font_data.insert(
        "jbmono".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMono-Regular.ttf"
        ))),
    );
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".into());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "jbmono".into());
    let mut semi = vec!["inter-semibold".to_string(), "inter".to_string()];
    semi.extend(fallback);
    fonts.families.insert(semibold(), semi);
    ctx.set_fonts(fonts);
}

/// Applique la palette à tous les widgets egui.
pub fn apply(ctx: &egui::Context, p: &Palette) {
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let r = CornerRadius::same(6);

    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(8);
    v.extreme_bg_color = p.input_bg;
    v.text_edit_bg_color = Some(p.input_bg);
    v.faint_bg_color = p.bg;
    v.code_bg_color = p.btn_bg;
    v.hyperlink_color = p.accent_text;
    v.warn_fg_color = p.warn;
    v.selection.bg_fill = p.sel_bg;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.override_text_color = None;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = p.btn_bg;
    w.inactive.weak_bg_fill = p.btn_bg;
    w.inactive.bg_stroke = Stroke::new(1.0, p.btn_border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text2);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = p.sel_bg;
    w.hovered.weak_bg_fill = p.sel_bg;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    w.hovered.fg_stroke = Stroke::new(1.5, p.strong);
    w.hovered.corner_radius = r;

    w.active.bg_fill = p.sel_bg;
    w.active.weak_bg_fill = p.sel_bg;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(2.0, p.strong);
    w.active.corner_radius = r;

    w.open = w.active;

    ctx.set_visuals(v);

    ctx.global_style_mut(|s| {
        s.text_styles.insert(TextStyle::Body, font(13.0));
        s.text_styles.insert(TextStyle::Button, font(13.0));
        s.text_styles.insert(TextStyle::Small, font(11.0));
        s.text_styles.insert(TextStyle::Heading, font_bold(16.0));
        s.text_styles.insert(TextStyle::Monospace, font_mono(12.0));
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
    });
}
