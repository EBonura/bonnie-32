//! Core geometry types for TR1-style levels
//!
//! Sector-based geometry system inspired by TRLE.
//! Rooms contain a 2D grid of sectors, each with floor, ceiling, and walls.

use serde::{Serialize, Deserialize};
use crate::rasterizer::{Vec3, Vec2, Vertex, Face as RasterFace, BlendMode, Color};

/// TRLE sector size in world units
pub const SECTOR_SIZE: f32 = 1024.0;

/// UV scale factor: how much UV space one sector consumes
/// 0.5 means one sector uses UV range [0, 0.5], so a 64x64 texture covers 2x2 blocks (32 texels per block)
/// 1.0 means one sector uses UV range [0, 1], so a 64x64 texture covers 1 block (64 texels per block)
pub const UV_SCALE: f32 = 0.5;

/// Texture reference by pack and name
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextureRef {
    /// Texture pack name (e.g., "SAMPLE")
    pub pack: String,
    /// Texture name without extension (e.g., "floor_01")
    pub name: String,
}

impl TextureRef {
    pub fn new(pack: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            pack: pack.into(),
            name: name.into(),
        }
    }

    /// Create a None reference (uses fallback checkerboard)
    pub fn none() -> Self {
        Self {
            pack: String::new(),
            name: String::new(),
        }
    }

    /// Check if this is a valid reference
    pub fn is_valid(&self) -> bool {
        !self.pack.is_empty() && !self.name.is_empty()
    }
}

impl Default for TextureRef {
    fn default() -> Self {
        Self::none()
    }
}

fn default_true() -> bool { true }
fn default_neutral_color() -> Color { Color::NEUTRAL }
fn default_neutral_colors_4() -> [Color; 4] { [Color::NEUTRAL; 4] }

/// Direction for horizontal gradient tint (sun/moon position)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HorizonDirection {
    #[default]
    East,   // 0 degrees
    North,  // 90 degrees
    West,   // 180 degrees (sunset)
    South,  // 270 degrees
}

impl HorizonDirection {
    /// Convert to radians (angle from +X axis)
    pub fn to_radians(self) -> f32 {
        match self {
            HorizonDirection::East => 0.0,
            HorizonDirection::North => std::f32::consts::FRAC_PI_2,
            HorizonDirection::West => std::f32::consts::PI,
            HorizonDirection::South => 3.0 * std::f32::consts::FRAC_PI_2,
        }
    }
}

/// Sun/Moon celestial body configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CelestialBody {
    /// Enable this celestial body
    #[serde(default)]
    pub enabled: bool,
    /// Horizontal angle (0 = East, PI/2 = North, PI = West, 3PI/2 = South)
    #[serde(default = "default_celestial_azimuth")]
    pub azimuth: f32,
    /// Vertical angle from horizon (0 = horizon, PI/2 = zenith)
    #[serde(default = "default_celestial_elevation")]
    pub elevation: f32,
    /// Angular size (radians, typical sun ~0.1)
    #[serde(default = "default_celestial_size")]
    pub size: f32,
    /// Core color (center of the orb)
    #[serde(default = "default_sun_color")]
    pub color: Color,
    /// Glow color (radiating outward)
    #[serde(default = "default_sun_glow_color")]
    pub glow_color: Color,
    /// Glow falloff (higher = tighter glow, 1.0-10.0 typical)
    #[serde(default = "default_glow_falloff")]
    pub glow_falloff: f32,
}

fn default_celestial_azimuth() -> f32 { std::f32::consts::PI } // West
fn default_celestial_elevation() -> f32 { 0.2 }
fn default_celestial_size() -> f32 { 0.1 }
fn default_sun_color() -> Color { Color::new(255, 250, 220) }
fn default_sun_glow_color() -> Color { Color::new(255, 200, 100) }
fn default_glow_falloff() -> f32 { 2.5 }

impl Default for CelestialBody {
    fn default() -> Self {
        Self {
            enabled: false,
            azimuth: default_celestial_azimuth(),
            elevation: default_celestial_elevation(),
            size: default_celestial_size(),
            color: default_sun_color(),
            glow_color: default_sun_glow_color(),
            glow_falloff: default_glow_falloff(),
        }
    }
}

/// Cloud layer configuration (Spyro-style wispy horizontal streaks)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudLayer {
    /// Vertical position (0.0 = zenith, 1.0 = below horizon)
    #[serde(default = "default_cloud_height")]
    pub height: f32,
    /// Vertical thickness of the cloud band
    #[serde(default = "default_cloud_thickness")]
    pub thickness: f32,
    /// Cloud color
    #[serde(default = "default_cloud_color")]
    pub color: Color,
    /// Opacity (0.0-1.0)
    #[serde(default = "default_cloud_opacity")]
    pub opacity: f32,
    /// Horizontal scroll speed (negative = opposite direction)
    #[serde(default = "default_cloud_speed")]
    pub scroll_speed: f32,
    /// "Wispiness" - how stretched/wispy the clouds are (0=solid, 1=very wispy)
    #[serde(default = "default_cloud_wispiness")]
    pub wispiness: f32,
    /// Density/frequency of cloud patterns
    #[serde(default = "default_cloud_density")]
    pub density: f32,
    /// Phase offset for variety between layers
    #[serde(default)]
    pub phase: f32,
}

fn default_cloud_height() -> f32 { 0.42 }
fn default_cloud_thickness() -> f32 { 0.06 }
fn default_cloud_color() -> Color { Color::new(255, 230, 200) }
fn default_cloud_opacity() -> f32 { 0.4 }
fn default_cloud_speed() -> f32 { 0.02 }
fn default_cloud_wispiness() -> f32 { 0.7 }
fn default_cloud_density() -> f32 { 1.0 }

impl Default for CloudLayer {
    fn default() -> Self {
        Self {
            height: default_cloud_height(),
            thickness: default_cloud_thickness(),
            color: default_cloud_color(),
            opacity: default_cloud_opacity(),
            scroll_speed: default_cloud_speed(),
            wispiness: default_cloud_wispiness(),
            density: default_cloud_density(),
            phase: 0.0,
        }
    }
}

/// 3D Mountain range configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountainRange {
    /// Base color (used for lit faces)
    #[serde(default = "default_mountain_lit_color")]
    pub lit_color: Color,
    /// Shadow color (used for shaded faces)
    #[serde(default = "default_mountain_shadow_color")]
    pub shadow_color: Color,
    /// Highlight color (for peaks catching direct light)
    #[serde(default = "default_mountain_highlight_color")]
    pub highlight_color: Color,
    /// Height scale (0.0-1.0)
    #[serde(default = "default_mountain_height")]
    pub height: f32,
    /// Distance/depth factor (affects atmospheric fade, 0=near, 1=far)
    #[serde(default = "default_mountain_depth")]
    pub depth: f32,
    /// Jaggedness (0=smooth rolling hills, 1=sharp peaks)
    #[serde(default = "default_mountain_jaggedness")]
    pub jaggedness: f32,
    /// Seed for procedural variation
    #[serde(default = "default_mountain_seed")]
    pub seed: u32,
}

fn default_mountain_lit_color() -> Color { Color::new(140, 120, 160) }
fn default_mountain_shadow_color() -> Color { Color::new(60, 50, 80) }
fn default_mountain_highlight_color() -> Color { Color::new(200, 180, 220) }
fn default_mountain_height() -> f32 { 0.15 }
fn default_mountain_depth() -> f32 { 0.5 }
fn default_mountain_jaggedness() -> f32 { 0.5 }
fn default_mountain_seed() -> u32 { 12345 }

impl Default for MountainRange {
    fn default() -> Self {
        Self {
            lit_color: default_mountain_lit_color(),
            shadow_color: default_mountain_shadow_color(),
            highlight_color: default_mountain_highlight_color(),
            height: default_mountain_height(),
            depth: default_mountain_depth(),
            jaggedness: default_mountain_jaggedness(),
            seed: default_mountain_seed(),
        }
    }
}

/// Star field configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarField {
    /// Enable stars
    #[serde(default)]
    pub enabled: bool,
    /// Star color
    #[serde(default = "default_star_color")]
    pub color: Color,
    /// Number of stars
    #[serde(default = "default_star_count")]
    pub count: u16,
    /// Star size (pixels, 1-3 typical)
    #[serde(default = "default_star_size")]
    pub size: f32,
    /// Twinkle animation speed (0 = static)
    #[serde(default)]
    pub twinkle_speed: f32,
    /// Seed for star positions
    #[serde(default = "default_star_seed")]
    pub seed: u32,
}

fn default_star_color() -> Color { Color::new(255, 255, 240) }
fn default_star_count() -> u16 { 80 }
fn default_star_size() -> f32 { 1.5 }
fn default_star_seed() -> u32 { 42 }

impl Default for StarField {
    fn default() -> Self {
        Self {
            enabled: false,
            color: default_star_color(),
            count: default_star_count(),
            size: default_star_size(),
            twinkle_speed: 0.0,
            seed: default_star_seed(),
        }
    }
}

/// Horizon haze configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizonHaze {
    /// Enable horizon haze
    #[serde(default = "default_haze_enabled")]
    pub enabled: bool,
    /// Haze color
    #[serde(default = "default_haze_color")]
    pub color: Color,
    /// Haze intensity (0.0-1.0)
    #[serde(default = "default_haze_intensity")]
    pub intensity: f32,
    /// Vertical extent (how far up/down from horizon)
    #[serde(default = "default_haze_extent")]
    pub extent: f32,
}

fn default_haze_enabled() -> bool { true }
fn default_haze_color() -> Color { Color::new(200, 180, 160) }
fn default_haze_intensity() -> f32 { 0.25 }
fn default_haze_extent() -> f32 { 0.12 }

impl Default for HorizonHaze {
    fn default() -> Self {
        Self {
            enabled: default_haze_enabled(),
            color: default_haze_color(),
            intensity: default_haze_intensity(),
            extent: default_haze_extent(),
        }
    }
}

fn default_horizon() -> f32 { 0.5 }

/// Skybox configuration - PS1 Spyro-style vertex-colored sky
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skybox {
    // === GRADIENT SYSTEM ===
    /// Top of sky color (zenith)
    #[serde(default = "default_zenith_color")]
    pub zenith_color: Color,
    /// Just above horizon color
    #[serde(default = "default_horizon_sky_color")]
    pub horizon_sky_color: Color,
    /// Just below horizon color
    #[serde(default = "default_horizon_ground_color")]
    pub horizon_ground_color: Color,
    /// Bottom color (nadir/ground)
    #[serde(default = "default_nadir_color")]
    pub nadir_color: Color,

    /// Enable horizontal tint (for sunrise/sunset side lighting)
    #[serde(default)]
    pub horizontal_tint_enabled: bool,
    /// Horizontal tint color
    #[serde(default = "default_horizontal_tint_color")]
    pub horizontal_tint_color: Color,
    /// Direction of the horizontal tint
    #[serde(default)]
    pub horizontal_tint_direction: HorizonDirection,
    /// Horizontal tint intensity (0.0-1.0)
    #[serde(default = "default_horizontal_tint_intensity")]
    pub horizontal_tint_intensity: f32,
    /// How wide the tint spreads (radians)
    #[serde(default = "default_horizontal_tint_spread")]
    pub horizontal_tint_spread: f32,

    /// Horizon line position (0.0 = top, 1.0 = bottom)
    #[serde(default = "default_horizon")]
    pub horizon: f32,

    // === CELESTIAL BODIES ===
    /// Sun configuration
    #[serde(default)]
    pub sun: CelestialBody,
    /// Moon configuration
    #[serde(default)]
    pub moon: CelestialBody,

    // === CLOUD LAYERS ===
    /// Two cloud layers for depth
    #[serde(default)]
    pub cloud_layers: [Option<CloudLayer>; 2],

    // === MOUNTAINS ===
    /// Two mountain ranges at different depths
    #[serde(default)]
    pub mountain_ranges: [Option<MountainRange>; 2],
    /// Direction the "sun" is for mountain lighting
    #[serde(default)]
    pub mountain_light_direction: HorizonDirection,

    // === STARS ===
    /// Star field configuration
    #[serde(default)]
    pub stars: StarField,

    // === ATMOSPHERE ===
    /// Horizon haze configuration
    #[serde(default)]
    pub horizon_haze: HorizonHaze,
}

fn default_zenith_color() -> Color { Color::new(40, 60, 120) }
fn default_horizon_sky_color() -> Color { Color::new(180, 140, 120) }
fn default_horizon_ground_color() -> Color { Color::new(160, 120, 100) }
fn default_nadir_color() -> Color { Color::new(80, 70, 90) }
fn default_horizontal_tint_color() -> Color { Color::new(255, 180, 120) }
fn default_horizontal_tint_intensity() -> f32 { 0.4 }
fn default_horizontal_tint_spread() -> f32 { 1.05 } // ~60 degrees

impl Skybox {
    /// Sample the sky color at a given direction
    /// theta: horizontal angle (0 = +X/East, increases counter-clockwise)
    /// phi: vertical angle (0 = zenith, PI = nadir)
    pub fn sample_at_direction(&self, theta: f32, phi: f32, time: f32) -> Color {
        use std::f32::consts::PI;

        // Convert phi to vertical gradient position (0 = top, 1 = bottom)
        let v = phi / PI;

        // === BASE VERTICAL GRADIENT ===
        let base_color = if v < self.horizon {
            // Above horizon: zenith to horizon_sky
            let t = if self.horizon > 0.0 { v / self.horizon } else { 0.0 };
            self.zenith_color.lerp(self.horizon_sky_color, t)
        } else {
            // Below horizon: horizon_ground to nadir
            let t = if self.horizon < 1.0 { (v - self.horizon) / (1.0 - self.horizon) } else { 1.0 };
            self.horizon_ground_color.lerp(self.nadir_color, t)
        };

        let mut color = base_color;

        // === HORIZONTAL TINT ===
        if self.horizontal_tint_enabled && self.horizontal_tint_intensity > 0.0 {
            let tint_angle = self.horizontal_tint_direction.to_radians();
            let mut angle_diff = (theta - tint_angle).abs();
            if angle_diff > PI {
                angle_diff = 2.0 * PI - angle_diff;
            }

            if angle_diff < self.horizontal_tint_spread {
                let tint_strength = (1.0 - angle_diff / self.horizontal_tint_spread).powi(2);
                let tint_strength = tint_strength * self.horizontal_tint_intensity;

                // Tint is strongest near horizon
                let horizon_factor = 1.0 - ((v - self.horizon).abs() / 0.3).min(1.0);
                let final_strength = tint_strength * horizon_factor;

                color = color.lerp(self.horizontal_tint_color, final_strength);
            }
        }

        // === HORIZON HAZE ===
        if self.horizon_haze.enabled && self.horizon_haze.intensity > 0.0 {
            let dist_from_horizon = (v - self.horizon).abs();
            if dist_from_horizon < self.horizon_haze.extent {
                let haze_strength = (1.0 - dist_from_horizon / self.horizon_haze.extent).powi(2);
                let haze_strength = haze_strength * self.horizon_haze.intensity;
                color = color.lerp(self.horizon_haze.color, haze_strength);
            }
        }

        // === SUN/MOON GLOW ===
        for celestial in [&self.sun, &self.moon] {
            if celestial.enabled {
                let body_phi = PI / 2.0 - celestial.elevation; // Convert elevation to phi
                let body_theta = celestial.azimuth;

                // Calculate angular distance from celestial body using spherical law of cosines
                let cos_dist = phi.sin() * body_phi.sin() * (theta - body_theta).cos()
                             + phi.cos() * body_phi.cos();
                let angular_dist = cos_dist.clamp(-1.0, 1.0).acos();

                // Core of sun/moon
                if angular_dist < celestial.size {
                    let core_strength = 1.0 - (angular_dist / celestial.size);
                    color = color.lerp(celestial.color, core_strength);
                }
                // Glow around sun/moon
                else {
                    let glow_radius = celestial.size * 4.0;
                    if angular_dist < glow_radius {
                        let glow_t = (angular_dist - celestial.size) / (glow_radius - celestial.size);
                        let glow_strength = (1.0 - glow_t).powf(celestial.glow_falloff);
                        color = color.lerp(celestial.glow_color, glow_strength * 0.6);
                    }
                }
            }
        }

        // === CLOUD LAYERS ===
        for layer_opt in &self.cloud_layers {
            if let Some(layer) = layer_opt {
                let layer_v_min = layer.height - layer.thickness / 2.0;
                let layer_v_max = layer.height + layer.thickness / 2.0;

                if v >= layer_v_min && v <= layer_v_max && layer.opacity > 0.0 {
                    let scroll = time * layer.scroll_speed;
                    let cloud_val = self.sample_wispy_cloud(
                        theta + scroll,
                        v,
                        layer.wispiness,
                        layer.density,
                        layer.phase
                    );

                    // Edge falloff within the band
                    let dist_from_center = (v - layer.height).abs() / (layer.thickness / 2.0);
                    let edge_falloff = (1.0 - dist_from_center).clamp(0.0, 1.0);

                    let cloud_strength = cloud_val * layer.opacity * edge_falloff;
                    color = color.lerp(layer.color, cloud_strength);
                }
            }
        }

        color
    }

    /// Sample wispy cloud pattern (Spyro-style stretched horizontal clouds)
    fn sample_wispy_cloud(&self, theta: f32, v: f32, wispiness: f32, density: f32, phase: f32) -> f32 {
        // Multiple octaves of horizontally-stretched noise
        let stretch = 8.0 + wispiness * 16.0; // More wispy = more horizontal stretch

        let n1 = ((theta * density * 3.0 + phase).sin() * stretch + v * 50.0).sin();
        let n2 = ((theta * density * 7.0 + phase * 2.0).sin() * stretch * 0.5 + v * 120.0).sin();
        let n3 = ((theta * density * 13.0 + phase * 0.7).sin() * stretch * 0.3 + v * 200.0).sin();

        let raw = (n1 * 0.5 + n2 * 0.3 + n3 * 0.2 + 0.5).clamp(0.0, 1.0);

        // Apply wispiness threshold - more wispy = more gaps
        let threshold = wispiness * 0.5;
        if raw < threshold {
            0.0
        } else {
            ((raw - threshold) / (1.0 - threshold)).powf(0.7)
        }
    }

