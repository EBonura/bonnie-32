//! Core rendering functions
//! Triangle rasterization with PS1-style effects

use macroquad::prelude::get_time;
use super::math::{perspective_transform, project, project_ortho, Vec3, NEAR_PLANE};
use super::types::{BlendMode, Color, Color15, Clut, Face, IndexedTexture, Light, LightType, RasterSettings, RasterTimings, ShadingMode, Texture, Texture15, Vertex};

/// Framebuffer for software rendering
pub struct Framebuffer {
    pub pixels: Vec<u8>,    // RGBA, 4 bytes per pixel
    pub zbuffer: Vec<f32>,  // Depth buffer
    pub width: usize,
    pub height: usize,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: vec![0; width * height * 4],
            zbuffer: vec![f32::MAX; width * height],
            width,
            height,
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.pixels = vec![0; width * height * 4];
            self.zbuffer = vec![f32::MAX; width * height];
        }
    }

    pub fn clear(&mut self, color: Color) {
        for i in 0..(self.width * self.height) {
            let bytes = color.to_bytes();
            self.pixels[i * 4] = bytes[0];
            self.pixels[i * 4 + 1] = bytes[1];
            self.pixels[i * 4 + 2] = bytes[2];
            self.pixels[i * 4 + 3] = bytes[3];
            self.zbuffer[i] = f32::MAX;
        }
    }

    /// Clear framebuffer with transparent black (for alpha compositing)
    pub fn clear_transparent(&mut self) {
        for i in 0..(self.width * self.height) {
            self.pixels[i * 4] = 0;
            self.pixels[i * 4 + 1] = 0;
            self.pixels[i * 4 + 2] = 0;
            self.pixels[i * 4 + 3] = 0;
            self.zbuffer[i] = f32::MAX;
        }
    }

    /// Clear framebuffer with a vertical gradient (skybox effect)
    /// top_color at y=0, bottom_color at y=height-1
    pub fn clear_gradient(&mut self, top_color: Color, bottom_color: Color) {
        let h = self.height;
        for y in 0..h {
            // Linear interpolation factor (0.0 at top, 1.0 at bottom)
            let t = if h > 1 { y as f32 / (h - 1) as f32 } else { 0.0 };
            let color = top_color.lerp(bottom_color, t);
            let bytes = color.to_bytes();

            for x in 0..self.width {
                let idx = (y * self.width + x) * 4;
                self.pixels[idx] = bytes[0];
                self.pixels[idx + 1] = bytes[1];
                self.pixels[idx + 2] = bytes[2];
                self.pixels[idx + 3] = bytes[3];
                self.zbuffer[y * self.width + x] = f32::MAX;
            }
        }
    }

    /// Render PS1 Spyro-style skybox with all effects
    /// Renders: base sphere (gradient + sun glow + clouds), stars, and 3D mountains
    pub fn render_skybox(
        &mut self,
        skybox: &crate::world::Skybox,
        camera: &super::Camera,
        time: f32,
    ) {
        use super::math::{perspective_transform, project};

        // 1. Render base skybox sphere (gradient + sun glow + clouds baked in vertex colors)
        let cam_pos = (camera.position.x, camera.position.y, camera.position.z);
        let (vertices, faces) = skybox.generate_mesh(cam_pos, time);

        // Transform and project all vertices
        let mut projected: Vec<(f32, f32, f32)> = Vec::with_capacity(vertices.len());

        for v in &vertices {
            let world_pos = super::math::Vec3::new(v.pos.0, v.pos.1, v.pos.2);
            let rel_pos = world_pos - camera.position;
            let cam_space = perspective_transform(rel_pos, camera.basis_x, camera.basis_y, camera.basis_z);

            // Skip vertices behind camera
            if cam_space.z <= 0.1 {
                projected.push((f32::NAN, f32::NAN, f32::NAN));
                continue;
            }

            let screen = project(cam_space, self.width, self.height);
            projected.push((screen.x, screen.y, cam_space.z));
        }

        // Render each triangle
        for face in &faces {
            let p0 = projected[face[0]];
            let p1 = projected[face[1]];
            let p2 = projected[face[2]];

            // Skip if any vertex is behind camera
            if p0.0.is_nan() || p1.0.is_nan() || p2.0.is_nan() {
                continue;
            }

            // Screen-space backface culling (looking from inside the sphere)
            // We want triangles with NEGATIVE signed area (facing inward toward camera)
            let signed_area = (p1.0 - p0.0) * (p2.1 - p0.1) - (p2.0 - p0.0) * (p1.1 - p0.1);
            if signed_area >= 0.0 {
                continue; // Front-facing (outward) - skip when inside sphere
            }

            // Get vertex colors
            let c0 = vertices[face[0]].color;
            let c1 = vertices[face[1]].color;
            let c2 = vertices[face[2]].color;

            // Rasterize triangle with Gouraud-shaded vertex colors
            self.rasterize_skybox_triangle(
                (p0.0, p0.1), (p1.0, p1.1), (p2.0, p2.1),
                c0, c1, c2,
            );
        }

        // 2. Render stars (screen-space diamond sparkles)
        // Stars are rendered after the sphere (which now includes 3D mountains)
        if skybox.stars.enabled {
            self.render_stars(skybox, camera, time);
        }
    }

    /// Render star field as screen-space diamond sparkles
    fn render_stars(
        &mut self,
        skybox: &crate::world::Skybox,
        camera: &super::Camera,
        time: f32,
    ) {
        use std::f32::consts::PI;
        use super::math::{perspective_transform, project, Vec3};

        let stars = &skybox.stars;
        let mut rng_seed = stars.seed as u64;

        // Simple LCG for deterministic pseudo-random
        let mut next_rand = || -> f32 {
            rng_seed = rng_seed.wrapping_mul(1103515245).wrapping_add(12345);
            (rng_seed >> 16) as f32 / 65536.0
        };

        for _ in 0..stars.count {
            // Deterministic pseudo-random star positions
            let theta = next_rand() * 2.0 * PI;
            let phi_max = skybox.horizon * PI; // Only above horizon
            let phi = next_rand() * phi_max;

            // Convert spherical to world direction
            let y = phi.cos();
            let ring_radius = phi.sin();
            let x = ring_radius * theta.cos();
            let z = ring_radius * theta.sin();

            let dir = Vec3::new(x, y, z);

            // Transform to camera space
            let cam_space = perspective_transform(dir * 10000.0, camera.basis_x, camera.basis_y, camera.basis_z);

            if cam_space.z > 0.1 {
                let screen = project(cam_space, self.width, self.height);

                // Twinkle animation
                let mut brightness = 1.0f32;
                if stars.twinkle_speed > 0.0 {
                    let phase = next_rand() * 2.0 * PI;
                    brightness = 0.5 + 0.5 * (time * stars.twinkle_speed + phase).sin();
                }

                // Draw diamond sparkle
                let color = Color::new(
                    (stars.color.r as f32 * brightness) as u8,
                    (stars.color.g as f32 * brightness) as u8,
                    (stars.color.b as f32 * brightness) as u8,
                );
                self.draw_star_diamond(screen.x as i32, screen.y as i32, stars.size, color);
            }
        }
    }

    /// Draw a small diamond/cross star shape
    fn draw_star_diamond(&mut self, cx: i32, cy: i32, size: f32, color: Color) {
        let s = size.max(1.0) as i32;

        // Center pixel (always)
        self.set_pixel_safe(cx, cy, color);

        if s >= 2 {
            // 4-point diamond
            let dim_color = Color::new(
                (color.r as f32 * 0.7) as u8,
                (color.g as f32 * 0.7) as u8,
                (color.b as f32 * 0.7) as u8,
            );
            self.set_pixel_safe(cx - 1, cy, dim_color);
            self.set_pixel_safe(cx + 1, cy, dim_color);
            self.set_pixel_safe(cx, cy - 1, dim_color);
            self.set_pixel_safe(cx, cy + 1, dim_color);
        }

        if s >= 3 {
            // Extended points
            let faint_color = Color::new(
                (color.r as f32 * 0.4) as u8,
                (color.g as f32 * 0.4) as u8,
                (color.b as f32 * 0.4) as u8,
            );
            self.set_pixel_safe(cx - 2, cy, faint_color);
            self.set_pixel_safe(cx + 2, cy, faint_color);
            self.set_pixel_safe(cx, cy - 2, faint_color);
            self.set_pixel_safe(cx, cy + 2, faint_color);
        }
    }

    fn set_pixel_safe(&mut self, x: i32, y: i32, color: Color) {
        if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
            self.set_pixel(x as usize, y as usize, color);
        }
    }

    /// Rasterize a single skybox triangle with Gouraud vertex color interpolation
    /// No depth testing, no textures - just pure vertex colors
    #[allow(dead_code)]
    fn rasterize_skybox_triangle(
        &mut self,
        p0: (f32, f32),
        p1: (f32, f32),
        p2: (f32, f32),
        c0: Color,
        c1: Color,
        c2: Color,
    ) {
        // Calculate bounding box
        let min_x = p0.0.min(p1.0).min(p2.0).max(0.0) as usize;
        let max_x = p0.0.max(p1.0).max(p2.0).min(self.width as f32 - 1.0) as usize;
        let min_y = p0.1.min(p1.1).min(p2.1).max(0.0) as usize;
        let max_y = p0.1.max(p1.1).max(p2.1).min(self.height as f32 - 1.0) as usize;

        if min_x > max_x || min_y > max_y {
            return;
        }

        // Edge equations for barycentric coords
        let denom = (p1.1 - p2.1) * (p0.0 - p2.0) + (p2.0 - p1.0) * (p0.1 - p2.1);
        if denom.abs() < 0.0001 {
            return; // Degenerate triangle
        }
        let inv_denom = 1.0 / denom;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;

                // Barycentric coordinates
                let w0 = ((p1.1 - p2.1) * (px - p2.0) + (p2.0 - p1.0) * (py - p2.1)) * inv_denom;
                let w1 = ((p2.1 - p0.1) * (px - p2.0) + (p0.0 - p2.0) * (py - p2.1)) * inv_denom;
                let w2 = 1.0 - w0 - w1;

                // Check if inside triangle
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    // Interpolate vertex colors
                    let r = (c0.r as f32 * w0 + c1.r as f32 * w1 + c2.r as f32 * w2) as u8;
                    let g = (c0.g as f32 * w0 + c1.g as f32 * w1 + c2.g as f32 * w2) as u8;
                    let b = (c0.b as f32 * w0 + c1.b as f32 * w1 + c2.b as f32 * w2) as u8;

                    let idx = (y * self.width + x) * 4;
                    self.pixels[idx] = r;
                    self.pixels[idx + 1] = g;
                    self.pixels[idx + 2] = b;
                    self.pixels[idx + 3] = 255;
                }
            }
        }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) * 4;
            let bytes = color.to_bytes();
            self.pixels[idx] = bytes[0];
            self.pixels[idx + 1] = bytes[1];
            self.pixels[idx + 2] = bytes[2];
            self.pixels[idx + 3] = bytes[3];
        }
    }

    /// Set pixel with PS1-style blending
    pub fn set_pixel_blended(&mut self, x: usize, y: usize, color: Color, mode: BlendMode) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) * 4;

            // Read existing pixel (back) - framebuffer stores RGBA with 255 = opaque
            let back = Color::with_blend(
                self.pixels[idx],
                self.pixels[idx + 1],
                self.pixels[idx + 2],
                BlendMode::Opaque, // Framebuffer pixels are always opaque
            );

            // Blend and write
            let blended = color.blend(back, mode);
            let bytes = blended.to_bytes();
            self.pixels[idx] = bytes[0];
            self.pixels[idx + 1] = bytes[1];
            self.pixels[idx + 2] = bytes[2];
            self.pixels[idx + 3] = bytes[3];
        }
    }

    pub fn set_pixel_with_depth(&mut self, x: usize, y: usize, z: f32, color: Color) -> bool {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if z < self.zbuffer[idx] {
                self.zbuffer[idx] = z;
                let pixel_idx = idx * 4;
                let bytes = color.to_bytes();
                self.pixels[pixel_idx] = bytes[0];
                self.pixels[pixel_idx + 1] = bytes[1];
                self.pixels[pixel_idx + 2] = bytes[2];
                self.pixels[pixel_idx + 3] = bytes[3];
                return true;
            }
        }
        false
    }

    // =========================================================================
    // RGB555 (Color15) methods for PS1-authentic rendering
    // =========================================================================

    /// Set pixel using Color15 (RGB555)
    #[inline]
    pub fn set_pixel_15(&mut self, x: usize, y: usize, color: Color15) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) * 4;
            let rgba = color.to_rgba();
            self.pixels[idx] = rgba[0];
            self.pixels[idx + 1] = rgba[1];
            self.pixels[idx + 2] = rgba[2];
            self.pixels[idx + 3] = rgba[3];
        }
    }

    /// Set pixel with depth test using Color15 (RGB555)
    #[inline]
    pub fn set_pixel_with_depth_15(&mut self, x: usize, y: usize, z: f32, color: Color15) -> bool {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if z < self.zbuffer[idx] {
                self.zbuffer[idx] = z;
                let pixel_idx = idx * 4;
                let rgba = color.to_rgba();
                self.pixels[pixel_idx] = rgba[0];
                self.pixels[pixel_idx + 1] = rgba[1];
                self.pixels[pixel_idx + 2] = rgba[2];
                self.pixels[pixel_idx + 3] = rgba[3];
                return true;
            }
        }
        false
    }

    /// PS1-authentic blending using Color15
    /// If pixel's semi-transparency bit is set, apply face_blend_mode
    /// Otherwise, write directly (opaque)
    #[inline]
    pub fn set_pixel_blended_15(&mut self, x: usize, y: usize, color: Color15, face_blend_mode: BlendMode) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) * 4;

            // Read existing pixel (back) from framebuffer
            let back_r = self.pixels[idx];
            let back_g = self.pixels[idx + 1];
            let back_b = self.pixels[idx + 2];

            // Apply blending based on semi-transparency bit and face blend mode
            let (r, g, b) = if color.is_semi_transparent() {
                // Apply the face's blend mode
                blend_rgb555(color.r8(), color.g8(), color.b8(), back_r, back_g, back_b, face_blend_mode)
            } else {
                // Opaque - write directly
                (color.r8(), color.g8(), color.b8())
            };

            self.pixels[idx] = r;
            self.pixels[idx + 1] = g;
            self.pixels[idx + 2] = b;
            self.pixels[idx + 3] = 255;
        }
    }

    /// Set pixel with depth test and PS1-authentic blending using Color15
    #[inline]
    pub fn set_pixel_with_depth_blended_15(
        &mut self,
        x: usize,
        y: usize,
        z: f32,
        color: Color15,
        face_blend_mode: BlendMode,
    ) -> bool {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if z < self.zbuffer[idx] {
                self.zbuffer[idx] = z;

                let pixel_idx = idx * 4;
                let back_r = self.pixels[pixel_idx];
                let back_g = self.pixels[pixel_idx + 1];
                let back_b = self.pixels[pixel_idx + 2];

                let (r, g, b) = if color.is_semi_transparent() {
                    blend_rgb555(color.r8(), color.g8(), color.b8(), back_r, back_g, back_b, face_blend_mode)
                } else {
                    (color.r8(), color.g8(), color.b8())
                };

                self.pixels[pixel_idx] = r;
                self.pixels[pixel_idx + 1] = g;
                self.pixels[pixel_idx + 2] = b;
                self.pixels[pixel_idx + 3] = 255;
                return true;
            }
        }
        false
    }

    /// Draw a filled circle at (cx, cy) with given radius and color
    pub fn draw_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color) {
        let r_sq = radius * radius;
        for y in (cy - radius).max(0)..=(cy + radius).min(self.height as i32 - 1) {
            for x in (cx - radius).max(0)..=(cx + radius).min(self.width as i32 - 1) {
                let dx = x - cx;
                let dy = y - cy;
                if dx * dx + dy * dy <= r_sq {
                    self.set_pixel(x as usize, y as usize, color);
                }
            }
        }
    }

    /// Draw a line from (x0, y0) to (x1, y1) using Bresenham's algorithm
    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        self.draw_line_blended(x0, y0, x1, y1, color, BlendMode::Opaque);
    }

    /// Draw a line with PS1-style blending
    pub fn draw_line_blended(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color, mode: BlendMode) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
                if mode == BlendMode::Opaque {
                    self.set_pixel(x as usize, y as usize, color);
                } else {
                    self.set_pixel_blended(x as usize, y as usize, color, mode);
                }
            }

            if x == x1 && y == y1 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a line with depth testing (respects z-buffer)
    /// z0 and z1 are the depth values at each endpoint (smaller = closer)
    /// Uses strict less-than comparison - line must be in front of geometry
    pub fn draw_line_3d(&mut self, x0: i32, y0: i32, z0: f32, x1: i32, y1: i32, z1: f32, color: Color) {
        self.draw_line_3d_impl(x0, y0, z0, x1, y1, z1, color, false);
    }

    /// Draw a line with depth testing, allowing co-planar drawing
    /// Uses less-than-or-equal comparison - draws on surfaces at same depth
    /// Ideal for wireframe overlays on geometry
    pub fn draw_line_3d_overlay(&mut self, x0: i32, y0: i32, z0: f32, x1: i32, y1: i32, z1: f32, color: Color) {
        self.draw_line_3d_impl(x0, y0, z0, x1, y1, z1, color, true);
    }

    fn draw_line_3d_impl(&mut self, x0: i32, y0: i32, z0: f32, x1: i32, y1: i32, z1: f32, color: Color, allow_equal: bool) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;

        // Calculate total steps for interpolation
        let total_steps = dx.max((-dy).max(1)) as f32;
        let mut step = 0.0f32;

        loop {
            if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
                // Interpolate depth along the line
                let t = step / total_steps;
                let z = z0 + t * (z1 - z0);

                // Use depth test
                let idx = y as usize * self.width + x as usize;
                let passes = if allow_equal {
                    z <= self.zbuffer[idx]  // Draw on co-planar surfaces
                } else {
                    z < self.zbuffer[idx]   // Only draw if strictly in front
                };
                if passes {
                    self.set_pixel(x as usize, y as usize, color);
                }
            }

            if x == x1 && y == y1 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
                step += 1.0;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
                if e2 < dy {
                    step += 1.0;
                }
            }
        }
    }

    /// Draw a thick line as a filled quad
    pub fn draw_thick_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, thickness: i32, color: Color) {
        if thickness <= 1 {
            self.draw_line(x0, y0, x1, y1, color);
            return;
        }

        // Calculate perpendicular offset vector
        let dx = (x1 - x0) as f32;
        let dy = (y1 - y0) as f32;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.001 {
            return;
        }

        let half = thickness as f32 * 0.5;
        let px = -dy / len * half;
        let py = dx / len * half;

        // Four corners of the thick line quad
        let corners = [
            (x0 as f32 + px, y0 as f32 + py),
            (x0 as f32 - px, y0 as f32 - py),
            (x1 as f32 - px, y1 as f32 - py),
            (x1 as f32 + px, y1 as f32 + py),
        ];

        // Find bounding box
        let min_x = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min) as i32;
        let max_x = corners.iter().map(|c| c.0).fold(f32::NEG_INFINITY, f32::max) as i32;
        let min_y = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min) as i32;
        let max_y = corners.iter().map(|c| c.1).fold(f32::NEG_INFINITY, f32::max) as i32;

        // Rasterize quad using scanline - test each pixel in bounding box
        for py in min_y..=max_y {
            for px in min_x..=max_x {
                if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                    // Point-in-quad test using cross products (convex quad)
                    let p = (px as f32 + 0.5, py as f32 + 0.5);
                    let mut inside = true;
                    for i in 0..4 {
                        let a = corners[i];
                        let b = corners[(i + 1) % 4];
                        let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
                        if cross < 0.0 {
                            inside = false;
                            break;
                        }
                    }
                    if inside {
                        self.set_pixel(px as usize, py as usize, color);
                    }
                }
            }
        }
    }

    /// Draw a rectangle outline from (x0, y0) to (x1, y1)
    pub fn draw_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        // Normalize coordinates
        let (min_x, max_x) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
        let (min_y, max_y) = if y0 < y1 { (y0, y1) } else { (y1, y0) };

        // Draw four edges
        self.draw_line(min_x, min_y, max_x, min_y, color); // Top
        self.draw_line(max_x, min_y, max_x, max_y, color); // Right
        self.draw_line(max_x, max_y, min_x, max_y, color); // Bottom
        self.draw_line(min_x, max_y, min_x, min_y, color); // Left
    }

    /// Draw a filled rectangle from (x0, y0) to (x1, y1) with semi-transparent color
    pub fn draw_filled_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        // Normalize coordinates
        let (min_x, max_x) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
        let (min_y, max_y) = if y0 < y1 { (y0, y1) } else { (y1, y0) };

        // Clamp to framebuffer bounds
        let min_x = min_x.max(0);
        let min_y = min_y.max(0);
        let max_x = max_x.min(self.width as i32 - 1);
        let max_y = max_y.min(self.height as i32 - 1);

        // Fill rectangle
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                self.set_pixel(x as usize, y as usize, color);
            }
        }
    }
}

