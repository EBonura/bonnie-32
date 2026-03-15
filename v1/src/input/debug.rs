//! Controller input debug view
//!
//! Simple visualization of gamepad state for testing.

use macroquad::prelude::*;
use crate::ui::Rect;
use super::{InputState, Action, ButtonLabels};

/// Draw controller debug view showing current input state
/// Returns the new deadzone value if changed by user
pub fn draw_controller_debug(rect: Rect, input: &mut InputState) {
    // Background
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::from_rgba(20, 22, 28, 255));

    let mut y = rect.y + 40.0;
    let x = rect.x + 40.0;

    // Get button labels for detected controller type
    let labels = input.button_labels();

    // Header: "Detected: Xbox/PlayStation/Nintendo" or "No Controller"
    let header = if input.has_gamepad() {
        format!("Detected: {}", labels.controller_type().display_name())
    } else {
        "No Controller Detected".to_string()
    };
    let header_color = if input.has_gamepad() {
        Color::from_rgba(100, 255, 100, 255)
    } else {
        Color::from_rgba(255, 100, 100, 255)
    };
    draw_text(&header, x, y, 20.0, header_color);
    y += 35.0;

    // Deadzone setting
    let deadzone = input.deadzone();
    draw_text("DEADZONE", x, y, 14.0, Color::from_rgba(150, 150, 160, 255));
    y += 20.0;

    // Deadzone slider
    let slider_width = 200.0;
    let slider_height = 8.0;
    let slider_x = x;
    let slider_y = y;

    // Background track
    draw_rectangle(slider_x, slider_y, slider_width, slider_height, Color::from_rgba(40, 42, 48, 255));

    // Filled portion
    let fill_width = (deadzone / 0.5) * slider_width;
    draw_rectangle(slider_x, slider_y, fill_width, slider_height, Color::from_rgba(80, 140, 200, 255));

    // Handle
    let handle_x = slider_x + fill_width;
    draw_circle(handle_x, slider_y + slider_height / 2.0, 8.0, Color::from_rgba(100, 180, 255, 255));

    // Value text
    let value_text = format!("{:.0}%", deadzone * 100.0);
    draw_text(&value_text, slider_x + slider_width + 15.0, slider_y + 6.0, 14.0, Color::from_rgba(150, 150, 160, 255));

    // Handle mouse interaction for slider
    let mouse_pos = mouse_position();
    let slider_rect = Rect::new(slider_x - 10.0, slider_y - 10.0, slider_width + 20.0, slider_height + 20.0);
    if is_mouse_button_down(MouseButton::Left) && slider_rect.contains(mouse_pos.0, mouse_pos.1) {
        let new_value = ((mouse_pos.0 - slider_x) / slider_width).clamp(0.0, 1.0) * 0.5;
        input.set_deadzone(new_value);
    }

    y += 30.0;

    // Analog sticks section
    draw_text("ANALOG STICKS", x, y, 14.0, Color::from_rgba(150, 150, 160, 255));
    y += 25.0;

    let left_stick = input.left_stick();
    let right_stick = input.right_stick();

    // Draw sticks side by side
    let stick_radius = 40.0;
    let stick_spacing = 140.0;
    draw_stick_widget(x + stick_radius + 10.0, y + stick_radius, stick_radius, left_stick, "Left", deadzone);
    draw_stick_widget(x + stick_radius + 10.0 + stick_spacing, y + stick_radius, stick_radius, right_stick, "Right", deadzone);

    y += stick_radius * 2.0 + 50.0;

    // Actions section
    draw_text("ACTIONS", x, y, 14.0, Color::from_rgba(150, 150, 160, 255));
    y += 25.0;

    // Build action labels dynamically based on controller type
    let actions = build_action_labels(&labels);

    let col_width = 200.0;
    let start_y = y;
    let mut col = 0;

    for (i, (action, label)) in actions.iter().enumerate() {
        let ax = x + (col as f32) * col_width;
        let ay = y;

        let pressed = input.action_pressed(*action);
        let down = input.action_down(*action);

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

        y += 20.0;

        // Switch to next column after 9 items
        if (i + 1) % 9 == 0 {
            col += 1;
            y = start_y;
        }
    }

    // Instructions at bottom left
    if !input.has_gamepad() {
        let hint = "Connect a controller to test input";
        draw_text(hint, x, rect.y + rect.h - 30.0, 12.0, Color::from_rgba(100, 100, 110, 180));
    }
}

/// Build action labels with controller-specific button names
fn build_action_labels(labels: &ButtonLabels) -> Vec<(Action, String)> {
    vec![
        (Action::Jump, format!("Jump ({})", labels.south())),
        (Action::Dodge, format!("Dodge ({})", labels.east())),
        (Action::UseItem, format!("Use Item ({})", labels.west())),
        (Action::Interact, format!("Interact ({})", labels.north())),
        (Action::Attack, format!("Attack ({})", labels.right_bumper())),
        (Action::StrongAttack, format!("Strong Attack ({})", labels.right_trigger())),
        (Action::Guard, format!("Guard ({})", labels.left_bumper())),
        (Action::Skill, format!("Skill ({})", labels.left_trigger())),
        (Action::Crouch, format!("Crouch ({})", labels.left_stick())),
        (Action::LockOn, format!("Lock-On ({})", labels.right_stick())),
        (Action::OpenMenu, format!("Menu ({})", labels.start())),
        (Action::OpenMap, format!("Map ({})", labels.select())),
        (Action::SwitchLeftWeapon, labels.dpad_left().to_string()),
        (Action::SwitchRightWeapon, labels.dpad_right().to_string()),
        (Action::SwitchSpell, labels.dpad_up().to_string()),
        (Action::SwitchItem, labels.dpad_down().to_string()),
        // Note: FlyUp/FlyDown are intentionally omitted - they reuse Guard/Skill buttons
    ]
}

/// Draw an analog stick widget with deadzone visualization
fn draw_stick_widget(cx: f32, cy: f32, radius: f32, value: macroquad::math::Vec2, label: &str, deadzone: f32) {
    // Outer circle
    draw_circle_lines(cx, cy, radius, 2.0, Color::from_rgba(60, 60, 70, 255));

    // Deadzone circle (scaled to actual deadzone value)
    let deadzone_radius = radius * deadzone;
    draw_circle_lines(cx, cy, deadzone_radius, 1.0, Color::from_rgba(100, 60, 60, 255));

    // Current position
    let px = cx + value.x * radius;
    let py = cy - value.y * radius; // Y is inverted in screen space
    draw_circle(px, py, 6.0, Color::from_rgba(100, 180, 255, 255));

    // Line from center to current position
    if value.length() > 0.01 {
        draw_line(cx, cy, px, py, 2.0, Color::from_rgba(100, 180, 255, 150));
    }

    // Label below
    let label_dims = measure_text(label, None, 11, 1.0);
    draw_text(label, cx - label_dims.width / 2.0, cy + radius + 15.0, 11.0, Color::from_rgba(120, 120, 130, 255));
}
