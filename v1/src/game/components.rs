//! Game Components
//!
//! All the component types used in souls-like and metroidvania games.
//! Components are plain data structs - behavior lives in systems.

use serde::{Serialize, Deserialize};
use crate::rasterizer::Vec3;
use super::entity::Entity;

// =============================================================================
// Physics / Movement
// =============================================================================

/// Velocity component for moving entities
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Velocity(pub Vec3);

impl Velocity {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self(Vec3::new(x, y, z))
    }
}

// =============================================================================
// Character Controller (TR-style cylinder collision)
// =============================================================================

/// Character controller constants (TR-style, scaled to our 1024-unit sectors)
pub mod character {
    /// Player collision cylinder radius (about 1/10 of a sector)
    pub const PLAYER_RADIUS: f32 = 100.0;
    /// Player height (about 3/4 of a sector)
    pub const PLAYER_HEIGHT: f32 = 762.0;
    /// Maximum step-up height (about 1.5 "clicks" or 384 units)
    pub const STEP_HEIGHT: f32 = 384.0;
    /// Gravity acceleration (units per second squared)
    pub const GRAVITY: f32 = 2400.0;
    /// Terminal velocity (max falling speed)
    pub const TERMINAL_VELOCITY: f32 = 4000.0;
    /// Walk speed (units per second)
    pub const WALK_SPEED: f32 = 800.0;
    /// Run speed (units per second)
    pub const RUN_SPEED: f32 = 1600.0;
}

/// Character controller component for TR-style cylinder collision
///
/// The character is modeled as a vertical cylinder that collides with
/// sector-based floor/ceiling geometry. Like OpenLara, we check 4 points
/// around the cylinder for wall collision.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CharacterController {
    /// Cylinder collision radius
    pub radius: f32,
    /// Character height (for ceiling collision)
    pub height: f32,
    /// Maximum height character can step up
    pub step_height: f32,
    /// Is the character currently on the ground?
    pub grounded: bool,
    /// Current room index (for sector lookups)
    pub current_room: usize,
    /// Vertical velocity (for gravity/jumping)
    pub vertical_velocity: f32,
    /// Facing direction (yaw in radians)
    pub facing: f32,
}

impl CharacterController {
    /// Create a player-sized character controller
    pub fn player() -> Self {
        Self {
            radius: character::PLAYER_RADIUS,
            height: character::PLAYER_HEIGHT,
            step_height: character::STEP_HEIGHT,
            grounded: false,
            current_room: 0,
            vertical_velocity: 0.0,
            facing: 0.0,
        }
    }

    /// Create a character controller with custom dimensions
    pub fn new(radius: f32, height: f32) -> Self {
        Self {
            radius,
            height,
            step_height: character::STEP_HEIGHT,
            grounded: false,
            current_room: 0,
            vertical_velocity: 0.0,
            facing: 0.0,
        }
    }
}

// =============================================================================
// Combat Components
// =============================================================================

/// Health component for damageable entities
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Health {
    pub current: i32,
    pub max: i32,
    /// Invincibility frames remaining (for i-frames after hit)
    pub invincible_frames: u8,
}

impl Health {
    pub fn new(max: i32) -> Self {
        Self {
            current: max,
            max,
            invincible_frames: 0,
        }
    }

    pub fn damage(&mut self, amount: i32) -> bool {
        if self.invincible_frames > 0 {
            return false; // No damage during i-frames
        }
        self.current = (self.current - amount).max(0);
        self.current == 0
    }

    pub fn heal(&mut self, amount: i32) {
        self.current = (self.current + amount).min(self.max);
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0
    }

    pub fn set_invincible(&mut self, frames: u8) {
        self.invincible_frames = frames;
    }

    pub fn tick_invincibility(&mut self) {
        self.invincible_frames = self.invincible_frames.saturating_sub(1);
    }
}

/// Hitbox - an area that deals damage (weapon, projectile)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Hitbox {
    pub shape: CollisionShape,
    pub damage: i32,
    /// Which "team" this hitbox belongs to (to prevent friendly fire)
    pub team: Team,
    /// Is this hitbox currently active? (for attack windups)
    pub active: bool,
}

impl Hitbox {
    pub fn sphere(radius: f32) -> Self {
        Self {
            shape: CollisionShape::Sphere { radius },
            damage: 0,
            team: Team::Neutral,
            active: true,
        }
    }

    pub fn with_damage(mut self, damage: i32) -> Self {
        self.damage = damage;
        self
    }

    pub fn with_team(mut self, team: Team) -> Self {
        self.team = team;
        self
    }
}

/// Hurtbox - an area that can receive damage (body, weak point)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Hurtbox {
    pub shape: CollisionShape,
    /// Damage multiplier (2.0 for weak points, 0.5 for armored areas)
    pub damage_multiplier: f32,
}

