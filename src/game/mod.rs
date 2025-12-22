//! Game Foundation Module
//!
//! A lightweight ECS-inspired game framework tailored for PS1-era souls-like
//! and metroidvania games. Inspired by Bevy's patterns but simplified for
//! the specific needs of this engine.
//!
//! Key concepts:
//! - Entity: Generational index for safe entity references
//! - Component: Plain data structs attached to entities
//! - World: Container for all entities and their components
//! - Event: Decoupled communication between systems
//!
//! Design philosophy:
//! - Simple over flexible (we know what game we're making)
//! - Cache-friendly data layouts
//! - No runtime type registration (compile-time known components)

pub mod entity;
pub mod component;
pub mod world;
pub mod event;
pub mod transform;
pub mod components;
pub mod collision;
pub mod runtime;
pub mod renderer;

// Re-export main types
pub use entity::{Entity, EntityAllocator};
pub use component::ComponentStorage;
pub use world::World;
pub use event::{Events, EventQueue};
pub use transform::{Transform, GlobalTransform, propagate_transforms};
pub use components::*;
pub use collision::{collide_cylinder, move_and_slide, CollisionResult};
pub use runtime::{GameToolState, CameraMode, FpsLimit};
pub use renderer::draw_test_viewport;
