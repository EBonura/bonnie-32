//! Particle System
//!
//! PS1-authentic particle effects using a fixed-size pool.
//! Particles are rendered as colored pixels or small quads on the software
//! framebuffer, matching how PS1 games handled effects (FF7 battle sparks,
//! Spyro gem sparkles, Crash crate explosions).

use serde::{Serialize, Deserialize};
use crate::rasterizer::{Vec3, Color, Framebuffer, Camera};
use crate::rasterizer::{perspective_transform, project};

/// Maximum number of live particles (PS1-authentic limit)
pub const MAX_PARTICLES: usize = 256;

/// A single particle in the pool
#[derive(Debug, Clone, Copy)]
pub struct Particle {
    /// World position
    pub position: Vec3,
    /// Velocity (units per second)
    pub velocity: Vec3,
    /// Remaining life in seconds
    pub life: f32,
    /// Total lifetime (for interpolation)
    pub max_life: f32,
    /// Start color (RGB 0-255)
    pub color_start: [u8; 3],
    /// End color (RGB 0-255)
    pub color_end: [u8; 3],
    /// Pixel size (1-4, 1 = single pixel)
    pub size: u8,
    /// Is this particle slot active?
    pub alive: bool,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            life: 0.0,
            max_life: 1.0,
            color_start: [255, 255, 255],
            color_end: [128, 128, 128],
            size: 1,
            alive: false,
        }
    }
}

/// Definition for a particle emitter (design-time data, stored in assets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleEmitterDef {
    /// Particles to emit per second
    pub spawn_rate: f32,
    /// Minimum initial speed
    pub speed_min: f32,
    /// Maximum initial speed
    pub speed_max: f32,
    /// Spread half-angle in radians (0 = straight up, PI = full sphere)
    pub spread: f32,
    /// Gravity multiplier (1.0 = normal gravity, 0.0 = no gravity, -1.0 = floats up)
    pub gravity: f32,
    /// Minimum particle lifetime in seconds
    pub life_min: f32,
    /// Maximum particle lifetime in seconds
    pub life_max: f32,
    /// Start color (RGB 0-255)
    pub color_start: [u8; 3],
    /// End color (RGB 0-255)
    pub color_end: [u8; 3],
    /// Pixel size (1-4)
    pub size: u8,
}

impl Default for ParticleEmitterDef {
    fn default() -> Self {
        Self {
            spawn_rate: 10.0,
            speed_min: 100.0,
            speed_max: 300.0,
            spread: 0.5,
            gravity: 1.0,
            life_min: 0.3,
            life_max: 1.0,
            color_start: [255, 200, 50],
            color_end: [200, 50, 0],
            size: 1,
        }
    }
}

/// Common particle effect presets
impl ParticleEmitterDef {
    /// Blood/hit sparks effect (red, fast, short-lived)
    pub fn blood() -> Self {
        Self {
            spawn_rate: 0.0, // Burst only
            speed_min: 200.0,
            speed_max: 600.0,
            spread: 1.0,
            gravity: 1.5,
            life_min: 0.2,
            life_max: 0.5,
            color_start: [200, 20, 20],
            color_end: [80, 0, 0],
            size: 1,
        }
    }

    /// Spark/hit effect (yellow-white, very fast)
    pub fn sparks() -> Self {
        Self {
            spawn_rate: 0.0,
            speed_min: 400.0,
            speed_max: 800.0,
            spread: 0.8,
            gravity: 0.5,
            life_min: 0.1,
            life_max: 0.3,
            color_start: [255, 255, 200],
            color_end: [255, 150, 0],
            size: 1,
        }
    }

    /// Dust/debris effect (gray, slow, medium-lived)
    pub fn dust() -> Self {
        Self {
            spawn_rate: 5.0,
            speed_min: 20.0,
            speed_max: 80.0,
            spread: std::f32::consts::PI,
            gravity: -0.2,
            life_min: 0.5,
            life_max: 1.5,
            color_start: [150, 140, 130],
            color_end: [80, 75, 70],
            size: 2,
        }
    }

    /// Fire/torch effect (orange-yellow, rises)
    pub fn fire() -> Self {
        Self {
            spawn_rate: 20.0,
            speed_min: 50.0,
            speed_max: 150.0,
            spread: 0.3,
            gravity: -0.8,
            life_min: 0.3,
            life_max: 0.8,
            color_start: [255, 200, 50],
            color_end: [200, 50, 0],
            size: 2,
        }
    }
}

/// Runtime particle emitter attached to an entity
#[derive(Debug, Clone)]
pub struct ParticleEmitter {
    /// The emitter definition (what kind of particles)
    pub def: ParticleEmitterDef,
    /// Accumulated time for spawn rate (fractional particle)
    pub spawn_accumulator: f32,
    /// Is this emitter currently active?
    pub active: bool,
}

impl ParticleEmitter {
    pub fn new(def: ParticleEmitterDef) -> Self {
        Self {
            def,
            spawn_accumulator: 0.0,
            active: true,
        }
    }
}

