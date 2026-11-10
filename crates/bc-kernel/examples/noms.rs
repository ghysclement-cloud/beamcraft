//! Affiche les pièces d'un STEP : `cargo run -p bc-kernel --example noms -- fichier.step`
fn main() {
    let path = std::env::args().nth(1).expect("chemin du fichier STEP");
    let m = bc_kernel::load_step(&path).expect("lecture");
    for p in &m.parts {
        let s = p.bounds.size();
        println!(
            "{:<50} {:>7.0} × {:>7.0} × {:>7.0} mm  ({} faces)",
            p.name, s[0], s[1], s[2], p.face_count
        );
    }
}
