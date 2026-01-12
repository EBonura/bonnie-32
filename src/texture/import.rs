//! PNG import and conversion to indexed texture format
//!
//! Handles loading PNG images, resizing to target size, and quantizing to CLUT format.

use image::{imageops::FilterType, RgbaImage};
use crate::rasterizer::{ClutDepth, Color15};
use crate::modeler::{quantize_image, count_unique_colors};
use super::TextureSize;

/// Supported import target sizes (32x32 to 256x256)
pub const IMPORT_SIZES: &[TextureSize] = &[
    TextureSize::Size32x32,
    TextureSize::Size64x64,
    TextureSize::Size128x128,
    TextureSize::Size256x256,
];

/// How to resize non-square images to target size
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum ResizeMode {
    /// Scale to fit within target maintaining aspect ratio, pad with transparent
    #[default]
    FitPad,
    /// Stretch/squish to exactly target size regardless of aspect ratio
    Stretch,
    /// Scale to cover target, crop edges that exceed
    CropCenter,
}

impl ResizeMode {
    pub fn label(&self) -> &'static str {
        match self {
            ResizeMode::FitPad => "Fit & Pad",
            ResizeMode::Stretch => "Stretch",
            ResizeMode::CropCenter => "Crop",
        }
    }

    pub const ALL: &'static [ResizeMode] = &[
        ResizeMode::FitPad,
        ResizeMode::Stretch,
        ResizeMode::CropCenter,
    ];
}

/// State for the texture import dialog
#[derive(Debug)]
pub struct TextureImportState {
    /// Whether the import dialog is active
    pub active: bool,
    /// Original PNG as RGBA bytes
    pub source_rgba: Vec<u8>,
    /// Original image width
    pub source_width: usize,
    /// Original image height
    pub source_height: usize,
    /// Target texture size (32x32 to 256x256)
    pub target_size: TextureSize,
    /// Selected resize mode
    pub resize_mode: ResizeMode,
    /// Selected palette depth
    pub depth: ClutDepth,
    /// Number of unique colors detected in source
    pub unique_colors: usize,
    /// Whether preview needs regeneration
    pub preview_dirty: bool,
    /// Quantized preview indices (target_size x target_size)
    pub preview_indices: Vec<u8>,
    /// Quantized preview palette
    pub preview_palette: Vec<Color15>,
}

impl Default for TextureImportState {
    fn default() -> Self {
        Self {
            active: false,
            source_rgba: Vec::new(),
            source_width: 0,
            source_height: 0,
            target_size: TextureSize::Size64x64, // Default to 64x64
            resize_mode: ResizeMode::default(),
            depth: ClutDepth::default(),
            unique_colors: 0,
            preview_dirty: false,
            preview_indices: Vec::new(),
            preview_palette: Vec::new(),
        }
    }
}

impl TextureImportState {
    /// Reset the import state
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Load PNG bytes into import state
pub fn load_png_to_import_state(bytes: &[u8], state: &mut TextureImportState) -> Result<(), String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    let rgba = img.to_rgba8();
    state.source_width = rgba.width() as usize;
    state.source_height = rgba.height() as usize;
    state.source_rgba = rgba.into_raw();
    state.active = true;
    state.preview_dirty = true;

    // Auto-detect optimal depth based on unique colors
    state.unique_colors = count_unique_colors(&state.source_rgba);
    // Reserve 1 color for transparent (index 0), so 15 colors fit in 4-bit
    state.depth = if state.unique_colors <= 15 {
        ClutDepth::Bpp4
    } else {
        ClutDepth::Bpp8
    };

    Ok(())
}

/// Resize RGBA image to target size with given mode
pub fn resize_to_target(
    rgba: &[u8],
    width: usize,
    height: usize,
    target_size: usize,
    mode: ResizeMode,
) -> Vec<u8> {
    let img = RgbaImage::from_raw(width as u32, height as u32, rgba.to_vec())
        .expect("Invalid RGBA data");

    let target = target_size as u32;
    let target_f = target_size as f32;

    let resized = match mode {
        ResizeMode::FitPad => {
            // Scale to fit, center on transparent target x target
            let scale = (target_f / width as f32).min(target_f / height as f32);
            let new_w = ((width as f32 * scale).round() as u32).max(1);
            let new_h = ((height as f32 * scale).round() as u32).max(1);
            let scaled = image::imageops::resize(&img, new_w, new_h, FilterType::Lanczos3);

            let mut result = RgbaImage::from_pixel(target, target, image::Rgba([0, 0, 0, 0]));
            let offset_x = (target - new_w) / 2;
            let offset_y = (target - new_h) / 2;
            image::imageops::overlay(&mut result, &scaled, offset_x as i64, offset_y as i64);
            result
        }
        ResizeMode::Stretch => {
            image::imageops::resize(&img, target, target, FilterType::Lanczos3)
        }
        ResizeMode::CropCenter => {
            // Scale to cover, crop center
            let scale = (target_f / width as f32).max(target_f / height as f32);
            let new_w = ((width as f32 * scale).round() as u32).max(target);
            let new_h = ((height as f32 * scale).round() as u32).max(target);
            let scaled = image::imageops::resize(&img, new_w, new_h, FilterType::Lanczos3);

            let crop_x = (new_w.saturating_sub(target)) / 2;
            let crop_y = (new_h.saturating_sub(target)) / 2;
            image::imageops::crop_imm(&scaled, crop_x, crop_y, target, target).to_image()
        }
    };

    resized.into_raw()
}

/// Generate quantized preview from source image
pub fn generate_preview(state: &mut TextureImportState) {
    if state.source_rgba.is_empty() {
        return;
    }

    let (target_w, target_h) = state.target_size.dimensions();
    let target = target_w; // Square textures, so width == height

    let resized = resize_to_target(
        &state.source_rgba,
        state.source_width,
        state.source_height,
        target,
        state.resize_mode,
    );

    // Use existing quantize_image from modeler/quantize.rs
    let result = quantize_image(&resized, target, target_h, state.depth, "preview");

    state.preview_indices = result.texture.indices;
    state.preview_palette = result.clut.colors;
    state.preview_dirty = false;
}

/// Render preview indices to RGBA for display
pub fn preview_to_rgba(state: &TextureImportState) -> Vec<u8> {
    let (w, h) = state.target_size.dimensions();
    let mut rgba = vec![0u8; w * h * 4];

    for (i, &index) in state.preview_indices.iter().enumerate() {
        let color = state.preview_palette.get(index as usize)
            .copied()
            .unwrap_or(Color15::TRANSPARENT);

        let pixel_offset = i * 4;
        if color.is_transparent() {
            // Transparent - leave as 0,0,0,0
        } else {
            let [r, g, b, a] = color.to_rgba();
            rgba[pixel_offset] = r;
            rgba[pixel_offset + 1] = g;
            rgba[pixel_offset + 2] = b;
            rgba[pixel_offset + 3] = a;
        }
    }

    rgba
}
