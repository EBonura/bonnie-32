//! 3D Modeler with segmented/hierarchy animation (PS1-style)
//!
//! Key principle: Each model part IS its own bone.
//! No weight painting, no GPU skinning - just hierarchical transforms.
//!
//! Used in Metal Gear Solid, Resident Evil, Final Fantasy VII.

mod model;
mod state;
mod layout;
mod viewport;

pub use model::*;
pub use state::*;
pub use layout::*;
pub use viewport::*;
