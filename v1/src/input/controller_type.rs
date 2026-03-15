//! Controller type detection and button label mapping
//!
//! Detects controller type from name and provides platform-appropriate button labels.
//! Used to display correct button prompts (e.g., "Cross" for PlayStation, "A" for Xbox).

/// Controller manufacturer/type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControllerType {
    /// PlayStation (DualShock, DualSense)
    PlayStation,
    /// Xbox (Xbox 360, Xbox One, Xbox Series)
    #[default]
    Xbox,
    /// Nintendo (Switch Pro, Joy-Con)
    Nintendo,
    /// Unknown or generic controller (uses Xbox labels as default)
    Generic,
}

impl ControllerType {
    /// Detect controller type from its name string
    pub fn from_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();

        // PlayStation detection
        if name_lower.contains("playstation")
            || name_lower.contains("dualshock")
            || name_lower.contains("dualsense")
            || name_lower.contains("sony")
            || name_lower.contains("ps3")
            || name_lower.contains("ps4")
            || name_lower.contains("ps5")
        {
            return ControllerType::PlayStation;
        }

        // Nintendo detection
        if name_lower.contains("nintendo")
            || name_lower.contains("switch")
            || name_lower.contains("joy-con")
            || name_lower.contains("joycon")
            || name_lower.contains("pro controller")
        {
            return ControllerType::Nintendo;
        }

        // Xbox detection
        if name_lower.contains("xbox")
            || name_lower.contains("microsoft")
            || name_lower.contains("xinput")
        {
            return ControllerType::Xbox;
        }

        // Default to Generic (which uses Xbox labels since that's the Web Gamepad API standard)
        ControllerType::Generic
    }

    /// Get display name for this controller type
    pub fn display_name(&self) -> &'static str {
        match self {
            ControllerType::PlayStation => "PlayStation",
            ControllerType::Xbox => "Xbox",
            ControllerType::Nintendo => "Nintendo",
            ControllerType::Generic => "Generic",
        }
    }
}

/// Standard gamepad button positions (matches Web Gamepad API indices)
/// These represent physical positions, not labels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonPosition {
    /// Bottom face button (Xbox A, PS Cross, Nintendo B)
    South,
    /// Right face button (Xbox B, PS Circle, Nintendo A)
    East,
    /// Left face button (Xbox X, PS Square, Nintendo Y)
    West,
    /// Top face button (Xbox Y, PS Triangle, Nintendo X)
    North,
    /// Left shoulder button (Xbox LB, PS L1, Nintendo L)
    LeftBumper,
    /// Right shoulder button (Xbox RB, PS R1, Nintendo R)
    RightBumper,
    /// Left trigger (Xbox LT, PS L2, Nintendo ZL)
    LeftTrigger,
    /// Right trigger (Xbox RT, PS R2, Nintendo ZR)
    RightTrigger,
    /// Back/Select button (Xbox View, PS Share/Create, Nintendo -)
    Select,
    /// Start/Options button (Xbox Menu, PS Options, Nintendo +)
    Start,
    /// Left stick click (Xbox LS, PS L3, Nintendo LS)
    LeftStick,
    /// Right stick click (Xbox RS, PS R3, Nintendo RS)
    RightStick,
    /// D-pad up
    DPadUp,
    /// D-pad down
    DPadDown,
    /// D-pad left
    DPadLeft,
    /// D-pad right
    DPadRight,
    /// Guide/Home button (Xbox button, PS button, Home)
    Guide,
}

