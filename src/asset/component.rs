use serde::{Deserialize, Serialize};

/// Components that can be attached to a placed asset instance.
/// Asset references use project-relative paths (e.g. "meshes/ghost.obj").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssetComponent {
    Mesh {
        /// Project-relative path to the .obj file
        #[serde(default)]
        mesh_path: String,
    },

    Collision {
        shape: CollisionShapeDef,
        #[serde(default)]
        is_trigger: bool,
        /// Optional project-relative path to a collision mesh
        #[serde(default)]
        collision_mesh: Option<String>,
    },

    Light {
        color: [u8; 3],
        intensity: f32,
        radius: f32,
        #[serde(default)]
        offset: [f32; 3],
    },

    Trigger {
        trigger_id: String,
        #[serde(default)]
        on_enter: Option<String>,
        #[serde(default)]
        on_exit: Option<String>,
    },

    Pickup {
        item_type: ItemType,
        #[serde(default)]
        respawn_time: Option<f32>,
    },

    Enemy {
        enemy_type: EnemyType,
        health: i32,
        damage: i32,
        #[serde(default)]
        patrol_radius: f32,
    },

    Door {
        #[serde(default)]
        required_key: Option<String>,
        #[serde(default)]
        start_open: bool,
    },

    Audio {
        /// Project-relative path to the sound file
        sound_path: String,
        #[serde(default = "default_volume")]
        volume: f32,
        radius: f32,
        #[serde(default)]
        looping: bool,
    },

    Particle {
        effect: String,
        #[serde(default)]
        offset: [f32; 3],
    },

    CharacterController {
        height: f32,
        radius: f32,
        #[serde(default = "default_step_height")]
        step_height: f32,
    },

    SpawnPoint {
        #[serde(default)]
        is_player: bool,
        #[serde(default)]
        respawns: bool,
    },

    Skeleton {
        bones: Vec<BoneDef>,
    },

    Script {
        /// Project-relative path to the .lua file
        script_path: String,
    },
}

fn default_volume() -> f32 { 1.0 }
fn default_step_height() -> f32 { 384.0 }

impl AssetComponent {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Mesh { .. }               => "Mesh",
            Self::Collision { .. }          => "Collision",
            Self::Light { .. }              => "Light",
            Self::Trigger { .. }            => "Trigger",
            Self::Pickup { .. }             => "Pickup",
            Self::Enemy { .. }              => "Enemy",
            Self::Door { .. }               => "Door",
            Self::Audio { .. }              => "Audio",
            Self::Particle { .. }           => "Particle",
            Self::CharacterController { .. } => "CharacterController",
            Self::SpawnPoint { .. }         => "SpawnPoint",
            Self::Skeleton { .. }           => "Skeleton",
            Self::Script { .. }             => "Script",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollisionShapeDef {
    Sphere { radius: f32 },
    Box { half_extents: [f32; 3] },
    Capsule { radius: f32, height: f32 },
    Cylinder { radius: f32, height: f32 },
    FromMesh,
}

impl Default for CollisionShapeDef {
    fn default() -> Self { Self::FromMesh }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnemyType { Grunt, Archer, Heavy, Swarm, Elite }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemType {
    HealthPickup { amount: i32 },
    Currency { amount: i32 },
    Key { key_id: String },
    Upgrade,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoneDef {
    pub name: String,
    pub parent: Option<usize>,
    pub local_position: [f32; 3],
    pub local_rotation: [f32; 3],
    pub length: f32,
}
