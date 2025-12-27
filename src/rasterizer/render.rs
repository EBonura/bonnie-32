//! Core rendering functions
//! Triangle rasterization with PS1-style effects

use std::time::Instant;
use super::math::{perspective_transform, project, project_ortho, Vec3, NEAR_PLANE};
use super::types::{BlendMode, Color, Face, Light, LightType, RasterSettings, RasterTimings, ShadingMode, Texture, Vertex};

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
            basis_x: Vec3::new(1.0, 0.0, 0.0),   // Right
            basis_y: Vec3::new(0.0, -1.0, 0.0),  // Up (Y goes up, but screen Y is inverted)
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
            basis_x: Vec3::new(0.0, 0.0, -1.0),  // Right (Z goes left on screen)
            basis_y: Vec3::new(0.0, -1.0, 0.0),  // Up
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

/// PS1 4x4 ordered dithering matrix (Bayer pattern)
/// Raw values 0-15, same pattern used by PlayStation hardware
const BAYER_4X4: [[i32; 4]; 4] = [
    [ 0,  8,  2, 10],
    [12,  4, 14,  6],
    [ 3, 11,  1,  9],
    [15,  7, 13,  5],
];

/// Apply PS1-style ordered dithering to a color
/// The PS1 used 15-bit color (5 bits per channel = 32 levels)
/// Dithering adds spatial noise to hide color banding in gradients
fn apply_dither(color: Color, x: usize, y: usize) -> Color {
    // Get dither value from matrix based on pixel position (0-15)
    let dither = BAYER_4X4[y & 3][x & 3];

    // PS1 offset formula: (dither / 2.0 - 4.0) gives range -4 to +3.5
    // We use integer math: (dither - 8) / 2 gives range -4 to +3
    let offset = (dither - 8) / 2;

    // Apply offset to each channel and quantize to 5-bit (32 levels)
    // PS1 used 0xF8 mask to truncate to 5 bits (keeps top 5 bits)
    let r = ((color.r as i32 + offset).clamp(0, 255) as u8) & 0xF8;
    let g = ((color.g as i32 + offset).clamp(0, 255) as u8) & 0xF8;
    let b = ((color.b as i32 + offset).clamp(0, 255) as u8) & 0xF8;

    Color::with_blend(r, g, b, color.blend)
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
    let transform_start = Instant::now();

    // Transform all vertices to camera space
    let mut cam_space_positions: Vec<Vec3> = Vec::with_capacity(vertices.len());
    let mut cam_space_normals: Vec<Vec3> = Vec::with_capacity(vertices.len());
    let mut projected: Vec<Vec3> = Vec::with_capacity(vertices.len());

    for v in vertices {
        // Transform position to camera space
        let rel_pos = v.pos - camera.position;
        let cam_pos = perspective_transform(rel_pos, camera.basis_x, camera.basis_y, camera.basis_z);
        cam_space_positions.push(cam_pos);

        // Project to screen - use ortho or perspective based on settings
        let screen_pos = if let Some(ref ortho) = settings.ortho_projection {
            project_ortho(cam_pos, ortho.zoom, ortho.center_x, ortho.center_y, fb.width, fb.height)
        } else {
            project(cam_pos, settings.vertex_snap, fb.width, fb.height)
        };
        projected.push(screen_pos);

        // Transform normal to camera space
        let cam_normal = perspective_transform(v.normal, camera.basis_x, camera.basis_y, camera.basis_z);
        cam_space_normals.push(cam_normal.normalize());
    }

    timings.transform_ms = transform_start.elapsed().as_secs_f32() * 1000.0;

    // === CULL PHASE ===
    let cull_start = Instant::now();

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
        if cv1.z <= NEAR_PLANE || cv2.z <= NEAR_PLANE || cv3.z <= NEAR_PLANE {
            continue;
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
            });

            // Collect for wireframe overlay
            if settings.wireframe_overlay {
                frontface_wireframes.push((v1, v2, v3));
            }
        }
    }

    timings.cull_ms = cull_start.elapsed().as_secs_f32() * 1000.0;

    // === SORT PHASE ===
    let sort_start = Instant::now();

    // Sort by depth if not using Z-buffer (painter's algorithm)
    if !settings.use_zbuffer {
        surfaces.sort_by(|a, b| {
            let a_max_z = a.v1.z.max(a.v2.z).max(a.v3.z);
            let b_max_z = b.v1.z.max(b.v2.z).max(b.v3.z);
            b_max_z.partial_cmp(&a_max_z).unwrap()
        });
    }

    timings.sort_ms = sort_start.elapsed().as_secs_f32() * 1000.0;

    // === DRAW PHASE ===
    let draw_start = Instant::now();

    // Rasterize each solid surface (skip if wireframe-only mode)
    if !settings.wireframe_overlay {
        for surface in &surfaces {
            let texture = faces[surface.face_idx]
                .texture_id
                .and_then(|id| textures.get(id));

            rasterize_triangle(fb, surface, texture, settings);
        }
    }

    timings.draw_ms = draw_start.elapsed().as_secs_f32() * 1000.0;

    // === WIREFRAME PHASE ===
    let wireframe_start = Instant::now();

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

    timings.wireframe_ms = wireframe_start.elapsed().as_secs_f32() * 1000.0;

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
