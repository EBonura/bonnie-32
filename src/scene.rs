//! Unified scene rendering
//!
//! Shared functions for rendering level geometry and placed asset meshes.
//! Used by the world editor, level browser, game renderer, and camera preview.

#![allow(dead_code)]

use crate::rasterizer::{
    Framebuffer, Camera, RasterSettings, Vertex,
    Texture as RasterTexture, Texture15, Light, Color as RasterColor,
    render_mesh, render_mesh_15, Clut, ClutId, Vec3,
};
use crate::world::Room;
use crate::asset::{AssetLibrary, AssetComponent};
use crate::modeler::{MeshPart, IndexedAtlas, TextureRef as MeshTextureRef, checkerboard_clut};
use crate::texture::TextureLibrary;

/// Options controlling what gets rendered in a scene
pub struct SceneRenderOptions<'a> {
    /// Whether to build and apply per-room fog
    pub use_fog: bool,
    /// Whether to render asset meshes placed in rooms
    pub render_assets: bool,
    /// Room indices to skip (e.g., hidden rooms in the editor)
    pub skip_rooms: &'a [usize],
}

/// Collect all lights from asset instances placed in rooms.
///
/// Extracts Light component data (color, intensity, radius, offset) with
/// per-instance overrides applied. Returns an empty Vec if no lights found.
pub fn collect_scene_lights(
    rooms: &[Room],
    asset_library: &AssetLibrary,
) -> Vec<Light> {
    rooms.iter()
        .flat_map(|room| {
            room.objects.iter()
                .filter_map(|obj| {
                    if !obj.enabled {
                        return None;
                    }
                    let asset = asset_library.get_by_id(obj.asset_id)?;
                    for comp in &asset.components {
                        if let AssetComponent::Light { color, intensity, radius, offset } = comp {
                            // Apply per-instance overrides if present
                            let overrides = &obj.overrides.light;
                            let final_color = overrides.as_ref().and_then(|o| o.color).unwrap_or(*color);
                            let final_intensity = overrides.as_ref().and_then(|o| o.intensity).unwrap_or(*intensity);
                            let final_radius = overrides.as_ref().and_then(|o| o.radius).unwrap_or(*radius);
                            let final_offset = overrides.as_ref().and_then(|o| o.offset).unwrap_or(*offset);

                            let base_pos = obj.world_position(room);
                            let light_pos = Vec3::new(
                                base_pos.x + final_offset[0],
                                base_pos.y + final_offset[1],
                                base_pos.z + final_offset[2],
                            );
                            let r = final_color[0] as f32 / 255.0;
                            let g = final_color[1] as f32 / 255.0;
                            let b = final_color[2] as f32 / 255.0;
                            return Some(Light::point_colored(light_pos, final_radius, final_intensity, r, g, b));
                        }
                    }
                    None
                })
        })
        .collect()
}

/// Resolve atlas and CLUT for a mesh part based on its TextureRef.
///
/// For TextureRef::Id, looks up the actual UserTexture data.
/// For all other variants, uses the part's atlas with a checkerboard CLUT.
pub fn resolve_part_texture(
    part: &MeshPart,
    user_textures: &TextureLibrary,
) -> (IndexedAtlas, Clut) {
    match &part.texture_ref {
        MeshTextureRef::Id(id) => {
            if let Some(tex) = user_textures.get_by_id(*id) {
                let atlas = IndexedAtlas {
                    width: tex.width,
                    height: tex.height,
                    depth: tex.depth,
                    indices: tex.indices.clone(),
                    default_clut: ClutId::NONE,
                };
                let mut clut = Clut::new_4bit("scene_texture");
                clut.colors = tex.palette.clone();
                clut.depth = tex.depth;
                (atlas, clut)
            } else {
                (part.atlas.clone(), checkerboard_clut().clone())
            }
        }
        MeshTextureRef::Embedded(_) => {
            (part.atlas.clone(), checkerboard_clut().clone())
        }
        MeshTextureRef::Checkerboard | MeshTextureRef::None => {
            (part.atlas.clone(), checkerboard_clut().clone())
        }
    }
}