impl Hurtbox {
    pub fn sphere(radius: f32) -> Self {
        Self {
            shape: CollisionShape::Sphere { radius },
            damage_multiplier: 1.0,
        }
    }

    pub fn with_multiplier(mut self, multiplier: f32) -> Self {
        self.damage_multiplier = multiplier;
        self
    }
}

/// Simple collision shapes for hitboxes/hurtboxes
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CollisionShape {
    /// Sphere with radius (simplest, good for most entities)
    Sphere { radius: f32 },
    /// Axis-aligned box (for doors, platforms)
    Box { half_extents: Vec3 },
    /// Capsule (for humanoid characters) - cylinder with sphere caps
    Capsule { radius: f32, height: f32 },
}

/// Team affiliation for damage filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Team {
    Player,
    Enemy,
    Neutral, // Damages everyone (traps, environmental hazards)
}

// =============================================================================
// Entity Type Markers
// =============================================================================

/// Marks the player entity
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Player;

/// Marks enemy entities
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Enemy {
    pub enemy_type: EnemyType,
}

/// Types of enemies for behavior differentiation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnemyType {
    /// Basic melee enemy
    Grunt,
    /// Ranged attacker
    Archer,
    /// Tough, slow enemy
    Heavy,
    /// Fast, weak enemy
    Swarm,
    /// Mini-boss
    Elite,
    /// Full boss
    Boss,
}

/// Marks projectile entities
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Projectile {
    /// Who fired this projectile (for damage attribution, friendly fire prevention)
    pub owner: Entity,
    /// Base damage
    pub damage: i32,
}

/// Marks collectible items
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Item {
    pub item_type: ItemType,
}

/// Types of collectible items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemType {
    /// Heals the player
    HealthPickup { amount: i32 },
    /// Currency (souls, geo, etc.)
    Currency { amount: i32 },
    /// Key item for progression
    Key(KeyType),
    /// Permanent upgrade
    Upgrade,
}

// =============================================================================
// World Interaction Components
// =============================================================================

/// Door or gate that can be opened
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Door {
    pub is_open: bool,
    /// If Some, requires this key type to open
    pub required_key: Option<KeyType>,
}

impl Door {
    pub fn locked(key: KeyType) -> Self {
        Self {
            is_open: false,
            required_key: Some(key),
        }
    }

    pub fn unlocked() -> Self {
        Self {
            is_open: false,
            required_key: None,
        }
    }
}

/// Key types for lock-and-key progression
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyType {
    /// Generic keys (Resident Evil style)
    Generic(u32),
    /// Named keys for specific doors
    BossKey,
    MasterKey,
    /// Ability-based "keys" (metroidvania style)
    DoubleJump,
    WallClimb,
    Dash,
}

/// Key item that the player can collect
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Key {
    pub key_type: KeyType,
}

/// Checkpoint / bonfire / save point
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Has the player activated this checkpoint?
    pub is_activated: bool,
    /// Offset from checkpoint position for player respawn
    pub respawn_offset: Vec3,
}

/// Enemy spawn point (for respawning enemies on checkpoint rest)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SpawnPoint {
    /// What enemy type spawns here
    pub enemy_type: EnemyType,
    /// Health of spawned enemy
    pub enemy_health: i32,
    /// The currently spawned enemy (if alive)
    pub spawned_entity: Option<Entity>,
    /// Is this spawn point active?
    pub active: bool,
}

impl SpawnPoint {
    pub fn new(enemy_type: EnemyType, health: i32) -> Self {
        Self {
            enemy_type,
            enemy_health: health,
            spawned_entity: None,
            active: true,
        }
    }
}

// =============================================================================
// Trigger Volume Component
// =============================================================================

/// Trigger volume that fires events when entities enter/exit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    /// Unique identifier for this trigger
    pub trigger_id: String,
    /// Collision shape defining the trigger bounds
    pub shape: CollisionShape,
    /// Event name to fire on enter (if any)
    pub on_enter: Option<String>,
    /// Event name to fire on exit (if any)
    pub on_exit: Option<String>,
    /// Entities currently inside this trigger (tracked by index for enter/exit detection)
    #[serde(skip)]
    pub occupants: Vec<u32>,
}

impl Trigger {
    pub fn new(trigger_id: String, shape: CollisionShape) -> Self {
        Self {
            trigger_id,
            shape,
            on_enter: None,
            on_exit: None,
            occupants: Vec::new(),
        }
    }
}

// =============================================================================
// AI / Behavior Components (for future expansion)
// =============================================================================

/// AI state for enemies
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AiState {
    /// Idle, not aware of player
    Idle,
    /// Patrolling a route
    Patrol,
    /// Detected player, moving to engage
    Chase,
    /// In combat range, attacking
    Attack,
    /// Recovering from attack or stagger
    Recover,
    /// Fleeing (low health)
    Flee,
    /// Dead (for death animation before despawn)
    Dead,
}

impl Default for AiState {
    fn default() -> Self {
        Self::Idle
    }
}
