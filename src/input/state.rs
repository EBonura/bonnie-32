//! Input state management
//!
//! Polls both keyboard (macroquad) and gamepad (gamepads crate) input,
//! combining them into a unified action-based API.

use gamepads::{Gamepads, Button};
use macroquad::prelude::*;
use super::Action;

/// Unified input state that handles both keyboard/mouse and gamepad
pub struct InputState {
    gamepads: Gamepads,
    /// Analog stick deadzone (0.0-1.0)
    pub stick_deadzone: f32,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            gamepads: Gamepads::new(),
            stick_deadzone: 0.15,
        }
    }

    /// Call once per frame before checking actions
    pub fn poll(&mut self) {
        self.gamepads.poll();
    }

    /// Get left stick as Vec2 (movement)
    /// Combines keyboard WASD with gamepad left stick
    pub fn left_stick(&self) -> Vec2 {
        let mut result = Vec2::ZERO;

        // Keyboard WASD
        if is_key_down(KeyCode::W) { result.y += 1.0; }
        if is_key_down(KeyCode::S) { result.y -= 1.0; }
        if is_key_down(KeyCode::A) { result.x -= 1.0; }
        if is_key_down(KeyCode::D) { result.x += 1.0; }

        // Gamepad left stick (take if larger magnitude)
        if let Some(gp) = self.gamepads.all().next() {
            let gp_stick = self.apply_deadzone(gp.left_stick());
            if gp_stick.length() > result.length() {
                result = gp_stick;
            }
        }

        // Normalize if > 1 (diagonal keyboard input)
        if result.length() > 1.0 {
            result = result.normalize();
        }
        result
    }

    /// Get right stick as Vec2 (camera look)
    /// Only from gamepad - mouse handled separately
    pub fn right_stick(&self) -> Vec2 {
        if let Some(gp) = self.gamepads.all().next() {
            return self.apply_deadzone(gp.right_stick());
        }
        Vec2::ZERO
    }

    /// Apply radial deadzone with linear rescaling
    fn apply_deadzone(&self, (x, y): (f32, f32)) -> Vec2 {
        let len = (x * x + y * y).sqrt();
        if len < self.stick_deadzone {
            return Vec2::ZERO;
        }
        // Rescale from deadzone..1.0 to 0.0..1.0
        let scale = (len - self.stick_deadzone) / (1.0 - self.stick_deadzone) / len;
        Vec2::new(x * scale, y * scale)
    }

    /// Check if action is currently held down
    pub fn action_down(&self, action: Action) -> bool {
        self.keyboard_down(action) || self.gamepad_down(action)
    }

    /// Check if action was just pressed this frame
    pub fn action_pressed(&self, action: Action) -> bool {
        self.keyboard_pressed(action) || self.gamepad_pressed(action)
    }

    fn keyboard_down(&self, action: Action) -> bool {
        match action {
            // Movement
            Action::MoveForward => is_key_down(KeyCode::W),
            Action::MoveBackward => is_key_down(KeyCode::S),
            Action::MoveLeft => is_key_down(KeyCode::A),
            Action::MoveRight => is_key_down(KeyCode::D),

            // Actions
            Action::Jump => is_key_down(KeyCode::Space),
            Action::Dodge => is_key_down(KeyCode::LeftShift),
            Action::Attack => is_key_down(KeyCode::J),
            Action::StrongAttack => is_key_down(KeyCode::K),
            Action::Guard => is_key_down(KeyCode::L),
            Action::Skill => is_key_down(KeyCode::I),
            Action::UseItem => is_key_down(KeyCode::R),
            Action::Interact => is_key_down(KeyCode::E),
            Action::Crouch => is_key_down(KeyCode::C),
            Action::LockOn => is_key_down(KeyCode::Tab),

            // System
            Action::OpenMenu => is_key_down(KeyCode::Escape),

            // Free-fly
            Action::FlyUp => is_key_down(KeyCode::Q),
            Action::FlyDown => is_key_down(KeyCode::E),

            _ => false,
        }
    }

    fn gamepad_down(&self, action: Action) -> bool {
        let Some(gp) = self.gamepads.all().next() else { return false };

        match action {
            // Face buttons (Elden Ring layout)
            Action::Jump => gp.is_currently_pressed(Button::ActionDown),        // A
            Action::Dodge => gp.is_currently_pressed(Button::ActionRight),      // B
            Action::UseItem => gp.is_currently_pressed(Button::ActionLeft),     // X
            Action::Interact => gp.is_currently_pressed(Button::ActionUp),      // Y

            // Shoulders (Elden Ring layout)
            Action::Guard => gp.is_currently_pressed(Button::FrontLeftUpper),   // LB
            Action::Skill => gp.is_currently_pressed(Button::FrontLeftLower),   // LT
            Action::Attack => gp.is_currently_pressed(Button::FrontRightUpper), // RB
            Action::StrongAttack => gp.is_currently_pressed(Button::FrontRightLower), // RT

            // Stick clicks
            Action::Crouch => gp.is_currently_pressed(Button::LeftStick),       // L3
            Action::LockOn => gp.is_currently_pressed(Button::RightStick),      // R3

            // D-pad
            Action::SwitchLeftWeapon => gp.is_currently_pressed(Button::DPadLeft),
            Action::SwitchRightWeapon => gp.is_currently_pressed(Button::DPadRight),
            Action::SwitchSpell => gp.is_currently_pressed(Button::DPadUp),
            Action::SwitchItem => gp.is_currently_pressed(Button::DPadDown),

            // System
            Action::OpenMenu => gp.is_currently_pressed(Button::RightCenterCluster),  // Start
            Action::OpenMap => gp.is_currently_pressed(Button::LeftCenterCluster),    // Select

            // Free-fly mode (reuses LB/LT)
            Action::FlyUp => gp.is_currently_pressed(Button::FrontLeftUpper),
            Action::FlyDown => gp.is_currently_pressed(Button::FrontLeftLower),

            _ => false,
        }
    }

    fn keyboard_pressed(&self, action: Action) -> bool {
        match action {
            Action::Jump => is_key_pressed(KeyCode::Space),
            Action::Dodge => is_key_pressed(KeyCode::LeftShift),
            Action::Attack => is_key_pressed(KeyCode::J),
            Action::StrongAttack => is_key_pressed(KeyCode::K),
            Action::Interact => is_key_pressed(KeyCode::E),
            Action::OpenMenu => is_key_pressed(KeyCode::Escape),
            Action::LockOn => is_key_pressed(KeyCode::Tab),
            Action::Crouch => is_key_pressed(KeyCode::C),
            _ => false,
        }
    }

    fn gamepad_pressed(&self, action: Action) -> bool {
        let Some(gp) = self.gamepads.all().next() else { return false };

        match action {
            Action::Jump => gp.is_just_pressed(Button::ActionDown),
            Action::Dodge => gp.is_just_pressed(Button::ActionRight),
            Action::Attack => gp.is_just_pressed(Button::FrontRightUpper),
            Action::StrongAttack => gp.is_just_pressed(Button::FrontRightLower),
            Action::Interact => gp.is_just_pressed(Button::ActionUp),
            Action::OpenMenu => gp.is_just_pressed(Button::RightCenterCluster),
            Action::LockOn => gp.is_just_pressed(Button::RightStick),
            Action::Crouch => gp.is_just_pressed(Button::LeftStick),
            Action::UseItem => gp.is_just_pressed(Button::ActionLeft),
            Action::Guard => gp.is_just_pressed(Button::FrontLeftUpper),
            Action::Skill => gp.is_just_pressed(Button::FrontLeftLower),
            Action::SwitchLeftWeapon => gp.is_just_pressed(Button::DPadLeft),
            Action::SwitchRightWeapon => gp.is_just_pressed(Button::DPadRight),
            Action::SwitchSpell => gp.is_just_pressed(Button::DPadUp),
            Action::SwitchItem => gp.is_just_pressed(Button::DPadDown),
            _ => false,
        }
    }

    /// Check if any gamepad is connected
    pub fn has_gamepad(&self) -> bool {
        self.gamepads.all().next().is_some()
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}
