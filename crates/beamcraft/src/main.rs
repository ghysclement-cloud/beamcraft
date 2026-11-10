//! # BEAMCraft — application de bureau
//!
//! Disposition reprise de la maquette : en-tête, onglets et ruban, arborescence
//! à gauche, vue 3D au centre, propriétés à droite, barre d'état en bas.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // pas de console sous Windows

mod theme;
mod widgets;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use bc_kernel::{KernelError, Model};
use bc_viewer::{StandardView, ViewStyle, Viewport};
use eframe::egui::{self, Align2, Color32, CornerRadius, Frame, Margin, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use theme::{font, font_bold, Palette};
use widgets::{icons, prop_row, ribbon_button, ribbon_group, section_label, tab, vsep};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BEAMCraft")
            .with_inner_size([1440.0, 920.0])
            .with_min_inner_size([1000.0, 640.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("BEAMCraft", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

/// Résultat d'un chargement fait en arrière-plan.
type LoadResult = (PathBuf, Result<Model, KernelError>, Duration);

struct App {
    viewport: Viewport,
    texture: Option<egui::TextureId>,
    model: Option<Model>,
    visible: Vec<bool>,
    selected: Option<usize>,
    /// Groupes repliés dans l'arborescence.
    collapsed: HashSet<String>,
    dark: bool,
    loading: Option<(PathBuf, Receiver<LoadResult>)>,
    status: String,
    error: Option<String>,
    command: String,
    /// Fichier passé en ligne de commande (`beamcraft piece.step` ou `--exemple`).
    startup_arg: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::install_fonts(&cc.egui_ctx);
        let rs = cc.wgpu_render_state.as_ref().expect("BEAMCraft a besoin de wgpu");
        let app = App {
            viewport: Viewport::new(&rs.device),
            texture: None,
            model: None,
            visible: Vec::new(),
            selected: None,
            collapsed: HashSet::new(),
            // BEAMCRAFT_THEME=clair pour démarrer en thème clair
            dark: std::env::var("BEAMCRAFT_THEME").map(|t| t != "clair").unwrap_or(true),
            loading: None,
            status: "Prêt".into(),
            error: None,
            command: String::new(),
            startup_arg: std::env::args().nth(1),
        };
        theme::apply(&cc.egui_ctx, &app.pal());
        app
    }

    fn pal(&self) -> Palette {
        Palette::get(self.dark)
    }

    fn toggle_theme(&mut self, ctx: &egui::Context) {
        self.dark = !self.dark;
        theme::apply(ctx, &self.pal());
    }

    /// Lance la lecture d'un STEP dans un fil séparé (l'interface reste fluide).
    fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let p = path.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let r = bc_kernel::load_step(&p);
            let _ = tx.send((p, r, t0.elapsed()));
            ctx.request_repaint();
        });
        self.status = format!("Lecture de {}…", file_name(&path));
        self.loading = Some((path, rx));
        self.error = None;
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Ouvrir un fichier STEP")
            .add_filter("STEP", &["step", "stp", "STEP", "STP"])
            .pick_file()
        {
            self.open(path, ctx);
        }
    }

    fn load_sample(&mut self, frame: &eframe::Frame) {
        let m = bc_kernel::sample_portal();
        self.status = format!("Exemple chargé · {} pièces", m.parts.len());
        self.set_model(frame, m);
    }

    fn set_model(&mut self, frame: &eframe::Frame, model: Model) {
        if let Some(rs) = frame.wgpu_render_state() {
            self.viewport.set_model(&rs.device, &model);
        }
        self.visible = vec![true; model.parts.len()];
        self.collapsed.clear();
        self.select(None);
        self.model = Some(model);
    }

    fn poll_loading(&mut self, frame: &eframe::Frame) {
        let Some((_, rx)) = &self.loading else { return };
        if let Ok((path, result, dt)) = rx.try_recv() {
            self.loading = None;
            match result {
                Ok(model) => {
                    self.status = format!(
                        "{} ouvert en {:.1} s · {} pièce(s) · {} triangles",
                        file_name(&path),
                        dt.as_secs_f32(),
                        model.parts.len(),
                        model.triangle_count()
                    );
                    self.set_model(frame, model);
                }
                Err(e) => {
                    self.status = "Échec de l'ouverture".into();
                    self.error = Some(e.to_string());
                }
            }
        }
    }

    fn select(&mut self, i: Option<usize>) {
        self.selected = i;
        self.viewport.set_highlight(i);
    }

    fn set_view(&mut self, v: StandardView) {
        self.viewport.camera.set_view(v);
        self.viewport.fit();
    }

    fn set_visible(&mut self, i: usize, v: bool) {
        self.visible[i] = v;
        self.viewport.set_part_visible(i, v);
    }

    /// La barre de commande : « ouvrir », « iso », « cadrer »…
    fn run_command(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let cmd = self.command.trim().to_lowercase();
        self.command.clear();
        match cmd.as_str() {
            "" => {}
            "ouvrir" | "open" | "step" => self.open_dialog(ctx),
            "exemple" | "portique" => self.load_sample(frame),
            "cadrer" | "f" | "zoom" => self.viewport.fit(),
            "iso" => self.set_view(StandardView::Iso),
            "dessus" | "haut" => self.set_view(StandardView::Dessus),
            "face" | "avant" => self.set_view(StandardView::Face),
            "droite" => self.set_view(StandardView::Droite),
            "ortho" => self.viewport.camera.orthographic = !self.viewport.camera.orthographic,
            "theme" | "thème" | "clair" | "sombre" => self.toggle_theme(ctx),
            "tout" | "afficher tout" => {
                for i in 0..self.visible.len() {
                    self.set_visible(i, true);
                }
            }
            other => self.status = format!("Commande inconnue : « {other} »"),
        }
    }
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// Formate un nombre de mm à la française : 8 000.
fn mm(v: f32) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{202F}');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// « Cornière 40X40X4(1)[2] » → (« Cornière 40X40X4(1) », « [2] »)
fn split_instance(name: &str) -> (&str, &str) {
    if name.ends_with(']') {
        if let Some(i) = name.rfind('[') {
            return (name[..i].trim_end(), &name[i..]);
        }
    }
    (name, "")
}

/// Regroupe les pièces de même nom de base, dans l'ordre d'apparition.
fn groups(model: &Model) -> Vec<(String, Vec<usize>)> {
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, p) in model.parts.iter().enumerate() {
        let (base, _) = split_instance(&p.name);
        match out.iter_mut().find(|(b, _)| b == base) {
            Some((_, v)) => v.push(i),
            None => out.push((base.to_string(), vec![i])),
        }
    }
    out
}