    /// Generate skybox mesh geometry (vertices and face indices)
    /// Returns (vertices, faces) for a vertex-colored sphere centered at camera_pos
    /// Includes 3D mountain geometry as part of the sphere
    pub fn generate_mesh(&self, camera_pos: (f32, f32, f32), time: f32) -> (Vec<SkyboxVertex>, Vec<[usize; 3]>) {
        use std::f32::consts::PI;

        let radius = 10000.0; // Large radius so it's always "far away"
        let h_segments = 48; // More horizontal segments for color variation
        let v_segments = 32; // More vertical for smoother gradients

        let mut vertices = Vec::new();
        let mut faces = Vec::new();

        // Generate sphere vertices with vertex colors
        for v in 0..=v_segments {
            let phi = PI * v as f32 / v_segments as f32; // 0 at top, PI at bottom
            let y = phi.cos();      // 1 at top, -1 at bottom
            let ring_radius = phi.sin();

            for h in 0..=h_segments {
                let theta = 2.0 * PI * h as f32 / h_segments as f32;

                let x = ring_radius * theta.cos();
                let z = ring_radius * theta.sin();

                // Sample color at this direction
                let color = self.sample_at_direction(theta, phi, time);

                vertices.push(SkyboxVertex {
                    pos: (
                        camera_pos.0 + x * radius,
                        camera_pos.1 + y * radius,
                        camera_pos.2 + z * radius,
                    ),
                    color,
                });
            }
        }

        // Generate faces (triangles) - winding for inside-facing normals
        for v in 0..v_segments {
            for h in 0..h_segments {
                let row_width = h_segments + 1;
                let i0 = v * row_width + h;
                let i1 = v * row_width + h + 1;
                let i2 = (v + 1) * row_width + h;
                let i3 = (v + 1) * row_width + h + 1;

                // Two triangles per quad, wound for inward-facing view
                faces.push([i0, i2, i1]);
                faces.push([i1, i2, i3]);
            }
        }

        // Generate 3D mountain geometry - individual peaked mountains like Spyro
        let light_angle = self.mountain_light_direction.to_radians();

        // Process mountain ranges from back to front (higher depth = further back = draw first)
        let mut ranges_with_depth: Vec<(usize, &MountainRange)> = self.mountain_ranges.iter()
            .enumerate()
            .filter_map(|(i, opt)| opt.as_ref().map(|r| (i, r)))
            .collect();
        ranges_with_depth.sort_by(|a, b| b.1.depth.partial_cmp(&a.1.depth).unwrap());

        for (_range_idx, range) in ranges_with_depth {
            // Mountain radius slightly inside the sky sphere (so it renders in front)
            let mtn_radius = radius * (0.99 - range.depth * 0.02);

            // Horizon phi angle - where mountains sit
            let horizon_phi = self.horizon * PI;
            let base_phi = horizon_phi + 0.08; // Base below horizon
            let max_mtn_height = range.height * 1.2; // Much taller mountains

            // Generate individual peaked mountains
            // Use procedural placement based on seed
            let num_peaks = 12 + (range.jaggedness * 8.0) as usize;
            let mut peak_angles: Vec<f32> = Vec::new();
            let mut peak_heights: Vec<f32> = Vec::new();

            // Generate peak positions using LCG
            let mut rng = range.seed as u64;
            let mut next_rand = || -> f32 {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                ((rng >> 16) & 0xFFFF) as f32 / 65536.0
            };

            for _ in 0..num_peaks {
                let angle = next_rand() * 2.0 * PI;
                let height = 0.3 + next_rand() * 0.7; // Height variation
                peak_angles.push(angle);
                peak_heights.push(height);
            }

            // Sort peaks by angle for consistent rendering
            let mut peaks: Vec<(f32, f32)> = peak_angles.iter().zip(peak_heights.iter())
                .map(|(&a, &h)| (a, h))
                .collect();
            peaks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

            // Generate each mountain as a triangular peak with left and right faces
            for (peak_theta, peak_height) in &peaks {
                let base_vertex_idx = vertices.len();

                // Peak width varies with height (taller = wider base)
                let half_width = 0.12 + peak_height * 0.15 * (1.0 - range.jaggedness * 0.5);

                let left_theta = peak_theta - half_width;
                let right_theta = peak_theta + half_width;
                let peak_phi = horizon_phi - peak_height * max_mtn_height;

                // Calculate face normals for lighting
                // Left face points toward (peak_theta - PI/2)
                // Right face points toward (peak_theta + PI/2)
                let left_face_angle = peak_theta - half_width / 2.0;
                let right_face_angle = peak_theta + half_width / 2.0;

                // Light factor for left face
                let mut left_to_light = (left_face_angle - light_angle).abs();
                if left_to_light > PI { left_to_light = 2.0 * PI - left_to_light; }
                let left_lit = left_to_light < PI / 2.0;
                let left_light = if left_lit {
                    ((PI / 2.0 - left_to_light) / (PI / 2.0)).clamp(0.0, 1.0)
                } else { 0.0 };

                // Light factor for right face
                let mut right_to_light = (right_face_angle - light_angle).abs();
                if right_to_light > PI { right_to_light = 2.0 * PI - right_to_light; }
                let right_lit = right_to_light < PI / 2.0;
                let right_light = if right_lit {
                    ((PI / 2.0 - right_to_light) / (PI / 2.0)).clamp(0.0, 1.0)
                } else { 0.0 };

                // Colors for each face
                let left_color = range.shadow_color.lerp(range.lit_color, left_light);
                let right_color = range.shadow_color.lerp(range.lit_color, right_light);

                // Peak highlight
                let peak_light = (left_light + right_light) / 2.0;
                let peak_color = if *peak_height > 0.5 {
                    let highlight_t = ((*peak_height - 0.5) / 0.5 * peak_light).min(0.5);
                    range.shadow_color.lerp(range.highlight_color, highlight_t)
                } else {
                    range.shadow_color.lerp(range.lit_color, peak_light)
                };

                // Apply atmospheric fade
                let depth_fade = range.depth * 0.5;
                let left_final = left_color.lerp(self.horizon_haze.color, depth_fade);
                let right_final = right_color.lerp(self.horizon_haze.color, depth_fade);
                let peak_final = peak_color.lerp(self.horizon_haze.color, depth_fade * 0.8);
                let base_color = range.shadow_color.lerp(self.horizon_haze.color, depth_fade);

                // Vertex positions
                let peak_y = peak_phi.cos();
                let peak_ring = peak_phi.sin();
                let base_y = base_phi.cos();
                let base_ring = base_phi.sin();

                // Peak vertex (index 0)
                vertices.push(SkyboxVertex {
                    pos: (
                        camera_pos.0 + peak_ring * peak_theta.cos() * mtn_radius,
                        camera_pos.1 + peak_y * mtn_radius,
                        camera_pos.2 + peak_ring * peak_theta.sin() * mtn_radius,
                    ),
                    color: peak_final,
                });

                // Left base vertex (index 1)
                vertices.push(SkyboxVertex {
                    pos: (
                        camera_pos.0 + base_ring * left_theta.cos() * mtn_radius,
                        camera_pos.1 + base_y * mtn_radius,
                        camera_pos.2 + base_ring * left_theta.sin() * mtn_radius,
                    ),
                    color: left_final,
                });

                // Right base vertex (index 2)
                vertices.push(SkyboxVertex {
                    pos: (
                        camera_pos.0 + base_ring * right_theta.cos() * mtn_radius,
                        camera_pos.1 + base_y * mtn_radius,
                        camera_pos.2 + base_ring * right_theta.sin() * mtn_radius,
                    ),
                    color: right_final,
                });

                // Center base vertex for filling (index 3)
                vertices.push(SkyboxVertex {
                    pos: (
                        camera_pos.0 + base_ring * peak_theta.cos() * mtn_radius,
                        camera_pos.1 + base_y * mtn_radius,
                        camera_pos.2 + base_ring * peak_theta.sin() * mtn_radius,
                    ),
                    color: base_color,
                });

                // Left face: peak -> left_base -> center_base
                faces.push([base_vertex_idx, base_vertex_idx + 1, base_vertex_idx + 3]);
                // Right face: peak -> center_base -> right_base
                faces.push([base_vertex_idx, base_vertex_idx + 3, base_vertex_idx + 2]);
            }
        }

        (vertices, faces)
    }

    /// Sample mountain height at a given angle
    pub fn sample_mountain_height(&self, theta: f32, range: &MountainRange) -> f32 {
        let seed = range.seed as f32 * 0.001;
        let j = range.jaggedness;

        // Multiple octaves for natural peaks
        let m1 = ((theta * 3.0 + seed).sin() * 0.5 + 0.5) * 0.4;
        let m2 = ((theta * 7.0 + seed * 2.0).sin() * 0.5 + 0.5) * 0.3 * (0.5 + j * 0.5);
        let m3 = ((theta * 13.0 + seed * 0.5).sin() * 0.5 + 0.5) * 0.2 * j;
        let m4 = ((theta * 23.0 + seed * 1.5).sin() * 0.5 + 0.5) * 0.1 * j;

        (m1 + m2 + m3 + m4).min(1.0)
    }

    /// Create a sunset preset
    pub fn preset_sunset() -> Self {
        Self {
            zenith_color: Color::new(60, 40, 100),
            horizon_sky_color: Color::new(255, 160, 100),
            horizon_ground_color: Color::new(200, 140, 160),
            nadir_color: Color::new(120, 100, 140),

            horizontal_tint_enabled: true,
            horizontal_tint_color: Color::new(255, 200, 120),
            horizontal_tint_direction: HorizonDirection::West,
            horizontal_tint_intensity: 0.5,
            horizontal_tint_spread: 1.2,

            horizon: 0.52,

            sun: CelestialBody {
                enabled: true,
                azimuth: std::f32::consts::PI,
                elevation: 0.15,
                size: 0.12,
                color: Color::new(255, 250, 200),
                glow_color: Color::new(255, 180, 80),
                glow_falloff: 2.0,
            },
            moon: CelestialBody::default(),

            cloud_layers: [
                Some(CloudLayer {
                    height: 0.35,
                    thickness: 0.05,
                    color: Color::new(255, 200, 160),
                    opacity: 0.4,
                    scroll_speed: 0.01,
                    wispiness: 0.85,
                    density: 0.8,
                    phase: 0.0,
                }),
                Some(CloudLayer {
                    height: 0.45,
                    thickness: 0.08,
                    color: Color::new(255, 180, 140),
                    opacity: 0.5,
                    scroll_speed: 0.02,
                    wispiness: 0.7,
                    density: 1.0,
                    phase: 2.5,
                }),
            ],

            mountain_ranges: [
                Some(MountainRange {
                    lit_color: Color::new(180, 140, 180),
                    shadow_color: Color::new(80, 60, 100),
                    highlight_color: Color::new(255, 200, 200),
                    height: 0.15,
                    depth: 0.6,
                    jaggedness: 0.4,
                    seed: 11111,
                }),
                None,
            ],
            mountain_light_direction: HorizonDirection::West,

            stars: StarField { enabled: false, ..Default::default() },

            horizon_haze: HorizonHaze {
                enabled: true,
                color: Color::new(255, 200, 160),
                intensity: 0.35,
                extent: 0.15,
            },
        }
    }

    /// Create a twilight preset
    pub fn preset_twilight() -> Self {
        Self {
            zenith_color: Color::new(30, 40, 80),
            horizon_sky_color: Color::new(100, 80, 140),
            horizon_ground_color: Color::new(60, 80, 100),
            nadir_color: Color::new(40, 60, 80),

            horizontal_tint_enabled: true,
            horizontal_tint_color: Color::new(200, 140, 180),
            horizontal_tint_direction: HorizonDirection::West,
            horizontal_tint_intensity: 0.35,
            horizontal_tint_spread: 1.0,

            horizon: 0.55,

            sun: CelestialBody::default(),
            moon: CelestialBody::default(),

            cloud_layers: [
                Some(CloudLayer {
                    height: 0.42,
                    thickness: 0.06,
                    color: Color::new(220, 200, 180),
                    opacity: 0.35,
                    scroll_speed: 0.008,
                    wispiness: 0.9,
                    density: 0.7,
                    phase: 0.0,
                }),
                None,
            ],

            mountain_ranges: [
                Some(MountainRange {
                    lit_color: Color::new(80, 90, 140),
                    shadow_color: Color::new(40, 50, 80),
                    highlight_color: Color::new(120, 130, 180),
                    height: 0.12,
                    depth: 0.7,
                    jaggedness: 0.3,
                    seed: 22222,
                }),
                None,
            ],
            mountain_light_direction: HorizonDirection::West,

            stars: StarField {
                enabled: true,
                color: Color::new(255, 255, 220),
                count: 60,
                size: 1.5,
                twinkle_speed: 0.5,
                seed: 42,
            },

            horizon_haze: HorizonHaze {
                enabled: true,
                color: Color::new(140, 120, 160),
                intensity: 0.25,
                extent: 0.12,
            },
        }
    }

    /// Create an arctic/ice preset
    pub fn preset_arctic() -> Self {
        Self {
            zenith_color: Color::new(60, 100, 140),
            horizon_sky_color: Color::new(140, 180, 200),
            horizon_ground_color: Color::new(180, 200, 220),
            nadir_color: Color::new(100, 140, 180),

            horizontal_tint_enabled: true,
            horizontal_tint_color: Color::new(200, 150, 180),
            horizontal_tint_direction: HorizonDirection::East,
            horizontal_tint_intensity: 0.25,
            horizontal_tint_spread: 1.5,

            horizon: 0.5,

            sun: CelestialBody::default(),
            moon: CelestialBody::default(),

            cloud_layers: [
                Some(CloudLayer {
                    height: 0.35,
                    thickness: 0.04,
                    color: Color::new(220, 200, 240),
                    opacity: 0.3,
                    scroll_speed: 0.005,
                    wispiness: 0.6,
                    density: 0.5,
                    phase: 0.0,
                }),
                Some(CloudLayer {
                    height: 0.48,
                    thickness: 0.03,
                    color: Color::new(200, 220, 240),
                    opacity: 0.4,
                    scroll_speed: 0.003,
                    wispiness: 0.4,
                    density: 0.6,
                    phase: 1.5,
                }),
            ],

            mountain_ranges: [
                Some(MountainRange {
                    lit_color: Color::new(200, 210, 230),
                    shadow_color: Color::new(100, 120, 160),
                    highlight_color: Color::new(255, 255, 255),
                    height: 0.2,
                    depth: 0.3,
                    jaggedness: 0.7,
                    seed: 33333,
                }),
                Some(MountainRange {
                    lit_color: Color::new(160, 180, 210),
                    shadow_color: Color::new(80, 100, 140),
                    highlight_color: Color::new(220, 230, 250),
                    height: 0.25,
                    depth: 0.5,
                    jaggedness: 0.5,
                    seed: 44444,
                }),
            ],
            mountain_light_direction: HorizonDirection::East,

            stars: StarField::default(),

            horizon_haze: HorizonHaze {
                enabled: true,
                color: Color::new(180, 200, 220),
                intensity: 0.4,
                extent: 0.1,
            },
        }
    }

    /// Create a night preset with stars and moon
    pub fn preset_night() -> Self {
        Self {
            zenith_color: Color::new(10, 15, 40),
            horizon_sky_color: Color::new(20, 35, 70),
            horizon_ground_color: Color::new(15, 25, 50),
            nadir_color: Color::new(5, 10, 25),

            horizontal_tint_enabled: false,
            horizontal_tint_color: Color::new(100, 100, 150),
            horizontal_tint_direction: HorizonDirection::East,
            horizontal_tint_intensity: 0.0,
            horizontal_tint_spread: 1.0,

            horizon: 0.5,

            sun: CelestialBody::default(),
            moon: CelestialBody {
                enabled: true,
                azimuth: std::f32::consts::FRAC_PI_4, // Northeast
                elevation: 0.6,
                size: 0.08,
                color: Color::new(240, 240, 255),
                glow_color: Color::new(180, 180, 220),
                glow_falloff: 4.0,
            },

            cloud_layers: [None, None],

            mountain_ranges: [
                Some(MountainRange {
                    lit_color: Color::new(30, 35, 50),
                    shadow_color: Color::new(15, 20, 35),
                    highlight_color: Color::new(50, 55, 75),
                    height: 0.12,
                    depth: 0.6,
                    jaggedness: 0.4,
                    seed: 55555,
                }),
                None,
            ],
            mountain_light_direction: HorizonDirection::East,

            stars: StarField {
                enabled: true,
                color: Color::new(255, 255, 245),
                count: 150,
                size: 1.8,
                twinkle_speed: 1.0,
                seed: 12345,
            },

            horizon_haze: HorizonHaze {
                enabled: true,
                color: Color::new(30, 40, 70),
                intensity: 0.2,
                extent: 0.08,
            },
        }
    }
}

/// Vertex for skybox mesh (simplified, no UV/normal needed)
#[derive(Debug, Clone)]
pub struct SkyboxVertex {
    pub pos: (f32, f32, f32),
    pub color: Color,
}

impl Default for Skybox {
    fn default() -> Self {
        Self::preset_sunset()
    }
}

/// Face normal rendering mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaceNormalMode {
    #[default]
    Front,  // Normal faces outward (default)
    Both,   // Double-sided (render both sides)
    Back,   // Normal faces inward (flipped)
}

/// UV projection mode for sloped faces
/// Controls how UVs are interpolated across the face's triangles
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UvProjection {
    #[default]
    Default,    // Standard per-vertex UV interpolation (may cause seams on sloped faces)
    Projected,  // Project UVs as if the face were flat (uniform texture across face)
}

/// Direction of the diagonal split for floor/ceiling triangulation
/// A quad is always split into 2 triangles - this controls which diagonal is used
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDirection {
    #[default]
    NwSe,   // Split along NW-SE diagonal: Triangle1 = NW,NE,SE, Triangle2 = NW,SE,SW
    NeSw,   // Split along NE-SW diagonal: Triangle1 = NW,NE,SW, Triangle2 = NE,SE,SW
}

impl SplitDirection {
    /// Cycle to the next split direction
    pub fn next(self) -> Self {
        match self {
            SplitDirection::NwSe => SplitDirection::NeSw,
            SplitDirection::NeSw => SplitDirection::NwSe,
        }
    }

    /// Human-readable label
    pub fn label(&self) -> &'static str {
        match self {
            SplitDirection::NwSe => "NW-SE",
            SplitDirection::NeSw => "NE-SW",
        }
    }

    /// Get the corner indices for triangle 1
    /// Returns [corner_a, corner_b, corner_c] in winding order
    pub fn triangle_1_corners(&self) -> [usize; 3] {
        match self {
            SplitDirection::NwSe => [0, 1, 2], // NW, NE, SE
            SplitDirection::NeSw => [0, 1, 3], // NW, NE, SW
        }
    }

    /// Get the corner indices for triangle 2
    /// Returns [corner_a, corner_b, corner_c] in winding order
    pub fn triangle_2_corners(&self) -> [usize; 3] {
        match self {
            SplitDirection::NwSe => [0, 2, 3], // NW, SE, SW
            SplitDirection::NeSw => [1, 2, 3], // NE, SE, SW
        }
    }
}

