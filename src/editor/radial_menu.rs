//! Generic radial (pie) menu widget.
//!
//! Renders as a floating overlay centered on the mouse position.
//! Trigger with a key press, navigate by moving the mouse, confirm by clicking.
//!
//! Usage:
//! ```ignore
//! // In your panel state:
//! wheel: Option<WheelSession<MyAction>>
//!
//! // Open on keypress:
//! if triggered { self.wheel = Some(WheelSession::open(ctx, my_items)); }
//!
//! // Draw every frame while open:
//! if let Some(session) = &self.wheel {
//!     match session.show(ctx) {
//!         WheelOut::Selected(action) => { /* act */ self.wheel = None; }
//!         WheelOut::Dismissed        => { self.wheel = None; }
//!         WheelOut::Open             => {}
//!     }
//! }
//! ```

use std::f32::consts::{PI, TAU};
use std::time::Instant;
use egui::{Color32, FontId, Pos2, Stroke, vec2};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct RadialItem<T> {
    pub icon: char,
    pub label: &'static str,
    pub value: T,
    pub enabled: bool,
}

impl<T> RadialItem<T> {
    pub fn new(icon: char, label: &'static str, value: T) -> Self {
        Self { icon, label, value, enabled: true }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

pub enum WheelOut<T> {
    /// User is still navigating.
    Open,
    /// User confirmed a selection.
    Selected(T),
    /// User cancelled (Escape, click outside, click dead-zone).
    Dismissed,
}

/// A live wheel session. Create with [`WheelSession::open`], call
/// [`WheelSession::show`] every frame until it returns a non-`Open` result.
pub struct WheelSession<T: Clone + PartialEq> {
    id: egui::Id,
    items: Vec<RadialItem<T>>,
    center: Pos2,
    opened_at: Instant,
    outer_radius: f32,
    inner_radius: f32,
    // The slice index under the cursor from last frame — used for rendering.
    hovered: Option<usize>,
    // Skip input processing on the very first frame so the click/key that
    // opened the wheel doesn't immediately dismiss it.
    skip_first_frame: bool,
}

impl<T: Clone + PartialEq> WheelSession<T> {
    /// Open a new wheel centered on the current mouse position.
    pub fn open(ctx: &egui::Context, id_source: impl std::hash::Hash, items: Vec<RadialItem<T>>) -> Self {
        let center = ctx.input(|i| i.pointer.hover_pos())
            .unwrap_or(ctx.input(|i| i.screen_rect().center()));
        Self {
            id: egui::Id::new(id_source),
            items,
            center,
            opened_at: Instant::now(),
            outer_radius: 110.0,
            inner_radius: 28.0,
            hovered: None,
            skip_first_frame: true,
        }
    }

    /// Draw the wheel for this frame. Returns the result.
    pub fn show(&mut self, ctx: &egui::Context) -> WheelOut<T> {
        let n = self.items.len();
        if n == 0 { return WheelOut::Dismissed; }

        // Skip input on the opening frame so the click/key that triggered
        // the wheel doesn't immediately dismiss it.
        if self.skip_first_frame {
            self.skip_first_frame = false;
            ctx.request_repaint();
            return WheelOut::Open;
        }

        // Animation: cubic ease-out over 150 ms
        let open_t = self.opened_at.elapsed().as_secs_f32();
        let anim = ease_out_cubic((open_t / 0.15).min(1.0));
        let eff_outer = self.outer_radius * anim;
        let eff_inner = self.inner_radius * anim;

        // Hover detection
        let mouse = ctx.input(|i| i.pointer.hover_pos()).unwrap_or(self.center);
        let delta = mouse - self.center;
        let dist = delta.length();

        self.hovered = if anim > 0.3 && dist > eff_inner && dist < eff_outer + 40.0 {
            let angle = delta.y.atan2(delta.x);
            let normalized = (angle + PI / 2.0).rem_euclid(TAU);
            let idx = (normalized / (TAU / n as f32)) as usize;
            Some(idx.min(n - 1))
        } else {
            None
        };

        // Input: left click confirms, Escape or click in dead-zone dismisses
        let clicked = ctx.input(|i| i.pointer.primary_clicked());
        let escape  = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        // Also dismiss if user presses the same key again (Tab)
        let retrigger = ctx.input(|i| i.key_pressed(egui::Key::Tab));

        if escape || retrigger || (clicked && self.hovered.is_none()) {
            return WheelOut::Dismissed;
        }

        if clicked {
            if let Some(i) = self.hovered {
                if self.items[i].enabled {
                    let val = self.items[i].value.clone();
                    return WheelOut::Selected(val);
                }
            }
        }

        // Render
        self.paint(ctx, eff_inner, eff_outer, n, anim);

        if anim < 1.0 { ctx.request_repaint(); }

        WheelOut::Open
    }

    // ---- Rendering ---------------------------------------------------------

    fn paint(&self, ctx: &egui::Context, inner_r: f32, outer_r: f32, n: usize, anim: f32) {
        let area_half = self.outer_radius + 60.0;
        let area_pos  = self.center - vec2(area_half, area_half);

        egui::Area::new(self.id)
            .fixed_pos(area_pos)
            .order(egui::Order::Foreground)
            .interactable(false) // we read input directly from ctx
            .show(ctx, |ui| {
                let painter = ui.painter();
                // The center in painter-local coords
                let c = self.center;

                let alpha = (anim * 230.0) as u8;
                let slice_angle = TAU / n as f32;
                let gap = if n <= 4 { 0.04 } else { 0.025 };

                // Background scrim disc
                painter.circle_filled(
                    c,
                    outer_r + 18.0,
                    Color32::from_rgba_unmultiplied(12, 12, 18, (anim * 180.0) as u8),
                );

                // Slices
                for (i, item) in self.items.iter().enumerate() {
                    let mid_angle = -PI / 2.0 + (i as f32 + 0.5) * slice_angle;
                    let a = -PI / 2.0 + i as f32 * slice_angle + gap;
                    let b = -PI / 2.0 + (i + 1) as f32 * slice_angle - gap;

                    let is_hov = self.hovered == Some(i);
                    let is_ena = item.enabled;

                    let fill = slice_fill(is_hov, is_ena, alpha);
                    let edge = slice_edge(is_hov, is_ena, alpha);

                    // Wedge polygon
                    let pts = wedge_points(c, inner_r, outer_r, a, b, 14);
                    painter.add(egui::Shape::Path(egui::epaint::PathShape {
                        points: pts,
                        closed: true,
                        fill,
                        stroke: egui::epaint::PathStroke::new(1.0, edge),
                    }));

                    // Icon
                    let icon_r = (inner_r + outer_r) * 0.5;
                    let icon_pos = c + vec2(mid_angle.cos(), mid_angle.sin()) * icon_r;
                    painter.text(
                        icon_pos,
                        egui::Align2::CENTER_CENTER,
                        item.icon.to_string(),
                        FontId::new(18.0, egui::FontFamily::Name("lucide".into())),
                        icon_color(is_hov, is_ena, alpha),
                    );

                    // Label
                    let label_r = outer_r + 20.0;
                    let label_pos = c + vec2(mid_angle.cos(), mid_angle.sin()) * label_r;
                    painter.text(
                        label_pos,
                        egui::Align2::CENTER_CENTER,
                        item.label,
                        FontId::proportional(10.5),
                        label_color(is_hov, is_ena, alpha),
                    );
                }

                // Centre disc (dead-zone)
                painter.circle_filled(c, inner_r, Color32::from_rgba_unmultiplied(18, 18, 26, alpha));
                painter.circle_stroke(c, inner_r, Stroke::new(1.0, Color32::from_rgba_unmultiplied(65, 65, 85, alpha)));

                // Optional: small crosshair dot at exact center
                painter.circle_filled(c, 2.5, Color32::from_rgba_unmultiplied(80, 80, 100, alpha));
            });
    }
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// Build the polygon points for one pie wedge.
/// Outer arc from `a` → `b`, then inner arc from `b` → `a`.
fn wedge_points(center: Pos2, inner_r: f32, outer_r: f32, a: f32, b: f32, steps: usize) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity((steps + 1) * 2);
    for i in 0..=steps {
        let t = a + (b - a) * i as f32 / steps as f32;
        pts.push(center + vec2(t.cos(), t.sin()) * outer_r);
    }
    for i in (0..=steps).rev() {
        let t = a + (b - a) * i as f32 / steps as f32;
        pts.push(center + vec2(t.cos(), t.sin()) * inner_r);
    }
    pts
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

fn slice_fill(hov: bool, ena: bool, a: u8) -> Color32 {
    if !ena  { Color32::from_rgba_unmultiplied(28, 28, 34, a) }
    else if hov { Color32::from_rgba_unmultiplied(55, 110, 200, a) }
    else         { Color32::from_rgba_unmultiplied(32, 32, 44, (a as f32 * 0.92) as u8) }
}

fn slice_edge(hov: bool, ena: bool, a: u8) -> Color32 {
    if !ena  { Color32::from_rgba_unmultiplied(45, 45, 55, (a as f32 * 0.6) as u8) }
    else if hov { Color32::from_rgba_unmultiplied(100, 165, 255, a) }
    else         { Color32::from_rgba_unmultiplied(55, 55, 75, (a as f32 * 0.7) as u8) }
}

fn icon_color(hov: bool, ena: bool, a: u8) -> Color32 {
    if !ena  { Color32::from_rgba_unmultiplied(75, 75, 88, a) }
    else if hov { Color32::from_rgba_unmultiplied(255, 255, 255, a) }
    else         { Color32::from_rgba_unmultiplied(185, 190, 210, a) }
}

fn label_color(hov: bool, ena: bool, a: u8) -> Color32 {
    if !ena  { Color32::from_rgba_unmultiplied(60, 60, 72, a) }
    else if hov { Color32::from_rgba_unmultiplied(210, 215, 235, a) }
    else         { Color32::from_rgba_unmultiplied(120, 125, 145, a) }
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
