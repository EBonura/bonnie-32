//! Controller input debug view
//!
//! Simple visualization of gamepad state for testing.

use macroquad::prelude::*;
use crate::ui::Rect;
use super::{InputState, Action};

/// Draw controller debug view showing current input state
pub fn draw_controller_debug(rect: Rect, input: &InputState) {
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(20, 22, 28, 255));

    let mut y = rect.y + 40.0;
    let x = rect.x + 40.0;
    let col2 = rect.x + rect.w / 2.0;

    // Title
    let title = if input.has_gamepad() {
        "CONTROLLER CONNECTED"
    } else {
        "NO CONTROLLER DETECTED"
    };
    let title_color = if input.has_gamepad() {
        Color::from_rgba(100, 255, 100, 255)
    } else {
        Color::from_rgba(255, 100, 100, 255)
    };
    draw_text(title, rect.x + (rect.w - measure_text(title, None, 24, 1.0).width) / 2.0, y, 24.0, title_color);
    y += 50.0;

    // Analog sticks
    draw_text("ANALOG STICKS", x, y, 16.0, Color::from_rgba(150, 150, 160, 255));
    y += 30.0;

    let left_stick = input.left_stick();
    let right_stick = input.right_stick();

    // Draw left stick
    draw_stick_widget(x + 60.0, y + 60.0, 50.0, left_stick, "Left Stick");

    // Draw right stick
    draw_stick_widget(col2 + 60.0, y + 60.0, 50.0, right_stick, "Right Stick");

    y += 150.0;

    // Face buttons
    draw_text("ACTIONS (pressed this frame)", x, y, 16.0, Color::from_rgba(150, 150, 160, 255));
    y += 25.0;

    let actions = [
        (Action::Jump, "Jump (A)"),
        (Action::Dodge, "Dodge (B)"),
        (Action::UseItem, "Use Item (X)"),
        (Action::Interact, "Interact (Y)"),
        (Action::Attack, "Attack (RB)"),
        (Action::StrongAttack, "Strong Attack (RT)"),
        (Action::Guard, "Guard (LB)"),
        (Action::Skill, "Skill (LT)"),
        (Action::Crouch, "Crouch (L3)"),
        (Action::LockOn, "Lock-On (R3)"),
        (Action::OpenMenu, "Menu (Start)"),
        (Action::OpenMap, "Map (Select)"),
        (Action::SwitchLeftWeapon, "D-Pad Left"),
        (Action::SwitchRightWeapon, "D-Pad Right"),
        (Action::SwitchSpell, "D-Pad Up"),
        (Action::SwitchItem, "D-Pad Down"),
        (Action::FlyUp, "Fly Up (LB)"),
        (Action::FlyDown, "Fly Down (LT)"),
    ];

    let mut col = 0;
    let col_width = 200.0;
    let start_y = y;

    for (action, label) in actions {
        let ax = x + (col as f32) * col_width;
        let ay = y;

        let pressed = input.action_pressed(action);
        let down = input.action_down(action);

        let color = if pressed {
            Color::from_rgba(100, 255, 100, 255) // Green flash for just pressed
        } else if down {
            Color::from_rgba(255, 200, 100, 255) // Yellow for held
        } else {
            Color::from_rgba(80, 80, 90, 255) // Gray for not pressed
        };

        // Draw indicator circle
        let indicator_color = if down {
            Color::from_rgba(100, 200, 100, 255)
        } else {
            Color::from_rgba(50, 50, 55, 255)
        };
        draw_circle(ax + 8.0, ay - 5.0, 6.0, indicator_color);

        draw_text(label, ax + 20.0, ay, 14.0, color);

        y += 22.0;

        // Switch to next column after 9 items
        if (actions.iter().position(|(a, _)| *a == action).unwrap() + 1) % 9 == 0 {
            col += 1;
            y = start_y;
        }
    }

    // Instructions at bottom
    let hint = "Connect a controller to test input. Press buttons to see them light up.";
    let hint_dims = measure_text(hint, None, 12, 1.0);
    draw_text(
        hint,
        rect.x + (rect.w - hint_dims.width) / 2.0,
        rect.y + rect.h - 30.0,
        12.0,
        Color::from_rgba(100, 100, 110, 180),
    );
}

/// Draw an analog stick widget
fn draw_stick_widget(cx: f32, cy: f32, radius: f32, value: macroquad::math::Vec2, label: &str) {
    // Outer circle (deadzone boundary)
    draw_circle_lines(cx, cy, radius, 2.0, Color::from_rgba(60, 60, 70, 255));

    // Deadzone circle
    draw_circle_lines(cx, cy, radius * 0.15, 1.0, Color::from_rgba(80, 80, 90, 255));

    // Current position
    let px = cx + value.x * radius;
    let py = cy - value.y * radius; // Y is inverted in screen space
    draw_circle(px, py, 8.0, Color::from_rgba(100, 180, 255, 255));

    // Line from center to current position
    if value.length() > 0.01 {
        draw_line(cx, cy, px, py, 2.0, Color::from_rgba(100, 180, 255, 150));
    }

    // Label
    let label_dims = measure_text(label, None, 12, 1.0);
    draw_text(label, cx - label_dims.width / 2.0, cy + radius + 20.0, 12.0, Color::from_rgba(120, 120, 130, 255));

    // Value text
    let value_text = format!("({:.2}, {:.2})", value.x, value.y);
    let value_dims = measure_text(&value_text, None, 11, 1.0);
    draw_text(&value_text, cx - value_dims.width / 2.0, cy + radius + 35.0, 11.0, Color::from_rgba(80, 80, 90, 255));
}
