//! Entity System with Generational Indices
//!
//! Entities are lightweight identifiers that reference game objects.
//! The generational index pattern prevents dangling references:
//! - Each entity slot has a generation counter
//! - When an entity is despawned, its slot can be reused
//! - The generation increments on reuse, invalidating old references
//!
//! This is critical for souls-likes: a reference to a dead enemy
//! won't accidentally match a new enemy that reused the slot.

use serde::{Serialize, Deserialize};

/// A unique identifier for a game entity.
///
/// Consists of an index (which slot in the entity array) and a generation
/// (which version of that slot). Two entities with the same index but
/// different generations are different entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Entity {
    /// Index into the entity storage
    index: u32,
    /// Generation counter - increments when slot is reused
    generation: u32,
}

impl Entity {
    /// Create a new entity with the given index and generation.
    /// Should only be called by EntityAllocator.
    pub(crate) fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Get the index of this entity (for component array access).
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Get the generation of this entity.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// A null/invalid entity reference.
    /// Useful for "no target" or uninitialized fields.
    pub const NULL: Entity = Entity { index: u32::MAX, generation: 0 };

    /// Check if this is the null entity.
    pub fn is_null(&self) -> bool {
        self.index == u32::MAX
    }
}

impl Default for Entity {
    fn default() -> Self {
        Entity::NULL
    }
}

/// Allocates and tracks entity lifetimes.
///
/// Manages a pool of entity slots, reusing freed slots with incremented
/// generations to prevent dangling references.
pub struct EntityAllocator {
    /// Generation counter for each slot
    generations: Vec<u32>,
    /// Free slots available for reuse (LIFO for cache friendliness)
    free_indices: Vec<u32>,
    /// Next fresh index if no free slots available
    next_fresh: u32,
    /// Number of currently alive entities
    alive_count: u32,
}

impl EntityAllocator {
    /// Create a new allocator with no entities.
    pub fn new() -> Self {
        Self {
            generations: Vec::new(),
            free_indices: Vec::new(),
            next_fresh: 0,
            alive_count: 0,
        }
    }

    /// Create a new allocator with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            generations: Vec::with_capacity(capacity),
            free_indices: Vec::new(),
            next_fresh: 0,
            alive_count: 0,
        }
    }

    /// Allocate a new entity.
    pub fn allocate(&mut self) -> Entity {
        self.alive_count += 1;

        if let Some(index) = self.free_indices.pop() {
            // Reuse a freed slot - generation was already incremented on free
            Entity::new(index, self.generations[index as usize])
        } else {
            // Allocate a fresh slot
            let index = self.next_fresh;
            self.next_fresh += 1;
            self.generations.push(0);
            Entity::new(index, 0)
        }
    }

    /// Free an entity, making its slot available for reuse.
    /// Returns true if the entity was alive and is now freed.
    pub fn free(&mut self, entity: Entity) -> bool {
        if !self.is_alive(entity) {
            return false;
        }

        // Increment generation to invalidate existing references
        self.generations[entity.index as usize] += 1;
        self.free_indices.push(entity.index);
        self.alive_count -= 1;
        true
    }

    /// Check if an entity is currently alive.
    pub fn is_alive(&self, entity: Entity) -> bool {
        if entity.is_null() {
            return false;
        }
        let idx = entity.index as usize;
        idx < self.generations.len() && self.generations[idx] == entity.generation
    }

    /// Get the number of currently alive entities.
    pub fn alive_count(&self) -> u32 {
        self.alive_count
    }

    /// Get the total capacity (highest index ever allocated + 1).
    pub fn capacity(&self) -> u32 {
        self.next_fresh
    }

    /// Clear all entities, resetting the allocator.
    pub fn clear(&mut self) {
        // Increment all generations to invalidate existing references
        for gen in &mut self.generations {
            *gen += 1;
        }
        self.free_indices.clear();
        // Add all indices back to free list
        for i in 0..self.next_fresh {
            self.free_indices.push(i);
        }
        self.alive_count = 0;
    }
}

impl Default for EntityAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_and_free() {
        let mut alloc = EntityAllocator::new();

        let e1 = alloc.allocate();
        let e2 = alloc.allocate();
        assert_eq!(alloc.alive_count(), 2);
        assert!(alloc.is_alive(e1));
        assert!(alloc.is_alive(e2));

        alloc.free(e1);
        assert_eq!(alloc.alive_count(), 1);
        assert!(!alloc.is_alive(e1));
        assert!(alloc.is_alive(e2));
    }

    #[test]
    fn test_generation_prevents_reuse_collision() {
        let mut alloc = EntityAllocator::new();

        let e1 = alloc.allocate();
        let old_gen = e1.generation();
        alloc.free(e1);

        // Allocate again - should reuse slot 0 but with new generation
        let e2 = alloc.allocate();
        assert_eq!(e2.index(), e1.index()); // Same slot
        assert_ne!(e2.generation(), old_gen); // Different generation

        // Old reference is no longer valid
        assert!(!alloc.is_alive(e1));
        assert!(alloc.is_alive(e2));
    }

    #[test]
    fn test_null_entity() {
        let alloc = EntityAllocator::new();
        assert!(!alloc.is_alive(Entity::NULL));
        assert!(Entity::NULL.is_null());
    }
}
