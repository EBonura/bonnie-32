//! Game World
//!
//! The World is the central container for all game state:
//! - Entity allocation and lifetime tracking
//! - Component storage for all component types
//! - Deferred entity despawn (to avoid iterator invalidation)
//!
//! Unlike Bevy which uses runtime type registration, we define all
//! component types at compile time. This is simpler and sufficient
//! for our known game requirements.

use super::entity::{Entity, EntityAllocator};
use super::component::ComponentStorage;
use super::transform::{Transform, GlobalTransform};
use super::components::*;
use crate::rasterizer::Vec3;

/// The game world containing all entities and their components.
///
/// Components are stored in typed fields rather than a HashMap<TypeId, ...>
/// because we know exactly what components we need at compile time.
pub struct World {
    /// Entity allocator for creating/destroying entities
    entities: EntityAllocator,

    /// Entities queued for despawn at end of frame
    despawn_queue: Vec<Entity>,

    // =========================================================================
    // Core Components (every game needs these)
    // =========================================================================

    /// Local transform (position, rotation, scale relative to parent)
    pub transforms: ComponentStorage<Transform>,

    /// Computed world-space transform
    pub global_transforms: ComponentStorage<GlobalTransform>,

    /// Parent entity for hierarchy
    pub parents: ComponentStorage<Entity>,

    /// Children entities for hierarchy
    pub children: ComponentStorage<Vec<Entity>>,

    // =========================================================================
    // Gameplay Components
    // =========================================================================

    /// Velocity for moving entities
    pub velocities: ComponentStorage<Velocity>,

    /// Character controller (TR-style cylinder collision)
    pub controllers: ComponentStorage<CharacterController>,

    /// Health and damage tracking
    pub health: ComponentStorage<Health>,

    /// Hitbox for collision/damage
    pub hitboxes: ComponentStorage<Hitbox>,

    /// Hurtbox (area that can receive damage)
    pub hurtboxes: ComponentStorage<Hurtbox>,

    // =========================================================================
    // Entity Type Markers (zero-sized, just for identification)
    // =========================================================================

    /// Marks the player entity
    pub players: ComponentStorage<Player>,

    /// Marks enemy entities
    pub enemies: ComponentStorage<Enemy>,

    /// Marks projectile entities
    pub projectiles: ComponentStorage<Projectile>,

    /// Marks collectible items
    pub items: ComponentStorage<Item>,

    // =========================================================================
    // World Interaction Components
    // =========================================================================

    /// Doors and gates that can be opened
    pub doors: ComponentStorage<Door>,

    /// Keys and items that unlock doors
    pub keys: ComponentStorage<Key>,

    /// Respawn points / bonfires / checkpoints
    pub checkpoints: ComponentStorage<Checkpoint>,

    /// Spawn points for enemies (for respawn on rest)
    pub spawn_points: ComponentStorage<SpawnPoint>,

    /// Trigger volumes for area-based events
    pub triggers: ComponentStorage<Trigger>,

    /// Particle emitters attached to entities
    pub emitters: ComponentStorage<super::particles::ParticleEmitter>,
}

impl World {
    /// Create a new empty world.
    pub fn new() -> Self {
        Self {
            entities: EntityAllocator::new(),
            despawn_queue: Vec::new(),

            // Core
            transforms: ComponentStorage::new(),
            global_transforms: ComponentStorage::new(),
            parents: ComponentStorage::new(),
            children: ComponentStorage::new(),

            // Gameplay
            velocities: ComponentStorage::new(),
            controllers: ComponentStorage::new(),
            health: ComponentStorage::new(),
            hitboxes: ComponentStorage::new(),
            hurtboxes: ComponentStorage::new(),

            // Markers
            players: ComponentStorage::new(),
            enemies: ComponentStorage::new(),
            projectiles: ComponentStorage::new(),
            items: ComponentStorage::new(),

            // World interaction
            doors: ComponentStorage::new(),
            keys: ComponentStorage::new(),
            checkpoints: ComponentStorage::new(),
            spawn_points: ComponentStorage::new(),
            triggers: ComponentStorage::new(),
            emitters: ComponentStorage::new(),
        }
    }

    // =========================================================================
    // Entity Management
    // =========================================================================

    /// Spawn a new entity with just a transform.
    /// Returns the entity ID for adding more components.
    pub fn spawn(&mut self) -> Entity {
        let entity = self.entities.allocate();
        // Every entity gets a transform by default
        self.transforms.insert(entity, Transform::default());
        self.global_transforms.insert(entity, GlobalTransform::default());
        entity
    }

    /// Spawn a new entity at a specific position.
    pub fn spawn_at(&mut self, position: Vec3) -> Entity {
        let entity = self.entities.allocate();
        self.transforms.insert(entity, Transform::from_position(position));
        self.global_transforms.insert(entity, GlobalTransform::from_position(position));
        entity
    }

    /// Queue an entity for despawn at end of frame.
    /// This is safer than immediate despawn during iteration.
    pub fn despawn(&mut self, entity: Entity) {
        if self.is_alive(entity) {
            self.despawn_queue.push(entity);
        }
    }

