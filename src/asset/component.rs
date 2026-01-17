//! Asset Component Definitions
//!
//! Components that can be attached to an asset. These are "templates" that
//! spawn into runtime ECS components when the asset is instantiated.
//!
//! The key principle: Mesh is just another component with embedded data,
//! not a special field. This enables mesh-less assets (pure triggers, lights, etc.)

use serde::{Deserialize, Serialize};
use crate::modeler::MeshPart;
use crate::game::components::{EnemyType, ItemType};

/// Components that can be attached to an asset
///
/// These are design-time definitions that get converted to runtime ECS components
/// when the asset is spawned into the game world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssetComponent {
    /// 3D mesh - EMBEDDED data, not a file reference
    ///
    /// Contains full geometry + TextureRef::Id pointing to shared textures.
    /// This is the visual representation of the asset.
    Mesh {
        /// Mesh parts (each with geometry + texture reference)
        parts: Vec<MeshPart>,
    },

    /// Collision shape for physics
    ///
    /// Defines how the asset interacts with physics and other entities.
    Collision {
        /// The collision shape definition
        shape: CollisionShapeDef,
        /// If true, this is a trigger zone (pass-through, fires events)
        /// If false, this is a solid collider (blocks movement)
        #[serde(default)]
        is_trigger: bool,
    },

    /// Point light attached to asset
    ///
    /// For torch-holding enemies, glowing pickups, etc.
    Light {
        /// RGB color (0-255)
        color: [u8; 3],
        /// Light intensity multiplier
        intensity: f32,
        /// Light falloff radius in world units
        radius: f32,
        /// Offset from asset origin
        #[serde(default)]
        offset: [f32; 3],
    },

    /// Trigger zone for scripting events
    ///
    /// Fires events when entities enter or exit the zone.
    Trigger {
        /// Unique identifier for this trigger
        trigger_id: String,
        /// Event name to fire on enter (if any)
        #[serde(default)]
        on_enter: Option<String>,
        /// Event name to fire on exit (if any)
        #[serde(default)]
        on_exit: Option<String>,
    },

    /// Collectible item
    ///
    /// Health pickups, keys, currency, upgrades.
    Pickup {
        /// Type of item this pickup represents
        item_type: ItemType,
        /// Respawn time in seconds (None = doesn't respawn)
        #[serde(default)]
        respawn_time: Option<f32>,
    },

    /// Enemy definition
    ///
    /// Defines enemy behavior, stats, and combat properties.
    Enemy {
        /// Type of enemy (affects AI behavior)
        enemy_type: EnemyType,
        /// Starting health points
        health: i32,
        /// Base damage dealt
        damage: i32,
        /// Patrol radius in world units (for AI)
        #[serde(default)]
        patrol_radius: f32,
    },

    /// Checkpoint / save point
    ///
    /// Where the player respawns after death.
    Checkpoint {
        /// Offset from checkpoint position for player respawn
        #[serde(default)]
        respawn_offset: [f32; 3],
    },

    /// Interactive door
    ///
    /// Can be locked, requiring a key to open.
    Door {
        /// Key required to open (None = unlocked)
        #[serde(default)]
        required_key: Option<String>,
        /// Whether the door starts in the open state
        #[serde(default)]
        start_open: bool,
    },

    /// Audio source
    ///
    /// Ambient sounds, music zones, sound effects.
    Audio {
        /// Sound file or identifier
        sound: String,
        /// Volume multiplier (0.0 - 1.0)
        #[serde(default = "default_volume")]
        volume: f32,
        /// Falloff radius in world units
        radius: f32,
        /// Whether the sound loops
        #[serde(default)]
        looping: bool,
    },

    /// Particle emitter
    ///
    /// Smoke, fire, sparkles, etc.
    Particle {
        /// Particle effect identifier
        effect: String,
        /// Offset from asset origin
        #[serde(default)]
        offset: [f32; 3],
    },

    /// Character controller for movement
    ///
    /// For player or NPC movement with collision.
    CharacterController {
        /// Character height for collision
        height: f32,
        /// Collision cylinder radius
        radius: f32,
        /// Maximum step-up height
        #[serde(default = "default_step_height")]
        step_height: f32,
    },

    /// Spawn point for player or NPCs
    ///
    /// Defines where entities can spawn in the level.
    SpawnPoint {
        /// True for player start position, false for NPC/enemy spawns
        #[serde(default)]
        is_player_start: bool,
    },
}

fn default_volume() -> f32 {
    1.0
}

fn default_step_height() -> f32 {
    384.0 // Default from game::components::character
}

