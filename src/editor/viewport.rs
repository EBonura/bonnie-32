use crate::rasterizer::{Camera, Color, Framebuffer};
use crate::rasterizer::constants::{WIDTH, HEIGHT};
use super::context::EditorContext;

pub struct ViewportPanel {
    pub framebuffer: Framebuffer,
    pub camera: Camera,
}

impl ViewportPanel {
    pub fn new() -> Self {
        use crate::rasterizer::Vec3;

        let mut camera = Camera::new();
        camera.position = Vec3::new(0.0, 1.5, -4.0);
        camera.rotation_x = -0.2;
        camera.update_basis();

        Self {
            framebuffer: Framebuffer::new(WIDTH, HEIGHT),
            camera,
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context, _editor: &mut EditorContext) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Viewport");
                ui.separator();
                ui.weak(format!("{}x{}", WIDTH, HEIGHT));
            });
            ui.separator();

            // The framebuffer is rendered by the wgpu fullscreen quad behind egui.
            // This panel just provides the frame and will handle input in the future.
            let available = ui.available_size();
            let (_rect, _response) = ui.allocate_exact_size(
                available,
                egui::Sense::click_and_drag(),
            );

            // TODO: camera controls (orbit, pan, zoom) from response
            // TODO: entity picking from click
        });
    }

    pub fn render_frame(&mut self, dt: f32, rotation: &mut f32) {
        use crate::rasterizer::{Vertex, Texture, Vec3, RasterSettings, render_mesh};
        use crate::rasterizer::draw::create_test_cube;

        *rotation += dt * 0.8;

        self.framebuffer.clear(Color::new(30, 30, 50));

        let (vertices, faces) = create_test_cube();
        let cos_r = rotation.cos();
        let sin_r = rotation.sin();
        let rotated: Vec<Vertex> = vertices.iter().map(|v| {
            let x = v.pos.x * cos_r - v.pos.z * sin_r;
            let z = v.pos.x * sin_r + v.pos.z * cos_r;
            Vertex { pos: Vec3::new(x, v.pos.y, z), ..*v }
        }).collect();

        let textures: Vec<Texture> = vec![];
        let settings = RasterSettings::default();
        render_mesh(&mut self.framebuffer, &rotated, &faces, &textures, &self.camera, &settings);
    }
}

impl Default for ViewportPanel {
    fn default() -> Self {
        Self::new()
    }
}
