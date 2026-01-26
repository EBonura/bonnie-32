//! Immediate-mode UI library for the level editor
//!
//! Inspired by TRLE's layout:
//! - Resizable split panels (horizontal/vertical)
//! - 2D grid view, 3D preview, texture palette
//! - Property panels and toolbars
//!
//! Design principles:
//! - Immediate mode (no retained state, rebuilt each frame)
//! - Simple rectangle-based layout
//! - Macroquad integration for rendering
//!
//! Note: Some widget state fields are tracked for future features.

#![allow(dead_code)]

mod rect;
mod panel;
mod widgets;
mod input;
mod tabbar;
mod icons;
mod theme;
mod actions;
pub mod drag_tracker;
pub mod tool;
pub mod tool_box;
pub mod tool_controller;

pub use rect::*;
pub use panel::*;
pub use widgets::*;
pub use input::*;
pub use tabbar::*;
pub use icons::*;
pub use theme::*;
pub use actions::*;
pub use drag_tracker::{
    DragState, DragStatus, DragConfig, SnapMode, Axis, PickerType, apply_drag_update, snap_angle,
};
pub use tool::{Tool, ToolRegistry};
pub use tool_box::ToolBox;
pub use tool_controller::{ToolController, InputState, DragAcceptResult};
