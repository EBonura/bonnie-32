//! Unified gamepad support for native and WASM
//!
//! Native: Uses gilrs crate for cross-platform gamepad input
//! WASM: Uses custom Web Gamepad API bindings via JavaScript plugin

use macroquad::prelude::Vec2;

// Standard gamepad button indices (matches Web Gamepad API standard mapping)
// These correspond to Xbox layout used by most platforms
pub mod button {
    pub const A: u32 = 0;           // ActionDown / South
    pub const B: u32 = 1;           // ActionRight / East
    pub const X: u32 = 2;           // ActionLeft / West
    pub const Y: u32 = 3;           // ActionUp / North
    pub const LB: u32 = 4;          // Left Bumper
    pub const RB: u32 = 5;          // Right Bumper
    pub const LT: u32 = 6;          // Left Trigger (as button)
    pub const RT: u32 = 7;          // Right Trigger (as button)
    pub const SELECT: u32 = 8;      // Back/Select
    pub const START: u32 = 9;       // Start/Options
    pub const L3: u32 = 10;         // Left Stick click
    pub const R3: u32 = 11;         // Right Stick click
    pub const DPAD_UP: u32 = 12;
    pub const DPAD_DOWN: u32 = 13;
    pub const DPAD_LEFT: u32 = 14;
    pub const DPAD_RIGHT: u32 = 15;
    pub const GUIDE: u32 = 16;      // Xbox/PS button
}