/// The particle pool — manages all live particles
pub struct ParticlePool {
    pub particles: [Particle; MAX_PARTICLES],
    /// Simple PRNG state for randomization
    rng_state: u32,
}

impl ParticlePool {
    pub fn new() -> Self {
        Self {
            particles: [Particle::default(); MAX_PARTICLES],
            rng_state: 12345,
        }
    }

    /// Fast xorshift PRNG (no external deps, deterministic)
    fn next_random(&mut self) -> f32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 17;
        self.rng_state ^= self.rng_state << 5;
        (self.rng_state as f32) / (u32::MAX as f32)
    }

    /// Random float in range [min, max]
    fn random_range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_random() * (max - min)
    }

    /// Find a dead particle slot
    fn find_free_slot(&self) -> Option<usize> {
        self.particles.iter().position(|p| !p.alive)
    }

    /// Spawn a single particle from an emitter definition at a world position
    pub fn spawn_one(&mut self, def: &ParticleEmitterDef, origin: Vec3) {
        if let Some(idx) = self.find_free_slot() {
            let speed = self.random_range(def.speed_min, def.speed_max);
            let life = self.random_range(def.life_min, def.life_max);

            // Generate random direction within spread cone (pointing up by default)
            let theta = self.random_range(0.0, std::f32::consts::TAU); // Azimuth
            let phi = self.random_range(0.0, def.spread); // Elevation from up vector

            let sin_phi = phi.sin();
            let velocity = Vec3::new(
                sin_phi * theta.cos() * speed,
                phi.cos() * speed,
                sin_phi * theta.sin() * speed,
            );

            self.particles[idx] = Particle {
                position: origin,
                velocity,
                life,
                max_life: life,
                color_start: def.color_start,
                color_end: def.color_end,
                size: def.size.max(1).min(4),
                alive: true,
            };
        }
    }

    /// Spawn a burst of particles (for one-shot effects like hits)
    pub fn spawn_burst(&mut self, def: &ParticleEmitterDef, origin: Vec3, count: usize) {
        for _ in 0..count {
            self.spawn_one(def, origin);
        }
    }

    /// Update all live particles
    pub fn update(&mut self, delta_time: f32, gravity: f32) {
        for particle in &mut self.particles {
            if !particle.alive {
                continue;
            }

            particle.life -= delta_time;
            if particle.life <= 0.0 {
                particle.alive = false;
                continue;
            }

            // Apply gravity
            particle.velocity.y -= gravity * delta_time;

            // Integrate position
            particle.position = particle.position + particle.velocity * delta_time;
        }
    }

    /// Render all live particles to the framebuffer
    pub fn render(&self, fb: &mut Framebuffer, camera: &Camera) {
        for particle in &self.particles {
            if !particle.alive {
                continue;
            }

            // Interpolation factor (0 = just spawned, 1 = about to die)
            let t = 1.0 - (particle.life / particle.max_life);

            // Lerp color
            let r = lerp_u8(particle.color_start[0], particle.color_end[0], t);
            let g = lerp_u8(particle.color_start[1], particle.color_end[1], t);
            let b = lerp_u8(particle.color_start[2], particle.color_end[2], t);
            let color = Color::new(r, g, b);

            // Project to screen space
            let rel = particle.position - camera.position;
            let cam_space = perspective_transform(rel, camera.basis_x, camera.basis_y, camera.basis_z);

            // Behind camera check
            if cam_space.z < 0.1 {
                continue;
            }

            let screen = project(cam_space, fb.width, fb.height);
            let sx = screen.x as i32;
            let sy = screen.y as i32;
            let depth = cam_space.z;

            // Draw pixel(s) based on size
            let size = particle.size as i32;
            if size <= 1 {
                // Single pixel
                if sx >= 0 && sx < fb.width as i32 && sy >= 0 && sy < fb.height as i32 {
                    fb.set_pixel_with_depth(sx as usize, sy as usize, depth, color);
                }
            } else {
                // Small square (2x2, 3x3, or 4x4)
                let half = size / 2;
                for dy in -half..=(size - half - 1) {
                    for dx in -half..=(size - half - 1) {
                        let px = sx + dx;
                        let py = sy + dy;
                        if px >= 0 && px < fb.width as i32 && py >= 0 && py < fb.height as i32 {
                            fb.set_pixel_with_depth(px as usize, py as usize, depth, color);
                        }
                    }
                }
            }
        }
    }

    /// Get count of live particles
    pub fn alive_count(&self) -> usize {
        self.particles.iter().filter(|p| p.alive).count()
    }

    /// Kill all particles
    pub fn clear(&mut self) {
        for p in &mut self.particles {
            p.alive = false;
        }
    }
}

impl Default for ParticlePool {
    fn default() -> Self {
        Self::new()
    }
}

/// Lerp between two u8 values
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let result = a as f32 * (1.0 - t) + b as f32 * t;
    result.clamp(0.0, 255.0) as u8
}
