//! # BEAMCraft — application de bureau
//!
//! Phase 0 : une fenêtre 3D qui ouvre un fichier STEP.
//! La disposition suit la maquette : en-tête, onglets du ruban, arborescence
//! à gauche, vue 3D au centre, propriétés à droite, barre d'état en bas.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // pas de console sous Windows

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use bc_kernel::{KernelError, Model};
use bc_viewer::{StandardView, ViewStyle, Viewport};
use eframe::egui::{self, Color32, RichText};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BEAMCraft")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
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
    dark: bool,
    loading: Option<(PathBuf, Receiver<LoadResult>)>,
    status: String,
    error: Option<String>,
    /// Fichier passé en ligne de commande (`beamcraft piece.step` ou `--exemple`).
    startup_arg: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.as_ref().expect("BEAMCraft a besoin de wgpu");
        let viewport = Viewport::new(&rs.device);
        let app = App {
            viewport,
            texture: None,
            model: None,
            visible: Vec::new(),
            selected: None,
            dark: true,
            loading: None,
            status: "Prêt".into(),
            error: None,
            startup_arg: std::env::args().nth(1),
        };
        app.apply_theme(&cc.egui_ctx);
        app
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_visuals(if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
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

    fn set_model(&mut self, frame: &eframe::Frame, model: Model) {
        if let Some(rs) = frame.wgpu_render_state() {
            self.viewport.set_model(&rs.device, &model);
        }
        self.visible = vec![true; model.parts.len()];
        self.selected = None;
        self.viewport.set_highlight(None);
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

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        if let Some(arg) = self.startup_arg.take() {
            if arg == "--exemple" {
                self.set_model(frame, bc_kernel::sample_portal());
                self.status = "Exemple chargé".into();
            } else {
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
        let (open_shortcut, fit_shortcut) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::O),
                !typing && i.key_pressed(egui::Key::F),
            )
        });
        if open_shortcut {
            self.open_dialog(ctx);
        }
        if fit_shortcut {
            self.viewport.fit();
        }

        let accent = if self.dark {
            Color32::from_rgb(0x4C, 0x9A, 0xFF)
        } else {
            Color32::from_rgb(0x1F, 0x6F, 0xD6)
        };

        // ---------- En-tête ----------
        egui::Panel::top("entete").show(root, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("BEAMCraft").strong().size(16.0).color(accent));
                ui.separator();
                let titre = self
                    .model
                    .as_ref()
                    .map(|m| file_name(&m.source))
                    .unwrap_or_else(|| "Aucun fichier".into());
                ui.label(RichText::new(titre).weak());
                ui.separator();
                if ui.button("Ouvrir un STEP…").on_hover_text("Ctrl/Cmd + O").clicked() {
                    self.open_dialog(ctx);
                }
                if ui.button("Exemple : portique").clicked() {
                    let m = bc_kernel::sample_portal();
                    self.status = format!("Exemple chargé · {} triangles", m.triangle_count());
                    self.set_model(frame, m);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if self.dark { "Thème clair" } else { "Thème sombre" };
                    if ui.button(label).clicked() {
                        self.dark = !self.dark;
                        self.apply_theme(ctx);
                    }
                });
            });
            ui.add_space(2.0);
            // Onglets du ruban (les autres modules arrivent dans les phases suivantes).
            ui.horizontal(|ui| {
                let _ = ui.selectable_label(true, "Modélisation");
                for t in ["Assemblages", "Numérotation", "Débit & CN", "Plans", "Échanges IFC"] {
                    ui.add_enabled(false, egui::Button::selectable(false, t))
                        .on_disabled_hover_text("Bientôt");
                }
            });
            ui.add_space(4.0);
        });

        // ---------- Barre d'état ----------
        egui::Panel::bottom("etat").show(root, |ui| {
            ui.horizontal(|ui| {
                if self.loading.is_some() {
                    ui.spinner();
                }
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Unités : mm").weak());
                    ui.separator();
                    ui.label(
                        RichText::new(
                            "Clic gauche : tourner · Clic droit / Maj : déplacer · Molette : zoom · F : cadrer",
                        )
                        .weak(),
                    );
                });
            });
        });

        // ---------- Arborescence ----------
        egui::Panel::left("arbo").default_size(250.0).show(root, |ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("MODÈLE").small().strong().weak());
            ui.add_space(4.0);
            let mut new_sel = None;
            let mut vis_changes = Vec::new();
            match &self.model {
                None => {
                    ui.label(RichText::new("Aucune pièce").weak());
                }
                Some(m) => {
                    ui.label(RichText::new(format!("{} pièce(s)", m.parts.len())).weak());
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (i, part) in m.parts.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let mut v = self.visible[i];
                                if ui.checkbox(&mut v, "").on_hover_text("Afficher / masquer").changed() {
                                    vis_changes.push((i, v));
                                }
                                if ui.selectable_label(self.selected == Some(i), &part.name).clicked() {
                                    new_sel = Some(if self.selected == Some(i) { None } else { Some(i) });
                                }
                            });
                        }
                    });
                }
            }
            for (i, v) in vis_changes {
                self.visible[i] = v;
                self.viewport.set_part_visible(i, v);
            }
            if let Some(s) = new_sel {
                self.select(s);
            }
        });

        // ---------- Propriétés ----------
        egui::Panel::right("props").default_size(280.0).show(root, |ui| {
            ui.add_space(6.0);
            ui.label(RichText::new("PROPRIÉTÉS").small().strong().weak());
            ui.add_space(6.0);
            let info = match (&self.model, self.selected) {
                (Some(m), Some(i)) => {
                    let p = &m.parts[i];
                    Some((p.name.clone(), p.bounds, p.face_count, p.mesh.triangle_count()))
                }
                (Some(m), None) => Some((
                    "Modèle complet".to_string(),
                    m.bounds,
                    m.parts.iter().map(|p| p.face_count).sum(),
                    m.triangle_count(),
                )),
                _ => None,
            };
            match info {
                None => {
                    ui.label(RichText::new("Ouvre un fichier STEP pour voir ses propriétés.").weak());
                }
                Some((name, b, faces, tris)) => {
                    ui.label(RichText::new(name).strong().size(15.0));
                    ui.add_space(8.0);
                    let s = b.size();
                    egui::Grid::new("props_grid")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Encombrement X").weak());
                            ui.label(format!("{} mm", mm(s[0])));
                            ui.end_row();
                            ui.label(RichText::new("Encombrement Y").weak());
                            ui.label(format!("{} mm", mm(s[1])));
                            ui.end_row();
                            ui.label(RichText::new("Encombrement Z").weak());
                            ui.label(format!("{} mm", mm(s[2])));
                            ui.end_row();
                            ui.label(RichText::new("Faces B-rep").weak());
                            ui.label(faces.to_string());
                            ui.end_row();
                            ui.label(RichText::new("Triangles").weak());
                            ui.label(tris.to_string());
                            ui.end_row();
                        });
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Profil, matière et poids arriveront avec les objets métier (phase 2).")
                            .weak()
                            .small(),
                    );
                }
            }
        });

        // ---------- Vue 3D ----------
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
            // Barre de vues
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (label, v) in [("Iso", StandardView::Iso), ("Dessus", StandardView::Dessus), ("Face", StandardView::Face), ("Droite", StandardView::Droite)] {
                        if ui.button(label).clicked() {
                            self.viewport.camera.set_view(v);
                            self.viewport.fit();
                        }
                    }
                    ui.separator();
                    if ui.button("Cadrer (F)").clicked() {
                        self.viewport.fit();
                    }
                    ui.checkbox(&mut self.viewport.camera.orthographic, "Orthographique");
                });
            });

            let rect = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

            // Souris : tourner / déplacer / zoomer
            let delta = response.drag_delta();
            let shift = ui.input(|i| i.modifiers.shift);
            if response.dragged_by(egui::PointerButton::Secondary)
                || response.dragged_by(egui::PointerButton::Middle) && shift
                || response.dragged_by(egui::PointerButton::Primary) && shift
            {
                self.viewport.camera.pan(delta.x, delta.y, rect.height());
            } else if response.dragged_by(egui::PointerButton::Primary) || response.dragged_by(egui::PointerButton::Middle) {
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

            // Rendu
            self.viewport.style = if self.dark { ViewStyle::dark() } else { ViewStyle::light() };
            if let Some(rs) = frame.wgpu_render_state() {
                let ppp = ctx.pixels_per_point();
                let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
                let view = self.viewport.render(&rs.device, &rs.queue, size);
                let mut renderer = rs.renderer.write();
                let id = match self.texture {
                    Some(id) => {
                        renderer.update_egui_texture_from_wgpu_texture(&rs.device, view, egui_wgpu::wgpu::FilterMode::Linear, id);
                        id
                    }
                    None => renderer.register_native_texture(&rs.device, view, egui_wgpu::wgpu::FilterMode::Linear),
                };
                self.texture = Some(id);
                ui.painter().image(id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }

            // Message d'accueil / erreur
            if self.model.is_none() && self.loading.is_none() {
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Ouvre un fichier STEP (Ctrl/Cmd + O)\nou glisse-le ici\n\nou clique sur « Exemple : portique »")
                                .size(16.0)
                                .weak(),
                        );
                    });
                });
            }
            if let Some(err) = self.error.clone() {
                egui::Window::new("Impossible d'ouvrir le fichier")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(err);
                        if ui.button("OK").clicked() {
                            self.error = None;
                        }
                    });
            }

            // Pendant un déplacement de caméra, on redessine en continu.
            if response.dragged() || self.loading.is_some() {
                ctx.request_repaint();
            }
        });
    }
}
