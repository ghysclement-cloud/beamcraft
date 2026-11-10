//! Petits éléments d'interface « maison », dessinés pour coller à la maquette.

use eframe::egui::{self, Align2, Color32, CornerRadius, ImageSource, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::{font, font_bold, Palette};

/// Les icônes (SVG intégrés au programme).
pub mod icons {
    use eframe::egui::{include_image, ImageSource};
    pub const POTEAU: ImageSource<'static> = include_image!("../assets/icons/poteau.svg");
    pub const POUTRE: ImageSource<'static> = include_image!("../assets/icons/poutre.svg");
    pub const CONTREVENT: ImageSource<'static> = include_image!("../assets/icons/contrevent.svg");
    pub const PLATINE: ImageSource<'static> = include_image!("../assets/icons/platine.svg");
    pub const AXES: ImageSource<'static> = include_image!("../assets/icons/axes.svg");
    pub const NIVEAU: ImageSource<'static> = include_image!("../assets/icons/niveau.svg");
    pub const OUVRIR: ImageSource<'static> = include_image!("../assets/icons/ouvrir.svg");
    pub const EXEMPLE: ImageSource<'static> = include_image!("../assets/icons/exemple.svg");
    pub const VUE_ISO: ImageSource<'static> = include_image!("../assets/icons/vue_iso.svg");
    pub const VUE_DESSUS: ImageSource<'static> = include_image!("../assets/icons/vue_dessus.svg");
    pub const VUE_FACE: ImageSource<'static> = include_image!("../assets/icons/vue_face.svg");
    pub const VUE_DROITE: ImageSource<'static> = include_image!("../assets/icons/vue_droite.svg");
    pub const CADRER: ImageSource<'static> = include_image!("../assets/icons/cadrer.svg");
    pub const LOGO: ImageSource<'static> = include_image!("../assets/icons/logo.svg");
    pub const NAV_CADRER: ImageSource<'static> = include_image!("../assets/icons/nav_cadrer.svg");
    pub const NAV_ORTHO: ImageSource<'static> = include_image!("../assets/icons/nav_ortho.svg");
    pub const NAV_ISO: ImageSource<'static> = include_image!("../assets/icons/nav_iso.svg");
    pub const OEIL: ImageSource<'static> = include_image!("../assets/icons/oeil.svg");
    pub const OEIL_FERME: ImageSource<'static> = include_image!("../assets/icons/oeil_ferme.svg");
}

/// Titre de section en petites capitales : « MODÈLE », « PROPRIÉTÉS »…
pub fn section_label(ui: &mut Ui, text: &str, p: &Palette) {
    ui.label(
        egui::RichText::new(text)
            .font(font_bold(11.0))
            .color(p.muted)
            .extra_letter_spacing(0.8),
    );
}

/// Onglet du ruban souligné quand il est actif.
pub fn tab(ui: &mut Ui, label: &str, active: bool, enabled: bool, p: &Palette) -> Response {
    let f = if active { font_bold(13.0) } else { font(13.0) };
    let galley = ui.painter().layout_no_wrap(label.to_string(), f, p.text);
    let size = galley.size() + Vec2::new(28.0, 20.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let color = if active {
        p.strong
    } else if !enabled {
        p.muted.gamma_multiply(0.7)
    } else if resp.hovered() {
        p.text
    } else {
        p.muted
    };
    ui.painter()
        .galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, color);
    if active {
        let y = rect.bottom() - 1.0;
        ui.painter().line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(2.0, p.accent),
        );
    }
    resp
}

/// Gros bouton du ruban : icône 3D au-dessus, libellé dessous.
pub fn ribbon_button(
    ui: &mut Ui,
    icon: ImageSource<'static>,
    label: &str,
    active: bool,
    enabled: bool,
    p: &Palette,
) -> Response {
    let size = Vec2::new(76.0, 64.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let painter = ui.painter_at(rect);
    let hovered = enabled && resp.hovered();
    if active || hovered {
        painter.rect(
            rect.shrink(1.0),
            CornerRadius::same(6),
            p.sel_bg,
            Stroke::new(1.0, if active { p.accent } else { p.btn_border }),
            egui::StrokeKind::Inside,
        );
    }
    let icon_rect = Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 22.0), Vec2::splat(30.0));
    let mut img = egui::Image::new(icon).fit_to_exact_size(Vec2::splat(30.0));
    if !enabled {
        img = img.tint(Color32::from_white_alpha(110));
    }
    img.paint_at(ui, icon_rect);
    let color = if !enabled {
        p.muted
    } else if active {
        p.strong
    } else {
        p.text
    };
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 13.0),
        Align2::CENTER_CENTER,
        label,
        font(12.0),
        color,
    );
    resp
}

/// Groupe de boutons du ruban avec sa légende dessous.
pub fn ribbon_group(ui: &mut Ui, title: &str, p: &Palette, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
        let row = ui.horizontal(add).response.rect;
        let (r, _) = ui.allocate_exact_size(Vec2::new(row.width(), 14.0), Sense::hover());
        ui.painter()
            .text(r.center(), Align2::CENTER_CENTER, title, font(11.0), p.muted);
    });
}

/// Séparateur vertical entre deux groupes du ruban.
pub fn vsep(ui: &mut Ui, p: &Palette, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(9.0, height), Sense::hover());
    ui.painter()
        .line_segment([rect.center_top(), rect.center_bottom()], Stroke::new(1.0, p.border));
}

/// Ligne de propriété : libellé grisé à gauche, valeur dans un cadre à droite.
pub fn prop_row(ui: &mut Ui, label: &str, value: &str, mono: bool, p: &Palette) {
    ui.horizontal(|ui| {
        let (lr, _) = ui.allocate_exact_size(Vec2::new(96.0, 28.0), Sense::hover());
        ui.painter()
            .text(lr.left_center(), Align2::LEFT_CENTER, label, font(13.0), p.muted);
        let w = ui.available_width().max(60.0);
        let (vr, _) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::hover());
        ui.painter().rect(
            vr,
            CornerRadius::same(6),
            p.input_bg,
            Stroke::new(1.0, p.input_border),
            egui::StrokeKind::Inside,
        );
        let f = if mono {
            crate::theme::font_mono(12.5)
        } else {
            font(13.0)
        };
        ui.painter().text(
            vr.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            value,
            f,
            p.text,
        );
    });
}

/// Petit bouton icône (barre d'outils de la vue, œil de visibilité…).
pub fn icon_button(
    ui: &mut Ui,
    icon: ImageSource<'static>,
    size: f32,
    active: bool,
    tooltip: &str,
    p: &Palette,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if active || resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), if active { p.sel_bg } else { p.btn_bg });
    }
    let color = if active || resp.hovered() { p.strong } else { p.text2 };
    egui::Image::new(icon)
        .tint(color)
        .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(size * 0.5)));
    resp.on_hover_text(tooltip)
}
