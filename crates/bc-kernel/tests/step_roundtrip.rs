//! Test : on fabrique un petit modèle, on l'écrit en STEP, on le relit.

use bc_kernel::load_step;
use cadrum::{DVec3, Solid};

#[test]
fn lecture_step_d_un_profil_en_i() {
    // Un « IPE 300 » simplifié de 1 000 mm : deux ailes + une âme.
    let solids = vec![
        Solid::cube(DVec3::new(0.0, -75.0, 289.3), DVec3::new(1000.0, 75.0, 300.0)),
        Solid::cube(DVec3::new(0.0, -3.55, 10.7), DVec3::new(1000.0, 3.55, 289.3)),
        Solid::cube(DVec3::new(0.0, -75.0, 0.0), DVec3::new(1000.0, 75.0, 10.7)),
    ];
    let dir = std::env::temp_dir().join("bc_kernel_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ipe.step");
    Solid::write_step(&solids, &mut std::fs::File::create(&path).unwrap()).unwrap();

    let model = load_step(&path).expect("le STEP doit se relire");
    assert_eq!(model.parts.len(), 3);
    assert!(model.triangle_count() >= 36);
    let size = model.bounds.size();
    assert!((size[0] - 1000.0).abs() < 0.5, "longueur {size:?}");
    assert!((size[2] - 300.0).abs() < 0.5, "hauteur {size:?}");
    assert!(!model.parts[0].mesh.edges.is_empty(), "arêtes attendues");
    assert_eq!(model.parts[0].face_count, 6);
}

#[test]
fn modele_d_exemple() {
    let m = bc_kernel::sample_portal();
    assert_eq!(m.parts.len(), 3);
    let s = m.bounds.size();
    assert!((s[2] - 4800.0).abs() < 1.0, "{s:?}");
}