/// A horizontal face (floor or ceiling)
/// Consists of 2 triangles that share 4 corner heights but can have different textures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizontalFace {
    /// Corner heights [NW, NE, SE, SW] - allows sloped surfaces
    /// NW = (-X, -Z), NE = (+X, -Z), SE = (+X, +Z), SW = (-X, +Z)
    pub heights: [f32; 4],

    /// Direction of diagonal split (which diagonal divides the quad into 2 triangles)
    #[serde(default)]
    pub split_direction: SplitDirection,

    // === Triangle 1 properties (primary) ===
    /// Texture reference for triangle 1 (and triangle 2 if texture_2 is None)
    pub texture: TextureRef,
    /// Custom UV coordinates for triangle 1 (None = use default 0,0 to 1,1)
    #[serde(default)]
    pub uv: Option<[Vec2; 4]>,
    /// PS1-style vertex colors for texture modulation [NW, NE, SE, SW]
    #[serde(default = "default_neutral_colors_4")]
    pub colors: [Color; 4],

    // === Triangle 2 properties (optional overrides) ===
    /// Texture for triangle 2 (None = use same as triangle 1)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture_2: Option<TextureRef>,
    /// UV coordinates for triangle 2 (None = use same as triangle 1)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv_2: Option<[Vec2; 4]>,
    /// Vertex colors for triangle 2 (None = use same as triangle 1)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors_2: Option<[Color; 4]>,

    // === Shared properties ===
    /// Is this surface walkable? (for collision/AI)
    #[serde(default = "default_true")]
    pub walkable: bool,
    /// Transparency/blend mode
    #[serde(default)]
    pub blend_mode: BlendMode,
    /// Normal rendering mode (front, both, or back)
    #[serde(default)]
    pub normal_mode: FaceNormalMode,
    /// If true, pure black pixels (RGB 0,0,0) are treated as transparent (PS1 CLUT-style)
    #[serde(default = "default_true")]
    pub black_transparent: bool,
}

impl HorizontalFace {
    /// Create a flat horizontal face at the given height
    pub fn flat(height: f32, texture: TextureRef) -> Self {
        Self {
            heights: [height, height, height, height],
            split_direction: SplitDirection::NwSe,
            texture,
            uv: None,
            colors: [Color::NEUTRAL; 4],
            texture_2: None,
            uv_2: None,
            colors_2: None,
            walkable: true,
            blend_mode: BlendMode::Opaque,
            normal_mode: FaceNormalMode::default(),
            black_transparent: true,
        }
    }

    /// Create a sloped horizontal face
    pub fn sloped(heights: [f32; 4], texture: TextureRef) -> Self {
        Self {
            heights,
            split_direction: SplitDirection::NwSe,
            texture,
            uv: None,
            colors: [Color::NEUTRAL; 4],
            texture_2: None,
            uv_2: None,
            colors_2: None,
            walkable: true,
            blend_mode: BlendMode::Opaque,
            normal_mode: FaceNormalMode::default(),
            black_transparent: true,
        }
    }

    /// Get effective texture for triangle 2 (returns texture_2 or falls back to texture)
    pub fn get_texture_2(&self) -> &TextureRef {
        self.texture_2.as_ref().unwrap_or(&self.texture)
    }

    /// Get effective UV for triangle 2 (returns uv_2 or falls back to uv)
    pub fn get_uv_2(&self) -> Option<&[Vec2; 4]> {
        self.uv_2.as_ref().or(self.uv.as_ref())
    }

    /// Get effective colors for triangle 2 (returns colors_2 or falls back to colors)
    pub fn get_colors_2(&self) -> &[Color; 4] {
        self.colors_2.as_ref().unwrap_or(&self.colors)
    }

    /// Check if triangle 2 has different properties than triangle 1
    pub fn has_split_textures(&self) -> bool {
        self.texture_2.is_some() || self.uv_2.is_some() || self.colors_2.is_some()
    }


    /// Set all vertex colors to the same value (uniform tint)
    pub fn set_uniform_color(&mut self, color: Color) {
        self.colors = [color; 4];
    }

    /// Check if all vertex colors are the same
    pub fn has_uniform_color(&self) -> bool {
        self.colors[0].r == self.colors[1].r && self.colors[0].r == self.colors[2].r && self.colors[0].r == self.colors[3].r &&
        self.colors[0].g == self.colors[1].g && self.colors[0].g == self.colors[2].g && self.colors[0].g == self.colors[3].g &&
        self.colors[0].b == self.colors[1].b && self.colors[0].b == self.colors[2].b && self.colors[0].b == self.colors[3].b
    }

    /// Get average height of the face
    pub fn avg_height(&self) -> f32 {
        (self.heights[0] + self.heights[1] + self.heights[2] + self.heights[3]) / 4.0
    }

    /// Check if the face is flat (all corners at same height)
    pub fn is_flat(&self) -> bool {
        let h = self.heights[0];
        self.heights.iter().all(|&corner| (corner - h).abs() < 0.001)
    }

    /// Get interpolated height at a position within the sector.
    /// `u` and `v` are normalized coordinates within the sector (0.0 to 1.0).
    /// u = 0 is West (-X), u = 1 is East (+X)
    /// v = 0 is North (-Z), v = 1 is South (+Z)
    ///
    /// Heights layout: [NW, NE, SE, SW] = [0, 1, 2, 3]
    /// NW = (u=0, v=0), NE = (u=1, v=0), SE = (u=1, v=1), SW = (u=0, v=1)
    ///
    /// The quad is split into two triangles based on split_direction:
    /// - NW-SE split: Triangle 1 = NW,NE,SE (u >= v), Triangle 2 = NW,SE,SW (u < v)
    /// - NE-SW split: Triangle 1 = NW,NE,SW (u + v <= 1), Triangle 2 = NE,SE,SW (u + v > 1)
    pub fn interpolate_height(&self, u: f32, v: f32) -> f32 {
        // Clamp to valid range
        let u = u.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);

        match self.split_direction {
            SplitDirection::NwSe => {
                // Split along NW-SE diagonal
                if u >= v {
                    // Triangle 1: NW, NE, SE
                    let h_nw = self.heights[0];
                    let h_ne = self.heights[1];
                    let h_se = self.heights[2];
                    h_nw + u * (h_ne - h_nw) + v * (h_se - h_ne)
                } else {
                    // Triangle 2: NW, SE, SW
                    let h_nw = self.heights[0];
                    let h_se = self.heights[2];
                    let h_sw = self.heights[3];
                    h_nw + u * (h_se - h_sw) + v * (h_sw - h_nw)
                }
            }
            SplitDirection::NeSw => {
                // Split along NE-SW diagonal
                if u + v <= 1.0 {
                    // Triangle 1: NW, NE, SW
                    let h_nw = self.heights[0];
                    let h_ne = self.heights[1];
                    let h_sw = self.heights[3];
                    h_nw + u * (h_ne - h_nw) + v * (h_sw - h_nw)
                } else {
                    // Triangle 2: NE, SE, SW
                    let h_ne = self.heights[1];
                    let h_se = self.heights[2];
                    let h_sw = self.heights[3];
                    h_sw + u * (h_se - h_sw) + (1.0 - v) * (h_ne - h_se)
                }
            }
        }
    }

    /// Get heights at a specific edge (left_corner, right_corner) when looking from inside the sector
    /// Returns (left_height, right_height) for the edge in that direction
    pub fn edge_heights(&self, dir: Direction) -> (f32, f32) {
        // Heights are [NW, NE, SE, SW] = [0, 1, 2, 3]
        // NW = (-X, -Z), NE = (+X, -Z), SE = (+X, +Z), SW = (-X, +Z)
        match dir {
            Direction::North => (self.heights[0], self.heights[1]), // NW, NE (looking at -Z edge)
            Direction::East => (self.heights[1], self.heights[2]),  // NE, SE (looking at +X edge)
            Direction::South => (self.heights[3], self.heights[2]), // SW, SE (looking at +Z edge)
            Direction::West => (self.heights[0], self.heights[3]),  // NW, SW (looking at -X edge)
            // For diagonals, return the two corners that form the diagonal
            Direction::NwSe => (self.heights[0], self.heights[2]),  // NW, SE
            Direction::NeSw => (self.heights[1], self.heights[3]),  // NE, SW
        }
    }

    /// Get max height at a specific edge
    pub fn edge_max(&self, dir: Direction) -> f32 {
        let (h1, h2) = self.edge_heights(dir);
        h1.max(h2)
    }

    /// Get min height at a specific edge
    pub fn edge_min(&self, dir: Direction) -> f32 {
        let (h1, h2) = self.edge_heights(dir);
        h1.min(h2)
    }
}

/// A vertical face (wall) on a sector edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerticalFace {
    /// Corner heights: [bottom-left, bottom-right, top-right, top-left]
    pub heights: [f32; 4],
    /// Texture reference
    pub texture: TextureRef,
    /// Custom UV coordinates (None = use default)
    #[serde(default)]
    pub uv: Option<[Vec2; 4]>,
    /// Is this a solid wall for collision?
    #[serde(default = "default_true")]
    pub solid: bool,
    /// Transparency/blend mode
    #[serde(default)]
    pub blend_mode: BlendMode,
    /// PS1-style vertex colors for texture modulation [bottom-left, bottom-right, top-right, top-left]
    /// 128 = neutral (no tint), <128 = darken, >128 = brighten
    /// Per-vertex colors enable Gouraud-style color gradients across the wall
    #[serde(default = "default_neutral_colors_4")]
    pub colors: [Color; 4],
    /// Normal rendering mode (front, both, or back)
    #[serde(default)]
    pub normal_mode: FaceNormalMode,
    /// If true, pure black pixels (RGB 0,0,0) are treated as transparent (PS1 CLUT-style)
    #[serde(default = "default_true")]
    pub black_transparent: bool,
    /// UV projection mode for sloped walls
    #[serde(default)]
    pub uv_projection: UvProjection,
}

impl VerticalFace {
    /// Compute 1:1 texel UV mapping based on wall height
    /// x_scale = UV_SCALE (wall width maps to UV_SCALE of texture)
    /// y_scale = wall_height / SECTOR_SIZE * UV_SCALE
    fn compute_1to1_uv(heights: &[f32; 4]) -> [Vec2; 4] {
        let bottom = (heights[0] + heights[1]) / 2.0;
        let top = (heights[2] + heights[3]) / 2.0;
        let wall_height = (top - bottom).abs();
        let v_scale = wall_height / SECTOR_SIZE * UV_SCALE;

        // UV corners: [bottom-left, bottom-right, top-right, top-left]
        // Default UV with x_scale=UV_SCALE, y_scale=v_scale, no rotation, no offset
        [
            Vec2::new(0.0, v_scale),       // bottom-left
            Vec2::new(UV_SCALE, v_scale),  // bottom-right
            Vec2::new(UV_SCALE, 0.0),      // top-right
            Vec2::new(0.0, 0.0),           // top-left
        ]
    }

    /// Create a wall from bottom to top (all corners at same heights)
    /// UVs default to None so world-aligned tiling is used during rendering
    pub fn new(y_bottom: f32, y_top: f32, texture: TextureRef) -> Self {
        let heights = [y_bottom, y_bottom, y_top, y_top];
        Self {
            uv: None,  // Use world-aligned default UVs
            heights,
            texture,
            solid: true,
            blend_mode: BlendMode::Opaque,
            colors: [Color::NEUTRAL; 4],
            normal_mode: FaceNormalMode::default(),
            black_transparent: true,
            uv_projection: UvProjection::default(),
        }
    }

    /// Create a wall with individual corner heights for sloped surfaces
    /// Heights order: [bottom-left, bottom-right, top-right, top-left]
    /// UVs default to None so world-aligned tiling is used during rendering
    pub fn new_sloped(bl: f32, br: f32, tr: f32, tl: f32, texture: TextureRef) -> Self {
        let heights = [bl, br, tr, tl];
        Self {
            uv: None,  // Use world-aligned default UVs
            heights,
            texture,
            solid: true,
            blend_mode: BlendMode::Opaque,
            colors: [Color::NEUTRAL; 4],
            normal_mode: FaceNormalMode::default(),
            black_transparent: true,
            uv_projection: UvProjection::Projected,
        }
    }

    /// Set all vertex colors to the same value (uniform tint)
    pub fn set_uniform_color(&mut self, color: Color) {
        self.colors = [color; 4];
    }

    /// Check if all vertex colors are the same
    pub fn has_uniform_color(&self) -> bool {
        self.colors[0].r == self.colors[1].r && self.colors[0].r == self.colors[2].r && self.colors[0].r == self.colors[3].r &&
        self.colors[0].g == self.colors[1].g && self.colors[0].g == self.colors[2].g && self.colors[0].g == self.colors[3].g &&
        self.colors[0].b == self.colors[1].b && self.colors[0].b == self.colors[2].b && self.colors[0].b == self.colors[3].b
    }

    /// Get the average height of this wall
    pub fn height(&self) -> f32 {
        let bottom = (self.heights[0] + self.heights[1]) / 2.0;
        let top = (self.heights[2] + self.heights[3]) / 2.0;
        top - bottom
    }

    /// Get the bottom Y (average of bottom corners)
    pub fn y_bottom(&self) -> f32 {
        (self.heights[0] + self.heights[1]) / 2.0
    }

    /// Get the top Y (average of top corners)
    pub fn y_top(&self) -> f32 {
        (self.heights[2] + self.heights[3]) / 2.0
    }

    /// Get the absolute minimum Y of any corner
    pub fn y_min(&self) -> f32 {
        self.heights.iter().cloned().fold(f32::INFINITY, f32::min)
    }

    /// Get the absolute maximum Y of any corner
    pub fn y_max(&self) -> f32 {
        self.heights.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
    }

    /// Get the left side coverage (bottom-left to top-left)
    pub fn left_coverage(&self) -> (f32, f32) {
        (self.heights[0], self.heights[3])  // bottom-left, top-left
    }

    /// Get the right side coverage (bottom-right to top-right)
    pub fn right_coverage(&self) -> (f32, f32) {
        (self.heights[1], self.heights[2])  // bottom-right, top-right
    }

    /// Check if wall has uniform heights (all bottom same, all top same)
    pub fn is_flat(&self) -> bool {
        let bottom_same = (self.heights[0] - self.heights[1]).abs() < 0.001;
        let top_same = (self.heights[2] - self.heights[3]).abs() < 0.001;
        bottom_same && top_same
    }
}

/// A single sector in the room grid
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sector {
    /// Floor face (None = no floor / pit)
    pub floor: Option<HorizontalFace>,
    /// Ceiling face (None = no ceiling / open sky)
    pub ceiling: Option<HorizontalFace>,
    /// Walls on north edge (-Z) - can have multiple stacked
    #[serde(default)]
    pub walls_north: Vec<VerticalFace>,
    /// Walls on east edge (+X)
    #[serde(default)]
    pub walls_east: Vec<VerticalFace>,
    /// Walls on south edge (+Z)
    #[serde(default)]
    pub walls_south: Vec<VerticalFace>,
    /// Walls on west edge (-X)
    #[serde(default)]
    pub walls_west: Vec<VerticalFace>,
    /// Diagonal walls from NW corner to SE corner
    #[serde(default)]
    pub walls_nwse: Vec<VerticalFace>,
    /// Diagonal walls from NE corner to SW corner
    #[serde(default)]
    pub walls_nesw: Vec<VerticalFace>,
}

impl Sector {
    /// Create an empty sector (no floor, ceiling, or walls)
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create a sector with just a floor
    pub fn with_floor(height: f32, texture: TextureRef) -> Self {
        Self {
            floor: Some(HorizontalFace::flat(height, texture)),
            ..Default::default()
        }
    }

    /// Create a sector with floor and ceiling
    pub fn with_floor_and_ceiling(floor_height: f32, ceiling_height: f32, texture: TextureRef) -> Self {
        Self {
            floor: Some(HorizontalFace::flat(floor_height, texture.clone())),
            ceiling: Some(HorizontalFace::flat(ceiling_height, texture)),
            ..Default::default()
        }
    }

    /// Check if this sector has any geometry
    pub fn has_geometry(&self) -> bool {
        self.floor.is_some()
            || self.ceiling.is_some()
            || !self.walls_north.is_empty()
            || !self.walls_east.is_empty()
            || !self.walls_south.is_empty()
            || !self.walls_west.is_empty()
            || !self.walls_nwse.is_empty()
            || !self.walls_nesw.is_empty()
    }

    /// Get all walls on a given edge (or diagonal)
    pub fn walls(&self, direction: Direction) -> &Vec<VerticalFace> {
        match direction {
            Direction::North => &self.walls_north,
            Direction::East => &self.walls_east,
            Direction::South => &self.walls_south,
            Direction::West => &self.walls_west,
            Direction::NwSe => &self.walls_nwse,
            Direction::NeSw => &self.walls_nesw,
        }
    }

    /// Get mutable walls on a given edge (or diagonal)
    pub fn walls_mut(&mut self, direction: Direction) -> &mut Vec<VerticalFace> {
        match direction {
            Direction::North => &mut self.walls_north,
            Direction::East => &mut self.walls_east,
            Direction::South => &mut self.walls_south,
            Direction::West => &mut self.walls_west,
            Direction::NwSe => &mut self.walls_nwse,
            Direction::NeSw => &mut self.walls_nesw,
        }
    }

    /// Find the highest point of all walls on an edge
    /// Returns None if no walls exist on that edge
    pub fn walls_max_height(&self, direction: Direction) -> Option<f32> {
        let walls = self.walls(direction);
        if walls.is_empty() {
            return None;
        }
        walls.iter().map(|w| w.y_top()).max_by(|a, b| a.partial_cmp(b).unwrap())
    }

    /// Find the lowest point of all walls on an edge
    /// Returns None if no walls exist on that edge
    pub fn walls_min_height(&self, direction: Direction) -> Option<f32> {
        let walls = self.walls(direction);
        if walls.is_empty() {
            return None;
        }
        walls.iter().map(|w| w.y_bottom()).min_by(|a, b| a.partial_cmp(b).unwrap())
    }

    /// Get the floor height at a specific edge (average of the two corners on that edge)
    pub fn floor_height_at_edge(&self, direction: Direction) -> Option<f32> {
        self.floor.as_ref().map(|f| {
            let (h1, h2) = f.edge_heights(direction);
            (h1 + h2) / 2.0
        })
    }

    /// Get the ceiling height at a specific edge (average of the two corners on that edge)
    pub fn ceiling_height_at_edge(&self, direction: Direction) -> Option<f32> {
        self.ceiling.as_ref().map(|c| {
            let (h1, h2) = c.edge_heights(direction);
            (h1 + h2) / 2.0
        })
    }