/// Camera state
pub struct Camera {
    pub position: Vec3,
    pub rotation_x: f32, // Pitch
    pub rotation_y: f32, // Yaw

    // Computed basis vectors
    pub basis_x: Vec3,
    pub basis_y: Vec3,
    pub basis_z: Vec3,
}

impl Camera {
    pub fn new() -> Self {
        let mut cam = Self {
            position: Vec3::ZERO,
            rotation_x: 0.0,
            rotation_y: 0.0,
            basis_x: Vec3::new(1.0, 0.0, 0.0),
            basis_y: Vec3::new(0.0, 1.0, 0.0),
            basis_z: Vec3::new(0.0, 0.0, 1.0),
        };
        cam.update_basis();
        cam
    }

    /// Create a camera looking down the Y axis (top-down view, XZ plane)
    pub fn ortho_top() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation_x: 0.0,
            rotation_y: 0.0,
            // Looking down -Y, so: right=+X, up=+Z, forward=-Y
            basis_x: Vec3::new(1.0, 0.0, 0.0),   // Right
            basis_y: Vec3::new(0.0, 0.0, 1.0),   // Up (Z goes up on screen)
            basis_z: Vec3::new(0.0, -1.0, 0.0),  // Forward (into the scene)
        }
    }

    /// Create a camera looking down the Z axis (front view, XY plane)
    pub fn ortho_front() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation_x: 0.0,
            rotation_y: 0.0,
            // Looking down -Z, so: right=+X, up=+Y, forward=-Z
            // Don't negate Y here - project_ortho handles the screen Y flip
            basis_x: Vec3::new(1.0, 0.0, 0.0),   // Right: world +X -> camera +X
            basis_y: Vec3::new(0.0, 1.0, 0.0),   // Up: world +Y -> camera +Y
            basis_z: Vec3::new(0.0, 0.0, -1.0),  // Forward (into the scene)
        }
    }

    /// Create a camera looking down the X axis (side view, ZY plane)
    pub fn ortho_side() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation_x: 0.0,
            rotation_y: 0.0,
            // Looking down -X, so: right=+Z, up=+Y, forward=-X
            // Wireframe uses (v.pos.z, v.pos.y), so:
            // cam_pos.x should be v.pos.z, cam_pos.y should be v.pos.y
            basis_x: Vec3::new(0.0, 0.0, 1.0),   // Right: world +Z -> camera +X
            basis_y: Vec3::new(0.0, 1.0, 0.0),   // Up: world +Y -> camera +Y
            basis_z: Vec3::new(-1.0, 0.0, 0.0),  // Forward (into the scene)
        }
    }

    pub fn update_basis(&mut self) {
        let upward = Vec3::new(0.0, -1.0, 0.0);  // Use -Y as up to match screen coordinates

        // Forward vector based on rotation
        self.basis_z = Vec3 {
            x: self.rotation_x.cos() * self.rotation_y.sin(),
            y: -self.rotation_x.sin(),  // Back to original with negation
            z: self.rotation_x.cos() * self.rotation_y.cos(),
        };

        // Right vector
        self.basis_x = upward.cross(self.basis_z).normalize();

        // Up vector
        self.basis_y = self.basis_z.cross(self.basis_x);
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.rotation_y += dy;
        self.rotation_x = (self.rotation_x + dx).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );
        self.update_basis();
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

