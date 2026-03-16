use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::rasterizer::{Camera, Color, Framebuffer, Texture};
use crate::rasterizer::constants::{WIDTH, HEIGHT};
use crate::scene::{Level, TextureRef};
use super::context::EditorContext;

pub struct ViewportPanel {
    pub framebuffer: Framebuffer,
    pub camera: Camera,
    /// Cached render data from level rooms (vertices, faces)
    cached_render: Option<CachedLevelRender>,
    /// Texture cache: (pack, name) → index into CachedLevelRender::textures
    texture_cache: HashMap<(String, String), usize>,
}

struct CachedLevelRender {
    vertices: Vec<crate::rasterizer::Vertex>,
    faces: Vec<crate::rasterizer::Face>,
    textures: Vec<Texture>,
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
            cached_render: None,
            texture_cache: HashMap::new(),
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
            let available = ui.available_size();
            let (_rect, response) = ui.allocate_exact_size(
                available,
                egui::Sense::click_and_drag(),
            );

            // Camera orbit (left drag), pan (middle/right drag)
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

            // Zoom (scroll wheel)
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

    /// Rebuild cached render data from a level.
    /// `project_root` is used to resolve texture files; pass `None` for untextured rendering.
    pub fn rebuild_from_level(&mut self, level: &Level) {
        self.rebuild_from_level_with_textures(level, None);
    }

    /// Rebuild with optional texture resolution from a project root.
    pub fn rebuild_from_level_with_textures(
        &mut self,
        level: &Level,
        project_root: Option<&Path>,
    ) {
        // Use RefCell so the closure satisfies Fn (not FnMut) while still mutating state
        use std::cell::RefCell;
        let tex_cache: RefCell<HashMap<(String, String), usize>> = RefCell::new(HashMap::new());
        let textures: RefCell<Vec<Texture>> = RefCell::new(Vec::new());
        let project_root = project_root.map(|p| p.to_path_buf());

        let resolve = |tex_ref: &TextureRef| -> Option<(usize, u32)> {
            let key = (tex_ref.pack.clone(), tex_ref.name.clone());
            {
                let cache = tex_cache.borrow();
                if let Some(&idx) = cache.get(&key) {
                    let w = textures.borrow()[idx].width as u32;
                    return Some((idx, w));
                }
            }
            let path = resolve_texture_path(tex_ref, project_root.as_deref())?;
            match Texture::from_file(&path) {
                Ok(tex) => {
                    let w = tex.width as u32;
                    let mut cache = tex_cache.borrow_mut();
                    let mut tex_vec = textures.borrow_mut();
                    let idx = tex_vec.len();
                    tex_vec.push(tex);
                    cache.insert(key, idx);
                    Some((idx, w))
                }
                Err(e) => {
                    log::warn!("Failed to load texture {:?}/{}: {}", tex_ref.pack, tex_ref.name, e);
                    None
                }
            }
        };

        let mut all_vertices = Vec::new();
        let mut all_faces = Vec::new();

        for room in &level.rooms {
            let (verts, faces) = room.to_render_data_with_textures(&resolve);
            let offset = all_vertices.len();
            all_vertices.extend(verts);
            for mut face in faces {
                face.v0 += offset;
                face.v1 += offset;
                face.v2 += offset;
                all_faces.push(face);
            }
        }

        self.texture_cache = tex_cache.into_inner();
        self.cached_render = Some(CachedLevelRender {
            vertices: all_vertices,
            faces: all_faces,
            textures: textures.into_inner(),
        });
    }

    pub fn render_frame(&mut self, dt: f32, rotation: &mut f32) {
        use crate::rasterizer::{Vertex, Texture, Vec3, RasterSettings, render_mesh};

        self.framebuffer.clear(Color::new(30, 30, 50));

        // Draw floor grid
        use crate::rasterizer::draw::draw_floor_grid;
        draw_floor_grid(
            &mut self.framebuffer,
            &self.camera,
            0.0,   // y
            1.0,   // spacing
            20.0,  // extent
            Color::new(45, 45, 55),  // grid
            Color::new(100, 40, 40), // x axis
            Color::new(40, 40, 100), // z axis
        );

        if let Some(cached) = &self.cached_render {
            // Render level geometry
            let settings = RasterSettings::default();
            render_mesh(
                &mut self.framebuffer,
                &cached.vertices,
                &cached.faces,
                &cached.textures,
                &self.camera,
                &settings,
            );
        } else {
            // No level loaded — render spinning test cube
            use crate::rasterizer::draw::create_test_cube;

            *rotation += dt * 0.8;

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
}

impl Default for ViewportPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a TextureRef to a file path, searching bundled and project dirs.
fn resolve_texture_path(tex: &TextureRef, project_root: Option<&Path>) -> Option<PathBuf> {
    if !tex.is_valid() { return None; }

    let extensions = ["png", "jpg", "jpeg"];

    // 1. Project textures directory: {root}/textures/{name}.{ext}
    if let Some(root) = project_root {
        for ext in extensions {
            let p = root.join("textures").join(format!("{}.{}", tex.name, ext));
            if p.exists() { return Some(p); }
        }
    }

    // 2. Bundled runtime textures: assets/runtime/textures/{pack}/{name}.{ext}
    for ext in extensions {
        let p = PathBuf::from("assets/runtime/textures")
            .join(&tex.pack)
            .join(format!("{}.{}", tex.name, ext));
        if p.exists() { return Some(p); }
    }

    // 3. Flat bundled: assets/runtime/textures/{name}.{ext}
    for ext in extensions {
        let p = PathBuf::from("assets/runtime/textures")
            .join(format!("{}.{}", tex.name, ext));
        if p.exists() { return Some(p); }
    }

    None
}