    /// Calculate where a new wall should be placed on this edge.
    ///
    /// Logic (max 3 walls per edge):
    /// - 0 walls: fill floor-to-ceiling with slanted heights
    /// - 1-2 walls: fill the gap at mouse_y position (or largest gap if mouse_y is None)
    /// - 3 walls: fully covered, return None
    ///
    /// `mouse_y`: Optional room-relative Y coordinate to select which gap to fill.
    ///
    /// Returns corner heights [bottom-left, bottom-right, top-right, top-left] for the new wall,
    /// or None if edge is fully covered.
    pub fn next_wall_position(&self, direction: Direction, fallback_bottom: f32, fallback_top: f32, mouse_y: Option<f32>) -> Option<[f32; 4]> {
        // Minimum gap size to consider fillable (one click = SECTOR_SIZE / 4)
        const MIN_GAP: f32 = 256.0;

        // Get individual corner heights for floor and ceiling (preserves slant)
        // edge_heights returns (left, right) when looking from INSIDE the sector,
        // but wall heights are [BL, BR, TR, TL] from the WALL's perspective (facing outward).
        // So we swap: sector's left = wall's right, sector's right = wall's left.
        let (floor_right, floor_left) = self.floor.as_ref()
            .map(|f| f.edge_heights(direction))
            .unwrap_or((fallback_bottom, fallback_bottom));
        let (ceiling_right, ceiling_left) = self.ceiling.as_ref()
            .map(|c| c.edge_heights(direction))
            .unwrap_or((fallback_top, fallback_top));

        let walls = self.walls(direction);

        if walls.len() >= 3 {
            // Max 3 walls per edge
            return None;
        }

        if walls.is_empty() {
            // No walls yet - check if floor/ceiling are sloped to offer triangular gaps
            let floor_diff = (floor_left - floor_right).abs();
            let ceiling_diff = (ceiling_left - ceiling_right).abs();

            if floor_diff > MIN_GAP || ceiling_diff > MIN_GAP {
                // Sloped floor/ceiling - offer triangular gaps based on mouse_y preference
                let floor_max = floor_left.max(floor_right);
                let ceiling_min = ceiling_left.min(ceiling_right);
                let mid_height = (floor_max + ceiling_min) / 2.0;

                if let Some(y) = mouse_y {
                    if y < mid_height {
                        // User wants bottom triangular gap
                        // Bottom goes from actual floor heights, top aligns to the higher floor
                        let top_height = floor_max;
                        return Some([floor_left, floor_right, top_height, top_height]);
                    } else {
                        // User wants top triangular gap
                        // Bottom aligns to the higher floor, top goes to actual ceiling heights
                        let bottom_height = floor_max;
                        return Some([bottom_height, bottom_height, ceiling_right, ceiling_left]);
                    }
                }
            }

            // No slope or no preference - fill from floor to ceiling, preserving slant
            // [bottom-left, bottom-right, top-right, top-left]
            return Some([floor_left, floor_right, ceiling_right, ceiling_left]);
        }

        // Sort walls by their bottom height to find gaps
        // Use AVERAGE of bottom corners to handle triangular walls correctly
        let mut sorted_walls: Vec<_> = walls.iter().collect();
        sorted_walls.sort_by(|a, b| {
            let a_bottom = (a.heights[0] + a.heights[1]) / 2.0;
            let b_bottom = (b.heights[0] + b.heights[1]) / 2.0;
            a_bottom.partial_cmp(&b_bottom).unwrap()
        });

        // Collect all gaps: (heights, bottom_y, top_y)
        let mut gaps: Vec<([f32; 4], f32, f32)> = Vec::new();

        // Check gap at bottom (floor to lowest wall)
        // For triangular gaps, check each corner separately
        let lowest = sorted_walls[0];
        let left_gap = lowest.heights[0] - floor_left;  // BL corner gap
        let right_gap = lowest.heights[1] - floor_right; // BR corner gap
        let bottom_gap_size = left_gap.max(right_gap);
        if bottom_gap_size > MIN_GAP {
            // For triangular gaps: if one side has no gap, collapse both vertices to same point
            let (bl, tl) = if left_gap > MIN_GAP {
                (floor_left, lowest.heights[0])
            } else {
                // No gap on left - collapse to floor height
                (floor_left, floor_left)
            };
            let (br, tr) = if right_gap > MIN_GAP {
                (floor_right, lowest.heights[1])
            } else {
                // No gap on right - collapse to floor height
                (floor_right, floor_right)
            };
            // Use average Y for selection purposes
            let avg_bottom = (bl + br) / 2.0;
            let avg_top = (tl + tr) / 2.0;
            gaps.push((
                [bl, br, tr, tl],
                avg_bottom,
                avg_top
            ));
        }

        // Check gaps between walls
        for i in 0..sorted_walls.len() - 1 {
            let lower = sorted_walls[i];
            let upper = sorted_walls[i + 1];
            // Gap between top of lower wall and bottom of upper wall
            // For triangular gaps, check each corner separately
            let left_gap = upper.heights[0] - lower.heights[3];  // TL of lower to BL of upper
            let right_gap = upper.heights[1] - lower.heights[2]; // TR of lower to BR of upper
            let gap_size = left_gap.max(right_gap);
            if gap_size > MIN_GAP {
                // Use average Y for selection purposes
                let avg_bottom = (lower.heights[2] + lower.heights[3]) / 2.0;
                let avg_top = (upper.heights[0] + upper.heights[1]) / 2.0;
                gaps.push((
                    [lower.heights[3], lower.heights[2], upper.heights[1], upper.heights[0]],
                    avg_bottom,
                    avg_top
                ));
            }
        }

        // Check gap at top (highest wall to ceiling)
        // For triangular gaps, check each corner separately
        let highest = sorted_walls.last().unwrap();
        let left_gap = ceiling_left - highest.heights[3];  // TL corner gap
        let right_gap = ceiling_right - highest.heights[2]; // TR corner gap
        let top_gap_size = left_gap.max(right_gap);
        if top_gap_size > MIN_GAP {
            // For triangular gaps: if one side has no gap, collapse both vertices to same point
            let (bl, tl) = if left_gap > MIN_GAP {
                (highest.heights[3], ceiling_left)
            } else {
                // No gap on left - collapse to ceiling height
                (ceiling_left, ceiling_left)
            };
            let (br, tr) = if right_gap > MIN_GAP {
                (highest.heights[2], ceiling_right)
            } else {
                // No gap on right - collapse to ceiling height
                (ceiling_right, ceiling_right)
            };
            // Use average Y for selection purposes
            let avg_bottom = (bl + br) / 2.0;
            let avg_top = (tl + tr) / 2.0;
            gaps.push((
                [bl, br, tr, tl],
                avg_bottom,
                avg_top
            ));
        }

        if gaps.is_empty() {
            return None;
        }

        // Select gap based on mouse_y position
        if let Some(y) = mouse_y {
            // Find gap containing mouse_y, or closest to it
            let best = gaps.into_iter()
                .min_by(|a, b| {
                    // Distance from mouse_y to gap center
                    let a_center = (a.1 + a.2) / 2.0;
                    let b_center = (b.1 + b.2) / 2.0;
                    let a_dist = (y - a_center).abs();
                    let b_dist = (y - b_center).abs();
                    a_dist.partial_cmp(&b_dist).unwrap()
                });
            best.map(|(heights, _, _)| heights)
        } else {
            // No mouse position - return largest gap
            gaps.into_iter()
                .max_by(|a, b| {
                    let a_size = a.2 - a.1;
                    let b_size = b.2 - b.1;
                    a_size.partial_cmp(&b_size).unwrap()
                })
                .map(|(heights, _, _)| heights)
        }
    }

    /// Calculate where a new diagonal wall should be placed.
    ///
    /// For diagonal walls, corners are:
    /// - NW-SE: corner1 = NW (index 0), corner2 = SE (index 2)
    /// - NE-SW: corner1 = NE (index 1), corner2 = SW (index 3)
    ///
    /// Returns corner heights [corner1_bot, corner2_bot, corner2_top, corner1_top] for the new wall,
    /// or None if the diagonal is fully covered.
    pub fn next_diagonal_wall_position(&self, is_nwse: bool, fallback_bottom: f32, fallback_top: f32, mouse_y: Option<f32>) -> Option<[f32; 4]> {
        const MIN_GAP: f32 = 256.0;

        // Get floor/ceiling heights at the diagonal corners
        let (corner1_idx, corner2_idx) = if is_nwse {
            (0, 2) // NW, SE
        } else {
            (1, 3) // NE, SW
        };

        let floor_c1 = self.floor.as_ref().map(|f| f.heights[corner1_idx]).unwrap_or(fallback_bottom);
        let floor_c2 = self.floor.as_ref().map(|f| f.heights[corner2_idx]).unwrap_or(fallback_bottom);
        let ceiling_c1 = self.ceiling.as_ref().map(|c| c.heights[corner1_idx]).unwrap_or(fallback_top);
        let ceiling_c2 = self.ceiling.as_ref().map(|c| c.heights[corner2_idx]).unwrap_or(fallback_top);

        let walls = if is_nwse { &self.walls_nwse } else { &self.walls_nesw };

        if walls.len() >= 3 {
            return None;
        }

        if walls.is_empty() {
            // No walls yet - check if floor/ceiling are sloped to offer triangular gaps
            let floor_diff = (floor_c1 - floor_c2).abs();
            let ceiling_diff = (ceiling_c1 - ceiling_c2).abs();

            if floor_diff > MIN_GAP || ceiling_diff > MIN_GAP {
                // Sloped floor/ceiling - offer triangular gaps based on mouse_y preference
                // Calculate the "midpoint" height where floor and ceiling would intersect
                // if extended (or just use average for simpler selection)
                let floor_max = floor_c1.max(floor_c2);
                let ceiling_min = ceiling_c1.min(ceiling_c2);
                let mid_height = (floor_max + ceiling_min) / 2.0;

                if let Some(y) = mouse_y {
                    if y < mid_height {
                        // User wants bottom triangular gap
                        // Bottom goes from actual floor heights, top aligns to the higher floor
                        let top_height = floor_max;
                        return Some([floor_c1, floor_c2, top_height, top_height]);
                    } else {
                        // User wants top triangular gap
                        // Bottom aligns to the higher floor, top goes to actual ceiling heights
                        let bottom_height = floor_max;
                        return Some([bottom_height, bottom_height, ceiling_c2, ceiling_c1]);
                    }
                }
            }

            // No slope or no preference - fill from floor to ceiling
            // [corner1_bot, corner2_bot, corner2_top, corner1_top]
            return Some([floor_c1, floor_c2, ceiling_c2, ceiling_c1]);
        }

        // Sort walls by bottom height
        // Use AVERAGE of bottom corners to handle triangular walls correctly
        let mut sorted_walls: Vec<_> = walls.iter().collect();
        sorted_walls.sort_by(|a, b| {
            let a_bottom = (a.heights[0] + a.heights[1]) / 2.0;
            let b_bottom = (b.heights[0] + b.heights[1]) / 2.0;
            a_bottom.partial_cmp(&b_bottom).unwrap()
        });

        // Collect gaps: (heights, bottom_y, top_y)
        // For triangular gaps, check each corner separately (same as axis-aligned walls)
        let mut gaps: Vec<([f32; 4], f32, f32)> = Vec::new();

        // Gap at bottom (floor to lowest wall)
        // Check each corner separately for triangular gap support
        let lowest = sorted_walls[0];
        let c1_gap = lowest.heights[0] - floor_c1;  // Corner 1 gap (bottom of wall to floor)
        let c2_gap = lowest.heights[1] - floor_c2;  // Corner 2 gap
        let bottom_gap_size = c1_gap.max(c2_gap);
        if bottom_gap_size > MIN_GAP {
            // For triangular gaps: if one side has no gap, collapse vertices
            let (c1_bot, c1_top) = if c1_gap > MIN_GAP {
                (floor_c1, lowest.heights[0])
            } else {
                (floor_c1, floor_c1)  // Collapse to floor
            };
            let (c2_bot, c2_top) = if c2_gap > MIN_GAP {
                (floor_c2, lowest.heights[1])
            } else {
                (floor_c2, floor_c2)  // Collapse to floor
            };
            let avg_bottom = (c1_bot + c2_bot) / 2.0;
            let avg_top = (c1_top + c2_top) / 2.0;
            gaps.push((
                [c1_bot, c2_bot, c2_top, c1_top],
                avg_bottom,
                avg_top
            ));
        }

        // Gaps between walls
        for i in 0..sorted_walls.len() - 1 {
            let lower = sorted_walls[i];
            let upper = sorted_walls[i + 1];
            // Check each corner separately
            let c1_gap = upper.heights[0] - lower.heights[3];  // Corner 1: top of lower to bottom of upper
            let c2_gap = upper.heights[1] - lower.heights[2];  // Corner 2
            let gap_size = c1_gap.max(c2_gap);
            if gap_size > MIN_GAP {
                let avg_bottom = (lower.heights[2] + lower.heights[3]) / 2.0;
                let avg_top = (upper.heights[0] + upper.heights[1]) / 2.0;
                gaps.push((
                    [lower.heights[3], lower.heights[2], upper.heights[1], upper.heights[0]],
                    avg_bottom,
                    avg_top
                ));
            }
        }

        // Gap at top (highest wall to ceiling)
        // Check each corner separately for triangular gap support
        let highest = sorted_walls.last().unwrap();
        let c1_gap = ceiling_c1 - highest.heights[3];  // Corner 1 gap (ceiling to top of wall)
        let c2_gap = ceiling_c2 - highest.heights[2];  // Corner 2 gap
        let top_gap_size = c1_gap.max(c2_gap);
        if top_gap_size > MIN_GAP {
            // For triangular gaps: if one side has no gap, collapse vertices
            let (c1_bot, c1_top) = if c1_gap > MIN_GAP {
                (highest.heights[3], ceiling_c1)
            } else {
                (ceiling_c1, ceiling_c1)  // Collapse to ceiling
            };
            let (c2_bot, c2_top) = if c2_gap > MIN_GAP {
                (highest.heights[2], ceiling_c2)
            } else {
                (ceiling_c2, ceiling_c2)  // Collapse to ceiling
            };
            let avg_bottom = (c1_bot + c2_bot) / 2.0;
            let avg_top = (c1_top + c2_top) / 2.0;
            gaps.push((
                [c1_bot, c2_bot, c2_top, c1_top],
                avg_bottom,
                avg_top
            ));
        }

        if gaps.is_empty() {
            return None;
        }

        // Select gap based on mouse_y
        if let Some(y) = mouse_y {
            gaps.into_iter()
                .min_by(|a, b| {
                    let a_center = (a.1 + a.2) / 2.0;
                    let b_center = (b.1 + b.2) / 2.0;
                    (y - a_center).abs().partial_cmp(&(y - b_center).abs()).unwrap()
                })
                .map(|(heights, _, _)| heights)
        } else {
            gaps.into_iter()
                .max_by(|a, b| (a.2 - a.1).partial_cmp(&(b.2 - b.1)).unwrap())
                .map(|(heights, _, _)| heights)
        }
    }

    /// Extrude the floor upward by `amount` units.
    /// Creates walls around the perimeter connecting the old floor height to the new height.
    /// Returns true if extrusion was performed, false if no floor exists.
    pub fn extrude_floor(&mut self, amount: f32, wall_texture: TextureRef) -> bool {
        let Some(floor) = &mut self.floor else {
            return false;
        };

        // Store old heights before modifying
        let old_heights = floor.heights;

        // Raise all floor corners by the extrusion amount
        for h in &mut floor.heights {
            *h += amount;
        }
        let new_heights = floor.heights;

        // For each edge: if there's already a wall, extend it; otherwise create a new one
        // Wall heights: [bottom-left, bottom-right, top-right, top-left]
        // For extrusion walls facing OUTWARD, we use FaceNormalMode::Back

        // North wall (-Z edge): BL at west (NW), BR at east (NE)
        if let Some(wall) = self.walls_north.last_mut() {
            // Raise existing wall's bottom to new floor height
            wall.heights[0] = new_heights[0];  // BL = NW
            wall.heights[1] = new_heights[1];  // BR = NE
        } else {
            let mut north_wall = VerticalFace::new_sloped(
                old_heights[0], old_heights[1],  // bottom: BL=NW, BR=NE
                new_heights[1], new_heights[0],  // top: TR=NE, TL=NW
                wall_texture.clone(),
            );
            north_wall.normal_mode = FaceNormalMode::Back;
            self.walls_north.push(north_wall);
        }

        // East wall (+X edge): BL at north (NE), BR at south (SE)
        if let Some(wall) = self.walls_east.last_mut() {
            wall.heights[0] = new_heights[1];  // BL = NE
            wall.heights[1] = new_heights[2];  // BR = SE
        } else {
            let mut east_wall = VerticalFace::new_sloped(
                old_heights[1], old_heights[2],  // bottom: BL=NE, BR=SE
                new_heights[2], new_heights[1],  // top: TR=SE, TL=NE
                wall_texture.clone(),
            );
            east_wall.normal_mode = FaceNormalMode::Back;
            self.walls_east.push(east_wall);
        }

        // South wall (+Z edge): BL at east (SE), BR at west (SW)
        if let Some(wall) = self.walls_south.last_mut() {
            wall.heights[0] = new_heights[2];  // BL = SE
            wall.heights[1] = new_heights[3];  // BR = SW
        } else {
            let mut south_wall = VerticalFace::new_sloped(
                old_heights[2], old_heights[3],  // bottom: BL=SE, BR=SW
                new_heights[3], new_heights[2],  // top: TR=SW, TL=SE
                wall_texture.clone(),
            );
            south_wall.normal_mode = FaceNormalMode::Back;
            self.walls_south.push(south_wall);
        }

        // West wall (-X edge): BL at south (SW), BR at north (NW)
        if let Some(wall) = self.walls_west.last_mut() {
            wall.heights[0] = new_heights[3];  // BL = SW
            wall.heights[1] = new_heights[0];  // BR = NW
        } else {
            let mut west_wall = VerticalFace::new_sloped(
                old_heights[3], old_heights[0],  // bottom: BL=SW, BR=NW
                new_heights[0], new_heights[3],  // top: TR=NW, TL=SW
                wall_texture,
            );
            west_wall.normal_mode = FaceNormalMode::Back;
            self.walls_west.push(west_wall);
        }

        true
    }
}

/// Cardinal and diagonal directions for walls
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Direction {
    #[default]
    North,  // -Z
    East,   // +X
    South,  // +Z
    West,   // -X
    NwSe,   // Diagonal from NW to SE corner
    NeSw,   // Diagonal from NE to SW corner
}

impl Direction {
    /// Get the opposite direction
    pub fn opposite(self) -> Self {
        match self {
            Direction::North => Direction::South,
            Direction::East => Direction::West,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
            Direction::NwSe => Direction::NwSe, // Diagonals are their own opposite
            Direction::NeSw => Direction::NeSw,
        }
    }