impl ButtonPosition {
    /// Get the button label for this position on a given controller type
    pub fn label(&self, controller: ControllerType) -> &'static str {
        match controller {
            ControllerType::PlayStation => self.playstation_label(),
            ControllerType::Xbox | ControllerType::Generic => self.xbox_label(),
            ControllerType::Nintendo => self.nintendo_label(),
        }
    }

    /// Get the short label (for tight spaces)
    pub fn short_label(&self, controller: ControllerType) -> &'static str {
        match controller {
            ControllerType::PlayStation => self.playstation_short_label(),
            ControllerType::Xbox | ControllerType::Generic => self.xbox_short_label(),
            ControllerType::Nintendo => self.nintendo_short_label(),
        }
    }

    // Xbox labels (also used as default/generic)
    fn xbox_label(&self) -> &'static str {
        match self {
            ButtonPosition::South => "A",
            ButtonPosition::East => "B",
            ButtonPosition::West => "X",
            ButtonPosition::North => "Y",
            ButtonPosition::LeftBumper => "LB",
            ButtonPosition::RightBumper => "RB",
            ButtonPosition::LeftTrigger => "LT",
            ButtonPosition::RightTrigger => "RT",
            ButtonPosition::Select => "View",
            ButtonPosition::Start => "Menu",
            ButtonPosition::LeftStick => "LS",
            ButtonPosition::RightStick => "RS",
            ButtonPosition::DPadUp => "D-Pad Up",
            ButtonPosition::DPadDown => "D-Pad Down",
            ButtonPosition::DPadLeft => "D-Pad Left",
            ButtonPosition::DPadRight => "D-Pad Right",
            ButtonPosition::Guide => "Xbox",
        }
    }

    fn xbox_short_label(&self) -> &'static str {
        match self {
            ButtonPosition::South => "A",
            ButtonPosition::East => "B",
            ButtonPosition::West => "X",
            ButtonPosition::North => "Y",
            ButtonPosition::LeftBumper => "LB",
            ButtonPosition::RightBumper => "RB",
            ButtonPosition::LeftTrigger => "LT",
            ButtonPosition::RightTrigger => "RT",
            ButtonPosition::Select => "View",
            ButtonPosition::Start => "Menu",
            ButtonPosition::LeftStick => "LS",
            ButtonPosition::RightStick => "RS",
            ButtonPosition::DPadUp => "↑",
            ButtonPosition::DPadDown => "↓",
            ButtonPosition::DPadLeft => "←",
            ButtonPosition::DPadRight => "→",
            ButtonPosition::Guide => "⊞",
        }
    }

    // PlayStation labels
    fn playstation_label(&self) -> &'static str {
        match self {
            ButtonPosition::South => "Cross",
            ButtonPosition::East => "Circle",
            ButtonPosition::West => "Square",
            ButtonPosition::North => "Triangle",
            ButtonPosition::LeftBumper => "L1",
            ButtonPosition::RightBumper => "R1",
            ButtonPosition::LeftTrigger => "L2",
            ButtonPosition::RightTrigger => "R2",
            ButtonPosition::Select => "Share",
            ButtonPosition::Start => "Options",
            ButtonPosition::LeftStick => "L3",
            ButtonPosition::RightStick => "R3",
            ButtonPosition::DPadUp => "D-Pad Up",
            ButtonPosition::DPadDown => "D-Pad Down",
            ButtonPosition::DPadLeft => "D-Pad Left",
            ButtonPosition::DPadRight => "D-Pad Right",
            ButtonPosition::Guide => "PS",
        }
    }

    fn playstation_short_label(&self) -> &'static str {
        match self {
            ButtonPosition::South => "✕",
            ButtonPosition::East => "○",
            ButtonPosition::West => "□",
            ButtonPosition::North => "△",
            ButtonPosition::LeftBumper => "L1",
            ButtonPosition::RightBumper => "R1",
            ButtonPosition::LeftTrigger => "L2",
            ButtonPosition::RightTrigger => "R2",
            ButtonPosition::Select => "Share",
            ButtonPosition::Start => "Opt",
            ButtonPosition::LeftStick => "L3",
            ButtonPosition::RightStick => "R3",
            ButtonPosition::DPadUp => "↑",
            ButtonPosition::DPadDown => "↓",
            ButtonPosition::DPadLeft => "←",
            ButtonPosition::DPadRight => "→",
            ButtonPosition::Guide => "PS",
        }
    }

    // Nintendo labels (note: A/B and X/Y are in different positions than Xbox)
    fn nintendo_label(&self) -> &'static str {
        match self {
            ButtonPosition::South => "B",
            ButtonPosition::East => "A",
            ButtonPosition::West => "Y",
            ButtonPosition::North => "X",
            ButtonPosition::LeftBumper => "L",
            ButtonPosition::RightBumper => "R",
            ButtonPosition::LeftTrigger => "ZL",
            ButtonPosition::RightTrigger => "ZR",
            ButtonPosition::Select => "−",
            ButtonPosition::Start => "+",
            ButtonPosition::LeftStick => "LS",
            ButtonPosition::RightStick => "RS",
            ButtonPosition::DPadUp => "D-Pad Up",
            ButtonPosition::DPadDown => "D-Pad Down",
            ButtonPosition::DPadLeft => "D-Pad Left",
            ButtonPosition::DPadRight => "D-Pad Right",
            ButtonPosition::Guide => "Home",
        }
    }

    fn nintendo_short_label(&self) -> &'static str {
        match self {
            ButtonPosition::South => "B",
            ButtonPosition::East => "A",
            ButtonPosition::West => "Y",
            ButtonPosition::North => "X",
            ButtonPosition::LeftBumper => "L",
            ButtonPosition::RightBumper => "R",
            ButtonPosition::LeftTrigger => "ZL",
            ButtonPosition::RightTrigger => "ZR",
            ButtonPosition::Select => "−",
            ButtonPosition::Start => "+",
            ButtonPosition::LeftStick => "LS",
            ButtonPosition::RightStick => "RS",
            ButtonPosition::DPadUp => "↑",
            ButtonPosition::DPadDown => "↓",
            ButtonPosition::DPadLeft => "←",
            ButtonPosition::DPadRight => "→",
            ButtonPosition::Guide => "⌂",
        }
    }
}

