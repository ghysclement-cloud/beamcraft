//! # bc-kernel — le noyau géométrique de BEAMCraft
//!
//! Ce crate est la **seule** porte d'entrée vers le noyau B-rep (OpenCASCADE,
//! via le crate `cadrum`). Le reste du logiciel ne voit que les types simples
//! définis ici (`Model`, `Part`, `TriMesh`) : si un jour on change de noyau
//! (opencascade-rs, truck…), seul ce fichier est à réécrire.
//!
//! Phase 0 : lire un fichier STEP et le transformer en maillages affichables.

use std::path::{Path, PathBuf};

use cadrum::{Solid, Tessellation};

/// Couleur par défaut d'une pièce sans couleur dans le STEP (gris acier).
pub const STEEL_GREY: [f32; 3] = [0.62, 0.67, 0.72];

/// Erreurs possibles du noyau.
#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("impossible d'ouvrir le fichier « {path} » : {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("lecture STEP impossible : {0}")]
    Step(String),
    #[error("le fichier ne contient aucun solide")]
    Empty,
    #[error("maillage impossible : {0}")]
    Mesh(String),
}

/// Boîte englobante alignée sur les axes (en mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb {
        min: [f32::MAX; 3],
        max: [f32::MIN; 3],
    };

    pub fn is_empty(&self) -> bool {
        self.min[0] > self.max[0]
    }
    pub fn grow(&mut self, p: [f32; 3]) {
        for i in 0..3 {
            self.min[i] = self.min[i].min(p[i]);
            self.max[i] = self.max[i].max(p[i]);
        }
    }
    pub fn union(&self, o: &Aabb) -> Aabb {
        let mut r = *self;
        if !o.is_empty() {
            r.grow(o.min);
            r.grow(o.max);
        }
        r
    }
    pub fn size(&self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
    pub fn center(&self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }
}

/// Maillage triangulé prêt pour la carte graphique.
#[derive(Debug, Clone, Default)]
pub struct TriMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Couleur par sommet (issue des couleurs de faces du STEP).
    pub colors: Vec<[f32; 3]>,
    /// Triangles : 3 indices par triangle.
    pub indices: Vec<u32>,
    /// Arêtes vives du B-rep, en segments : 2 points par segment.
    pub edges: Vec<[f32; 3]>,
}

impl TriMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Une pièce (un solide du fichier).
#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    pub mesh: TriMesh,
    pub bounds: Aabb,
    /// Nombre de faces B-rep (utile pour les infos dans le panneau).
    pub face_count: usize,
}

/// Un modèle chargé : la liste des pièces.
#[derive(Debug, Clone)]
pub struct Model {
    pub source: PathBuf,
    pub parts: Vec<Part>,
    pub bounds: Aabb,
}

impl Model {
    pub fn triangle_count(&self) -> usize {
        self.parts.iter().map(|p| p.mesh.triangle_count()).sum()
    }
}