/// Projected surface (triangle ready for rasterization)
struct Surface {
    pub v1: Vec3, // Screen-space vertex 1
    pub v2: Vec3, // Screen-space vertex 2
    pub v3: Vec3, // Screen-space vertex 3
    pub w1: Vec3, // World-space vertex 1 (for point light calculations)
    pub w2: Vec3, // World-space vertex 2
    pub w3: Vec3, // World-space vertex 3
    pub vn1: Vec3, // Vertex normal 1 (camera space)
    pub vn2: Vec3, // Vertex normal 2
    pub vn3: Vec3, // Vertex normal 3
    pub wn1: Vec3, // World-space vertex normal 1 (for point light calculations)
    pub wn2: Vec3, // World-space vertex normal 2
    pub wn3: Vec3, // World-space vertex normal 3
    pub uv1: super::math::Vec2,
    pub uv2: super::math::Vec2,
    pub uv3: super::math::Vec2,
    pub vc1: Color, // Vertex color 1 (for PS1 texture modulation)
    pub vc2: Color, // Vertex color 2
    pub vc3: Color, // Vertex color 3
    pub normal: Vec3, // Face normal (camera space)
    pub face_idx: usize,
    pub black_transparent: bool, // If true, black pixels are transparent (PS1 CLUT-style)
    pub has_transparency: bool,  // True if this face uses semi-transparency (for two-pass rendering)
}

/// Calculate shading intensity from a single directional light
#[allow(dead_code)]
fn shade_intensity_directional(normal: Vec3, light_dir: Vec3, ambient: f32) -> f32 {
    let neg_light_dir = light_dir.scale(-1.0);
    let diffuse = normal.dot(neg_light_dir).max(0.0);
    (ambient + (1.0 - ambient) * diffuse).clamp(0.0, 1.0)
}

/// Calculate shading color from multiple lights (with colored light support)
/// Returns RGB values 0.0-1.0 for each channel
/// For per-vertex shading (Gouraud), world_pos can be approximate (vertex position)
fn shade_multi_light_color(normal: Vec3, world_pos: Vec3, lights: &[Light], ambient: f32) -> (f32, f32, f32) {
    let mut total_r = ambient;
    let mut total_g = ambient;
    let mut total_b = ambient;

    for light in lights.iter().filter(|l| l.enabled) {
        let contribution = match &light.light_type {
            LightType::Directional { direction } => {
                // Directional: same intensity everywhere
                let neg_dir = direction.scale(-1.0);
                let n_dot_l = normal.dot(neg_dir).max(0.0);
                n_dot_l * light.intensity
            }
            LightType::Point { position, radius } => {
                // Point: intensity falls off with distance
                let to_light = *position - world_pos;
                let dist = to_light.len();
                if dist > *radius || dist < 0.001 {
                    0.0
                } else {
                    let attenuation = 1.0 - (dist / radius);
                    let n_dot_l = normal.dot(to_light.normalize()).max(0.0);
                    n_dot_l * light.intensity * attenuation * attenuation // squared falloff
                }
            }
            LightType::Spot { position, direction, angle, radius } => {
                // Spot: point light with cone restriction
                let to_light = *position - world_pos;
                let dist = to_light.len();
                if dist > *radius || dist < 0.001 {
                    0.0
                } else {
                    let light_dir_to_surface = to_light.normalize();
                    let neg_light_dir = light_dir_to_surface.scale(-1.0);
                    let spot_angle = neg_light_dir.dot(*direction).acos();

                    if spot_angle > *angle {
                        0.0
                    } else {
                        let attenuation = 1.0 - (dist / radius);
                        let edge_falloff = 1.0 - (spot_angle / angle);
                        let n_dot_l = normal.dot(light_dir_to_surface).max(0.0);
                        n_dot_l * light.intensity * attenuation * attenuation * edge_falloff
                    }
                }
            }
        };

        // Apply light color to contribution
        let light_r = light.color.r as f32 / 255.0;
        let light_g = light.color.g as f32 / 255.0;
        let light_b = light.color.b as f32 / 255.0;
        total_r += contribution * light_r;
        total_g += contribution * light_g;
        total_b += contribution * light_b;
    }

    (total_r.min(1.0), total_g.min(1.0), total_b.min(1.0))
}

/// Apply RGB shading to a color
fn shade_color_rgb(color: Color, shade_r: f32, shade_g: f32, shade_b: f32) -> Color {
    Color::with_blend(
        (color.r as f32 * shade_r).min(255.0) as u8,
        (color.g as f32 * shade_g).min(255.0) as u8,
        (color.b as f32 * shade_b).min(255.0) as u8,
        color.blend,
    )
}

// =============================================================================
// RGB555 Helper Functions
// =============================================================================

/// PS1-authentic blend operation for RGB555 colors
/// front = new pixel, back = existing framebuffer pixel
/// Returns blended (r, g, b) tuple, quantized to 5-bit and expanded back to 8-bit
///
/// PS1 GPU performed blending in 5-bit space, so results must be quantized
#[inline]
fn blend_rgb555(front_r: u8, front_g: u8, front_b: u8, back_r: u8, back_g: u8, back_b: u8, mode: BlendMode) -> (u8, u8, u8) {
    // Convert inputs to 5-bit (they should already be quantized, but ensure consistency)
    let f_r5 = front_r >> 3;
    let f_g5 = front_g >> 3;
    let f_b5 = front_b >> 3;
    let b_r5 = back_r >> 3;
    let b_g5 = back_g >> 3;
    let b_b5 = back_b >> 3;

    // Perform blending in 5-bit space (PS1 authentic)
    let (r5, g5, b5) = match mode {
        BlendMode::Opaque => (f_r5, f_g5, f_b5),
        BlendMode::Average => {
            // Mode 0: 0.5*B + 0.5*F (in 5-bit space)
            (
                ((b_r5 as u16 + f_r5 as u16) / 2).min(31) as u8,
                ((b_g5 as u16 + f_g5 as u16) / 2).min(31) as u8,
                ((b_b5 as u16 + f_b5 as u16) / 2).min(31) as u8,
            )
        }
        BlendMode::Add => {
            // Mode 1: B + F (clamped to 31)
            (
                (b_r5 as u16 + f_r5 as u16).min(31) as u8,
                (b_g5 as u16 + f_g5 as u16).min(31) as u8,
                (b_b5 as u16 + f_b5 as u16).min(31) as u8,
            )
        }
        BlendMode::Subtract => {
            // Mode 2: B - F (clamped to 0)
            (
                (b_r5 as i16 - f_r5 as i16).max(0) as u8,
                (b_g5 as i16 - f_g5 as i16).max(0) as u8,
                (b_b5 as i16 - f_b5 as i16).max(0) as u8,
            )
        }
        BlendMode::AddQuarter => {
            // Mode 3: B + 0.25*F (clamped to 31)
            (
                (b_r5 as u16 + f_r5 as u16 / 4).min(31) as u8,
                (b_g5 as u16 + f_g5 as u16 / 4).min(31) as u8,
                (b_b5 as u16 + f_b5 as u16 / 4).min(31) as u8,
            )
        }
        BlendMode::Erase => {
            // Erase: transparent (return back unchanged)
            (b_r5, b_g5, b_b5)
        }
    };

    // Expand back to 8-bit (quantized output)
    (r5 << 3, g5 << 3, b5 << 3)
}

/// PS1 GPU dither matrix (authentic signed values -4 to +3)
/// Verified against psx-spx specifications and Duckstation emulator
/// Applied to 8-bit color values before quantization to 5-bit
const PS1_DITHER_MATRIX: [[i8; 4]; 4] = [
    [-4,  0, -3,  1],
    [ 2, -2,  3, -1],
    [-3,  1, -4,  0],
    [ 3, -1,  2, -2],
];

/// Expand 5-bit color to 8-bit with proper range (0-31 → 0-255)
/// Uses the standard formula: (v5 << 3) | (v5 >> 2)
/// This gives: 0→0, 1→8, 2→16, ..., 31→255
#[inline]
fn expand_5_to_8(v5: u8) -> u8 {
    (v5 << 3) | (v5 >> 2)
}

/// Apply PS1-authentic dithering during 8-bit to 5-bit quantization
/// Takes 8-bit RGB values, applies dither offset, returns 5-bit values
///
/// Hardware behavior (verified against Duckstation):
/// 1. Add dither offset to 8-bit value (can go negative or >255)
/// 2. Right-shift by 3 (divide by 8)
/// 3. Clamp result to 0-31
#[inline]
fn dither_and_quantize(r8: u8, g8: u8, b8: u8, x: usize, y: usize) -> (u8, u8, u8) {
    let offset = PS1_DITHER_MATRIX[y & 3][x & 3] as i32;

    // Add offset, shift, then clamp to 5-bit range (matches hardware)
    let r5 = ((r8 as i32 + offset) >> 3).clamp(0, 31) as u8;
    let g5 = ((g8 as i32 + offset) >> 3).clamp(0, 31) as u8;
    let b5 = ((b8 as i32 + offset) >> 3).clamp(0, 31) as u8;

    (r5, g5, b5)
}

/// Apply PS1-authentic ordered dithering to an 8-bit color
/// Uses the authentic PS1 dither matrix and algorithm
fn apply_dither(color: Color, x: usize, y: usize) -> Color {
    let offset = PS1_DITHER_MATRIX[y & 3][x & 3] as i32;

    // Add offset, shift by 3, clamp to 5-bit, then expand back to 8-bit
    // This matches PS1 hardware behavior
    let r5 = ((color.r as i32 + offset) >> 3).clamp(0, 31) as u8;
    let g5 = ((color.g as i32 + offset) >> 3).clamp(0, 31) as u8;
    let b5 = ((color.b as i32 + offset) >> 3).clamp(0, 31) as u8;

    // Convert back to 8-bit (keeping the quantized look)
    Color::with_blend(r5 << 3, g5 << 3, b5 << 3, color.blend)
}