    /// Get offset in grid coordinates (only for cardinal directions)
    pub fn offset(self) -> (i32, i32) {
        match self {
            Direction::North => (0, -1),
            Direction::East => (1, 0),
            Direction::South => (0, 1),
            Direction::West => (-1, 0),
            Direction::NwSe | Direction::NeSw => (0, 0), // No offset for diagonals
        }
    }

    /// Check if this is a diagonal direction
    pub fn is_diagonal(self) -> bool {
        matches!(self, Direction::NwSe | Direction::NeSw)
    }

    /// Rotate clockwise through all 6 directions: N -> E -> S -> W -> NwSe -> NeSw -> N
    pub fn rotate_cw(self) -> Self {
        match self {
            Direction::North => Direction::East,
            Direction::East => Direction::South,
            Direction::South => Direction::West,
            Direction::West => Direction::NwSe,
            Direction::NwSe => Direction::NeSw,
            Direction::NeSw => Direction::North,
        }
    }

    /// Get display name for status messages
    pub fn name(self) -> &'static str {
        match self {
            Direction::North => "North",
            Direction::East => "East",
            Direction::South => "South",
            Direction::West => "West",
            Direction::NwSe => "Diagonal NW-SE",
            Direction::NeSw => "Diagonal NE-SW",
        }
    }
}

/// Axis-aligned bounding box
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Check if a point is inside the box
    pub fn contains(&self, point: Vec3) -> bool {
        point.x >= self.min.x && point.x <= self.max.x
            && point.y >= self.min.y && point.y <= self.max.y
            && point.z >= self.min.z && point.z <= self.max.z
    }

    /// Expand bounds to include a point
    pub fn expand(&mut self, point: Vec3) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.min.z = self.min.z.min(point.z);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
        self.max.z = self.max.z.max(point.z);
    }

    /// Get center of the box
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }
}

/// Types of spawn points in the level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpawnPointType {
    /// Player start position
    PlayerStart,
    /// Checkpoint / bonfire / save point
    Checkpoint,
    /// Enemy spawn location
    Enemy,
    /// Item pickup location
    Item,
}

/// Player settings for the level (TR-style character controller parameters)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerSettings {
    /// Collision cylinder radius
    pub radius: f32,
    /// Character height (collision cylinder)
    pub height: f32,
    /// Maximum step-up height
    pub step_height: f32,
    /// Walk speed (units per second)
    pub walk_speed: f32,
    /// Run speed (units per second)
    pub run_speed: f32,
    /// Gravity acceleration (units per second squared)
    pub gravity: f32,
    /// Jump velocity (initial upward velocity when jumping)
    pub jump_velocity: f32,
    /// Sprint jump velocity multiplier (1.0 = same as normal, 1.2 = 20% higher)
    pub sprint_jump_multiplier: f32,
    /// Camera distance from player (orbit radius)
    pub camera_distance: f32,
    /// Camera vertical offset above player feet (look-at target height)
    pub camera_vertical_offset: f32,
    /// Minimum camera pitch (looking up, radians, negative = up)
    pub camera_pitch_min: f32,
    /// Maximum camera pitch (looking down, radians, positive = down)
    pub camera_pitch_max: f32,
    /// Camera height offset (legacy, kept for compatibility)
    pub camera_height: f32,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            radius: 100.0,
            height: 762.0,
            step_height: 384.0,
            walk_speed: 800.0,
            run_speed: 1600.0,
            gravity: 2400.0,
            jump_velocity: 1200.0,          // Initial upward velocity for jump
            sprint_jump_multiplier: 1.15,   // 15% higher jump when sprinting
            camera_distance: 800.0,
            camera_vertical_offset: 500.0,  // Shoulder/upper chest height
            camera_pitch_min: -0.8,         // Can look up ~45 degrees
            camera_pitch_max: 0.8,          // Can look down ~45 degrees
            camera_height: 610.0,           // Legacy, kept for compatibility
        }
    }
}

// ============================================================================
// Unified Tile-Based Object System (TR-style)
// ============================================================================

/// Object types that can be placed on tiles
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObjectType {
    /// Spawn point (player start, checkpoint, enemy, item)
    Spawn(SpawnPointType),
    /// Point light source
    Light {
        /// Light color (RGB)
        color: Color,
        /// Light intensity (0.0-2.0+)
        intensity: f32,
        /// Falloff radius in world units
        radius: f32,
    },
    /// Static prop/decoration (references model by name)
    Prop(String),
    /// Trigger zone (for scripting)
    Trigger {
        /// Trigger identifier for scripting
        trigger_id: String,
        /// Trigger type (e.g., "on_enter", "on_leave", "on_use")
        trigger_type: String,
    },
    /// Particle emitter
    Particle {
        /// Particle effect name
        effect: String,
    },
    /// Audio source (ambient sound)
    Audio {
        /// Sound asset name
        sound: String,
        /// Volume (0.0-1.0)
        volume: f32,
        /// Radius for 3D falloff
        radius: f32,
        /// Loop the sound?
        looping: bool,
    },
}

impl ObjectType {
    /// Get a display name for the object type
    pub fn display_name(&self) -> &'static str {
        match self {
            ObjectType::Spawn(SpawnPointType::PlayerStart) => "Player Start",
            ObjectType::Spawn(SpawnPointType::Checkpoint) => "Checkpoint",
            ObjectType::Spawn(SpawnPointType::Enemy) => "Enemy Spawn",
            ObjectType::Spawn(SpawnPointType::Item) => "Item Spawn",
            ObjectType::Light { .. } => "Light",
            ObjectType::Prop(_) => "Prop",
            ObjectType::Trigger { .. } => "Trigger",
            ObjectType::Particle { .. } => "Particle",
            ObjectType::Audio { .. } => "Audio",
        }
    }

    /// Check if this object type is unique per tile (only one allowed)
    pub fn is_unique_per_tile(&self) -> bool {
        matches!(self,
            ObjectType::Spawn(SpawnPointType::PlayerStart) |
            ObjectType::Spawn(SpawnPointType::Checkpoint) |
            ObjectType::Light { .. }
        )
    }

    /// Check if this object type is unique per level (only one in entire level)
    pub fn is_unique_per_level(&self) -> bool {
        matches!(self, ObjectType::Spawn(SpawnPointType::PlayerStart))
    }
}

/// A tile-based object placed in a room
///
/// Objects are tied to sectors (tiles) using grid coordinates within the room.
/// Height offset allows vertical positioning within the sector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelObject {
    /// Sector X coordinate within the room
    pub sector_x: usize,
    /// Sector Z coordinate within the room
    pub sector_z: usize,
    /// Height offset from sector floor (in world units)
    #[serde(default)]
    pub height: f32,
    /// Facing direction (yaw angle in radians, 0 = +Z)
    #[serde(default)]
    pub facing: f32,
    /// The object type and its specific properties
    pub object_type: ObjectType,
    /// Optional name/identifier
    #[serde(default)]
    pub name: String,
    /// Is this object active/enabled?
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl LevelObject {
    /// Create a new object at a sector position
    pub fn new(sector_x: usize, sector_z: usize, object_type: ObjectType) -> Self {
        Self {
            sector_x,
            sector_z,
            height: 0.0,
            facing: 0.0,
            object_type,
            name: String::new(),
            enabled: true,
        }
    }

    /// Create a player start object
    pub fn player_start(sector_x: usize, sector_z: usize) -> Self {
        Self::new(sector_x, sector_z, ObjectType::Spawn(SpawnPointType::PlayerStart))
    }

    /// Create a light object
    pub fn light(sector_x: usize, sector_z: usize, color: Color, intensity: f32, radius: f32) -> Self {
        Self::new(sector_x, sector_z, ObjectType::Light { color, intensity, radius })
    }

    /// Create a prop object
    pub fn prop(sector_x: usize, sector_z: usize, model_name: impl Into<String>) -> Self {
        Self::new(sector_x, sector_z, ObjectType::Prop(model_name.into()))
    }

    /// Set height offset
    pub fn with_height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Set facing direction
    pub fn with_facing(mut self, facing: f32) -> Self {
        self.facing = facing;
        self
    }

    /// Set name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Calculate world-space position of this object
    /// Requires the room to calculate the sector's floor height
    pub fn world_position(&self, room: &Room) -> Vec3 {
        let base_x = room.position.x + (self.sector_x as f32) * SECTOR_SIZE + SECTOR_SIZE * 0.5;
        let base_z = room.position.z + (self.sector_z as f32) * SECTOR_SIZE + SECTOR_SIZE * 0.5;

        // Get floor height at this sector (average if sloped)
        let base_y = room.get_sector(self.sector_x, self.sector_z)
            .and_then(|s| s.floor.as_ref())
            .map(|f| f.avg_height())
            .unwrap_or(room.position.y);

        Vec3::new(base_x, base_y + self.height, base_z)
    }

    /// Check if this object is a spawn point
    pub fn is_spawn(&self) -> bool {
        matches!(self.object_type, ObjectType::Spawn(_))
    }

    /// Check if this object is a light
    pub fn is_light(&self) -> bool {
        matches!(self.object_type, ObjectType::Light { .. })
    }

    /// Get spawn type if this is a spawn object
    pub fn spawn_type(&self) -> Option<SpawnPointType> {
        match &self.object_type {
            ObjectType::Spawn(t) => Some(*t),
            _ => None,
        }
    }
}

/// Portal connecting two rooms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portal {
    /// Target room ID
    pub target_room: usize,
    /// Portal corners in room-relative coordinates (4 vertices)
    pub vertices: [Vec3; 4],
    /// Portal facing direction (points into the room)
    pub normal: Vec3,
}

impl Portal {
    pub fn new(target_room: usize, vertices: [Vec3; 4], normal: Vec3) -> Self {
        Self {
            target_room,
            vertices,
            normal: normal.normalize(),
        }
    }

    /// Get portal center
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.vertices[0].x + self.vertices[1].x + self.vertices[2].x + self.vertices[3].x) * 0.25,
            (self.vertices[0].y + self.vertices[1].y + self.vertices[2].y + self.vertices[3].y) * 0.25,
            (self.vertices[0].z + self.vertices[1].z + self.vertices[2].z + self.vertices[3].z) * 0.25,
        )
    }
}

/// A room in the level - contains a 2D grid of sectors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    /// Unique room identifier
    pub id: usize,
    /// Room position in world space (origin of sector grid)
    pub position: Vec3,
    /// Grid width (number of sectors in X direction)
    pub width: usize,
    /// Grid depth (number of sectors in Z direction)
    pub depth: usize,
    /// 2D array of sectors [x][z], None = no sector at this position
    pub sectors: Vec<Vec<Option<Sector>>>,
    /// Portals to adjacent rooms
    #[serde(default)]
    pub portals: Vec<Portal>,
    /// Bounding box (room-relative) - computed from sectors, not serialized
    #[serde(skip)]
    pub bounds: Aabb,
    /// Ambient light level (0.0 = dark, 1.0 = bright)
    #[serde(default = "default_ambient")]
    pub ambient: f32,
    /// Tile-based objects in this room (spawns, lights, props, triggers, etc.)
    #[serde(default)]
    pub objects: Vec<LevelObject>,
}

fn default_ambient() -> f32 {
    0.5
}

impl Room {
    /// Create a new empty room with the given grid size
    pub fn new(id: usize, position: Vec3, width: usize, depth: usize) -> Self {
        // Initialize 2D grid with None
        let sectors = (0..width)
            .map(|_| (0..depth).map(|_| None).collect())
            .collect();

        Self {
            id,
            position,
            width,
            depth,
            sectors,
            portals: Vec::new(),
            bounds: Aabb::default(),
            ambient: 0.5,
            objects: Vec::new(),
        }
    }

    /// Get sector at grid position (returns None if out of bounds or empty)
    pub fn get_sector(&self, x: usize, z: usize) -> Option<&Sector> {
        self.sectors.get(x)?.get(z)?.as_ref()
    }

    /// Get mutable sector at grid position
    pub fn get_sector_mut(&mut self, x: usize, z: usize) -> Option<&mut Sector> {
        self.sectors.get_mut(x)?.get_mut(z)?.as_mut()
    }

    /// Set sector at grid position (creates if doesn't exist)
    pub fn set_sector(&mut self, x: usize, z: usize, sector: Sector) {
        if x < self.width && z < self.depth {
            self.sectors[x][z] = Some(sector);
        }
    }

    /// Remove sector at grid position
    pub fn remove_sector(&mut self, x: usize, z: usize) {
        if x < self.width && z < self.depth {
            self.sectors[x][z] = None;
        }
    }

    /// Ensure sector exists at position, creating empty one if needed
    pub fn ensure_sector(&mut self, x: usize, z: usize) -> &mut Sector {
        if x < self.width && z < self.depth {
            if self.sectors[x][z].is_none() {
                self.sectors[x][z] = Some(Sector::empty());
            }
            self.sectors[x][z].as_mut().unwrap()
        } else {
            panic!("Sector position ({}, {}) out of bounds", x, z);
        }
    }

    /// Set floor at grid position
    pub fn set_floor(&mut self, x: usize, z: usize, height: f32, texture: TextureRef) {
        let sector = self.ensure_sector(x, z);
        sector.floor = Some(HorizontalFace::flat(height, texture));
    }

    /// Set ceiling at grid position
    pub fn set_ceiling(&mut self, x: usize, z: usize, height: f32, texture: TextureRef) {
        let sector = self.ensure_sector(x, z);
        sector.ceiling = Some(HorizontalFace::flat(height, texture));
    }

    /// Add a wall on a sector edge
    pub fn add_wall(&mut self, x: usize, z: usize, direction: Direction, y_bottom: f32, y_top: f32, texture: TextureRef) {
        let sector = self.ensure_sector(x, z);
        sector.walls_mut(direction).push(VerticalFace::new(y_bottom, y_top, texture));
    }

    /// Add a portal to another room
    pub fn add_portal(&mut self, target_room: usize, vertices: [Vec3; 4], normal: Vec3) {
        self.portals.push(Portal::new(target_room, vertices, normal));
    }

    /// Convert world position to grid coordinates
    pub fn world_to_grid(&self, world_x: f32, world_z: f32) -> Option<(usize, usize)> {
        let local_x = world_x - self.position.x;
        let local_z = world_z - self.position.z;

        if local_x < 0.0 || local_z < 0.0 {
            return None;
        }

        let grid_x = (local_x / SECTOR_SIZE) as usize;
        let grid_z = (local_z / SECTOR_SIZE) as usize;

        if grid_x < self.width && grid_z < self.depth {
            Some((grid_x, grid_z))
        } else {
            None
        }
    }

    /// Convert grid coordinates to world position (returns corner of sector)
    pub fn grid_to_world(&self, x: usize, z: usize) -> Vec3 {
        Vec3::new(
            self.position.x + (x as f32) * SECTOR_SIZE,
            self.position.y,
            self.position.z + (z as f32) * SECTOR_SIZE,
        )
    }