impl AssetComponent {
    /// Get a human-readable name for this component type
    pub fn type_name(&self) -> &'static str {
        match self {
            AssetComponent::Mesh { .. } => "Mesh",
            AssetComponent::Collision { .. } => "Collision",
            AssetComponent::Light { .. } => "Light",
            AssetComponent::Trigger { .. } => "Trigger",
            AssetComponent::Pickup { .. } => "Pickup",
            AssetComponent::Enemy { .. } => "Enemy",
            AssetComponent::Checkpoint { .. } => "Checkpoint",
            AssetComponent::Door { .. } => "Door",
            AssetComponent::Audio { .. } => "Audio",
            AssetComponent::Particle { .. } => "Particle",
            AssetComponent::CharacterController { .. } => "CharacterController",
            AssetComponent::SpawnPoint { .. } => "SpawnPoint",
        }
    }

    /// Get an icon character for this component type (for UI)
    pub fn icon(&self) -> char {
        match self {
            AssetComponent::Mesh { .. } => '\u{E834}', // cube icon
            AssetComponent::Collision { .. } => '\u{E835}', // box icon
            AssetComponent::Light { .. } => '\u{E90F}', // lightbulb icon
            AssetComponent::Trigger { .. } => '\u{E8B8}', // flag icon
            AssetComponent::Pickup { .. } => '\u{E838}', // star icon
            AssetComponent::Enemy { .. } => '\u{E87C}', // skull icon
            AssetComponent::Checkpoint { .. } => '\u{E153}', // flag icon
            AssetComponent::Door { .. } => '\u{E88A}', // door icon
            AssetComponent::Audio { .. } => '\u{E050}', // speaker icon
            AssetComponent::Particle { .. } => '\u{E3A5}', // sparkle icon
            AssetComponent::CharacterController { .. } => '\u{E7FD}', // person icon
            AssetComponent::SpawnPoint { .. } => '\u{E566}', // location icon
        }
    }

    /// Check if this is a Mesh component
    pub fn is_mesh(&self) -> bool {
        matches!(self, AssetComponent::Mesh { .. })
    }

    /// Check if this is a Collision component
    pub fn is_collision(&self) -> bool {
        matches!(self, AssetComponent::Collision { .. })
    }

    /// Check if this is a Light component
    pub fn is_light(&self) -> bool {
        matches!(self, AssetComponent::Light { .. })
    }

    /// Check if this is an Enemy component
    pub fn is_enemy(&self) -> bool {
        matches!(self, AssetComponent::Enemy { .. })
    }

    /// Check if this is a Checkpoint component
    pub fn is_checkpoint(&self) -> bool {
        matches!(self, AssetComponent::Checkpoint { .. })
    }

    /// Check if this is a SpawnPoint component
    pub fn is_spawn_point(&self) -> bool {
        matches!(self, AssetComponent::SpawnPoint { .. })
    }
}

/// Collision shape definition for assets
///
/// These are design-time definitions that get converted to runtime
/// CollisionShape when the asset is spawned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollisionShapeDef {
    /// Sphere with radius (simplest, good for most entities)
    Sphere { radius: f32 },
    /// Axis-aligned box with half-extents
    Box { half_extents: [f32; 3] },
    /// Capsule (cylinder with sphere caps) - good for humanoids
    Capsule { radius: f32, height: f32 },
    /// Cylinder (flat top/bottom)
    Cylinder { radius: f32, height: f32 },
    /// Auto-generate from mesh bounds (computed at load time)
    FromMesh,
}

impl CollisionShapeDef {
    /// Create a sphere collision shape
    pub fn sphere(radius: f32) -> Self {
        CollisionShapeDef::Sphere { radius }
    }

    /// Create a box collision shape
    pub fn box_shape(half_x: f32, half_y: f32, half_z: f32) -> Self {
        CollisionShapeDef::Box {
            half_extents: [half_x, half_y, half_z],
        }
    }

    /// Create a capsule collision shape
    pub fn capsule(radius: f32, height: f32) -> Self {
        CollisionShapeDef::Capsule { radius, height }
    }

    /// Create a cylinder collision shape
    pub fn cylinder(radius: f32, height: f32) -> Self {
        CollisionShapeDef::Cylinder { radius, height }
    }

    /// Get a human-readable description of this shape
    pub fn description(&self) -> String {
        match self {
            CollisionShapeDef::Sphere { radius } => format!("Sphere (r={:.0})", radius),
            CollisionShapeDef::Box { half_extents } => {
                format!("Box ({:.0}x{:.0}x{:.0})",
                    half_extents[0] * 2.0,
                    half_extents[1] * 2.0,
                    half_extents[2] * 2.0
                )
            }
            CollisionShapeDef::Capsule { radius, height } => {
                format!("Capsule (r={:.0}, h={:.0})", radius, height)
            }
            CollisionShapeDef::Cylinder { radius, height } => {
                format!("Cylinder (r={:.0}, h={:.0})", radius, height)
            }
            CollisionShapeDef::FromMesh => "From Mesh".to_string(),
        }
    }
}

impl Default for CollisionShapeDef {
    fn default() -> Self {
        // Default to auto-compute from mesh
        CollisionShapeDef::FromMesh
    }
}