/// Rasterize a single triangle using incremental barycentric stepping.
/// Uses edge function increments instead of recalculating barycentric
/// coordinates per-pixel for better performance.
fn rasterize_triangle(
    fb: &mut Framebuffer,
    surface: &Surface,
    texture: Option<&Texture>,
    settings: &RasterSettings,
) {
    // Bounding box (same as original)
    let min_x = surface.v1.x.min(surface.v2.x).min(surface.v3.x).max(0.0) as usize;
    let max_x = (surface.v1.x.max(surface.v2.x).max(surface.v3.x) + 1.0).min(fb.width as f32) as usize;
    let min_y = surface.v1.y.min(surface.v2.y).min(surface.v3.y).max(0.0) as usize;
    let max_y = (surface.v1.y.max(surface.v2.y).max(surface.v3.y) + 1.0).min(fb.height as f32) as usize;

    // Early exit for degenerate/off-screen triangles
    if min_x >= max_x || min_y >= max_y {
        return;
    }

    // Pre-calculate flat shading if needed (same as original)
    let flat_shade = if settings.shading == ShadingMode::Flat {
        let center_pos = (surface.w1 + surface.w2 + surface.w3).scale(1.0 / 3.0);
        let world_normal = (surface.wn1 + surface.wn2 + surface.wn3).scale(1.0 / 3.0).normalize();
        shade_multi_light_color(world_normal, center_pos, &settings.lights, settings.ambient)
    } else {
        (1.0, 1.0, 1.0)
    };

    // Pre-compute Gouraud vertex shading if needed
    let gouraud_shades = if settings.shading == ShadingMode::Gouraud {
        Some((
            shade_multi_light_color(surface.wn1, surface.w1, &settings.lights, settings.ambient),
            shade_multi_light_color(surface.wn2, surface.w2, &settings.lights, settings.ambient),
            shade_multi_light_color(surface.wn3, surface.w3, &settings.lights, settings.ambient),
        ))
    } else {
        None
    };

    // === EDGE FUNCTION SETUP ===
    // Edge function: E(x,y) = (y1-y2)*x + (x2-x1)*y + (x1*y2 - x2*y1)
    // For barycentric: bc.x = E23/area, bc.y = E31/area, bc.z = E12/area

    let v1 = surface.v1;
    let v2 = surface.v2;
    let v3 = surface.v3;

    // Triangle area * 2 (used for normalization)
    let area = (v2.y - v3.y) * (v1.x - v3.x) + (v3.x - v2.x) * (v1.y - v3.y);
    if area.abs() < 0.00001 {
        return; // Degenerate triangle
    }
    let inv_area = 1.0 / area;

    // Edge function coefficients for barycentric coordinate bc.x (weight for v1)
    // E23: edge from v2 to v3
    let a0 = v2.y - v3.y;
    let b0 = v3.x - v2.x;
    // Edge function coefficients for barycentric coordinate bc.y (weight for v2)
    // E31: edge from v3 to v1
    let a1 = v3.y - v1.y;
    let b1 = v1.x - v3.x;
    // bc.z = 1 - bc.x - bc.y (no need to compute separately)

    // Starting point
    let start_x = min_x as f32;
    let start_y = min_y as f32;

    // Initial edge function values at (start_x, start_y)
    let w0_row_start = a0 * (start_x - v3.x) + b0 * (start_y - v3.y);
    let w1_row_start = a1 * (start_x - v3.x) + b1 * (start_y - v3.y);

    // Step increments
    let a0_step = a0; // x step for w0
    let b0_step = b0; // y step for w0
    let a1_step = a1; // x step for w1
    let b1_step = b1; // y step for w1

    let mut w0_row = w0_row_start;
    let mut w1_row = w1_row_start;

    // Rasterize using incremental edge functions
    for y in min_y..max_y {
        let mut w0 = w0_row;
        let mut w1 = w1_row;

        for x in min_x..max_x {
            // Convert to barycentric coordinates
            let bc_x = w0 * inv_area;
            let bc_y = w1 * inv_area;
            let bc_z = 1.0 - bc_x - bc_y;

            // Check if inside triangle (same threshold as original)
            const ERR: f32 = -0.0001;
            if bc_x >= ERR && bc_y >= ERR && bc_z >= ERR {
                // Interpolate depth
                let z = bc_x * v1.z + bc_y * v2.z + bc_z * v3.z;

                // Z-buffer test
                if settings.use_zbuffer {
                    let idx = y * fb.width + x;
                    if z >= fb.zbuffer[idx] {
                        w0 += a0_step;
                        w1 += a1_step;
                        continue;
                    }
                }

                // Interpolate UV coordinates
                let (u, v) = if settings.affine_textures {
                    // Affine (PS1 style) - linear interpolation
                    let u = bc_x * surface.uv1.x + bc_y * surface.uv2.x + bc_z * surface.uv3.x;
                    let v = bc_x * surface.uv1.y + bc_y * surface.uv2.y + bc_z * surface.uv3.y;
                    (u, v)
                } else {
                    // Perspective-correct interpolation
                    let bcc_x = bc_x / v1.z;
                    let bcc_y = bc_y / v2.z;
                    let bcc_z = bc_z / v3.z;
                    let bd = bcc_x + bcc_y + bcc_z;
                    let bcc_x = bcc_x / bd;
                    let bcc_y = bcc_y / bd;
                    let bcc_z = bcc_z / bd;

                    let u = bcc_x * surface.uv1.x + bcc_y * surface.uv2.x + bcc_z * surface.uv3.x;
                    let v = bcc_x * surface.uv1.y + bcc_y * surface.uv2.y + bcc_z * surface.uv3.y;
                    (u, v)
                };

                // Sample texture or use white
                let mut color = if let Some(tex) = texture {
                    tex.sample(u, 1.0 - v)
                } else {
                    Color::WHITE
                };

                // Skip transparent pixels
                if color.is_transparent() {
                    w0 += a0_step;
                    w1 += a1_step;
                    continue;
                }

                // Interpolate vertex colors (PS1-style Gouraud for color)
                let vertex_color = Color {
                    r: (bc_x * surface.vc1.r as f32 + bc_y * surface.vc2.r as f32 + bc_z * surface.vc3.r as f32) as u8,
                    g: (bc_x * surface.vc1.g as f32 + bc_y * surface.vc2.g as f32 + bc_z * surface.vc3.g as f32) as u8,
                    b: (bc_x * surface.vc1.b as f32 + bc_y * surface.vc2.b as f32 + bc_z * surface.vc3.b as f32) as u8,
                    blend: BlendMode::Opaque,
                };

                // Apply PS1-style texture modulation
                color = color.modulate(vertex_color);

                // Apply shading (lighting)
                let (shade_r, shade_g, shade_b) = match settings.shading {
                    ShadingMode::None => (1.0, 1.0, 1.0),
                    ShadingMode::Flat => flat_shade,
                    ShadingMode::Gouraud => {
                        // Use pre-computed vertex shading
                        let ((r1, g1, b1), (r2, g2, b2), (r3, g3, b3)) = gouraud_shades.unwrap();
                        (
                            bc_x * r1 + bc_y * r2 + bc_z * r3,
                            bc_x * g1 + bc_y * g2 + bc_z * g3,
                            bc_x * b1 + bc_y * b2 + bc_z * b3,
                        )
                    }
                };

                color = shade_color_rgb(color, shade_r, shade_g, shade_b);

                // Apply PS1-style ordered dithering
                if settings.dithering {
                    color = apply_dither(color, x, y);
                }

                // Write pixel
                if color.blend == BlendMode::Opaque {
                    fb.set_pixel_with_depth(x, y, z, color);
                } else {
                    let idx = y * fb.width + x;
                    if z < fb.zbuffer[idx] {
                        fb.zbuffer[idx] = z;
                        fb.set_pixel_blended(x, y, color, color.blend);
                    }
                }
            }

            // Step to next pixel (x increment)
            w0 += a0_step;
            w1 += a1_step;
        }

        // Step to next row (y increment)
        w0_row += b0_step;
        w1_row += b1_step;
    }
}