/// Helper struct for getting button labels for a specific controller
#[derive(Debug, Clone, Copy)]
pub struct ButtonLabels {
    controller: ControllerType,
}

impl ButtonLabels {
    pub fn new(controller: ControllerType) -> Self {
        Self { controller }
    }

    /// Create from controller name string
    pub fn from_name(name: &str) -> Self {
        Self {
            controller: ControllerType::from_name(name),
        }
    }

    /// Get the controller type
    pub fn controller_type(&self) -> ControllerType {
        self.controller
    }

    // Face buttons
    pub fn south(&self) -> &'static str {
        ButtonPosition::South.label(self.controller)
    }
    pub fn east(&self) -> &'static str {
        ButtonPosition::East.label(self.controller)
    }
    pub fn west(&self) -> &'static str {
        ButtonPosition::West.label(self.controller)
    }
    pub fn north(&self) -> &'static str {
        ButtonPosition::North.label(self.controller)
    }

    // Shoulder buttons
    pub fn left_bumper(&self) -> &'static str {
        ButtonPosition::LeftBumper.label(self.controller)
    }
    pub fn right_bumper(&self) -> &'static str {
        ButtonPosition::RightBumper.label(self.controller)
    }
    pub fn left_trigger(&self) -> &'static str {
        ButtonPosition::LeftTrigger.label(self.controller)
    }
    pub fn right_trigger(&self) -> &'static str {
        ButtonPosition::RightTrigger.label(self.controller)
    }

    // Menu buttons
    pub fn select(&self) -> &'static str {
        ButtonPosition::Select.label(self.controller)
    }
    pub fn start(&self) -> &'static str {
        ButtonPosition::Start.label(self.controller)
    }

    // Stick clicks
    pub fn left_stick(&self) -> &'static str {
        ButtonPosition::LeftStick.label(self.controller)
    }
    pub fn right_stick(&self) -> &'static str {
        ButtonPosition::RightStick.label(self.controller)
    }

    // D-pad
    pub fn dpad_up(&self) -> &'static str {
        ButtonPosition::DPadUp.label(self.controller)
    }
    pub fn dpad_down(&self) -> &'static str {
        ButtonPosition::DPadDown.label(self.controller)
    }
    pub fn dpad_left(&self) -> &'static str {
        ButtonPosition::DPadLeft.label(self.controller)
    }
    pub fn dpad_right(&self) -> &'static str {
        ButtonPosition::DPadRight.label(self.controller)
    }

    pub fn guide(&self) -> &'static str {
        ButtonPosition::Guide.label(self.controller)
    }
}

impl Default for ButtonLabels {
    fn default() -> Self {
        Self {
            controller: ControllerType::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_detection() {
        assert_eq!(
            ControllerType::from_name("DualSense Wireless Controller"),
            ControllerType::PlayStation
        );
        assert_eq!(
            ControllerType::from_name("Xbox Wireless Controller"),
            ControllerType::Xbox
        );
        assert_eq!(
            ControllerType::from_name("Nintendo Switch Pro Controller"),
            ControllerType::Nintendo
        );
        assert_eq!(
            ControllerType::from_name("Generic USB Gamepad"),
            ControllerType::Generic
        );
    }

    #[test]
    fn test_button_labels() {
        let ps = ButtonLabels::new(ControllerType::PlayStation);
        assert_eq!(ps.south(), "Cross");
        assert_eq!(ps.east(), "Circle");

        let xbox = ButtonLabels::new(ControllerType::Xbox);
        assert_eq!(xbox.south(), "A");
        assert_eq!(xbox.east(), "B");

        let nintendo = ButtonLabels::new(ControllerType::Nintendo);
        assert_eq!(nintendo.south(), "B");
        assert_eq!(nintendo.east(), "A");
    }
}
