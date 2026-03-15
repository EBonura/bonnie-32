//! PNG import and conversion to indexed texture format
//!
//! Handles loading PNG images, resizing to target size, and quantizing to CLUT format.

use image::{imageops::FilterType, RgbaImage};
use crate::rasterizer::{ClutDepth, Color15};
use crate::modeler::{quantize_image_with_options, count_unique_colors, QuantizeMode, QuantizeOptions};
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


/// Atlas cell sizes for spritesheet import
pub const ATLAS_CELL_SIZES: &[usize] = &[32, 64, 128, 256];

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
    /// Selected quantization mode
    pub quantize_mode: QuantizeMode,
    /// Use LAB color space for perceptually uniform quantization
    pub use_lab: bool,
    /// Denoise: reduce to 4-bit per channel (0=off, 1=on)
    pub pre_quantize: u8,
    /// Perceptual weighting (0.0-1.0) - weight green channel more
    pub perceptual_weight: f32,
    /// Saturation bias (0.0-1.0) - prioritize saturated colors
    pub saturation_bias: f32,
    /// Minimum bucket fraction (0.0-0.05) - merge tiny color clusters
    pub min_bucket_fraction: f32,
    /// Number of unique colors detected in source
    pub unique_colors: usize,
    /// Whether preview needs regeneration
    pub preview_dirty: bool,
    /// Quantized preview indices (target_size x target_size)
    pub preview_indices: Vec<u8>,
    /// Quantized preview palette
    pub preview_palette: Vec<Color15>,
    // === Atlas/Spritesheet Mode ===
    /// Whether atlas mode is enabled (subdivide source into grid)
    pub atlas_mode: bool,
    /// Cell size for atlas grid (32, 64, 128, or 256)
    pub atlas_cell_size: usize,
    /// Selected cell in atlas (col, row)
    pub atlas_selected: (usize, usize),
    // === Crop Selection Mode (non-atlas) ===
    /// Crop selection rectangle (x, y, width, height) in source pixels
    /// None = use whole image
    pub crop_selection: Option<(usize, usize, usize, usize)>,
    /// Whether user is currently dragging to make a new selection
    pub crop_dragging: bool,
    /// Start point of drag in source pixels
    pub crop_drag_start: Option<(usize, usize)>,
    /// Animation frame for marching ants
    pub crop_anim_frame: u32,
    /// Edge being resized (None = not resizing, creating new selection, or moving)
    pub crop_resize_edge: Option<CropResizeEdge>,
    /// Original selection when resize started (for calculating deltas)
    pub crop_resize_original: Option<(usize, usize, usize, usize)>,
}

/// Edge/corner of crop selection being resized
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
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
            quantize_mode: QuantizeMode::default(),
            use_lab: false,
            pre_quantize: 0,
            perceptual_weight: 0.0,
            saturation_bias: 0.0,
            min_bucket_fraction: 0.0,
            unique_colors: 0,
            preview_dirty: false,
            preview_indices: Vec::new(),
            preview_palette: Vec::new(),
            // Atlas mode
            atlas_mode: false,
            atlas_cell_size: 64,
            atlas_selected: (0, 0),
            // Crop selection
            crop_selection: None,
            crop_dragging: false,
            crop_drag_start: None,
            crop_anim_frame: 0,
            crop_resize_edge: None,
            crop_resize_original: None,
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

/// Extract a cell from an atlas/spritesheet
/// Returns RGBA data for the specified cell, or None if out of bounds
pub fn extract_atlas_cell(
    rgba: &[u8],
    width: usize,
    height: usize,
    cell_size: usize,
    col: usize,
    row: usize,
) -> Option<Vec<u8>> {
    let cell_x = col * cell_size;
    let cell_y = row * cell_size;

    // Check bounds
    if cell_x + cell_size > width || cell_y + cell_size > height {
        return None;
    }

    // Extract the cell
    let mut cell_rgba = Vec::with_capacity(cell_size * cell_size * 4);
    for y in 0..cell_size {
        let src_y = cell_y + y;
        let src_start = (src_y * width + cell_x) * 4;
        let src_end = src_start + cell_size * 4;
        cell_rgba.extend_from_slice(&rgba[src_start..src_end]);
    }

    Some(cell_rgba)
}

/// Get the number of columns and rows in an atlas
pub fn atlas_dimensions(width: usize, height: usize, cell_size: usize) -> (usize, usize) {
    let cols = width / cell_size;
    let rows = height / cell_size;
    (cols, rows)
}

/// Extract a rectangular selection from an image
/// Returns RGBA data for the specified region
pub fn extract_selection(
    rgba: &[u8],
    width: usize,
    _height: usize,
    sel_x: usize,
    sel_y: usize,
    sel_w: usize,
    sel_h: usize,
) -> Vec<u8> {
    let mut result = Vec::with_capacity(sel_w * sel_h * 4);
    for y in 0..sel_h {
        let src_y = sel_y + y;
        let src_start = (src_y * width + sel_x) * 4;
        let src_end = src_start + sel_w * 4;
        result.extend_from_slice(&rgba[src_start..src_end]);
    }
    result
}

/// Generate quantized preview from source image
pub fn generate_preview(state: &mut TextureImportState) {
    if state.source_rgba.is_empty() {
        return;
    }

    let (target_w, target_h) = state.target_size.dimensions();
    let target = target_w; // Square textures, so width == height

    // In atlas mode, extract the selected cell first
    // In non-atlas mode with crop selection, extract that region
    let (source_rgba, source_w, source_h) = if state.atlas_mode {
        let (col, row) = state.atlas_selected;
        let cell_size = state.atlas_cell_size;

        if let Some(cell) = extract_atlas_cell(
            &state.source_rgba,
            state.source_width,
            state.source_height,
            cell_size,
            col,
            row,
        ) {
            (cell, cell_size, cell_size)
        } else {
            // Cell out of bounds, use whole source
            (state.source_rgba.clone(), state.source_width, state.source_height)
        }
    } else if let Some((sel_x, sel_y, sel_w, sel_h)) = state.crop_selection {
        // Non-atlas mode with crop selection
        let cropped = extract_selection(
            &state.source_rgba,
            state.source_width,
            state.source_height,
            sel_x,
            sel_y,
            sel_w,
            sel_h,
        );
        (cropped, sel_w, sel_h)
    } else {
        (state.source_rgba.clone(), state.source_width, state.source_height)
    };

    let resized = resize_to_target(
        &source_rgba,
        source_w,
        source_h,
        target,
        state.resize_mode,
    );

    // Build quantization options from state
    let opts = QuantizeOptions {
        mode: state.quantize_mode,
        use_lab: state.use_lab,
        pre_quantize: state.pre_quantize,
        perceptual_weight: state.perceptual_weight,
        saturation_bias: state.saturation_bias,
        min_bucket_fraction: state.min_bucket_fraction,
    };

    let result = quantize_image_with_options(
        &resized, target, target_h, state.depth, "preview", &opts
    );

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