/// Rasterize a single triangle using RGB555 (PS1-authentic mode)
/// Uses Color15 for texture sampling and Color15 for pixel output
/// Texture's blend_mode is used when texture pixel has semi-transparency bit set
/// Falls back to face_blend_mode if texture has no blend mode
/// black_transparent: if true, pure black pixels (before shading) are skipped as transparent
fn rasterize_triangle_15(
    fb: &mut Framebuffer,
    surface: &Surface,
    texture: Option<&Texture15>,
    face_blend_mode: BlendMode,
    black_transparent: bool,
    settings: &RasterSettings,
    skip_z_write: bool,  // If true, don't update z-buffer (for semi-transparent pass)
) {
    // Use texture's blend mode if available, otherwise face_blend_mode
    let blend_mode = texture
        .map(|t| t.blend_mode)
        .unwrap_or(face_blend_mode);

    // Bounding box
    let min_x = surface.v1.x.min(surface.v2.x).min(surface.v3.x).max(0.0) as usize;
    let max_x = (surface.v1.x.max(surface.v2.x).max(surface.v3.x) + 1.0).min(fb.width as f32) as usize;
    let min_y = surface.v1.y.min(surface.v2.y).min(surface.v3.y).max(0.0) as usize;
    let max_y = (surface.v1.y.max(surface.v2.y).max(surface.v3.y) + 1.0).min(fb.height as f32) as usize;

    // Early exit for degenerate/off-screen triangles
    if min_x >= max_x || min_y >= max_y {
        return;
    }

    // Pre-calculate flat shading if needed
    let flat_shade = if settings.shading == ShadingMode::Flat {
        let center_pos = (surface.w1 + surface.w2 + surface.w3).scale(1.0 / 3.0);
        let world_normal = (surface.wn1 + surface.wn2 + surface.wn3).scale(1.0 / 3.0).normalize();
        shade_multi_light_color(world_normal, center_pos, &settings.lights, settings.ambient)
    } else {
        (1.0, 1.0, 1.0)
    };

    // Pre-compute Gouraud vertex shading if needed
    let gouraud_shades = if settings.shading == ShadingMode::Gouraud {
        Some((
            shade_multi_light_color(surface.wn1, surface.w1, &settings.lights, settings.ambient),
            shade_multi_light_color(surface.wn2, surface.w2, &settings.lights, settings.ambient),
            shade_multi_light_color(surface.wn3, surface.w3, &settings.lights, settings.ambient),
        ))
    } else {
        None
    };

    // === EDGE FUNCTION SETUP ===
    let v1 = surface.v1;
    let v2 = surface.v2;
    let v3 = surface.v3;

    // Triangle area * 2 (used for normalization)
    let area = (v2.y - v3.y) * (v1.x - v3.x) + (v3.x - v2.x) * (v1.y - v3.y);
    if area.abs() < 0.00001 {
        return; // Degenerate triangle
    }
    let inv_area = 1.0 / area;

    // Edge function coefficients
    let a0 = v2.y - v3.y;
    let b0 = v3.x - v2.x;
    let a1 = v3.y - v1.y;
    let b1 = v1.x - v3.x;

    // Starting point
    let start_x = min_x as f32;
    let start_y = min_y as f32;

    // Initial edge function values at (start_x, start_y)
    let w0_row_start = a0 * (start_x - v3.x) + b0 * (start_y - v3.y);
    let w1_row_start = a1 * (start_x - v3.x) + b1 * (start_y - v3.y);

    // Step increments
    let a0_step = a0;
    let b0_step = b0;
    let a1_step = a1;
    let b1_step = b1;

    let mut w0_row = w0_row_start;
    let mut w1_row = w1_row_start;

    // Rasterize using incremental edge functions
    for y in min_y..max_y {
        let mut w0 = w0_row;
        let mut w1 = w1_row;

        for x in min_x..max_x {
            // Convert to barycentric coordinates
            let bc_x = w0 * inv_area;
            let bc_y = w1 * inv_area;
            let bc_z = 1.0 - bc_x - bc_y;

            // Check if inside triangle
            const ERR: f32 = -0.0001;
            if bc_x >= ERR && bc_y >= ERR && bc_z >= ERR {
                // Interpolate depth
                let z = bc_x * v1.z + bc_y * v2.z + bc_z * v3.z;

                // Z-buffer test
                if settings.use_zbuffer {
                    let idx = y * fb.width + x;
                    if z >= fb.zbuffer[idx] {
                        w0 += a0_step;
                        w1 += a1_step;
                        continue;
                    }
                }

                // Interpolate UV coordinates
                let (u, v) = if settings.affine_textures {
                    // Affine (PS1 style) - linear interpolation
                    let u = bc_x * surface.uv1.x + bc_y * surface.uv2.x + bc_z * surface.uv3.x;
                    let v = bc_x * surface.uv1.y + bc_y * surface.uv2.y + bc_z * surface.uv3.y;
                    (u, v)
                } else {
                    // Perspective-correct interpolation
                    let bcc_x = bc_x / v1.z;
                    let bcc_y = bc_y / v2.z;
                    let bcc_z = bc_z / v3.z;
                    let bd = bcc_x + bcc_y + bcc_z;
                    let bcc_x = bcc_x / bd;
                    let bcc_y = bcc_y / bd;
                    let bcc_z = bcc_z / bd;

                    let u = bcc_x * surface.uv1.x + bcc_y * surface.uv2.x + bcc_z * surface.uv3.x;
                    let v = bcc_x * surface.uv1.y + bcc_y * surface.uv2.y + bcc_z * surface.uv3.y;
                    (u, v)
                };

                // Sample texture (RGB555) or use white
                let mut color = if let Some(tex) = texture {
                    tex.sample(u, 1.0 - v)
                } else {
                    Color15::WHITE
                };

                // Handle transparency based on black_transparent setting:
                // - If black_transparent = true: skip both 0x0000 (transparent) and black pixels (r=g=b=0)
                // - If black_transparent = false: only skip 0x0000, but render black pixels (convert to 0x8000)
                let is_black = color.r5() == 0 && color.g5() == 0 && color.b5() == 0;
                if color.is_transparent() {
                    if is_black && !black_transparent {
                        // Convert transparent black to drawable black
                        color = Color15::BLACK_DRAWABLE;
                    } else {
                        // Skip truly transparent pixels
                        w0 += a0_step;
                        w1 += a1_step;
                        continue;
                    }
                } else if black_transparent && is_black {
                    // Skip black pixels when black_transparent is enabled
                    w0 += a0_step;
                    w1 += a1_step;
                    continue;
                }

                // === PS1-AUTHENTIC COLOR PIPELINE ===
                // All calculations happen in 8-bit space, dithering applied during final quantization

                // Expand texture from 5-bit to 8-bit for internal calculations
                let tex_r8 = expand_5_to_8(color.r5());
                let tex_g8 = expand_5_to_8(color.g5());
                let tex_b8 = expand_5_to_8(color.b5());

                // Interpolate vertex colors (already 8-bit)
                let vertex_r = (bc_x * surface.vc1.r as f32 + bc_y * surface.vc2.r as f32 + bc_z * surface.vc3.r as f32) as u8;
                let vertex_g = (bc_x * surface.vc1.g as f32 + bc_y * surface.vc2.g as f32 + bc_z * surface.vc3.g as f32) as u8;
                let vertex_b = (bc_x * surface.vc1.b as f32 + bc_y * surface.vc2.b as f32 + bc_z * surface.vc3.b as f32) as u8;

                // Apply PS1-style texture modulation in 8-bit space
                // Formula: (texture * vertex_color) / 128, clamped to 255
                let mod_r8 = ((tex_r8 as u32 * vertex_r as u32) / 128).min(255) as u8;
                let mod_g8 = ((tex_g8 as u32 * vertex_g as u32) / 128).min(255) as u8;
                let mod_b8 = ((tex_b8 as u32 * vertex_b as u32) / 128).min(255) as u8;

                // Apply shading (lighting) in 8-bit space
                let (shade_r, shade_g, shade_b) = match settings.shading {
                    ShadingMode::None => (1.0, 1.0, 1.0),
                    ShadingMode::Flat => flat_shade,
                    ShadingMode::Gouraud => {
                        let ((r1, g1, b1), (r2, g2, b2), (r3, g3, b3)) = gouraud_shades.unwrap();
                        (
                            bc_x * r1 + bc_y * r2 + bc_z * r3,
                            bc_x * g1 + bc_y * g2 + bc_z * g3,
                            bc_x * b1 + bc_y * b2 + bc_z * b3,
                        )
                    }
                };

                // Apply shading to get final 8-bit values (clamp shading to 2.0 for overbright)
                let shaded_r8 = (mod_r8 as f32 * shade_r.clamp(0.0, 2.0)).min(255.0) as u8;
                let shaded_g8 = (mod_g8 as f32 * shade_g.clamp(0.0, 2.0)).min(255.0) as u8;
                let shaded_b8 = (mod_b8 as f32 * shade_b.clamp(0.0, 2.0)).min(255.0) as u8;

                // Final quantization: dither (if enabled) and convert 8-bit to 5-bit
                let (r5, g5, b5) = if settings.dithering {
                    dither_and_quantize(shaded_r8, shaded_g8, shaded_b8, x, y)
                } else {
                    // Simple truncation without dithering
                    (shaded_r8 >> 3, shaded_g8 >> 3, shaded_b8 >> 3)
                };

                // Create final color, preserving semi-transparency from original texture
                // IMPORTANT: If final color is all-black (r5=g5=b5=0), we must set bit 15
                // to make it "drawable black" (0x8000) instead of "transparent black" (0x0000)
                let is_all_black = r5 == 0 && g5 == 0 && b5 == 0;
                let semi = color.is_semi_transparent() || is_all_black;
                let color = Color15::new_semi(r5, g5, b5, semi);

                // Write pixel with PS1-authentic semi-transparency handling
                // If pixel's semi-transparency bit is set, use texture's blend_mode
                let idx = y * fb.width + x;
                if z < fb.zbuffer[idx] {
                    // Only update z-buffer if not skipping (opaque pass updates, transparent pass doesn't)
                    if !skip_z_write {
                        fb.zbuffer[idx] = z;
                    }
                    if color.is_semi_transparent() && blend_mode != BlendMode::Opaque {
                        fb.set_pixel_blended_15(x, y, color, blend_mode);
                    } else {
                        fb.set_pixel_15(x, y, color);
                    }
                }
            }

            // Step to next pixel (x increment)
            w0 += a0_step;
            w1 += a1_step;
        }

        // Step to next row (y increment)
        w0_row += b0_step;
        w1_row += b1_step;
    }
}

/// Rasterize a triangle with indexed texture + CLUT lookup
///
/// This is the PS1-authentic rendering path:
/// 1. Sample palette INDEX from indexed texture
/// 2. Look up actual COLOR in CLUT
/// 3. Continue with standard PS1 pipeline (modulation, shading, dithering)
fn rasterize_triangle_indexed(
    fb: &mut Framebuffer,
    surface: &Surface,
    indexed_texture: Option<&IndexedTexture>,
    clut: Option<&Clut>,
    face_blend_mode: BlendMode,
    black_transparent: bool,
    settings: &RasterSettings,
) {
    // Bounding box
    let min_x = surface.v1.x.min(surface.v2.x).min(surface.v3.x).max(0.0) as usize;
    let max_x = (surface.v1.x.max(surface.v2.x).max(surface.v3.x) + 1.0).min(fb.width as f32) as usize;
    let min_y = surface.v1.y.min(surface.v2.y).min(surface.v3.y).max(0.0) as usize;
    let max_y = (surface.v1.y.max(surface.v2.y).max(surface.v3.y) + 1.0).min(fb.height as f32) as usize;

    // Early exit for degenerate/off-screen triangles
    if min_x >= max_x || min_y >= max_y {
        return;
    }

    // Pre-calculate flat shading if needed
    let flat_shade = if settings.shading == ShadingMode::Flat {
        let center_pos = (surface.w1 + surface.w2 + surface.w3).scale(1.0 / 3.0);
        let world_normal = (surface.wn1 + surface.wn2 + surface.wn3).scale(1.0 / 3.0).normalize();
        shade_multi_light_color(world_normal, center_pos, &settings.lights, settings.ambient)
    } else {
        (1.0, 1.0, 1.0)
    };

    // Pre-compute Gouraud vertex shading if needed
    let gouraud_shades = if settings.shading == ShadingMode::Gouraud {
        Some((
            shade_multi_light_color(surface.wn1, surface.w1, &settings.lights, settings.ambient),
            shade_multi_light_color(surface.wn2, surface.w2, &settings.lights, settings.ambient),
            shade_multi_light_color(surface.wn3, surface.w3, &settings.lights, settings.ambient),
        ))
    } else {
        None
    };

    // === EDGE FUNCTION SETUP ===
    let v1 = surface.v1;
    let v2 = surface.v2;
    let v3 = surface.v3;

    // Triangle area * 2 (used for normalization)
    let area = (v2.y - v3.y) * (v1.x - v3.x) + (v3.x - v2.x) * (v1.y - v3.y);
    if area.abs() < 0.00001 {
        return; // Degenerate triangle
    }
    let inv_area = 1.0 / area;

    // Edge function coefficients
    let a0 = v2.y - v3.y;
    let b0 = v3.x - v2.x;
    let a1 = v3.y - v1.y;
    let b1 = v1.x - v3.x;

    // Starting point
    let start_x = min_x as f32;
    let start_y = min_y as f32;

    // Initial edge function values at (start_x, start_y)
    let w0_row_start = a0 * (start_x - v3.x) + b0 * (start_y - v3.y);
    let w1_row_start = a1 * (start_x - v3.x) + b1 * (start_y - v3.y);

    // Step increments
    let a0_step = a0;
    let b0_step = b0;
    let a1_step = a1;
    let b1_step = b1;

    let mut w0_row = w0_row_start;
    let mut w1_row = w1_row_start;

    // Rasterize using incremental edge functions
    for y in min_y..max_y {
        let mut w0 = w0_row;
        let mut w1 = w1_row;

        for x in min_x..max_x {
            // Convert to barycentric coordinates
            let bc_x = w0 * inv_area;
            let bc_y = w1 * inv_area;
            let bc_z = 1.0 - bc_x - bc_y;

            // Check if inside triangle
            const ERR: f32 = -0.0001;
            if bc_x >= ERR && bc_y >= ERR && bc_z >= ERR {
                // Interpolate depth
                let z = bc_x * v1.z + bc_y * v2.z + bc_z * v3.z;

                // Z-buffer test
                if settings.use_zbuffer {
                    let idx = y * fb.width + x;
                    if z >= fb.zbuffer[idx] {
                        w0 += a0_step;
                        w1 += a1_step;
                        continue;
                    }
                }

                // Interpolate UV coordinates
                let (u, v) = if settings.affine_textures {
                    // Affine (PS1 style) - linear interpolation
                    let u = bc_x * surface.uv1.x + bc_y * surface.uv2.x + bc_z * surface.uv3.x;
                    let v = bc_x * surface.uv1.y + bc_y * surface.uv2.y + bc_z * surface.uv3.y;
                    (u, v)
                } else {
                    // Perspective-correct interpolation
                    let bcc_x = bc_x / v1.z;
                    let bcc_y = bc_y / v2.z;
                    let bcc_z = bc_z / v3.z;
                    let bd = bcc_x + bcc_y + bcc_z;
                    let bcc_x = bcc_x / bd;
                    let bcc_y = bcc_y / bd;
                    let bcc_z = bcc_z / bd;

                    let u = bcc_x * surface.uv1.x + bcc_y * surface.uv2.x + bcc_z * surface.uv3.x;
                    let v = bcc_x * surface.uv1.y + bcc_y * surface.uv2.y + bcc_z * surface.uv3.y;
                    (u, v)
                };

                // === CLUT LOOKUP: Sample index from texture, then look up in CLUT ===
                let mut color = match (indexed_texture, clut) {
                    (Some(tex), Some(clut)) => {
                        let index = tex.sample_index(u, 1.0 - v);
                        clut.lookup(index)
                    }
                    (Some(tex), None) => {
                        // No CLUT: use index as grayscale (for debugging)
                        let index = tex.sample_index(u, 1.0 - v);
                        let v = (index as u16 * 31 / 255) as u8;
                        Color15::new(v, v, v)
                    }
                    _ => Color15::WHITE,
                };

                // Handle transparency based on black_transparent setting
                let is_black = color.r5() == 0 && color.g5() == 0 && color.b5() == 0;
                if color.is_transparent() {
                    if is_black && !black_transparent {
                        color = Color15::BLACK_DRAWABLE;
                    } else {
                        w0 += a0_step;
                        w1 += a1_step;
                        continue;
                    }
                } else if black_transparent && is_black {
                    w0 += a0_step;
                    w1 += a1_step;
                    continue;
                }

                // === PS1-AUTHENTIC COLOR PIPELINE ===
                // Expand texture from 5-bit to 8-bit
                let tex_r8 = expand_5_to_8(color.r5());
                let tex_g8 = expand_5_to_8(color.g5());
                let tex_b8 = expand_5_to_8(color.b5());

                // Interpolate vertex colors
                let vertex_r = (bc_x * surface.vc1.r as f32 + bc_y * surface.vc2.r as f32 + bc_z * surface.vc3.r as f32) as u8;
                let vertex_g = (bc_x * surface.vc1.g as f32 + bc_y * surface.vc2.g as f32 + bc_z * surface.vc3.g as f32) as u8;
                let vertex_b = (bc_x * surface.vc1.b as f32 + bc_y * surface.vc2.b as f32 + bc_z * surface.vc3.b as f32) as u8;

                // Apply PS1-style texture modulation
                let mod_r8 = ((tex_r8 as u32 * vertex_r as u32) / 128).min(255) as u8;
                let mod_g8 = ((tex_g8 as u32 * vertex_g as u32) / 128).min(255) as u8;
                let mod_b8 = ((tex_b8 as u32 * vertex_b as u32) / 128).min(255) as u8;

                // Apply shading
                let (shade_r, shade_g, shade_b) = match settings.shading {
                    ShadingMode::None => (1.0, 1.0, 1.0),
                    ShadingMode::Flat => flat_shade,
                    ShadingMode::Gouraud => {
                        let ((r1, g1, b1), (r2, g2, b2), (r3, g3, b3)) = gouraud_shades.unwrap();
                        (
                            bc_x * r1 + bc_y * r2 + bc_z * r3,
                            bc_x * g1 + bc_y * g2 + bc_z * g3,
                            bc_x * b1 + bc_y * b2 + bc_z * b3,
                        )
                    }
                };

                let shaded_r8 = (mod_r8 as f32 * shade_r.clamp(0.0, 2.0)).min(255.0) as u8;
                let shaded_g8 = (mod_g8 as f32 * shade_g.clamp(0.0, 2.0)).min(255.0) as u8;
                let shaded_b8 = (mod_b8 as f32 * shade_b.clamp(0.0, 2.0)).min(255.0) as u8;

                // Final quantization with dithering
                let (r5, g5, b5) = if settings.dithering {
                    dither_and_quantize(shaded_r8, shaded_g8, shaded_b8, x, y)
                } else {
                    (shaded_r8 >> 3, shaded_g8 >> 3, shaded_b8 >> 3)
                };

                // Create final color, preserving semi-transparency
                let is_all_black = r5 == 0 && g5 == 0 && b5 == 0;
                let semi = color.is_semi_transparent() || is_all_black;
                let color = Color15::new_semi(r5, g5, b5, semi);

                // Write pixel
                let idx = y * fb.width + x;
                if z < fb.zbuffer[idx] {
                    fb.zbuffer[idx] = z;
                    if color.is_semi_transparent() && face_blend_mode != BlendMode::Opaque {
                        fb.set_pixel_blended_15(x, y, color, face_blend_mode);
                    } else {
                        fb.set_pixel_15(x, y, color);
                    }
                }
            }

            w0 += a0_step;
            w1 += a1_step;
        }

        w0_row += b0_step;
        w1_row += b1_step;
    }
}

