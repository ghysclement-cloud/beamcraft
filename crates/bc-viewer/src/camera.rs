//! Caméra orbitale « façon CAO » : l'axe Z est vertical, on tourne autour
//! d'un point cible, on peut se déplacer (pan) et zoomer.

use bc_kernel::Aabb;
use glam::{Mat4, Vec3};

/// Vues standard (comme le cube de vue).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardView {
    Iso,
    Dessus,
    Face,
    Droite,
}

#[derive(Debug, Clone)]
pub struct Camera {
    /// Point autour duquel on tourne (mm).
    pub target: Vec3,
    /// Distance caméra → cible (mm).
    pub distance: f32,
    /// Rotation autour de Z (radians).
    pub yaw: f32,
    /// Élévation au-dessus du plan XY (radians).
    pub pitch: f32,
    /// Champ de vision vertical (radians) en perspective.
    pub fov_y: f32,
    /// Projection orthographique (vraie grandeur) plutôt que perspective.
    pub orthographic: bool,
    /// Taille du modèle, pour régler les plans de coupe proche/lointain.
    scene_radius: f32,
}

impl Default for Camera {
    fn default() -> Self {
        let mut c = Camera {
            target: Vec3::ZERO,
            distance: 5000.0,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 35f32.to_radians(),
            orthographic: false,
            scene_radius: 2000.0,
        };
        c.set_view(StandardView::Iso);
        c
    }
}

impl Camera {
    /// Position de l'œil dans le monde.
    pub fn eye(&self) -> Vec3 {
        let dir = Vec3::new(
            self.pitch.cos() * self.yaw.cos(),
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
        );
        self.target + dir * self.distance
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let eye = self.eye();
        let view = Mat4::look_at_rh(eye, self.target, Vec3::Z);
        let r = self.scene_radius.max(1.0);
        let near = (self.distance - r * 2.0).max(self.distance * 0.001).max(0.1);
        let far = self.distance + r * 4.0;
        let proj = if self.orthographic {
            let h = self.distance * (self.fov_y * 0.5).tan();
            Mat4::orthographic_rh(-h * aspect, h * aspect, -h, h, near.min(1.0), far)
        } else {
            Mat4::perspective_rh(self.fov_y, aspect.max(0.01), near, far)
        };
        proj * view
    }

    /// Tourner : dx, dy en pixels.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(-1.55, 1.55);
    }

    /// Déplacer la vue : dx, dy en pixels, `viewport_h` = hauteur de la vue.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_h: f32) {
        let eye = self.eye();
        let forward = (self.target - eye).normalize();
        let right = forward.cross(Vec3::Z).normalize_or(Vec3::X);
        let up = right.cross(forward);
        let world_per_px = 2.0 * self.distance * (self.fov_y * 0.5).tan() / viewport_h.max(1.0);
        self.target += (-right * dx + up * dy) * world_per_px;
    }

    /// Zoomer : `steps` > 0 rapproche.
    pub fn zoom(&mut self, steps: f32) {
        self.distance = (self.distance * (1.0 - steps * 0.0015).clamp(0.5, 1.5)).max(1.0);
    }

    /// Cadrer tout le modèle.
    pub fn fit(&mut self, bounds: &Aabb) {
        if bounds.is_empty() {
            return;
        }
        let c = bounds.center();
        let s = bounds.size();
        let radius = 0.5 * (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
        self.scene_radius = radius.max(1.0);
        self.target = Vec3::from(c);
        self.distance = self.scene_radius / (self.fov_y * 0.5).sin() * 1.1;
    }

    pub fn set_view(&mut self, v: StandardView) {
        let (yaw, pitch) = match v {
            StandardView::Iso => (-45f32.to_radians(), 30f32.to_radians()),
            StandardView::Dessus => (-90f32.to_radians(), 89.9f32.to_radians()),
            StandardView::Face => (-90f32.to_radians(), 0.0),
            StandardView::Droite => (0.0, 0.0),
        };
        self.yaw = yaw;
        self.pitch = pitch;
    }
}