// ============================================================================
// WASM Implementation (Web Gamepad API)
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::*;

    // FFI bindings to JavaScript functions in index.html
    extern "C" {
        fn bonnie_gamepad_has_gamepad() -> i32;
        fn bonnie_gamepad_get_button_mask() -> u32;
        fn bonnie_gamepad_get_button_pressed_mask() -> u32;
        fn bonnie_gamepad_get_left_stick_x() -> i32;
        fn bonnie_gamepad_get_left_stick_y() -> i32;
        fn bonnie_gamepad_get_right_stick_x() -> i32;
        fn bonnie_gamepad_get_right_stick_y() -> i32;
        fn bonnie_gamepad_get_left_trigger() -> i32;
        fn bonnie_gamepad_get_right_trigger() -> i32;
    }

    pub struct Gamepad {
        deadzone: f32,
    }

    impl Gamepad {
        pub fn new() -> Self {
            Self { deadzone: 0.15 }
        }

        pub fn poll(&mut self) {
            // Web Gamepad API is polled automatically by the browser
        }

        pub fn has_gamepad(&self) -> bool {
            unsafe { bonnie_gamepad_has_gamepad() != 0 }
        }

        pub fn is_button_down(&self, button: u32) -> bool {
            let mask = unsafe { bonnie_gamepad_get_button_mask() };
            (mask & (1 << button)) != 0
        }

        pub fn is_button_pressed(&self, button: u32) -> bool {
            let mask = unsafe { bonnie_gamepad_get_button_pressed_mask() };
            (mask & (1 << button)) != 0
        }

        pub fn left_stick(&self) -> Vec2 {
            let x = unsafe { bonnie_gamepad_get_left_stick_x() } as f32 / 10000.0;
            let y = -(unsafe { bonnie_gamepad_get_left_stick_y() } as f32 / 10000.0); // Invert Y
            apply_deadzone(x, y, self.deadzone)
        }

        pub fn right_stick(&self) -> Vec2 {
            let x = unsafe { bonnie_gamepad_get_right_stick_x() } as f32 / 10000.0;
            let y = -(unsafe { bonnie_gamepad_get_right_stick_y() } as f32 / 10000.0); // Invert Y
            apply_deadzone(x, y, self.deadzone)
        }

        pub fn left_trigger(&self) -> f32 {
            (unsafe { bonnie_gamepad_get_left_trigger() }) as f32 / 10000.0
        }

        pub fn right_trigger(&self) -> f32 {
            (unsafe { bonnie_gamepad_get_right_trigger() }) as f32 / 10000.0
        }
    }

    impl Default for Gamepad {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ============================================================================
// Native Implementation (gilrs)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::*;
    use std::cell::Cell;
    use gilrs::{Gilrs, Button as GilrsButton, Axis};

    pub struct Gamepad {
        gilrs: Gilrs,
        deadzone: f32,
        last_buttons: Cell<u32>,
    }

    impl Gamepad {
        pub fn new() -> Self {
            Self {
                gilrs: Gilrs::new().unwrap(),
                deadzone: 0.15,
                last_buttons: Cell::new(0),
            }
        }

        pub fn poll(&mut self) {
            // Process gilrs events to update internal state
            while let Some(_event) = self.gilrs.next_event() {
                // Events are processed internally by gilrs
            }
        }

        pub fn has_gamepad(&self) -> bool {
            self.gilrs.gamepads().next().is_some()
        }

        fn get_active_gamepad(&self) -> Option<gilrs::Gamepad> {
            self.gilrs.gamepads().next().map(|(_, gp)| gp)
        }

        fn get_button_mask(&self) -> u32 {
            let Some(gp) = self.get_active_gamepad() else { return 0 };
            let mut mask = 0u32;

            if gp.is_pressed(GilrsButton::South) { mask |= 1 << super::button::A; }
            if gp.is_pressed(GilrsButton::East) { mask |= 1 << super::button::B; }
            if gp.is_pressed(GilrsButton::West) { mask |= 1 << super::button::X; }
            if gp.is_pressed(GilrsButton::North) { mask |= 1 << super::button::Y; }
            if gp.is_pressed(GilrsButton::LeftTrigger) { mask |= 1 << super::button::LB; }
            if gp.is_pressed(GilrsButton::RightTrigger) { mask |= 1 << super::button::RB; }
            if gp.is_pressed(GilrsButton::LeftTrigger2) { mask |= 1 << super::button::LT; }
            if gp.is_pressed(GilrsButton::RightTrigger2) { mask |= 1 << super::button::RT; }
            if gp.is_pressed(GilrsButton::Select) { mask |= 1 << super::button::SELECT; }
            if gp.is_pressed(GilrsButton::Start) { mask |= 1 << super::button::START; }
            if gp.is_pressed(GilrsButton::LeftThumb) { mask |= 1 << super::button::L3; }
            if gp.is_pressed(GilrsButton::RightThumb) { mask |= 1 << super::button::R3; }
            if gp.is_pressed(GilrsButton::DPadUp) { mask |= 1 << super::button::DPAD_UP; }
            if gp.is_pressed(GilrsButton::DPadDown) { mask |= 1 << super::button::DPAD_DOWN; }
            if gp.is_pressed(GilrsButton::DPadLeft) { mask |= 1 << super::button::DPAD_LEFT; }
            if gp.is_pressed(GilrsButton::DPadRight) { mask |= 1 << super::button::DPAD_RIGHT; }
            if gp.is_pressed(GilrsButton::Mode) { mask |= 1 << super::button::GUIDE; }

            mask
        }

        pub fn is_button_down(&self, button: u32) -> bool {
            (self.get_button_mask() & (1 << button)) != 0
        }

        pub fn is_button_pressed(&self, button: u32) -> bool {
            let current = self.get_button_mask();
            let last = self.last_buttons.get();
            let was_down = (last & (1 << button)) != 0;
            let is_down = (current & (1 << button)) != 0;
            self.last_buttons.set(current);
            is_down && !was_down
        }

        pub fn left_stick(&self) -> Vec2 {
            let Some(gp) = self.get_active_gamepad() else { return Vec2::ZERO };
            let x = gp.value(Axis::LeftStickX);
            let y = -gp.value(Axis::LeftStickY); // Invert Y to match Web API
            apply_deadzone(x, y, self.deadzone)
        }

        pub fn right_stick(&self) -> Vec2 {
            let Some(gp) = self.get_active_gamepad() else { return Vec2::ZERO };
            let x = gp.value(Axis::RightStickX);
            let y = -gp.value(Axis::RightStickY); // Invert Y to match Web API
            apply_deadzone(x, y, self.deadzone)
        }

        pub fn left_trigger(&self) -> f32 {
            let Some(gp) = self.get_active_gamepad() else { return 0.0 };
            gp.value(Axis::LeftZ).max(0.0)
        }

        pub fn right_trigger(&self) -> f32 {
            let Some(gp) = self.get_active_gamepad() else { return 0.0 };
            gp.value(Axis::RightZ).max(0.0)
        }
    }

    impl Default for Gamepad {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ============================================================================
// Shared utilities
// ============================================================================

/// Apply radial deadzone with linear rescaling
fn apply_deadzone(x: f32, y: f32, deadzone: f32) -> Vec2 {
    let len = (x * x + y * y).sqrt();
    if len < deadzone {
        return Vec2::ZERO;
    }
    // Rescale from deadzone..1.0 to 0.0..1.0
    let scale = (len - deadzone) / (1.0 - deadzone) / len;
    Vec2::new(x * scale, y * scale)
}

// Re-export the platform-specific implementation
pub use platform::Gamepad;