/// Render a mesh to the framebuffer
/// Returns timing breakdown for profiling
pub fn render_mesh(
    fb: &mut Framebuffer,
    vertices: &[Vertex],
    faces: &[Face],
    textures: &[Texture],
    camera: &Camera,
    settings: &RasterSettings,
) -> RasterTimings {
    let mut timings = RasterTimings::default();

    // === TRANSFORM PHASE ===
    let transform_start = get_time();

    // Transform all vertices to camera space
    let mut cam_space_positions: Vec<Vec3> = Vec::with_capacity(vertices.len());
    let mut cam_space_normals: Vec<Vec3> = Vec::with_capacity(vertices.len());
    let mut projected: Vec<Vec3> = Vec::with_capacity(vertices.len());

    for v in vertices {
        // Project to screen - use ortho, fixed-point, or float projection
        let (screen_pos, cam_pos) = if let Some(ref ortho) = settings.ortho_projection {
            // Ortho: use float path
            let rel_pos = v.pos - camera.position;
            let cam_pos = perspective_transform(rel_pos, camera.basis_x, camera.basis_y, camera.basis_z);
            let screen = project_ortho(cam_pos, ortho.zoom, ortho.center_x, ortho.center_y, fb.width, fb.height);
            (screen, cam_pos)
        } else if settings.use_fixed_point {
            // PS1-style: entire transform+project pipeline in fixed-point (1.3.12 format + UNR division)
            let (sx, sy, depth) = super::fixed::project_fixed(
                v.pos,
                camera.position,
                camera.basis_x,
                camera.basis_y,
                camera.basis_z,
                fb.width,
                fb.height,
            );
            // Still need cam_pos for culling/shading (use float for this)
            let rel_pos = v.pos - camera.position;
            let cam_pos = perspective_transform(rel_pos, camera.basis_x, camera.basis_y, camera.basis_z);
            (Vec3::new(sx as f32, sy as f32, depth), cam_pos)
        } else {
            // Standard float path
            let rel_pos = v.pos - camera.position;
            let cam_pos = perspective_transform(rel_pos, camera.basis_x, camera.basis_y, camera.basis_z);
            let screen = project(cam_pos, fb.width, fb.height);
            (screen, cam_pos)
        };

        cam_space_positions.push(cam_pos);
        projected.push(screen_pos);

        // Transform normal to camera space
        let cam_normal = perspective_transform(v.normal, camera.basis_x, camera.basis_y, camera.basis_z);
        cam_space_normals.push(cam_normal.normalize());
    }

    timings.transform_ms = ((get_time() - transform_start) * 1000.0) as f32;

    // === CULL PHASE ===
    let cull_start = get_time();

    // Build surfaces for front-faces and collect back-faces for wireframe
    let mut surfaces: Vec<Surface> = Vec::with_capacity(faces.len());
    let mut backface_wireframes: Vec<(Vec3, Vec3, Vec3)> = Vec::new();
    let mut frontface_wireframes: Vec<(Vec3, Vec3, Vec3)> = Vec::new();

    for (face_idx, face) in faces.iter().enumerate() {
        // Get camera-space positions
        let cv1 = cam_space_positions[face.v0];
        let cv2 = cam_space_positions[face.v1];
        let cv3 = cam_space_positions[face.v2];

        // PS1-style: Skip triangles that have ANY vertex behind the near plane
        // This is conservative but simple and matches PS1 behavior
        // (Games were designed to not let geometry get too close to camera)
        // NOTE: Skip near-plane culling for orthographic projection (camera Z is meaningless)
        if settings.ortho_projection.is_none() {
            if cv1.z <= NEAR_PLANE || cv2.z <= NEAR_PLANE || cv3.z <= NEAR_PLANE {
                continue;
            }
        }

        // Use pre-projected screen positions
        let v1 = projected[face.v0];
        let v2 = projected[face.v1];
        let v3 = projected[face.v2];

        // 2D screen-space backface culling (PS1-style)
        let signed_area = (v2.x - v1.x) * (v3.y - v1.y) - (v3.x - v1.x) * (v2.y - v1.y);
        let is_backface = signed_area <= 0.0;

        // Compute geometric normal for shading (cross product in camera space)
        let edge1 = cv2 - cv1;
        let edge2 = cv3 - cv1;
        let normal = edge1.cross(edge2).normalize();

        // Determine if this face uses semi-transparency (8-bit path: check texture's blend_mode)
        let has_transparency = face.texture_id
            .and_then(|id| textures.get(id))
            .map(|t| t.blend_mode != BlendMode::Opaque)
            .unwrap_or(false);

        if is_backface {
            // Back-face: collect for wireframe rendering
            backface_wireframes.push((v1, v2, v3));

            // If backface culling is disabled, also render as solid
            if !settings.backface_cull {
                surfaces.push(Surface {
                    v1,
                    v2,
                    v3,
                    w1: vertices[face.v0].pos,
                    w2: vertices[face.v1].pos,
                    w3: vertices[face.v2].pos,
                    vn1: cam_space_normals[face.v0].scale(-1.0),
                    vn2: cam_space_normals[face.v1].scale(-1.0),
                    vn3: cam_space_normals[face.v2].scale(-1.0),
                    wn1: vertices[face.v0].normal.scale(-1.0),
                    wn2: vertices[face.v1].normal.scale(-1.0),
                    wn3: vertices[face.v2].normal.scale(-1.0),
                    uv1: vertices[face.v0].uv,
                    uv2: vertices[face.v1].uv,
                    uv3: vertices[face.v2].uv,
                    vc1: vertices[face.v0].color,
                    vc2: vertices[face.v1].color,
                    vc3: vertices[face.v2].color,
                    normal: normal.scale(-1.0),
                    face_idx,
                    black_transparent: face.black_transparent,
                    has_transparency,
                });
            }
        } else {
            // Front-face: always render as solid
            surfaces.push(Surface {
                v1,
                v2,
                v3,
                w1: vertices[face.v0].pos,
                w2: vertices[face.v1].pos,
                w3: vertices[face.v2].pos,
                vn1: cam_space_normals[face.v0],
                vn2: cam_space_normals[face.v1],
                vn3: cam_space_normals[face.v2],
                wn1: vertices[face.v0].normal,
                wn2: vertices[face.v1].normal,
                wn3: vertices[face.v2].normal,
                uv1: vertices[face.v0].uv,
                uv2: vertices[face.v1].uv,
                uv3: vertices[face.v2].uv,
                vc1: vertices[face.v0].color,
                vc2: vertices[face.v1].color,
                vc3: vertices[face.v2].color,
                normal,
                face_idx,
                black_transparent: face.black_transparent,
                has_transparency,
            });

            // Collect for wireframe overlay
            if settings.wireframe_overlay {
                frontface_wireframes.push((v1, v2, v3));
            }
        }
    }

    timings.cull_ms = ((get_time() - cull_start) * 1000.0) as f32;

    // === SORT PHASE ===
    let sort_start = get_time();

    // Sort by depth if not using Z-buffer (painter's algorithm)
    if !settings.use_zbuffer {
        surfaces.sort_by(|a, b| {
            let a_max_z = a.v1.z.max(a.v2.z).max(a.v3.z);
            let b_max_z = b.v1.z.max(b.v2.z).max(b.v3.z);
            b_max_z.partial_cmp(&a_max_z).unwrap()
        });
    }

    timings.sort_ms = ((get_time() - sort_start) * 1000.0) as f32;

    // === DRAW PHASE ===
    let draw_start = get_time();

    // Rasterize each solid surface (skip if wireframe-only mode)
    if !settings.wireframe_overlay {
        for surface in &surfaces {
            let texture = faces[surface.face_idx]
                .texture_id
                .and_then(|id| textures.get(id));

            rasterize_triangle(fb, surface, texture, settings);
        }
    }

    timings.draw_ms = ((get_time() - draw_start) * 1000.0) as f32;

    // === WIREFRAME PHASE ===
    let wireframe_start = get_time();

    // Draw wireframes for back-faces (visible but not solid)
    // Only draw if backface culling is enabled AND backface wireframe is enabled
    if settings.backface_cull && settings.backface_wireframe {
        // Deduplicate edges to avoid drawing shared edges twice (which causes double-line artifacts)
        // Include z values for depth testing
        let mut unique_edges: Vec<(i32, i32, f32, i32, i32, f32)> = Vec::new();

        for (v1, v2, v3) in &backface_wireframes {
            let edges = [
                (v1.x as i32, v1.y as i32, v1.z, v2.x as i32, v2.y as i32, v2.z),
                (v2.x as i32, v2.y as i32, v2.z, v3.x as i32, v3.y as i32, v3.z),
                (v3.x as i32, v3.y as i32, v3.z, v1.x as i32, v1.y as i32, v1.z),
            ];

            for (x0, y0, z0, x1, y1, z1) in edges {
                // Normalize edge direction so (a,b)-(c,d) and (c,d)-(a,b) are the same
                let edge = if (x0, y0) < (x1, y1) {
                    (x0, y0, z0, x1, y1, z1)
                } else {
                    (x1, y1, z1, x0, y0, z0)
                };

                // Only add if not already present (compare just screen coords for dedup)
                if !unique_edges.iter().any(|e| e.0 == edge.0 && e.1 == edge.1 && e.3 == edge.3 && e.4 == edge.4) {
                    unique_edges.push(edge);
                }
            }
        }

        // Draw each unique edge once with depth testing
        let wireframe_color = Color::new(80, 80, 100);
        for (x0, y0, z0, x1, y1, z1) in unique_edges {
            fb.draw_line_3d(x0, y0, z0, x1, y1, z1, wireframe_color);
        }
    }

    // Draw wireframes for front-faces (overlay on top of solid geometry)
    if settings.wireframe_overlay && !frontface_wireframes.is_empty() {
        // Deduplicate edges
        let mut unique_edges: Vec<(i32, i32, f32, i32, i32, f32)> = Vec::new();

        for (v1, v2, v3) in &frontface_wireframes {
            let edges = [
                (v1.x as i32, v1.y as i32, v1.z, v2.x as i32, v2.y as i32, v2.z),
                (v2.x as i32, v2.y as i32, v2.z, v3.x as i32, v3.y as i32, v3.z),
                (v3.x as i32, v3.y as i32, v3.z, v1.x as i32, v1.y as i32, v1.z),
            ];

            for (x0, y0, z0, x1, y1, z1) in edges {
                // Normalize edge direction for deduplication
                let edge = if (x0, y0) < (x1, y1) {
                    (x0, y0, z0, x1, y1, z1)
                } else {
                    (x1, y1, z1, x0, y0, z0)
                };

                if !unique_edges.iter().any(|e| e.0 == edge.0 && e.1 == edge.1 && e.3 == edge.3 && e.4 == edge.4) {
                    unique_edges.push(edge);
                }
            }
        }

        // Draw front-face wireframe with a brighter color (on top, no depth test needed since it's on visible faces)
        let front_wireframe_color = Color::new(200, 200, 220);
        for (x0, y0, _z0, x1, y1, _z1) in unique_edges {
            // Draw without depth testing since these are on front faces (already visible)
            fb.draw_line(x0, y0, x1, y1, front_wireframe_color);
        }
    }

    timings.wireframe_ms = ((get_time() - wireframe_start) * 1000.0) as f32;

    timings
}

