//! 3D Modeler with segmented/hierarchy animation (PS1-style)
//!
//! Key principle: Each model part IS its own bone.
//! No weight painting, no GPU skinning - just hierarchical transforms.
//!
//! Used in Metal Gear Solid, Resident Evil, Final Fantasy VII.
//!
//! Note: Work in progress - many API items not yet fully integrated.

#![allow(dead_code)]

mod model;
mod state;
mod layout;
mod viewport;

// Re-export public API
// Some of these aren't used externally yet but are part of the intended public API
#[allow(unused_imports)]
pub use model::*;
pub use state::*;
pub use layout::*;
#[allow(unused_imports)]
pub use viewport::*;
