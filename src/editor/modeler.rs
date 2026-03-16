//! Modeler panel — egui UI for the 3D mesh editor

use crate::rasterizer::{Camera, Color, Framebuffer, Vec3};
use crate::rasterizer::constants::{WIDTH, HEIGHT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTool {
    Select,
    Move,
    Rotate,
    Scale,
    Vertex,
    Face,
    Extrude,
}

impl ModelTool {
    fn label(&self) -> &'static str {
        match self {
            ModelTool::Select => "Select",
            ModelTool::Move => "Move",
            ModelTool::Rotate => "Rotate",
            ModelTool::Scale => "Scale",
            ModelTool::Vertex => "Vertex",
            ModelTool::Face => "Face",
            ModelTool::Extrude => "Extrude",
        }
    }
}

pub struct ModelerPanel {
    pub framebuffer: Framebuffer,
    pub camera: Camera,
    pub tool: ModelTool,
    rotation: f32,
}

impl ModelerPanel {
    pub fn new() -> Self {
        let mut camera = Camera::new();
        camera.position = Vec3::new(2.0, 2.0, -3.0);
        camera.rotation_x = -0.4;
        camera.rotation_y = 0.5;
        camera.update_basis();

        Self {
            framebuffer: Framebuffer::new(WIDTH, HEIGHT),
            camera,
            tool: ModelTool::Select,
            rotation: 0.0,
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context) {
        // Toolbar
        egui::TopBottomPanel::top("modeler_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for tool in [
                    ModelTool::Select, ModelTool::Move, ModelTool::Rotate,
                    ModelTool::Scale, ModelTool::Vertex, ModelTool::Face,
                    ModelTool::Extrude,
                ] {
                    if ui.selectable_label(self.tool == tool, tool.label()).clicked() {
                        self.tool = tool;
                    }
                }

                ui.separator();
                ui.weak(format!("Tool: {}", self.tool.label()));
            });
        });

        // Properties panel
        egui::SidePanel::right("modeler_properties")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Properties");
                ui.separator();

                ui.label("No model loaded");
                ui.separator();

                ui.collapsing("Mesh Info", |ui| {
                    ui.label("Vertices: 0");
                    ui.label("Faces: 0");
                    ui.label("Textures: 0");
                });

                ui.collapsing("Transform", |ui| {
                    ui.label("Position: (0, 0, 0)");
                    ui.label("Rotation: (0, 0, 0)");
                    ui.label("Scale: (1, 1, 1)");
                });
            });

        // Viewport
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let (_rect, response) = ui.allocate_exact_size(
                available,
                egui::Sense::click_and_drag(),
            );

            // Camera orbit
            if response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_delta();
                self.camera.rotation_y += delta.x * 0.005;
                self.camera.rotation_x += delta.y * 0.005;
                self.camera.rotation_x = self.camera.rotation_x.clamp(-1.5, 1.5);
                self.camera.update_basis();
            }
            if response.dragged_by(egui::PointerButton::Secondary)
                || response.dragged_by(egui::PointerButton::Middle)
            {
                let delta = response.drag_delta();
                let right = self.camera.basis_x;
                let up = self.camera.basis_y;
                let speed = 0.01;
                self.camera.position.x += (-right.x * delta.x + up.x * delta.y) * speed;
                self.camera.position.y += (-right.y * delta.x + up.y * delta.y) * speed;
                self.camera.position.z += (-right.z * delta.x + up.z * delta.y) * speed;
            }

            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let forward = self.camera.basis_z;
                let speed = 0.05;
                self.camera.position.x += forward.x * scroll * speed;
                self.camera.position.y += forward.y * scroll * speed;
                self.camera.position.z += forward.z * scroll * speed;
            }
        });
    }

    pub fn render_frame(&mut self, dt: f32) {
        use crate::rasterizer::{Vertex, Texture, RasterSettings, render_mesh};
        use crate::rasterizer::draw::{create_test_cube, draw_floor_grid};

        self.framebuffer.clear(Color::new(40, 40, 45));

        // Draw grid
        draw_floor_grid(
            &mut self.framebuffer,
            &self.camera,
            0.0,   // y
            1.0,   // spacing
            10.0,  // extent
            Color::new(60, 60, 65),  // grid
            Color::new(120, 50, 50), // x axis
            Color::new(50, 50, 120), // z axis
        );

        // Render a test cube placeholder
        self.rotation += dt * 0.3;
        let (vertices, faces) = create_test_cube();
        let cos_r = self.rotation.cos();
        let sin_r = self.rotation.sin();
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

impl Default for ModelerPanel {
    fn default() -> Self {
        Self::new()
    }
}