    /// Immediately despawn an entity and all its components.
    /// Prefer `despawn()` during gameplay to avoid iterator issues.
    pub fn despawn_immediate(&mut self, entity: Entity) {
        if !self.entities.free(entity) {
            return; // Already dead
        }

        let idx = entity.index();

        // Remove from parent's children list
        if let Some(parent) = self.parents.remove(entity) {
            if let Some(siblings) = self.children.get_mut(parent) {
                siblings.retain(|&e| e != entity);
            }
        }

        // Recursively despawn children
        if let Some(child_list) = self.children.remove(entity) {
            for child in child_list {
                self.despawn_immediate(child);
            }
        }

        // Clear all component slots for this entity
        self.transforms.clear_slot(idx);
        self.global_transforms.clear_slot(idx);
        self.velocities.clear_slot(idx);
        self.controllers.clear_slot(idx);
        self.health.clear_slot(idx);
        self.hitboxes.clear_slot(idx);
        self.hurtboxes.clear_slot(idx);
        self.players.clear_slot(idx);
        self.enemies.clear_slot(idx);
        self.projectiles.clear_slot(idx);
        self.items.clear_slot(idx);
        self.doors.clear_slot(idx);
        self.keys.clear_slot(idx);
        self.checkpoints.clear_slot(idx);
        self.spawn_points.clear_slot(idx);
        self.triggers.clear_slot(idx);
        self.emitters.clear_slot(idx);
    }

    /// Process all queued despawns. Call at end of frame.
    pub fn flush_despawns(&mut self) {
        let queue = std::mem::take(&mut self.despawn_queue);
        for entity in queue {
            self.despawn_immediate(entity);
        }
    }

    /// Check if an entity is currently alive.
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.entities.is_alive(entity)
    }

    /// Get the number of alive entities.
    pub fn entity_count(&self) -> u32 {
        self.entities.alive_count()
    }

    // =========================================================================
    // Hierarchy Helpers
    // =========================================================================

    /// Set an entity's parent, updating both parent and children components.
    pub fn set_parent(&mut self, child: Entity, parent: Entity) {
        // Remove from old parent's children list
        if let Some(old_parent) = self.parents.get(child).copied() {
            if let Some(siblings) = self.children.get_mut(old_parent) {
                siblings.retain(|&e| e != child);
            }
        }

        // Set new parent
        self.parents.insert(child, parent);

        // Add to new parent's children list
        if let Some(children) = self.children.get_mut(parent) {
            children.push(child);
        } else {
            self.children.insert(parent, vec![child]);
        }
    }

    /// Remove an entity's parent (make it a root entity).
    pub fn remove_parent(&mut self, child: Entity) {
        if let Some(old_parent) = self.parents.remove(child) {
            if let Some(siblings) = self.children.get_mut(old_parent) {
                siblings.retain(|&e| e != child);
            }
        }
    }

    /// Get all children of an entity.
    pub fn get_children(&self, entity: Entity) -> &[Entity] {
        self.children.get(entity).map(|v| v.as_slice()).unwrap_or(&[])
    }

    // =========================================================================
    // Convenience Spawners
    // =========================================================================

    /// Spawn a player entity with standard components.
    /// Uses player settings from Level for collision dimensions.
    pub fn spawn_player(&mut self, position: Vec3, max_health: i32, settings: &crate::world::PlayerSettings) -> Entity {
        let entity = self.spawn_at(position);
        self.players.insert(entity, Player);
        // Create controller with settings from Level
        let mut controller = CharacterController::new(settings.radius, settings.height);
        controller.step_height = settings.step_height;
        self.controllers.insert(entity, controller);
        self.health.insert(entity, Health::new(max_health));
        self.velocities.insert(entity, Velocity::default());
        self.hurtboxes.insert(entity, Hurtbox::sphere(settings.radius));
        entity
    }

    /// Spawn an enemy entity.
    pub fn spawn_enemy(&mut self, position: Vec3, max_health: i32, enemy_type: EnemyType) -> Entity {
        let entity = self.spawn_at(position);
        self.enemies.insert(entity, Enemy { enemy_type });
        self.health.insert(entity, Health::new(max_health));
        self.velocities.insert(entity, Velocity::default());
        self.hurtboxes.insert(entity, Hurtbox::sphere(1.0));
        entity
    }

    /// Spawn a projectile entity.
    pub fn spawn_projectile(&mut self, position: Vec3, velocity: Vec3, damage: i32, owner: Entity) -> Entity {
        let entity = self.spawn_at(position);
        self.projectiles.insert(entity, Projectile { owner, damage });
        self.velocities.insert(entity, Velocity(velocity));
        self.hitboxes.insert(entity, Hitbox::sphere(0.5));
        entity
    }

    /// Spawn a door entity.
    pub fn spawn_door(&mut self, position: Vec3, required_key: Option<KeyType>) -> Entity {
        let entity = self.spawn_at(position);
        self.doors.insert(entity, Door {
            is_open: false,
            required_key,
        });
        entity
    }

    /// Spawn a checkpoint/bonfire.
    pub fn spawn_checkpoint(&mut self, position: Vec3) -> Entity {
        let entity = self.spawn_at(position);
        self.checkpoints.insert(entity, Checkpoint {
            is_activated: false,
            respawn_offset: Vec3::new(0.0, 1.0, 0.0),
        });
        entity
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_and_despawn() {
        let mut world = World::new();

        let e1 = world.spawn();
        let e2 = world.spawn();
        assert_eq!(world.entity_count(), 2);

        world.despawn_immediate(e1);
        assert_eq!(world.entity_count(), 1);
        assert!(!world.is_alive(e1));
        assert!(world.is_alive(e2));
    }

    #[test]
    fn test_hierarchy_despawn() {
        let mut world = World::new();

        let parent = world.spawn();
        let child1 = world.spawn();
        let child2 = world.spawn();

        world.set_parent(child1, parent);
        world.set_parent(child2, parent);

        assert_eq!(world.entity_count(), 3);

        // Despawning parent should despawn children
        world.despawn_immediate(parent);
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn test_spawn_player() {
        let mut world = World::new();
        let settings = crate::world::PlayerSettings::default();
        let player = world.spawn_player(Vec3::new(0.0, 0.0, 0.0), 100, &settings);

        assert!(world.players.contains(player));
        assert!(world.health.contains(player));
        assert_eq!(world.health.get(player).unwrap().current, 100);
    }
}