/// Lit un fichier STEP (.step / .stp) et le maille.
pub fn load_step(path: impl AsRef<Path>) -> Result<Model, KernelError> {
    let path = path.as_ref();
    let mut file = std::fs::File::open(path).map_err(|source| KernelError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let solids = Solid::read_step(&mut file).map_err(|e| KernelError::Step(format!("{e:?}")))?;
    model_from_solids(path.to_path_buf(), &solids)
}

/// Transforme des solides du noyau en `Model` affichable.
pub fn model_from_solids(source: PathBuf, solids: &[Solid]) -> Result<Model, KernelError> {
    if solids.is_empty() {
        return Err(KernelError::Empty);
    }
    let groups = solids
        .iter()
        .enumerate()
        .map(|(i, s)| (format!("Solide {}", i + 1), std::slice::from_ref(s)))
        .collect::<Vec<_>>();
    model_from_groups(source, &groups)
}

/// Comme [`model_from_solids`], mais chaque pièce peut regrouper plusieurs solides.
pub fn model_from_groups(source: PathBuf, groups: &[(String, &[Solid])]) -> Result<Model, KernelError> {
    if groups.is_empty() {
        return Err(KernelError::Empty);
    }
    let mut parts = Vec::with_capacity(groups.len());
    let mut bounds = Aabb::EMPTY;
    for (name, solids) in groups {
        let part = mesh_solids(solids, name.clone())?;
        bounds = bounds.union(&part.bounds);
        parts.push(part);
    }
    Ok(Model { source, parts, bounds })
}

/// Modèle d'exemple : un portique (2 poteaux HEA 200 + 1 traverse IPE 300),
/// pour voir quelque chose sans fichier STEP.
pub fn sample_portal() -> Model {
    let hea200 = |x: f64| i_column(x, 0.0, 4500.0, 190.0, 200.0, 6.5, 10.0);
    let p1 = hea200(0.0);
    let p2 = hea200(8000.0);
    let traverse = i_beam_x(-100.0, 8100.0, 0.0, 4500.0, 300.0, 150.0, 7.1, 10.7);
    let groups: Vec<(String, &[Solid])> = vec![
        ("P1-01 · Poteau HEA 200".into(), &p1),
        ("P1-02 · Poteau HEA 200".into(), &p2),
        ("P2-01 · Traverse IPE 300".into(), &traverse),
    ];
    model_from_groups(PathBuf::from("exemple-portique"), &groups).expect("modèle d'exemple")
}

/// Profil en I vertical (poteau), centré en (x, y), du sol à `height`.
/// Âme dans le plan XZ : les ailes sont perpendiculaires à X.
fn i_column(x: f64, y: f64, height: f64, h: f64, b: f64, tw: f64, tf: f64) -> Vec<Solid> {
    use cadrum::DVec3 as V;
    vec![
        Solid::cube(
            V::new(x - h / 2.0, y - b / 2.0, 0.0),
            V::new(x - h / 2.0 + tf, y + b / 2.0, height),
        ),
        Solid::cube(
            V::new(x + h / 2.0 - tf, y - b / 2.0, 0.0),
            V::new(x + h / 2.0, y + b / 2.0, height),
        ),
        Solid::cube(
            V::new(x - h / 2.0 + tf, y - tw / 2.0, 0.0),
            V::new(x + h / 2.0 - tf, y + tw / 2.0, height),
        ),
    ]
}

/// Profil en I horizontal le long de X, posé à l'altitude `z0`.
#[allow(clippy::too_many_arguments)]
fn i_beam_x(x0: f64, x1: f64, y: f64, z0: f64, h: f64, b: f64, tw: f64, tf: f64) -> Vec<Solid> {
    use cadrum::DVec3 as V;
    vec![
        Solid::cube(V::new(x0, y - b / 2.0, z0), V::new(x1, y + b / 2.0, z0 + tf)),
        Solid::cube(V::new(x0, y - b / 2.0, z0 + h - tf), V::new(x1, y + b / 2.0, z0 + h)),
        Solid::cube(V::new(x0, y - tw / 2.0, z0 + tf), V::new(x1, y + tw / 2.0, z0 + h - tf)),
    ]
}

fn mesh_solids(solids: &[Solid], name: String) -> Result<Part, KernelError> {
    let m = Solid::mesh(solids.iter(), Tessellation::default()).map_err(|e| KernelError::Mesh(format!("{e:?}")))?;

    let positions: Vec<[f32; 3]> = m
        .vertices
        .iter()
        .map(|v| [v.x as f32, v.y as f32, v.z as f32])
        .collect();
    let normals: Vec<[f32; 3]> = m.normals.iter().map(|n| [n.x as f32, n.y as f32, n.z as f32]).collect();

    // Couleur par sommet : chaque sommet n'appartient qu'à une face B-rep,
    // on lui donne la couleur de cette face (ou le gris acier par défaut).
    let mut colors = vec![STEEL_GREY; positions.len()];
    for (tri, face_id) in m.indices.chunks_exact(3).zip(m.face_ids.iter()) {
        if let Some(c) = m.colormap.get(face_id) {
            let rgb = color_to_rgb(c);
            for &vi in tri {
                colors[vi] = rgb;
            }
        }
    }

    // Arêtes : polylignes séparées par NaN → liste de segments.
    let mut edges = Vec::new();
    let mut prev: Option<[f32; 3]> = None;
    for p in &m.edges {
        if p.x.is_nan() {
            prev = None;
            continue;
        }
        let cur = [p.x as f32, p.y as f32, p.z as f32];
        if let Some(a) = prev {
            edges.push(a);
            edges.push(cur);
        }
        prev = Some(cur);
    }

    let mut bounds = Aabb::EMPTY;
    for p in &positions {
        bounds.grow(*p);
    }
    let mut faces: Vec<u64> = m.face_ids.clone();
    faces.sort_unstable();
    faces.dedup();

    Ok(Part {
        name,
        mesh: TriMesh {
            positions,
            normals,
            colors,
            indices: m.indices.iter().map(|&i| i as u32).collect(),
            edges,
        },
        bounds,
        face_count: faces.len(),
    })
}

fn color_to_rgb(c: &cadrum::Color) -> [f32; 3] {
    [c.r as f32, c.g as f32, c.b as f32]
}
