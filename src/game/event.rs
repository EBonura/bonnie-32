//! Event System
//!
//! Events allow decoupled communication between game systems.
//! Instead of systems directly calling each other, they send events
//! that other systems can listen to.
//!
//! Example flow:
//! 1. Combat system detects hit → sends DamageEvent
//! 2. Health system reads DamageEvent → reduces health
//! 3. Audio system reads DamageEvent → plays hit sound
//! 4. VFX system reads DamageEvent → spawns particles
//!
//! Each system handles its own concern without knowing about the others.

use super::entity::Entity;
use crate::rasterizer::Vec3;

/// A queue for events of a single type.
/// Events are collected during the frame and drained at specific points.
#[derive(Debug)]
pub struct EventQueue<T> {
    events: Vec<T>,
}

impl<T> EventQueue<T> {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Send an event (add to queue)
    pub fn send(&mut self, event: T) {
        self.events.push(event);
    }

    /// Iterate over events without clearing
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.events.iter()
    }

    /// Drain all events (returns iterator and clears queue)
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.events.drain(..)
    }

    /// Check if there are any events
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Clear all events without processing
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Number of events in queue
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

impl<T> Default for EventQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Container for all game events.
/// Add new event types as fields here.
pub struct Events {
    /// Damage dealt to an entity
    pub damage: EventQueue<DamageEvent>,

    /// Entity died
    pub death: EventQueue<DeathEvent>,

    /// Entity spawned
    pub spawn: EventQueue<SpawnEvent>,

    /// Player reached a checkpoint
    pub checkpoint_activated: EventQueue<CheckpointEvent>,

    /// Door opened
    pub door_opened: EventQueue<DoorEvent>,

    /// Item collected
    pub item_collected: EventQueue<ItemCollectedEvent>,

    /// Collision between two entities
    pub collision: EventQueue<CollisionEvent>,

    /// Player respawn requested
    pub respawn: EventQueue<RespawnEvent>,

    /// Trigger volume entered
    pub trigger_enter: EventQueue<TriggerEvent>,

    /// Trigger volume exited
    pub trigger_exit: EventQueue<TriggerEvent>,
}

impl Events {
    pub fn new() -> Self {
        Self {
            damage: EventQueue::new(),
            death: EventQueue::new(),
            spawn: EventQueue::new(),
            checkpoint_activated: EventQueue::new(),
            door_opened: EventQueue::new(),
            item_collected: EventQueue::new(),
            collision: EventQueue::new(),
            respawn: EventQueue::new(),
            trigger_enter: EventQueue::new(),
            trigger_exit: EventQueue::new(),
        }
    }

    /// Clear all event queues. Call at end of frame.
    pub fn clear_all(&mut self) {
        self.damage.clear();
        self.death.clear();
        self.spawn.clear();
        self.checkpoint_activated.clear();
        self.door_opened.clear();
        self.item_collected.clear();
        self.collision.clear();
        self.respawn.clear();
        self.trigger_enter.clear();
        self.trigger_exit.clear();
    }
}

impl Default for Events {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Event Types
// =============================================================================

/// Damage was dealt to an entity
#[derive(Debug, Clone, Copy)]
pub struct DamageEvent {
    /// Who got hit
    pub target: Entity,
    /// Who dealt the damage (if any)
    pub source: Option<Entity>,
    /// Amount of damage
    pub amount: i32,
    /// Where the hit occurred (for VFX)
    pub position: Vec3,
}

/// An entity died
#[derive(Debug, Clone, Copy)]
pub struct DeathEvent {
    /// Who died
    pub entity: Entity,
    /// Who killed them (if any)
    pub killer: Option<Entity>,
    /// Where they died (for drops, VFX)
    pub position: Vec3,
}

/// An entity was spawned
#[derive(Debug, Clone, Copy)]
pub struct SpawnEvent {
    /// The new entity
    pub entity: Entity,
    /// Where it spawned
    pub position: Vec3,
}

/// A checkpoint was activated
#[derive(Debug, Clone, Copy)]
pub struct CheckpointEvent {
    /// The checkpoint entity
    pub checkpoint: Entity,
    /// The player who activated it
    pub player: Entity,
}

/// A door was opened
#[derive(Debug, Clone, Copy)]
pub struct DoorEvent {
    /// The door entity
    pub door: Entity,
    /// Who opened it
    pub opener: Entity,
}

/// An item was collected
#[derive(Debug, Clone, Copy)]
pub struct ItemCollectedEvent {
    /// The item entity (will be despawned)
    pub item: Entity,
    /// Who collected it
    pub collector: Entity,
    /// The item type (copied since entity will be gone)
    pub item_type: super::components::ItemType,
}

/// Two entities collided
#[derive(Debug, Clone, Copy)]
pub struct CollisionEvent {
    /// First entity
    pub entity_a: Entity,
    /// Second entity
    pub entity_b: Entity,
    /// Collision point (approximate)
    pub point: Vec3,
}

/// Player respawn requested
#[derive(Debug, Clone, Copy)]
pub struct RespawnEvent {
    /// The player to respawn
    pub player: Entity,
    /// Where to respawn (checkpoint position)
    pub position: Vec3,
}

/// A trigger volume was entered or exited
#[derive(Debug, Clone)]
pub struct TriggerEvent {
    /// The trigger entity
    pub trigger: Entity,
    /// The entity that entered/exited the trigger
    pub other: Entity,
    /// The trigger identifier string
    pub trigger_id: String,
    /// The event name from the trigger definition (on_enter or on_exit)
    pub event_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_queue() {
        let mut queue: EventQueue<i32> = EventQueue::new();

        queue.send(1);
        queue.send(2);
        queue.send(3);

        assert_eq!(queue.len(), 3);

        let collected: Vec<_> = queue.drain().collect();
        assert_eq!(collected, vec![1, 2, 3]);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_events_container() {
        let mut events = Events::new();

        events.damage.send(DamageEvent {
            target: Entity::default(),
            source: None,
            amount: 10,
            position: Vec3::ZERO,
        });

        assert_eq!(events.damage.len(), 1);

        events.clear_all();
        assert!(events.damage.is_empty());
    }
}