fn point_in_poly(p: Pos2, poly: &[Pos2]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        if let Some(arg) = self.startup_arg.take() {
            if arg == "--exemple" {
                self.load_sample(frame);
            } else if !arg.starts_with('-') {
                self.open(PathBuf::from(arg), ctx);
            }
        }
        self.poll_loading(frame);

        // Fichiers glissés-déposés sur la fenêtre.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if let Some(p) = dropped.into_iter().next() {
            self.open(p, ctx);
        }

        // Raccourcis clavier.
        let typing = ctx.egui_wants_keyboard_input();
        let (open_k, fit_k, esc_k) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::O),
                !typing && i.key_pressed(egui::Key::F),
                !typing && i.key_pressed(egui::Key::Escape),
            )
        });
        if open_k {
            self.open_dialog(ctx);
        }
        if fit_k {
            self.viewport.fit();
        }
        if esc_k {
            self.select(None);
        }

        let p = self.pal();
        self.header(root, ctx, frame, &p);
        self.ribbon(root, ctx, frame, &p);
        self.status_bar(root, &p);
        self.tree(root, &p);
        self.properties(root, &p);
        self.view3d(root, ctx, frame, &p);

        if let Some(err) = self.error.clone() {
            egui::Window::new("Impossible d'ouvrir le fichier")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(err);
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        self.error = None;
                    }
                });
        }
    }
}

