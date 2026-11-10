// BEAMCraft — shaders de la vue 3D.

struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct PartUniform {
    // rgb = couleur de surbrillance, a = intensité (0 = aucune)
    tint: vec4<f32>,
};
@group(1) @binding(0) var<uniform> part: PartUniform;

// ---------- Surfaces ----------
struct MeshIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};
struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

@vertex
fn vs_mesh(v: MeshIn) -> MeshOut {
    var o: MeshOut;
    o.clip = globals.view_proj * vec4<f32>(v.position, 1.0);
    o.world = v.position;
    o.normal = v.normal;
    o.color = v.color;
    return o;
}

@fragment
fn fs_mesh(i: MeshOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(i.normal);
    if (!front) { n = -n; }
    let to_eye = normalize(globals.eye.xyz - i.world);
    // Lumière « frontale » (attachée à la caméra) + une lumière douce venant du haut.
    let head = max(dot(n, to_eye), 0.0);
    let sky = 0.5 + 0.5 * n.z;
    let shade = 0.30 + 0.55 * head + 0.15 * sky;
    let base = mix(i.color, part.tint.rgb, part.tint.a);
    return vec4<f32>(base * shade, 1.0);
}

// ---------- Lignes (arêtes, grille) ----------
struct LineIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};
struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(v: LineIn) -> LineOut {
    var o: LineOut;
    o.clip = globals.view_proj * vec4<f32>(v.position, 1.0);
    o.color = v.color;
    return o;
}

@fragment
fn fs_line(i: LineOut) -> @location(0) vec4<f32> {
    return i.color;
}