/// Render an asset's mesh parts with per-part double_sided handling and texture resolution.
///
/// Each part is rendered in a separate render_mesh call with its own backface
/// settings and resolved texture. Handles facing rotation and world position offset.
///
/// Used by `render_scene` for placed assets and by the asset browser for previews.
pub fn render_asset_parts(
    fb: &mut Framebuffer,
    parts: &[MeshPart],
    camera: &Camera,
    base_settings: &RasterSettings,
    facing: f32,
    world_pos: Vec3,
    fog: Option<(f32, f32, f32, RasterColor)>,
    user_textures: &TextureLibrary,
) {
    let use_rgb555 = base_settings.use_rgb555;
    let cos_f = facing.cos();
    let sin_f = facing.sin();
    let has_transform = facing.abs() > 0.0001 || world_pos.x.abs() > 0.0001 || world_pos.y.abs() > 0.0001 || world_pos.z.abs() > 0.0001;

    for part in parts.iter().filter(|p| p.visible) {
        let (local_vertices, faces) = part.mesh.to_render_data_textured();
        if local_vertices.is_empty() {
            continue;
        }

        // Per-part backface settings: disable culling for double-sided parts
        let render_settings = RasterSettings {
            backface_cull: !part.double_sided && base_settings.backface_cull,
            backface_wireframe: !part.double_sided && base_settings.backface_wireframe,
            ..base_settings.clone()
        };

        // Transform vertices: rotate around Y by facing, then translate
        let vertices: Vec<Vertex> = if has_transform {
            local_vertices.iter().map(|v| {
                let rx = v.pos.x * cos_f - v.pos.z * sin_f;
                let rz = v.pos.x * sin_f + v.pos.z * cos_f;
                Vertex {
                    pos: Vec3::new(rx + world_pos.x, v.pos.y + world_pos.y, rz + world_pos.z),
                    uv: v.uv,
                    normal: Vec3::new(
                        v.normal.x * cos_f - v.normal.z * sin_f,
                        v.normal.y,
                        v.normal.x * sin_f + v.normal.z * cos_f,
                    ),
                    color: v.color,
                    bone_index: v.bone_index,
                }
            }).collect()
        } else {
            local_vertices
        };

        let (atlas, clut) = resolve_part_texture(part, user_textures);

        if use_rgb555 {
            let tex15 = atlas.to_texture15(&clut, "asset_part");
            render_mesh_15(fb, &vertices, &faces, &[tex15], camera, &render_settings, fog);
        } else {
            let tex = atlas.to_raster_texture(&clut, "asset_part");
            render_mesh(fb, &vertices, &faces, &[tex], camera, &render_settings);
        }
    }
}

/// Render a complete scene: room geometry + placed asset meshes.
///
/// This is the single rendering path shared by the world editor, level browser,
/// game renderer, and camera preview. All consumers get identical behavior:
/// - Per-room ambient and fog
/// - Per-part double_sided backface handling for asset meshes
/// - Full texture resolution for asset mesh parts
pub fn render_scene(
    fb: &mut Framebuffer,
    rooms: &[Room],
    asset_library: &AssetLibrary,
    user_textures: &TextureLibrary,
    camera: &Camera,
    base_settings: &RasterSettings,
    lights: &[Light],
    textures: &[RasterTexture],
    textures_15: &[Texture15],
    resolve_texture: &dyn Fn(&crate::world::TextureRef) -> Option<(usize, u32)>,
    options: &SceneRenderOptions,
) {
    let use_rgb555 = base_settings.use_rgb555;

    // === Room geometry ===
    for (room_idx, room) in rooms.iter().enumerate() {
        if options.skip_rooms.contains(&room_idx) {
            continue;
        }

        let render_settings = RasterSettings {
            lights: lights.to_vec(),
            ambient: room.ambient,
            ..base_settings.clone()
        };

        let (vertices, faces) = room.to_render_data_with_textures(resolve_texture);
        if vertices.is_empty() {
            continue;
        }

        let fog = if options.use_fog { build_room_fog(room) } else { None };

        if use_rgb555 {
            render_mesh_15(fb, &vertices, &faces, textures_15, camera, &render_settings, fog);
        } else {
            render_mesh(fb, &vertices, &faces, textures, camera, &render_settings);
        }
    }

    // === Asset meshes placed in rooms ===
    if !options.render_assets {
        return;
    }

    for (room_idx, room) in rooms.iter().enumerate() {
        if options.skip_rooms.contains(&room_idx) {
            continue;
        }

        let fog = if options.use_fog { build_room_fog(room) } else { None };

        for obj in &room.objects {
            if !obj.enabled {
                continue;
            }

            let asset = match asset_library.get_by_id(obj.asset_id) {
                Some(a) => a,
                None => continue,
            };

            let mesh_parts = match asset.mesh() {
                Some(parts) => parts,
                None => continue,
            };

            let world_pos = obj.world_position(room);
            let room_settings = RasterSettings {
                lights: lights.to_vec(),
                ambient: room.ambient,
                ..base_settings.clone()
            };

            render_asset_parts(
                fb, mesh_parts, camera, &room_settings,
                obj.facing, world_pos, fog, user_textures,
            );
        }
    }
}

/// Build fog parameters from a room's fog settings.
fn build_room_fog(room: &Room) -> Option<(f32, f32, f32, RasterColor)> {
    if !room.fog.enabled {
        return None;
    }
    let (r, g, b) = room.fog.color;
    let fog_color = RasterColor::new(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
    );
    let cull_distance = room.fog.start + room.fog.falloff + room.fog.cull_offset;
    Some((room.fog.start, room.fog.falloff, cull_distance, fog_color))
}
