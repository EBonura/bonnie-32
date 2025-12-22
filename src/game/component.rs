//! Component Storage
//!
//! Components are plain data attached to entities. This module provides
//! `ComponentStorage<T>` - a sparse array that maps entity indices to
//! component data.
//!
//! Unlike Bevy's archetype system (which groups entities by component sets),
//! we use simple sparse storage. For PS1-scale games (hundreds of entities),
//! the simpler approach is fine and easier to reason about.

use super::entity::Entity;

/// Sparse storage for a single component type.
///
/// Uses Option<T> so we can have "holes" where entities don't have
/// this component. The index is the entity's index (not generation).
pub struct ComponentStorage<T> {
    /// Sparse array indexed by entity.index()
    data: Vec<Option<T>>,
}

impl<T> ComponentStorage<T> {
    /// Create empty storage.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Create storage with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Ensure storage can hold an entity at the given index.
    fn ensure_capacity(&mut self, index: usize) {
        if index >= self.data.len() {
            self.data.resize_with(index + 1, || None);
        }
    }

    /// Insert a component for an entity.
    /// Replaces any existing component.
    pub fn insert(&mut self, entity: Entity, component: T) {
        let idx = entity.index() as usize;
        self.ensure_capacity(idx);
        self.data[idx] = Some(component);
    }

    /// Remove a component from an entity.
    /// Returns the removed component if it existed.
    pub fn remove(&mut self, entity: Entity) -> Option<T> {
        let idx = entity.index() as usize;
        if idx < self.data.len() {
            self.data[idx].take()
        } else {
            None
        }
    }

    /// Get a reference to an entity's component.
    pub fn get(&self, entity: Entity) -> Option<&T> {
        let idx = entity.index() as usize;
        self.data.get(idx).and_then(|opt| opt.as_ref())
    }

    /// Get a mutable reference to an entity's component.
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let idx = entity.index() as usize;
        self.data.get_mut(idx).and_then(|opt| opt.as_mut())
    }

    /// Check if an entity has this component.
    pub fn contains(&self, entity: Entity) -> bool {
        let idx = entity.index() as usize;
        idx < self.data.len() && self.data[idx].is_some()
    }

    /// Iterate over all (index, component) pairs.
    /// Note: index is u32, you'll need to validate entity is alive separately.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &T)> {
        self.data
            .iter()
            .enumerate()
            .filter_map(|(idx, opt)| opt.as_ref().map(|c| (idx as u32, c)))
    }

    /// Iterate mutably over all (index, component) pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u32, &mut T)> {
        self.data
            .iter_mut()
            .enumerate()
            .filter_map(|(idx, opt)| opt.as_mut().map(|c| (idx as u32, c)))
    }

    /// Clear the component from an entity slot.
    /// Called when an entity is despawned to clean up its components.
    pub fn clear_slot(&mut self, index: u32) {
        let idx = index as usize;
        if idx < self.data.len() {
            self.data[idx] = None;
        }
    }

    /// Clear all components.
    pub fn clear(&mut self) {
        for slot in &mut self.data {
            *slot = None;
        }
    }

    /// Get the number of entities that have this component.
    pub fn count(&self) -> usize {
        self.data.iter().filter(|opt| opt.is_some()).count()
    }
}

impl<T> Default for ComponentStorage<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut storage: ComponentStorage<i32> = ComponentStorage::new();
        let entity = Entity::new(5, 0);

        storage.insert(entity, 42);
        assert_eq!(storage.get(entity), Some(&42));
        assert!(storage.contains(entity));
    }

    #[test]
    fn test_remove() {
        let mut storage: ComponentStorage<i32> = ComponentStorage::new();
        let entity = Entity::new(3, 0);

        storage.insert(entity, 100);
        let removed = storage.remove(entity);
        assert_eq!(removed, Some(100));
        assert!(!storage.contains(entity));
    }

    #[test]
    fn test_sparse_storage() {
        let mut storage: ComponentStorage<i32> = ComponentStorage::new();

        // Insert at index 100 without filling 0-99
        let entity = Entity::new(100, 0);
        storage.insert(entity, 999);

        assert_eq!(storage.get(entity), Some(&999));
        assert!(!storage.contains(Entity::new(50, 0)));
    }

    #[test]
    fn test_iteration() {
        let mut storage: ComponentStorage<&str> = ComponentStorage::new();

        storage.insert(Entity::new(0, 0), "zero");
        storage.insert(Entity::new(2, 0), "two");
        storage.insert(Entity::new(5, 0), "five");

        let items: Vec<_> = storage.iter().collect();
        assert_eq!(items.len(), 3);
        assert!(items.contains(&(0, &"zero")));
        assert!(items.contains(&(2, &"two")));
        assert!(items.contains(&(5, &"five")));
    }
}