impl App {
    // ---------- En-tête : logo, fichier, menus, commande, thème ----------
    fn header(&mut self, root: &mut egui::Ui, ctx: &egui::Context, frame: &eframe::Frame, p: &Palette) {
        egui::Panel::top("entete")
            .frame(Frame::new().fill(p.chrome).inner_margin(Margin::symmetric(16, 8)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Image::new(icons::LOGO)
                            .tint(p.accent)
                            .fit_to_exact_size(Vec2::splat(22.0)),
                    );
                    ui.label(RichText::new("BEAMCraft").font(font_bold(15.0)).color(p.text));
                    ui.add_space(8.0);
                    let titre = self
                        .model
                        .as_ref()
                        .map(|m| file_name(&m.source))
                        .unwrap_or_else(|| "Aucun fichier ouvert".into());
                    ui.label(RichText::new(titre).color(p.muted));
                    ui.add_space(8.0);

                    let menu_style = ui.style_mut();
                    menu_style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                    menu_style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
                    ui.menu_button(RichText::new("Fichier").color(p.text2), |ui| {
                        if ui.button("Ouvrir un STEP…   ⌘O").clicked() {
                            ui.close();
                            self.open_dialog(ctx);
                        }
                        if ui.button("Charger l'exemple (portique)").clicked() {
                            ui.close();
                            self.load_sample(frame);
                        }
                    });
                    ui.menu_button(RichText::new("Affichage").color(p.text2), |ui| {
                        for (l, v) in [
                            ("Vue isométrique", StandardView::Iso),
                            ("Vue de dessus", StandardView::Dessus),
                            ("Vue de face", StandardView::Face),
                            ("Vue de droite", StandardView::Droite),
                        ] {
                            if ui.button(l).clicked() {
                                ui.close();
                                self.set_view(v);
                            }
                        }
                        ui.separator();
                        ui.checkbox(&mut self.viewport.camera.orthographic, "Projection orthographique");
                        if ui
                            .button(if self.dark {
                                "Passer en thème clair"
                            } else {
                                "Passer en thème sombre"
                            })
                            .clicked()
                        {
                            ui.close();
                            self.toggle_theme(ctx);
                        }
                    });
                    ui.menu_button(RichText::new("Aide").color(p.text2), |ui| {
                        ui.label(RichText::new("BEAMCraft — phase 0").font(font_bold(13.0)));
                        ui.label("Commandes : ouvrir, exemple, iso, dessus, face, droite, cadrer, ortho, thème, tout");
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if self.dark { "Thème clair" } else { "Thème sombre" };
                        if ui.button(RichText::new(label).font(font(12.0))).clicked() {
                            self.toggle_theme(ctx);
                        }
                        let edit = egui::TextEdit::singleline(&mut self.command)
                            .hint_text("Tape une commande… (ex. ouvrir, iso, cadrer)")
                            .desired_width(300.0)
                            .margin(Margin::symmetric(10, 6));
                        let r = ui.add(edit);
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.run_command(ctx, frame);
                        }
                        ui.label(RichText::new("Commande").color(p.muted));
                    });
                });
            });
    }

    // ---------- Onglets + ruban ----------
    fn ribbon(&mut self, root: &mut egui::Ui, ctx: &egui::Context, frame: &eframe::Frame, p: &Palette) {
        egui::Panel::top("onglets")
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(12, 0)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let _ = tab(ui, "Modélisation", true, true, p);
                    for t in ["Assemblages", "Numérotation", "Débit & CN", "Plans", "Échanges IFC"] {
                        tab(ui, t, false, false, p).on_hover_text("Arrive dans une prochaine phase");
                    }
                });
            });
        egui::Panel::top("ruban")
            .frame(Frame::new().fill(p.ribbon).inner_margin(Margin::symmetric(12, 8)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ribbon_group(ui, "Fichier", p, |ui| {
                        if ribbon_button(ui, icons::OUVRIR, "Ouvrir STEP", false, true, p)
                            .on_hover_text("⌘O")
                            .clicked()
                        {
                            self.open_dialog(ctx);
                        }
                        if ribbon_button(ui, icons::EXEMPLE, "Exemple", false, true, p).clicked() {
                            self.load_sample(frame);
                        }
                    });
                    vsep(ui, p, 80.0);
                    ribbon_group(ui, "Vues", p, |ui| {
                        for (icon, l, v) in [
                            (icons::VUE_ISO, "Iso", StandardView::Iso),
                            (icons::VUE_DESSUS, "Dessus", StandardView::Dessus),
                            (icons::VUE_FACE, "Face", StandardView::Face),
                            (icons::VUE_DROITE, "Droite", StandardView::Droite),
                        ] {
                            if ribbon_button(ui, icon, l, false, true, p).clicked() {
                                self.set_view(v);
                            }
                        }
                        if ribbon_button(ui, icons::CADRER, "Cadrer", false, true, p)
                            .on_hover_text("F")
                            .clicked()
                        {
                            self.viewport.fit();
                        }
                    });
                    vsep(ui, p, 80.0);
                    ribbon_group(ui, "Éléments", p, |ui| {
                        for (icon, l) in [
                            (icons::POTEAU, "Poteau"),
                            (icons::POUTRE, "Poutre"),
                            (icons::CONTREVENT, "Contrevent."),
                            (icons::PLATINE, "Platine"),
                        ] {
                            ribbon_button(ui, icon, l, false, false, p)
                                .on_hover_text("Arrive avec la phase 2 (objets acier)");
                        }
                    });
                    vsep(ui, p, 80.0);
                    ribbon_group(ui, "Repères", p, |ui| {
                        for (icon, l) in [(icons::AXES, "Axes"), (icons::NIVEAU, "Niveau")] {
                            ribbon_button(ui, icon, l, false, false, p).on_hover_text("Arrive avec la phase 2");
                        }
                    });
                });
            });
    }

    // ---------- Barre d'état ----------
    fn status_bar(&mut self, root: &mut egui::Ui, p: &Palette) {
        egui::Panel::bottom("etat")
            .frame(Frame::new().fill(p.chrome).inner_margin(Margin::symmetric(16, 5)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    if self.loading.is_some() {
                        ui.spinner();
                    }
                    ui.label(RichText::new(&self.status).font(font(12.0)).color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("Unités : mm").font(font(12.0)).color(p.muted));
                        ui.add_space(12.0);
                        let proj = if self.viewport.camera.orthographic {
                            "Orthographique"
                        } else {
                            "Perspective"
                        };
                        ui.label(RichText::new(proj).font(font(12.0)).color(p.accent_text));
                    });
                });
            });
    }

    // ---------- Arborescence ----------
    fn tree(&mut self, root: &mut egui::Ui, p: &Palette) {
        egui::Panel::left("arbo")
            .default_size(300.0)
            .size_range(230.0..=480.0)
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(0, 12)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    section_label(ui, "MODÈLE", p);
                    if let Some(m) = &self.model {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(format!("{} éléments", m.parts.len()))
                                    .font(font(12.0))
                                    .color(p.muted),
                            );
                        });
                    }
                });
                ui.add_space(6.0);
                let Some(model) = &self.model else {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new("Aucune pièce").color(p.muted));
                    });
                    return;
                };
                let gs = groups(model);
                let title = file_name(&model.source);
                let names: Vec<String> = model.parts.iter().map(|x| x.name.clone()).collect();
                let mut actions: Vec<TreeAction> = Vec::new();

                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    tree_row(ui, p, 0, &title, Some(true), None, None, false, true);
                    for (base, idxs) in &gs {
                        let open = !self.collapsed.contains(base);
                        let all_vis = idxs.iter().all(|&i| self.visible[i]);
                        let single = idxs.len() == 1;
                        let label = base.clone();
                        let sel = single && self.selected == Some(idxs[0]);
                        let chevron = if single { None } else { Some(open) };
                        let badge = if single {
                            None
                        } else {
                            Some(format!("×{}", idxs.len()))
                        };
                        let r = tree_row(ui, p, 1, &label, chevron, badge, Some(all_vis), sel, false);
                        if r.eye_clicked {
                            actions.push(TreeAction::Visible(idxs.clone(), !all_vis));
                        } else if r.clicked {
                            if single {
                                actions.push(TreeAction::Select(if sel { None } else { Some(idxs[0]) }));
                            } else {
                                actions.push(TreeAction::Toggle(base.clone()));
                            }
                        }
                        if open && !single {
                            for &i in idxs {
                                let (_, inst) = split_instance(&names[i]);
                                let lbl = if inst.is_empty() {
                                    names[i].clone()
                                } else {
                                    format!("Corps {inst}")
                                };
                                let sel = self.selected == Some(i);
                                let r = tree_row(ui, p, 2, &lbl, None, None, Some(self.visible[i]), sel, false);
                                if r.eye_clicked {
                                    actions.push(TreeAction::Visible(vec![i], !self.visible[i]));
                                } else if r.clicked {
                                    actions.push(TreeAction::Select(if sel { None } else { Some(i) }));
                                }
                            }
                        }
                    }
                });

                for a in actions {
                    match a {
                        TreeAction::Select(s) => self.select(s),
                        TreeAction::Toggle(b) => {
                            if !self.collapsed.remove(&b) {
                                self.collapsed.insert(b);
                            }
                        }
                        TreeAction::Visible(idxs, v) => {
                            for i in idxs {
                                self.set_visible(i, v);
                            }
                        }
                    }
                }
            });
    }

    // ---------- Propriétés ----------
    fn properties(&mut self, root: &mut egui::Ui, p: &Palette) {
        egui::Panel::right("props")
            .default_size(310.0)
            .size_range(260.0..=460.0)
            .frame(Frame::new().fill(p.panel).inner_margin(Margin::symmetric(16, 12)))
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    section_label(ui, "PROPRIÉTÉS", p);
                    if self.selected.is_some() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("1 sélectionné").font(font(12.0)).color(p.accent_text));
                        });
                    }
                });
                ui.add_space(8.0);
                let Some(m) = &self.model else {
                    ui.label(RichText::new("Ouvre un fichier STEP pour voir ses propriétés.").color(p.muted));
                    return;
                };
                let (title, sub, b, faces, tris) = match self.selected {
                    Some(i) => {
                        let part = &m.parts[i];
                        let (base, inst) = split_instance(&part.name);
                        let sub = if inst.is_empty() {
                            "Corps".to_string()
                        } else {
                            format!("Corps {inst}")
                        };
                        (
                            base.to_string(),
                            sub,
                            part.bounds,
                            part.face_count,
                            part.mesh.triangle_count(),
                        )
                    }
                    None => (
                        file_name(&m.source),
                        format!("Modèle complet · {} éléments", m.parts.len()),
                        m.bounds,
                        m.parts.iter().map(|x| x.face_count).sum(),
                        m.triangle_count(),
                    ),
                };
                ui.label(RichText::new(title).font(font_bold(16.0)).color(p.text));
                ui.label(RichText::new(sub).color(p.muted));
                ui.add_space(12.0);

                section_label(ui, "ENCOMBREMENT", p);
                ui.add_space(4.0);
                let s = b.size();
                prop_row(ui, "Longueur X", &format!("{} mm", mm(s[0])), true, p);
                prop_row(ui, "Largeur Y", &format!("{} mm", mm(s[1])), true, p);
                prop_row(ui, "Hauteur Z", &format!("{} mm", mm(s[2])), true, p);
                ui.add_space(10.0);

                section_label(ui, "GÉOMÉTRIE", p);
                ui.add_space(4.0);
                prop_row(ui, "Faces B-rep", &faces.to_string(), true, p);
                prop_row(ui, "Triangles", &tris.to_string(), true, p);
                ui.add_space(10.0);

                section_label(ui, "PROFIL & MATIÈRE", p);
                ui.add_space(4.0);
                prop_row(ui, "Profil", "—", false, p);
                prop_row(ui, "Matière", "—", false, p);
                prop_row(ui, "Poids", "—", false, p);
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Profil, matière et poids arriveront avec les objets acier (phase 2).")
                        .font(font(11.0))
                        .color(p.muted),
                );
            });
    }

    // ---------- Vue 3D + surcouches ----------
    fn view3d(&mut self, root: &mut egui::Ui, ctx: &egui::Context, frame: &eframe::Frame, p: &Palette) {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.view))
            .show(root, |ui| {
                // Onglets de vues
                Frame::new().inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(78.0, 28.0), Sense::hover());
                        ui.painter().rect(
                            r,
                            CornerRadius::same(6),
                            p.btn_bg,
                            Stroke::new(1.0, p.btn_border),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter()
                            .text(r.center(), Align2::CENTER_CENTER, "Vue 3D", font(12.0), p.strong);
                    });
                });
                let sep_y = ui.min_rect().bottom();
                ui.painter().line_segment(
                    [
                        Pos2::new(ui.max_rect().left(), sep_y),
                        Pos2::new(ui.max_rect().right(), sep_y),
                    ],
                    Stroke::new(1.0, p.border),
                );

                let rect = ui.available_rect_before_wrap();
                let response = ui.allocate_rect(rect, Sense::click_and_drag());

                // Souris : tourner / déplacer / zoomer
                let delta = response.drag_delta();
                let shift = ui.input(|i| i.modifiers.shift);
                if response.dragged_by(egui::PointerButton::Secondary)
                    || (response.dragged_by(egui::PointerButton::Middle)
                        || response.dragged_by(egui::PointerButton::Primary))
                        && shift
                {
                    self.viewport.camera.pan(delta.x, delta.y, rect.height());
                } else if response.dragged_by(egui::PointerButton::Primary)
                    || response.dragged_by(egui::PointerButton::Middle)
                {
                    self.viewport.camera.orbit(delta.x, delta.y);
                }
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        self.viewport.camera.zoom(scroll);
                    }
                }
                if response.double_clicked() {
                    self.viewport.fit();
                }

                // Rendu de la scène
                self.viewport.style = if self.dark {
                    ViewStyle::dark()
                } else {
                    ViewStyle::light()
                };
                if let Some(rs) = frame.wgpu_render_state() {
                    let ppp = ctx.pixels_per_point();
                    let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
                    let view = self.viewport.render(&rs.device, &rs.queue, size);
                    let mut renderer = rs.renderer.write();
                    let id = match self.texture {
                        Some(id) => {
                            renderer.update_egui_texture_from_wgpu_texture(
                                &rs.device,
                                view,
                                egui_wgpu::wgpu::FilterMode::Linear,
                                id,
                            );
                            id
                        }
                        None => renderer.register_native_texture(&rs.device, view, egui_wgpu::wgpu::FilterMode::Linear),
                    };
                    self.texture = Some(id);
                    ui.painter().image(
                        id,
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                self.view_cube(ui, rect, p);
                self.nav_toolbar(ui, rect, p);

                // Aide en bas à droite
                let hint = "Clic gauche : tourner · Clic droit : déplacer · Molette : zoom · F : cadrer";
                let g = ui.painter().layout_no_wrap(hint.into(), font(12.0), p.text2);
                let hr = Rect::from_min_size(
                    rect.right_bottom() - g.size() - Vec2::new(16.0 + 24.0, 16.0 + 16.0),
                    g.size() + Vec2::new(24.0, 16.0),
                );
                if hr.left() > rect.left() + 260.0 {
                    ui.painter().rect(
                        hr,
                        CornerRadius::same(8),
                        p.panel,
                        Stroke::new(1.0, p.border),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().galley(hr.min + Vec2::new(12.0, 8.0), g, p.text2);
                }

                // Message d'accueil
                if self.model.is_none() && self.loading.is_none() {
                    let c = rect.center();
                    ui.painter().text(
                        c - Vec2::new(0.0, 14.0),
                        Align2::CENTER_CENTER,
                        "Ouvre un fichier STEP",
                        font_bold(18.0),
                        p.text2,
                    );
                    ui.painter().text(
                        c + Vec2::new(0.0, 14.0),
                        Align2::CENTER_CENTER,
                        "⌘O, bouton « Ouvrir STEP » ou glisser-déposer le fichier ici",
                        font(13.0),
                        p.muted,
                    );
                }

                if response.dragged() || self.loading.is_some() {
                    ctx.request_repaint();
                }
            });
    }

    /// Cube de vue (en haut à droite) : cliquer une face oriente la vue.
    fn view_cube(&mut self, ui: &mut egui::Ui, rect: Rect, p: &Palette) {
        let o = Pos2::new(rect.right() - 16.0 - 80.0, rect.top() + 16.0);
        let s = 80.0 / 72.0;
        let pt = |x: f32, y: f32| o + Vec2::new(x * s, y * s);
        let faces = [
            (
                [pt(36., 8.), pt(62., 22.), pt(36., 36.), pt(10., 22.)],
                "HAUT",
                StandardView::Dessus,
                if p.dark {
                    Color32::from_rgb(0x2A, 0x33, 0x3E)
                } else {
                    Color32::WHITE
                },
            ),
            (
                [pt(10., 22.), pt(36., 36.), pt(36., 64.), pt(10., 50.)],
                "AV.",
                StandardView::Face,
                if p.dark {
                    Color32::from_rgb(0x22, 0x2A, 0x33)
                } else {
                    Color32::from_rgb(0xE8, 0xEC, 0xF1)
                },
            ),
            (
                [pt(36., 36.), pt(62., 22.), pt(62., 50.), pt(36., 64.)],
                "DR.",
                StandardView::Droite,
                if p.dark {
                    Color32::from_rgb(0x1C, 0x23, 0x2B)
                } else {
                    Color32::from_rgb(0xD6, 0xDC, 0xE3)
                },
            ),
        ];
        let area = Rect::from_min_size(o, Vec2::splat(80.0));
        let resp = ui.interact(area, ui.scope_id().with("cube"), Sense::click());
        let hover = resp.hover_pos();
        let mut clicked_view = None;
        for (poly, label, view, fill) in faces {
            let hot = hover.is_some_and(|h| point_in_poly(h, &poly));
            let f = if hot { p.accent.gamma_multiply(0.55) } else { fill };
            ui.painter().add(egui::Shape::convex_polygon(
                poly.to_vec(),
                f,
                Stroke::new(1.2, p.btn_border),
            ));
            let c = poly.iter().fold(Vec2::ZERO, |a, q| a + q.to_vec2()) / 4.0;
            ui.painter().text(
                c.to_pos2(),
                Align2::CENTER_CENTER,
                label,
                font_bold(9.5),
                if hot { p.strong } else { p.text2 },
            );
            if hot && resp.clicked() {
                clicked_view = Some(view);
            }
        }
        if let Some(v) = clicked_view {
            self.set_view(v);
        }
        resp.on_hover_text("Clique une face pour orienter la vue");
    }

    /// Barre d'outils de navigation (en bas à gauche).
    fn nav_toolbar(&mut self, ui: &mut egui::Ui, rect: Rect, p: &Palette) {
        let bar = Rect::from_min_size(
            rect.left_bottom() + Vec2::new(16.0, -16.0 - 44.0),
            Vec2::new(3.0 * 36.0 + 2.0 * 4.0 + 8.0, 44.0),
        );
        ui.painter().rect(
            bar,
            CornerRadius::same(8),
            p.panel,
            Stroke::new(1.0, p.border),
            egui::StrokeKind::Inside,
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar.shrink(4.0))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child.spacing_mut().item_spacing.x = 4.0;
        if widgets::icon_button(&mut child, icons::NAV_ISO, 36.0, false, "Vue isométrique", p).clicked() {
            self.set_view(StandardView::Iso);
        }
        if widgets::icon_button(&mut child, icons::NAV_CADRER, 36.0, false, "Cadrer tout (F)", p).clicked() {
            self.viewport.fit();
        }
        let ortho = self.viewport.camera.orthographic;
        if widgets::icon_button(
            &mut child,
            icons::NAV_ORTHO,
            36.0,
            ortho,
            "Projection orthographique",
            p,
        )
        .clicked()
        {
            self.viewport.camera.orthographic = !ortho;
        }
    }
}