    /// Recalculate bounds from sectors (call after loading from file)
    pub fn recalculate_bounds(&mut self) {
        self.bounds = Aabb::new(
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        );

        for x in 0..self.width {
            for z in 0..self.depth {
                if let Some(sector) = &self.sectors[x][z] {
                    let base_x = (x as f32) * SECTOR_SIZE;
                    let base_z = (z as f32) * SECTOR_SIZE;

                    // Expand bounds for floor corners
                    if let Some(floor) = &sector.floor {
                        for (i, &h) in floor.heights.iter().enumerate() {
                            let (dx, dz) = match i {
                                0 => (0.0, 0.0),           // NW
                                1 => (SECTOR_SIZE, 0.0),   // NE
                                2 => (SECTOR_SIZE, SECTOR_SIZE), // SE
                                3 => (0.0, SECTOR_SIZE),   // SW
                                _ => unreachable!(),
                            };
                            self.bounds.expand(Vec3::new(base_x + dx, h, base_z + dz));
                        }
                    }

                    // Expand bounds for ceiling corners
                    if let Some(ceiling) = &sector.ceiling {
                        for (i, &h) in ceiling.heights.iter().enumerate() {
                            let (dx, dz) = match i {
                                0 => (0.0, 0.0),
                                1 => (SECTOR_SIZE, 0.0),
                                2 => (SECTOR_SIZE, SECTOR_SIZE),
                                3 => (0.0, SECTOR_SIZE),
                                _ => unreachable!(),
                            };
                            self.bounds.expand(Vec3::new(base_x + dx, h, base_z + dz));
                        }
                    }

                    // Expand bounds for wall corners (walls can extend beyond floor/ceiling)
                    for wall in &sector.walls_north {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x, h, base_z));
                        }
                    }
                    for wall in &sector.walls_east {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x + SECTOR_SIZE, h, base_z));
                        }
                    }
                    for wall in &sector.walls_south {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x, h, base_z + SECTOR_SIZE));
                        }
                    }
                    for wall in &sector.walls_west {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x, h, base_z));
                        }
                    }
                    // Diagonal walls go corner-to-corner, so expand for both corners
                    for wall in &sector.walls_nwse {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x, h, base_z)); // NW corner
                            self.bounds.expand(Vec3::new(base_x + SECTOR_SIZE, h, base_z + SECTOR_SIZE)); // SE corner
                        }
                    }
                    for wall in &sector.walls_nesw {
                        for &h in &wall.heights {
                            self.bounds.expand(Vec3::new(base_x + SECTOR_SIZE, h, base_z)); // NE corner
                            self.bounds.expand(Vec3::new(base_x, h, base_z + SECTOR_SIZE)); // SW corner
                        }
                    }
                }
            }
        }
    }

    /// Remove sectors that have no geometry (no floor, ceiling, or walls).
    /// Call this after deleting faces to clean up orphaned sectors.
    pub fn cleanup_empty_sectors(&mut self) {
        for x in 0..self.width {
            for z in 0..self.depth {
                if let Some(sector) = &self.sectors[x][z] {
                    if !sector.has_geometry() {
                        self.sectors[x][z] = None;
                    }
                }
            }
        }
    }

    /// Trim empty rows and columns from the edges of the room grid.
    /// Adjusts room position to keep sectors in the same world position.
    pub fn trim_empty_edges(&mut self) {
        if self.sectors.is_empty() || self.width == 0 || self.depth == 0 {
            return;
        }

        // Find first non-empty column (from left)
        let mut first_col = 0;
        while first_col < self.width {
            let col_has_sector = (0..self.depth).any(|z| self.sectors[first_col][z].is_some());
            if col_has_sector {
                break;
            }
            first_col += 1;
        }

        // Find last non-empty column (from right)
        let mut last_col = self.width;
        while last_col > first_col {
            let col_has_sector = (0..self.depth).any(|z| self.sectors[last_col - 1][z].is_some());
            if col_has_sector {
                break;
            }
            last_col -= 1;
        }

        // Find first non-empty row (from front)
        let mut first_row = 0;
        while first_row < self.depth {
            let row_has_sector = (first_col..last_col).any(|x| self.sectors[x][first_row].is_some());
            if row_has_sector {
                break;
            }
            first_row += 1;
        }

        // Find last non-empty row (from back)
        let mut last_row = self.depth;
        while last_row > first_row {
            let row_has_sector = (first_col..last_col).any(|x| self.sectors[x][last_row - 1].is_some());
            if row_has_sector {
                break;
            }
            last_row -= 1;
        }

        // If grid is completely empty, leave at least 1x1
        if first_col >= last_col || first_row >= last_row {
            self.width = 1;
            self.depth = 1;
            self.sectors = vec![vec![None]];
            return;
        }

        // Apply trimming if needed
        if first_col > 0 || first_row > 0 || last_col < self.width || last_row < self.depth {
            // Adjust room position for removed columns/rows at the start
            self.position.x += (first_col as f32) * SECTOR_SIZE;
            self.position.z += (first_row as f32) * SECTOR_SIZE;

            // Adjust object sector coordinates to account for trimmed rows/columns
            // Objects need their sector_x/sector_z reduced by the trimmed amount
            // and any objects outside the new bounds should be removed
            let new_width = last_col - first_col;
            let new_depth = last_row - first_row;

            self.objects.retain_mut(|obj| {
                // Check if object is within the kept portion
                if obj.sector_x >= first_col && obj.sector_x < last_col
                    && obj.sector_z >= first_row && obj.sector_z < last_row
                {
                    // Adjust coordinates relative to new grid origin
                    obj.sector_x -= first_col;
                    obj.sector_z -= first_row;
                    true
                } else {
                    // Object is in a trimmed area - remove it
                    false
                }
            });

            // Extract the trimmed portion
            let mut new_sectors = Vec::with_capacity(new_width);

            for x in first_col..last_col {
                let mut col = Vec::with_capacity(new_depth);
                for z in first_row..last_row {
                    col.push(self.sectors[x][z].take());
                }
                new_sectors.push(col);
            }

            self.sectors = new_sectors;
            self.width = new_width;
            self.depth = new_depth;
        }
    }

    /// Check if a world-space point is inside this room's bounds
    pub fn contains_point(&self, point: Vec3) -> bool {
        let relative = Vec3::new(
            point.x - self.position.x,
            point.y - self.position.y,
            point.z - self.position.z,
        );
        self.bounds.contains(relative)
    }

    /// Get world-space bounds
    pub fn world_bounds(&self) -> Aabb {
        Aabb::new(
            Vec3::new(
                self.bounds.min.x + self.position.x,
                self.bounds.min.y + self.position.y,
                self.bounds.min.z + self.position.z,
            ),
            Vec3::new(
                self.bounds.max.x + self.position.x,
                self.bounds.max.y + self.position.y,
                self.bounds.max.z + self.position.z,
            ),
        )
    }

    /// Iterate over all sectors with their grid coordinates
    pub fn iter_sectors(&self) -> impl Iterator<Item = (usize, usize, &Sector)> {
        self.sectors.iter().enumerate().flat_map(|(x, col)| {
            col.iter().enumerate().filter_map(move |(z, sector)| {
                sector.as_ref().map(|s| (x, z, s))
            })
        })
    }

    /// Convert room geometry to rasterizer format (vertices + faces)
    /// Returns world-space vertices ready for rendering
    pub fn to_render_data_with_textures<F>(&self, resolve_texture: F) -> (Vec<Vertex>, Vec<RasterFace>)
    where
        F: Fn(&TextureRef) -> Option<usize>,
    {
        let mut vertices = Vec::new();
        let mut faces = Vec::new();

        for (grid_x, grid_z, sector) in self.iter_sectors() {
            let base_x = self.position.x + (grid_x as f32) * SECTOR_SIZE;
            let base_z = self.position.z + (grid_z as f32) * SECTOR_SIZE;

            // Render floor
            if let Some(floor) = &sector.floor {
                self.add_horizontal_face_to_render_data(
                    &mut vertices,
                    &mut faces,
                    floor,
                    base_x,
                    base_z,
                    grid_x,
                    grid_z,
                    true, // is_floor
                    &resolve_texture,
                );
            }

            // Render ceiling
            if let Some(ceiling) = &sector.ceiling {
                self.add_horizontal_face_to_render_data(
                    &mut vertices,
                    &mut faces,
                    ceiling,
                    base_x,
                    base_z,
                    grid_x,
                    grid_z,
                    false, // is_ceiling
                    &resolve_texture,
                );
            }

            // Render walls on each edge
            for wall in &sector.walls_north {
                self.add_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, grid_x, grid_z, Direction::North, &resolve_texture);
            }
            for wall in &sector.walls_east {
                self.add_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, grid_x, grid_z, Direction::East, &resolve_texture);
            }
            for wall in &sector.walls_south {
                self.add_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, grid_x, grid_z, Direction::South, &resolve_texture);
            }
            for wall in &sector.walls_west {
                self.add_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, grid_x, grid_z, Direction::West, &resolve_texture);
            }
            // Diagonal walls
            for wall in &sector.walls_nwse {
                self.add_diagonal_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, grid_x, grid_z, true, &resolve_texture);
            }
            for wall in &sector.walls_nesw {
                self.add_diagonal_wall_to_render_data(&mut vertices, &mut faces, wall, base_x, base_z, grid_x, grid_z, false, &resolve_texture);
            }
        }

        (vertices, faces)
    }

    /// Helper to add a horizontal face (floor or ceiling) to render data
    fn add_horizontal_face_to_render_data<F>(
        &self,
        vertices: &mut Vec<Vertex>,
        faces: &mut Vec<RasterFace>,
        face: &HorizontalFace,
        base_x: f32,
        base_z: f32,
        grid_x: usize,
        grid_z: usize,
        is_floor: bool,
        resolve_texture: &F,
    )
    where
        F: Fn(&TextureRef) -> Option<usize>,
    {
        // Corner positions: NW, NE, SE, SW
        // Heights are room-relative, so add room.position.y for world-space rendering
        let corners = [
            Vec3::new(base_x, self.position.y + face.heights[0], base_z),                         // NW
            Vec3::new(base_x + SECTOR_SIZE, self.position.y + face.heights[1], base_z),           // NE
            Vec3::new(base_x + SECTOR_SIZE, self.position.y + face.heights[2], base_z + SECTOR_SIZE), // SE
            Vec3::new(base_x, self.position.y + face.heights[3], base_z + SECTOR_SIZE),           // SW
        ];

        // Default UVs for triangle 1 (scaled by UV_SCALE, offset by grid position for tiling)
        // When face.uv is None, use world-aligned UVs based on grid position
        let uvs_1 = face.uv.unwrap_or_else(|| {
            let u_offset = (grid_x as f32) * UV_SCALE;
            let v_offset = (grid_z as f32) * UV_SCALE;
            [
                Vec2::new(u_offset, v_offset),                         // NW
                Vec2::new(u_offset + UV_SCALE, v_offset),              // NE
                Vec2::new(u_offset + UV_SCALE, v_offset + UV_SCALE),   // SE
                Vec2::new(u_offset, v_offset + UV_SCALE),              // SW
            ]
        });

        // UVs for triangle 2 (use override or fall back to triangle 1's UVs)
        let uvs_2 = face.get_uv_2().copied().unwrap_or(uvs_1);

        // Colors for each triangle
        let colors_1 = &face.colors;
        let colors_2 = face.get_colors_2();

        // Texture IDs for each triangle
        let texture_id_1 = resolve_texture(&face.texture).unwrap_or(0);
        let texture_id_2 = resolve_texture(face.get_texture_2()).unwrap_or(0);

        // Handle normal mode: Front, Back, or Both
        let render_front = face.normal_mode != FaceNormalMode::Back;
        let render_back = face.normal_mode != FaceNormalMode::Front;

        // Get corner indices for each triangle based on split direction
        let split = face.split_direction;
        let tri1_corners = split.triangle_1_corners();
        let tri2_corners = split.triangle_2_corners();

        // Calculate normal from cross product (using first triangle's edges)
        let edge1 = corners[1] - corners[0]; // NW -> NE (along +X)
        let edge2 = corners[3] - corners[0]; // NW -> SW (along +Z)
        let front_normal = if is_floor {
            edge2.cross(edge1).normalize() // +Z x +X = +Y (up)
        } else {
            edge1.cross(edge2).normalize() // +X x +Z = -Y (down)
        };
        let back_normal = front_normal.scale(-1.0);

        // Helper to add a single triangle
        let add_triangle = |vertices: &mut Vec<Vertex>, faces: &mut Vec<RasterFace>,
                           c: [usize; 3], uvs: &[Vec2; 4], colors: &[Color; 4],
                           normal: Vec3, texture_id: usize, flip_winding: bool| {
            let base_idx = vertices.len();
            vertices.push(Vertex::with_color(corners[c[0]], uvs[c[0]], normal, colors[c[0]]));
            vertices.push(Vertex::with_color(corners[c[1]], uvs[c[1]], normal, colors[c[1]]));
            vertices.push(Vertex::with_color(corners[c[2]], uvs[c[2]], normal, colors[c[2]]));

            if flip_winding {
                faces.push(RasterFace::with_texture(base_idx, base_idx + 2, base_idx + 1, texture_id)
                    .with_black_transparent(face.black_transparent));
            } else {
                faces.push(RasterFace::with_texture(base_idx, base_idx + 1, base_idx + 2, texture_id)
                    .with_black_transparent(face.black_transparent));
            }
        };

        // Render triangle 1
        if render_front {
            let flip = !is_floor; // Ceilings need flipped winding
            add_triangle(vertices, faces, tri1_corners, &uvs_1, colors_1, front_normal, texture_id_1, flip);
        }
        if render_back {
            let flip = is_floor; // Back faces flip the winding
            add_triangle(vertices, faces, tri1_corners, &uvs_1, colors_1, back_normal, texture_id_1, flip);
        }

        // Render triangle 2
        if render_front {
            let flip = !is_floor;
            add_triangle(vertices, faces, tri2_corners, &uvs_2, colors_2, front_normal, texture_id_2, flip);
        }
        if render_back {
            let flip = is_floor;
            add_triangle(vertices, faces, tri2_corners, &uvs_2, colors_2, back_normal, texture_id_2, flip);
        }
    }

    /// Helper to add a wall to render data
    fn add_wall_to_render_data<F>(
        &self,
        vertices: &mut Vec<Vertex>,
        faces: &mut Vec<RasterFace>,
        wall: &VerticalFace,
        base_x: f32,
        base_z: f32,
        grid_x: usize,
        grid_z: usize,
        direction: Direction,
        resolve_texture: &F,
    )
    where
        F: Fn(&TextureRef) -> Option<usize>,
    {
        // Wall corners based on direction
        // Each wall has 4 corners: bottom-left, bottom-right, top-right, top-left (from inside room)
        // wall.heights = [bottom-left, bottom-right, top-right, top-left]
        // Heights are room-relative, so add room.position.y for world-space rendering
        let y_offset = self.position.y;
        let (corners, front_normal) = match direction {
            Direction::North => {
                // Wall at -Z edge, facing +Z (into room)
                let corners = [
                    Vec3::new(base_x, y_offset + wall.heights[0], base_z),                    // bottom-left
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[1], base_z),      // bottom-right
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[2], base_z),      // top-right
                    Vec3::new(base_x, y_offset + wall.heights[3], base_z),                    // top-left
                ];
                (corners, Vec3::new(0.0, 0.0, 1.0))
            }
            Direction::East => {
                // Wall at +X edge, facing -X (into room)
                let corners = [
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[0], base_z),
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[1], base_z + SECTOR_SIZE),
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[2], base_z + SECTOR_SIZE),
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[3], base_z),
                ];
                (corners, Vec3::new(-1.0, 0.0, 0.0))
            }
            Direction::South => {
                // Wall at +Z edge, facing -Z (into room)
                let corners = [
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[0], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, y_offset + wall.heights[1], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, y_offset + wall.heights[2], base_z + SECTOR_SIZE),
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[3], base_z + SECTOR_SIZE),
                ];
                (corners, Vec3::new(0.0, 0.0, -1.0))
            }
            Direction::West => {
                // Wall at -X edge, facing +X (into room)
                let corners = [
                    Vec3::new(base_x, y_offset + wall.heights[0], base_z + SECTOR_SIZE),
                    Vec3::new(base_x, y_offset + wall.heights[1], base_z),
                    Vec3::new(base_x, y_offset + wall.heights[2], base_z),
                    Vec3::new(base_x, y_offset + wall.heights[3], base_z + SECTOR_SIZE),
                ];
                (corners, Vec3::new(1.0, 0.0, 0.0))
            }
            Direction::NwSe => {
                // Diagonal wall from NW to SE corner
                // NW = (base_x, base_z), SE = (base_x + SECTOR_SIZE, base_z + SECTOR_SIZE)
                // Normal faces NE-SW direction (perpendicular to NW-SE)
                let corners = [
                    Vec3::new(base_x, y_offset + wall.heights[0], base_z),                                 // NW bottom
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[1], base_z + SECTOR_SIZE),     // SE bottom
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[2], base_z + SECTOR_SIZE),     // SE top
                    Vec3::new(base_x, y_offset + wall.heights[3], base_z),                                 // NW top
                ];
                // Normal perpendicular to NW-SE line, normalized: (1, 0, -1) / sqrt(2)
                let n = 1.0 / 2.0_f32.sqrt();
                (corners, Vec3::new(n, 0.0, -n))
            }
            Direction::NeSw => {
                // Diagonal wall from NE to SW corner
                // NE = (base_x + SECTOR_SIZE, base_z), SW = (base_x, base_z + SECTOR_SIZE)
                // Normal faces NW-SE direction (perpendicular to NE-SW)
                let corners = [
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[0], base_z),                   // NE bottom
                    Vec3::new(base_x, y_offset + wall.heights[1], base_z + SECTOR_SIZE),                   // SW bottom
                    Vec3::new(base_x, y_offset + wall.heights[2], base_z + SECTOR_SIZE),                   // SW top
                    Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[3], base_z),                   // NE top
                ];
                // Normal perpendicular to NE-SW line, normalized: (-1, 0, -1) / sqrt(2)
                let n = 1.0 / 2.0_f32.sqrt();
                (corners, Vec3::new(-n, 0.0, -n))
            }
        };

        // Calculate U coordinates for world-aligned tiling
        // Use same technique as floors: grid position determines UV offset
        // Each wall spans one grid cell, so U goes from grid*UV_SCALE to (grid+1)*UV_SCALE
        // Corners [0,3] are "left" and [1,2] are "right" from viewer's perspective
        let (u_left, u_right) = match direction {
            Direction::North | Direction::South | Direction::NwSe | Direction::NeSw => {
                let u = (grid_x as f32) * UV_SCALE;
                (u, u + UV_SCALE)
            }
            Direction::East | Direction::West => {
                let u = (grid_z as f32) * UV_SCALE;
                (u, u + UV_SCALE)
            }
        };
        // All walls have corners ordered as: [bottom-left, bottom-right, top-right, top-left]
        // from the viewer's perspective (inside the room looking at the wall)
        let corner_u: [f32; 4] = [u_left, u_right, u_right, u_left];

        // Calculate UVs based on projection mode
        let uvs = if wall.uv_projection == UvProjection::Projected {
            // Projected mode: UVs based on absolute world Y position
            // This creates globally aligned texture mapping across adjacent walls

            // Get base UVs to extract the U coordinates
            let base_uvs = wall.uv.unwrap_or_else(|| [
                Vec2::new(corner_u[0], UV_SCALE),  // bottom-left
                Vec2::new(corner_u[1], UV_SCALE),  // bottom-right
                Vec2::new(corner_u[2], 0.0),       // top-right
                Vec2::new(corner_u[3], 0.0),       // top-left
            ]);

            // Calculate world Y positions (including room y_offset)
            // heights order: [bottom-left, bottom-right, top-right, top-left]
            let world_heights = [
                y_offset + wall.heights[0],
                y_offset + wall.heights[1],
                y_offset + wall.heights[2],
                y_offset + wall.heights[3],
            ];

            // Calculate V based on absolute world position (scaled by UV_SCALE)
            // V = -world_y / SECTOR_SIZE * UV_SCALE (higher Y = lower V value, texture wraps via rasterizer)
            // Don't use fract() - let the rasterizer handle wrapping to maintain continuous interpolation
            [
                Vec2::new(base_uvs[0].x, -world_heights[0] / SECTOR_SIZE * UV_SCALE),
                Vec2::new(base_uvs[1].x, -world_heights[1] / SECTOR_SIZE * UV_SCALE),
                Vec2::new(base_uvs[2].x, -world_heights[2] / SECTOR_SIZE * UV_SCALE),
                Vec2::new(base_uvs[3].x, -world_heights[3] / SECTOR_SIZE * UV_SCALE),
            ]
        } else {
            // Default mode: standard per-vertex UVs based on corner world positions
            wall.uv.unwrap_or_else(|| [
                Vec2::new(corner_u[0], UV_SCALE),  // bottom-left
                Vec2::new(corner_u[1], UV_SCALE),  // bottom-right
                Vec2::new(corner_u[2], 0.0),       // top-right
                Vec2::new(corner_u[3], 0.0),       // top-left
            ])
        };

        let texture_id = resolve_texture(&wall.texture).unwrap_or(0);

        // Handle normal mode: Front, Back, or Both
        let render_front = wall.normal_mode != FaceNormalMode::Back;
        let render_back = wall.normal_mode != FaceNormalMode::Front;

        // Add front-facing face
        if render_front {
            let base_idx = vertices.len();
            for i in 0..4 {
                vertices.push(Vertex::with_color(corners[i], uvs[i], front_normal, wall.colors[i]));
            }
            // Two triangles for the quad (CCW winding when viewed from inside room)
            faces.push(RasterFace::with_texture(base_idx, base_idx + 2, base_idx + 1, texture_id).with_black_transparent(wall.black_transparent));
            faces.push(RasterFace::with_texture(base_idx, base_idx + 3, base_idx + 2, texture_id).with_black_transparent(wall.black_transparent));
        }

        // Add back-facing face (flipped normal and winding)
        if render_back {
            let base_idx = vertices.len();
            let back_normal = front_normal.scale(-1.0);
            for i in 0..4 {
                vertices.push(Vertex::with_color(corners[i], uvs[i], back_normal, wall.colors[i]));
            }
            // Reverse winding order for back face
            faces.push(RasterFace::with_texture(base_idx, base_idx + 1, base_idx + 2, texture_id).with_black_transparent(wall.black_transparent));
            faces.push(RasterFace::with_texture(base_idx, base_idx + 2, base_idx + 3, texture_id).with_black_transparent(wall.black_transparent));
        }
    }

    /// Helper to add a diagonal wall to render data
    /// is_nwse: true for NW-SE diagonal (corners 0 and 2), false for NE-SW diagonal (corners 1 and 3)
    fn add_diagonal_wall_to_render_data<F>(
        &self,
        vertices: &mut Vec<Vertex>,
        faces: &mut Vec<RasterFace>,
        wall: &VerticalFace,
        base_x: f32,
        base_z: f32,
        grid_x: usize,
        _grid_z: usize,
        is_nwse: bool,
        resolve_texture: &F,
    )
    where
        F: Fn(&TextureRef) -> Option<usize>,
    {
        // Heights are room-relative, so add room.position.y for world-space rendering
        let y_offset = self.position.y;

        // Diagonal walls span corner-to-corner
        // wall.heights = [corner1_bottom, corner2_bottom, corner2_top, corner1_top]
        // Vertex order is reversed from cardinal walls so front face points INTO the room
        let (corners, front_normal) = if is_nwse {
            // NW-SE diagonal: wall cuts off SW corner, front faces NE (into room)
            // Corners: SE (bottom), NW (bottom), NW (top), SE (top) - reversed winding
            let corners = [
                Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[1], base_z + SECTOR_SIZE),   // SE bottom
                Vec3::new(base_x, y_offset + wall.heights[0], base_z),                               // NW bottom
                Vec3::new(base_x, y_offset + wall.heights[3], base_z),                               // NW top
                Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[2], base_z + SECTOR_SIZE),   // SE top
            ];
            // Normal points NE (into room): (n, 0, -n)
            let n = 1.0 / (2.0_f32).sqrt();
            (corners, Vec3::new(n, 0.0, -n))
        } else {
            // NE-SW diagonal: wall cuts off NW corner, front faces SE (into room)
            // Corners: SW (bottom), NE (bottom), NE (top), SW (top) - reversed winding
            let corners = [
                Vec3::new(base_x, y_offset + wall.heights[1], base_z + SECTOR_SIZE),                 // SW bottom
                Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[0], base_z),                 // NE bottom
                Vec3::new(base_x + SECTOR_SIZE, y_offset + wall.heights[3], base_z),                 // NE top
                Vec3::new(base_x, y_offset + wall.heights[2], base_z + SECTOR_SIZE),                 // SW top
            ];
            // Normal points SE (into room): (n, 0, n)
            let n = 1.0 / (2.0_f32).sqrt();
            (corners, Vec3::new(n, 0.0, n))
        };

        // Calculate U coordinates for world-aligned tiling
        // Use same technique as cardinal walls: grid position determines UV offset
        let u_left = (grid_x as f32) * UV_SCALE;
        let u_right = u_left + UV_SCALE;
        // Corners [0,3] are left side, [1,2] are right side from viewer's perspective
        let corner_u: [f32; 4] = [u_left, u_right, u_right, u_left];

        // Calculate UVs based on projection mode
        let uvs = if wall.uv_projection == UvProjection::Projected {
            // Projected mode: UVs based on absolute world Y position (scaled by UV_SCALE)
            let base_uvs = wall.uv.unwrap_or_else(|| [
                Vec2::new(corner_u[0], UV_SCALE),  // bottom-left
                Vec2::new(corner_u[1], UV_SCALE),  // bottom-right
                Vec2::new(corner_u[2], 0.0),       // top-right
                Vec2::new(corner_u[3], 0.0),       // top-left
            ]);

            let world_heights = [
                y_offset + wall.heights[0],
                y_offset + wall.heights[1],
                y_offset + wall.heights[2],
                y_offset + wall.heights[3],
            ];

            [
                Vec2::new(base_uvs[0].x, -world_heights[0] / SECTOR_SIZE * UV_SCALE),
                Vec2::new(base_uvs[1].x, -world_heights[1] / SECTOR_SIZE * UV_SCALE),
                Vec2::new(base_uvs[2].x, -world_heights[2] / SECTOR_SIZE * UV_SCALE),
                Vec2::new(base_uvs[3].x, -world_heights[3] / SECTOR_SIZE * UV_SCALE),
            ]
        } else {
            // Default mode: standard per-vertex UVs based on grid position
            wall.uv.unwrap_or_else(|| [
                Vec2::new(corner_u[0], UV_SCALE),  // bottom-left
                Vec2::new(corner_u[1], UV_SCALE),  // bottom-right
                Vec2::new(corner_u[2], 0.0),       // top-right
                Vec2::new(corner_u[3], 0.0),       // top-left
            ])
        };

        let texture_id = resolve_texture(&wall.texture).unwrap_or(0);

        // Handle normal mode: Front, Back, or Both
        let render_front = wall.normal_mode != FaceNormalMode::Back;
        let render_back = wall.normal_mode != FaceNormalMode::Front;

        // Add front-facing face
        if render_front {
            let base_idx = vertices.len();
            for i in 0..4 {
                vertices.push(Vertex::with_color(corners[i], uvs[i], front_normal, wall.colors[i]));
            }
            // Two triangles for the quad
            faces.push(RasterFace::with_texture(base_idx, base_idx + 2, base_idx + 1, texture_id).with_black_transparent(wall.black_transparent));
            faces.push(RasterFace::with_texture(base_idx, base_idx + 3, base_idx + 2, texture_id).with_black_transparent(wall.black_transparent));
        }

        // Add back-facing face (flipped normal and winding)
        if render_back {
            let base_idx = vertices.len();
            let back_normal = front_normal.scale(-1.0);
            for i in 0..4 {
                vertices.push(Vertex::with_color(corners[i], uvs[i], back_normal, wall.colors[i]));
            }
            // Reverse winding order for back face
            faces.push(RasterFace::with_texture(base_idx, base_idx + 1, base_idx + 2, texture_id).with_black_transparent(wall.black_transparent));
            faces.push(RasterFace::with_texture(base_idx, base_idx + 2, base_idx + 3, texture_id).with_black_transparent(wall.black_transparent));
        }
    }
}

