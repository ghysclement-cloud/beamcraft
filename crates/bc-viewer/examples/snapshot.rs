//! Capture d'écran sans fenêtre : `cargo run -p bc-viewer --example snapshot [fichier.step] [sortie.png]`
//! Sans fichier, utilise le portique d'exemple. Pratique pour vérifier le rendu.

use bc_viewer::{wgpu, StandardView, ViewStyle, Viewport};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let model = match args.get(1) {
        Some(p) if p.ends_with(".step") || p.ends_with(".stp") => bc_kernel::load_step(p).expect("STEP"),
        _ => bc_kernel::sample_portal(),
    };
    let out = args
        .iter()
        .find(|a| a.ends_with(".png"))
        .cloned()
        .unwrap_or("snapshot.png".into());
    let light = args.iter().any(|a| a == "--clair");

    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adaptateur GPU");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");

    let mut vp = Viewport::new(&device);
    vp.style = if light { ViewStyle::light() } else { ViewStyle::dark() };
    vp.set_model(&device, &model);
    vp.camera.set_view(StandardView::Iso);
    vp.fit();
    vp.set_highlight(Some(2));
    let (w, h) = (1280u32, 800u32);
    vp.render(&device, &queue, [w, h]);

    let bpr = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (bpr * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        vp.output_texture().unwrap().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(enc.finish()));
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buf.slice(..).get_mapped_range().expect("lecture");
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let s = (row * bpr) as usize;
        pixels.extend_from_slice(&data[s..s + (w * 4) as usize]);
    }
    let file = std::fs::File::create(&out).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&pixels).unwrap();
    println!("capture : {out} ({} triangles)", model.triangle_count());
}