// === FOG HELPER FUNCTIONS (PS1-style depth cueing) ===

/// Calculate fog factor for a vertex depth (PS1-style depth cueing)
/// Returns 0.0 (no fog) to 1.0 (full fog)
#[inline]
fn calculate_fog_factor(z: f32, fog_start: f32, fog_falloff: f32) -> f32 {
    if z <= fog_start {
        0.0
    } else if fog_falloff <= 0.0 {
        1.0
    } else {
        ((z - fog_start) / fog_falloff).min(1.0)
    }
}

/// Apply fog to a vertex color (linear interpolation toward fog color)
/// Works with 8-bit Color (Surface vertex colors)
#[inline]
fn apply_fog_to_color(color: Color, fog_color: Color, fog_factor: f32) -> Color {
    if fog_factor <= 0.0 {
        return color;
    }
    if fog_factor >= 1.0 {
        return fog_color;
    }

    let inv_factor = 1.0 - fog_factor;
    let r = (color.r as f32 * inv_factor + fog_color.r as f32 * fog_factor) as u8;
    let g = (color.g as f32 * inv_factor + fog_color.g as f32 * fog_factor) as u8;
    let b = (color.b as f32 * inv_factor + fog_color.b as f32 * fog_factor) as u8;

    Color::new(r, g, b)
}

/// Render a mesh using RGB555 textures (PS1-authentic mode)
/// Uses Texture15 for texture sampling with proper semi-transparency handling
///
/// The `fog` parameter enables PS1-style depth cueing: (start_z, end_z, fog_color)
/// - Vertices closer than start_z have no fog
/// - Vertices further than end_z are fully fogged (draw distance)
/// - Between start and end, fog is linearly interpolated
pub fn render_mesh_15(
    fb: &mut Framebuffer,
    vertices: &[Vertex],
    faces: &[Face],
    textures: &[Texture15],
    face_blend_modes: Option<&[BlendMode]>,
    camera: &Camera,
    settings: &RasterSettings,
    fog: Option<(f32, f32, Color)>,
) -> RasterTimings {
    let mut timings = RasterTimings::default();

    // === TRANSFORM PHASE ===
    let transform_start = get_time();

    // Transform all vertices to camera space
    let mut cam_space_positions: Vec<Vec3> = Vec::with_capacity(vertices.len());
    let mut cam_space_normals: Vec<Vec3> = Vec::with_capacity(vertices.len());
    let mut projected: Vec<Vec3> = Vec::with_capacity(vertices.len());

    for v in vertices {
        // Project to screen - use ortho, fixed-point, or float projection
        let (screen_pos, cam_pos) = if let Some(ref ortho) = settings.ortho_projection {
            // Ortho: use float path
            let rel_pos = v.pos - camera.position;
            let cam_pos = perspective_transform(rel_pos, camera.basis_x, camera.basis_y, camera.basis_z);
            let screen = project_ortho(cam_pos, ortho.zoom, ortho.center_x, ortho.center_y, fb.width, fb.height);
            (screen, cam_pos)
        } else if settings.use_fixed_point {
            // PS1-style: entire transform+project pipeline in fixed-point (1.3.12 format + UNR division)
            let (sx, sy, depth) = super::fixed::project_fixed(
                v.pos,
                camera.position,
                camera.basis_x,
                camera.basis_y,
                camera.basis_z,
                fb.width,
                fb.height,
            );
            // Still need cam_pos for culling/shading (use float for this)
            let rel_pos = v.pos - camera.position;
            let cam_pos = perspective_transform(rel_pos, camera.basis_x, camera.basis_y, camera.basis_z);
            (Vec3::new(sx as f32, sy as f32, depth), cam_pos)
        } else {
            // Standard float path
            let rel_pos = v.pos - camera.position;
            let cam_pos = perspective_transform(rel_pos, camera.basis_x, camera.basis_y, camera.basis_z);
            let screen = project(cam_pos, fb.width, fb.height);
            (screen, cam_pos)
        };

        cam_space_positions.push(cam_pos);
        projected.push(screen_pos);

        // Transform normal to camera space
        let cam_normal = perspective_transform(v.normal, camera.basis_x, camera.basis_y, camera.basis_z);
        cam_space_normals.push(cam_normal.normalize());
    }

    timings.transform_ms = ((get_time() - transform_start) * 1000.0) as f32;

    // === CULL PHASE ===
    let cull_start = get_time();
    let mut fog_total_time = 0.0f64;

    // Build surfaces for front-faces and collect back-faces for wireframe
    let mut surfaces: Vec<Surface> = Vec::with_capacity(faces.len());
    let mut backface_wireframes: Vec<(Vec3, Vec3, Vec3)> = Vec::new();
    let mut frontface_wireframes: Vec<(Vec3, Vec3, Vec3)> = Vec::new();

    for (face_idx, face) in faces.iter().enumerate() {
        // Get camera-space positions
        let cv1 = cam_space_positions[face.v0];
        let cv2 = cam_space_positions[face.v1];
        let cv3 = cam_space_positions[face.v2];

        // PS1-style: Skip triangles that have ANY vertex behind the near plane
        // NOTE: Skip near-plane culling for orthographic projection (camera Z is meaningless)
        if settings.ortho_projection.is_none() {
            if cv1.z <= NEAR_PLANE || cv2.z <= NEAR_PLANE || cv3.z <= NEAR_PLANE {
                continue;
            }
        }

        // Use pre-projected screen positions
        let v1 = projected[face.v0];
        let v2 = projected[face.v1];
        let v3 = projected[face.v2];

        // 2D screen-space backface culling (PS1-style)
        let signed_area = (v2.x - v1.x) * (v3.y - v1.y) - (v3.x - v1.x) * (v2.y - v1.y);
        let is_backface = signed_area <= 0.0;

        // Compute geometric normal for shading (cross product in camera space)
        let edge1 = cv2 - cv1;
        let edge2 = cv3 - cv1;
        let normal = edge1.cross(edge2).normalize();

        // Determine if this face uses semi-transparency (for two-pass rendering)
        // Check texture's blend_mode first, then face blend mode
        let has_transparency = {
            let tex_blend = face.texture_id
                .and_then(|id| textures.get(id))
                .map(|t| t.blend_mode);
            let face_blend = face_blend_modes
                .and_then(|modes| modes.get(face_idx))
                .copied();

            // Face is transparent if texture or face has non-Opaque blend mode
            match (tex_blend, face_blend) {
                (Some(b), _) if b != BlendMode::Opaque => true,
                (None, Some(b)) if b != BlendMode::Opaque => true,
                _ => false,
            }
        };

        // Apply PS1-style fog to vertex colors (depth cueing)
        let fog_start_time = get_time();
        let (vc1, vc2, vc3) = if let Some((fog_start, fog_falloff, fog_color)) = fog {
            // Calculate fog factor for each vertex based on camera-space Z
            let f1 = calculate_fog_factor(cv1.z, fog_start, fog_falloff);
            let f2 = calculate_fog_factor(cv2.z, fog_start, fog_falloff);
            let f3 = calculate_fog_factor(cv3.z, fog_start, fog_falloff);

            (
                apply_fog_to_color(vertices[face.v0].color, fog_color, f1),
                apply_fog_to_color(vertices[face.v1].color, fog_color, f2),
                apply_fog_to_color(vertices[face.v2].color, fog_color, f3),
            )
        } else {
            (
                vertices[face.v0].color,
                vertices[face.v1].color,
                vertices[face.v2].color,
            )
        };
        fog_total_time += get_time() - fog_start_time;

        if is_backface {
            backface_wireframes.push((v1, v2, v3));

            if !settings.backface_cull {
                surfaces.push(Surface {
                    v1,
                    v2,
                    v3,
                    w1: vertices[face.v0].pos,
                    w2: vertices[face.v1].pos,
                    w3: vertices[face.v2].pos,
                    vn1: cam_space_normals[face.v0].scale(-1.0),
                    vn2: cam_space_normals[face.v1].scale(-1.0),
                    vn3: cam_space_normals[face.v2].scale(-1.0),
                    wn1: vertices[face.v0].normal.scale(-1.0),
                    wn2: vertices[face.v1].normal.scale(-1.0),
                    wn3: vertices[face.v2].normal.scale(-1.0),
                    uv1: vertices[face.v0].uv,
                    uv2: vertices[face.v1].uv,
                    uv3: vertices[face.v2].uv,
                    vc1,
                    vc2,
                    vc3,
                    normal: normal.scale(-1.0),
                    face_idx,
                    black_transparent: face.black_transparent,
                    has_transparency,
                });
            }
        } else {
            surfaces.push(Surface {
                v1,
                v2,
                v3,
                w1: vertices[face.v0].pos,
                w2: vertices[face.v1].pos,
                w3: vertices[face.v2].pos,
                vn1: cam_space_normals[face.v0],
                vn2: cam_space_normals[face.v1],
                vn3: cam_space_normals[face.v2],
                wn1: vertices[face.v0].normal,
                wn2: vertices[face.v1].normal,
                wn3: vertices[face.v2].normal,
                uv1: vertices[face.v0].uv,
                uv2: vertices[face.v1].uv,
                uv3: vertices[face.v2].uv,
                vc1,
                vc2,
                vc3,
                normal,
                face_idx,
                black_transparent: face.black_transparent,
                has_transparency,
            });

            if settings.wireframe_overlay {
                frontface_wireframes.push((v1, v2, v3));
            }
        }
    }

    timings.cull_ms = ((get_time() - cull_start) * 1000.0) as f32;
    timings.fog_ms = (fog_total_time * 1000.0) as f32;

    // === SORT PHASE (Two-Pass: Separate Opaque & Transparent) ===
    let sort_start = get_time();

    // Partition surfaces into opaque and semi-transparent
    let (mut opaque_surfaces, mut transparent_surfaces): (Vec<_>, Vec<_>) =
        surfaces.into_iter().partition(|s| !s.has_transparency);

    // Sort transparent surfaces back-to-front (always, regardless of z-buffer mode)
    // This is required for correct blending order (PS1 Ordering Table style)
    transparent_surfaces.sort_by(|a, b| {
        let a_max_z = a.v1.z.max(a.v2.z).max(a.v3.z);
        let b_max_z = b.v1.z.max(b.v2.z).max(b.v3.z);
        b_max_z.partial_cmp(&a_max_z).unwrap()  // Back-to-front (far first)
    });

    // Sort opaque surfaces only if using painter's algorithm (no z-buffer)
    if !settings.use_zbuffer {
        opaque_surfaces.sort_by(|a, b| {
            let a_max_z = a.v1.z.max(a.v2.z).max(a.v3.z);
            let b_max_z = b.v1.z.max(b.v2.z).max(b.v3.z);
            b_max_z.partial_cmp(&a_max_z).unwrap()  // Back-to-front
        });
    }

    timings.sort_ms = ((get_time() - sort_start) * 1000.0) as f32;

    // === DRAW PHASE (Two-Pass Rendering) ===
    let draw_start = get_time();

    if !settings.wireframe_overlay {
        // PASS 1: Render opaque surfaces (z-buffer writes enabled)
        // Establishes depth buffer for correct occlusion
        for surface in &opaque_surfaces {
            let texture = faces[surface.face_idx]
                .texture_id
                .and_then(|id| textures.get(id));

            let blend_mode = face_blend_modes
                .and_then(|modes| modes.get(surface.face_idx))
                .copied()
                .unwrap_or(BlendMode::Opaque);

            rasterize_triangle_15(fb, surface, texture, blend_mode, surface.black_transparent, settings, false);
        }

        // PASS 2: Render semi-transparent surfaces (z-buffer writes DISABLED)
        // Sorted back-to-front for correct blending, depth-tested but doesn't occlude
        for surface in &transparent_surfaces {
            let texture = faces[surface.face_idx]
                .texture_id
                .and_then(|id| textures.get(id));

            let blend_mode = face_blend_modes
                .and_then(|modes| modes.get(surface.face_idx))
                .copied()
                .unwrap_or(BlendMode::Opaque);

            rasterize_triangle_15(fb, surface, texture, blend_mode, surface.black_transparent, settings, true);
        }
    }

    timings.draw_ms = ((get_time() - draw_start) * 1000.0) as f32;

    // === WIREFRAME PHASE ===
    let wireframe_start = get_time();

    if settings.backface_cull && settings.backface_wireframe {
        let mut unique_edges: Vec<(i32, i32, f32, i32, i32, f32)> = Vec::new();

        for (v1, v2, v3) in &backface_wireframes {
            let edges = [
                (v1.x as i32, v1.y as i32, v1.z, v2.x as i32, v2.y as i32, v2.z),
                (v2.x as i32, v2.y as i32, v2.z, v3.x as i32, v3.y as i32, v3.z),
                (v3.x as i32, v3.y as i32, v3.z, v1.x as i32, v1.y as i32, v1.z),
            ];

            for (x0, y0, z0, x1, y1, z1) in edges {
                let edge = if (x0, y0) < (x1, y1) {
                    (x0, y0, z0, x1, y1, z1)
                } else {
                    (x1, y1, z1, x0, y0, z0)
                };

                if !unique_edges.iter().any(|e| e.0 == edge.0 && e.1 == edge.1 && e.3 == edge.3 && e.4 == edge.4) {
                    unique_edges.push(edge);
                }
            }
        }

        let wireframe_color = Color::new(80, 80, 100);
        for (x0, y0, z0, x1, y1, z1) in unique_edges {
            fb.draw_line_3d(x0, y0, z0, x1, y1, z1, wireframe_color);
        }
    }

    if settings.wireframe_overlay && !frontface_wireframes.is_empty() {
        let mut unique_edges: Vec<(i32, i32, f32, i32, i32, f32)> = Vec::new();

        for (v1, v2, v3) in &frontface_wireframes {
            let edges = [
                (v1.x as i32, v1.y as i32, v1.z, v2.x as i32, v2.y as i32, v2.z),
                (v2.x as i32, v2.y as i32, v2.z, v3.x as i32, v3.y as i32, v3.z),
                (v3.x as i32, v3.y as i32, v3.z, v1.x as i32, v1.y as i32, v1.z),
            ];

            for (x0, y0, z0, x1, y1, z1) in edges {
                let edge = if (x0, y0) < (x1, y1) {
                    (x0, y0, z0, x1, y1, z1)
                } else {
                    (x1, y1, z1, x0, y0, z0)
                };

                if !unique_edges.iter().any(|e| e.0 == edge.0 && e.1 == edge.1 && e.3 == edge.3 && e.4 == edge.4) {
                    unique_edges.push(edge);
                }
            }
        }

        let front_wireframe_color = Color::new(200, 200, 220);
        for (x0, y0, _z0, x1, y1, _z1) in unique_edges {
            fb.draw_line(x0, y0, x1, y1, front_wireframe_color);
        }
    }

    timings.wireframe_ms = ((get_time() - wireframe_start) * 1000.0) as f32;

    timings
}

