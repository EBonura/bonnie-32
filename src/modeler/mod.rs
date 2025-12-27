//! PicoCAD-inspired 3D Modeler
//!
//! A tiny modeler for tiny models - prioritizes simplicity and fun.
//! 4-panel viewport layout, face-centric workflow, grid snapping.
//!
//! Also supports PS1-style hierarchical animation (each part = bone).

#![allow(dead_code)]

mod model;
mod state;
mod layout;
mod viewport;
mod model_browser;
mod mesh_editor;
mod obj_import;
mod mesh_browser;

// Re-export public API
#[allow(unused_imports)]
pub use model::*;
pub use state::*;
pub use layout::*;
#[allow(unused_imports)]
pub use viewport::*;
pub use model_browser::*;
#[allow(unused_imports)]
pub use mesh_editor::*;
pub use obj_import::*;
pub use mesh_browser::*;