/// Editor layout configuration (saved with level)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorLayoutConfig {
    /// Main horizontal split ratio (left panels | center+right)
    pub main_split: f32,
    /// Right split ratio (center viewport | right panels)
    pub right_split: f32,
    /// Left vertical split ratio (2D grid | room properties)
    pub left_split: f32,
    /// Right vertical split ratio (texture palette | properties)
    pub right_panel_split: f32,
    /// 2D grid view pan offset X (screen pixels)
    #[serde(default)]
    pub grid_offset_x: f32,
    /// 2D grid view pan offset Y (screen pixels)
    #[serde(default)]
    pub grid_offset_y: f32,
    /// 2D grid view zoom level (pixels per world unit)
    #[serde(default = "default_grid_zoom")]
    pub grid_zoom: f32,
    /// 3D orbit camera target X
    #[serde(default = "default_orbit_target_x")]
    pub orbit_target_x: f32,
    /// 3D orbit camera target Y
    #[serde(default = "default_orbit_target_y")]
    pub orbit_target_y: f32,
    /// 3D orbit camera target Z
    #[serde(default = "default_orbit_target_z")]
    pub orbit_target_z: f32,
    /// 3D orbit camera distance from target
    #[serde(default = "default_orbit_distance")]
    pub orbit_distance: f32,
    /// 3D orbit camera horizontal angle (radians)
    #[serde(default = "default_orbit_azimuth")]
    pub orbit_azimuth: f32,
    /// 3D orbit camera vertical angle (radians)
    #[serde(default = "default_orbit_elevation")]
    pub orbit_elevation: f32,
}

fn default_grid_zoom() -> f32 {
    0.1
}

fn default_orbit_target_x() -> f32 { 512.0 }
fn default_orbit_target_y() -> f32 { 512.0 }
fn default_orbit_target_z() -> f32 { 512.0 }
fn default_orbit_distance() -> f32 { 4000.0 }
fn default_orbit_azimuth() -> f32 { 0.8 }
fn default_orbit_elevation() -> f32 { 0.4 }

impl Default for EditorLayoutConfig {
    fn default() -> Self {
        Self {
            main_split: 0.25,
            right_split: 0.75,
            left_split: 0.6,
            right_panel_split: 0.6,
            grid_offset_x: 0.0,
            grid_offset_y: 0.0,
            grid_zoom: 0.1,
            orbit_target_x: 512.0,
            orbit_target_y: 512.0,
            orbit_target_z: 512.0,
            orbit_distance: 4000.0,
            orbit_azimuth: 0.8,
            orbit_elevation: 0.4,
        }
    }
}

/// Floor info at a world position for collision detection
#[derive(Debug, Clone, Copy)]
pub struct FloorInfo {
    /// Room index containing this point
    pub room: usize,
    /// Floor height at this position (world Y)
    pub floor: f32,
    /// Ceiling height at this position (world Y)
    pub ceiling: f32,
    /// Sector grid X within room
    pub sector_x: usize,
    /// Sector grid Z within room
    pub sector_z: usize,
}

/// The entire level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    pub rooms: Vec<Room>,
    /// Editor layout configuration (optional, uses default if missing)
    #[serde(default)]
    pub editor_layout: EditorLayoutConfig,
    /// Player character settings (collision, movement, camera)
    #[serde(default)]
    pub player_settings: PlayerSettings,
    /// Skybox configuration (gradient sky)
    #[serde(default)]
    pub skybox: Option<Skybox>,
}

impl Level {
    pub fn new() -> Self {
        Self {
            rooms: Vec::new(),
            editor_layout: EditorLayoutConfig::default(),
            player_settings: PlayerSettings::default(),
            skybox: None,
        }
    }

    // ========================================================================
    // Tile-based object system (objects are now stored per-room)
    // ========================================================================

    /// Get the player start object and its room index
    pub fn get_player_start(&self) -> Option<(usize, &LevelObject)> {
        for (room_idx, room) in self.rooms.iter().enumerate() {
            if let Some(obj) = room.objects.iter()
                .find(|obj| obj.enabled && matches!(obj.object_type, ObjectType::Spawn(SpawnPointType::PlayerStart)))
            {
                return Some((room_idx, obj));
            }
        }
        None
    }

    /// Get all objects at a specific sector in a room
    pub fn objects_at(&self, room_idx: usize, sector_x: usize, sector_z: usize) -> impl Iterator<Item = &LevelObject> {
        self.rooms.get(room_idx)
            .map(|room| room.objects.iter()
                .filter(move |obj| obj.sector_x == sector_x && obj.sector_z == sector_z))
            .into_iter()
            .flatten()
    }

    /// Get all objects in a room
    pub fn objects_in_room(&self, room_idx: usize) -> impl Iterator<Item = &LevelObject> {
        self.rooms.get(room_idx)
            .map(|room| room.objects.iter())
            .into_iter()
            .flatten()
    }

    /// Check if an object can be added at a sector (validates restrictions)
    pub fn can_add_object(&self, room_idx: usize, sector_x: usize, sector_z: usize, object_type: &ObjectType) -> Result<(), &'static str> {
        // Check per-level uniqueness (e.g., only one PlayerStart)
        if object_type.is_unique_per_level() {
            for room in &self.rooms {
                let exists = room.objects.iter().any(|obj| {
                    std::mem::discriminant(&obj.object_type) == std::mem::discriminant(object_type)
                });
                if exists {
                    return Err("Only one of this object type allowed per level");
                }
            }
        }

        // Check per-tile uniqueness (e.g., only one light per tile)
        if object_type.is_unique_per_tile() {
            let tile_objects = self.objects_at(room_idx, sector_x, sector_z);
            for obj in tile_objects {
                // Check if same category exists
                let same_category = match (&obj.object_type, object_type) {
                    (ObjectType::Light { .. }, ObjectType::Light { .. }) => true,
                    (ObjectType::Spawn(SpawnPointType::PlayerStart), ObjectType::Spawn(SpawnPointType::PlayerStart)) => true,
                    (ObjectType::Spawn(SpawnPointType::Checkpoint), ObjectType::Spawn(SpawnPointType::Checkpoint)) => true,
                    _ => false,
                };
                if same_category {
                    return Err("Only one of this object type allowed per tile");
                }
            }
        }