/// Create a simple test cube mesh
pub fn create_test_cube() -> (Vec<Vertex>, Vec<Face>) {
    use super::math::Vec2;

    let mut vertices = Vec::new();
    let mut faces = Vec::new();

    // Cube vertices with positions, UVs, and normals
    let positions = [
        // Front face
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        // Back face
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        // Top face
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(1.0, 1.0, -1.0),
        // Bottom face
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        // Right face
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        // Left face
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, -1.0),
    ];

    let normals = [
        Vec3::new(0.0, 0.0, 1.0),  // Front
        Vec3::new(0.0, 0.0, -1.0), // Back
        Vec3::new(0.0, 1.0, 0.0),  // Top
        Vec3::new(0.0, -1.0, 0.0), // Bottom
        Vec3::new(1.0, 0.0, 0.0),  // Right
        Vec3::new(-1.0, 0.0, 0.0), // Left
    ];

    let uvs = [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ];

    // Build vertices for each face
    for face_idx in 0..6 {
        let base = face_idx * 4;
        let normal = normals[face_idx];

        for i in 0..4 {
            vertices.push(Vertex {
                pos: positions[base + i],
                uv: uvs[i],
                normal,
                color: Color::NEUTRAL,
                bone_index: None,
            });
        }

        // Two triangles per face
        let vbase = face_idx * 4;
        faces.push(Face::with_texture(vbase, vbase + 1, vbase + 2, 0));
        faces.push(Face::with_texture(vbase, vbase + 2, vbase + 3, 0));
    }

    (vertices, faces)
}

// =============================================================================
// Shared 3D line drawing with near-plane clipping
// =============================================================================

/// Draw a 3D line with proper near-plane clipping.
/// Used by both world editor and modeler for grid/wireframe rendering.
pub fn draw_3d_line_clipped(
    fb: &mut Framebuffer,
    camera: &Camera,
    p0: Vec3,
    p1: Vec3,
    color: Color,
) {
    use super::math::{world_to_screen, NEAR_PLANE};

    // Transform to camera space
    let rel0 = p0 - camera.position;
    let rel1 = p1 - camera.position;

    let z0 = rel0.dot(camera.basis_z);
    let z1 = rel1.dot(camera.basis_z);

    // Both behind camera - skip entirely
    if z0 <= NEAR_PLANE && z1 <= NEAR_PLANE {
        return;
    }

    // Clip line to near plane if needed
    let (clipped_p0, clipped_p1) = if z0 <= NEAR_PLANE {
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p0 = p0 + (p1 - p0) * t;
        (new_p0, p1)
    } else if z1 <= NEAR_PLANE {
        let t = (NEAR_PLANE - z0) / (z1 - z0);
        let new_p1 = p0 + (p1 - p0) * t;
        (p0, new_p1)
    } else {
        (p0, p1)
    };

    // Project clipped endpoints to screen space
    let s0 = world_to_screen(
        clipped_p0,
        camera.position,
        camera.basis_x,
        camera.basis_y,
        camera.basis_z,
        fb.width,
        fb.height,
    );
    let s1 = world_to_screen(
        clipped_p1,
        camera.position,
        camera.basis_x,
        camera.basis_y,
        camera.basis_z,
        fb.width,
        fb.height,
    );

    if let (Some((x0, y0)), Some((x1, y1))) = (s0, s1) {
        fb.draw_line(x0 as i32, y0 as i32, x1 as i32, y1 as i32, color);
    }
}

/// Draw a floor grid on a horizontal plane.
/// Uses short segments for better near-plane clipping behavior.
///
/// # Arguments
/// * `fb` - Framebuffer to draw to
/// * `camera` - Camera for projection
/// * `y` - Y height of the grid plane
/// * `spacing` - Distance between grid lines
/// * `extent` - Half-size of the grid (grid goes from -extent to +extent)
/// * `grid_color` - Color for regular grid lines
/// * `x_axis_color` - Color for the X axis (line at Z=0)
/// * `z_axis_color` - Color for the Z axis (line at X=0)
pub fn draw_floor_grid(
    fb: &mut Framebuffer,
    camera: &Camera,
    y: f32,
    spacing: f32,
    extent: f32,
    grid_color: Color,
    x_axis_color: Color,
    z_axis_color: Color,
) {
    // Use shorter segments for better clipping behavior
    let segment_length = spacing;

    // X-parallel lines (varying X, fixed Z)
    let mut z = -extent;
    while z <= extent {
        let is_z_axis = z.abs() < 0.001;
        let color = if is_z_axis { z_axis_color } else { grid_color };

        let mut x = -extent;
        while x < extent {
            let x_end = (x + segment_length).min(extent);
            draw_3d_line_clipped(
                fb,
                camera,
                Vec3::new(x, y, z),
                Vec3::new(x_end, y, z),
                color,
            );
            x += segment_length;
        }
        z += spacing;
    }

    // Z-parallel lines (fixed X, varying Z)
    let mut x = -extent;
    while x <= extent {
        let is_x_axis = x.abs() < 0.001;
        let color = if is_x_axis { x_axis_color } else { grid_color };

        let mut z = -extent;
        while z < extent {
            let z_end = (z + segment_length).min(extent);
            draw_3d_line_clipped(
                fb,
                camera,
                Vec3::new(x, y, z),
                Vec3::new(x, y, z_end),
                color,
            );
            z += segment_length;
        }
        x += spacing;
    }
}
