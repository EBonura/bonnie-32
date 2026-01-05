//! Input state management
//!
//! Polls both keyboard (macroquad) and gamepad input, combining them into
//! a unified action-based API.

use macroquad::prelude::*;
use super::{Action, Gamepad, button, ControllerType, ButtonLabels};

/// Unified input state that handles both keyboard/mouse and gamepad
pub struct InputState {
    gamepad: Gamepad,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            gamepad: Gamepad::new(),
        }
    }

    /// Call once per frame before checking actions
    pub fn poll(&mut self) {
        self.gamepad.poll();
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
        let gp_stick = self.gamepad.left_stick();
        if gp_stick.length() > result.length() {
            result = gp_stick;
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
        self.gamepad.right_stick()
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
        match action {
            // Face buttons (Elden Ring layout)
            Action::Jump => self.gamepad.is_button_down(button::A),
            Action::Dodge => self.gamepad.is_button_down(button::B),
            Action::UseItem => self.gamepad.is_button_down(button::X),
            Action::Interact => self.gamepad.is_button_down(button::Y),

            // Shoulders (Elden Ring layout)
            Action::Guard => self.gamepad.is_button_down(button::LB),
            Action::Skill => self.gamepad.is_button_down(button::LT),
            Action::Attack => self.gamepad.is_button_down(button::RB),
            Action::StrongAttack => self.gamepad.is_button_down(button::RT),

            // Stick clicks
            Action::Crouch => self.gamepad.is_button_down(button::L3),
            Action::LockOn => self.gamepad.is_button_down(button::R3),

            // D-pad
            Action::SwitchLeftWeapon => self.gamepad.is_button_down(button::DPAD_LEFT),
            Action::SwitchRightWeapon => self.gamepad.is_button_down(button::DPAD_RIGHT),
            Action::SwitchSpell => self.gamepad.is_button_down(button::DPAD_UP),
            Action::SwitchItem => self.gamepad.is_button_down(button::DPAD_DOWN),

            // System
            Action::OpenMenu => self.gamepad.is_button_down(button::START),
            Action::OpenMap => self.gamepad.is_button_down(button::SELECT),

            // Free-fly mode (reuses LB/LT)
            Action::FlyUp => self.gamepad.is_button_down(button::LB),
            Action::FlyDown => self.gamepad.is_button_down(button::LT),

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
        match action {
            Action::Jump => self.gamepad.is_button_pressed(button::A),
            Action::Dodge => self.gamepad.is_button_pressed(button::B),
            Action::Attack => self.gamepad.is_button_pressed(button::RB),
            Action::StrongAttack => self.gamepad.is_button_pressed(button::RT),
            Action::Interact => self.gamepad.is_button_pressed(button::Y),
            Action::OpenMenu => self.gamepad.is_button_pressed(button::START),
            Action::LockOn => self.gamepad.is_button_pressed(button::R3),
            Action::Crouch => self.gamepad.is_button_pressed(button::L3),
            Action::UseItem => self.gamepad.is_button_pressed(button::X),
            Action::Guard => self.gamepad.is_button_pressed(button::LB),
            Action::Skill => self.gamepad.is_button_pressed(button::LT),
            Action::SwitchLeftWeapon => self.gamepad.is_button_pressed(button::DPAD_LEFT),
            Action::SwitchRightWeapon => self.gamepad.is_button_pressed(button::DPAD_RIGHT),
            Action::SwitchSpell => self.gamepad.is_button_pressed(button::DPAD_UP),
            Action::SwitchItem => self.gamepad.is_button_pressed(button::DPAD_DOWN),
            _ => false,
        }
    }

    /// Check if any gamepad is connected
    pub fn has_gamepad(&self) -> bool {
        self.gamepad.has_gamepad()
    }

    /// Get the name of the connected gamepad (empty string if none)
    pub fn gamepad_name(&self) -> String {
        self.gamepad.gamepad_name()
    }

    /// Get the detected controller type based on gamepad name
    pub fn controller_type(&self) -> ControllerType {
        ControllerType::from_name(&self.gamepad_name())
    }

    /// Get button labels for the connected controller
    pub fn button_labels(&self) -> ButtonLabels {
        ButtonLabels::new(self.controller_type())
    }

    /// Get the current stick deadzone (0.0-0.5)
    pub fn deadzone(&self) -> f32 {
        self.gamepad.deadzone()
    }

    /// Set the stick deadzone (clamped to 0.0-0.5)
    pub fn set_deadzone(&mut self, deadzone: f32) {
        self.gamepad.set_deadzone(deadzone);
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}