enum TreeAction {
    Select(Option<usize>),
    Toggle(String),
    Visible(Vec<usize>, bool),
}

struct RowResult {
    clicked: bool,
    eye_clicked: bool,
}

/// Une ligne de l'arborescence (indentée, avec œil de visibilité optionnel).
#[allow(clippy::too_many_arguments)]
fn tree_row(
    ui: &mut egui::Ui,
    p: &Palette,
    depth: usize,
    label: &str,
    chevron: Option<bool>,
    badge: Option<String>,
    visible: Option<bool>,
    selected: bool,
    strong: bool,
) -> RowResult {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, CornerRadius::ZERO, p.sel_bg);
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height())),
            CornerRadius::ZERO,
            p.accent,
        );
    } else if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::ZERO, p.btn_bg);
    }
    let cx = rect.left() + 16.0 + depth as f32 * 16.0;
    if let Some(open) = chevron {
        // petit triangle ▸ / ▾ dessiné à la main
        let c = Pos2::new(cx, rect.center().y);
        let pts = if open {
            vec![
                c + Vec2::new(-4.0, -2.5),
                c + Vec2::new(4.0, -2.5),
                c + Vec2::new(0.0, 3.0),
            ]
        } else {
            vec![
                c + Vec2::new(-2.5, -4.0),
                c + Vec2::new(3.0, 0.0),
                c + Vec2::new(-2.5, 4.0),
            ]
        };
        painter.add(egui::Shape::convex_polygon(pts, p.muted, Stroke::NONE));
    }
    let x = cx + 12.0;
    let color = if selected {
        p.strong
    } else if strong {
        p.text
    } else if visible == Some(false) {
        p.muted
    } else {
        p.text2
    };
    let f = if strong { font_bold(13.0) } else { font(13.0) };
    // Badge « ×4 » à droite, juste avant l'œil
    let mut right = rect.right() - 36.0;
    if let Some(b) = &badge {
        let bg = ui.painter().layout_no_wrap(b.clone(), font(12.0), p.muted);
        right -= bg.size().x;
        painter.galley(Pos2::new(right, rect.center().y - bg.size().y / 2.0), bg, p.muted);
        right -= 8.0;
    }
    // Libellé tronqué avec « … » s'il est trop long
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_string(), f, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width((right - x).max(20.0));
    let g = ui.painter().layout_job(job);
    let truncated = g.size().x + 1.0 >= right - x;
    painter.galley(Pos2::new(x, rect.center().y - g.size().y / 2.0), g, color);
    let resp = if truncated { resp.on_hover_text(label) } else { resp };

    let mut eye_clicked = false;
    if let Some(vis) = visible {
        let er = Rect::from_center_size(Pos2::new(rect.right() - 20.0, rect.center().y), Vec2::splat(24.0));
        let eresp = ui.interact(er, resp.id.with("oeil"), Sense::click());
        let show = resp.hovered() || eresp.hovered() || !vis;
        if show {
            let icon = if vis { icons::OEIL } else { icons::OEIL_FERME };
            let c = if eresp.hovered() { p.strong } else { p.muted };
            egui::Image::new(icon)
                .tint(c)
                .paint_at(ui, Rect::from_center_size(er.center(), Vec2::splat(15.0)));
        }
        eye_clicked = eresp.clicked();
        eresp.on_hover_text(if vis { "Masquer" } else { "Afficher" });
    }
    RowResult {
        clicked: resp.clicked() && !eye_clicked,
        eye_clicked,
    }
}
