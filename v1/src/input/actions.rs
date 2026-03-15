//! Game action definitions
//!
//! Based on Elden Ring controller layout for familiar Souls-like controls.

/// All possible game/editor actions that can be triggered by input
///
/// Button mappings (Xbox/PlayStation):
/// - A/X = Jump
/// - B/O = Dodge/Sprint
/// - X/Square = Use Item
/// - Y/Triangle = Interact
/// - LB/L1 = Guard
/// - LT/L2 = Skill
/// - RB/R1 = Attack
/// - RT/R2 = Strong Attack
/// - L3 = Crouch
/// - R3 = Lock-On
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // Movement (analog - left stick / WASD)
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,

    // Camera (analog - right stick / mouse)
    LookUp,
    LookDown,
    LookLeft,
    LookRight,

    // Combat - right shoulder buttons
    Attack,         // RB - light attack
    StrongAttack,   // RT - heavy/charged attack
    Skill,          // LT - weapon skill/ash of war

    // Defense - left shoulder
    Guard,          // LB - block

    // Face buttons
    Jump,           // A
    Dodge,          // B - also backstep, dash (hold)
    UseItem,        // X
    Interact,       // Y - event action (examine, open, talk)

    // Stick clicks
    Crouch,         // L3 (left stick click)
    LockOn,         // R3 (right stick click)

    // D-pad (weapon/item switching)
    SwitchLeftWeapon,
    SwitchRightWeapon,
    SwitchSpell,
    SwitchItem,

    // System
    OpenMenu,       // Start - opens options/pause menu
    OpenMap,        // Select/Back

    // Free-fly mode (editor + game option)
    FlyUp,          // LB in free-fly / Q on keyboard
    FlyDown,        // LT in free-fly / E on keyboard
}