        Ok(())
    }

    /// Add an object to a room (validates restrictions)
    pub fn add_object(&mut self, room_idx: usize, object: LevelObject) -> Result<usize, &'static str> {
        self.can_add_object(room_idx, object.sector_x, object.sector_z, &object.object_type)?;
        if let Some(room) = self.rooms.get_mut(room_idx) {
            let idx = room.objects.len();
            room.objects.push(object);
            Ok(idx)
        } else {
            Err("Invalid room index")
        }
    }

    /// Add an object without validation (for internal use or loading)
    pub fn add_object_unchecked(&mut self, room_idx: usize, object: LevelObject) -> Option<usize> {
        if let Some(room) = self.rooms.get_mut(room_idx) {
            let idx = room.objects.len();
            room.objects.push(object);
            Some(idx)
        } else {
            None
        }
    }

    /// Remove an object by room and index
    pub fn remove_object(&mut self, room_idx: usize, object_idx: usize) -> Option<LevelObject> {
        if let Some(room) = self.rooms.get_mut(room_idx) {
            if object_idx < room.objects.len() {
                return Some(room.objects.remove(object_idx));
            }
        }
        None
    }

    /// Remove all objects at a specific sector in a room
    pub fn remove_objects_at(&mut self, room_idx: usize, sector_x: usize, sector_z: usize) {
        if let Some(room) = self.rooms.get_mut(room_idx) {
            room.objects.retain(|obj| !(obj.sector_x == sector_x && obj.sector_z == sector_z));
        }
    }

    /// Find object index by position and type in a room
    pub fn find_object(&self, room_idx: usize, sector_x: usize, sector_z: usize, object_type: &ObjectType) -> Option<usize> {
        self.rooms.get(room_idx)?.objects.iter().position(|obj| {
            obj.sector_x == sector_x
                && obj.sector_z == sector_z
                && std::mem::discriminant(&obj.object_type) == std::mem::discriminant(object_type)
        })
    }

    /// Get mutable reference to an object by room and index
    pub fn get_object_mut(&mut self, room_idx: usize, object_idx: usize) -> Option<&mut LevelObject> {
        self.rooms.get_mut(room_idx)?.objects.get_mut(object_idx)
    }

    /// Count objects of a specific type across all rooms
    pub fn count_objects_of_type(&self, object_type: &ObjectType) -> usize {
        self.rooms.iter()
            .flat_map(|room| room.objects.iter())
            .filter(|obj| std::mem::discriminant(&obj.object_type) == std::mem::discriminant(object_type))
            .count()
    }

    /// Add a room and return its index
    pub fn add_room(&mut self, room: Room) -> usize {
        let id = self.rooms.len();
        self.rooms.push(room);
        id
    }

    /// Find which room contains a point
    pub fn find_room_at(&self, point: Vec3) -> Option<usize> {
        for (i, room) in self.rooms.iter().enumerate() {
            if room.contains_point(point) {
                return Some(i);
            }
        }
        None
    }

    /// Find room with hint (check hint first for faster lookup)
    pub fn find_room_at_with_hint(&self, point: Vec3, hint: Option<usize>) -> Option<usize> {
        // Check hint first
        if let Some(hint_id) = hint {
            if let Some(room) = self.rooms.get(hint_id) {
                if room.contains_point(point) {
                    return Some(hint_id);
                }
            }
        }

        // Fall back to linear search
        self.find_room_at(point)
    }

    // ========================================================================
    // Floor/Ceiling queries (for collision detection)
    // ========================================================================

    /// Get floor and ceiling info at a world position
    ///
    /// Returns None if the point is outside all rooms.
    pub fn get_floor_info(&self, point: Vec3, room_hint: Option<usize>) -> Option<FloorInfo> {
        let room_idx = self.find_room_at_with_hint(point, room_hint)?;
        let room = &self.rooms[room_idx];

        // Convert world position to sector coordinates
        let local_x = point.x - room.position.x;
        let local_z = point.z - room.position.z;

        let sector_x = (local_x / SECTOR_SIZE).floor() as isize;
        let sector_z = (local_z / SECTOR_SIZE).floor() as isize;

        // Bounds check
        if sector_x < 0 || sector_z < 0 {
            return None;
        }
        let sector_x = sector_x as usize;
        let sector_z = sector_z as usize;

        // Get sector
        let sector = room.get_sector(sector_x, sector_z)?;

        // Calculate normalized position within the sector (0.0 to 1.0)
        // u = 0 is West (-X), u = 1 is East (+X)
        // v = 0 is North (-Z), v = 1 is South (+Z)
        let sector_base_x = sector_x as f32 * SECTOR_SIZE;
        let sector_base_z = sector_z as f32 * SECTOR_SIZE;
        let u = (local_x - sector_base_x) / SECTOR_SIZE;
        let v = (local_z - sector_base_z) / SECTOR_SIZE;

        // Get floor height using proper triangle interpolation for slopes
        let floor_y = sector.floor.as_ref()
            .map(|f| room.position.y + f.interpolate_height(u, v))
            .unwrap_or(room.position.y);

        // Get ceiling height using proper triangle interpolation
        let ceiling_y = sector.ceiling.as_ref()
            .map(|c| room.position.y + c.interpolate_height(u, v))
            .unwrap_or(room.position.y + 2048.0); // Default 2 sectors high

        Some(FloorInfo {
            room: room_idx,
            floor: floor_y,
            ceiling: ceiling_y,
            sector_x,
            sector_z,
        })
    }

    /// Get floor height at a world position (simpler query)
    pub fn get_floor_height(&self, point: Vec3, room_hint: Option<usize>) -> Option<f32> {
        self.get_floor_info(point, room_hint).map(|info| info.floor)
    }

    /// Get ceiling height at a world position
    pub fn get_ceiling_height(&self, point: Vec3, room_hint: Option<usize>) -> Option<f32> {
        self.get_floor_info(point, room_hint).map(|info| info.ceiling)
    }

    /// Recalculate all portals based on room adjacency
    /// Call this after room positions change, heights change, or walls are added/removed
    pub fn recalculate_portals(&mut self) {
        // Clear existing portals from all rooms
        for room in &mut self.rooms {
            room.portals.clear();
        }

        // For each pair of rooms, detect portals between them
        let num_rooms = self.rooms.len();
        for room_a_idx in 0..num_rooms {
            for room_b_idx in (room_a_idx + 1)..num_rooms {
                self.detect_portals_between(room_a_idx, room_b_idx);
            }
        }
    }

    /// Detect and create portals between two rooms based on adjacent sectors
    fn detect_portals_between(&mut self, room_a_idx: usize, room_b_idx: usize) {
        // We need to check if any sector edges in room A are adjacent to sector edges in room B
        // Two sectors are adjacent if they share an edge at the same world position

        // Get room data (positions and dimensions)
        let (pos_a, width_a, depth_a) = {
            let room = &self.rooms[room_a_idx];
            (room.position, room.width, room.depth)
        };
        let (pos_b, width_b, depth_b) = {
            let room = &self.rooms[room_b_idx];
            (room.position, room.width, room.depth)
        };

        // Check all directions for adjacency
        let directions = [Direction::North, Direction::East, Direction::South, Direction::West];

        for &dir in &directions {
            // For each sector in room A on its boundary facing direction `dir`
            // Check if there's a matching sector in room B on the opposite boundary

            for gx_a in 0..width_a {
                for gz_a in 0..depth_a {
                    // World position of this sector in room A
                    let world_x_a = pos_a.x + (gx_a as f32) * SECTOR_SIZE;
                    let world_z_a = pos_a.z + (gz_a as f32) * SECTOR_SIZE;

                    // World position of the adjacent sector (on the edge in direction `dir`)
                    // Note: This function only checks cardinal directions for portal detection
                    let (adj_world_x, adj_world_z) = match dir {
                        Direction::North => (world_x_a, world_z_a - SECTOR_SIZE),
                        Direction::East => (world_x_a + SECTOR_SIZE, world_z_a),
                        Direction::South => (world_x_a, world_z_a + SECTOR_SIZE),
                        Direction::West => (world_x_a - SECTOR_SIZE, world_z_a),
                        Direction::NwSe | Direction::NeSw => continue, // Diagonals not checked for portals
                    };

                    // Check if this adjacent position falls within room B's grid
                    let local_x_b = adj_world_x - pos_b.x;
                    let local_z_b = adj_world_z - pos_b.z;

                    // Must be aligned to grid
                    if local_x_b < 0.0 || local_z_b < 0.0 {
                        continue;
                    }
                    if (local_x_b % SECTOR_SIZE).abs() > 0.1 || (local_z_b % SECTOR_SIZE).abs() > 0.1 {
                        continue;
                    }

                    let gx_b = (local_x_b / SECTOR_SIZE) as usize;
                    let gz_b = (local_z_b / SECTOR_SIZE) as usize;

                    if gx_b >= width_b || gz_b >= depth_b {
                        continue;
                    }

                    // Now check if both sectors exist and have no walls blocking
                    let sector_a = self.rooms[room_a_idx].get_sector(gx_a, gz_a);
                    let sector_b = self.rooms[room_b_idx].get_sector(gx_b, gz_b);

                    let (sector_a, sector_b) = match (sector_a, sector_b) {
                        (Some(a), Some(b)) => (a, b),
                        _ => continue, // One or both sectors don't exist
                    };

                    // Check for walls blocking the portal
                    let opposite_dir = dir.opposite();
                    if !sector_a.walls(dir).is_empty() || !sector_b.walls(opposite_dir).is_empty() {
                        continue; // Wall blocks the portal
                    }

                    // Wall portals require both sectors to have floors AND ceilings
                    // If either sector is open (no floor or no ceiling), wall portals don't make sense -
                    // the connection should be through floor/ceiling portals instead
                    if sector_a.floor.is_none() || sector_a.ceiling.is_none() ||
                       sector_b.floor.is_none() || sector_b.ceiling.is_none() {
                        continue;
                    }

                    // Calculate portal opening at each corner (trapezoidal portal for sloped surfaces)
                    // Get edge heights from both sectors: (left, right) when looking from inside
                    // The edge_heights function returns room-relative heights, so we must add room.position.y
                    // to get world-space heights for proper comparison between rooms at different Y levels.
                    //
                    // For open-air sectors (no ceiling), we use INFINITY to represent unbounded height.
                    // This is valid for rendering but must be handled specially during serialization.
                    let (floor_a_left, floor_a_right) = sector_a.floor.as_ref()
                        .map(|f| {
                            let (l, r) = f.edge_heights(dir);
                            (l + pos_a.y, r + pos_a.y)  // Convert to world-space
                        })
                        .unwrap_or((f32::NEG_INFINITY, f32::NEG_INFINITY));
                    let (floor_b_left, floor_b_right) = sector_b.floor.as_ref()
                        .map(|f| {
                            let (l, r) = f.edge_heights(opposite_dir);
                            (l + pos_b.y, r + pos_b.y)  // Convert to world-space
                        })
                        .unwrap_or((f32::NEG_INFINITY, f32::NEG_INFINITY));

                    let (ceil_a_left, ceil_a_right) = sector_a.ceiling.as_ref()
                        .map(|c| {
                            let (l, r) = c.edge_heights(dir);
                            (l + pos_a.y, r + pos_a.y)  // Convert to world-space
                        })
                        .unwrap_or((f32::INFINITY, f32::INFINITY));
                    let (ceil_b_left, ceil_b_right) = sector_b.ceiling.as_ref()
                        .map(|c| {
                            let (l, r) = c.edge_heights(opposite_dir);
                            (l + pos_b.y, r + pos_b.y)  // Convert to world-space
                        })
                        .unwrap_or((f32::INFINITY, f32::INFINITY));

                    // Portal bottom at each corner = max of both floors (world-space)
                    // Portal top at each corner = min of both ceilings (world-space)
                    let portal_bottom_left = floor_a_left.max(floor_b_left);
                    let portal_bottom_right = floor_a_right.max(floor_b_right);
                    let portal_top_left = ceil_a_left.min(ceil_b_left);
                    let portal_top_right = ceil_a_right.min(ceil_b_right);

                    // Skip if no vertical opening at either corner
                    if portal_bottom_left >= portal_top_left && portal_bottom_right >= portal_top_right {
                        continue;
                    }

                    // Create portal vertices (quad at the shared edge)
                    // Vertices are in world space, will be converted to room-relative when stored
                    // v0=bottom-left, v1=bottom-right, v2=top-right, v3=top-left
                    let (v0, v1, v2, v3, normal_a) = match dir {
                        Direction::North => {
                            // Edge at -Z side of sector A
                            let edge_z = world_z_a;
                            (
                                Vec3::new(world_x_a, portal_bottom_left, edge_z),              // bottom-left (NW corner)
                                Vec3::new(world_x_a + SECTOR_SIZE, portal_bottom_right, edge_z), // bottom-right (NE corner)
                                Vec3::new(world_x_a + SECTOR_SIZE, portal_top_right, edge_z),    // top-right
                                Vec3::new(world_x_a, portal_top_left, edge_z),                  // top-left
                                Vec3::new(0.0, 0.0, -1.0), // Normal points into room A (toward -Z)
                            )
                        }
                        Direction::East => {
                            // Edge at +X side of sector A
                            let edge_x = world_x_a + SECTOR_SIZE;
                            (
                                Vec3::new(edge_x, portal_bottom_left, world_z_a),              // bottom-left (NE corner)
                                Vec3::new(edge_x, portal_bottom_right, world_z_a + SECTOR_SIZE), // bottom-right (SE corner)
                                Vec3::new(edge_x, portal_top_right, world_z_a + SECTOR_SIZE),    // top-right
                                Vec3::new(edge_x, portal_top_left, world_z_a),                  // top-left
                                Vec3::new(1.0, 0.0, 0.0), // Normal points into room A (toward +X)
                            )
                        }
                        Direction::South => {
                            // Edge at +Z side of sector A
                            let edge_z = world_z_a + SECTOR_SIZE;
                            (
                                Vec3::new(world_x_a + SECTOR_SIZE, portal_bottom_left, edge_z), // bottom-left (SE corner)
                                Vec3::new(world_x_a, portal_bottom_right, edge_z),              // bottom-right (SW corner)
                                Vec3::new(world_x_a, portal_top_right, edge_z),                  // top-right
                                Vec3::new(world_x_a + SECTOR_SIZE, portal_top_left, edge_z),    // top-left
                                Vec3::new(0.0, 0.0, 1.0), // Normal points into room A (toward +Z)
                            )
                        }
                        Direction::West => {
                            // Edge at -X side of sector A
                            let edge_x = world_x_a;
                            (
                                Vec3::new(edge_x, portal_bottom_left, world_z_a + SECTOR_SIZE), // bottom-left (SW corner)
                                Vec3::new(edge_x, portal_bottom_right, world_z_a),              // bottom-right (NW corner)
                                Vec3::new(edge_x, portal_top_right, world_z_a),                  // top-right
                                Vec3::new(edge_x, portal_top_left, world_z_a + SECTOR_SIZE),    // top-left
                                Vec3::new(-1.0, 0.0, 0.0), // Normal points into room A (toward -X)
                            )
                        }
                        Direction::NwSe | Direction::NeSw => continue, // Diagonals not checked for portals
                    };

                    // Convert to room-relative coordinates and add portals to both rooms
                    // Portal in room A points to room B
                    let vertices_a = [
                        Vec3::new(v0.x - pos_a.x, v0.y - pos_a.y, v0.z - pos_a.z),
                        Vec3::new(v1.x - pos_a.x, v1.y - pos_a.y, v1.z - pos_a.z),
                        Vec3::new(v2.x - pos_a.x, v2.y - pos_a.y, v2.z - pos_a.z),
                        Vec3::new(v3.x - pos_a.x, v3.y - pos_a.y, v3.z - pos_a.z),
                    ];
                    self.rooms[room_a_idx].portals.push(Portal::new(room_b_idx, vertices_a, normal_a));

                    // Portal in room B points to room A (opposite normal)
                    let normal_b = Vec3::new(-normal_a.x, -normal_a.y, -normal_a.z);
                    let vertices_b = [
                        Vec3::new(v1.x - pos_b.x, v1.y - pos_b.y, v1.z - pos_b.z), // Swap order for opposite facing
                        Vec3::new(v0.x - pos_b.x, v0.y - pos_b.y, v0.z - pos_b.z),
                        Vec3::new(v3.x - pos_b.x, v3.y - pos_b.y, v3.z - pos_b.z),
                        Vec3::new(v2.x - pos_b.x, v2.y - pos_b.y, v2.z - pos_b.z),
                    ];
                    self.rooms[room_b_idx].portals.push(Portal::new(room_a_idx, vertices_b, normal_b));
                }
            }
        }

        // Check for horizontal portals (floor-to-ceiling connections)
        // These occur when sectors overlap in X-Z and one room's ceiling meets another's floor
        self.detect_horizontal_portals_between(room_a_idx, room_b_idx, pos_a, pos_b, width_a, depth_a, width_b, depth_b);
    }

    /// Detect horizontal portals between two rooms (floor-to-ceiling connections)
    fn detect_horizontal_portals_between(
        &mut self,
        room_a_idx: usize,
        room_b_idx: usize,
        pos_a: Vec3,
        pos_b: Vec3,
        width_a: usize,
        depth_a: usize,
        width_b: usize,
        depth_b: usize,
    ) {
        const HEIGHT_TOLERANCE: f32 = 1.0;

        let mut portals_a: Vec<Portal> = Vec::new();
        let mut portals_b: Vec<Portal> = Vec::new();

        for gx_a in 0..width_a {
            for gz_a in 0..depth_a {
                let world_x = pos_a.x + (gx_a as f32) * SECTOR_SIZE;
                let world_z = pos_a.z + (gz_a as f32) * SECTOR_SIZE;

                // Find corresponding sector in room B
                let local_x_b = world_x - pos_b.x;
                let local_z_b = world_z - pos_b.z;
                if local_x_b < 0.0 || local_z_b < 0.0 { continue; }
                if (local_x_b % SECTOR_SIZE).abs() > 0.1 || (local_z_b % SECTOR_SIZE).abs() > 0.1 { continue; }

                let gx_b = (local_x_b / SECTOR_SIZE) as usize;
                let gz_b = (local_z_b / SECTOR_SIZE) as usize;
                if gx_b >= width_b || gz_b >= depth_b { continue; }

                let (sector_a, sector_b) = match (
                    self.rooms[room_a_idx].get_sector(gx_a, gz_a),
                    self.rooms[room_b_idx].get_sector(gx_b, gz_b),
                ) {
                    (Some(a), Some(b)) => (a, b),
                    _ => continue,
                };

                // Helper to create portal pair at given Y heights
                let mut add_portal_pair = |heights: [f32; 4], upper_room_idx: usize, lower_room_idx: usize, upper_pos: Vec3, lower_pos: Vec3| {
                    let verts = [
                        Vec3::new(world_x, heights[0], world_z),
                        Vec3::new(world_x + SECTOR_SIZE, heights[1], world_z),
                        Vec3::new(world_x + SECTOR_SIZE, heights[2], world_z + SECTOR_SIZE),
                        Vec3::new(world_x, heights[3], world_z + SECTOR_SIZE),
                    ];

                    // Portal in lower room pointing up
                    let lower_verts = [
                        Vec3::new(verts[0].x - lower_pos.x, verts[0].y - lower_pos.y, verts[0].z - lower_pos.z),
                        Vec3::new(verts[1].x - lower_pos.x, verts[1].y - lower_pos.y, verts[1].z - lower_pos.z),
                        Vec3::new(verts[2].x - lower_pos.x, verts[2].y - lower_pos.y, verts[2].z - lower_pos.z),
                        Vec3::new(verts[3].x - lower_pos.x, verts[3].y - lower_pos.y, verts[3].z - lower_pos.z),
                    ];

                    // Portal in upper room pointing down (reversed winding)
                    let upper_verts = [
                        Vec3::new(verts[0].x - upper_pos.x, verts[0].y - upper_pos.y, verts[0].z - upper_pos.z),
                        Vec3::new(verts[3].x - upper_pos.x, verts[3].y - upper_pos.y, verts[3].z - upper_pos.z),
                        Vec3::new(verts[2].x - upper_pos.x, verts[2].y - upper_pos.y, verts[2].z - upper_pos.z),
                        Vec3::new(verts[1].x - upper_pos.x, verts[1].y - upper_pos.y, verts[1].z - upper_pos.z),
                    ];

                    if lower_room_idx == room_a_idx {
                        portals_a.push(Portal::new(upper_room_idx, lower_verts, Vec3::new(0.0, 1.0, 0.0)));
                        portals_b.push(Portal::new(lower_room_idx, upper_verts, Vec3::new(0.0, -1.0, 0.0)));
                    } else {
                        portals_b.push(Portal::new(upper_room_idx, lower_verts, Vec3::new(0.0, 1.0, 0.0)));
                        portals_a.push(Portal::new(lower_room_idx, upper_verts, Vec3::new(0.0, -1.0, 0.0)));
                    }
                };

                // Case 1: A's ceiling meets B's floor (A is below B)
                if let (Some(ceil_a), Some(floor_b)) = (&sector_a.ceiling, &sector_b.floor) {
                    let ceil_heights = [ceil_a.heights[0] + pos_a.y, ceil_a.heights[1] + pos_a.y,
                                        ceil_a.heights[2] + pos_a.y, ceil_a.heights[3] + pos_a.y];
                    let floor_heights = [floor_b.heights[0] + pos_b.y, floor_b.heights[1] + pos_b.y,
                                         floor_b.heights[2] + pos_b.y, floor_b.heights[3] + pos_b.y];

                    if (0..4).all(|i| (ceil_heights[i] - floor_heights[i]).abs() < HEIGHT_TOLERANCE) {
                        add_portal_pair(ceil_heights, room_b_idx, room_a_idx, pos_b, pos_a);
                    }
                }

                // Case 2: B's ceiling meets A's floor (B is below A)
                if let (Some(ceil_b), Some(floor_a)) = (&sector_b.ceiling, &sector_a.floor) {
                    let ceil_heights = [ceil_b.heights[0] + pos_b.y, ceil_b.heights[1] + pos_b.y,
                                        ceil_b.heights[2] + pos_b.y, ceil_b.heights[3] + pos_b.y];
                    let floor_heights = [floor_a.heights[0] + pos_a.y, floor_a.heights[1] + pos_a.y,
                                         floor_a.heights[2] + pos_a.y, floor_a.heights[3] + pos_a.y];

                    if (0..4).all(|i| (ceil_heights[i] - floor_heights[i]).abs() < HEIGHT_TOLERANCE) {
                        add_portal_pair(ceil_heights, room_a_idx, room_b_idx, pos_a, pos_b);
                    }
                }

                // Case 3: Open vertical - A has no ceiling, B has no floor, B is above A
                if sector_a.ceiling.is_none() && sector_b.floor.is_none() && pos_b.y > pos_a.y {
                    let h = pos_b.y;
                    add_portal_pair([h, h, h, h], room_b_idx, room_a_idx, pos_b, pos_a);
                }

                // Case 4: Open vertical - B has no ceiling, A has no floor, A is above B
                if sector_b.ceiling.is_none() && sector_a.floor.is_none() && pos_a.y > pos_b.y {
                    let h = pos_a.y;
                    add_portal_pair([h, h, h, h], room_a_idx, room_b_idx, pos_a, pos_b);
                }
            }
        }

        self.rooms[room_a_idx].portals.extend(portals_a);
        self.rooms[room_b_idx].portals.extend(portals_b);
    }
}

/// Create an empty level with a single starter room (floor only)
/// Uses TRLE sector size (1024 units) for proper grid alignment
pub fn create_empty_level() -> Level {
    let mut level = Level::new();

    // Create a single starter room with one sector (1x1 grid)
    let mut room0 = Room::new(0, Vec3::ZERO, 1, 1);

    // Add floor at height 0
    let texture = TextureRef::new("retro-texture-pack", "FLOOR_1A");
    room0.set_floor(0, 0, 0.0, texture);

    room0.recalculate_bounds();
    level.rooms.push(room0);

    level
}

/// Create a simple test level with a fully enclosed room
/// Uses TRLE sector sizes (1024 units per sector)
pub fn create_test_level() -> Level {
    let mut level = Level::new();

    // Room 0: Single sector room (1024×1024, height 1024 = 4 clicks)
    let mut room0 = Room::new(0, Vec3::ZERO, 1, 1);

    // Floor at y=0, ceiling at y=1024
    let floor_tex = TextureRef::new("retro-texture-pack", "FLOOR_1A");
    let ceiling_tex = TextureRef::new("retro-texture-pack", "FLOOR_1A");
    let wall_tex = TextureRef::new("retro-texture-pack", "WALL_1A");

    room0.set_floor(0, 0, 0.0, floor_tex);
    room0.set_ceiling(0, 0, 1024.0, ceiling_tex);

    // Four walls around the single sector
    room0.add_wall(0, 0, Direction::North, 0.0, 1024.0, wall_tex.clone());
    room0.add_wall(0, 0, Direction::East, 0.0, 1024.0, wall_tex.clone());
    room0.add_wall(0, 0, Direction::South, 0.0, 1024.0, wall_tex.clone());
    room0.add_wall(0, 0, Direction::West, 0.0, 1024.0, wall_tex);

    room0.recalculate_bounds();
    level.add_room(room0);

    level
}
